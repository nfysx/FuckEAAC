//! 动作层：真正去停服务/驱动/进程，以及恢复。
//!
//! ★ 所有动作都会先打印"我将要执行什么"（日志行会返回给界面），
//!   演习模式（dry_run）只打印不执行 —— 和 PowerShell 版的 `-DryRun` 行为一致。
//!
//! ★ 本文件只调用这些系统命令，你审查时一眼可见：
//!     sc.exe        query / stop / start / config <名> start= <类型>
//!     taskkill.exe  /IM <名>.exe /F
//!     explorer.exe  <客户端路径>  ← 恢复时以**普通用户身份**代开代理客户端
//!
//!   除此之外只读写 `state.json` / `FuckEAAC.config.json`：不碰注册表、不注入进程、不联网。

use crate::config::Config;
use crate::detect::{self, Snapshot};
use crate::state::{self, State, StoppedItem};
use crate::util;

/// 一次停用计划
pub struct Plan {
    pub services: Vec<PlannedService>,
    pub processes: Vec<PlannedProcess>,
}

pub struct PlannedService {
    pub name: String,
    pub target: String,
    pub was_running: bool,
    pub on_demand: bool, // 是否顺手改成按需启动
}

pub struct PlannedProcess {
    pub name: String, // 不带 .exe
    pub target: String,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.services.is_empty() && self.processes.is_empty()
    }
}

/// 根据快照和配置，算出"该停什么"。
///
/// * 只处理 `effective == "Stop"` 的目标（Auto 已经在 detect 里根据 TUN 网卡折算过）
/// * `drivers_on_demand` = 用户勾了「顺带把命中的内核驱动改成按需加载」
pub fn build_plan(cfg: &Config, snap: &Snapshot, drivers_on_demand: bool) -> Plan {
    let mut svc = Vec::new();
    let mut pro = Vec::new();

    for t in &cfg.targets {
        // 快照里已经过滤过（只保留命中的目标），所以按名字找回它当前的状态
        let Some(ts) = snap.targets.iter().find(|s| s.name == t.name) else {
            continue;
        };
        // 只有"实际会停"的目标才进计划（Auto 已在 detect 里按 TUN 网卡折算成 Stop/Warn）
        if ts.effective != "Stop" {
            continue;
        }
        for h in &ts.services {
            let running = h.status.eq_ignore_ascii_case("RUNNING");
            svc.push(PlannedService {
                name: h.name.clone(),
                target: t.name.clone(),
                was_running: running,
                on_demand: drivers_on_demand && t.drivers_on_demand,
            });
        }
        for p in &ts.processes {
            pro.push(PlannedProcess {
                name: p.clone(),
                target: t.name.clone(),
            });
        }
    }
    Plan {
        services: svc,
        processes: pro,
    }
}

