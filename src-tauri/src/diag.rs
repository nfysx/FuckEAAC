//! 环境诊断：EAAC / BF6 关心的系统项，以及"已知冲突软件"的命中情况。
//!
//! 全部是一次 PowerShell 查询取回，且**只读**：
//! Secure Boot / TPM / 虚拟机 / Hyper-V / 测试模式 / 内核隔离(VBS)。

use crate::config::Config;
use crate::detect;
use crate::util;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CheckItem {
    pub name: String,
    pub value: String,
    /// ok | warn | bad | info
    pub state: String,
    pub note: String,
}

/// 一次拿回所有系统项
fn system_items() -> serde_json::Value {
    let script = r#"
$r = [ordered]@{}
# 1) Secure Boot（BF6 硬性要求）
try { $r.SecureBoot = [int](Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\SecureBoot\State' -Name UEFISecureBootEnabled -ErrorAction Stop).UEFISecureBootEnabled } catch { $r.SecureBoot = -1 }
# 2) TPM
try { $t = Get-CimInstance -Namespace 'root\cimv2\security\microsofttpm' -ClassName Win32_Tpm -ErrorAction Stop; $r.Tpm = if ($t.IsEnabled_InitialValue -and $t.IsActivated_InitialValue) { 1 } else { 0 } } catch { $r.Tpm = -1 }
# 3) 是不是虚拟机
try { $cs = Get-CimInstance Win32_ComputerSystem -ErrorAction Stop; $r.Vm = if ("$($cs.Manufacturer) $($cs.Model)" -match 'VMware|VirtualBox|Virtual Machine|QEMU|Hyper-V|Parallels') { 1 } else { 0 } } catch { $r.Vm = -1 }
# 4) Hyper-V 平台是否在跑（vmcompute 服务）
try { $s = Get-Service vmcompute -ErrorAction Stop; $r.HyperV = [string]$s.Status } catch { $r.HyperV = 'NotFound' }
# 5) 测试模式（bcdedit）
try { $b = & bcdedit /enum '{current}' 2>$null | Out-String; $r.TestSigning = if ($b -match 'testsigning\s+Yes') { 1 } elseif ($b -match 'testsigning\s+No') { 0 } else { -1 } } catch { $r.TestSigning = -1 }
# 6) 内核隔离 / VBS（部分反作弊在意）
try { $d = Get-CimInstance -ClassName Win32_DeviceGuard -Namespace root\Microsoft\Windows\DeviceGuard -ErrorAction Stop; $r.Vbs = [int]$d.VirtualizationBasedSecurityStatus } catch { $r.Vbs = -1 }
# 7) 内存与 CPU（展示用）
try { $r.Cpu = (Get-CimInstance Win32_Processor -ErrorAction Stop | Select-Object -First 1 -ExpandProperty Name); $r.RamGb = [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory/1GB,1) } catch { }
# 8) 系统版本
$r.Os = (Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue).Caption
$r.OsBuild = [string][System.Environment]::OSVersion.Version
[pscustomobject]$r | ConvertTo-Json -Compress
"#;
    util::run_ps_json(script).unwrap_or(serde_json::json!({}))
}

/// 从 JSON 里取字符串（数字/布尔也顺手转成字符串）
fn s(v: &serde_json::Value, k: &str) -> String {
    match v.get(k) {
        Some(serde_json::Value::String(x)) => x.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}
fn n(v: &serde_json::Value, k: &str) -> i64 {
    v.get(k).and_then(|x| x.as_i64()).unwrap_or(-999)
}

/// 跑一遍检查，返回给界面的列表
pub fn run_checks(cfg: &Config) -> Vec<CheckItem> {
    let mut out = Vec::new();
    let v = system_items();

    // ---- 系统要求 ----
    match n(&v, "SecureBoot") {
        1 => out.push(item("Secure Boot", "已开启", "ok", "BF6 硬性要求，已满足")),
        0 => out.push(item(
            "Secure Boot",
            "未开启",
            "bad",
            "BF6 会报 Security Violation；请在 BIOS/UEFI 里打开",
        )),
        _ => out.push(item(
            "Secure Boot",
            "读取失败",
            "info",
            "可能不是 UEFI 启动，或没有权限",
        )),
    }
    match n(&v, "Tpm") {
        1 => out.push(item("TPM", "已启用", "ok", "")),
        0 => out.push(item("TPM", "未启用", "warn", "部分反作弊会要求 TPM 2.0")),
        _ => out.push(item(
            "TPM",
            "未知",
            "info",
            "读取失败（系统较老或权限不足）",
        )),
    }
    match n(&v, "Vm") {
        1 => out.push(item("虚拟机", "是", "bad", "反作弊通常拒绝在虚拟机里运行")),
        0 => out.push(item("虚拟机", "否", "ok", "")),
        _ => out.push(item("虚拟机", "未知", "info", "")),
    }
    let hv = s(&v, "HyperV");
    if hv.eq_ignore_ascii_case("Running") {
        out.push(item("Hyper-V 平台", "运行中", "warn", "部分反作弊会拒绝"));
    } else {
        out.push(item(
            "Hyper-V 平台",
            if hv.is_empty() { "未运行" } else { &hv },
            "ok",
            "",
        ));
    }
    match n(&v, "TestSigning") {
        1 => out.push(item(
            "测试模式",
            "已开启",
            "bad",
            "testsigning 会让反作弊拒绝；请 bcdedit /set testsigning off 并重启",
        )),
        0 => out.push(item("测试模式", "未开启", "ok", "")),
        _ => out.push(item(
            "测试模式",
            "未知",
            "info",
            "需要在管理员权限下读取 bcdedit",
        )),
    }
    match n(&v, "Vbs") {
        1 | 2 => out.push(item("内核隔离 (VBS)", "已开启", "info", "一般不影响")),
        _ => out.push(item("内核隔离 (VBS)", "未开启/未知", "info", "")),
    }

    // ---- 展示用 ----
    let cpu = s(&v, "Cpu");
    if !cpu.is_empty() {
        out.push(item("CPU", &cpu, "info", ""));
    }
    let ram = s(&v, "RamGb");
    if !ram.is_empty() && ram != "-999" {
        out.push(item("内存", &format!("{ram} GB"), "info", ""));
    }
    let os = s(&v, "Os");
    let build = s(&v, "OsBuild");
    if !os.is_empty() || !build.is_empty() {
        out.push(item("系统", &format!("{os} ({build})"), "info", ""));
    }

    // ---- 已知冲突软件（EAAC/BF6 报过错的）----
    let snap = detect::snapshot(cfg, util::is_admin());
    let conflicts = [
        ("AutoHotkey 宏", "autohotkey|^ahk", ""),
        (
            "Daemon Tools 虚拟光驱",
            "daemon|dtlite|discsoft",
            "sptd|dtsoft|dtscsi",
        ),
        ("DS4Windows / 手柄映射", "ds4windows|xpadder|rewasd", ""),
        ("ViGEmBus 虚拟手柄驱动", "", "vigem"),
        ("Interception 输入驱动", "", "interception"),
        (
            "调试器 (x64dbg/IDA/WinDbg)",
            "x64dbg|x32dbg|ida64|windbg",
            "",
        ),
        ("ReShade 画质注入", "reshade", ""),
        ("MSI Afterburner / RTSS", "afterburner|rtss", ""),
        ("Voicemod 变声器", "voicemod", ""),
    ];
    for (name, ppat, spat) in conflicts {
        let mut hits: Vec<String> = Vec::new();
        for t in &snap.targets {
            if t.name != name {
                continue;
            }
            hits.extend(t.processes.iter().cloned());
            hits.extend(
                t.services
                    .iter()
                    .filter(|s| s.status.eq_ignore_ascii_case("RUNNING"))
                    .map(|s| s.name.clone()),
            );
        }
        // 配置里没有这个目标时，退化成"未发现"（不主动扫描，避免误报）
        let _ = (ppat, spat);
        if hits.is_empty() {
            out.push(item(name, "未发现", "ok", ""));
        } else {
            out.push(item(
                name,
                &format!("命中: {}", hits.join(", ")),
                "warn",
                "EAAC 可能因此报错",
            ));
        }
    }

    // ---- 系统代理与虚拟网卡 ----
    if snap.proxy_enabled {
        out.push(item(
            "系统代理",
            &format!("已开启 {}", snap.proxy_server),
            "warn",
            "系统代理一般不影响反作弊，仅TUN 模式需要停",
        ));
    } else {
        out.push(item("系统代理", "未开启", "ok", ""));
    }
    if snap.tun_adapters.is_empty() {
        out.push(item("虚拟网卡", "未发现", "ok", ""));
    } else {
        for a in &snap.tun_adapters {
            out.push(item(
                "虚拟网卡",
                &format!("{} [{}] {}", a.name, a.status, a.description),
                "warn",
                "TUN/TAP 网卡可能触发反作弊",
            ));
        }
    }

    out
}

fn item(name: &str, value: &str, state: &str, note: &str) -> CheckItem {
    CheckItem {
        name: name.to_string(),
        value: value.to_string(),
        state: state.to_string(),
        note: note.to_string(),
    }
}
