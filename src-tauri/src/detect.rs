//! 检测层：服务/驱动、进程、虚拟网卡、系统代理，以及"每个目标现在是什么状态"。
//!
//! ★ 全部通过 Windows 自带命令实现，不引入任何第三方 crate：
//!     sc.exe       —— 服务与内核驱动（注意：Get-Service 看不到纯驱动，必须用 sc）
//!     tasklist.exe —— 进程列表
//!     netsh.exe    —— 网卡（Get-NetAdapter 不可用时的兜底）
//!     powershell   —— 虚拟网卡 / 系统代理这类需要 WMI/注册表的地方
//!
//! 正则匹配用标准库做不到，这里用**大小写无关的子串匹配 + `|` 分隔**的简化语法

use crate::config::{Action, Config, Target};
use crate::util;

/// 一个匹配到的服务/驱动
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceHit {
    pub name: String,
    pub status: String, // Running / Stopped / ...
}

/// 一个目标当前的状态
#[derive(Debug, Clone, serde::Serialize)]
pub struct TargetState {
    pub name: String,
    pub action: String,    // 配置里的动作
    pub effective: String, // 实际会做什么：Stop / Warn
    pub note: String,
    pub processes: Vec<String>, // 命中且正在运行的进程名
    pub services: Vec<ServiceHit>,
    pub running: bool, // 是否"有东西在跑"
}

/// 一个虚拟网卡
#[derive(Debug, Clone, serde::Serialize)]
pub struct AdapterInfo {
    pub name: String,
    pub status: String,
    pub description: String,
    /// 这条信息是用哪一级方法拿到的（Get-NetAdapter / netsh / .NET）
    pub source: String,
}

/// 整体状态快照
#[derive(Debug, Clone, serde::Serialize)]
pub struct Snapshot {
    pub targets: Vec<TargetState>,
    pub tun_adapters: Vec<AdapterInfo>,
    /// 正在运行的 TUN 类驱动服务名（空 = 没起来）。这是"TUN 开着"的兜底信号
    pub tun_driver: String,
    pub proxy_enabled: bool,
    pub proxy_server: String,
    /// PAC 脚本地址（AutoConfigURL，通常为空）
    pub proxy_auto_config: String,
    pub admin: bool,
    pub ready: bool,        // 环境是否"干净"（可以启动反作弊游戏）
    pub conclusion: String, // 给人看的一句话结论
}

/// 供配置扫描复用：`|` 分隔模式的大小写无关子串匹配
pub fn pattern_hit(pattern: &str, text: &str) -> bool {
    pat_match(pattern, text)
}

/// `|` 分隔的模式是否命中目标文本（大小写无关的子串匹配）
fn pat_match(pattern: &str, text: &str) -> bool {
    if pattern.trim().is_empty() {
        return false;
    }
    let t = text.to_lowercase();
    pattern
        .split('|')
        .map(|p| {
            p.trim()
                .trim_start_matches('^')
                .trim_end_matches('$')
                .to_lowercase()
        })
        .filter(|p| !p.is_empty())
        .any(|p| t.contains(&p))
}

