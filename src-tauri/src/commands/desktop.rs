use tauri::State;

use crate::db::AppState;
use crate::models::{AppEntry, DesktopItem, OrganizeLog, OrganizePlan};

#[tauri::command]
pub fn scan_desktop(state: State<AppState>) -> Result<Vec<DesktopItem>, String> {
    let conn = state.conn();
    crate::services::desktop::scan_desktop(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn organize_desktop(
    state: State<AppState>,
    items: Vec<OrganizePlan>,
) -> Result<Vec<OrganizeLog>, String> {
    let conn = state.conn();
    crate::services::desktop::organize_desktop(&conn, &items).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn revert_organize(state: State<AppState>, log_id: Option<i64>) -> Result<usize, String> {
    let conn = state.conn();
    crate::services::desktop::revert_organize(&conn, log_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn launch_app(state: State<AppState>, id: i64) -> Result<(), String> {
    let conn = state.conn();
    crate::services::desktop::launch_by_id(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_apps(state: State<AppState>) -> Result<Vec<AppEntry>, String> {
    let conn = state.conn();
    crate::services::desktop::get_apps(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_app(state: State<AppState>, app: AppEntry) -> Result<AppEntry, String> {
    let conn = state.conn();
    crate::services::desktop::save_app(&conn, &app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_app(state: State<AppState>, id: i64) -> Result<(), String> {
    let conn = state.conn();
    crate::services::desktop::delete_app(&conn, id).map_err(|e| e.to_string())
}