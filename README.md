# FuckEAAC（Tauri 版 v2.0.0）

进游戏前**自动停掉本机的代理/网络类工具**，游戏退出后**自动恢复**。

它解决的问题：EAAC（EA AntiCheat）在检测到 Proxifier / Clash 这类工具时会拒绝启动游戏。
本工具在启动游戏前把这些工具停干净（服务、内核驱动、客户端进程），玩完再原样恢复。

> ⚠️ **三句实话（写清楚，别误会）**
> 1. **它不碰你游戏的反作弊** —— 不注入、不劫持、不伪造签名、不调优先级；
>    `EAAntiCheat` / `EasyAntiCheat` **从不**出现在它的目标清单里。
> 2. **它会改的东西只有四类**：结束进程（`taskkill`）、停服务/驱动（`sc stop`）、
>    改服务启动类型（`sc config ... start=`）、关掉 **Windows 系统代理**
>    （写 `HKCU\...\Internet Settings` 的 3 个值，原值记进 `state.json` 可还原，
>    也能用配置项 `DisableSystemProxy: false` 整个关掉）。逐条核对见
>    [docs/代码审查指南.md](docs/代码审查指南.md)。
> 3. **封号风险你自己承担**：在游戏里用任何网络工具都可能违反游戏 ToS；临时停掉别的平台的反作弊
>    （清单里那两条 `Hint` 目标）也同理 —— 只在启动 EA 游戏前做，别在玩那个平台的游戏时停它。

---

## 1. 它到底做了什么

| 阶段 | 动作 | 用的系统命令 |
|---|---|---|
| 扫描 | 看本机有哪些服务/驱动/进程/网卡命中 | `sc.exe query`、`tasklist.exe`、`netsh.exe`、PowerShell |
| 进入游戏模式 | 结束代理客户端进程 | `taskkill.exe /IM xxx.exe /F` |
| | 停掉命中的服务/内核驱动 | `sc.exe stop <名>` |
| | （可选）把内核驱动改成按需加载 | `sc.exe config <名> start= demand` |
| | **关掉 Windows 系统代理**（原值记下来，恢复时写回） | PowerShell 写 `HKCU\...\Internet Settings` |
| 等待游戏 | 每 5/10 秒查一次游戏进程在不在 | `tasklist.exe`（被挡时退回 PowerShell `Get-Process`） |
| 恢复 | 启动服务/驱动、还原启动类型 | `sc.exe start`、`sc.exe config <名> start= auto` |
| | 把代理客户端重新拉起来 | `explorer.exe <路径>`（以普通用户身份代开） |
| | 把系统代理写回原值 | PowerShell + `InternetSetOption` 通知立即生效 |

全部是 Windows 自带命令，**没有任何网络请求**。审查方法见 [docs/代码审查指南.md](docs/代码审查指南.md)。

**日志**：界面上看到的是中文摘要 + 每条系统命令的**原始输出**（英文原样，方便按报错去搜）；
完整日志在 `%ProgramData%\FuckEAAC\fuckeaac.log`，界面上点「打开日志」即可。每条命令都记了
命令行、退出码、stdout/stderr 和耗时，出问题时能查清"为什么没停"。

---

## 文档索引

| 文档 | 适合什么时候看 |
|---|---|
| **README.md**（本文件） | 第一次接触这个工具：它做什么、怎么用、配置怎么写 |
| [docs/代码审查指南.md](docs/代码审查指南.md) | 想确认它"到底干了什么"：文件地图、执行的全部命令、怎么自证不联网/不碰反作弊 |
| [docs/编译与改代码.md](docs/编译与改代码.md) | 想编译或改代码：VS / VSCode 里怎么操作、"想改 X 该动哪个文件" |
| [docs/Rust-入门与代码导读.md](docs/Rust-入门与代码导读.md) | 不懂 Rust：语法速成（用本工程源码当例子）+ 怎么读代码 + 6 个改代码练习 |

---

## 2. 使用流程

