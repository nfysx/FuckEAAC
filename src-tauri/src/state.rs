//! 状态文件：记录"这次进入游戏模式停掉了什么"，供恢复时使用。
//! 路径 `%ProgramData%\FuckEAAC\state.json`，与 PowerShell 版共用。

use serde::{Deserialize, Serialize};

/// 一条"被停掉的东西"
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoppedItem {
    pub kind: String,       // "service" | "process"
    pub name: String,       // 服务名 / 进程名
    pub was_running: bool,  // 当时是否在运行
    pub start_type: String, // 服务原始启动类型（如有）
    pub target: String,     // 属于哪个目标（显示用）
}

/// 系统代理的原始设置（进游戏模式时被我们关掉了，恢复时要写回去）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct ProxyBackup {
    /// 原来的 `ProxyEnable`（1 = 开）
    pub enable: i64,
    /// 原来的 `ProxyServer`（如 127.0.0.1:7890）
    pub server: String,
    /// 原来的 `AutoConfigURL`（PAC 脚本地址，通常为空）
    pub auto_config_url: String,
}

/// 整份状态
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct State {
    pub when: String,
    pub items: Vec<StoppedItem>,
    /// 被我们关掉的 Windows 系统代理原值（没动过就是 None）
    pub proxy: Option<ProxyBackup>,
}

fn state_file() -> std::path::PathBuf {
    crate::util::state_dir().join("state.json")
}

/// 写入状态（停用成功后调用）
pub fn save(state: &State) -> Result<(), String> {
    let dir = crate::util::state_dir();
    crate::util::ensure_dir(&dir);
    let txt = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    std::fs::write(state_file(), txt).map_err(|e| format!("写入状态失败: {e}"))
}

/// 读取状态；没有则返回 None（表示"没有需要恢复的东西"）
pub fn read() -> Option<State> {
    let p = state_file();
    if !p.exists() {
        return None;
    }
    let txt = std::fs::read_to_string(p).ok()?;
    serde_json::from_str::<State>(&txt).ok()
}

/// 清空状态（恢复完成后调用）
pub fn clear() {
    let _ = std::fs::remove_file(state_file());
}

/// 待办动作："切换到管理员模式"会重启进程，用它把用户刚点的操作带过去，重启后自动继续。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    /// "stop" | "restore"
    pub action: String,
    #[serde(default)]
    pub drivers_on_demand: bool,
}

fn pending_file() -> std::path::PathBuf {
    crate::util::state_dir().join("pending.json")
}

pub fn set_pending(p: &Pending) {
    let dir = crate::util::state_dir();
    crate::util::ensure_dir(&dir);
    if let Ok(txt) = serde_json::to_string(p) {
        let _ = std::fs::write(pending_file(), txt);
    }
}

/// 取出并删除待办动作（只执行一次）
pub fn take_pending() -> Option<Pending> {
    let p = pending_file();
    if !p.exists() {
        return None;
    }
    let txt = std::fs::read_to_string(&p).ok()?;
    let _ = std::fs::remove_file(&p);
    serde_json::from_str::<Pending>(&txt).ok()
}

/// 追加一行日志到 `%ProgramData%\FuckEAAC\fuckeaac.log`
pub fn log(level: &str, msg: &str) {
    let dir = crate::util::state_dir();
    crate::util::ensure_dir(&dir);
    let line = format!("[{}][{}] {}\r\n", crate::util::now_string(), level, msg);
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("fuckeaac.log"))
    {
        let _ = f.write_all(line.as_bytes());
    }
}
