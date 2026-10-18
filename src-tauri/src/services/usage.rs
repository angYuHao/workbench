use rusqlite::{params, Connection};

use crate::error::AppResult;

/// `usage_stats.kind` 的取值：桌面程序挂在 apps 的 id 上，网址挂在 bookmarks 的 id 上
pub const APP: &str = "app";
pub const BOOKMARK: &str = "bookmark";

/// 双击打开一次记一次。UPSERT：没记过就插 1，记过就加 1
pub fn note_use(conn: &Connection, kind: &str, id: i64) -> AppResult<()> {
    conn.execute(
        "INSERT INTO usage_stats (kind, id, uses) VALUES (?1, ?2, 1)
         ON CONFLICT (kind, id) DO UPDATE SET uses = uses + 1",
        params![kind, id],
    )?;
    Ok(())
}

/// 条目被用户删掉时把计数一起带走，不然这张表只会一直长。
/// 注意智能分析删重复不走这里：那条有回收站，将来恢复回来时历史热度该跟着回来。
pub fn forget(conn: &Connection, kind: &str, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM usage_stats WHERE kind = ?1 AND id = ?2", params![kind, id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute(
            "CREATE TABLE usage_stats (kind TEXT NOT NULL, id INTEGER NOT NULL, uses INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (kind, id))",
            [],
        )
        .unwrap();
        c
    }

    fn uses(c: &Connection, kind: &str, id: i64) -> i64 {
        c.query_row(
            "SELECT uses FROM usage_stats WHERE kind = ?1 AND id = ?2",
            params![kind, id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn first_click_plants_the_row_and_later_ones_add_up() {
        let c = conn();
        note_use(&c, APP, 7).unwrap();
        assert_eq!(uses(&c, APP, 7), 1);
        note_use(&c, APP, 7).unwrap();
        note_use(&c, APP, 7).unwrap();
        assert_eq!(uses(&c, APP, 7), 3, "同一个程序反复开要累加，不能插出两行");
    }

    #[test]
    fn same_id_under_different_kind_stays_separate() {
        let c = conn();
        note_use(&c, APP, 1).unwrap();
        note_use(&c, APP, 1).unwrap();
        note_use(&c, BOOKMARK, 1).unwrap();
        assert_eq!((uses(&c, APP, 1), uses(&c, BOOKMARK, 1)), (2, 1), "两张表的 id 会撞号，靠 kind 分开");
    }

    #[test]
    fn forget_takes_the_counter_with_the_row() {
        let c = conn();
        note_use(&c, BOOKMARK, 5).unwrap();
        note_use(&c, BOOKMARK, 5).unwrap();
        forget(&c, BOOKMARK, 5).unwrap();

        let left: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM usage_stats WHERE kind = ?1 AND id = ?2",
                params![BOOKMARK, 5],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 0, "删掉条目不该留一条永远排不到、也看不见的计数");
        // 同一个 id 再记一次从头开始，不复活旧数字
        note_use(&c, BOOKMARK, 5).unwrap();
        assert_eq!(uses(&c, BOOKMARK, 5), 1);
    }
}
