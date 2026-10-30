use tauri::{Manager, State};

use crate::db::AppState;
use crate::models::{ClipboardFiles, DriveInfo, FileNode, FileRecord, KnownFolder, ReadTextResult};

/// 侧栏「此电脑」那一节
#[tauri::command]
pub fn get_drives() -> Result<Vec<DriveInfo>, String> {
    Ok(crate::services::files::list_drives())
}

/// 侧栏「常用文件夹」那一节
#[tauri::command]
pub fn get_known_folders() -> Result<Vec<KnownFolder>, String> {
    Ok(crate::services::files::known_folders())
}

/// 把某个文件所在目录临时授权给 webview 读。
/// 这页现在能浏览整台机器，但 asset scope 静态是空的、启动时也只授权了桌面和图标缓存：
/// 用户真要点开哪个文件，才授它所在那一层目录（不递归）——同目录的相对路径图片也就跟着能读，
/// 而没被点开的盘面不会被渲染代码摸到。
#[tauri::command]
pub fn authorize_preview(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    let scope = app.asset_protocol_scope();
    if let Some(dir) = p.parent() {
        let _ = scope.allow_directory(dir, false);
    }
    scope.allow_file(p).map_err(|e| e.to_string())
}

/// 一批路径的 shell 图标：返回 `路径 → 缓存里那张 PNG 的绝对路径`，抽不到的就不在表里。
/// 图标缓存目录启动时就授权过了，前端 `convertFileSrc` 直接读得动。只读，不落库。
#[tauri::command]
pub fn get_shell_icons(
    paths: Vec<String>,
    size: Option<u32>,
) -> Result<std::collections::HashMap<String, String>, String> {
    Ok(crate::services::files::shell_icons(&paths, size.unwrap_or(32)))
}

/// 监视哪几个目录由界面说：这页开的是整台机器，把每块盘都挂上监视既不现实也没必要，
/// 只盯当前几个标签停着的目录就够了（磁盘一动 → `files-changed` → 各栏重读）
#[tauri::command]
pub fn watch_folders(state: State<AppState>, folders: Vec<String>) -> Result<(), String> {
    if let Some(fw) = state.watcher.lock().unwrap().as_mut() {
        fw.update(&folders);
    }
    Ok(())
}

#[tauri::command]
pub fn list_files(
    state: State<AppState>,
    root: String,
    include_hidden: bool,
) -> Result<Vec<FileNode>, String> {
    let conn = state.conn();
    crate::services::files::list_files(&conn, &root, include_hidden).map_err(|e| e.to_string())
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

/// 新建文件夹。只动磁盘不动库，所以前端用 invoke 不用 write
#[tauri::command]
pub fn create_folder(parent: String, name: String) -> Result<String, String> {
    crate::services::files::create_folder(&parent, &name).map_err(|e| e.to_string())
}

/// 改名。库里那条标注跟着走（`repoint_meta`），所以算改了库
#[tauri::command]
pub fn rename_path(state: State<AppState>, path: String, new_name: String) -> Result<String, String> {
    let conn = state.conn();
    crate::services::files::rename_path(&conn, &path, &new_name).map_err(|e| e.to_string())
}

/// 删除。`to_trash=false` 是彻底删，连那条标注一起摘
#[tauri::command]
pub fn delete_paths(state: State<AppState>, paths: Vec<String>, to_trash: bool) -> Result<usize, String> {
    let conn = state.conn();
    crate::services::files::delete_paths(&conn, &paths, to_trash).map_err(|e| e.to_string())
}

/// 拷进某个目录。只动磁盘
#[tauri::command]
pub fn copy_paths(paths: Vec<String>, dest: String) -> Result<usize, String> {
    crate::services::files::copy_into(&paths, &dest).map_err(|e| e.to_string())
}

/// 搬进某个目录。标注跟着走，算改了库
#[tauri::command]
pub fn move_paths(state: State<AppState>, paths: Vec<String>, dest: String) -> Result<usize, String> {
    let conn = state.conn();
    crate::services::files::move_into(&conn, &paths, &dest).map_err(|e| e.to_string())
}

/// 把这几项放进系统剪贴板，资源管理器 / Outlook / 浏览器都认。
/// `mode` 是 `copy` / `move`；`none` 是「搬完了，这串别再用」——资源管理器事后也这么标
#[tauri::command]
pub fn clipboard_write(paths: Vec<String>, mode: String) -> Result<(), String> {
    let mode = match mode.as_str() {
        "copy" => crate::win::clipboard::Mode::Copy,
        "move" => crate::win::clipboard::Mode::Move,
        "none" => crate::win::clipboard::Mode::None,
        other => return Err(format!("不认识的剪贴板模式：{other}")),
    };
    crate::win::clipboard::write(&paths, mode).map_err(|e| e.to_string())
}

/// 读系统剪贴板里的文件清单。拷的是文字或图片就回 `None`：这一页只认路径
#[tauri::command]
pub fn clipboard_read() -> Result<Option<ClipboardFiles>, String> {
    crate::win::clipboard::read().map_err(|e| e.to_string())
}
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    crate::win::shell::open(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reveal_in_explorer(path: String) -> Result<(), String> {
    crate::win::shell::reveal(&path).map_err(|e| e.to_string())
}