1. **以管理员身份运行**（停内核驱动必须有管理员权限）。
   没提权也不会崩：界面右上角会显示琥珀色的 `⚠ 普通权限 · 点此切换`，点一下就弹 UAC 切换，
   切换后刚刚被打断的操作会**自动继续**。
2. 首次运行会**扫描本机并生成配置**（只保留你机器上真有的目标），不会拿一份写死的清单糊弄你。
3. 点 **「进入游戏模式」** → 停用命中的东西（进程、服务/驱动），并且**顺手关掉 Windows 系统代理**
   （原值会记下来，恢复时写回）。日志里会逐条写出"为什么停/为什么不停"。
4. 你自己去开 EA App / 游戏（本工具**不会**替你启动 EA App 或游戏，避免和 EA App 的启动流程打架）。
5. 游戏一退出 → 自动恢复：客户端重开、服务/驱动启回来、启动类型还原、系统代理写回原值。
   界面上方一直有一条横幅显示"正在等待什么、等了多久"，随时可以点 **「停止等待并立即恢复」**。

几个细节：

- **点 X 关窗口会问一句**：弹窗告诉你"程序会留在右下角托盘继续运行（等待/自动恢复不中断）"，
  你可以选「留在后台」或「彻底退出」。想彻底退出也可以直接点左侧「退出」或托盘菜单的「退出」。
- **强杀也不丢**：状态记在 `%ProgramData%\FuckEAAC\state.json`，即使被任务管理器结束/断电，
  下次打开会提示"上次停了 N 项还没恢复"，点「恢复代理工具」即可还原。
- **演习模式**：「演习（只看不动）」会把所有停用/恢复变成只打印，第一次用建议先跑一遍看它准备动什么。
- **退出时会问一句**：如果还在等游戏，退出前会问你要不要先恢复代理工具。

---

## 3. 界面

- 左侧导航：进入游戏模式 / 仅停用代理 / 恢复代理工具 / 演习 / 刷新状态 / 重新扫描生成配置 / 打开配置文件 / 打开日志 / 退出
- 右上角：深色/浅色切换、环境诊断、权限徽标（普通权限时可点，用来切换管理员模式）
- 中间三张卡：本机状态（权限、系统代理、虚拟网卡）、本次会停用（Stop）、只提醒（Warn）
- 检测结果表：每个目标命中了哪些进程/服务、当前是否在跑、说明
- 运行日志：所有动作都有一行"我准备做什么/结果怎样"

---

## 4. 配置文件

`FuckEAAC.config.json`（开发运行时在工程根目录；打包后和 exe 同目录）。
JSON 结构与旧的 PowerShell 版**完全一致**，两边可以共用同一份配置。

```jsonc
{
  "VpnAdapterPattern": "tun|tap|wintun|clash|mihomo|proxifier|utun",
  "GameProcessNames": ["bf6", "BF2042", "FC25", "FC26"],  // 等待这些进程，谁先来都认
  "GameExe": "",
  "DisableSystemProxy": true,   // 进游戏模式时是否顺手关掉 Windows 系统代理（原值会记下来，恢复时写回）
  "Targets": [
    {
      "Name": "Proxifier",
      "Action": "Stop",                    // Stop=进游戏时停用 / Warn=只提醒 / Auto=检测到 TUN 或系统代理开着才停
      "ProcessPattern": "proxifier",
      "ServicePattern": "proxifier",
      "RelaunchPaths": ["%ProgramFiles(x86)%\\Proxifier\\Proxifier.exe"],  // 恢复时重开客户端
      "DriversOnDemand": true,
      "Note": "WFP 内核驱动 ProxifierDrv 为 AUTO_START"
    }
  ]
}
```

改完保存，下次启动生效（等待过程中改也生效 —— 自动恢复时会重新读一次配置）。

几个字段说明：

