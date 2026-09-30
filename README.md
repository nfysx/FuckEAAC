# FuckEAAC

进游戏前**自动停掉本机的代理 / 网络类工具**，退出游戏后**自动恢复**。

EA 系游戏（战地 6、FC 系列……）由 **EAAC（EA AntiCheat）** 保护，它检测到 Proxifier、Clash 这类
会改写网络流量的工具时，可能直接拒绝启动游戏。FuckEAAC 做的事很直接：**启动游戏之前把这些工具停干净
（客户端进程、Windows 服务、内核驱动、系统代理开关），玩完再原样恢复**。

| | |
|---|---|
| 平台 | Windows 10 / 11（x64） |
| 形态 | 单文件绿色程序：`dist/FuckEAAC.exe`，约 3.5 MB，免安装、无控制台窗口 |
| 技术栈 | Tauri 2 + Rust 后端／纯 HTML + CSS + JS 前端（无打包器、无第三方 JS 库、无 CDN） |
| 依赖 | 运行时依赖只有 4 个：`tauri`、`tauri-plugin-dialog`、`serde`、`serde_json`（另加 1 个构建依赖 `tauri-build`） |
| 网络 | **不联网**：无 HTTP 客户端、无遥测、无自动更新（可自行核对，见下文"它会动什么、不会动什么"） |

---

## ⚠️ 免责声明（请先读）

- 本项目**不针对、不修改、不绕过任何反作弊**：不注入进程、不 HOOK、不伪造签名、不修改游戏文件、
  不调整优先级。`EAAntiCheat` / `EasyAntiCheat` 自身**从不**出现在它的目标清单里。
- 它会改动的东西**只有四类**，全部可逆、全部有日志：
  1. 结束进程（`taskkill /IM <名>.exe /F`）
  2. 停止 Windows 服务 / 内核驱动（`sc stop`）
  3. 修改服务启动类型（`sc config <名> start=`，恢复时改回原值）
  4. 关闭 **Windows 系统代理**（写 `HKCU\...\Internet Settings` 的 3 个值，原值记入 `state.json` 可还原；
     可用配置项 `DisableSystemProxy: false` 整个关掉）
- 在游戏中使用任何网络工具都可能违反游戏 ToS，**因此产生的封号等后果由使用者自行承担**。
- 清单里那两条"别的平台反作弊"（腾讯 ACE / 完美平台）是 `Hint` 类型：**工具只提醒、不自动动手**，
  而且只应在启动 EA 游戏前处理 —— 在玩那个平台的游戏时关掉它的反作弊属于违反该平台规则。

---

## 功能

| 功能 | 说明 |
|---|---|
| **进入游戏模式** | 结束命中的代理客户端 → 停掉命中的服务与内核驱动 →（可选）驱动改按需启动 → 关闭系统代理 |
| **自动等待 + 自动恢复** | 后台线程每 5/10 秒查一次游戏进程；游戏退出即恢复：重开客户端、启动服务/驱动、还原启动类型、写回系统代理原值 |
| **首次运行自动扫描** | 只保留**这台机器上真的装了**的目标，不预置一份写死的清单；随时可点「重新扫描生成配置」 |
| **权限自适应** | 普通权限也能开；需要提权时点右上角徽标，切换管理员后**刚才被打断的操作会自动继续** |
| **演习模式** | 所有停用/恢复只打印不执行，第一次用建议先跑一遍看它准备做什么 |
| **环境诊断** | Secure Boot、TPM、虚拟机、测试签名、VBS/HVCI、CPU/内存、冲突软件等 |
| **完整日志** | 每条外部命令的命令行、退出码、stdout/stderr、耗时都记录，附带"每条目标为什么停/为什么不停" |
| **托盘保活** | 关窗口不退出，等待与自动恢复在托盘里继续；退出前会问是否先恢复 |
| **命令超时** | 所有系统命令 20 秒超时，卡住会被强制结束并记日志，不会把界面拖死 |

---

## 下载与运行

1. 下载 `dist/FuckEAAC.exe`（仓库内直接可取，或在 Releases 里下载）。
2. 双击运行即可。**建议以管理员身份运行**：不提权也能开，但停不了内核驱动与服务
   （界面上会显示琥珀色 `⚠ 普通权限 · 点此切换`，点一下就能切换）。
3. 首次运行会扫描本机并生成 `FuckEAAC.config.json`（与 exe 同目录）。

