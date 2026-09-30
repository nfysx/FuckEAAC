//! 游戏模式：停用代理工具之后，在后台等游戏进程，游戏退出时**自动恢复**。
//!
//! 设计说明（方便你审查这个文件就够了）：
//! - 只有一条后台线程，每 5 / 10 秒用 `tasklist.exe` 问一次"这些进程还在不在"。
//!   不注入进程、不挂钩子、不碰反作弊 —— 只是"看进程在不在"。
//! - 等待期间随时可以结束：点「恢复代理工具」或退出程序都会问你要不要先恢复。
//! - 状态只存在本机内存和 `%ProgramData%\FuckEAAC\fuckeaac.log` 里，没有任何网络行为。

use crate::actions;
use crate::state;
use crate::util;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// 等游戏进程出现的最长时间：超过就放弃等待（代理仍是停用状态，界面会一直提醒你手动恢复）
const APPEAR_MINUTES: u64 = 30;
/// 还没进游戏时，多久查一次
const POLL_UP_SECS: u64 = 5;
/// 游戏运行中时，多久查一次
const POLL_GAME_SECS: u64 = 10;

/// 用户要求"别再等了"（点恢复 / 退出程序时置位）
static CANCEL: AtomicBool = AtomicBool::new(false);

/// 给界面轮询的等待状态
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayStatus {
    /// 是否正在后台等待
    pub active: bool,
    /// waiting-up（等游戏出现） / in-game（游戏运行中） / restoring / done / canceled / gave-up
    pub phase: String,
    /// 在等哪些进程（显示用，如 "bf6 / BF2042"）
    pub process: String,
    /// 什么时候开始等的
    pub started: String,
    /// 已经等了多久（秒）
    pub elapsed_secs: u64,
    /// 结束原因
    pub note: String,
    /// 这次等待过程中产生的日志（结束时一次性交给界面显示）
    pub logs: Vec<String>,
}

fn slot() -> &'static Mutex<PlayStatus> {
    static S: OnceLock<Mutex<PlayStatus>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(PlayStatus {
            phase: "idle".into(),
            ..Default::default()
        })
    })
}

pub fn status() -> PlayStatus {
    slot().lock().unwrap().clone()
}

pub fn is_active() -> bool {
    slot().lock().unwrap().active
}

/// 只改状态里的一小块（其余字段保留）
fn set(f: impl FnOnce(&mut PlayStatus)) {
    if let Ok(mut g) = slot().lock() {
        f(&mut g);
    }
}

fn canceled() -> bool {
    CANCEL.load(Ordering::SeqCst)
}

/// 把用户填的东西变成进程名：
/// `D:\Games\BF6\bf6.exe` → `bf6`；`bf6.exe` → `bf6`；`bf6` → `bf6`
fn normalize(p: &str) -> String {
    let p = p.trim();
    if p.is_empty() {
        return String::new();
    }
    let leaf = std::path::Path::new(p)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string());
    leaf.trim().to_string()
}

/// 列表里任意一个在运行就算"游戏在"（BF6 / BF2042 这种，谁先开都认）
fn any_running(names: &[String]) -> bool {
    names.iter().any(|n| util::process_running(n))
}

fn note(logs: &mut Vec<String>, level: &str, msg: String) {
    state::log(level, &msg);
    logs.push(msg);
}

/// 收尾：写回最终状态，把日志留给界面显示
fn finish(phase: &str, note_text: String, logs: Vec<String>, t0: Instant) {
    if let Ok(mut s) = slot().lock() {
        s.active = false;
        s.phase = phase.to_string();
        s.note = note_text;
        s.elapsed_secs = t0.elapsed().as_secs();
        s.logs = logs;
    }
}