/// 取所有服务与驱动的名字（含"已停止"的）。
///
/// 为什么不用 `Get-Service`：它**看不到纯内核驱动**，而 ProxifierDrv、Interception、
/// ViGEmBus 这类恰恰都是驱动 —— 必须用 `sc query type= driver`。
pub fn list_service_names() -> Vec<String> {
    let mut out = Vec::new();
    for kind in ["service", "driver"] {
        let (code, so, _) = util::run("sc.exe", &["query", "type=", kind, "state=", "all"]);
        if code != 0 {
            continue;
        }
        for line in so.lines() {
            let l = line.trim();
            if let Some(rest) = l.strip_prefix("SERVICE_NAME:") {
                out.push(rest.trim().to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// 查询单个服务/驱动的状态：返回 "Running" / "Stopped" / "NotFound"
pub fn service_status(name: &str) -> String {
    let (code, so, _) = util::run("sc.exe", &["query", name]);
    if code != 0 {
        return "NotFound".into();
    }
    for line in so.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("STATE") {
            // 形如 "STATE              : 4  RUNNING"
            if let Some(pos) = rest.find(':') {
                let v = rest[pos + 1..].trim();
                let word = v.split_whitespace().last().unwrap_or("").to_string();
                return word; // RUNNING / STOPPED / START_PENDING ...
            }
        }
    }
    "Unknown".into()
}

/// 当前正在运行的进程名（小写，去重）
///
/// ★ 编码坑：`tasklist` 的 CSV 输出用的是**控制台代码页**（中文系统 = GBK），
///   而我们按 UTF-8 解码 —— 中文进程名（例如「完美世界竞技平台.exe」）会变成乱码，
///   任何模式都匹配不上。所以：一旦发现输出里有替换字符（U+FFFD，说明解码坏了），
///   就改走 PowerShell（它的输出被强制成 UTF-8）。
pub fn running_processes() -> Vec<String> {
    let (code, so, _) = util::run("tasklist.exe", &["/fo", "csv", "/nh"]);
    if code != 0 || so.contains('\u{FFFD}') {
        return running_processes_ps();
    }
    let mut out = Vec::new();
    for line in so.lines() {
        // CSV 第一列就是映像名："chrome.exe","1234",...
        let line = line.trim();
        if !line.starts_with('"') {
            continue;
        }
        if let Some(end) = line[1..].find('"') {
            let name = &line[1..1 + end];
            let name = name.trim_end_matches(".exe").to_lowercase();
            if !name.is_empty() {
                out.push(name);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// 退回 PowerShell 拿进程名（UTF-8 输出，中文名不会乱）
fn running_processes_ps() -> Vec<String> {
    if let Some(v) =
        util::run_ps_json("Get-Process | Select-Object -ExpandProperty ProcessName -Unique")
    {
        let mut out: Vec<String> = to_string_vec(&v)
            .into_iter()
            .map(|s| s.to_lowercase())
            .collect();
        out.sort();
        out.dedup();
        return out;
    }
    Vec::new()
}

fn to_string_vec(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect(),
        serde_json::Value::String(s) => vec![s.clone()],
        _ => Vec::new(),
    }
}

/// 网卡原始信息（三级来源统一成这个形状）
#[derive(Debug, Clone)]
struct RawAdapter {
    name: String,
    status: String,
    description: String,
    source: &'static str,
}

/// 伪接口 / 过滤器条目 —— 这些**不是**真网卡，必须排除，否则 `tun` 这种关键字会误命中
/// （典型误报：`Teredo Tunneling Pseudo-Interface`，描述里带 "Tunneling"）。
fn is_pseudo_adapter(name: &str, description: &str) -> bool {
    let n = name.to_lowercase();
    let d = description.to_lowercase();
    let pseudo_words = [
        "teredo",
        "6to4",
        "ip-https",
        "isatap",
        "pseudo-interface",
        "pseudo interface",
        "loopback",
        "kernel debug",
        "wan miniport",
        "bluetooth device",
    ];
    if pseudo_words.iter().any(|w| n.contains(w) || d.contains(w)) {
        return true;
    }
    // 这些是挂在真网卡上的过滤器/调度器条目（名字通常以 -0000 结尾）
    let filter_words = [
        "-0000",
        "packet driver",
        "lightweight filter",
        "qos packet",
        "virtual wifi filter",
        "native wifi filter",
        "wfp ",
    ];
    filter_words.iter().any(|w| n.contains(w))
}

/// ① `Get-NetAdapter`（状态最规范：Up / Disconnected / Disabled…）
fn adapters_from_netadapter() -> Vec<RawAdapter> {
    let script = "Get-NetAdapter -ErrorAction SilentlyContinue | \
                  Select-Object Name,Status,InterfaceDescription";
    let Some(v) = util::run_ps_json(script) else {
        return Vec::new();
    };
    let vals = match v {
        serde_json::Value::Array(a) => a,
        other => vec![other],
    };
    vals.iter()
        .map(|a| RawAdapter {
            name: str_of(a, "Name"),
            status: str_of(a, "Status"),
            description: str_of(a, "InterfaceDescription"),
            source: "Get-NetAdapter",
        })
        .filter(|a| !a.name.is_empty())
        .collect()
}

/// 从 JSON 对象里取字符串字段
fn str_of(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

/// 解析 `netsh interface show interface` 的一行。
///
/// ★ 这个命令的输出**是本地化的**：中文系统是
/// `已启用  已连接  专用  WLAN 2`，英文系统是 `Enabled Connected Dedicated WLAN 2`。
/// 早期版本只认英文 `Connected`，于是中文系统上所有网卡都被判成"未连接"，
/// TUN 明明开着也检测不到 —— 这就是"TUN 模式没能停代理"的直接原因之一。
///
/// 返回 `(接口名, 是否已连接)`；不是数据行就返回 None。
fn parse_netsh_line(line: &str) -> Option<(String, bool)> {
    let l = line.trim();
    if l.is_empty()
        || l.starts_with("---")
        || l.starts_with("Admin State")
        || l.starts_with("管理员状态")
    {
        return None;
    }
    let cols: Vec<&str> = l.split_whitespace().collect();
    if cols.len() < 4 {
        return None;
    }
    // 第 3 列是"类型"（Dedicated/专用/Internal/内部…），名字是它后面的全部内容
    let type_tok = cols[2];
    let pos = l.find(type_tok)?;
    let name = l[pos + type_tok.len()..].trim().to_string();
    if name.is_empty() {
        return None;
    }
    // 状态列：中英文都要认（已连接 / Connected / 連線…）
    let state = cols[1].to_lowercase();
    let up = state.contains("connected") && !state.contains("disconnected")
        || state.contains("已连接")
        || state.contains("已連線")
        || state.contains("接続");
    Some((name, up))
}

/// ② `netsh interface show interface`（不依赖 WMI/CIM）
fn adapters_from_netsh() -> Vec<RawAdapter> {
    let (code, out, _) = util::run("netsh.exe", &["interface", "show", "interface"]);
    if code != 0 {
        return Vec::new();
    }
    let mut v = Vec::new();
    for line in out.lines() {
        if let Some((name, up)) = parse_netsh_line(line) {
            v.push(RawAdapter {
                name,
                status: if up { "Up" } else { "Disconnected" }.to_string(),
                description: String::new(),
                source: "netsh",
            });
        }
    }
    v
}

/// ③ 纯 .NET `NetworkInterface`（完全不依赖 CIM；状态是枚举名，不受系统语言影响）
///
/// 注意：这个方法会把 Teredo、WAN Miniport 这类伪接口也列出来，
/// 所以必须配合 `is_pseudo_adapter` 过滤（类型为 Tunnel 的直接丢掉）。
fn adapters_from_dotnet() -> Vec<RawAdapter> {
    let script = "[System.Net.NetworkInformation.NetworkInterface]::GetAllNetworkInterfaces() | \
                  Where-Object { $_.NetworkInterfaceType -ne 'Loopback' } | \
                  Select-Object Name,@{n='Description';e={$_.Description}},@{n='Status';e={[string]$_.OperationalStatus}},@{n='Kind';e={[string]$_.NetworkInterfaceType}}";
    let Some(v) = util::run_ps_json(script) else {
        return Vec::new();
    };
    let vals = match v {
        serde_json::Value::Array(a) => a,
        other => vec![other],
    };
    vals.iter()
        .filter(|a| !str_of(a, "Kind").eq_ignore_ascii_case("Tunnel"))
        .map(|a| RawAdapter {
            name: str_of(a, "Name"),
            status: match str_of(a, "Status").as_str() {
                s if s.eq_ignore_ascii_case("Up") => "Up".to_string(),
                s if s.eq_ignore_ascii_case("Down") => "Disconnected".to_string(),
                s => s.to_string(),
            },
            description: str_of(a, "Description"),
            source: ".NET",
        })
        .filter(|a| !a.name.is_empty())
        .collect()
}

/// 三级来源**合并**（不是"前面失败才用后面"）。
///
/// 为什么合并：`Get-NetAdapter` 在个别环境会缺条目、`netsh` 只有名字、`.NET` 有伪接口 ——
/// 任何一条路看到某块卡，我们就该知道它存在。同一块卡取"更活"的状态（只要有源说 Up 就当 Up），
/// 这样"TUN 明明开着却没检测到"的概率最低。
fn merged_raw() -> Vec<RawAdapter> {
    let mut all = adapters_from_dotnet();
    all.extend(adapters_from_netadapter());
    all.extend(adapters_from_netsh());

    let mut out: Vec<RawAdapter> = Vec::new();
    for a in all {
        if a.name.trim().is_empty() {
            continue;
        }
        match out
            .iter_mut()
            .find(|x| x.name.eq_ignore_ascii_case(&a.name))
        {
            Some(e) => {
                if !e.status.eq_ignore_ascii_case("Up") && a.status.eq_ignore_ascii_case("Up") {
                    e.status = "Up".to_string();
                    e.source = a.source; // 谁给的"Up"，就标谁
                }
                if e.description.is_empty() && !a.description.is_empty() {
                    e.description = a.description.clone();
                }
            }
            None => out.push(a),
        }
    }
    out
}

fn to_info(a: RawAdapter) -> AdapterInfo {
    AdapterInfo {
        name: a.name,
        status: a.status,
        description: a.description,
        source: a.source.to_string(),
    }
}

/// TUN/TAP 驱动服务是否在运行。
///
/// 这是**不依赖网卡枚举**的兜底信号：Clash/mihomo 开 TUN 时会把 wintun 驱动起来。
/// 只认名字明确的 TUN 类驱动，避免 `tunnel`、`atapi` 这种通用名误判。
fn tun_driver_running() -> Option<String> {
    const CANDIDATES: [&str; 5] = ["wintun", "wintun6", "tap0901", "tap", "win-tun"];
    for name in CANDIDATES {
        if service_status(name).eq_ignore_ascii_case("RUNNING") {
            return Some(name.to_string());
        }
    }
    None
}

/// 扫描网卡，返回 `(全部网卡, 命中虚拟网卡模式的网卡, 正在运行的 TUN 驱动名)`
pub fn scan_adapters(pattern: &str) -> (Vec<AdapterInfo>, Vec<AdapterInfo>, String) {
    // 只枚举一次（三次 PowerShell 调用不便宜）
    let raw = merged_raw();
    let tun_raw: Vec<RawAdapter> = raw
        .iter()
        .filter(|a| !is_pseudo_adapter(&a.name, &a.description))
        .filter(|a| pat_match(pattern, &a.description) || pat_match(pattern, &a.name))
        .cloned()
        .collect();
    let all: Vec<AdapterInfo> = raw.into_iter().map(to_info).collect();
    let tun: Vec<AdapterInfo> = tun_raw.into_iter().map(to_info).collect();
    let driver = tun_driver_running().unwrap_or_default();
    (all, tun, driver)
}

/// 系统代理（注册表 `HKCU\...\Internet Settings`）
///
/// 返回 `(是否开启, 代理服务器, PAC 地址)`。
/// 注意：本函数只**读**；真正关掉系统代理的动作在 `actions.rs`（可用配置关掉）。
pub fn system_proxy() -> (bool, String, String) {
    let mut enabled = false;
    let mut server = String::new();
    let mut pac = String::new();
    if let Some(v) = util::run_ps_json(
        "$p=Get-ItemProperty 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings' -ErrorAction SilentlyContinue; \
         [pscustomobject]@{ Enable=[int]$p.ProxyEnable; Server=[string]$p.ProxyServer; Pac=[string]$p.AutoConfigURL }",
    ) {
        enabled = v.get("Enable").and_then(|x| x.as_i64()).unwrap_or(0) == 1;
        server = v
            .get("Server")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        pac = v.get("Pac").and_then(|x| x.as_str()).unwrap_or("").to_string();
    }
    (enabled, server, pac)
}

/// 判断"该停什么"时需要的事实
struct Facts {
    /// 有 TUN 网卡在线
    tun_up: bool,
    /// 系统代理处于开启状态（ProxyEnable=1 或设了 PAC）
    proxy_on: bool,
    /// 用户是否允许动系统代理（配置 `DisableSystemProxy`）
    allow_proxy: bool,
}

/// 单个目标的状态 + **判定原因**（原因要写进日志，出问题才查得清）
fn target_state(
    t: &Target,
    procs: &[String],
    svc_names: &[String],
    facts: &Facts,
) -> (TargetState, String) {
    // 命中的进程（只列正在运行的）
    let mut hit_procs: Vec<String> = procs
        .iter()
        .filter(|p| pat_match(&t.process_pattern, p))
        .cloned()
        .collect();
    hit_procs.sort();
    hit_procs.dedup();

    // 命中的服务/驱动
    let mut hits: Vec<ServiceHit> = Vec::new();
    if !t.service_pattern.trim().is_empty() {
        for n in svc_names {
            if pat_match(&t.service_pattern, n) {
                hits.push(ServiceHit {
                    name: n.clone(),
                    status: service_status(n),
                });
            }
        }
    }
    let running = !hit_procs.is_empty()
        || hits
            .iter()
            .any(|h| h.status.eq_ignore_ascii_case("RUNNING"));

    // 实际动作：Auto 在"有 TUN 在线"或"系统代理开着（且允许动它）"时变成 Stop；
    // Hint 按当前状态分流 —— 没跑就是普通提醒，正在跑则标成"需你手动处理"（工具本身不动手）
    let (effective, why) = match t.action {
        Action::Stop => ("Stop", "配置里写的就是 Stop".to_string()),
        Action::Warn => ("Warn", "配置里只提醒".to_string()),
        Action::Hint => {
            if running {
                (
                    "Hint",
                    "正在运行，且这条是「需你手动处理」：本工具不会动它，请自行关掉它或重启电脑"
                        .to_string(),
                )
            } else {
                ("Warn", "已安装但当前未运行，无需处理".to_string())
            }
        }
        Action::Auto => {
            if facts.tun_up {
                ("Stop", "检测到虚拟网卡（TUN）在线".to_string())
            } else if facts.proxy_on && facts.allow_proxy {
                ("Stop", "系统代理处于开启状态".to_string())
            } else if facts.proxy_on {
                (
                    "Warn",
                    "系统代理开着，但配置里 DisableSystemProxy=false，所以不接管".to_string(),
                )
            } else {
                ("Warn", "没有 TUN 在线，系统代理未启用".to_string())
            }
        }
    };

    (
        TargetState {
            name: t.name.clone(),
            action: format!("{:?}", t.action),
            effective: effective.to_string(),
            note: t.note.clone(),
            processes: hit_procs,
            services: hits,
            running,
        },
        why,
    )
}

/// 采集整体快照（不打印详细检测日志）
pub fn snapshot(cfg: &Config, admin: bool) -> Snapshot {
    snapshot_ex(cfg, admin, false)
}

/// 采集快照；`verbose = true` 时把检测到的事实**全部**写进日志。
///
/// 动作类命令（停用/恢复/诊断）用 verbose，界面每 15 秒的后台刷新用非 verbose，
/// 这样日志既有细节、又不会被轮询刷屏。
pub fn snapshot_ex(cfg: &Config, admin: bool, verbose: bool) -> Snapshot {
    let t0 = std::time::Instant::now();
    let procs = running_processes();
    let svc_names = list_service_names();
    let (all_adapters, adapters, tun_driver) = scan_adapters(&cfg.vpn_adapter_pattern);
    // "TUN 在线"= 有匹配的网卡处于 Up，**或者** TUN 类驱动服务正在运行
    // （后者是兜底：某些环境网卡枚举拿不到 wintun 适配器，但驱动一定在跑）
    let tun_up =
        !tun_driver.is_empty() || adapters.iter().any(|a| a.status.eq_ignore_ascii_case("Up"));
    let (proxy_enabled, proxy_server, proxy_pac) = system_proxy();
    let proxy_on = proxy_enabled || !proxy_pac.trim().is_empty();

    let facts = Facts {
        tun_up,
        proxy_on,
        allow_proxy: cfg.disable_system_proxy,
    };

    if verbose {
        crate::state::log(
            "INFO",
            &format!(
                "扫描完成：进程 {} 个、服务/驱动 {} 个、耗时 {} ms",
                procs.len(),
                svc_names.len(),
                t0.elapsed().as_millis()
            ),
        );
        crate::state::log(
            "INFO",
            &format!(
                "系统代理：ProxyEnable={}  ProxyServer='{}'  AutoConfigURL='{}'  → 判定 {}（配置 DisableSystemProxy={}）",
                proxy_enabled as i32,
                proxy_server,
                proxy_pac,
                if proxy_on { "开启" } else { "未开启" },
                cfg.disable_system_proxy
            ),
        );
        // ---- 网卡：把**枚举到的全部**写进日志（诊断关键：能看出"当时到底有没有 TUN"）----
        crate::state::log(
            "INFO",
            &format!("枚举到 {} 张网卡（三级来源合并）：", all_adapters.len()),
        );
        for a in &all_adapters {
            crate::state::log(
                "INFO",
                &format!(
                    "    [{}] {}  |  {}  （来源 {}）",
                    a.status, a.name, a.description, a.source
                ),
            );
        }
        if tun_driver.is_empty() {
            crate::state::log(
                "INFO",
                "TUN 驱动服务：未运行（wintun / tap0901 / tap / wintun6 / win-tun 都不在运行）",
            );
        } else {
            crate::state::log("OK", &format!("TUN 驱动服务：{tun_driver} 正在运行"));
        }
        if adapters.is_empty() {
            crate::state::log(
                "WARN",
                "没有网卡命中 VpnAdapterPattern。如果此时 Clash 的 TUN 确实开着，\
                 请看上面那份「枚举到 N 张网卡」清单里有没有它 —— 把清单和配置里的 \
                 VpnAdapterPattern 一起发我，就能定位是「没建起来」还是「没认出来」",
            );
        } else {
            for a in &adapters {
                crate::state::log(
                    "INFO",
                    &format!(
                        "虚拟网卡命中：{} [{}] {} （来源 {}）",
                        a.name, a.status, a.description, a.source
                    ),
                );
            }
        }
    }

    let mut targets: Vec<TargetState> = Vec::new();
    for t in &cfg.targets {
        let (s, why) = target_state(t, &procs, &svc_names, &facts);
        if verbose {
            let svc = s
                .services
                .iter()
                .map(|h| format!("{}[{}]", h.name, h.status))
                .collect::<Vec<_>>()
                .join(", ");
            crate::state::log(
                "INFO",
                &format!(
                    "目标「{}」Action={} → 实际={}（原因：{}）",
                    t.name, s.action, s.effective, why
                ),
            );
            if !s.processes.is_empty() || !svc.is_empty() {
                crate::state::log(
                    "INFO",
                    &format!(
                        "    命中进程=[{}]  命中服务=[{}]",
                        s.processes.join(", "),
                        svc
                    ),
                );
            }
        }
        if s.running || !s.note.is_empty() {
            targets.push(s);
        }
    }

    // 结论：分两类 —— "本工具会自动处理" 和 "需要你自己处理"
    let blockers: Vec<String> = targets
        .iter()
        .filter(|s| s.effective == "Stop" && s.running)
        .map(|s| s.name.clone())
        .collect();
    let manual: Vec<String> = targets
        .iter()
        .filter(|s| s.effective == "Hint" && s.running)
        .map(|s| s.name.clone())
        .collect();
    let ready = blockers.is_empty() && manual.is_empty();
    let conclusion = if ready {
        "已确认无可能导致反作弊拒绝的组件。".to_string()
    } else {
        let mut parts: Vec<String> = Vec::new();
        if !blockers.is_empty() {
            parts.push(format!("{}（本工具会自动停用）", blockers.join("、")));
        }
        if !manual.is_empty() {
            parts.push(format!(
                "{}（需你自己关闭它或重启电脑，本工具按配置不会动它）",
                manual.join("、")
            ));
        }
        format!("仍有组件可能被反作弊拒绝：{}", parts.join("；"))
    };

    Snapshot {
        targets,
        tun_adapters: adapters,
        tun_driver,
        proxy_enabled,
        proxy_server,
        proxy_auto_config: proxy_pac,
        admin,
        ready,
        conclusion,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 伪接口必须被过滤掉 —— 否则 `Teredo Tunneling` 这类描述会让 "tun" 关键字误命中，
    /// 造成"假 TUN 在线"（症状：Clash 没开 TUN 也被停，或者反过来判断全乱）。
    #[test]
    fn pseudo_adapters_are_filtered_out() {
        assert!(is_pseudo_adapter(
            "Teredo Tunneling Pseudo-Interface",
            "Microsoft Teredo Tunneling Adapter"
        ));
        assert!(is_pseudo_adapter("6to4 Adapter", "Microsoft 6to4 Adapter"));
        assert!(is_pseudo_adapter("本地连接* 6", "WAN Miniport (IP)"));
        assert!(is_pseudo_adapter(
            "Loopback Pseudo-Interface 1",
            "Software Loopback Interface 1"
        ));
        assert!(is_pseudo_adapter(
            "以太网-Npcap Packet Driver (NPCAP)-0000",
            "Realtek PCIe GbE Family Controller"
        ));
        // 真网卡 / 真的 TUN 不能被过滤
        assert!(!is_pseudo_adapter("Mihomo", "Wintun Userspace Tunnel"));
        assert!(!is_pseudo_adapter("Clash", "Clash TUN Adapter"));
        assert!(!is_pseudo_adapter(
            "WLAN 2",
            "Realtek 8852CE WiFi 6E PCI-E NIC"
        ));
        assert!(!is_pseudo_adapter(
            "Radmin VPN",
            "Famatech Radmin VPN Ethernet Adapter"
        ));
    }

    /// 配置里的匹配语法：`|` 分隔、大小写无关、`^`/`$` 会被当装饰去掉
    #[test]
    fn pattern_matching_is_case_insensitive_substring() {
        assert!(pat_match("clash|mihomo|verge", "Clash Party"));
        assert!(pat_match("^npcap|pcap", "npcap"));
        assert!(!pat_match("clash|mihomo", "chrome"));
        assert!(!pat_match("", "anything"));
    }

    /// ★ 回归测试：`netsh` 的输出是**本地化**的。
    ///
    /// 早期版本只认英文 `Connected`，中文系统上所有网卡（包括 TUN）都被判成"未连接"，
    /// 于是 `Action = Auto` 的 Clash 永远不会被停 —— 这就是"TUN 模式下没能停代理"的直接原因。
    #[test]
    fn netsh_line_parsed_in_both_languages() {
        // 英文系统
        assert_eq!(
            parse_netsh_line("Enabled        Connected      Dedicated        WLAN 2"),
            Some(("WLAN 2".to_string(), true))
        );
        assert_eq!(
            parse_netsh_line("Enabled        Disconnected   Dedicated        以太网"),
            Some(("以太网".to_string(), false))
        );
        // 中文系统（当初漏掉的情况）
        assert_eq!(
            parse_netsh_line("已启用            已连接            专用               WLAN 2"),
            Some(("WLAN 2".to_string(), true))
        );
        assert_eq!(
            parse_netsh_line("已启用            已断开连接          专用               以太网"),
            Some(("以太网".to_string(), false))
        );
        // 名字里带空格也要完整取出来
        assert_eq!(
            parse_netsh_line(
                "已启用            已连接            专用               Mihomo Tunnel"
            ),
            Some(("Mihomo Tunnel".to_string(), true))
        );
        // 表头 / 分隔线 / 空行不是数据
        assert_eq!(
            parse_netsh_line("Admin State    State          Type             Interface Name"),
            None
        );
        assert_eq!(
            parse_netsh_line("管理员状态     状态           类型             接口名称"),
            None
        );
        assert_eq!(parse_netsh_line("-------------------------"), None);
        assert_eq!(parse_netsh_line(""), None);
    }
}
