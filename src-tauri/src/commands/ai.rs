use tauri::State;

use crate::db::AppState;
use crate::models::{AiModel, AiModelInput, LogView};

#[tauri::command]
pub fn get_ai_models(state: State<AppState>) -> Result<Vec<AiModel>, String> {
    let conn = state.conn();
    crate::services::ai::models::get_models(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_ai_model(state: State<AppState>, model: AiModelInput) -> Result<AiModel, String> {
    let conn = state.conn();
    crate::services::ai::models::save_model(&conn, &model).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_ai_model(state: State<AppState>, id: i64) -> Result<(), String> {
    let conn = state.conn();
    crate::services::ai::models::delete_model(&conn, id).map_err(|e| e.to_string())
}

/// 拉远端模型列表，拉到就顺手存本机（下次编辑直接用，不再打网络）。
/// async 是为了不在主线程上做网络等待；连接只用在取出配置和写缓存这两步，
/// 中间等网络时不占着库。
#[tauri::command]
pub async fn list_remote_ai_models(
    state: State<'_, AppState>,
    id: Option<i64>,
    base_url: String,
    api_key: Option<String>,
) -> Result<Vec<String>, String> {
    let target = {
        let conn = state.conn();
        crate::services::ai::models::list_target(&conn, id, &base_url, api_key.as_deref())
    }
    .map_err(|e| e.to_string())?;
    let models = crate::services::ai::models::fetch_models(&target)
        .await
        .map_err(|e| e.to_string())?;
    // 只有已经存下来的配置才有地方挂：新建那一条还没有 id
    if let Some(id) = id {
        let conn = state.conn();
        crate::services::ai::models::remember_models(&conn, id, &models).map_err(|e| e.to_string())?;
    }
    Ok(models)
}

/// 测密钥能不能真的调用模型（发一条极短的补全，会花几个 token）
#[tauri::command]
pub async fn probe_ai_model(
    state: State<'_, AppState>,
    id: Option<i64>,
    base_url: String,
    api_key: Option<String>,
    model: Option<String>,
) -> Result<crate::services::ai::models::ProbeResult, String> {
    let target = {
        let conn = state.conn();
        crate::services::ai::models::call_target(
            &conn,
            id,
            &base_url,
            api_key.as_deref(),
            model.as_deref(),
        )
    }
    .map_err(|e| e.to_string())?;
    crate::services::ai::models::probe(&target).await.map_err(|e| e.to_string())
}

/// 打开调用日志（每次 AI 请求的状态、耗时、请求与响应开头），顺手把路径回给界面
#[tauri::command]
pub fn open_ai_log() -> Result<String, String> {
    let path = crate::services::ai::log::path();
    if !path.exists() {
        let _ = std::fs::write(&path, "还没有 AI 调用记录：发一次「测试调用」或「让 AI 建议」后再来看。\n");
    }
    let text = path.to_string_lossy().to_string();
    crate::win::shell::open(&text).map_err(|e| e.to_string())?;
    Ok(text)
}

/// 设置里看的那一屏：只读最近 100 行，行数由后端定，前端不用知道
#[tauri::command]
pub fn read_ai_log() -> Result<LogView, String> {
    crate::services::ai::log::tail(crate::services::ai::log::VIEW_LINES).map_err(|e| e.to_string())
}