> 配置与日志属于每台机器自己的数据，不在仓库里：
> 配置 `FuckEAAC.config.json`（与 exe 同目录），状态/日志在 `%ProgramData%\FuckEAAC\`。

---

## 使用流程

1. 打开 FuckEAAC → 点 **「进入游戏模式」**：
   结束代理客户端、停掉命中的服务/驱动、（默认）关闭 Windows 系统代理。
2. 自己启动 EA App / 游戏 —— 本工具**不会**替你启动 EA App 或游戏，避免与其启动流程打架。
3. 玩完退出游戏：程序检测到游戏进程退出后**自动恢复**（约 10 秒内）。
   界面上方一直显示"在等什么、等了多久"，也可以随时点 **「停止等待并立即恢复」**。
4. 只想停不想等：点 **「仅停用代理」**；恢复始终由 **「恢复代理工具」** 明确执行。

细节：

- **窗口可以直接关掉**：程序会留在右下角托盘继续等待/恢复；从托盘双击可重新打开。
- **强杀也不丢**：状态写在 `%ProgramData%\FuckEAAC\state.json`，即使被任务管理器结束或断电，
  下次打开仍会提示"上次停了 N 项"并支持一键还原。
- **第一次用建议先点「演习（只看不动）」**，确认它准备动的东西都是你愿意的。

---

## 从源码构建

环境：Windows + Visual Studio（MSVC 工具链 + Windows SDK）+ Rust stable（`x86_64-pc-windows-msvc`）
+ Node.js / pnpm + WebView2 运行时（Win10/11 通常自带）。

```powershell
pnpm install                 # 只装一个依赖：@tauri-apps/cli
pnpm tauri dev               # 开发模式（改 Rust 自动重编；改 HTML/CSS/JS 在窗口里按 F5）

cd src-tauri
cargo build --release        # 发布版 → src-tauri\target\release\fuckeaac.exe（约 3.5 MB）
cargo test                   # 18 个单元测试（另有 1 个端到端测试要手动跑，见下）
cargo clippy --all-targets   # 静态检查（当前 0 warning）
cargo fmt                    # 格式化
cargo test -- --ignored --nocapture   # 端到端：造一个假"游戏"进程，跑完 出现→等待→退出→自动恢复 整条链（约 12 秒）

pnpm tauri build             # 打 NSIS 安装包（首次需联网下载 NSIS 工具链）
cargo clean                  # 清缓存（可腾出几个 GB）
```

**用 IDE 开发**：Visual Studio / VS Code 都没有 Rust 项目系统 —— 用「打开文件夹」打开项目根目录，
编译调试都在集成终端里跑上面的命令（VS Code 装 `rust-analyzer` 扩展会有补全和实时报错）。
**想改 X 该动哪个文件**：界面文字 `src/index.html` + `src/styles.css` + `src/main.js`；
停用/恢复逻辑 `src-tauri/src/actions.rs`（**全项目只有它会改系统**）；检测 `src-tauri/src/detect.rs`；
等待游戏 `src-tauri/src/play.rs`；目标清单 `src-tauri/src/config.rs` 的 `default_config()`。

---

## 配置文件

`FuckEAAC.config.json`（与 exe 同目录；开发运行时在项目根目录）。

```jsonc
{
  "VpnAdapterPattern": "tun|tap|wintun|clash|mihomo|proxifier|utun",
  "GameProcessNames": ["bf6", "BF2042", "FC25", "FC26"],  // 等这些进程，谁先来都认
  "GameExe": "",
  "DisableSystemProxy": true,   // 进游戏模式时是否顺手关掉系统代理（原值可还原）
  "Targets": [
    {
      "Name": "Proxifier",
      "Action": "Stop",
      "ProcessPattern": "proxifier",
      "ServicePattern": "proxifier",
      "RelaunchPaths": ["%ProgramFiles(x86)%\\Proxifier\\Proxifier.exe"],
      "DriversOnDemand": true,
      "Note": "WFP 内核驱动 ProxifierDrv 为 AUTO_START"
    }
  ]
}
```

| 字段 | 说明 |
|---|---|
| `Action` | `Stop` 直接停；`Warn` 只提醒；`Auto` = 检测到 **TUN 网卡在线**或**系统代理开着**才停（给 Clash 系用）；`Hint` = **需你手动处理**：没跑就只提醒，**正在跑则归入「需停用」栏并提示你自己关闭或重启，工具不动手**（给内核反作弊这类"停它风险大、多半也停不掉"的东西用） |
| `ProcessPattern` / `ServicePattern` | `\|` 分隔的关键字，**大小写无关的子串匹配**（写 `clash` 能命中 `Clash Party`） |
| `VpnAdapterPattern` | 什么网卡算"虚拟网卡"（显示在界面上，也是 `Auto` 的触发条件之一） |
| `RelaunchPaths` | 恢复时要重新拉起的客户端路径，支持 `%ENV%` |
| `DisableSystemProxy` | 设为 `false` = 本工具完全不碰系统代理设置 |

改完保存，下次启动生效（等待期间改也生效：自动恢复时会重新读一次配置）。
内嵌的目标清单在 `src-tauri/src/config.rs` 的 `default_config()`，共 20 条
（代理类、抓包/调试类、别的平台反作弊类）。

---

## 它会动什么、不会动什么

| 只读（安全） | 会改（可逆、有日志） |
|---|---|
| `sc query` 枚举服务/驱动 | `taskkill /IM <名>.exe /F` 结束进程 |
| `tasklist` 枚举进程 | `sc stop <名>` 停服务/驱动 |
| `netsh` / PowerShell 枚举网卡、读系统代理 | `sc config <名> start=` 改启动类型（恢复时还原） |
| 注册表**读**系统代理设置 | 注册表**写**系统代理设置（仅 `HKCU\...\Internet Settings` 的 3 个值） |

**不做**：注册表其它任何位置、IFEO 映像劫持、进程注入、驱动安装/加载、优先级调整、文件替换、
访问游戏或反作弊目录、任何网络请求。

**自己核对的办法**（源码里直接搜，应该全部搜不到）：

```powershell
cd src-tauri
Select-String -Path src\*.rs -Pattern 'OpenProcess|WriteProcessMemory|CreateRemoteThread|SetWindowsHookEx|reg add|Image File Execution|SetPriorityClass|NtLoadDriver'
Select-String -Path src\*.rs,..\src\*.js,..\src\*.html -Pattern 'fetch\(|XMLHttpRequest|WebSocket|http://|https://'
cargo tree -i reqwest          # 输出 "nothing to print" = 没有 HTTP 客户端库被链进来
Select-String -Path src\*.rs -Pattern 'util::run\("([^"]+)"' -AllMatches | % { $_.Matches | % { $_.Groups[1].Value } } | Group-Object
#   最后一条会列出它执行的全部外部程序：sc.exe / taskkill.exe / tasklist.exe / netsh.exe / explorer.exe / powershell.exe

