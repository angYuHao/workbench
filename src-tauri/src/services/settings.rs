//! `settings` 那张 KV：开关（如「过滤系统应用」）、路径清单，以及别的模块借来存的本机状态
//! （远端模型列表缓存、网页信息缓存都塞在这儿）。
//!
//! 这张表是「加一个键」的出口：本项目没有建表迁移机制，给已有的表加列在老库里加不出来，
//! 所以新状态一律优先落到这里或者新开一张表。
use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::AppResult;

pub fn get_setting(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    let v = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
    Ok(v)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn get_all_settings(conn: &Connection) -> AppResult<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (k, v) = row?;
        map.insert(k, v);
    }
    Ok(map)
}