//! 智能分类的本机记忆：一个软件判过一次就记住，下次既不用问模型也不用联网。
//!
//! 记的东西要短：名字 → 分类 + 一句话依据 + 从哪来的 + 什么时候。
//! 分类规则不参与：那是决定桌面文件挪去哪个文件夹的，跟程序该记成哪类没关系。

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;

use crate::error::AppResult;

const COLS: &str = "name, category, evidence, source, at";
/// 依据就留一句话，界面上扫得到、库里不占地方
const EVIDENCE_CHARS: usize = 90;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub name: String,
    pub category: String,
    pub evidence: String,
    /// `search`（搜索摘要里关键词命中）或 `model`（模型判的）
    pub source: String,
    pub at: i64,
}

fn map(r: &Row) -> rusqlite::Result<Memory> {
    Ok(Memory {
        name: r.get(0)?,
        category: r.get(1)?,
        evidence: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
        source: r.get(3)?,
        at: r.get(4)?,
    })
}

pub fn lookup(conn: &Connection, name: &str) -> AppResult<Option<Memory>> {
    let found = conn
        .query_row(
            &format!("SELECT {COLS} FROM ai_memory WHERE name = ?1"),
            params![name.trim()],
            map,
        )
        .optional()?;
    Ok(found)
}

/// 记下一条判断。同一个名字再记一次是覆盖：判断改了就更新，旧的那条不留着。
pub fn remember(
    conn: &Connection,
    name: &str,
    category: &str,
    evidence: &str,
    source: &str,
) -> AppResult<bool> {
    let name = name.trim();
    let category = category.trim();
    if name.is_empty() || category.is_empty() {
        return Ok(false);
    }
    let mut short: String = evidence.trim().chars().take(EVIDENCE_CHARS).collect();
    if evidence.trim().chars().count() > EVIDENCE_CHARS {
        short.push('…');
    }
    conn.execute(
        "INSERT INTO ai_memory (name, category, evidence, source, at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(name) DO UPDATE SET category = excluded.category,
                                         evidence = excluded.evidence,
                                         source = excluded.source,
                                         at = excluded.at",
        params![name, category, short, source, Utc::now().timestamp()],
    )?;
    Ok(true)
}

pub fn list(conn: &Connection) -> AppResult<Vec<Memory>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM ai_memory ORDER BY at DESC"))?;
    let rows = stmt.query_map([], map)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn clear(conn: &Connection) -> AppResult<usize> {
    Ok(conn.execute("DELETE FROM ai_memory", [])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute(
            "CREATE TABLE ai_memory (
               name TEXT PRIMARY KEY COLLATE NOCASE,
               category TEXT NOT NULL,
               evidence TEXT,
               source TEXT NOT NULL,
               at INTEGER NOT NULL
             )",
            [],
        )
        .unwrap();
        c
    }

    #[test]
    fn a_remembered_software_is_found_again() {
        let c = conn();
        assert!(remember(&c, "Loomy", "AI办公", "Loomy（loomy.ai）", "search").unwrap());

        let hit = lookup(&c, "loomy").unwrap().expect("大小写不同就该也算同一条");
        assert_eq!(hit.category, "AI办公");
        assert_eq!(hit.source, "search");
        assert!(hit.at > 0);
    }

    #[test]
    fn a_second_judgement_overwrites_the_first() {
        let c = conn();
        remember(&c, "WorkBuddy", "AI办公", "旧的", "search").unwrap();
        remember(&c, "WorkBuddy", "开发工具", "新的", "model").unwrap();

        let all = list(&c).unwrap();
        assert_eq!(all.len(), 1, "{all:?}");
        assert_eq!(all[0].category, "开发工具");
        assert_eq!(all[0].source, "model");
    }

    #[test]
    fn evidence_is_kept_to_one_line() {
        let c = conn();
        let long = "很".repeat(300);
        remember(&c, "X", "工具", &long, "search").unwrap();

        let hit = lookup(&c, "X").unwrap().unwrap();
        assert!(hit.evidence.chars().count() <= EVIDENCE_CHARS + 1, "{}", hit.evidence.chars().count());
        assert!(hit.evidence.ends_with('…'));
    }

    #[test]
    fn empty_names_and_categories_are_refused_and_clear_empties() {
        let c = conn();
        assert!(!remember(&c, "  ", "工具", "", "search").unwrap());
        assert!(!remember(&c, "X", "  ", "", "search").unwrap());
        assert_eq!(clear(&c).unwrap(), 0);

        remember(&c, "X", "工具", "", "search").unwrap();
        assert_eq!(clear(&c).unwrap(), 1);
        assert!(lookup(&c, "X").unwrap().is_none());
    }
}