/// 执行停用。返回日志行（会显示在界面日志区）。
///
/// 处理三类东西：
/// 1. 代理客户端进程（`taskkill /IM x.exe /F`）
/// 2. 服务/内核驱动（`sc stop`，可选改按需启动）
/// 3. **Windows 系统代理**（写 HKCU 注册表，原值记进 state.json，恢复时写回）
///
/// ★ 以前计划为空就直接 return，导致"只有 Clash（Auto）+ 没检测到 TUN"这种情况下
///   连系统代理都不碰 —— 这就是"点了停代理但系统代理还开着"的原因，已修。
pub fn stop(cfg: &Config, snap: &Snapshot, plan: &Plan, dry_run: bool) -> Vec<String> {
    let mut log = Vec::new();

    log.push(format!(
        "停用计划：{} 个服务/驱动、{} 个进程{}",
        plan.services.len(),
        plan.processes.len(),
        if dry_run {
            "（演习模式：只打印不执行）"
        } else {
            ""
        }
    ));
    if plan.is_empty() {
        log.push("  （计划里没有服务/进程 —— 这通常是因为命中的目标都是「仅提醒」，或者 TUN/系统代理未启动）".into());
    }

    let mut items: Vec<StoppedItem> = Vec::new();

    // ---- 1) 先停进程（客户端），再停服务/驱动 ----
    for p in &plan.processes {
        let img = format!("{}.exe", p.name);
        log.push(format!("结束进程 {img}（{0}）", p.target));
        if !dry_run {
            let (code, out, err) = util::run("taskkill.exe", &["/IM", &img, "/F"]);
            log.push(format!("    → {}", brief(code, &out, &err)));
            if code != 0 {
                log.push(format!(
                    "    ✗ taskkill 返回 {code}: {}",
                    first_line(&out, &err)
                ));
            } else {
                items.push(StoppedItem {
                    kind: "process".into(),
                    name: p.name.clone(),
                    was_running: true,
                    start_type: String::new(),
                    target: p.target.clone(),
                });
            }
        }
    }

    // ---- 2) 停服务/驱动 ----
    for s in &plan.services {
        // 记录原始启动类型，以便恢复
        let start_type = query_start_type(&s.name);
        if !s.was_running {
            // 本来就没跑：不要白跑一条 sc stop（会返回错误刷屏），也不用记进 state.json
            log.push(format!(
                "服务/驱动 {} 本来就没在运行（启动类型 {}），跳过",
                s.name, start_type
            ));
            continue;
        }
        log.push(format!(
            "停止服务/驱动 {}（{}，当前启动类型 {}）",
            s.name, s.target, start_type
        ));
        if !dry_run {
            let (code, out, err) = util::run("sc.exe", &["stop", &s.name]);
            log.push(format!("    → {}", brief(code, &out, &err)));
            if code != 0 {
                log.push(format!(
                    "    ✗ sc stop 返回 {code}: {}",
                    first_line(&out, &err)
                ));
            }
        }
        if s.on_demand {
            log.push(format!(
                "更改为按需启动（sc config {} start= demand）",
                s.name
            ));
            if !dry_run {
                let (code, out, err) =
                    util::run("sc.exe", &["config", &s.name, "start=", "demand"]);
                log.push(format!("    → {}", brief(code, &out, &err)));
                if code != 0 {
                    log.push(format!(
                        "    ✗ sc config 返回 {code}: {}",
                        first_line(&out, &err)
                    ));
                }
            }
        }
        items.push(StoppedItem {
            kind: "service".into(),
            name: s.name.clone(),
            was_running: s.was_running,
            start_type,
            target: s.target.clone(),
        });
    }

    // ---- 3) 关掉 Windows 系统代理（这才是"停代理"的最后一步）----
    let proxy_backup = handle_system_proxy(cfg, snap, dry_run, &mut log);

    // ---- 3.5) 复查：有没有"杀了又活过来"的进程 ----
    // 典型场景：反作弊/代理客户端有守护服务，杀掉客户端进程后它立刻又拉起来
    // （腾讯 ACE 的 AntiCheatExpert Service 就会这么干）。所以停完要再看一眼，别假报成功。
    if !dry_run && !plan.processes.is_empty() {
        let mut alive: Vec<String> = Vec::new();
        for p in &plan.processes {
            if util::count_process(&p.name).unwrap_or(0) > 0 {
                alive.push(p.name.clone());
            }
        }
        if alive.is_empty() {
            log.push("复查：计划里的进程都已结束 ✓".into());
        } else {
            log.push(format!(
                "复查：这些进程又被拉起来了 → {}（通常是它的守护服务干的；日志里如果前面有 \
                 sc stop 失败的 ✗ 行就是根因，需重启电脑）",
                alive.join("、")
            ));
        }
    }

    // ---- 4) 写状态文件（恢复时要用）----
    if !dry_run && (!items.is_empty() || proxy_backup.is_some()) {
        let st = State {
            when: util::now_string(),
            items,
            proxy: proxy_backup,
        };
        match state::save(&st) {
            Ok(()) => log.push(format!(
                "已记录本次停用清单：{} 项{}",
                st.items.len(),
                if st.proxy.is_some() {
                    " + 系统代理原值"
                } else {
                    ""
                }
            )),
            Err(e) => log.push(format!("✗ 写状态文件失败：{e}")),
        }
        state::log("STEP", "进入游戏模式：已停用相关服务/进程");
    } else if dry_run {
        log.push("（测试模式：仅记录，不改动任何东西）".into());
    } else {
        log.push("本次没有实际改动任何东西（未命中要停的服务/进程，系统代理未启动）。".into());
    }

    // ---- 5) 补一句提醒：只提醒类的目标里，哪些正在运行 ----
    let warn_only: Vec<String> = snap
        .targets
        .iter()
        .filter(|s| s.effective == "Warn" && s.running)
        .map(|s| s.name.clone())
        .collect();
    if !warn_only.is_empty() {
        log.push(format!(
            "以下软件检测到在运行，但按配置仅提醒、不会停它：{}",
            warn_only.join("、")
        ));
    }

    // ---- 6) 「需你手动处理」的目标（Action=Hint）----
    // 这类东西工具**故意不动手**（内核反作弊：卸不掉、乱停还可能让你在那边被平台封号），
    // 但既然它正在运行、又可能让 EAAC 拒绝，就必须把话说到位。
    let manual: Vec<&detect::TargetState> = snap
        .targets
        .iter()
        .filter(|s| s.effective == "Hint" && s.running)
        .collect();
    if !manual.is_empty() {
        log.push(String::new());
        log.push("⚠ 以下组件正在运行，且**本工具按配置不会动它们**（需你手动处理）：".into());
        for s in &manual {
            let mut bits: Vec<String> = Vec::new();
            if !s.processes.is_empty() {
                bits.push(format!("进程 {}", s.processes.join("、")));
            }
            let running_svc: Vec<String> = s
                .services
                .iter()
                .filter(|h| h.status.eq_ignore_ascii_case("RUNNING"))
                .map(|h| h.name.clone())
                .collect();
            if !running_svc.is_empty() {
                bits.push(format!("驱动/服务 {}", running_svc.join("、")));
            }
            log.push(format!(
                "  - {}：{}",
                s.name,
                if bits.is_empty() {
                    "（检测到在运行）".to_string()
                } else {
                    bits.join("；")
                }
            ));
        }
        log.push(
            "  → 请先退出对应的游戏/平台客户端；内核驱动通常无法卸载，**需重启电脑**。".into(),
        );
        log.push("  （想自动停，可以把配置里那条的 Action 改成 Stop）".into());
    }

    log
}

