//! 配置模型与默认目标清单。
//!

//!
//! 想增删"进游戏模式要停哪些软件"，改本文件里的 `default_config()` 即可（见 README-修改指南）。

use serde::{Deserialize, Serialize};

/// 对某个目标采取的动作
///
/// 默认值是 `Warn`：只有配置里明确写了 `"Action": "Stop"` 才会真去动它 ——
/// 这样"配置文件写错/字段漏了"最坏也只是少停一个软件，不会误停。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Action {
    /// 进入游戏模式时停掉它
    Stop,
    /// 只提醒，不动它（默认）
    #[default]
    Warn,
    /// 仅当存在启用状态的 TUN/TAP 虚拟网卡（或系统代理开着）时才停（给 Clash 系用）
    Auto,
    /// **需要处理，但本工具不动手** —— 只按当前状态分流并给出提示：
    ///
    /// * 没在运行 → 落到"仅提醒"栏（告诉你它装了，但现在没威胁）
    /// * 正在运行 → 落到"需停用"栏，但**不会**去 taskkill / sc stop，
    ///   而是提示你**自己关掉它，或者重启电脑**
    ///
    /// 给内核级反作弊这类"停它风险大、而且多半停不掉"的东西用：
    /// 内核对内核最容易出事（驱动拒绝卸载、乱停可能被那个平台封号），
    /// 所以把判断权留在人手里，工具只负责"看清并大声提醒"。
    Hint,
}

/// 一个"要处理的软件"
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct Target {
    /// 显示名
    pub name: String,
    /// Stop / Warn / Auto
    pub action: Action,
    /// 进程名正则（不区分大小写）
    pub process_pattern: String,
    /// 服务/驱动名正则
    pub service_pattern: String,
    /// 恢复后可选地重新拉起这些路径（支持 %ENV% 展开）
    pub relaunch_paths: Vec<String>,
    /// 停驱动时是否顺手改成"按需启动"（不再开机自动加载）
    pub drivers_on_demand: bool,
    /// 界面上的说明
    pub note: String,
}

/// 整份配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Config {
    /// 什么网卡算"虚拟网卡"（决定是否触发 Auto 动作）
    pub vpn_adapter_pattern: String,
    /// 游戏进程名（用来等它退出）
    pub game_process_names: Vec<String>,
    /// 目标清单
    pub targets: Vec<Target>,
    /// 游戏主程序路径（界面「浏览…」选的，可为空）
    #[serde(default)]
    pub game_exe: String,
    /// 进游戏模式时是否顺便关掉 **Windows 系统代理**（可还原）
    ///
    /// 只写 `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings` 下的
    /// ProxyEnable / ProxyServer / AutoConfigURL 三个值；原值会记进 state.json，恢复时写回。
    /// 不想要这个行为就把配置里这项改成 false。
    #[serde(default = "yes")]
    pub disable_system_proxy: bool,
}

/// serde 默认值：字段没写时当作 true（老的配置文件也能直接用）
fn yes() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        default_config()
    }
}

