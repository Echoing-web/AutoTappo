//! AutoTappo 库目标：把核心模块暴露给集成测试 (tests/bdd_*.rs) 与二进制入口复用。

pub mod box_logic;
pub mod box_opener;
pub mod clicker;
pub mod commands;
pub mod tray;
pub mod window;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::tray::TrayIcon;

use crate::box_opener::BoxOpenerController;
use crate::clicker::ClickerController;

/// 全局共享状态
pub struct Inner {
    pub clicker: Arc<ClickerController>,
    pub box_opener: Arc<BoxOpenerController>,
    pub tray: Mutex<Option<TrayIcon>>,
    pub tray_ok: AtomicBool,
    pub shutting_down: AtomicBool,
}

pub struct AppState(pub Arc<Inner>);

/// 统一退出信号：让所有后台工作线程停下
pub fn request_shutdown(inner: &Inner) {
    inner.shutting_down.store(true, Ordering::SeqCst);
    inner.clicker.shutdown.store(true, Ordering::SeqCst);
    inner.box_opener.shutdown.store(true, Ordering::SeqCst);
}
