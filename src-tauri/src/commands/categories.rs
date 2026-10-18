use tauri::State;

use crate::db::AppState;
use crate::models::{Category, DesktopRule};

#[tauri::command]
pub fn get_categories(state: State<AppState>, r#type: String) -> Result<Vec<Category>, String> {
    let conn = state.conn();
    crate::services::categories::get_categories(&conn, &r#type).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_category(state: State<AppState>, category: Category) -> Result<Category, String> {
    let conn = state.conn();
    crate::services::categories::save_category(&conn, &category).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_category(state: State<AppState>, id: i64, name: String) -> Result<usize, String> {
    let conn = state.conn();
    crate::services::categories::rename_category(&conn, id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_category(
    state: State<AppState>,
    id: i64,
    also_rules: Option<bool>,
) -> Result<usize, String> {
    let conn = state.conn();
    crate::services::categories::delete_category(&conn, id, also_rules.unwrap_or(false))
        .map_err(|e| e.to_string())
}

/// 批量删分类：一个事务，要么全删要么都不删。回 (退回未分类的项目数, 被一起删掉的规则条数)
#[tauri::command]
pub fn delete_categories(
    state: State<AppState>,
    ids: Vec<i64>,
    also_rules: Option<bool>,
) -> Result<(usize, usize), String> {
    let conn = state.conn();
    crate::services::categories::delete_categories(&conn, &ids, also_rules.unwrap_or(false))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_rules(state: State<AppState>) -> Result<Vec<DesktopRule>, String> {
    let conn = state.conn();
    crate::services::categories::get_rules(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_rule(state: State<AppState>, rule: DesktopRule) -> Result<DesktopRule, String> {
    let conn = state.conn();
    crate::services::categories::save_rule(&conn, &rule).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_rule(state: State<AppState>, id: i64) -> Result<(), String> {
    let conn = state.conn();
    crate::services::categories::delete_rule(&conn, id).map_err(|e| e.to_string())
}