---

## 常见问题

**Q：配置里为什么没有 Proxifier？点了停用也没停它。**
A：首次运行是**扫描本机**生成配置，只保留"这台机器上真的装了"的目标 —— 没装就不会有这一条，
自然也不会去停它。确认装没装：

```powershell
Test-Path "$env:ProgramFiles\Proxifier"   # 或 ${env:ProgramFiles(x86)}\Proxifier
sc query ProxifierDrv                     # 装了的话能看到这个 WFP 内核驱动
```

装了之后点一次 **「重新扫描生成配置」** 就会出现（也可以手动往 `Targets` 里加）。
届时点「进入游戏模式」会依次执行：

```powershell
taskkill.exe /IM Proxifier.exe /F              # 关界面进程
sc.exe stop ProxifierDrv                       # 卸掉 WFP 内核驱动
sc.exe config ProxifierDrv start= demand       # 不再随开机进内核（配置里 DriversOnDemand: true）
```

恢复时反过来：`sc start ProxifierDrv` → 启动类型改回 `AUTO_START` → 重新拉起 `Proxifier.exe`。

**Q：点了「停用代理」，Clash 确实关了，但 Windows 的"系统代理"开关还开着？**
A：v2.0 早期版本确实有这个缺陷（旧版只处理进程与服务，不碰系统代理），**现已修复**：
停用时会把系统代理一并关闭（`ProxyEnable=0`，有 PAC 也清掉），原值写入 `state.json`，恢复时写回。
日志里能看到：

```
系统代理：当前开着（ProxyEnable=1 / ProxyServer='127.0.0.1:7890' ...）→ 关掉它，并记下原值以便恢复
    → exit=0  |  ProxyEnable=0 ProxyServer=127.0.0.1:7890 AutoConfigURL=
    ✓ 系统代理已关闭（恢复时会写回原值）
```

不想让它动系统代理：把配置里 `"DisableSystemProxy"` 改成 `false`。

**Q：Clash 开了「系统代理」或「TUN 模式」，为什么没被停？**
A：两处相关缺陷均已修复：① `Auto` 原来只认 TUN 网卡，现在**系统代理开着也算**；
② 网卡检测原来依赖 WMI（`Get-NetAdapter` + `Get-CimInstance`），在部分环境会全部失败 ——
现在改为三源合并（`Get-NetAdapter` + `netsh` + 纯 .NET），排除 `Teredo Tunneling` 这类伪接口，
另外把 **TUN 驱动服务（wintun/tap）是否在运行**当作一路独立证据。
若仍未停用，请附上 `%ProgramData%\FuckEAAC\fuckeaac.log`：其中会写明枚举到的全部网卡、
系统代理的三个值、以及每条目标"为什么停 / 为什么只提醒"。

