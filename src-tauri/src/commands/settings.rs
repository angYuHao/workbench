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

/// 唤醒快捷键。两个入口都必须是 **async** 命令：Tauri 把同步命令丢在主线程跑，
/// 而这个插件内部要 `run_main_thread!`，主线程等自己 = 直接死锁（上一版就栽在这儿）。
/// 解绑也不留旧键：换键位是先 `unregister_all` 再注册新的，所以不可能出现两个热键同时生效。
const DEFAULT_WAKE_HOTKEY: &str = "Alt+Space";

/// 启动时装载设置里的热键；没存过就用默认键。前端在挂起来后调一次。
#[tauri::command]
pub async fn load_wake_hotkey(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let saved = {
        let conn = state.conn();
        crate::services::settings::get_setting(&conn, "wake_hotkey").map_err(|e| e.to_string())?
    };
    let key = saved.filter(|k| !k.trim().is_empty()).unwrap_or_else(|| DEFAULT_WAKE_HOTKEY.to_string());
    bind(&app, &state, &key).await
}

/// 录制器按下新组合键后调这个：注册成功才写设置，失败就把占用原话带回界面，不留下「设了但没生效」。
#[tauri::command]
pub async fn set_wake_hotkey(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    key: String,
) -> Result<String, String> {
    let key = key.trim().to_string();
    if key.is_empty() {
        clear(&app, &state).await?;
        return Ok(String::new());
    }
    bind(&app, &state, &key).await
}

/// 摘掉热键并记住「不要热键」：设置里存空串，下次启动就不装载。
#[tauri::command]
pub async fn clear_wake_hotkey(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    clear(&app, &state).await
}

async fn bind(app: &tauri::AppHandle, state: &AppState, key: &str) -> Result<String, String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let _ = app.global_shortcut().unregister_all();
    app.global_shortcut()
        .register(key)
        .map_err(|e| format!("「{key}」注册不上（多半被别的程序占了）：{e}"))?;
    let conn = state.conn();
    crate::services::settings::set_setting(&conn, "wake_hotkey", key).map_err(|e| e.to_string())?;
    Ok(key.to_string())
}

async fn clear(app: &tauri::AppHandle, state: &AppState) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let _ = app.global_shortcut().unregister_all();
    let conn = state.conn();
    crate::services::settings::set_setting(&conn, "wake_hotkey", "").map_err(|e| e.to_string())
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