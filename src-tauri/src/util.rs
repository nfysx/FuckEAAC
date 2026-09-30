//! 通用工具：调用系统命令、判断管理员、路径。
//！ 尽量不引入第三方 crate，系统检测全部走 Windows 自带命令
//! （sc.exe / tasklist.exe / powershell.exe）。

use std::path::PathBuf;
use std::process::Command;

/// Windows 上启动子进程时不弹出黑框
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 运行一个命令并返回 `(退出码, stdout, stderr)`，**同时记一条命令日志**。
///
/// 参数：
/// - `program`：可执行文件，如 `"sc.exe"`（自动在 PATH / System32 里找）
/// - `args`：参数
///
/// 说明：本函数会等待命令结束（`cmd.output()`），所以只用来跑 sc / tasklist 这类
/// 很快返回的命令；要启动 GUI 程序（游戏、代理客户端）请用 `launch_as_user` / `launch_direct`。
///
/// 高频轮询（数进程、判管理员）请用 `run_quiet`，否则日志会被刷屏。
pub fn run(program: &str, args: &[&str]) -> (i32, String, String) {
    run_impl(program, args, true)
}

/// 同上，但不写命令日志（给每秒/每十几秒都会跑的自检用）
pub fn run_quiet(program: &str, args: &[&str]) -> (i32, String, String) {
    run_impl(program, args, false)
}

/// 单条外部命令最长等多久（秒）。超过就杀掉子进程并按失败返回。
///
/// 为什么必须要有：这些命令是在处理前端请求的线程里跑的，一旦某个命令卡住
/// （驱动卸载卡死、被安全软件拦截、管道建不起来……），整个界面就跟着卡死。
/// 正常命令都是几十毫秒级，20 秒足够宽裕。
const CMD_TIMEOUT_SECS: u64 = 20;

fn run_impl(program: &str, args: &[&str], log_it: bool) -> (i32, String, String) {
    use std::io::Read;
    use std::process::Stdio;

    let t0 = std::time::Instant::now();
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("启动 {program} 失败: {e}");
            if log_it {
                record_cmd(CmdRecord {
                    cmd: cmd_line(program, args),
                    exit: -1,
                    out: String::new(),
                    err: msg.clone(),
                    ms: t0.elapsed().as_millis(),
                });
            }
            return (-1, String::new(), msg);
        }
    };

    // 用两个线程把管道读空，避免子进程写满缓冲区后卡住
    let h_out = child.stdout.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = s.read_to_end(&mut buf);
            buf
        })
    });
    let h_err = child.stderr.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = s.read_to_end(&mut buf);
            buf
        })
    });

    // 轮询等待；超时就杀
    let deadline = t0 + std::time::Duration::from_secs(CMD_TIMEOUT_SECS);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    timed_out = true;
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            Err(_) => break None,
        }
    };

    // 超时：子进程已经杀掉，但**不能**去 join 那两个读管道的线程 ——
    // 管道有可能一直不关闭（子进程被拦、异常环境），join 会把我们重新卡住。
    // 让它们自生自灭即可（子进程已死，句柄很快释放）。
    if timed_out {
        let msg = format!("[fuckeaac] 命令超过 {CMD_TIMEOUT_SECS} 秒没有返回，已强制结束");
        if log_it {
            record_cmd(CmdRecord {
                cmd: cmd_line(program, args),
                exit: -2,
                out: String::new(),
                err: msg.clone(),
                ms: t0.elapsed().as_millis(),
            });
        }
        return (-2, String::new(), msg);
    }

    // 正常结束：这时管道已经 EOF，join 不会卡
    let so = String::from_utf8_lossy(
        &h_out
            .map(|h| h.join().unwrap_or_default())
            .unwrap_or_default(),
    )
    .to_string();
    let se = String::from_utf8_lossy(
        &h_err
            .map(|h| h.join().unwrap_or_default())
            .unwrap_or_default(),
    )
    .to_string();
    let code = status.and_then(|s| s.code()).unwrap_or(-1);

    if log_it {
        record_cmd(CmdRecord {
            cmd: cmd_line(program, args),
            exit: code,
            out: head_lines(&so, CMD_KEEP_LINES),
            err: head_lines(&se, CMD_KEEP_LINES),
            ms: t0.elapsed().as_millis(),
        });
    }
    (code, so, se)
}

