//! 拉回来的远端模型列表缓存在 settings 那张 KV 里。
//! 不动 `ai_models` 的表结构——这项目没有建表迁移机制，加列等于老库读不出。

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::AppResult;
use crate::models::AiModel;

/// 拉回来的列表存进 settings 那张 KV，不动 ai_models 的表结构（这项目没有建表迁移机制）
pub(crate) fn cache_key(id: i64) -> String {
    format!("ai_model_cache:{id}")
}

#[derive(Serialize, Deserialize)]
struct Cache {
    at: i64,
    models: Vec<String>,
}

/// 记下这次获取的列表和时刻，下次点开编辑就不用再打网络
pub fn remember_models(conn: &Connection, id: i64, models: &[String]) -> AppResult<()> {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let value = serde_json::to_string(&Cache { at, models: models.to_vec() })?;
    crate::services::settings::set_setting(conn, &cache_key(id), &value)
}

pub(crate) fn forget_models(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM settings WHERE key = ?1", params![cache_key(id)])?;
    Ok(())
}

pub(crate) fn fill_cache(conn: &Connection, model: &mut AiModel) -> AppResult<()> {
    if let Some(raw) = crate::services::settings::get_setting(conn, &cache_key(model.id))? {
        // 认不出来就当没有缓存，大不了让他重新点一次获取
        if let Ok(cache) = serde_json::from_str::<Cache>(&raw) {
            model.remote_models = cache.models;
            model.remote_at = Some(cache.at);
        }
    }
    Ok(())
}