**Q：EAAC 和我另一个游戏平台的反作弊（腾讯 ACE / 完美平台）冲突吗？**
A：**同类内核反作弊之间确实会互相拦**，这一点有公开报告：《星际战士2》玩家被 EAC（小蓝熊）直接要求
「关闭 ACE-BASE」；也有工具 [anticheattoggle](https://raw.githubusercontent.com/gmh5225/awesome-game-security/refs/heads/main/wiki/overviews/anti-cheat.md#8)
专门用于临时停掉 **腾讯 ACE / 完美平台 / Reason** 反作弊。EAAC 侧对应的报错通常是
**incompatible driver / Security Violation**。

关键差别在于驱动是否常驻：

- **腾讯 ACE**：驱动为**按需启动**（`DEMAND_START`），平时不加载 —— 只有玩过腾讯系游戏、
  `ACE-BASE` 等驱动仍驻留内核时，才可能影响 EA 游戏。
- **完美平台**：`MessageTransfer.sys` 为**开机自启**（同时是蓝屏、与 VirtualBox / eNSP 冲突的常见元凶），
  属于常驻内核。

处理办法（按推荐顺序）：**退出对应平台的游戏/客户端，然后重启电脑**（内核驱动通常卸载不掉，重启最干净）；
或者在配置里把那条目标从 `Hint` 改成 `Stop`，让「进入游戏模式」替你停掉（恢复时会按原状态启动回来）。
再次强调：**不要在那个平台的游戏运行时停它的反作弊**。

**Q：界面点了按钮没反应，像是卡住了？**
A：所有系统命令都有 **20 秒超时**，卡住会被强制结束并记入日志（不会永久卡死）。
日志里会出现"命令超过 20 秒没有返回，已强制结束"。

**Q：为什么 `Cargo.lock` 里能看到 `reqwest` / `hyper`？**
A：那是 Tauri 为其它平台/可选特性登记的依赖，本机 Windows 构建的依赖图里并不包含它们
（`cargo tree -i reqwest` 输出为空，exe 内也搜不到相关字符串）。见代码审查文档。

**Q：日志在哪？**
A：`%ProgramData%\FuckEAAC\fuckeaac.log`（界面点「打开日志」直达）。状态与待办在同目录的
`state.json` / `pending.json` / `window.json`。

---

## 已知边界

1. 它停的是"服务/驱动/进程/系统代理开关"，不是"网络行为" —— 停完之后你自己再把 Proxifier 打开，它不会拦。
2. 某些驱动不支持热卸载，停用后需要重启才彻底生效（日志里会明确写出）。
3. `tasklist` 被安全软件拦截时，"等待游戏"不可靠：程序会在开始等待前自检，查不到进程列表就直接拒绝，
   而不是假装在等。
4. 程序被强杀时不会自动恢复（后台线程随进程消失），但 `state.json` 仍在，下次打开可一键还原。
5. 它**不检测 DLL**（例如完美平台 `platform\plugin\*.dll`）：只看进程与服务/驱动。
6. 系统代理处理可逆，但有两种情况需留意：关闭时若写注册表失败，程序**不会**记录原值（避免恢复时乱写），
   日志里会有 ✗；恢复时会按记录把 `ProxyEnable` 写回原值，若你代理客户端里的"系统代理"开关本身是关的，
   记得再手动关一次。

---

## 仓库结构

```
├─ dist/FuckEAAC.exe        现成可运行的构建产物（约 3.5 MB）
├─ src/                     前端：index.html / styles.css / main.js / icons
├─ src-tauri/
│  ├─ src/                  Rust 后端：main / commands / config / detect / actions / play / state / util / diag
│  ├─ capabilities/         权限声明
│  ├─ icons/                打包用图标
│  ├─ Cargo.toml            4 个直接依赖：tauri / tauri-plugin-dialog / serde / serde_json
│  └─ tauri.conf.json
├─ package.json             前端侧依赖（只有一个 @tauri-apps/cli）
├─ pnpm-lock.yaml           锁定 CLI 版本
├─ LICENSE                  GNU General Public License v3.0
└─ .gitignore               排除 target/、node_modules/、运行配置与日志
```

## 许可证

本项目以 **GNU General Public License v3.0** 发布，完整条款见仓库根目录的 [`LICENSE`](LICENSE)。

```
FuckEAAC — 进游戏前停用代理/网络类工具，退出后自动恢复
Copyright (C) 2026  nfysx

This program is free software: you can redistribute it and/or modify it under the terms of
the GNU General Public License as published by the Free Software Foundation, either version 3
of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
See the GNU General Public License for more details.

You should have received a copy of the GNU General Public License along with this program.
If not, see <https://www.gnu.org/licenses/>.
```

> 使用/分发请注意 GPL-3.0 的要求：修改后再分发需同样以 GPL-3.0 开放源码，并保留版权与许可声明。
