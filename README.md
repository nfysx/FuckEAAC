# FuckEAAC

进游戏前**自动停掉本机的代理 / 网络类工具**，退出游戏后**自动恢复**。

EA 系游戏（战地，Apex，FC 系列……）由 **EAAC** 保护，它检测到 Proxifier、Clash 这类
会改写网络流量的工具时，可能直接拒绝启动游戏。FuckEAAC会在**启动游戏之前把这些工具停干净，玩完再原样恢复**。

| | |
|---|---|
| 平台 | Windows 10 / 11（x64） |
| 形态 | 单文件程序：`FuckEAAC.exe` |
| 技术栈 | Tauri 2 + Rust 后端／纯 HTML + CSS + JS 前端 |
| 依赖 | 运行时依赖只有 4 个：`tauri`、`tauri-plugin-dialog`、`serde`、`serde_json` |

---

## ⚠️ 免责声明

- 本项目**不针对、不修改、不绕过任何反作弊**：不注入进程、不 HOOK、不伪造签名、不修改游戏文件、
  不调整优先级。`EAAntiCheat` / `EasyAntiCheat` 自身**从不**出现在它的目标清单里。


## 功能

| 功能 | 说明 |
|---|---|
| **进入游戏模式** | 结束命中的代理客户端 → 停掉命中的服务与内核驱动 →驱动改按需启动 → 关闭系统代理 |
| **自动等待 + 自动恢复** | 后台线程每 5/10 秒查一次游戏进程；游戏退出即恢复：重开客户端、启动服务/驱动、还原启动类型、写回系统代理原值 |
| **首次运行自动扫描** | 只保留**这台机器上真的装了**的目标，不预置清单 |
| **环境诊断** | Secure Boot、TPM、虚拟机、测试签名、VBS/HVCI、CPU/内存、冲突软件等 |

## 下载与运行

1. 下载 `FuckEAAC.exe`
2. 双击运行即可。**建议以管理员身份运行**。
3. 首次运行会扫描本机并生成 `FuckEAAC.config.json`（与 exe 同目录）。

> 配置与日志属于每台机器自己的数据，不在仓库里：
> 配置 `FuckEAAC.config.json`（与 exe 同目录），状态/日志在 `%ProgramData%\FuckEAAC\`。

---

## 从源码构建

环境：Windows + Visual Studio + Rust stable（`x86_64-pc-windows-msvc`）+ Node.js / pnpm + WebView2 。

```powershell
pnpm install                
pnpm tauri dev               

cd src-tauri
cargo build --release       

cargo test                  

cargo clippy --all-targets   # 静态检查
cargo fmt                    # 格式化
cargo test -- --ignored --nocapture   # 端到端：造一个假"游戏"进程

pnpm tauri build             # 打 NSIS 安装包
cargo clean                  # 清缓存
```

## 配置文件

`FuckEAAC.config.json`。

```jsonc
{
  "VpnAdapterPattern": "tun|tap|wintun|clash|mihomo|proxifier|utun",
  "GameProcessNames": ["bf6", "BF2042", "FC25", "FC26"],  
  "GameExe": "",
  "DisableSystemProxy": true,   
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
| `Action` | `Stop` 直接停；`Warn` 只提醒；`Auto` = 检测到 **TUN 网卡在线**或**系统代理开着**才停（给 Clash 系用）；`Hint` = **需你手动处理**：没跑就只提醒，**正在跑则归入「需停用」栏并提示你自己关闭或重启，工具不动手**|
| `ProcessPattern` / `ServicePattern` | `\|` 分隔的关键字，**大小写无关的子串匹配**（写 `clash` 能命中 `Clash Party`） |
| `VpnAdapterPattern` | 什么网卡算"虚拟网卡"（显示在界面上，也是 `Auto` 的触发条件之一） |
| `RelaunchPaths` | 恢复时要重新拉起的客户端路径，支持 `%ENV%` |
| `DisableSystemProxy` | 设为 `false` = 本工具完全不碰系统代理设置 |

内嵌的目标清单在 `src-tauri/src/config.rs` 的 `default_config()`，共 20 条）。

## 已知边界

1. 它停的是"服务/驱动/进程/系统代理开关"，不是"网络行为" —— 停完之后你自己再把 Proxifier 打开，它不会拦。
2. 某些驱动不支持热卸载，停用后需要重启才彻底生效。
3. `tasklist` 被安全软件拦截时，"等待游戏"不可靠：程序会在开始等待前自检，查不到进程列表就直接拒绝，
   而不是假装在等。
4. 程序被强杀时不会自动恢复，但 `state.json` 仍在，下次打开可一键还原。
5. 它**不检测 DLL**（例如完美平台 `platform\plugin\*.dll`）：仅看进程与服务/驱动。
6. 系统代理处理可逆，但有两种情况需留意：关闭时若写注册表失败，程序**不会**记录原值，
   日志里会有 ✗；恢复时会按记录把 `ProxyEnable` 写回原值，若你代理客户端里的"系统代理"开关本身是关的，
   记得再手动关一次。

---

## 仓库结构

```
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
