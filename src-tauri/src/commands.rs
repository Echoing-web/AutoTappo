//! 前端 Tauri IPC 调用命令接口

use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, State};

use crate::{window, AppState};

#[derive(Serialize)]
pub struct AppStatusResponse {
    pub running: bool,
    pub speed_mode: u8,
    pub total_sent: u64,
    pub hotkey_vk: i32,
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>) -> AppStatusResponse {
    let inner = &state.0;
    AppStatusResponse {
        running: inner.clicker.running.load(Ordering::Relaxed),
        speed_mode: inner.clicker.speed_mode.load(Ordering::Relaxed),
        total_sent: inner.clicker.total_sent.load(Ordering::Relaxed),
        hotkey_vk: inner.clicker.hotkey_vk.load(Ordering::Relaxed),
    }
}

#[tauri::command]
pub fn toggle_running(state: State<'_, AppState>) -> bool {
    let inner = &state.0;
    let prev = inner.clicker.running.fetch_xor(true, Ordering::SeqCst);
    !prev
}

#[tauri::command]
pub fn set_running(state: State<'_, AppState>, running: bool) {
    let inner = &state.0;
    inner.clicker.running.store(running, Ordering::SeqCst);
}

#[tauri::command]
pub fn set_speed(state: State<'_, AppState>, mode: u8) {
    let inner = &state.0;
    inner.clicker.speed_mode.store(mode, Ordering::SeqCst);
}

#[tauri::command]
pub fn set_hotkey(state: State<'_, AppState>, vk: i32) {
    let inner = &state.0;
    inner.clicker.hotkey_vk.store(vk, Ordering::SeqCst);
}

#[tauri::command]
pub fn get_hotkey(state: State<'_, AppState>) -> i32 {
    let inner = &state.0;
    inner.clicker.hotkey_vk.load(Ordering::Relaxed)
}

#[tauri::command]
pub fn reset_sent_count(state: State<'_, AppState>) {
    let inner = &state.0;
    inner.clicker.total_sent.store(0, Ordering::SeqCst);
}

#[tauri::command]
pub fn minimize_to_tray(app: AppHandle) {
    let _ = window::close(&app);
}

#[tauri::command]
pub fn quit_app(state: State<'_, AppState>, app: AppHandle) {
    let inner = &state.0;
    inner.shutting_down.store(true, Ordering::SeqCst);
    inner.clicker.shutdown.store(true, Ordering::SeqCst);
    inner.chest.shutdown.store(true, Ordering::SeqCst);
    app.exit(0);
}

// ---------------- 自动开箱 ----------------

/// 读取自动开箱引擎的实时状态（前端轮询用）
#[tauri::command]
pub fn get_chest_status(state: State<'_, AppState>) -> crate::chest::ChestSnapshot {
    state.0.chest.status()
}

/// 开关自动开箱
#[tauri::command]
pub fn set_auto_chest(state: State<'_, AppState>, enabled: bool) {
    state.0.chest.set_enabled(enabled);
}

/// 以管理员身份重启（开箱需要与官方桌宠同等完整性）
#[tauri::command]
pub fn relaunch_as_admin(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let inner = &state.0;
    crate::chest::relaunch_as_admin()?;
    inner.shutting_down.store(true, Ordering::SeqCst);
    inner.clicker.shutdown.store(true, Ordering::SeqCst);
    inner.chest.shutdown.store(true, Ordering::SeqCst);
    app.exit(0);
    Ok(())
}
