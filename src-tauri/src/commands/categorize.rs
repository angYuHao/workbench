use tauri::State;

use crate::db::AppState;
use crate::models::{ApplyCategorizeUpdate, SmartCategorizeItem, SmartCategorizeResult};
use crate::services::ai::{categorize, memory};

/// async 是有意的：模型一轮要等十几秒，同步命令会跑在主线程上把界面钉住。
/// 四步判断（规则 → 记忆 → 模型 → 联网搜）整个在 `services::ai::categorize::run` 里；
/// 命令层只负责取锁跑 `prepare`、放锁之后再 `execute`，MutexGuard 绝不跨 `.await`。
#[tauri::command]
pub async fn smart_categorize(
    state: State<'_, AppState>,
    items: Vec<SmartCategorizeItem>,
    item_type: String,
    model_id: Option<i64>,
) -> Result<Vec<SmartCategorizeResult>, String> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let plan = {
        let conn = state.conn();
        categorize::prepare(&conn, &items, &item_type, model_id).map_err(|e| e.to_string())?
    };
    categorize::execute(plan, &items).await.map_err(|e| e.to_string())
}

/// 本机记忆：判过一次就记住的软件。界面上要能看见记了什么、从哪来的。
#[tauri::command]
pub fn get_ai_memory(state: State<AppState>) -> Result<Vec<memory::Memory>, String> {
    let conn = state.conn();
    memory::list(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_ai_memory(state: State<AppState>) -> Result<usize, String> {
    let conn = state.conn();
    memory::clear(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn apply_smart_categorize(
    state: State<AppState>,
    item_type: String,
    updates: Vec<ApplyCategorizeUpdate>,
) -> Result<usize, String> {
    let conn = state.conn();
    categorize::apply(&conn, &item_type, &updates).map_err(|e| e.to_string())
}