/// 把一条命令的结果压成一行给界面看（系统原文原样保留，不翻译）
fn brief(code: i32, out: &str, err: &str) -> String {
    let s = first_line(out, err);
    if s.is_empty() {
        format!("exit={code}（无输出）")
    } else {
        format!("exit={code}  |  {s}")
    }
}

/// 生成"写系统代理设置"的 PowerShell 脚本。
///
/// 只碰 `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings` 下的三个值，
/// 写完调 `InternetSetOption` 通知 WinINet —— 否则已经打开的浏览器/程序还会用旧设置。
fn proxy_reg_script(enable: i64, server: &str, pac: &str) -> String {
    let esc = |s: &str| s.replace('\'', "''");
    format!(
        "$k='HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings'; \
         Set-ItemProperty -LiteralPath $k -Name ProxyEnable -Value {enable} -Type DWord -ErrorAction Stop; \
         Set-ItemProperty -LiteralPath $k -Name ProxyServer -Value '{srv}' -Type String -ErrorAction SilentlyContinue; \
         Set-ItemProperty -LiteralPath $k -Name AutoConfigURL -Value '{pac}' -Type String -ErrorAction SilentlyContinue; \
         $sig='[DllImport(\"wininet.dll\", SetLastError=true)] public static extern bool InternetSetOption(IntPtr h, int opt, IntPtr buf, int len);'; \
         try {{ $t=Add-Type -MemberDefinition $sig -Name WinINetNotify -Namespace FuckEAAC -PassThru -ErrorAction Stop; \
           [void]$t::InternetSetOption([IntPtr]::Zero,39,[IntPtr]::Zero,0); \
           [void]$t::InternetSetOption([IntPtr]::Zero,37,[IntPtr]::Zero,0) }} catch {{ }}; \
         $p=Get-ItemProperty -LiteralPath $k; \
         'ProxyEnable=' + $p.ProxyEnable + ' ProxyServer=' + $p.ProxyServer + ' AutoConfigURL=' + $p.AutoConfigURL",
        enable = enable,
        srv = esc(server),
        pac = esc(pac)
    )
}