| 字段 | 作用 |
|---|---|
| `Action` | `Stop` 直接停；`Warn` 只提醒；**`Auto`** = 检测到 **TUN 网卡在线** 或 **系统代理开着** 时才停（给 Clash 系用）；**`Hint`** = **需你手动处理**：没在运行就只提醒，**正在运行就归到「需停用」栏，但工具不动手**，只提示你自己关闭或重启（给内核反作弊这类"停它风险大、多半也停不掉"的东西用） |
| `ProcessPattern` / `ServicePattern` | `\|` 分隔的关键字，**大小写无关的子串匹配**（写 `clash` 能命中 `Clash Party`） |
| `VpnAdapterPattern` | 什么网卡算"虚拟网卡"。命中的网卡会显示在界面上，也是 `Auto` 的触发条件之一 |
| `DisableSystemProxy` | 关掉它 = 本工具**完全不碰**系统代理设置（那你自己记得在 Clash 里关） |
| `RelaunchPaths` | 恢复时要重新拉起的客户端路径，支持 `%ENV%` |

---

## 5. 构建与运行

前置环境（本机已验证可用）：Visual Studio 2026（MSVC 14.51 + Windows SDK 10.0.26100）、
Rust stable-x86_64-pc-windows-msvc 1.98、Node + pnpm、WebView2 运行时。

```powershell
# 开发（改 Rust 自动重编；改 HTML/CSS/JS 在窗口里按 F5 刷新）
pnpm install
pnpm tauri dev

# 只编后端 exe → src-tauri\target\release\fuckeaac.exe（约 3.5 MB，已 LTO + strip）
cd src-tauri
cargo build --release

# 打安装包（NSIS）→ src-tauri\target\release\bundle\nsis\*.exe
# 首次会联网下载 NSIS 工具链
pnpm tauri build

# 代码质量（当前：fmt 无差异、clippy 0 warning）
cargo fmt
cargo fmt --check
cargo clippy --all-targets

# 测试
cargo test                                # 18 个单元测试
cargo test -- --ignored --nocapture        # 端到端：造个假游戏进程跑完整条等待链（约 12 秒）

# 清缓存（能腾出几个 GB；清了之后上面这些命令第一次会慢一些）
cargo clean
```

**现成能跑的 exe 在 `dist\FuckEAAC.exe`**（3.54 MB，免安装，双击即用；同目录还放了一份 `FuckEAAC.config.json`，
带着你的目标清单）。它只是构建产物的一份拷贝 —— 源码改动后重新 `cargo build --release`，
要更新 `dist\` 里的那份就再复制一次（也可以直接跑 `target\release\` 里的）。

> 上面这几条就是全部命令。仓库里**没有**.cmd 双击脚本 —— 那类脚本只对"本机开发"有意义，
> 对从 GitHub 下载代码的人没用，留着反而显得杂。

**用 Visual Studio 改代码/编译**：见 [docs/编译与改代码.md](docs/编译与改代码.md)（VS 没有 Rust 项目系统，
用「打开文件夹」+ 集成终端；想让 VS 的 F5 直接能启动，照那份文档里的 JSON 片段自己建 `.vs\launch.vs.json`
—— 原先手写的那两个 VS 配置已按需删除，不是每个人都需要）。

**没有控制台窗口**：二进制子系统是 GUI（`#![windows_subsystem = "windows"]`，PE Subsystem = 2），
双击、提权、切换管理员都不会闪黑框。日志写在 `%ProgramData%\FuckEAAC\fuckeaac.log`，
界面上点「打开日志」即可。

---

## 6. 文件结构