// ---------------------------------------------------------------------------
// 命令日志：每次调外部命令都留一条痕，事后能查清"为什么没停 / 为什么报错"
// ---------------------------------------------------------------------------

/// 一条外部命令的执行记录
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CmdRecord {
    /// 命令行原文（英文，未翻译）
    pub cmd: String,
    pub exit: i32,
    /// 标准输出（最多 `CMD_KEEP_LINES` 行）
    pub out: String,
    /// 标准错误（最多 `CMD_KEEP_LINES` 行）
    pub err: String,
    pub ms: u128,
}

/// 缓冲区最多留多少条命令记录（等待游戏会跑很久，必须封顶）
const CMD_LOG_CAP: usize = 600;
/// 每条记录最多保留多少行输出
const CMD_KEEP_LINES: usize = 60;
/// 写进日志文件时，短输出最多附带多少行原文
const FILE_KEEP_LINES: usize = 12;

fn cmd_buf() -> &'static std::sync::Mutex<Vec<CmdRecord>> {
    static B: std::sync::OnceLock<std::sync::Mutex<Vec<CmdRecord>>> = std::sync::OnceLock::new();
    B.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

/// 命令行拼成一行（只做展示，不保证能原样粘贴执行）
fn cmd_line(program: &str, args: &[&str]) -> String {
    let mut s = program.to_string();
    for a in args {
        s.push(' ');
        if a.chars().count() > 160 {
            // PowerShell 脚本很长，截断展示（按字符截，避免切断中文/UTF-8）
            let head: String = a.chars().take(160).collect();
            s.push_str(&head);
            s.push_str(&format!("…[共 {} 字符]", a.chars().count()));
        } else {
            s.push_str(a);
        }
    }
    s
}

/// 取前 n 行（不足则原样返回）
fn head_lines(s: &str, n: usize) -> String {
    if s.lines().count() <= n {
        return s.to_string();
    }
    let mut out: Vec<&str> = s.lines().take(n).collect();
    let kept = out.len();
    let all = s.lines().count();
    out.push("");
    let joined = out.join("\n");
    format!("{joined}\n…[原始输出共 {all} 行，上面保留 {kept} 行]")
}

fn record_cmd(rec: CmdRecord) {
    // 1) 日志文件：先写一行摘要，输出短就把原文也写进去
    let n_out = rec.out.lines().count();
    let n_err = rec.err.lines().count();
    let mut summary = format!("{} → exit={}", rec.cmd, rec.exit);
    if n_out > 0 {
        summary.push_str(&format!(", stdout {n_out} 行"));
    }
    if n_err > 0 {
        summary.push_str(&format!(", stderr {n_err} 行"));
    }
    summary.push_str(&format!(", {} ms", rec.ms));
    crate::state::log("CMD", &summary);
    for (tag, text) in [("OUT", &rec.out), ("ERR", &rec.err)] {
        if text.trim().is_empty() || text.lines().count() > FILE_KEEP_LINES {
            continue;
        }
        for l in text.lines() {
            let l = l.trim_end();
            if !l.trim().is_empty() {
                crate::state::log(tag, l);
            }
        }
    }
    // 2) 内存缓冲：留给界面显示（封顶，避免长时间运行吃内存）
    if let Ok(mut b) = cmd_buf().lock() {
        if b.len() >= CMD_LOG_CAP {
            b.remove(0);
        }
        b.push(rec);
    }
}

/// 取走并清空命令日志（动作类命令用完就交给界面显示）
pub fn take_cmd_log() -> Vec<CmdRecord> {
    cmd_buf()
        .lock()
        .map(|mut b| std::mem::take(&mut *b))
        .unwrap_or_default()
}

/// 只清空不显示（"刷新状态"这种场景：原文已经写进日志文件了，不必往界面灌）
pub fn clear_cmd_log() {
    if let Ok(mut b) = cmd_buf().lock() {
        b.clear();
    }
}