/// 关掉 Windows 系统代理，并返回**原值**（供恢复时写回）。
///
/// 返回 `None` = 没动它（配置里关掉了 / 本来就没开 / 写入失败）。
fn handle_system_proxy(
    cfg: &Config,
    snap: &Snapshot,
    dry_run: bool,
    log: &mut Vec<String>,
) -> Option<state::ProxyBackup> {
    if !cfg.disable_system_proxy {
        log.push("系统代理：配置里 DisableSystemProxy=false，不接管。".into());
        return None;
    }
    let proxy_on = snap.proxy_enabled || !snap.proxy_auto_config.trim().is_empty();
    if !proxy_on {
        log.push("系统代理：未启用（ProxyEnable=0 且无 PAC），无需处理。".into());
        return None;
    }

    let backup = state::ProxyBackup {
        enable: if snap.proxy_enabled { 1 } else { 0 },
        server: snap.proxy_server.clone(),
        auto_config_url: snap.proxy_auto_config.clone(),
    };
    log.push(format!(
        "系统代理：当前启用（ProxyEnable={} / ProxyServer='{}' / AutoConfigURL='{}'）→ 关闭进程，并记下原值以便恢复",
        backup.enable, backup.server, backup.auto_config_url
    ));

    if dry_run {
        log.push("    （测试模式：不会真的改注册表）".into());
        return Some(backup);
    }

    // 关掉开关；PAC 也清掉（否则即使 ProxyEnable=0，PAC 仍然在生效）
    let (code, out, err) = util::run_ps(&proxy_reg_script(0, &backup.server, ""));
    log.push(format!("    → {}", brief(code, &out, &err)));
    if code != 0 {
        log.push(
            "    ✗ 关闭系统代理失败，请手动到「设置 → 网络和 Internet → 代理」里关掉；\
             此次不会记录原值，避免恢复时乱写"
                .into(),
        );
        return None;
    }
    log.push("    ✓ 系统代理已关闭".into());
    Some(backup)
}