```
FuckEAAC-Tauri/
├─ README.md                    ← 唯一入口（含上面的文档索引）
├─ docs/                        参考文档（都不影响程序运行，不看也行）
│  ├─ 代码审查指南.md            文件地图 + 执行的全部命令 + 怎么自证不联网
│  ├─ 编译与改代码.md            VS / VSCode 里怎么编译、想改 X 该动哪个文件
│  └─ Rust-入门与代码导读.md      不懂 Rust 时从这份开始读
├─ dist/                        构建产物的一份拷贝（可随时删，重新编译就有）
│  ├─ FuckEAAC.exe              3.54 MB，免安装，双击即用
│  └─ FuckEAAC.config.json      它的配置（和根目录那份同源）
├─ src/                        前端（纯 HTML/CSS/JS，无打包器，withGlobalTauri）
│  ├─ index.html  styles.css  main.js
│  └─ icons/                   图标（前端目录里的这份是窗口/顶栏用的）
├─ src-tauri/
│  ├─ src/
│  │  ├─ main.rs               窗口、托盘保活、关窗拦截、命令注册
│  │  ├─ commands.rs           前后端接口（前端只能通过这些命令做事）
│  │  ├─ config.rs             配置模型 + 内嵌目标清单 + 首次运行扫描生成
│  │  ├─ detect.rs             服务/驱动/进程/虚拟网卡/系统代理 → 快照
│  │  ├─ actions.rs            停用 / 恢复（含重开客户端）
│  │  ├─ play.rs               游戏模式：后台等游戏进程，退出后自动恢复
│  │  ├─ state.rs              状态文件 + 日志
│  │  ├─ util.rs               调系统命令、判管理员、提权、窗口几何
│  │  └─ diag.rs               环境诊断（Secure Boot / TPM / 测试模式 / 冲突软件…）
│  ├─ icons/ capabilities/     打包用图标 / 权限声明
│  ├─ Cargo.toml               只有 4 个直接依赖：tauri / tauri-plugin-dialog / serde / serde_json
│  └─ tauri.conf.json
├─ .gitignore                   忽略 target/、node_modules、dist/*.exe、配置与日志等
├─ FuckEAAC.config.json         开发运行时用的配置（扫描本机生成）
└─ package.json

（`src-tauri/target/`、`node_modules/` 是缓存/依赖，可以清掉但会重建；
  `src-tauri/gen/schemas/` 是 Tauri 生成的权限 schema（**已提交**，构建时会自动更新））
```

---

## 7. 常见问题

**Q：切换管理员后会冒出两个窗口吗？**
A：不会。提权实例起来并**确认它在跑**之后，旧实例才退出，窗口位置/大小会带过去。
（确认靠 `tasklist` 数同名进程；万一数不到，宁可留着旧窗口也不让它凭空消失。）

**Q：点了「进入游戏模式」但一直没等到游戏？**
A：30 分钟没等到会停止等待，并在界面顶部挂一条琥珀色提醒 ——
**这时代理工具仍是停用状态**，玩完记得点「恢复代理工具」。

**Q：它说"这台机器上查不到进程列表"？**
A：说明 `tasklist` 被安全软件挡住了（PowerShell 兜底也不行）。这种情况下"等待"不可靠，
所以直接拒绝，建议改用「仅停用代理」+ 手动「恢复代理工具」。

**Q：恢复时客户端没起来？**
A：看日志里的 ✗ 行。客户端优先用 `explorer.exe` 以**普通用户身份**代开
（避免继承管理员权限导致"代理对所有用户生效"这类副作用），失败才直接启动。

**Q：为什么 `Cargo.lock` 里有 reqwest / hyper？**
A：那是 Tauri 为**其它平台/可选特性**登记的，本机 Windows 构建的依赖图里没有它
（`cargo tree -i reqwest` 输出为空，exe 里也搜不到相关字符串）。详见代码审查文档。

**Q：为什么我的配置里没有 Proxifier？点停用也没停它。**
A：首次运行是**扫描本机**生成配置，只保留「这台机器上真的装了」的目标 —— **没装 Proxifier 就不会有这一条**，
自然也不会去停它（这是设计如此，不是 bug）。确认装没装：

```powershell
Test-Path "$env:ProgramFiles\Proxifier"      # 或 ${env:ProgramFiles(x86)}\Proxifier
sc query ProxifierDrv                        # 装了的话能看到这个 WFP 内核驱动
```

