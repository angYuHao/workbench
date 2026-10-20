use std::sync::{Mutex, MutexGuard};

use rusqlite::{params, Connection};

use crate::error::{AppError, AppResult};
use crate::filewatcher::FileWatcher;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub db_path: std::path::PathBuf,
    pub watcher: Mutex<Option<FileWatcher>>,
}

impl AppState {
    /// 取数据库连接。命令层是单线程串行访问，只有 service panic 才会中毒，
    /// 那种情况下应用已不可用，直接沿用中毒前的连接即可。
    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap()
    }
}

/// 提交本项目手写的事务（`execute_batch("BEGIN")` 那一套）。
/// COMMIT 自己失败时（SQLITE_BUSY 等）SQLite 是把事务留在原地的，不补一句 ROLLBACK
/// 它就一直挂在共享连接上，下一次 `BEGIN` 直接报「已在事务里」，之后所有写操作一起坏。
pub fn commit(conn: &Connection) -> AppResult<()> {
    match conn.execute_batch("COMMIT") {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e.into())
        }
    }
}

pub fn init() -> AppResult<AppState> {
    let data_dir = dirs::data_dir().ok_or_else(|| AppError::msg("无法获取 APPDATA 目录"))?;
    let dir = data_dir.join("workbench");
    std::fs::create_dir_all(&dir)?;
    let db_path = dir.join("workbench.db");

    let conn = Connection::open(&db_path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(include_str!("schema.sql"))?;
    seed(&conn)?;

    Ok(AppState {
        db: Mutex::new(conn),
        db_path,
        watcher: Mutex::new(None),
    })
}

fn seed(conn: &Connection) -> AppResult<()> {
    let app_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM categories WHERE type = 'app'", [], |r| {
            r.get(0)
        })?;
    if app_count == 0 {
        for (i, name) in ["办公软件", "开发工具", "文档", "压缩包", "图片"]
            .iter()
            .enumerate()
        {
            conn.execute(
                "INSERT INTO categories (name, type, sort) VALUES (?1, 'app', ?2)",
                params![name, i as i64],
            )?;
        }
    }

    let file_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM categories WHERE type = 'file'", [], |r| {
            r.get(0)
        })?;
    if file_count == 0 {
        conn.execute(
            "INSERT INTO categories (name, type, sort) VALUES ('默认', 'file', 0)",
            [],
        )?;
    }

    let rule_count: i64 = conn.query_row("SELECT COUNT(*) FROM desktop_rules", [], |r| r.get(0))?;
    if rule_count == 0 {
        let rules = [
            ("办公软件", "word,excel,ppt,wps,onenote", ""),
            ("开发工具", "code,idea,git,terminal,postman", ""),
            ("文档", "", ".docx,.pdf,.xlsx,.pptx,.txt,.md"),
            ("压缩包", "", ".zip,.rar,.7z"),
            ("图片", "", ".png,.jpg,.jpeg,.gif"),
        ];
        for (i, (cat, kw, ext)) in rules.iter().enumerate() {
            conn.execute(
                "INSERT INTO desktop_rules (category, keywords, extensions, sort) VALUES (?1, ?2, ?3, ?4)",
                params![cat, kw, ext, i as i64],
            )?;
        }
    }

    conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('managed_folders', '[]')",
        [],
    )?;
    Ok(())
}