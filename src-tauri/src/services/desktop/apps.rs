//! 「应用」页卡片的读写与启动。卡片数据是 `services::installed` 同步进来的，
//! 这一层只管人工改名、删除和双击那一下。

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{AppError, AppResult};
use crate::models::AppEntry;


pub fn launch_by_id(conn: &Connection, id: i64) -> AppResult<()> {
    let target: Option<String> = conn
        .query_row("SELECT target FROM apps WHERE id = ?1", params![id], |r| r.get(0))
        .optional()?;
    // 走 id 不走 target：计数得落在数据库这一行上，前端传回来的路径不能当键用
    let Some(target) = target else {
        return Err(AppError::msg(format!("没有 id 为 {id} 的程序")));
    };
    // 先真打开，再记数：计数写不进去（磁盘满、库被占）不该挡住用户要的那一下
    crate::win::shell::open(&target)?;
    let _ = crate::services::usage::note_use(conn, crate::services::usage::APP, id);
    Ok(())
}

pub fn get_apps(conn: &Connection) -> AppResult<Vec<AppEntry>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.name, a.alias, a.description, a.target, a.args, a.icon_path, a.category_id, a.source, a.created_at,
                COALESCE(u.uses, 0)
         FROM apps a
         LEFT JOIN usage_stats u ON u.kind = 'app' AND u.id = a.id
         ORDER BY COALESCE(u.uses, 0) DESC, a.name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(AppEntry {
            id: r.get(0)?,
            name: r.get(1)?,
            alias: r.get(2)?,
            description: r.get(3)?,
            target: r.get(4)?,
            args: r.get(5)?,
            icon_path: r.get(6)?,
            category_id: r.get(7)?,
            source: r.get(8)?,
            created_at: r.get(9)?,
            use_count: r.get(10)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn save_app(conn: &Connection, app: &AppEntry) -> AppResult<AppEntry> {
    if app.id > 0 {
        conn.execute(
            "UPDATE apps SET name = ?1, alias = ?2, description = ?3, target = ?4, args = ?5,
             icon_path = ?6, category_id = ?7 WHERE id = ?8",
            params![
                app.name, app.alias, app.description, app.target, app.args, app.icon_path,
                app.category_id, app.id
            ],
        )?;
        Ok(app.clone())
    } else {
        conn.execute(
            "INSERT INTO apps (name, alias, description, target, args, icon_path, category_id, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'manual')",
            params![
                app.name, app.alias, app.description, app.target, app.args, app.icon_path,
                app.category_id
            ],
        )?;
        let id = conn.last_insert_rowid();
        Ok(AppEntry { id, ..app.clone() })
    }
}

pub fn delete_app(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM apps WHERE id = ?1", params![id])?;
    crate::services::usage::forget(conn, crate::services::usage::APP, id)
}

