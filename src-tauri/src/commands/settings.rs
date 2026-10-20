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

/// 唤醒快捷键。三个入口都必须是 **async** 命令：Tauri 把同步命令丢在主线程跑，
/// 而这个插件内部要 `run_main_thread!`，主线程等自己 = 直接死锁（上一版就栽在这儿）。
const DEFAULT_WAKE_HOTKEY: &str = "Alt+Space";

/// 热键现在的真实样子。界面照这三个字段说话，不再靠「命令有没有抛」来猜：
/// 启动时默认键被占是常事，抛错只会变成每次进应用撞一句谁也看不懂的报错。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeHotkey {
    /// 该显示在框里的那个写法；空串 = 用户明确不要热键
    pub key: String,
    /// 这一会儿真挂着生效的是不是它
    pub bound: bool,
    /// 注册不上的原话；`bound` 为真时是空串
    pub error: String,
}

/// 该装哪个键：**没存过**才用默认键；**存了空串**是用户明确清掉了，别再替他装回来
fn planned_key(saved: Option<&str>) -> String {
    match saved {
        None => DEFAULT_WAKE_HOTKEY.to_string(),
        Some(v) => v.trim().to_string(),
    }
}

/// 启动时装载设置里的热键。注册不上不抛错：把键名和原话一起带回界面，
/// 让他看得见「现在挂的是哪个、有没有生效」。这条路上不写库——
/// 默认键注册失败时不该把 `Alt+Space` 悄悄种进设置里。
#[tauri::command]
pub async fn load_wake_hotkey(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<WakeHotkey, String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let saved = {
        let conn = state.conn();
        crate::services::settings::get_setting(&conn, "wake_hotkey").map_err(|e| e.to_string())?
    };
    let key = planned_key(saved.as_deref());
    if key.is_empty() {
        // 进程里可能还挂着上一次装的键：一起摘掉，别留一个界面上看不见的热键
        let _ = app.global_shortcut().unregister_all();
        return Ok(WakeHotkey { key: String::new(), bound: false, error: String::new() });
    }
    match app.global_shortcut().register(key.as_str()) {
        Ok(_) => Ok(WakeHotkey { key, bound: true, error: String::new() }),
        Err(e) => Ok(WakeHotkey { key, bound: false, error: e.to_string() }),
    }
}

/// 录制器按下新组合键后调这个：注册成功才写设置。
/// 新键注册不上时先把**原来那个挂回去**——换键失败不该把他本来能用的热键一起弄没。
#[tauri::command]
pub async fn set_wake_hotkey(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    key: String,
) -> Result<WakeHotkey, String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let new_key = key.trim().to_string();
    if new_key.is_empty() {
        clear(&app, &state).await?;
        return Ok(WakeHotkey { key: String::new(), bound: false, error: String::new() });
    }
    let saved = {
        let conn = state.conn();
        crate::services::settings::get_setting(&conn, "wake_hotkey").map_err(|e| e.to_string())?
    };
    let _ = app.global_shortcut().unregister_all();
    if let Err(e) = app.global_shortcut().register(new_key.as_str()) {
        let old = saved.unwrap_or_default().trim().to_string();
        let restored = !old.is_empty() && app.global_shortcut().register(old.as_str()).is_ok();
        let conn = state.conn();
        crate::services::settings::set_setting(&conn, "wake_hotkey", &old).map_err(|e| e.to_string())?;
        return Ok(WakeHotkey {
            key: old,
            bound: restored,
            error: format!("「{new_key}」注册不上（多半被别的程序占了）：{e}"),
        });
    }
    let conn = state.conn();
    crate::services::settings::set_setting(&conn, "wake_hotkey", &new_key).map_err(|e| e.to_string())?;
    Ok(WakeHotkey { key: new_key, bound: true, error: String::new() })
}

/// 摘掉热键并记住「不要热键」：设置里存空串，下次启动就不装载（不会被默认键顶回来）。
#[tauri::command]
pub async fn clear_wake_hotkey(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<WakeHotkey, String> {
    clear(&app, &state).await?;
    Ok(WakeHotkey { key: String::new(), bound: false, error: String::new() })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 「没存过」和「存了空串」是两件事：前者才用默认键，后者是他明确不要。
    /// 混在一起的结果就是每次进应用都被重装一个默认键，还顺带撞一句报错。
    #[test]
    fn cleared_stays_cleared_and_only_missing_falls_back_to_default() {
        assert_eq!(planned_key(None), "Alt+Space");
        assert_eq!(planned_key(Some("")), "");
        assert_eq!(planned_key(Some("   ")), "", "一片空白也算不要");
        assert_eq!(planned_key(Some("Ctrl+Alt+W")), "Ctrl+Alt+W");
        // 库里留着的旧写法（老录制器存过 e.code 原样）照原样带出去，让界面看得见它
        assert_eq!(planned_key(Some("Ctrl+Alt+KeyW")), "Ctrl+Alt+KeyW");
    }
}
