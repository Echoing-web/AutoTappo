// Release 发布版禁用 Windows 控制台黑框
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::Manager;

use autotappo::box_opener::BoxOpenerController;
use autotappo::clicker::ClickerController;
use autotappo::{AppState, Inner};

fn main() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let clicker = Arc::new(ClickerController::new());
            // 启动后台静默连点原生工作线程
            clicker.start_worker();

            let box_opener = Arc::new(BoxOpenerController::new());
            // 启动后台自动开箱 UIA 工作线程（默认关闭，由面板开关控制）
            box_opener.start_worker();

            let inner = Arc::new(Inner {
                clicker,
                box_opener,
                tray: std::sync::Mutex::new(None),
                tray_ok: std::sync::atomic::AtomicBool::new(false),
                shutting_down: std::sync::atomic::AtomicBool::new(false),
            });            app.manage(AppState(Arc::clone(&inner)));

            // 构造系统托盘图标
            match autotappo::tray::build(app, Arc::clone(&inner)) {
                Ok(icon) => {
                    if let Ok(mut slot) = inner.tray.lock() {
                        *slot = Some(icon);
                    }
                    inner.tray_ok.store(true, Ordering::SeqCst);
                }
                Err(e) => {
                    eprintln!("[AutoTappo] 托盘图标创建失败: {e}");
                }
            }

            // 启动时默认打开主控制面板窗口
            let _ = autotappo::window::ensure(app.handle());

            Ok(())
        })
        .on_window_event(|win, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = win.app_handle();
                let tray_ok = app.state::<AppState>().0.tray_ok.load(Ordering::SeqCst);
                if tray_ok {
                    // 点击右上角关闭：不退出程序，直接销毁窗口释放 WebView2 渲染内存
                    // 托盘图标依然常驻，点击托盘可随时秒级重新唤醒
                    api.prevent_close();
                    let _ = autotappo::window::close(app);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            autotappo::commands::get_status,
            autotappo::commands::toggle_running,
            autotappo::commands::set_running,
            autotappo::commands::set_speed,
            autotappo::commands::set_hotkey,
            autotappo::commands::get_hotkey,
            autotappo::commands::reset_sent_count,
            autotappo::commands::set_auto_box,
            autotappo::commands::minimize_to_tray,
            autotappo::commands::quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("AutoTappo 启动失败");
    app.run(|handle, event| {
        // 关闭主窗口后窗口数为 0，Tauri 默认会请求退出。
        // 托盘常驻守护时必须拦住；只有用户在托盘菜单真正点击「退出」时才放行。
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            let state = handle.state::<AppState>();
            if !state.0.shutting_down.load(Ordering::SeqCst) && state.0.tray_ok.load(Ordering::SeqCst) {
                api.prevent_exit();
            }
        }
    });
}