装了之后点一次 **「重新扫描生成配置」** 就会出现（或手动往 `FuckEAAC.config.json` 的 `Targets` 里加）。
届时点「进入游戏模式」，应用会依次执行：

```powershell
taskkill.exe /IM Proxifier.exe /F              # 关掉界面进程
sc.exe stop ProxifierDrv                       # 卸掉 WFP 内核驱动
sc.exe config ProxifierDrv start= demand       # 不再随开机进内核（配置里 DriversOnDemand: true）
```

恢复时反过来：`sc start ProxifierDrv` → 启动类型改回 `AUTO_START` → 重新拉起 `Proxifier.exe`。

想先确认这套命令在你机器上真能生效，**照下面三步自己跑一遍**（不需要额外脚本）：

```powershell
# ① 进程停止链路：造一个假进程（本体是 ping），再用应用同款的 taskkill 杀它
$d = "$env:TEMP\eaac-selftest"; New-Item -ItemType Directory $d -Force | Out-Null
Copy-Item "$env:SystemRoot\System32\ping.exe" "$d\eaac-selftest-proc.exe" -Force
Start-Process "$d\eaac-selftest-proc.exe" -ArgumentList '-n','60','127.0.0.1' -WindowStyle Hidden
Start-Sleep 2
taskkill.exe /IM eaac-selftest-proc.exe /F          # ← 成功会打印「成功: 已终止进程」
Get-Process eaac-selftest-proc -ErrorAction SilentlyContinue   # ← 应无输出
Remove-Item $d -Recurse -Force

# ② 服务停止 + 改启动类型（拿 Wireshark 的 npcap 驱动当靶子，做完会自动还原）
sc.exe qc npcap                                     # 记下原始 START_TYPE
sc.exe stop npcap ; Start-Sleep 3 ; sc.exe query npcap
sc.exe config npcap start= demand ; sc.exe qc npcap # 应该变成 DEMAND_START
sc.exe config npcap start= auto ; sc.exe start npcap  # 还原（原类型不是 auto 就按记下的改）

# ③ Windows 系统代理开关（应用会写的就是这三个值）
Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings' |
  Select-Object ProxyEnable, ProxyServer, AutoConfigURL
```

> ⚠️ 别把假进程命名成 `Proxifier.exe` 之类"知名程序"的名字：那样极容易被 Windows Defender
> 判成「伪装已知程序 + 结束进程」而直接隔离（我就踩过：一个自检脚本因此从磁盘上消失了）。
> `taskkill` 是按**映像名**匹配的，叫什么名字走的都是同一条代码路径，用中性名字即可。

**Q：EAAC 和我另一个游戏平台的反作弊（腾讯 ACE / 完美平台）冲突吗？**
A：**同类内核反作弊之间确实会互相拦**，这一点有据可查：有《星际战士2》玩家被 EAC（小蓝熊）直接要求
「关闭 ACE-BASE」；也有专门的工具 `anticheattoggle` 就是用来临时停掉 **腾讯 ACE / 完美平台 / Reason** 反作弊的。
EAAC 侧的典型报错是 **incompatible driver / Security Violation**。

在你机器上我实测到的结论：

```
腾讯 ACE（Anti-Cheat Expert）  已安装  C:\Program Files\AntiCheatExpert\
  ACE-BASE / ACE-GAME / ACE-ADVT / ACE-SSC-DRV64 / ACE-CORE*.sys
  ★ 全部 STOPPED，且启动类型 = DEMAND_START（按需）→ 平时不占内核，不冲突
  客户端进程：SGuard64.exe / SGuardSvc64.exe / SGuardUpdate64.exe
完美平台（Perfect World Arena）  未安装（全盘无痕迹）→ 无法实测
EAAntiCheat / EasyAntiCheat_EOS  服务已注册但驱动文件缺失（残留）
```

