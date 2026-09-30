# Rust 入门 + 本工程代码导读

> ← 返回 [README](../README.md)　｜　其他文档：[代码审查指南.md](代码审查指南.md) · [编译与改代码.md](编译与改代码.md)

这份文档只教你两件事：**看懂这个项目的 Rust 代码**、**安全地改它**。
所有例子都从 `FuckEAAC-Tauri` 自己的源码里摘，不是网上的玩具代码。

> 阅读建议：不要从头背语法。先读 **第 0 章** 和 **第 2 章**（看懂一条链路），
> 想改东西时回来查 **第 4 章**（练习），遇到不认识的写法再翻 **第 1 章**。

---

## 目录

- [第 0 章 三条心法（最重要）](#第-0-章-三条心法最重要)
- [第 1 章 Rust 语法速成（用你的代码当例子）](#第-1-章-rust-语法速成用你的代码当例子)
- [第 2 章 用一条链路读懂整个工程（竖切阅读法）](#第-2-章-用一条链路读懂整个工程竖切阅读法)
- [第 3 章 如何阅读陌生代码（方法 + 工具 + 报错）](#第-3-章-如何阅读陌生代码方法--工具--报错)
- [第 4 章 如何安全地改代码（6 个练习，由易到难）](#第-4-章-如何安全地改代码6-个练习由易到难)
- [第 5 章 术语对照表](#第-5-章-术语对照表)

---

## 第 0 章 三条心法（最重要）

### 心法 1：编译器是你的老师，不是你的敌人

Rust 编译很严（很多错在别的语言里要跑起来才炸），但**它会把错在哪、期望什么、怎么改都写出来**。
所以你的开发循环永远是这五步，别跳：

```powershell
cd D:\DSH\Ccc\FuckEAAC-Tauri\src-tauri
cargo fmt          # 1. 自动排版（改完代码先格式化，省得纠结空格）
cargo check        # 2. 只检查能不能编过（几秒钟，比 build 快）
cargo clippy       # 3. 静态检查：指出"能跑但写法有问题"的地方
cargo test         # 4. 跑测试
cargo build        # 5. 真的生成 exe
```

**关键**：先 `cargo check`，别直接 `cargo build`。check 几秒，build 要一分钟。
报错不用怕，抄着 `help:` 那行改就行。

### 心法 2：读代码不是"逐行读"，是"追一条链路"

一个项目几千行，逐行读会睡着。正确做法是**挑一个你熟悉的功能，从界面一路追到系统调用**，
追完一遍，整个项目的结构就清楚了。第 2 章带你追两条。

### 心法 3：改代码要小步 + 可验证

一次只改一个地方 → 立刻 `cargo check` → 跑起来看 → 对了再改下一处。
改之前先想清楚"我怎么知道它对了"（看到某行日志？界面某处变了？测试通过？）。

---

## 第 1 章 Rust 语法速成（用你的代码当例子）

### 1.1 函数：`fn`

`src-tauri/src/commands.rs`：

```rust
/// 勾选/取消「演习模式」：只打印不执行
#[tauri::command]
pub fn set_dry_run(state: State<'_, AppState>, on: bool) -> serde_json::Value {
    *state.dry_run.lock().unwrap() = on;
    serde_json::json!({ "ok": true, "dryRun": on })
}
```

逐块拆：

| 片段 | 含义 |
|---|---|
| `/// 勾选/取消...` | 文档注释（`/` 三个）。`//` 两个是普通注释 |
| `#[tauri::command]` | 属性（attribute），告诉 Tauri"这个函数前端可以调用" |
| `pub fn` | `pub` = 公开（别的模块能用）；`fn` = 函数 |
| `set_dry_run` | 函数名，Rust 惯例：**小写下划线**（snake_case） |
| `state: State<'_, AppState>` | 参数：名字: 类型。`'_` 是生命周期占位符，先当"引用的存活范围"理解 |
| `-> serde_json::Value` | 返回值类型。没有 `->` 就是返回 `()`（空） |
| `*state...lock().unwrap() = on;` | 加 `*` = 解引用后赋值；末尾 `;` = 语句结束 |
| 最后一行没有 `;` | **最后一行不加分号 = 这就是返回值**（Rust 的重要习惯） |

### 1.2 变量与可变性

```rust
let x = 5;          // 不可变（默认）—— 想改它？编译器不让你改
let mut y = 5;      // mut = mutable，可变
y = 6;              // 合法
```

你在 `play.rs` 里见过：

```rust
let mut logs: Vec<String> = Vec::new();   // 后面要 push，所以必须 mut
let up_deadline = Duration::from_secs(APPEAR_MINUTES * 60);  // 不用改，就不要 mut
```

**习惯**：需要 mut 才写 mut。编译器会在你忘了写 mut 时提醒你（报错信息很直白）。

### 1.3 字符串：`String` 和 `&str` 的区别（最容易懵的地方）

- `String` = **拥有**这段文字的字符串（能改、能增长，存在堆上）
- `&str` = **借用**一段文字（只读，不拥有）

`src-tauri/src/util.rs` 的 `run` 函数签名：

```rust
pub fn run(program: &str, args: &[&str]) -> (i32, String, String)
```

读法：
- `program: &str` —— 传进来的是"一段文字"，我不拥有它，也不改它
- `args: &[&str]` —— 一组"文字"的切片（`&[T]` = 数组的借用）
- 返回 `(i32, String, String)` —— **元组**：退出码、stdout、stderr

调用它的时候：

```rust
let (code, out, err) = util::run("sc.exe", &["stop", &s.name]);
```

`let (a, b, c) = ...` 是**解构**：把元组里的三个值分别取出来。`&["stop", &s.name]` 就是造一个借用数组。

**互相转换**（记这两句就够）：

```rust
let owned: String = "文字".to_string();   // &str → String
let borrowed: &str = &owned;              // String → &str
let s2 = owned.clone();                   // String → String（复制一份）
```

你在 `config.rs` 里会看到 `.into()`，它是"转成目标类型"的简写：

```rust
vpn_adapter_pattern: "tun|tap|wintun|clash|mihomo|proxifier|utun".into(),
//                     ↑ &str              .into() 自动变成 String（因为字段要 String）
```

`.into()` 和 `.to_string()` 在这里效果一样；`.into()` 更短，但需要编译器能推断出目标类型。

### 1.4 数字类型

```rust
pub struct PlayStatus {
    pub elapsed_secs: u64,   // u64 = 无符号 64 位整数（0 ~ 很大）
}

const APPEAR_MINUTES: u64 = 30;          // 常量，全大写
const POLL_GAME_SECS: u64 = 10;
```

常见几种：

| 类型 | 范围 | 什么时候用 |
|---|---|---|
| `i32` | ±21 亿 | 默认整数，一般用它 |
| `u32` | 0 ~ 42 亿 | 窗口宽高、非负计数 |
| `u64` | 0 ~ 极大 | 秒数、字节数 |
| `usize` | 跟平台有关 | 数组下标、长度、`Vec` 的索引 |
| `f64` | 小数 | 浮点 |

**报错 `expected u64, found i32`** → 就是数字类型对不上，加个 `as u64` 或改类型。

### 1.5 数组、切片、`Vec`

```rust
let paths: &[&str] = &[ r"%ProgramFiles%\Proxifier\Proxifier.exe" ];  // 固定长度，借用
let names: Vec<String> = Vec::new();                                   // 可变长，拥有
names.push("bf6".to_string());                                         // 加元素
let first = names[0];                                                  // 按下标取（越界会 panic！）
```

`r"..."` 是**原始字符串**：里面的 `\` 不当转义符，所以 Windows 路径写起来不用双斜杠。

### 1.6 `struct`：把一组字段打包

`src-tauri/src/state.rs`：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoppedItem {
    pub kind: String,          // "service" | "process"
    pub name: String,          // 服务名 / 进程名
    pub was_running: bool,     // 当时是否在运行
    pub start_type: String,    // 服务原始启动类型（如有）
    pub target: String,        // 属于哪个目标（显示用）
}
```

创建它（`actions.rs` 里）：

```rust
StoppedItem {
    kind: "service".to_string(),
    name: s.name.clone(),
    was_running: s.was_running,
    start_type,
    target: s.target.clone(),
}
```

字段名和变量名一样时可以简写（`start_type,` 就等于 `start_type: start_type,`）。

`#[derive(...)]` 是"自动生成代码"的意思：

| 写的东西 | 自动得到什么 |
|---|---|
| `Debug` | 能用 `{:?}` 打印出来看 |
| `Clone` | 能 `.clone()` 复制 |
| `Serialize, Deserialize` | 能转成/读自 JSON（serde 提供） |
| `Default` | 能用 `StoppedItem::default()` 造一个"全空"的 |
| `Copy` | 赋值时直接复制而不是"移动"（只适合小类型，如 enum） |

### 1.7 `enum` + `match`：Rust 最舒服的地方

`src-tauri/src/config.rs`：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Action {
    Stop,                 // 进游戏模式时停掉它
    #[default]
    Warn,                 // 只提醒（默认）
    Auto,                 // 检测到 TUN 网卡才停
}
```

`enum` = "只能是这几种之一"。用的时候配 `match`（`diag.rs` 里满屏都是）：

```rust
match n(&v, "SecureBoot") {
    1 => out.push(item("Secure Boot", "已开启", "ok", "BF6 硬性要求，已满足")),
    0 => out.push(item("Secure Boot", "未开启", "bad", "请在 BIOS/UEFI 里打开")),
    _ => out.push(item("Secure Boot", "读取失败", "info", "可能不是 UEFI 启动")),
}
```

- `match 值 { 模式 => 结果, ... }` 有点像 C 的 `switch`，但**更强**：可以匹配结构、带条件、还能返回值
- `_` = 兜底（其它所有情况）
- 每个分支要么都带 `,`（当表达式用），要么都用 `{}` 块
- **穷举检查**：`match` 一个 enum 时如果漏了一种，编译器直接报错 —— 这是 Rust 帮你防 bug 的典型

还有一个你天天见的：

```rust
if let WindowEvent::CloseRequested { api, .. } = event {
    // 只在"事件是 CloseRequested"时才进来，并把里面的 api 取出来
}
```

`if let 模式 = 值` = "如果形状匹配，就取出里面的东西"；`..` = "其余字段我不关心"。

### 1.8 `Option` 和 `Result`：Rust 没有 null，也没有异常

```rust
pub fn read() -> Option<State> {        // 要么 Some(状态)，要么 None（没有）
    let p = state_file();
    if !p.exists() {
        return None;
    }
    let txt = std::fs::read_to_string(p).ok()?;      // .ok() = Result → Option；? = 失败就提前返回 None
    serde_json::from_str::<State>(&txt).ok()
}
```

```rust
pub fn process_running(name: &str) -> bool {
    matches!(count_process(name), Some(n) if n > 0)   // matches! = 把 match 写成一句话
}
```

处理方式对照：

| 写法 | 含义 | 用在哪 |
|---|---|---|
| `match x { Some(v) => ..., None => ... }` | 老老实实两种都处理 | 想分别处理 |
| `if let Some(v) = x { ... }` | 有值才做事，没值就算了 | 常见 |
| `x.unwrap()` | **没有就 panic 崩掉** | 只在"绝不可能失败"时用 |
| `x.unwrap_or(1)` | 没有就用默认值 | 很常用（`count_same_process` 里就是） |
| `x?` | 失败就把失败**往上抛**给调用者 | 函数返回 Result/Option 时 |
| `x.ok()?` | Result → Option 再抛 | 上面 `read()` 里 |

> ⚠️ 你的项目里 `unwrap()` 出现在 `state.dry_run.lock().unwrap()` 这种地方 ——
> 那是"Mutex 中毒"才会失败的极端情况，属于可接受。但**自己写新代码时优先用 `unwrap_or` / `match`**，
> 别让一个意外把整个程序崩掉。

`Result` 长这样，左成功右失败：`Result<i32, String>` = 成功给个数字，失败给句错误说明。
本工程用 `String` 当错误类型（够用、好读，虽然不够精细）。

### 1.9 迭代器：`.iter().map().filter().collect()`

`config.rs` 里这一句值得单独讲：

```rust
relaunch_paths: paths.iter().map(|s| s.to_string()).collect(),
```

从里往外读：

1. `paths` 是 `&[&str]`（一串文字）
2. `.iter()` → 逐个借出来
3. `.map(|s| s.to_string())` → 每个都变成拥有所有权的 `String`
4. `.collect()` → 收成一个集合（这里目标是 `Vec<String>`，由字段类型推断）

`actions.rs` 里还有：

```rust
for t in cfg.targets.iter().filter(|t| touched.iter().any(|n| n == &t.name)) {
    ...
}
```

= "遍历所有目标，但要满足【被这次操作碰过】这个条件"。
`any(|n| n == &t.name)` = "里面只要有任意一个等于它就行"。

**记住几个常用的**：`map`（变换）、`filter`（筛选）、`find`（找第一个）、`any`（有没有）、
`all`（是不是全都）、`count`（数个数）、`collect`（收成集合）。

### 1.10 闭包：`|参数| 做什么`

```rust
// play.rs —— 传一个"怎么改"的小函数进去
fn set(f: impl FnOnce(&mut PlayStatus)) {
    if let Ok(mut g) = slot().lock() {
        f(&mut g);
    }
}

// 调用处：
set(|s| s.phase = "in-game".into());
```

`|s| s.phase = ...` 就是闭包（类似 JS 的 `(s) => s.phase = ...`）。
`impl FnOnce(&mut PlayStatus)` 意思是"接受一个能用一次的函数，它接受一个可变引用"。

在异步/事件代码里你经常看到：

```javascript
// main.js 里一样的东西（JS 的箭头函数）
window.__TAURI__.event.listen('app://close-requested', onCloseRequested);
```

### 1.11 所有权与借用（Rust 的招牌，也是最容易卡住的地方）

三条规则（先记住就能活）：

1. 每个值都有**一个**主人（变量）
2. 同一时刻，要么**多个只读借用**（`&x`），要么**一个可变借用**（`&mut x`），不能同时
3. 主人走了（作用域结束），值就被回收 —— 不需要手动 free，也没有 GC

你项目里的例子：

```rust
let cfg = state.config();                 // 拿到一份"克隆出来"的 Config（因为 config() 内部 .clone()）
let snap = detect::snapshot(&cfg, util::is_admin());   // &cfg = 只读借用，不交出所有权
let logs = actions::stop(&cfg, &snap, &plan, dry);     // 又是只读借用
```

```rust
// components.rs 里 AppState 的访问器
pub fn config(&self) -> Config {
    self.cfg.lock().unwrap().clone()      // 为什么要 clone？因为锁保护的数据不能借出去
}
```

**最常见的两个报错**：

| 报错 | 意思 | 怎么改 |
|---|---|---|
| `use of moved value: x` | 你把 x 交出去了（比如传给了函数），后面还想用 | 改成传 `&x`，或者先 `x.clone()` |
| `cannot borrow x as mutable` | 你没写 `let mut`，或者同一时刻有别的借用 | 加 `mut`；或者把借用拆开（先取值、再修改） |

### 1.12 模块、`use`、可见性

一个 `.rs` 文件就是一个模块；`main.rs` 里的 `mod` 声明把它接进工程：

```rust
mod actions;
mod commands;
mod config;
mod detect;
mod diag;
mod play;
mod state;
mod util;
```

用别的模块的东西：

```rust
use crate::actions;                 // crate = 本工程根
use crate::config::Config;          // 只导入一个类型
use std::sync::Mutex;               // 标准库的路径是 std::
```

可见性：`pub fn` / `pub struct` 才能被别的模块用；不写 `pub` = 模块私有。
你在 `play.rs` 里见过 `fn normalize(...)`（没有 pub）= 只给本文件用。

> **改代码时的对应关系**：报错 `unresolved import` / `cannot find value` 基本都是
> 路径写错（少 `crate::`）或者忘了在 main.rs 里 `mod xxx;`。

### 1.13 属性 `#[...]`：给编译器/框架的"说明书"

你在项目里会看到这几种，各有用途：

```rust
#![windows_subsystem = "windows"]        // 文件级：整个二进制不弹控制台窗口
#[derive(Debug, Clone)]                  // 自动实现 trait
#[serde(rename_all = "PascalCase")]      // JSON 字段名用大驼峰 ← 改字段名时最容易踩
#[tauri::command]                        // 前端可调用
#[cfg(windows)]                          // 只在 Windows 平台编译这段
#[allow(dead_code)]                      // 让编译器闭嘴（慎用，一般意味着该删）
#[test]                                  // 这是个测试函数
#[ignore = "手动跑"]                      // 测试默认跳过
```

### 1.14 线程与共享状态（`play.rs` 用的）

```rust
// 1) 全局状态：OnceLock = "第一次用时才初始化，之后一直存在"
fn slot() -> &'static Mutex<PlayStatus> {
    static S: OnceLock<Mutex<PlayStatus>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(PlayStatus { phase: "idle".into(), ..Default::default() }))
}

// 2) Mutex = 一把锁：同一时刻只有一个线程能改
fn set(f: impl FnOnce(&mut PlayStatus)) {
    if let Ok(mut g) = slot().lock() {     // lock() 拿锁（拿不到就跳过，不崩）
        f(&mut g);
    }
}

// 3) AtomicBool = 跨线程的开关，不用锁
static CANCEL: AtomicBool = AtomicBool::new(false);
CANCEL.store(true, Ordering::SeqCst);      // 置位
if CANCEL.load(Ordering::SeqCst) { ... }   // 读取

// 4) 开一个后台线程（不影响界面）
std::thread::spawn(move || run(names, mins, t0));
```

`move` = 把外面的变量**搬进**闭包（因为线程活得可能比函数长，必须拥有它们）。

### 1.15 宏：名字后面带 `!` 的

| 宏 | 作用 | 例 |
|---|---|---|
| `println!("hi {}", x)` | 打印一行 | `println!("{code}")` |
| `format!("{} - {}", a, b)` | 拼字符串 | `format!("sc start {name}")` |
| `vec![1, 2, 3]` | 造 Vec | `vec!["bf6".into()]` |
| `json!({ "ok": true })` | 造 JSON | 每个 command 的返回值 |
| `matches!(x, Some(n) if n > 0)` | 简写 match | `util.rs` |
| `write!` / `writeln!` | 往流里写 | `state.rs` 写日志 |

**宏不是函数**：它是在编译期"展开成代码"，所以能接受任意个数的参数、能检查字符串格式。

---

## 第 2 章 用一条链路读懂整个工程（竖切阅读法）

### 2.1 结构一览（就 9 个文件）

```
src-tauri/src/
├─ main.rs       入口：窗口、托盘、关窗拦截、注册前端能调用的命令
├─ commands.rs   前后端接口（前端只能通过这里做事）
├─ config.rs     配置结构 + 内嵌默认目标清单
├─ detect.rs     只读扫描：服务/驱动/进程/网卡/系统代理 → 快照
├─ actions.rs    唯一会"改系统"的地方：停用 / 恢复
├─ play.rs       游戏模式：后台线程等游戏进程，退出后自动恢复
├─ state.rs      state.json（停了什么）+ 日志文件
├─ util.rs       调系统命令、判管理员、提权、数进程
└─ diag.rs       环境诊断（Secure Boot / TPM / 测试模式…）
```

依赖方向（箭头 = "谁用谁"）：

```
main.rs ──注册──> commands.rs ──> actions.rs ──> util.rs ──> sc.exe / taskkill.exe
                      │                │                        tasklist.exe / explorer.exe
                      │                └──> state.rs (state.json / 日志)
                      ├──> detect.rs  (只读扫描)
                      ├──> config.rs  (配置模型)
                      ├──> diag.rs    (环境诊断)
                      └──> play.rs    (等游戏 → 自动恢复 → 回到 actions.rs)
```

### 2.2 前端能做什么？看 `main.rs` 的注册表

`main.rs` 里这一段就是**本程序全部能力的清单**（18 条）：

```rust
.invoke_handler(tauri::generate_handler![
    commands::app_info,        // 版本/权限/路径/启动日志
    commands::get_status,      // 采一次快照
    commands::get_config,      // 读配置
    commands::export_config,   // 写配置
    commands::rescan_config,   // 重新扫描生成配置
    commands::set_dry_run,     // 演习模式开关
    commands::stop_targets,    // ★ 停用
    commands::restore_targets, // ★ 恢复
    commands::relaunch_admin,  // 提权重启
    commands::open_path,       // 用默认程序打开文件
    commands::run_diag,        // 环境诊断
    commands::quit_app,        // 真退出
    commands::hide_window,     // 收进托盘
    commands::set_pending,     // 记住"待办动作"
    commands::take_pending,    // 取出待办动作
    commands::start_play,      // ★ 开始等游戏
    commands::play_status,     // 查等待状态
    commands::cancel_play      // 停止等待
])
```

> **这条清单就是安全边界**：前端（网页）能做的事只有这 18 件，多一件都没有。
> 审查这个程序安不安全，看这一段 + `actions.rs` 就够了。

### 2.3 竖切 A：点一下「进入游戏模式」，到界面刷出日志

| 步 | 文件 → 函数 | 发生了什么 |
|---|---|---|
| 1 | `src/main.js` → `bind()` | `$('btn-play').onclick = () => doStop(true)` 把按钮接上 |
| 2 | `main.js` → `doStop(true)` | 读 `chk-demand` 勾选状态；调 `ensureAdmin('stop', demand)` |
| 3 | `main.js` → `ensureAdmin` | 不是管理员？→ `invoke('set_pending', ...)` 记下待办 → 弹窗问 → `relaunch_admin` 提权重启 |
| 4 | `main.js` → `invoke('stop_targets', { driversOnDemand: demand })` | **跨语言边界**：JS 用 camelCase 传参 |
| 5 | `commands.rs` → `stop_targets(state, drivers_on_demand: bool)` | Tauri 自动把 `driversOnDemand` 转成 `drivers_on_demand`；先查权限 |
| 6 | `detect.rs` → `snapshot(&cfg, admin)` | 只读扫描：`sc query` 找服务/驱动、`tasklist` 找进程、PowerShell 查网卡/系统代理 |
| 7 | `actions.rs` → `build_plan(&cfg, &snap, on_demand)` | 算出"该停什么"：遍历目标 → 只留 `effective == "Stop"` 的 → 得到 `Plan { services, processes }` |
| 8 | `actions.rs` → `stop(&cfg, &snap, &plan, dry)` | **真的动手**：`taskkill /IM x.exe /F` → `sc stop 服务` →（可选）`sc config x start= demand`；把每一条都写进 `state.json` |
| 9 | 回到 `commands.rs` | 返回 `json!({ ok, logs, stopped, dryRun, snapshot })` |
| 10 | `main.js` → `logLines(r.logs)` | 逐行打印，并按关键字上色（`✓` 绿、`✗` 红、`准备/开始` 蓝…） |

**读懂这条链，你就读懂了 80% 的项目**。反过来，想加一个"新动作"（比如"只停 Proxifier"），
就是往这条链上插：命令 → 逻辑 → 日志 → 返回。

### 2.4 竖切 B：点 X 关窗，最后为什么"留在后台"（事件反向流动）

前面那条链是"前端叫后端做事"，这条是"后端通知前端"：

| 步 | 位置 | 代码 |
|---|---|---|
| 1 | 系统 | 你点了窗口的 ×，Windows 发出关闭请求 |
| 2 | `main.rs` → `on_window_event` | `if let WindowEvent::CloseRequested { api, .. } = event` → `api.prevent_close()`（先别真关！） |
| 3 | `main.rs` | `window.app_handle().emit("app://close-requested", ())` ← 往前端发一个自定义事件 |
| 4 | `main.js` → `bind()` | `window.__TAURI__.event.listen('app://close-requested', onCloseRequested)` 早就挂好了监听 |
| 5 | `main.js` → `onCloseRequested()` | 弹系统对话框：`plugin:dialog|message` + `buttons: 'YesNo'` |
| 6a | 选"是" | `invoke('quit_app')` → `commands.rs` → `app.exit(0)` |
| 6b | 选"否" | `invoke('hide_window')` → `commands.rs` → `w.webview_window("main").hide()` |

**这就是为什么"关窗程序还在跑"**：真正的关闭被第 2 步拦住了，只有第 6a 才是真退出。
你以后改关窗行为，就改这两个地方。

### 2.5 数据结构地图（谁定义、谁写、谁读）

| 结构 | 定义在 | 谁写 | 谁读 | 对应文件 |
|---|---|---|---|---|
| `Config` / `Target` / `Action` | `config.rs` | 扫描生成、用户手改 | `detect` / `actions` / `commands` | `FuckEAAC.config.json` |
| `Snapshot` / `TargetState` | `detect.rs` | `detect::snapshot()` | `build_plan`、界面 | 内存 |
| `Plan` | `actions.rs` | `build_plan()` | `stop()` | 内存 |
| `StoppedItem` / `State` | `state.rs` | `stop()` | `restore()` | `state.json` |
| `Pending` | `state.rs` | `set_pending` | `take_pending`（启动时） | `pending.json` |
| `PlayStatus` | `play.rs` | 等待线程 | `play_status` 命令 → 界面横幅 | 内存 |
| `CheckItem` | `diag.rs` | `run_checks()` | 界面诊断列表 | 内存 |
| `WindowGeom` | `util.rs` | 关窗/移动时 | 启动时 | `window.json` |

> 改字段名的时候，**这四个地方要一起改**：结构体定义、写它的地方、读它的地方、还有 JSON 里的旧数据。

### 2.6 两个"看不见的契约"（改代码最容易踩的坑）

**契约 1：前后端字段名大小写不一样**

| 数据 | Rust 侧字段 | JSON 里长什么样 | 为什么 |
|---|---|---|---|
| `Config` | `game_process_names` | `GameProcessNames` | 结构体上写了 `#[serde(rename_all = "PascalCase")]` |
| `Target` | `relaunch_paths` | `RelaunchPaths` | 同上 |
| `Pending` | `drivers_on_demand` | `driversOnDemand` | 结构体上写的是 `camelCase` |
| `PlayStatus` | `elapsed_secs` | `elapsedSecs` | 同上 |

所以前端拿配置要写 `cfg.GameProcessNames`，写等待状态要写 `st.elapsedSecs`。
**改字段名时，`rename_all` 决定了 JSON 长什么样，别忘了同步前端。**

**契约 2：命令必须注册**

新写一个 `#[tauri::command] pub fn foo()`，如果忘了加进 `generate_handler![...]`，
前端调用时会报 `command foo not found`。这是新手最常见的坑（我这个项目里也犯过，
靠 `cargo check` 的 "function is never used" 警告才发现的）。

---

## 第 3 章 如何阅读陌生代码（方法 + 工具 + 报错）

### 3.1 五步法

1. **找入口**：C/Java 找 `main`，Rust 二进制也找 `fn main()`（本项目在 `main.rs`）
2. **找能力清单**：找"注册表/路由/命令表"（本项目是 `generate_handler!`）
3. **挑一条链路追到底**：从界面按钮 → 命令 → 逻辑 → 系统调用（2.3 那样）
4. **记数据结构**：先搞清 5~8 个核心 struct 的字段含义，代码突然就好读了
5. **忽略细节**：第一遍跳过错误处理、日志、格式化；第二遍再看

### 3.2 实例：带你读一个陌生函数

`util.rs` 里这个：

```rust
pub fn count_process(name: &str) -> Option<usize> {
    let stem = stem_of(name);
    if stem.is_empty() {
        return Some(0);
    }
    let exe = format!("{stem}.exe");

    let (code, out, _) = run("tasklist.exe", &["/FI", &format!("IMAGENAME eq {exe}"), "/NH"]);
    if code == 0 {
        let needle = exe.to_lowercase();
        return Some(out.to_lowercase().lines().filter(|l| l.contains(&needle)).count());
    }

    let (code2, out2, _) = run_ps(&format!("@(Get-Process -Name '{}' -ErrorAction SilentlyContinue).Count", stem.replace('\'', "''")));
    if code2 == 0 {
        if let Ok(n) = out2.trim().parse::<usize>() {
            return Some(n);
        }
    }
    None
}
```

**读的顺序和心里想的话**：

1. 签名 `-> Option<usize>`：数出来的个数，可能数不到 → 所以是 Option
2. `stem_of(name)` → 先归一化（去掉 `.exe`）
3. `if stem.is_empty() { return Some(0) }` → 空名字就当 0 个
4. `format!("{stem}.exe")` → `{stem}` 是"把变量插进字符串"的简写（等于 JS 的模板字符串）
5. `let (code, out, _) = run(...)` → 跑 tasklist，`_` = "第三个返回值我不要"
6. 关键那句：`out.to_lowercase().lines().filter(含exe名).count()` → 一行一个进程，数有几行提到它
7. `if code == 0` 不成立（tasklist 被拒）→ 往下走 PowerShell 兜底
8. `.parse::<usize>()` → 把文字转成数字（JS 里相当于 `Number(x)`）
9. 全都失败 → `None`

**读完你应该能回答**：为什么不直接 `return 0` 而要 `Option`？
（因为"没运行"和"查不到"是两件不同的事，等待逻辑必须区分。）

这就是"读懂"的标准：**能说出每个分支为什么存在**，而不是"大概知道它在数进程"。

### 3.3 VSCode 里的高频操作（用 rust-analyzer 插件）

| 想干什么 | 快捷键 |
|---|---|
| 跳到定义 | `F12` |
| 看谁调用了它（找引用） | `Shift+F12` |
| 按名字搜符号（函数/结构体） | `Ctrl+T` |
| 全项目搜文本 | `Ctrl+Shift+F` |
| 看一个变量的类型 | 鼠标悬停 |
| 重命名符号（安全改名） | `F2` |
| 打开终端 | `` Ctrl+` `` |

终端里也可以用（比界面搜得快）：

```powershell
rg "fn build_plan" src-tauri\src          # 找函数定义
rg "stop_targets" -n                       # 找所有引用
rg "unwrap\(\)" src-tauri\src              # 找所有可能崩的地方
```

### 3.4 看懂报错（三段式）

```
error[E0599]: no method named `get_webview_window` found for struct `tauri::AppHandle<R>`
  --> src\commands.rs:86:26
   |
86 |     if let Some(w) = app.get_webview_window("main") {
   |                        ^^^^^^^^^^^^^^^^^^^^
   |
   = help: items from traits can only be used if the trait is in scope
   = help: the following trait is implemented but not in scope; perhaps add a `use` for it:
           `use tauri::Manager;`
```

- 第一段：**是什么错**（E0599 = 找不到这个方法）
- 中间：**在哪一行**（`commands.rs:86`），`^^^` 指出具体位置
- 最后：**怎么改**（`help:` 直接告诉你 `use tauri::Manager;`）—— 这个错就是我写 `hide_window` 时真踩过的

**本项目真出现过的错误，速查**：

| 报错 | 原因 | 解法 |
|---|---|---|
| `E0599 no method named X` | trait 没导入（比如 `Manager`、`Emitter`） | 在文件头 `use tauri::Manager;` |
| `E0425 cannot find value` | 模块路径写错 / 忘了 `mod` | 加 `crate::` 前缀；在 main.rs 里 `mod xxx;` |
| `E0061 this function takes N arguments` | 参数个数不对 | 对照函数签名补齐 |
| `mismatched types: expected String, found &str` | 少 `.to_string()` | 加 `.to_string()` 或 `.into()` |
| `use of moved value` | 值交出去后还想用 | 传 `&x` 或 `.clone()` |
| `cannot borrow as mutable` | 缺 `mut` | `let mut x` / 参数写 `&mut` |
| `warning: unused variable / never used` | 定义了没用 | 删掉，或加 `_` 前缀（真没用就该删） |
| `error: could not compile ... 拒绝访问` | exe 正在运行，覆盖不了 | 先关掉程序再 build |

### 3.5 几个"读起来像黑话"的写法

| 写法 | 大白话 |
|---|---|
| `x.into()` | 转成需要的类型 |
| `x.unwrap_or(1)` | 没有就用 1 |
| `matches!(x, Some(n) if n > 0)` | 是不是"有值且大于 0" |
| `let Some(x) = ... else { return; }` | 拿不到就提前返回（let-else） |
| `..Default::default()` | 其余字段用默认值（只写我关心的） |
| `if let Ok(mut g) = m.lock()` | 拿到锁才做事（拿不到就算了） |
| `impl FnOnce(&mut T)` | 接受一个"能用一次"的函数 |
| `&'static Mutex<T>` | 这块内存活到程序结束 |
| `#[cfg(windows)]` | 只在 Windows 下编译这段 |

---

## 第 4 章 如何安全地改代码（6 个练习，由易到难）

### 4.1 黄金循环（每次改完都跑）

```powershell
cd D:\DSH\Ccc\FuckEAAC-Tauri\src-tauri
cargo fmt && cargo check && cargo clippy --all-targets && cargo test
```

只有 `check` 通过、`clippy` 没新警告、`test` 全绿，才算改完。最后再 `pnpm tauri dev` 看效果。

### 4.2 改之前的三条习惯

1. **一次只改一件事**，改完立刻 check（一次改十个地方，报错了你不知道是谁的锅）
2. **改前先想"怎么验证"**：看到哪行日志？界面哪里变？哪个测试会挂？
3. **拿不准就先复制一份**：`Copy-Item src\play.rs src\play.rs.bak`，搞砸了覆盖回来

### 4.3 练习 1：改一个数值（最安全，5 分钟）

**目标**：把"等游戏出现"的超时从 30 分钟改成 60 分钟。

改 `src-tauri/src/play.rs` 最上面：

```rust
/// 等游戏进程出现的最长时间
const APPEAR_MINUTES: u64 = 30;   // ← 改成 60
```

**为什么能改**：它是常量，注释里写了用途，改动只影响这一个行为。
**验证**：`cargo check` → 跑起来进入游戏模式 → 界面横幅/日志里的等待逻辑用新值。
**坑**：别忘 `u64` 后缀那类数字类型（写 `60` 没问题，写 `60.0` 会类型错）。

### 4.4 练习 2：改界面文案（只碰 HTML/CSS/JS，安全）

**目标**：把顶栏副标题从 `正在FuckEAAC` 改成你想要的话。

改 `src/index.html`：

```html
<div class="subtitle">正在FuckEAAC</div>   <!-- ← 改这里 -->
```

**验证**：`pnpm tauri dev` 里按 `F5` 刷新即可看到（**前端改动不用重编 Rust**）。
**坑**：改完 HTML 元素 `id` 的话，`main.js` 里 `$('那个id')` 会变 `null` → 界面点不动。
所以**只改文字，别改 id**。

### 4.5 练习 3：加一个"要停用的目标"（改配置清单）

**目标**：让工具在进游戏模式时也停掉 `NetLimiter`。

改 `src-tauri/src/config.rs` 的 `default_config()`，在 `targets: vec![ ... ]` 里加一条：

```rust
t(
    "NetLimiter",              // 界面上显示的名字
    Action::Stop,              // Stop = 真的停它；Warn = 只提醒；Auto = 看有没有 TUN 网卡
    "netlimiter|nlsvc",        // 进程名匹配（支持 | 分隔多个，见 detect::pattern_hit）
    "netlimiter|nlsvc",        // 服务/驱动名匹配
    &[],                       // 恢复时要重开的客户端路径（可空）
    false,                     // 是否顺手把内核驱动改成按需加载
    "限速/防火墙类工具，可能影响反作弊通信",   // 界面上的说明
),
```

**注意**：本项目**首次运行会自动扫描生成配置**，已有 `FuckEAAC.config.json` 时不会用你这个新清单！
所以要么删掉配置文件（会重新扫描生成），要么直接改 `FuckEAAC.config.json`（推荐，不用重编）。
**验证**：跑一次 `detect::snapshot`（打开界面看检测结果表里有没有它）。
**坑**：`pattern_hit` 用的是"包含匹配"，写太短的词（比如 `nl`）会误伤别的软件名。

### 4.6 练习 4：加一条环境诊断检查（中等）

**目标**：诊断列表里多一条"Windows 版本是否够新"。

改 `src-tauri/src/diag.rs` 的 `run_checks()`，照着现有的加：

```rust
// 在第 61 行 `let v = system_items();` 之后，用同样的风格加一段：
match s(&v, "OsBuild").as_str() {          // s() = 从 JSON 里取字符串
    "" => out.push(item("系统版本", "读取失败", "info", "可能需要管理员权限")),
    b  => out.push(item("系统版本", b, "ok", "")),
}
```

**原理**：`system_items()` 早就把数据查回来了（一个 PowerShell 一次性查完，输出 JSON），
你要做的是"从 JSON 里取值 → 判好坏 → 塞进 CheckItem 列表"。
`item(名字, 值, 状态, 说明)` 四个参数，状态只能是 `"ok" | "warn" | "bad" | "info"`（前端按这个上色）。
**验证**：界面上点「环境诊断」，看列表多没多那一行。
**坑**：`n()` 取数字，`s()` 取字符串，别用错；取不到时 `n()` 返回 -999。

### 4.7 练习 5：加一个全新的前端命令（完整走一遍，最有收获）

**目标**：加一个命令 `say_hello`，前端调用后返回一句带时间的话。

**① Rust 侧**（`src-tauri/src/commands.rs` 末尾）：

```rust
/// 示例命令：返回一句问候（教你新命令的完整套路）
#[tauri::command]
pub fn say_hello(name: String) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "message": format!("你好 {name}，现在是 {}", now()),
        "time": now(),
    })
}
```

`now()` 是本文件已有的私有函数（返回时间字符串），所以不用额外 import。

**② 注册**（`src-tauri/src/main.rs` 的 `generate_handler![...]` 里加一行）：

```rust
            commands::cancel_play,
            commands::say_hello        // ← 加这一行（注意前面有逗号）
        ])
```

**③ 前端**（`src/index.html` 导航里加个按钮）：

```html
<button class="nav-item" id="btn-hello">示例：打招呼</button>
```

**④ 前端逻辑**（`src/main.js` 的 `bind()` 里加绑定）：

```javascript
  $('btn-hello').onclick = async () => {
    const r = await invoke('say_hello', { name: 'nfytzx' });   // 参数名 camelCase
    logAdd(r.message, 'OK');
  };
```

**⑤ 验证**：

```powershell
cd src-tauri; cargo check        # 先确认 Rust 编得过
cd ..; pnpm tauri dev            # 起来后点那个按钮
```

**这一步的收获**：你会体验到"前端 → 命令 → Rust → 返回 JSON → 前端渲染"的完整闭环，
以后想加任何功能都是这个套路。**最容易忘的就是第 ② 步**（不注册就报 `command not found`）。

### 4.8 练习 6：给已有函数写单元测试（学会"证明它是对的"）

**目标**：给 `play.rs` 的 `normalize()` 加一条测试（原有的测试在文件最底部 `mod tests`）。

```rust
    #[test]
    fn normalize_keeps_plain_name() {
        assert_eq!(normalize("bf6"), "bf6");          // 没有 .exe 也要能用
        assert_eq!(normalize(r"D:\a b\c.exe"), "c");  // 带空格的路径
    }
```

跑：

```powershell
cargo test                    # 应该 7 passed（原来 6 个 + 你新增的这 1 个）
cargo test -- --nocapture     # 想看 println 输出时加这个
```

（如果断言挂了，`cargo test` 会直接告诉你"左值 vs 右值"分别是什么，照着重写期望值就行。）

**为什么值得学**：你改了别人的函数，怎么知道没改坏？写个断言，一秒钟就能验证。
测试还能当文档看 —— 它写着"这个函数应该做什么"。

### 4.9 别碰 / 小心碰的东西

| 对象 | 为什么 |
|---|---|
| `src-tauri/Cargo.lock` | 依赖锁文件，手改会乱；要升级依赖用 `cargo update` |
| `src-tauri/target/` | 编译产物，几个 GB，跑 `cargo clean` 就行，别手动删里面的东西 |
| `.vs/`、`node_modules/` | 工具生成的缓存，删了会重建 |
| `tauri.conf.json` 的 `identifier` | 改它 = 换一个应用身份（安装/设置会"另起炉灶"） |
| `#[allow(dead_code)]` | 加它只是让编译器闭嘴，正经做法是把没用的代码删掉 |
| `unwrap()` | 新代码里尽量用 `unwrap_or` / `match`，否则一个意外就崩 |

### 4.10 改错了怎么退

- **编译不过**：`cargo check` 会拦住，exe 都不会更新 → 安全，改回去就行
- **逻辑改坏**：把你复制的那份 `.bak` 覆盖回来：
  `Copy-Item src\play.rs.bak src\play.rs -Force`
- **想让整个工程回到某个状态**：这个项目还没用 git 管理。想用的话：
  ```powershell
  cd D:\DSH\Ccc\FuckEAAC-Tauri
  git init
  git add .
  git commit -m "初始状态"      # .gitignore 已经写好，不会把 5 GB 缓存传上去
  ```
  以后每次改完 `git diff` 看改了什么、`git checkout -- 文件` 一键还原。

---

## 第 5 章 术语对照表

| Rust 术语 | 大白话 | 本工程里的例子 |
|---|---|---|
| `fn` | 函数 | `fn stop(...)` |
| `let` / `let mut` | 定义变量 / 可变变量 | `let mut logs = Vec::new()` |
| `struct` | 结构体（一组字段） | `StoppedItem` |
| `enum` | 枚举（几种之一） | `Action::Stop / Warn / Auto` |
| `impl` | 给类型添加方法 | `impl AppState { pub fn config(&self) }` |
| `trait` | 接口/能力（类似 interface） | `Emitter`（提供 `emit`）、`Manager`（提供 `get_webview_window`） |
| `match` | 分支匹配（switch 的加强版） | `match n(&v, "SecureBoot")` |
| `Option<T>` | 可能有、可能没有（替代 null） | `Option<State>` |
| `Result<T, E>` | 成功或失败 | `Result<Value, String>` |
| `?` | 失败就往上抛 | `serde_json::from_str(&txt).ok()?` |
| `unwrap()` | 拿不到就崩（慎用） | `lock().unwrap()` |
| 所有权 | 每个值只有一个主人 | `let cfg = state.config();` |
| 借用 `&` / `&mut` | 只读引用 / 可写引用 | `detect::snapshot(&cfg, ...)` |
| 生命周期 | 引用的有效范围（保证不会悬空） | `&'static Mutex<T>` |
| 泛型 `<T>` | 类型的占位符 | `Option<usize>` |
| 闭包 | 匿名函数 | `set(\|s\| s.phase = "in-game".into())` |
| 宏 `!` | 编译期展开的代码 | `json!({...})`、`vec![...]` |
| 属性 `#[...]` | 给编译器/框架的说明 | `#[tauri::command]` |
| crate | 一个编译单元 / 一个依赖包 | `tauri`、`serde_json` |
| 模块 `mod` / `use` | 文件的组织 / 引入 | `mod play;`、`use crate::state;` |
| `pub` | 公开（别的模块能用） | `pub fn run(...)` |
| `Mutex` | 互斥锁（一次一个线程改） | `Mutex<PlayStatus>` |
| `AtomicBool` | 跨线程的布尔开关 | `CANCEL` |
| `OnceLock` | 只初始化一次的全局 | `slot()` |
| `thread::spawn` | 开后台线程 | 等待游戏进程那条线程 |
| `derive` | 自动生成实现 | `#[derive(Debug, Clone)]` |

---

## 附录：最值得读的 6 段代码（按顺序读）

| 顺序 | 位置 | 读它能学到 |
|---|---|---|
| 1 | `main.rs` 的 `main()` | 程序怎么组装起来、注册了哪些命令 |
| 2 | `commands.rs` 的 `set_dry_run` / `app_info` | 最简单的一问一答（命令长什么样） |
| 3 | `util.rs` 的 `run` / `count_process` | 怎么调用系统命令、怎么区分"没有"和"查不到" |
| 4 | `actions.rs` 的 `build_plan` / `stop` / `restore` | 真正的业务逻辑 + 日志风格 |
| 5 | `config.rs` 的 `default_config` 里的 `t()` | 用辅助函数把一堆字面量写清楚 |
| 6 | `play.rs` 全文 | 线程 + 全局状态 + 状态机（本项目最"高级"的一处） |

配着 **第 2.3 节那条链路** 一起读，效果最好。