/// 恢复：读 state.json，把停掉的服务/驱动恢复回去，
/// 并按配置里的 `RelaunchPaths` 把被结束掉的代理客户端**重新拉起来**。
pub fn restore(cfg: &Config, dry_run: bool) -> Vec<String> {
    let mut log = Vec::new();
    let Some(st) = state::read() else {
        log.push("没有需要恢复的记录。".into());
        return log;
    };

    log.push(format!(
        "开始恢复（{} 记录的 {} 项{}）{}",
        st.when,
        st.items.len(),
        if st.proxy.is_some() {
            " + 系统代理原值"
        } else {
            ""
        },
        if dry_run {
            "（测试模式：仅打印不执行）"
        } else {
            ""
        }
    ));

    // 这次被停过的目标名（去重）→ 决定要重开哪些客户端
    let mut touched: Vec<String> = Vec::new();

    for it in &st.items {
        if !it.target.is_empty() && !touched.contains(&it.target) {
            touched.push(it.target.clone());
        }
        match it.kind.as_str() {
            "service" => {
                // 原来没跑的就不启动；原来跑的才启动
                if it.was_running {
                    log.push(format!("启动服务/驱动 {}", it.name));
                    if !dry_run {
                        let (code, out, err) = util::run("sc.exe", &["start", &it.name]);
                        log.push(format!("    → {}", brief(code, &out, &err)));
                        if code != 0 {
                            log.push(format!(
                                "    ✗ sc start 返回 {code}: {}",
                                first_line(&out, &err)
                            ));
                        }
                    }
                } else {
                    log.push(format!("  {} 当时未运行", it.name));
                }
                // 恢复原始启动类型（例如从 demand 改回 auto）
                if !it.start_type.is_empty() && !it.start_type.eq_ignore_ascii_case("unknown") {
                    let want = normalize_start_type(&it.start_type);
                    if !want.is_empty() {
                        log.push(format!(
                            "    恢复启动类型为 {}（sc config {} start= {}）",
                            it.start_type, it.name, want
                        ));
                        if !dry_run {
                            let (code, out, err) =
                                util::run("sc.exe", &["config", &it.name, "start=", want]);
                            if code != 0 {
                                log.push(format!(
                                    "    ✗ sc config 返回 {code}: {}",
                                    first_line(&out, &err)
                                ));
                            }
                        }
                    }
                }
            }
            "process" => {
                log.push(format!(
                    "  进程 {} 已被结束，稍后按配置重新启动客户端",
                    it.name
                ));
            }
            other => log.push(format!("  未知类型 {other}: {}", it.name)),
        }
    }

    // ---- 重新拉起被停过的目标的客户端（Proxifier / Clash 之类）----
    if !touched.is_empty() {
        let mut any = false;
        for t in cfg
            .targets
            .iter()
            .filter(|t| touched.iter().any(|n| n == &t.name))
        {
            for raw in &t.relaunch_paths {
                let path = util::expand_env(raw);
                if path.trim().is_empty() {
                    continue;
                }
                if !std::path::Path::new(&path).exists() {
                    log.push(format!("  ✗ 找不到 {}（按 {} 找过了）", path, t.name));
                    continue;
                }
                let leaf = std::path::Path::new(&path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                if util::process_running(&leaf) {
                    log.push(format!("  {leaf} 已经在运行"));
                    continue;
                }
                if dry_run {
                    log.push(format!("  [测试] 将启动 {path}"));
                    continue;
                }
                if !any {
                    log.push("重新拉起代理客户端：".into());
                    any = true;
                }
                // 优先让 explorer 代开（普通用户身份，和你双击一样）；
                // 不行再直接启动（那样会继承管理员身份）。
                let launched = (util::launch_as_user(&path) && util::wait_for_process(&leaf, 10))
                    || (util::launch_direct(&path) && util::wait_for_process(&leaf, 10));
                if launched {
                    log.push(format!("  ✓ 已启动 {path}"));
                } else {
                    log.push(format!("  ✗ 启动 {path} 未成功，请手动打开"));
                }
            }
        }
    }

    // ---- 把 Windows 系统代理写回原值（放在最后：客户端起来后可能自己又设一次）----
    if let Some(pb) = st.proxy.clone() {
        log.push(format!(
            "系统代理：写回原值（ProxyEnable={} / ProxyServer='{}' / AutoConfigURL='{}'）",
            pb.enable, pb.server, pb.auto_config_url
        ));
        if dry_run {
            log.push("    （测试模式：不会真的改注册表）".into());
        } else {
            let (code, out, err) = util::run_ps(&proxy_reg_script(
                pb.enable,
                &pb.server,
                &pb.auto_config_url,
            ));
            log.push(format!("    → {}", brief(code, &out, &err)));
            if code != 0 {
                log.push(
                    "    ✗ 写回系统代理失败，请手动到「设置 → 网络和 Internet → 代理」里设置"
                        .into(),
                );
            } else if pb.enable == 1 {
                log.push(
                    "    （如果你的代理客户端里「系统代理」开关是关的，请再关掉系统代理，否则流量会指向它）"
                        .into(),
                );
            }
        }
    }

    if !dry_run {
        state::clear();
        log.push("已清空状态记录。".into());
        state::log("OK", "退出游戏模式：已恢复");
    } else {
        log.push("（测试模式：状态记录保留）".into());
    }
    log
}

/// `sc qc <name>` 解析出启动类型：AUTO_START / DEMAND_START / DISABLED / ...
fn query_start_type(name: &str) -> String {
    let (code, out, _) = util::run("sc.exe", &["qc", name]);
    if code != 0 {
        return "Unknown".into();
    }
    for line in out.lines() {
        let l = line.trim();
        if l.starts_with("START_TYPE") {
            if let Some(pos) = l.find(':') {
                let v = l[pos + 1..].trim();
                // 形如 "2   AUTO_START  (DELAYED)"
                let word = v.split_whitespace().nth(1).unwrap_or("").to_string();
                if !word.is_empty() {
                    return word;
                }
            }
        }
    }
    "Unknown".into()
}

/// 把 `sc qc` 的启动类型映射回 `sc config start=` 接受的词
fn normalize_start_type(t: &str) -> &'static str {
    let u = t.to_uppercase();
    if u.contains("AUTO") {
        "auto"
    } else if u.contains("DEMAND") {
        "demand"
    } else if u.contains("DISABLED") {
        "disabled"
    } else if u.contains("BOOT") {
        "boot"
    } else if u.contains("SYSTEM") {
        "system"
    } else {
        ""
    }
}

