//! 前后端接口（Tauri commands）。
//!
//! 前端只通过这些命令与后端交互，所以"程序能做什么"看这一个文件就够了：
//! 没有网络请求、没有隐藏行为 —— 全部是本地检测/停用/恢复。

use crate::actions;
use crate::config::{self, Config};
use crate::detect;
use crate::state;
use crate::util;
use std::sync::Mutex;
use tauri::{Manager, State}; // Manager 提供 get_webview_window（隐藏窗口用）

/// 全局状态：配置 + 配置来源 + 是否演习模式
pub struct AppState {
    pub cfg: Mutex<Config>,
    pub source: Mutex<String>,
    pub dry_run: Mutex<bool>,
    /// 启动时扫描生成的说明（显示在日志里）
    pub boot_logs: Mutex<Vec<String>>,
}

impl AppState {
    pub fn new() -> Self {
        let mut boot = Vec::new();
        let loaded = config::load();

        // ★ 不再依赖写死的默认清单：
        //   第一次运行（没有配置文件）时，扫描本机 → 只保留"这台机器上真的有"的目标 → 生成配置文件。
        //   之后就以这个配置文件为准；想重新扫描，点界面上的「重新扫描生成配置」。
        let has_file = std::path::Path::new(&loaded.path).exists();
        let (cfg, source) = if has_file {
            (loaded.config, loaded.source)
        } else {
            let (detected, mut logs) = config::detect_and_generate();
            match config::save(&detected) {
                Ok(p) => logs.push(format!("已生成配置文件：{p}")),
                Err(e) => logs.push(format!("✗ 生成配置文件失败：{e}")),
            }
            boot.extend(logs);
            (detected, "首次运行扫描生成".to_string())
        };

        Self {
            cfg: Mutex::new(cfg),
            source: Mutex::new(source),
            dry_run: Mutex::new(false),
            boot_logs: Mutex::new(boot),
        }
    }
    pub fn config(&self) -> Config {
        self.cfg.lock().unwrap().clone()
    }
    pub fn source(&self) -> String {
        self.source.lock().unwrap().clone()
    }
    pub fn dry_run(&self) -> bool {
        *self.dry_run.lock().unwrap()
    }
    pub fn take_boot_logs(&self) -> Vec<String> {
        std::mem::take(&mut *self.boot_logs.lock().unwrap())
    }
}

fn now() -> String {
    util::now_string()
}

/// 只有托盘菜单/侧栏点「退出」才真的结束进程；否则关窗只是收进托盘保活。
/// 主窗口的 CloseRequested 处理器会读这个标志（见 main.rs）。
pub static REALLY_EXIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 真正退出程序
#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) {
    REALLY_EXIT.store(true, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
}

/// 把主窗口收进托盘（点 X 选择"留在后台托管"时用）。
///
/// 为什么要有这个命令：主窗口的关闭被 `main.rs` 拦下来了（见 `on_window_event`），
/// 所以"隐藏"必须由一个明确的动作完成，而不是靠默认的关窗行为。
#[tauri::command]
pub fn hide_window(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    state::log("INFO", "主窗口已收进托盘，程序将继续在后台运行");
}

/// 记下"用户想做的操作"，等切换到管理员模式重启后自动继续
#[tauri::command]
pub fn set_pending(action: String, drivers_on_demand: bool) -> serde_json::Value {
    state::set_pending(&state::Pending {
        action,
        drivers_on_demand,
    });
    serde_json::json!({ "ok": true })
}

/// 取出待办动作（前端启动时调用一次；取完即删，只执行一次）
#[tauri::command]
pub fn take_pending() -> serde_json::Value {
    match state::take_pending() {
        Some(p) => serde_json::json!({ "pending": p }),
        None => serde_json::json!({ "pending": null }),
    }
}

/// 基本信息：版本、是否管理员、各类路径。界面右上角与侧栏用。
#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> serde_json::Value {
    serde_json::json!({
        "name": "FuckEAAC",
        "version": env!("CARGO_PKG_VERSION"),
        "backend": "rust-tauri",
        "admin": util::is_admin(),
        "configSource": state.source(),
        "configPath": config::load().path,
        "stateDir": util::state_dir().to_string_lossy(),
        "exe": std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        "time": now(),
        // 启动时扫描生成的说明（首次运行才会有内容）
        "bootLogs": state.take_boot_logs(),
        "targetCount": state.config().targets.len(),
        // state.json 里还留着几项"停了但没恢复"的记录（比如上次被强杀）→ 界面提示可一键恢复
        "pendingRestore": state::read().map(|s| s.items.len()).unwrap_or(0),
        // 正在等待游戏进程吗（进程刚重启时一定是 null，等待线程随进程结束）
        "playing": crate::play::is_active(),
    })
}