/// 开始等待。`processes` 里可以是路径、可以是进程名，内部会归一化成映像名。
pub fn start(processes: Vec<String>, timeout_minutes: u64) -> Result<(), String> {
    if is_active() {
        return Err("正在等待游戏进程。".into());
    }

    let mut names: Vec<String> = Vec::new();
    for p in processes.iter().map(|p| normalize(p)) {
        if !p.is_empty() && !names.iter().any(|n| n.eq_ignore_ascii_case(&p)) {
            names.push(p);
        }
    }
    if names.is_empty() {
        return Err("没有可等待的进程名：请在「游戏启动」内填入游戏主程序。".into());
    }

    // 自检：这台机器上到底能不能可靠地"看到进程"？
    // 有些加固环境里 tasklist 会被直接拒绝（PowerShell 兜底也失败），
    // 那种情况下等待会变成"永远等不到/游戏退出了还不知道"，不如现在就明说。
    let me = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "fuckeaac".into());
    if util::count_process(&me).is_none() {
        return Err("此机器上查不到进程列表，\
             没法可靠地等待游戏进程。\n\
             建议：改用「仅停用代理」。"
            .into());
    }

    let mins = if timeout_minutes == 0 {
        720
    } else {
        timeout_minutes
    };
    CANCEL.store(false, Ordering::SeqCst);

    let label = names.join(" / ");
    set(|s| {
        *s = PlayStatus {
            active: true,
            phase: "waiting-up".into(),
            process: label.clone(),
            started: util::now_string(),
            ..Default::default()
        };
    });
    state::log(
        "STEP",
        &format!("进入等待：{label}（出现后最多再等 {mins} 分钟）"),
    );

    let t0 = Instant::now();
    std::thread::spawn(move || run(names, mins, t0));
    Ok(())
}

/// 请求结束等待（后台线程最多 10 秒内响应）。
///
/// 注意：这里**不**负责恢复 —— 恢复由界面上的「恢复代理工具」明确执行，
/// 免得"我以为它在等，其实已经停了"这种状态不清不楚的情况。
pub fn cancel() -> bool {
    if !is_active() {
        return false;
    }
    CANCEL.store(true, Ordering::SeqCst);
    state::log("WARN", "收到结束等待的请求");
    true
}

