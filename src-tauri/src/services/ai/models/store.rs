//! 库里的模型行：读、存、删，以及把行映射成对外的 `AiModel`。
//! 密钥列 `key_enc` 是当前 Windows 用户的 DPAPI 密文，永远不出库。

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{AppError, AppResult};
use crate::models::{AiModel, AiModelInput};
use crate::win::dpapi;

use super::cache::{cache_key, fill_cache, forget_models};

const MODEL_COLUMNS: &str = "id, name, base_url, model, key_enc, sort";

pub fn get_models(conn: &Connection) -> AppResult<Vec<AiModel>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MODEL_COLUMNS} FROM ai_models ORDER BY sort, id"
    ))?;
    let rows = stmt.query_map([], map_model)?;
    let mut out = rows.collect::<Result<Vec<_>, _>>()?;
    for model in &mut out {
        fill_cache(conn, model)?;
    }
    Ok(out)
}

pub fn save_model(conn: &Connection, input: &AiModelInput) -> AppResult<AiModel> {
    let name = input.name.trim();
    let base_url = input.base_url.trim().trim_end_matches('/');
    let model = input.model.trim();
    if name.is_empty() || base_url.is_empty() || model.is_empty() {
        return Err(AppError::msg("名称、接口地址、模型名都要填"));
    }
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        return Err(AppError::msg("接口地址要带 http:// 或 https://"));
    }
    let typed = input
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty());
    let sort = input.sort.unwrap_or(0);

    let id = if input.id > 0 {
        let stored: (Option<Vec<u8>>, String) = conn
            .query_row(
                "SELECT key_enc, base_url FROM ai_models WHERE id = ?1",
                params![input.id],
                |r| Ok((r.get::<_, Option<Vec<u8>>>(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::msg("要编辑的模型不存在"))?;
        let blob = match typed {
            Some(key) => Some(dpapi::protect(key.as_bytes())?),
            None => stored.0,
        };
        // 接口地址换了，上次拉回来的列表就是别家的了，留着会骗人
        if stored.1.trim_end_matches('/') != base_url {
            conn.execute(
                "DELETE FROM settings WHERE key = ?1",
                params![cache_key(input.id)],
            )?;
        }
        conn.execute(
            "UPDATE ai_models SET name = ?1, base_url = ?2, model = ?3, key_enc = ?4, sort = ?5 WHERE id = ?6",
            params![name, base_url, model, blob, sort, input.id],
        )?;
        input.id
    } else {
        let blob = typed.map(|key| dpapi::protect(key.as_bytes())).transpose()?;
        conn.execute(
            "INSERT INTO ai_models (name, base_url, model, key_enc, sort) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, base_url, model, blob, sort],
        )?;
        conn.last_insert_rowid()
    };

    get_model(conn, id)
}

pub fn delete_model(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM ai_models WHERE id = ?1", params![id])?;
    // 配置都没了，它那份列表也就不用留在 settings 里占着
    forget_models(conn, id)
}

pub(crate) fn get_model(conn: &Connection, id: i64) -> AppResult<AiModel> {
    let mut model = conn
        .query_row(
            &format!("SELECT {MODEL_COLUMNS} FROM ai_models WHERE id = ?1"),
            params![id],
            map_model,
        )
        .optional()?
        .ok_or_else(|| AppError::msg("模型不存在"))?;
    fill_cache(conn, &mut model)?;
    Ok(model)
}
fn map_model(r: &Row) -> rusqlite::Result<AiModel> {
    let blob: Option<Vec<u8>> = r.get(4)?;
    Ok(AiModel {
        id: r.get(0)?,
        name: r.get(1)?,
        base_url: r.get(2)?,
        model: r.get(3)?,
        has_key: blob.is_some(),
        sort: r.get(5)?,
        remote_models: Vec::new(),
        remote_at: None,
    })
}