/// 把命令记录格式化成界面日志行（保留系统英文原文，方便对照报错搜索）
pub fn cmd_log_lines(recs: &[CmdRecord]) -> Vec<String> {
    let mut out = Vec::new();
    for r in recs {
        out.push(format!("$ {}  → exit={}  {} ms", r.cmd, r.exit, r.ms));
        for l in r.out.lines() {
            let l = l.trim_end();
            if !l.trim().is_empty() {
                out.push(format!("    {l}"));
            }
        }
        for l in r.err.lines() {
            let l = l.trim_end();
            if !l.trim().is_empty() {
                out.push(format!("    [stderr] {l}"));
            }
        }
    }
    out
}

/// 运行一段 PowerShell 脚本，强制 UTF-8 输出（中文不乱码）
pub fn run_ps(script: &str) -> (i32, String, String) {
    // [Console]::OutputEncoding 让子进程按 UTF-8 输出；$ProgressPreference 防止进度条污染输出
    let wrapped = format!(
        "$ProgressPreference='SilentlyContinue'; [Console]::OutputEncoding=[System.Text.Encoding]::UTF8; {script}"
    );
    run(
        "powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &wrapped,
        ],
    )
}

/// 运行 PowerShell 并返回解析好的 JSON（失败返回 None）
pub fn run_ps_json(script: &str) -> Option<serde_json::Value> {
    let (code, out, _err) = run_ps(&format!("{script} | ConvertTo-Json -Compress -Depth 4"));
    if code != 0 || out.trim().is_empty() {
        return None;
    }
    serde_json::from_str(out.trim()).ok()
}

/// 同 `run_ps`，但不写命令日志（高频自检用）
pub fn run_ps_quiet(script: &str) -> (i32, String, String) {
    let wrapped = format!(
        "$ProgressPreference='SilentlyContinue'; [Console]::OutputEncoding=[System.Text.Encoding]::UTF8; {script}"
    );
    run_quiet(
        "powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &wrapped,
        ],
    )
}