fn run(names: Vec<String>, mins: u64, t0: Instant) {
    let label = names.join(" / ");
    let mut logs: Vec<String> = Vec::new();

    // ---------- 1) 等游戏进程出现 ----------
    let up_deadline = Duration::from_secs(APPEAR_MINUTES * 60);
    let mut appeared = any_running(&names);
    while !appeared && !canceled() && t0.elapsed() < up_deadline {
        std::thread::sleep(Duration::from_secs(POLL_UP_SECS));
        appeared = any_running(&names);
        set(|s| s.elapsed_secs = t0.elapsed().as_secs());
    }

    if canceled() {
        note(&mut logs, "WARN", "等待已结束。".into());
        finish("canceled", "等待已取消".into(), logs, t0);
        return;
    }
    if !appeared {
        note(
            &mut logs,
            "WARN",
            format!(
                "{APPEAR_MINUTES} 分钟内没等到 {label}，已停止等待。\n\
                 ★ 代理工具**仍然是停用状态**。"
            ),
        );
        finish(
            "gave-up",
            "没等到游戏进程，已停止等待".to_string(),
            logs,
            t0,
        );
        return;
    }

    // ---------- 2) 等它退出 ----------
    note(
        &mut logs,
        "OK",
        format!("检测到 {label} 正在运行，等待退出中…"),
    );
    set(|s| s.phase = "in-game".into());

    let deadline = Duration::from_secs(mins.saturating_mul(60));
    let mut still_running = true;
    while still_running && !canceled() && t0.elapsed() < deadline {
        std::thread::sleep(Duration::from_secs(POLL_GAME_SECS));
        still_running = any_running(&names);
        set(|s| s.elapsed_secs = t0.elapsed().as_secs());
    }

    if canceled() {
        note(&mut logs, "WARN", "等待已按你的要求结束。".into());
        finish("canceled", "等待已取消".into(), logs, t0);
        return;
    }

    let why = if still_running {
        format!("等了超过 {mins} 分钟游戏还在运行")
    } else {
        "游戏进程已退出".to_string()
    };

    // ---------- 3) 自动恢复 ----------
    note(&mut logs, "STEP", format!("{why}，开始自动恢复代理工具…"));
    set(|s| s.phase = "restoring".into());

    // 现场读一遍配置：你在等待期间改了配置（比如把 Proxifier 加回来）也能立刻生效
    let cfg = crate::config::load().config;
    let restored = actions::restore(&cfg, false);
    for l in &restored {
        state::log("OK", l);
        logs.push(l.clone());
    }
    let failed = restored.iter().any(|l| l.contains('✗'));
    let tail = if failed {
        "（有几步失败了，请查看上方 ✗ 行，手动补回）"
    } else {
        ""
    };
    note(&mut logs, "OK", format!("自动恢复结束{tail}"));

    finish("done", format!("{why} → 已自动恢复{tail}"), logs, t0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_paths_and_names() {
        assert_eq!(normalize(r"D:\Games\BF6\bf6.exe"), "bf6");
        assert_eq!(normalize("BF2042.EXE"), "BF2042");
        assert_eq!(normalize("  bf6  "), "bf6");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn idle_status_is_inactive() {
        // 没调用 start 之前应当是"没在等"
        assert!(!is_active());
        assert_eq!(status().phase, "idle");
    }

    #[test]
    fn cancel_without_start_is_noop() {
        assert!(!cancel());
    }

    #[test]
    fn start_rejects_empty_names() {
        let e = start(vec!["".into(), "   ".into()], 0);
        assert!(e.is_err());
    }

    /// 端到端：造一个假的"游戏"进程（把 ping.exe 复制成 eaac_test_game.exe，跑约 8 秒），
    /// 走完整条链：出现 → 等待 → 退出 → 自动恢复。
    ///
    /// 默认不跑（要花十几秒 + 会写日志），需要时手动：
    ///     cargo test -- --ignored --nocapture
    #[test]
    #[ignore = "正在手动跑中..."]
    fn end_to_end_watch_then_auto_restore() {
        // 本机要是还留着没恢复的 state.json，就别跑 —— 否则恢复那一步会动到你真实的服务
        if crate::state::read().is_some() {
            println!("跳过：本机有未恢复的 state.json（请先点「恢复代理工具」再跑此测试）");
            return;
        }

        let dir = std::env::temp_dir().join("fuckeaac-test");
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("eaac_test_game.exe");
        std::fs::copy(r"C:\Windows\System32\ping.exe", &fake).expect("复制 ping.exe 失败");

        // 起"游戏"：ping 8 次 127.0.0.1，约 7 秒
        let mut game = {
            use std::os::windows::process::CommandExt;
            std::process::Command::new(&fake)
                .args(["-n", "8", "127.0.0.1"])
                .creation_flags(util::CREATE_NO_WINDOW)
                .spawn()
                .expect("拉起假游戏失败")
        };

        start(vec!["eaac_test_game".into()], 1).expect("start 失败");

        // 最多等 90 秒让它自己收尾
        let t0 = std::time::Instant::now();
        while is_active() && t0.elapsed() < Duration::from_secs(90) {
            std::thread::sleep(Duration::from_secs(1));
        }
        let st = status();
        println!("最终状态: {:?}", st);
        assert!(!st.active, "等待没有正常结束");
        assert_eq!(st.phase, "done", "阶段不对：{}", st.phase);
        assert!(
            st.logs.iter().any(|l| l.contains("已退出")),
            "日志里没有'游戏进程已退出'：{:?}",
            st.logs
        );
        assert!(
            st.logs.iter().any(|l| l.contains("开始自动恢复")),
            "未自动恢复：{:?}",
            st.logs
        );

        // 收尾：等假进程真的结束（顺便不留僵尸进程），再删掉临时 exe
        let _ = game.wait();
        let _ = std::fs::remove_file(&fake);
    }
}