/// 重新扫描本机 → 只保留"真的有"的目标 → 写回配置文件
#[tauri::command]
pub fn rescan_config(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let (cfg, logs) = config::detect_and_generate();
    let path = config::save(&cfg)?;
    let n = cfg.targets.len();
    *state.cfg.lock().unwrap() = cfg;
    *state.source.lock().unwrap() = "重新扫描生成".into();
    Ok(serde_json::json!({ "ok": true, "logs": logs, "path": path, "targets": n }))
}

/// 采集一次状态快照（会实际执行 sc.exe / tasklist 等查询，约 100~600ms）
#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let cfg = state.config();
    let snap = detect::snapshot(&cfg, util::is_admin());
    // 后台每 15 秒刷一次：命令原文已经写进日志文件了，不必再往界面灌（否则刷屏）
    util::clear_cmd_log();
    Ok(serde_json::json!({
        "ok": true,
        "snapshot": snap,
        "dryRun": state.dry_run(),
        "configSource": state.source(),
        "time": now(),
    }))
}

/// 读取当前配置（界面"编辑配置"用）
#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> serde_json::Value {
    serde_json::json!({ "ok": true, "config": state.config(), "source": state.source() })
}

/// 把当前配置写回 FuckEAAC.config.json
#[tauri::command]
pub fn export_config(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let cfg = state.config();
    match config::save(&cfg) {
        Ok(path) => Ok(serde_json::json!({ "ok": true, "path": path })),
        Err(e) => Err(e),
    }
}

/// 勾选/取消「演习模式」：只打印不执行
#[tauri::command]
pub fn set_dry_run(state: State<'_, AppState>, on: bool) -> serde_json::Value {
    *state.dry_run.lock().unwrap() = on;
    serde_json::json!({ "ok": true, "dryRun": on })
}

/// 把一次快照的"判定摘要"整理成界面能看的几行（为什么停 / 为什么不停）
fn snap_summary(snap: &detect::Snapshot) -> Vec<String> {
    let mut v = Vec::new();
    v.push(format!("环境结论：{}", snap.conclusion));
    v.push(format!(
        "系统代理：{}",
        if snap.proxy_enabled {
            format!(
                "已开启（{}）{}",
                snap.proxy_server,
                if snap.proxy_auto_config.is_empty() {
                    String::new()
                } else {
                    format!("，PAC={}", snap.proxy_auto_config)
                }
            )
        } else if !snap.proxy_auto_config.is_empty() {
            format!("未开开关，但设置 PAC={}", snap.proxy_auto_config)
        } else {
            "未开启".to_string()
        }
    ));
    v.push(format!(
        "虚拟网卡：命中 {} 张；TUN 驱动服务：{}",
        snap.tun_adapters.len(),
        if snap.tun_driver.is_empty() {
            "未运行".to_string()
        } else {
            format!("{} 正在运行", snap.tun_driver)
        }
    ));
    if snap.tun_adapters.is_empty() {
        v.push("  （没有网卡命中 VpnAdapterPattern）".to_string());
    } else {
        for a in &snap.tun_adapters {
            v.push(format!(
                "  {} [{}] {}（来源 {}）",
                a.name, a.status, a.description, a.source
            ));
        }
    }
    for t in &snap.targets {
        v.push(format!(
            "  {} → {}（Action={}）{}",
            t.name,
            if t.effective == "Stop" {
                "会停用"
            } else {
                "仅提醒"
            },
            t.action,
            if t.running { "，正在运行" } else { "" }
        ));
    }
    v
}

/// 退回检测到的"系统命令原文"（给界面显示，方便对照系统英文报错）
fn cmd_transcript() -> Vec<String> {
    let recs = util::take_cmd_log();
    if recs.is_empty() {
        return Vec::new();
    }
    let mut v = vec!["── 系统命令 ──".to_string()];
    v.extend(util::cmd_log_lines(&recs));
    v
}

/// 进入游戏模式：停用命中的服务/驱动/进程，并关掉 Windows 系统代理
///
/// 返回 `{ ok, logs[], stopped, snapshot }` —— 日志行直接显示在界面日志区。
#[tauri::command]
pub fn stop_targets(
    state: State<'_, AppState>,
    drivers_on_demand: bool,
) -> Result<serde_json::Value, String> {
    let cfg = state.config();
    let dry = state.dry_run();
    let admin = util::is_admin();
    // verbose=true：扫描细节（网卡/代理/每个目标的判定原因）全部写进日志文件
    let snap = detect::snapshot_ex(&cfg, admin, true);

    if !admin && !dry {
        state::log("WARN", "未提权，无法停用内核驱动/服务");
        let mut logs = snap_summary(&snap);
        logs.push("✗ 当前不是管理员权限，无法停用内核驱动/服务。".to_string());
        logs.push("  点右上角「⚠ 普通权限」即可切换到管理员模式。".to_string());
        logs.extend(cmd_transcript());
        return Ok(serde_json::json!({
            "ok": false,
            "needAdmin": true,
            "logs": logs,
        }));
    }

    let plan = actions::build_plan(&cfg, &snap, drivers_on_demand);
    let stopped = plan.services.len() + plan.processes.len();
    let mut logs = snap_summary(&snap);
    logs.extend(actions::stop(&cfg, &snap, &plan, dry));
    logs.extend(cmd_transcript());

    Ok(serde_json::json!({
        "ok": true,
        "logs": logs,
        "stopped": stopped,
        "dryRun": dry,
        "snapshot": detect::snapshot(&cfg, admin),
    }))
}

