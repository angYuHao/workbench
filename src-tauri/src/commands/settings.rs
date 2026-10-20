use std::collections::HashMap;

use tauri::State;

use crate::db::AppState;

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<HashMap<String, String>, String> {
    let conn = state.conn();
    crate::services::settings::get_all_settings(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_setting(state: State<AppState>, key: String, value: String) -> Result<(), String> {
    let conn = state.conn();
    crate::services::settings::set_setting(&conn, &key, &value).map_err(|e| e.to_string())
}

/// 关闭到托盘：存设置 + 立刻装上/摘掉托盘图标，两边不会不同步
#[tauri::command]
pub fn set_close_to_tray(
    state: State<AppState>,
    app: tauri::AppHandle,
    on: bool,
) -> Result<(), String> {
    {
        let conn = state.conn();
        crate::services::settings::set_setting(&conn, "close_to_tray", if on { "1" } else { "0" })
            .map_err(|e| e.to_string())?;
    }
    if on {
        crate::tray::install(&app).map_err(|e| e.to_string())?;
    } else {
        crate::tray::remove(&app);
    }
    Ok(())
}

#[tauri::command]
pub fn export_db(state: State<AppState>, dest: String) -> Result<(), String> {
    let conn = state.conn();
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .map_err(|e| e.to_string())?;
    drop(conn);
    std::fs::copy(&state.db_path, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// 恢复备份：覆盖库文件后用新库重开连接。
#[tauri::command]
pub fn restore_db(state: State<AppState>, src: String) -> Result<(), String> {
    {
        let conn = state.conn();
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .map_err(|e| e.to_string())?;
    }
    std::fs::copy(&src, &state.db_path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(state.db_path.with_extension("db-wal"));
    let _ = std::fs::remove_file(state.db_path.with_extension("db-shm"));

    let mut conn = state.conn();
    *conn = rusqlite::Connection::open(&state.db_path).map_err(|e| e.to_string())?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| e.to_string())?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| e.to_string())?;
    // 旧备份里不会有后来新增的表（建表全是 IF NOT EXISTS，平时靠每次启动补一遍）。
    // 换完文件不补这一遍，桌面和书签两页会直接因为缺表读不出东西。
    conn.execute_batch(include_str!("../db/schema.sql"))
        .map_err(|e| e.to_string())?;
    Ok(())
}
