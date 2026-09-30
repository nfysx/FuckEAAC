// 始终不要控制台窗口：
//   * 调试信息改为写进 %ProgramData%\FuckEAAC\fuckeaac.log
#![windows_subsystem = "windows"]
//! FuckEAAC —— Tauri + Rust 版入口
//!
//! 结构：
//!   util.rs      调系统命令 / 判管理员 / 提权 / 时间
//!   config.rs    配置模型 + 18 个默认目标
//!   state.rs     状态文件+ 日志
//!   detect.rs    服务/驱动/进程/虚拟网卡/系统代理 → 快照
//!   actions.rs   停用 / 恢复
//!   play.rs      游戏模式：后台等游戏进程，退出后自动恢复
//!   diag.rs      环境诊断（Secure Boot / TPM / 测试模式 / 冲突软件…）
//!   commands.rs  前后端接口
//!   main.rs      本文件：窗口、托盘保活、关窗进托盘

mod actions;
mod commands;
mod config;
mod detect;
mod diag;
mod play;
mod state;
mod util;

use std::sync::atomic::Ordering;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WindowEvent};

/// 显示并聚焦主窗口（托盘点两下 / 菜单点「显示主界面」都走这里）
fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// 记住窗口位置/大小（下次启动、或"切换到管理员模式"后恢复）
fn save_window_geom(window: &tauri::Window) {
    if let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) {
        util::save_window(&util::WindowGeom {
            x: pos.x,
            y: pos.y,
            w: size.width,
            h: size.height,
            maximized: window.is_maximized().unwrap_or(false),
        });
    }
}

fn main() {
    tauri::Builder::default()
        // 「浏览…」选游戏 exe 用的文件对话框插件
        .plugin(tauri_plugin_dialog::init())
        // 全局状态：配置 + 配置来源 + 演习模式
        .manage(commands::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_status,
            commands::get_config,
            commands::export_config,
            commands::rescan_config,
            commands::set_dry_run,
            commands::stop_targets,
            commands::restore_targets,
            commands::relaunch_admin,
            commands::open_path,
            commands::run_diag,
            commands::quit_app,
            commands::hide_window,
            commands::set_pending,
            commands::take_pending,
            commands::start_play,
            commands::play_status,
            commands::cancel_play
        ])
        .setup(|app| {
            // 恢复上次的窗口位置/大小 —— "切换到管理员模式"会重启进程，
            // 恢复几何后看起来就像同一个窗口被"升级"了（而不是又冒出一个新窗口）。
            if let Some(w) = app.get_webview_window("main") {
                if let Some(g) = util::load_window() {
                    let _ = w.set_size(tauri::PhysicalSize::new(g.w, g.h));
                    let _ = w.set_position(tauri::PhysicalPosition::new(g.x, g.y));
                    if g.maximized {
                        let _ = w.maximize();
                    }
                }
            }
            // 托盘图标创建失败**不应该**让程序起不来：
            // 例如资源管理器正在重启、或以无交互桌面的方式启动时，创建托盘会"拒绝访问"。
            // 这种情况下主窗口照常工作，只是没有后台保活图标。
            if let Err(e) = setup_tray(app) {
                // 没有控制台了，写进日志文件（界面「打开日志」能看到）
                state::log("WARN", &format!("托盘图标创建失败：{e}"));
            }
            // 未提权时在日志里明确提醒一次（界面上还有醒目的琥珀色徽标，点一下就能切换）
            if !util::is_admin() {
                state::log(
                    "WARN",
                    "当前以普通用户权限运行：无法停用内核驱动/服务。点界面右上角「⚠ 普通权限」即可切换。",
                );
            }
            Ok(())
        })
        // ---------------- 关窗 → 问一句"留在后台还是彻底退出" ----------------
        // 进程本身继续活着，所以"等游戏退出 → 自动恢复"不会因为关窗口而中断。
        // 这里只负责**拦住关闭**并通报前端；弹什么提示交给前端（保持 UI 逻辑只有一处）。
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                save_window_geom(window);
                if commands::REALLY_EXIT.load(Ordering::SeqCst) {
                    return; // 正在退出：放行，别再拦
                }
                api.prevent_close();
                let _ = window.app_handle().emit("app://close-requested", ());
            }
        })
        .run(tauri::generate_context!())
        .expect("FuckEAAC 启动失败");
}

/// 创建托盘图标与右键菜单（失败只打日志，不影响主窗口）
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let handle = app.handle();

    let mi_show = MenuItem::with_id(handle, "show", "显示主界面", true, None::<&str>)?;
    let mi_play = MenuItem::with_id(handle, "play", "游戏模式", true, None::<&str>)?;
    let mi_restore = MenuItem::with_id(handle, "restore", "恢复代理", true, None::<&str>)?;
    let mi_diag = MenuItem::with_id(handle, "diag", "环境诊断", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(handle)?;
    let mi_quit = MenuItem::with_id(handle, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(
        handle,
        &[&mi_show, &mi_play, &mi_restore, &mi_diag, &sep1, &mi_quit],
    )?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("icons/ 的图标未被编进程序".into()))?;

    let _tray = TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("FuckEAAC · 游戏模式")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main(app),
            // 这两个动作交给前端执行，保持"只有一处 UI 逻辑"
            "play" => {
                show_main(app);
                let _ = app.emit("tray://play", ());
            }
            "restore" => {
                let _ = app.emit("tray://restore", ());
            }
            "diag" => {
                show_main(app);
                let _ = app.emit("tray://diag", ());
            }
            "quit" => {
                commands::REALLY_EXIT.store(true, Ordering::SeqCst);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}