/// 内嵌默认清单（与 PowerShell 版 `Get-DefaultConfig` 一一对应）
pub fn default_config() -> Config {
    fn t(
        name: &str,
        action: Action,
        proc_pat: &str,
        svc_pat: &str,
        paths: &[&str],
        on_demand: bool,
        note: &str,
    ) -> Target {
        Target {
            name: name.to_string(),
            action,
            process_pattern: proc_pat.to_string(),
            service_pattern: svc_pat.to_string(),
            relaunch_paths: paths.iter().map(|s| s.to_string()).collect(),
            drivers_on_demand: on_demand,
            note: note.to_string(),
        }
    }

    Config {
        vpn_adapter_pattern: "tun|tap|wintun|clash|mihomo|proxifier|utun".into(),
        game_process_names: vec!["bf6".into(), "BF2042".into(), "FC25".into(), "FC26".into()],
        game_exe: String::new(),
        disable_system_proxy: true, // 默认顺手关掉 Windows 系统代理（可还原）
        targets: vec![
            // ---------------- 会真的动手停的 ----------------
            t(
                "Proxifier",
                Action::Stop,
                "proxifier",
                "proxifier",
                &[
                    r"%ProgramFiles(x86)%\Proxifier\Proxifier.exe",
                    r"%ProgramFiles%\Proxifier\Proxifier.exe",
                ],
                true,
                "WFP 内核驱动 ProxifierDrv 为 AUTO_START，退出界面仍驻留内核",
            ),
            t(
                "Clash / mihomo 系客户端",
                Action::Auto,
                "clash|mihomo|verge|sing-box|v2ray|xray",
                "clash|mihomo|party",
                &[
                    r"%ProgramFiles%\Clash Party\Clash Party.exe",
                    r"%LOCALAPPDATA%\Programs\Clash Party\Clash Party.exe",
                    r"%ProgramFiles%\Clash Verge\Clash Verge.exe",
                ],
                false,
                "仅系统代理模式放行；TUN 模式关停",
            ),
            t(
                "ProxyCap",
                Action::Stop,
                "proxycap",
                "proxycap",
                &[],
                false,
                "内核驱动型代理工具，与 Proxifier 同类",
            ),
            t(
                "SocksCap",
                Action::Stop,
                "sockscap",
                "sockscap",
                &[],
                false,
                "",
            ),
            t(
                "Netch",
                Action::Stop,
                "netch",
                "netch",
                &[],
                false,
                "常配合 TUN/TAP 虚拟网卡",
            ),
            // ---------------- 只提醒的 ----------------
            t(
                "Fiddler / Charles / HTTP Debugger",
                Action::Warn,
                "fiddler|charles|httpdebugger",
                "",
                &[],
                false,
                "HTTPS 抓包 / 中间人工具，反作弊较敏感",
            ),
            t(
                "Wireshark / npcap",
                Action::Warn,
                "wireshark|dumpcap",
                "^npcap|pcap",
                &[],
                false,
                "抓包驱动，反作弊较敏感",
            ),
            t(
                "Cheat Engine",
                Action::Warn,
                "cheatengine",
                "",
                &[],
                false,
                "内存修改工具，可能导致封号",
            ),
            t(
                "Process Hacker / System Informer",
                Action::Warn,
                "processhacker|systeminformer",
                "",
                &[],
                false,
                "",
            ),
            t(
                "Sandboxie",
                Action::Warn,
                "sandboxie|sbiectrl",
                "sbie",
                &[],
                false,
                "",
            ),
            // ---- 以下是 BF6/EAAC 明确报过错的软件（默认只提醒）----
            t(
                "AutoHotkey 宏",
                Action::Warn,
                "autohotkey|^ahk",
                "",
                &[],
                false,
                "会被报 AutoHotkey 不兼容",
            ),
            t(
                "Daemon Tools 虚拟光驱",
                Action::Warn,
                "daemon|dtlite|discsoft",
                "sptd|dtsoft|dtscsi",
                &[],
                false,
                "会被报 DiscoSoftLTD",
            ),
            t(
                "手柄映射 / 虚拟手柄",
                Action::Warn,
                "ds4windows|xpadder|rewasd",
                "vigem",
                &[],
                false,
                "会被报 Virtual Controller",
            ),
            t(
                "Interception 输入驱动",
                Action::Warn,
                "",
                "interception",
                &[],
                false,
                "EAAC 明确检测的输入重映射驱动",
            ),
            t(
                "调试器 (x64dbg/IDA/WinDbg)",
                Action::Warn,
                "x64dbg|x32dbg|ida64|^ida$|windbg",
                "",
                &[],
                false,
                "调试/注入类工具",
            ),
            t(
                "ReShade / 画质注入",
                Action::Warn,
                "reshade",
                "",
                &[],
                false,
                "注入游戏进程",
            ),
            t(
                "MSI Afterburner / RTSS",
                Action::Warn,
                "afterburner|rtss",
                "",
                &[],
                false,
                "部分版本会被反作弊拦截",
            ),
            t(
                "Voicemod 变声器",
                Action::Warn,
                "voicemod",
                "",
                &[],
                false,
                "虚拟音频驱动，偶发被拦截",
            ),
            // ---- 别的反作弊：跟 EAAC/EAC 抢内核地盘 ----
            // 两条都用 `Action::Hint`（需你手动处理）：
            //   * 没在跑 → 出现在界面的「仅提醒」栏
            //   * 正在跑 → 出现在「需停用」栏，并提示你**自己关掉它/重启电脑**
            // 为什么不动手：内核反作弊驱动多半拒绝 sc stop，而且乱停别的平台的反作弊
            // 可能让你在那边被封号 —— 判断权留给你，工具只负责看清 + 大声提醒。
            t(
                "腾讯 ACE 反作弊",
                Action::Hint,
                "sguard|ace-service|anticheatexpert",
                "ace-base|ace-game|ace-advt|ace-ssc|ace-core|anticheatexpert",
                &[],
                false,
                "腾讯系内核反作弊。驱动是按需启动、平时不加载；\
                 玩过这些游戏后若 ACE-BASE 还在运行，EAAC/EAC 会报 incompatible driver。",
            ),
            t(
                "完美平台反作弊",
                Action::Hint,
                "完美世界竞技平台|perfectworldarena|pwarena",
                "messagetransfer|perfectworldanticheat|perfectprotector",
                &[],
                false,
                "完美世界竞技平台的内核驱动：MessageTransfer.sys是蓝屏、\
                 与 VirtualBox / eNSP 冲突的元凶",
            ),
        ],
    }
}

