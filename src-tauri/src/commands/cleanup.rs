use crate::services::cleanup::{CategoryResult, CategoryScan};

/// 扫描/清理都是重文件系统操作（临时目录可能几十万个文件）。
/// 同步命令会占住主线程把整页卡死，所以一律 async + spawn_blocking 挪到后台线程池。
#[tauri::command]
pub async fn cleanup_scan() -> Result<Vec<CategoryScan>, String> {
    tauri::async_runtime::spawn_blocking(crate::services::cleanup::scan)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cleanup_run(ids: Vec<String>) -> Result<Vec<CategoryResult>, String> {
    tauri::async_runtime::spawn_blocking(move || crate::services::cleanup::run(&ids))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}