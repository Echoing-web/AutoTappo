//! 主控制面板窗口生命周期管理 (参考 CampusFlow 方案)
//!
//! 窗口**不预建**，按需动态创建与显示。
//! 关闭窗口时是**直接销毁**而不是简单隐藏——彻底释放 WebView2 渲染内存，
//! 只保留极轻量的 Rust 托盘常驻守护。

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "main";

/// 窗口已存在则激活聚焦并显示，不存在则新建
pub fn ensure(app: &AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return Ok(());
    }
    open(app)
}

/// 新建主控制面板窗口
pub fn open(app: &AppHandle) -> tauri::Result<()> {
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::default())
        .title("AutoTappo · 鸣潮桌宠挂机助手")
        .inner_size(452.0, 620.0)
        .min_inner_size(420.0, 560.0)
        .resizable(false)
        .center()
        .build()?;
    let _ = w.show();
    let _ = w.set_focus();
    Ok(())
}

/// 销毁窗口以释放 WebView 内存。返回 true 表示成功销毁
pub fn close(app: &AppHandle) -> bool {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
        true
    } else {
        false
    }
}