/// 配置来源（界面上显示用）
#[derive(Debug, Clone, Serialize)]
pub struct ConfigLoad {
    pub config: Config,
    pub source: String,
    pub path: String,
}

/// 从程序目录读 `FuckEAAC.config.json`；不存在就用内嵌默认值。
/// 与 PS 版行为一致：**不会**自动生成配置文件，只有用户点「导出配置」时才写盘。
pub fn load() -> ConfigLoad {
    let path = crate::util::app_dir().join("FuckEAAC.config.json");
    let path_str = path.to_string_lossy().to_string();
    if path.exists() {
        if let Ok(txt) = std::fs::read_to_string(&path) {
            match serde_json::from_str::<Config>(&txt) {
                Ok(cfg) => {
                    return ConfigLoad {
                        config: cfg,
                        source: "配置文件".into(),
                        path: path_str,
                    }
                }
                Err(e) => {
                    return ConfigLoad {
                        config: default_config(),
                        source: format!("配置文件解析失败（{e}），已回退内嵌默认值"),
                        path: path_str,
                    }
                }
            }
        }
    }
    ConfigLoad {
        config: default_config(),
        source: "内嵌默认值".into(),
        path: path_str,
    }
}

/// 把当前配置写回 `FuckEAAC.config.json`
pub fn save(cfg: &Config) -> Result<String, String> {
    let path = crate::util::app_dir().join("FuckEAAC.config.json");
    let txt = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, txt).map_err(|e| format!("写入失败: {e}"))?;
    Ok(path.to_string_lossy().to_string())
}