/// 从两条输出里挑第一条非空行（sc.exe 出错信息有时在 stderr、有时在 stdout）
fn first_line(a: &str, b: &str) -> String {
    let s = if !a.trim().is_empty() { a } else { b };
    s.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个"什么都没命中、但系统代理开着"的场景
    fn proxy_on_snapshot() -> Snapshot {
        Snapshot {
            targets: Vec::new(),
            tun_adapters: Vec::new(),
            tun_driver: String::new(),
            proxy_enabled: true,
            proxy_server: "127.0.0.1:7890".into(),
            proxy_auto_config: String::new(),
            admin: true,
            ready: false,
            conclusion: String::new(),
        }
    }

    fn empty_plan() -> Plan {
        Plan {
            services: Vec::new(),
            processes: Vec::new(),
        }
    }

    /// ★ 回归测试：计划为空时**也要**处理系统代理。
    ///
    /// 老代码在 `plan.is_empty()` 时直接 return，所以"只有 Clash（Auto）+ 没检测到 TUN"
    /// 这种最常见的情况下，点「停代理」连系统代理都不会关 —— 这是用户报的 bug。
    #[test]
    fn empty_plan_still_disables_system_proxy() {
        let cfg = Config {
            disable_system_proxy: true,
            ..Config::default()
        };
        let logs = stop(&cfg, &proxy_on_snapshot(), &empty_plan(), true); // dry_run
        assert!(
            logs.iter().any(|l| l.contains("系统代理：当前启用")),
            "没有进入系统代理处理分支：{logs:?}"
        );
        assert!(
            logs.iter().any(|l| l.contains("不会真的改注册表")),
            "测试模式没有拦住注册表写入：{logs:?}"
        );
    }

    /// 配置里关掉这个行为时，必须完全不接管
    #[test]
    fn system_proxy_respects_config_switch() {
        let cfg = Config {
            disable_system_proxy: false,
            ..Config::default()
        };
        let logs = stop(&cfg, &proxy_on_snapshot(), &empty_plan(), true);
        assert!(
            logs.iter().any(|l| l.contains("DisableSystemProxy=false")),
            "配置开关未生效：{logs:?}"
        );
        assert!(
            !logs.iter().any(|l| l.contains("系统代理：当前启用")),
            "配置关掉了却仍然要关系统代理：{logs:?}"
        );
    }

    /// 系统代理本来就没开时，只写一句"无需处理"
    #[test]
    fn no_proxy_means_nothing_to_do() {
        let cfg = Config {
            disable_system_proxy: true,
            ..Config::default()
        };
        let mut snap = proxy_on_snapshot();
        snap.proxy_enabled = false;
        let logs = stop(&cfg, &snap, &empty_plan(), true);
        assert!(
            logs.iter().any(|l| l.contains("未启用")),
            "判定不对：{logs:?}"
        );
    }

    /// 注册表脚本必须含正确的键和值，并且带 InternetSetOption 通知
    #[test]
    fn proxy_script_contains_key_and_notify() {
        let s = proxy_reg_script(1, "127.0.0.1:7890", "");
        assert!(s.contains("Internet Settings"));
        assert!(s.contains("ProxyEnable -Value 1"));
        assert!(s.contains("'127.0.0.1:7890'"));
        assert!(s.contains("InternetSetOption"));
        // 关掉时的写法
        let s0 = proxy_reg_script(0, "127.0.0.1:7890", "");
        assert!(s0.contains("ProxyEnable -Value 0"));
    }

    /// 单引号要转义，否则 PAC 地址里带引号会拼出坏脚本
    #[test]
    fn proxy_script_escapes_quotes() {
        let s = proxy_reg_script(1, "127.0.0.1:7890", "http://a'b/c.pac");
        assert!(s.contains("http://a''b/c.pac"), "单引号没转义：{s}");
    }
}
