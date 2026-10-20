use tauri::{Manager, State};

use crate::db::AppState;
use crate::models::{FileNode, FileRecord, ReadTextResult};

#[tauri::command]
pub fn get_managed_folders(state: State<AppState>) -> Result<Vec<String>, String> {
    let conn = state.conn();
    crate::services::files::get_managed_folders(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_managed_folder(
    state: State<AppState>,
    app: tauri::AppHandle,
    path: String,
) -> Result<Vec<String>, String> {
    let folders = {
        let conn = state.conn();
        crate::services::files::add_managed_folder(&conn, &path).map_err(|e| e.to_string())?
    };
    // 当场授权，不然新加的目录要等下次启动才能预览；这条失败不影响登记本身——
    // 重启时 preview_roots 会把它补进名单，所以只在注释里说清，不假装添加失败了。
    let _ = app.asset_protocol_scope().allow_directory(&path, true);
    if let Some(fw) = state.watcher.lock().unwrap().as_mut() {
        fw.update(&folders);
    }
    Ok(folders)
}

#[tauri::command]
pub fn remove_managed_folder(state: State<AppState>, path: String) -> Result<Vec<String>, String> {
    let folders = {
        let conn = state.conn();
        crate::services::files::remove_managed_folder(&conn, &path).map_err(|e| e.to_string())?
    };
    // 授权撤不回来：scope 只有 allow_* 和永久压住它的 forbid_*，用 forbid 会把以后重新
    // 添加同一个目录一起堵死。所以这里只摘监视器，预览权限留到下次启动自然消失。
    if let Some(fw) = state.watcher.lock().unwrap().as_mut() {
        fw.update(&folders);
    }
    Ok(folders)
}

#[tauri::command]
pub fn list_files(state: State<AppState>, root: String) -> Result<Vec<FileNode>, String> {
    let conn = state.conn();
    crate::services::files::list_files(&conn, &root).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_file(state: State<AppState>, path: String) -> Result<FileRecord, String> {
    let conn = state.conn();
    crate::services::files::get_file(&conn, &path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_file_meta(
    state: State<AppState>,
    path: String,
    alias: Option<String>,
    description: Option<String>,
) -> Result<FileRecord, String> {
    let conn = state.conn();
    crate::services::files::save_file_meta(&conn, &path, alias, description)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn read_text_file(path: String) -> Result<ReadTextResult, String> {
    crate::services::files::read_text_file(&path).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    crate::win::shell::open(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reveal_in_explorer(path: String) -> Result<(), String> {
    crate::win::shell::reveal(&path).map_err(|e| e.to_string())
}