/// 判断"这台机器上到底有没有装/跑这个软件"，据此决定配置里保留哪些目标。
///
/// 命中任一条件就算"存在"：
/// 1. 它的服务/内核驱动在当前系统里注册了（装了驱动型软件）
/// 2. 它的进程正在运行
/// 3. 它的重新启动路径里有文件存在（装了但没运行）
fn target_present(t: &Target, services: &[String], procs: &[String]) -> Option<String> {
    // 1) 服务/驱动
    if !t.service_pattern.trim().is_empty() {
        for s in services {
            if crate::detect::pattern_hit(&t.service_pattern, s) {
                return Some(format!("服务/驱动 {s}"));
            }
        }
    }
    // 2) 正在运行的进程
    for p in procs {
        if crate::detect::pattern_hit(&t.process_pattern, p) {
            return Some(format!("进程 {p}"));
        }
    }
    // 3) 安装路径存在
    for raw in &t.relaunch_paths {
        let p = crate::util::expand_env(raw);
        if !p.is_empty() && std::path::Path::new(&p).exists() {
            return Some(format!("已安装 {}", p));
        }
    }
    None
}

/// 扫描本机 → 生成"只包含这台机器上真有东西"的配置。
///
/// 返回 (新配置, 日志行)。原来的内嵌清单相当于"知识库"，
/// 这台机器上一条都没命中的目标会被丢掉 —— 这样配置文件短、界面干净。
pub fn detect_and_generate() -> (Config, Vec<String>) {
    let kb = default_config();
    let mut log = Vec::new();

    let services = crate::detect::list_service_names();
    let procs = crate::detect::running_processes();
    log.push(format!(
        "扫描完成：系统里有 {} 个服务/驱动、{} 个进程在运行。",
        services.len(),
        procs.len()
    ));

    let mut kept: Vec<Target> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();
    for t in kb.targets.iter() {
        match target_present(t, &services, &procs) {
            Some(why) => {
                log.push(format!("  ✓ 保留 {}（{why}）", t.name));
                kept.push(t.clone());
            }
            None => dropped.push(t.name.clone()),
        }
    }
    if !dropped.is_empty() {
        log.push(format!(
            "  – 跳过 {} 项本机没有的：{}",
            dropped.len(),
            dropped.join("、")
        ));
    }
    log.push(format!("生成的配置包含 {} 个目标。", kept.len()));

    let cfg = Config {
        vpn_adapter_pattern: kb.vpn_adapter_pattern,
        game_process_names: kb.game_process_names,
        game_exe: String::new(),
        disable_system_proxy: true,
        targets: kept,
    };
    (cfg, log)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 扫描生成配置：只应包含"这台机器上真的有"的目标，且名字都在知识库里
    #[test]
    fn detect_and_generate_only_keeps_known_targets() {
        let (cfg, log) = detect_and_generate();
        let kb = default_config();

        assert!(!log.is_empty(), "扫描应产生说明日志");
        for t in &cfg.targets {
            assert!(
                kb.targets.iter().any(|k| k.name == t.name),
                "生成了知识库里没有的目标: {}",
                t.name
            );
        }
        // 生成结果不应超过知识库规模
        assert!(cfg.targets.len() <= kb.targets.len());
        println!("扫描说明：");
        for l in &log {
            println!("  {l}");
        }
        println!(
            "保留 {} / 知识库 {} 项",
            cfg.targets.len(),
            kb.targets.len()
        );
    }

    /// 默认清单里的模式匹配应当能命中常见写法
    #[test]
    fn patterns_hit_expected_names() {
        let kb = default_config();
        let proxifier = kb.targets.iter().find(|t| t.name == "Proxifier").unwrap();
        assert!(crate::detect::pattern_hit(
            &proxifier.service_pattern,
            "ProxifierDrv"
        ));
        assert!(crate::detect::pattern_hit(
            &proxifier.process_pattern,
            "proxifier"
        ));
        let clash = kb
            .targets
            .iter()
            .find(|t| t.name.contains("Clash"))
            .unwrap();
        assert!(crate::detect::pattern_hit(
            &clash.process_pattern,
            "clash-party"
        ));
        assert!(crate::detect::pattern_hit(&clash.service_pattern, "Mihomo"));
        // 不该误命中
        assert!(!crate::detect::pattern_hit(
            &proxifier.service_pattern,
            "Spooler"
        ));
    }
}