/// 当前进程是否以管理员身份运行
///
/// 用 `net session` 的退出码判断：普通用户会返回 "拒绝访问"（非 0）。
/// 比解析 whoami 输出更稳、更快。
pub fn is_admin() -> bool {
    #[cfg(windows)]
    {
        let (code, _, _) = run_quiet("net.exe", &["session"]);
        code == 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// 以管理员身份重新启动本程序（会弹 UAC），成功返回 true
pub fn relaunch_elevated() -> bool {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let script = format!(
        "Start-Process -FilePath '{}' -Verb RunAs",
        exe.to_string_lossy().replace('\'', "''")
    );
    let (code, _, _) = run_ps(&script);
    code == 0
}

/// 当前有几个同名的本程序进程在跑（用来确认"提权实例是否真的起来了"）。
/// 实在查不到时返回 1（= 当作"只有一个"，宁可不关旧窗口，也不让窗口凭空消失）。
pub fn count_same_process() -> usize {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "fuckeaac.exe".into());
    count_process(&exe).unwrap_or(1).max(1)
}

/// 把 "bf6.exe" / "bf6" 归一成 "bf6"
fn stem_of(name: &str) -> String {
    let n = name.trim();
    let n = if n.to_lowercase().ends_with(".exe") {
        &n[..n.len() - 4]
    } else {
        n
    };
    n.trim().to_string()
}

/// 数一数某个进程现在有几个实例。
///
/// * `Some(0)`  = 确实没在运行
/// * `None`     = **查不到**（tasklist 被安全软件挡住、权限被拒，且 PowerShell 兜底也失败）
///
/// 先试 `tasklist.exe`（最快）；失败就退回 PowerShell 的 `Get-Process`
/// —— 有些受限/加固环境里 tasklist 会直接 “Access denied”，而 Get-Process 还能用。
pub fn count_process(name: &str) -> Option<usize> {
    let stem = stem_of(name);
    if stem.is_empty() {
        return Some(0);
    }

    // 名字里有非 ASCII（例如中文 exe 名「完美世界竞技平台」）：
    // tasklist 的 CSV/列表在中文系统是 GBK，我们按 UTF-8 解码会乱 → 直接走 PowerShell（强制 UTF-8）
    if !stem.is_ascii() {
        return count_process_ps(&stem);
    }

    let exe = format!("{stem}.exe");
    let (code, out, _) = run_quiet(
        "tasklist.exe",
        &["/FI", &format!("IMAGENAME eq {exe}"), "/NH"],
    );
    if code == 0 {
        // 正常时一行一个进程；没有匹配时 tasklist 只打印一句"没有运行的任务…"，不含进程名
        let needle = exe.to_lowercase();
        return Some(
            out.to_lowercase()
                .lines()
                .filter(|l| l.contains(&needle))
                .count(),
        );
    }

    count_process_ps(&stem)
}

/// 用 PowerShell 数进程（UTF-8 输出，中文名不会乱）
fn count_process_ps(stem: &str) -> Option<usize> {
    let (code, out, _) = run_ps_quiet(&format!(
        "@(Get-Process -Name '{}' -ErrorAction SilentlyContinue).Count",
        stem.replace('\'', "''")
    ));
    if code == 0 {
        if let Ok(n) = out.trim().parse::<usize>() {
            return Some(n);
        }
    }
    None
}

/// 某个进程（按映像名，如 "bf6" 或 "bf6.exe"）现在是否在运行。
/// 查不到进程列表时返回 false（要区分"没运行"和"查不到"，请直接用 `count_process`）。
pub fn process_running(name: &str) -> bool {
    matches!(count_process(name), Some(n) if n > 0)
}

/// 用 explorer.exe 以**普通用户身份**启动一个程序（不等待它退出）。
///
/// 为什么要绕这一下：本程序通常是提权运行的，直接 CreateProcess 出来的子进程**也是管理员**。
/// 代理客户端以管理员身份跑有时会有副作用（代理对所有用户生效、开机自启项错乱等）。
/// explorer.exe 跑在普通用户会话里，让它代开 = 拿到和你双击一样的权限。
/// 返回是否成功把请求交给了 explorer（不代表程序一定起来了，调用方自己确认）。
pub fn launch_as_user(path: &str) -> bool {
    let mut cmd = Command::new("explorer.exe");
    cmd.arg(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn().is_ok()
}

/// 直接启动一个程序（不等待退出）。留作 explorer 代开失败时的兜底。
pub fn launch_direct(path: &str) -> bool {
    Command::new(path).spawn().is_ok()
}

/// 等某个进程出现，最多 `secs` 秒；出现返回 true
pub fn wait_for_process(name: &str, secs: u64) -> bool {
    let mut left = secs;
    loop {
        if process_running(name) {
            return true;
        }
        if left == 0 {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        left -= 1;
    }
}

/// 展开 Windows 风格的环境变量：%ProgramFiles(x86)%\X\y.exe
pub fn expand_env(path: &str) -> String {
    let mut out = String::new();
    let bytes: Vec<char> = path.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '%' {
            if let Some(end) = bytes[i + 1..].iter().position(|c| *c == '%') {
                let name: String = bytes[i + 1..i + 1 + end].iter().collect();
                if let Ok(v) = std::env::var(&name) {
                    out.push_str(&v);
                    i = i + end + 2;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// 窗口位置与大小（"切换到管理员模式"时用它把窗口"原地保留"）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct WindowGeom {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub maximized: bool,
}

fn window_file() -> std::path::PathBuf {
    state_dir().join("window.json")
}

pub fn save_window(g: &WindowGeom) {
    let dir = state_dir();
    ensure_dir(&dir);
    if let Ok(txt) = serde_json::to_string(g) {
        let _ = std::fs::write(window_file(), txt);
    }
}

pub fn load_window() -> Option<WindowGeom> {
    let txt = std::fs::read_to_string(window_file()).ok()?;
    serde_json::from_str::<WindowGeom>(&txt).ok()
}

/// %ProgramData%\FuckEAAC （状态文件与日志；与 PowerShell 版本保持一致，方便互相读）
pub fn state_dir() -> PathBuf {
    let base = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into());
    PathBuf::from(base).join("FuckEAAC")
}

/// 程序所在目录（配置文件放这里，和 PS 版一致）
///
/// 注意：开发运行（cargo run / tauri dev）时 exe 在 `target\debug\`，
/// 把配置丢在那儿很别扭，所以调试版改用工程目录（CARGO_MANIFEST_DIR 的上一级）。
pub fn app_dir() -> PathBuf {
    if cfg!(debug_assertions) {
        if let Some(m) = option_env!("CARGO_MANIFEST_DIR") {
            if let Some(parent) = std::path::Path::new(m).parent() {
                return parent.to_path_buf();
            }
        }
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 确保目录存在，忽略错误
pub fn ensure_dir(p: &std::path::Path) {
    let _ = std::fs::create_dir_all(p);
}

/// 现在时间（"yyyy-MM-dd HH:mm:ss"），不需要 chrono：直接用 PowerShell 太重，
/// 这里用系统时间戳自己拼（本地时区用 UTC 偏差修正）。
pub fn now_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // 本地时区偏移：从环境变量拿不到，简单用 PowerShell 只这一次（缓存）
    let offset = local_utc_offset_seconds();
    let t = secs + offset;
    let days = t.div_euclid(86_400);
    let rem = t.rem_euclid(86_400);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

/// 本地时区相对 UTC 的偏移秒数（只查一次并缓存）。
///
/// ★ 这里必须用 `run_ps_quiet`（**不写命令日志**）：
///   调用链是 `run → record_cmd → state::log → now_string → 本函数`，
///   如果本函数再去跑一条"会记日志"的命令，就会重入 `OnceLock::get_or_init`
///   —— 同一线程等自己初始化，直接死锁。整个程序会卡死在第一条日志上。
fn local_utc_offset_seconds() -> i64 {
    use std::sync::OnceLock;
    static OFF: OnceLock<i64> = OnceLock::new();
    *OFF.get_or_init(|| {
        // GetTimeZoneInformation 太啰嗦；用 PowerShell 问一次即可（启动时一次，开销可忽略）
        // 注意：必须是 quiet 版本，否则会和自己正在初始化的日志路径重入（死锁）
        let (code, out, _) = run_ps_quiet(
            "(Get-Date) - (Get-Date).ToUniversalTime() | Select-Object -ExpandProperty TotalSeconds",
        );
        if code == 0 {
            if let Ok(v) = out.trim().parse::<f64>() {
                return v as i64;
            }
        }
        0
    })
}

/// 把"从 1970-01-01 起的天数"换算成 (年, 月, 日) —— 标准 civil_from_days 算法
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 时区换算：验证几个已知日期（不依赖系统时区）
    #[test]
    fn civil_from_days_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
    }

    /// 命令行拼接会截断超长参数（PowerShell 脚本那种），且不会切断中文
    #[test]
    fn cmd_line_truncates_long_args() {
        let long = "啊".repeat(500);
        let s = cmd_line("powershell.exe", &[&long]);
        assert!(s.contains("共 500 字符"), "没有截断提示：{s}");
        assert!(s.chars().count() < 400);
    }

    /// 取前 N 行：多了要截断并注明
    #[test]
    fn head_lines_truncates() {
        let text = (1..=100)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let h = head_lines(&text, 5);
        // 注意：空串的 `chars().all(..)` 也返回 true，所以要先把空行排除掉
        let digits = h
            .lines()
            .filter(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_digit()))
            .count();
        assert_eq!(digits, 5, "实际内容：{h}");
        assert!(h.contains("共 100 行"));
    }

    /// 冒烟测试：**命令日志不能和时区初始化互相重入**。
    ///
    /// 这条链路是 `run → record_cmd → state::log → now_string → local_utc_offset_seconds`。
    /// 如果 `local_utc_offset_seconds` 里跑的是会记日志的命令，`OnceLock` 会自我重入并死锁
    /// —— 症状是程序在第一条日志处彻底卡死。这条测试跑完（而不是卡住）就说明没问题。
    #[test]
    fn command_logging_does_not_deadlock() {
        let (code, out, _) = run("cmd.exe", &["/c", "echo", "fuckeaac"]);
        assert_eq!(code, 0);
        assert!(out.to_lowercase().contains("fuckeaac"));
        // 能拿到时间字符串（内部会用到缓存过的时区偏移）
        assert!(now_string().len() >= 19);
    }
}