也就是说：**冲突只在「你刚玩过腾讯系游戏、ACE 驱动还驻留内核」之后去启动 EA 游戏时才会发生**。
判断方法：EA 游戏报错时 `sc query ACE-BASE` 看是不是 `RUNNING`；是的话重启电脑最干净，
或者在配置里把这条目标从 `Warn` 改成 `Stop`，让「进入游戏模式」替你停掉它（恢复时会按原启动类型启回来）。

工具里已经预置了两条**别的反作弊**目标，用的是一种专门的动作类型 **`Hint`（需你手动处理）**：

| 目标 | Action | 没在运行时 | 正在运行时 |
|---|---|---|---|
| **腾讯 ACE 反作弊**（Anti-Cheat Expert） | **`Hint`** | 落在「**仅提醒**」栏（提示：装了，但现在没威胁） | 落在「**需停用**」栏，标注**「需你手动处理」**；日志里给出具体清单 + 让你**自己关掉或重启电脑**。**工具不会 taskkill / sc stop** |
| 完美平台反作弊（Perfect World Arena） | `Hint` | 同上 | 同上 |

**完美平台**这条现在用的是**实际安装内容**（不是猜的）：

```
内核驱动/服务   MessageTransfer.sys            ← 反作弊核心，**开机自启**；蓝屏、与 VirtualBox/eNSP 冲突的元凶
                PerfectWorldAntiCheatSys.sys   ← Perfect World Co. Ltd 的反作弊系统驱动
                PerfectProtector.sys           ← 驱动保护，防外挂读写游戏进程
主进程          完美世界竞技平台.exe            ← winget 包 ID：PerfectWorld.PerfectWorldArena
插件 DLL        platform\plugin\PvpAlive.dll、csgopluginbase.dll  ← DLL 不是进程，本工具不检测 DLL
```

> 顺带修了一个**真 bug**：这些名字里有**中文**（`完美世界竞技平台.exe`），而进程列表来自 `tasklist` ——
> 它在中文系统输出 GBK，被我们按 UTF-8 解码会变成乱码，**中文进程名永远匹配不到**。
> 现在改成：发现输出里有解码坏掉的字符（U+FFFD）、或名字本身含非 ASCII 时，**自动改用 PowerShell 取进程名**（它被强制 UTF-8 输出）。

界面上长这样：

```
本机状态卡片：  本工具会自动停用  1 项      需你手动处理  1 项
本次会停用（Stop）卡片：
    Clash / mihomo 系客户端        运行中
    腾讯 ACE 反作弊（Anti-Cheat Expert）  运行中
    需你手动处理  腾讯 ACE 反作弊（工具不会动它：请自己关闭，或重启电脑）
```

点「进入游戏模式」时，日志里会出现这样一段（**只提示，不执行**）：

```
⚠ 以下组件正在运行，且**本工具按配置不会动它们**（需你手动处理）：
  - 腾讯 ACE 反作弊（Anti-Cheat Expert）：进程 sguard64；驱动/服务 ACE-BASE、ACE-GAME
  → 请先退出对应的游戏/平台客户端；内核驱动通常卸载不掉，**重启电脑最干净**。
  （想让它自动停：把配置里那条的 Action 改成 Stop）
```

ACE 这条在你这台机器上能匹配到 12 个服务/驱动（`ACE-BASE / ACE-GAME / ACE-ADVT / ACE-SSC-DRV64 / ACE-CORE×6 / ace-game-0 / AntiCheatExpert Service`）
和 3 个客户端进程（`SGuard64 / SGuardSvc64 / SGuardUpdate64`）。它们现在**全是 STOPPED、`DEMAND_START`**，
所以平时不会跟 EAAC 冲突 —— 只有你刚玩过腾讯系游戏（三角洲/CF 等）、ACE 还没从内核卸下来时才会，那时这条就跳到「需停用」栏提醒你。