/// 恢复：按 state.json 把停掉的服务启动回来、还原启动类型，并把系统代理写回原值
#[tauri::command]
pub fn restore_targets(state: State<'_, AppState>) -> serde_json::Value {
    let cfg = state.config();
    let dry = state.dry_run();
    let admin = util::is_admin();
    if !admin && !dry {
        return serde_json::json!({
            "ok": false,
            "needAdmin": true,
            "logs": ["当前不是管理员权限，无法恢复服务/驱动。",
                     "点右上角「⚠ 普通权限」切换后再试。"],
        });
    }
    let mut logs = actions::restore(&cfg, dry);
    logs.extend(cmd_transcript());
    serde_json::json!({
        "ok": true,
        "logs": logs,
        "dryRun": dry,
        "snapshot": detect::snapshot(&cfg, admin),
    })
}

/// 切换到管理员模式。
///
/// ★ Windows 不允许"把已有进程提升为管理员"（UAC 的设计要求必须新起进程），
///   所以这里做的是：启动提权实例 → 稍等它就绪 → **自动退出当前实例**，
///   于是最终只剩一个窗口（位置/大小已由 main.rs 存下来，新实例会恢复）。
#[tauri::command]
pub fn relaunch_admin(app: tauri::AppHandle) -> serde_json::Value {
    if util::is_admin() {
        return serde_json::json!({ "ok": true, "already": true, "logs": ["当前已经是管理员权限。"] });
    }
    let ok = util::relaunch_elevated();
    if ok {
        // 给提权实例一点启动时间；**确认它真的在运行**之后才关掉自己，
        // 否则万一提权实例启动失败，窗口会凭空消失（那种情况下保持原窗口不动更安全）。
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            if util::count_same_process() >= 2 {
                REALLY_EXIT.store(true, std::sync::atomic::Ordering::SeqCst);
                app.exit(0);
            } else {
                state::log("WARN", "提权实例未拉起，保留当前窗口");
            }
        });
    }
    serde_json::json!({
        "ok": ok,
        "logs": [ if ok {
            "已请求以管理员身份重启：请在 UAC 窗口点「是」，本窗口会自动关闭，由提权实例接管。"
        } else {
            "提权请求失败或被拒绝。"
        } ],
    })
}

/// 进入"等待游戏"：停用之后调用，后台盯着游戏进程，游戏退出就自动恢复。
///
/// * `processes` 可以是进程名、也可以是 exe 路径（内部会取文件名）
/// * 界面通过 `play_status` 轮询结果，结束时的日志在 `status.logs` 里
#[tauri::command]
pub fn start_play(processes: Vec<String>, timeout_minutes: u64) -> serde_json::Value {
    match crate::play::start(processes, timeout_minutes) {
        Ok(()) => serde_json::json!({ "ok": true, "status": crate::play::status() }),
        Err(e) => serde_json::json!({ "ok": false, "error": e }),
    }
}

/// 查一次等待状态（界面每 3 秒轮询）
#[tauri::command]
pub fn play_status() -> serde_json::Value {
    serde_json::json!({ "ok": true, "status": crate::play::status() })
}

/// 结束等待（不负责恢复；恢复由「恢复代理工具」明确执行）
#[tauri::command]
pub fn cancel_play() -> serde_json::Value {
    let was = crate::play::cancel();
    serde_json::json!({ "ok": true, "canceled": was, "status": crate::play::status() })
}

/// 用系统默认程序打开一个文件/目录（打开配置文件、日志）
#[tauri::command]
pub fn open_path(path: String) -> serde_json::Value {
    if path.trim().is_empty() {
        return serde_json::json!({ "ok": false, "error": "路径为空" });
    }
    if !std::path::Path::new(&path).exists() {
        return serde_json::json!({ "ok": false, "error": format!("不存在: {path}") });
    }
    let mut c = std::process::Command::new("cmd.exe");
    c.args(["/C", "start", "", &path]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(util::CREATE_NO_WINDOW);
    }
    match c.spawn() {
        Ok(_) => serde_json::json!({ "ok": true }),
        Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
    }
}

/// 环境诊断：EAAC/BF6 需要的东西现在是什么状态（纯本地查询）
#[tauri::command]
pub fn run_diag(state: State<'_, AppState>) -> serde_json::Value {
    let cfg = state.config();
    let checks = crate::diag::run_checks(&cfg);
    serde_json::json!({
        "ok": true,
        "checks": checks,
        "logs": cmd_transcript(),   // 诊断用的系统命令原文也一并给界面
        "time": now(),
    })
}
