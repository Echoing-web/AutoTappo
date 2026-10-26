//! 系统托盘图标与右键菜单 (参考 CampusFlow 方案)
//!
//! 关闭窗口只是收进系统托盘，后台挂机守护继续运行；真正退出在托盘菜单里。

use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::{window, Inner};

/// 构建托盘图标句柄。
/// 必须将其存放在 `Inner` 共享状态中，否则句柄析构后托盘图标会随之消失。
pub fn build(app: &tauri::App, inner: Arc<Inner>) -> tauri::Result<TrayIcon> {
    let show = MenuItem::with_id(app, "show", "显示控制面板", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "启动 / 暂停挂机 (F6)", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出 AutoTappo", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &toggle, &sep, &quit])?;

    let inner_for_menu = Arc::clone(&inner);

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("AutoTappo · 鸣潮桌宠挂机助手")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => {
                let _ = window::ensure(app);
            }
            "toggle" => {
                let prev = inner_for_menu.clicker.running.fetch_xor(true, Ordering::SeqCst);
                println!("[Tray] 切换运行状态: {}", if !prev { "运行中" } else { "已暂停" });
            }
            "quit" => {
                inner_for_menu.shutting_down.store(true, Ordering::SeqCst);
                inner_for_menu.clicker.shutdown.store(true, Ordering::SeqCst);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = window::ensure(tray.app_handle());
            }
        });

    if let Some(icon) = app.handle().default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    let icon = builder.build(app)?;
    Ok(icon)
}