> **为什么不让工具自动停？** ① 内核反作弊驱动通常**拒绝 `sc stop`**（没标记成可停止），自动停大概率失败还刷一屏错误；
> ② 乱停别的平台的反作弊**可能让你在那边被封号**。所以判断权留在你手里，工具只负责"看清 + 大声提醒"。
> 真想让工具上手：把配置里那条的 `"Action": "Hint"` 改成 `"Stop"`（它会杀进程 + `sc stop` 驱动，恢复时按原状态启回来）。
> 另外：程序**从不**碰 EAAC/EAC 自己（`EAAntiCheat`、`EasyAntiCheat` 都不在目标清单里，已核对）。

**Q：点了「停用代理」，Clash 确实关了，但 Windows 里"系统代理"开关还是开着的？**
A：这是 v2.0 早期版本的真实 bug，**已修**：现在停用时会把系统代理一起关掉
（`ProxyEnable=0`，有 PAC 也一并清掉），并把原值记进 `state.json`；恢复时写回。
日志里能看到这几行：

```
系统代理：当前开着（ProxyEnable=1 / ProxyServer='127.0.0.1:7890' ...）→ 关掉它，并记下原值以便恢复
    → exit=0  |  ProxyEnable=0 ProxyServer=127.0.0.1:7890 AutoConfigURL=
    ✓ 系统代理已关闭（恢复时会写回原值）
```

如果你**不想**让它动系统代理，把配置里 `"DisableSystemProxy"` 改成 `false`。

**Q：Clash 开了「系统代理」或「TUN 模式」，为什么没被停？**
A：两个原因都已修：① `Auto` 原来只认 TUN 网卡，现在**系统代理开着也算**；
② 网卡检测原来依赖 WMI（`Get-NetAdapter` + `Get-CimInstance`），在部分加固环境里会全部失败，
现在改成三级兜底 `Get-NetAdapter → netsh → 纯 .NET`，并排除 `Teredo Tunneling` 这类伪接口（以前会被误判成 TUN）。
如果还是没停，请把日志（`%ProgramData%\FuckEAAC\fuckeaac.log`）发我，里面现在会写清楚
"哪些网卡被认成虚拟网卡、系统代理的值是多少、每条目标为什么 Stop / 为什么只 Warn"。

**Q：界面点了按钮没反应、像卡住了？**
A：所有系统命令都有 **20 秒超时**，卡住会被强制结束并记进日志（不会永久卡死）。
如果真卡住过，日志里会出现"命令超过 20 秒没有返回，已强制结束"，把那行发我。

---

## 8. 发布到 GitHub

仓库里已经准备好：`.gitignore`（排除 `target/`、`node_modules/`、`.vs/`、运行配置与日志）、
`dist/`（**故意提交**的现成 exe，让下载的人不装 Rust 也能直接跑）、以及 `docs/` 下三份说明。

```powershell
cd D:\DSH\Ccc\FuckEAAC-Tauri
git init -b main
git add -A
git commit -m "FuckEAAC v2.0.0: Tauri + Rust 重写版"
git remote add origin https://github.com/<你的用户名>/FuckEAAC.git
git push -u origin main
```

推送前建议自查三件事：

```powershell
git ls-files | Measure-Object -Line            # 提交了多少个文件（本工程 37 个，正常）
git ls-files | Select-String 'target/|node_modules/' # 应为空 —— 缓存没被提交进去
git count-objects -vH                          # 仓库体积（含 dist 的 exe，约 1.9 MiB）
```

> ⚠️ 推送要用 GitHub 凭据。如果你之前配的 `gho_` token 已经失效，可能又弹 GCM 登录窗；
> 建议用 **PAT**（写进 `~/.git-credentials`，见 Git 常见问题）或改用 **SSH**。
> 另外：仓库是公开的话，记得先确认 `FuckEAAC.config.json`、日志这些**没有**被提交
> （它们已经在 `.gitignore` 里了，`git ls-files` 里查不到即可）。
>
> 关于 exe：现在是**直接提交** `dist/FuckEAAC.exe`（简单直接）。如果你更想保持仓库"纯源码"，
> 就把 `.gitignore` 里 `dist/*.exe` 那两行的注释去掉，改成每次发版在 GitHub **Releases** 里传附件。
