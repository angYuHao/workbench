//! 挪文件与回退：整理时把文件搬进「桌面整理/<分类>」并在 `organize_logs` 记一行，
//! 回退就按那一行的 `from_path`/`to_path` 原路搬回，只删被搬空的目录。

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{AppError, AppResult};
use crate::models::{OrganizeLog, OrganizePlan};

use super::ORGANIZE_DIR;

const ORGANIZE_LOG_COLUMNS: &str = "id, file_name, from_path, to_path, organized_at, reverted";

pub fn organize_desktop(conn: &Connection, plans: &[OrganizePlan]) -> AppResult<Vec<OrganizeLog>> {
    let desktop = crate::win::desktop::user_desktop()
        .ok_or_else(|| AppError::msg("无法获取桌面目录"))?;
    organize_into(conn, plans, Path::new(&desktop))
}

/// 真正干活的一层：`desktop` 当桌面根目录用，测试传临时目录进来就能真的跑一遍搬移。
/// 只搬文件 + 记流水，**不往 `apps` 里登记**：桌面上的东西什么都有（文档、压缩包、图片），
/// 塞进启动器就等于把「本机装了什么程序」那一页污染掉，两边各管各的表。
pub(crate) fn organize_into(
    conn: &Connection,
    plans: &[OrganizePlan],
    desktop: &Path,
) -> AppResult<Vec<OrganizeLog>> {
    let base = desktop.join(ORGANIZE_DIR);

    let mut logs = Vec::new();
    for plan in plans {
        let src = Path::new(&plan.path);
        let file_name = src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if file_name.is_empty() {
            continue;
        }

        std::fs::create_dir_all(&base)?;
        let cat_dir = base.join(&plan.category);
        std::fs::create_dir_all(&cat_dir)?;

        let dest = free_dest(&cat_dir, src, &file_name);

        std::fs::rename(src, &dest)?;
        let to_path = dest.to_string_lossy().to_string();

        conn.execute(
            "INSERT INTO organize_logs (file_name, from_path, to_path) VALUES (?1, ?2, ?3)",
            params![file_name, plan.path, to_path],
        )?;
        let log_id = conn.last_insert_rowid();
        logs.push(OrganizeLog {
            id: log_id,
            file_name: Some(file_name.clone()),
            from_path: Some(plan.path.clone()),
            to_path: Some(to_path.clone()),
            organized_at: None,
            reverted: false,
        });
    }
    Ok(logs)
}

/// 归档目录里已经有同名文件时按 (1)、(2) 递增改名，绝不覆盖用户的东西
pub(crate) fn free_dest(dir: &Path, src: &Path, file_name: &str) -> std::path::PathBuf {
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = src
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    let mut dest = dir.join(file_name);
    let mut n = 1;
    while dest.exists() {
        dest = dir.join(format!("{} ({}){}", stem, n, ext));
        n += 1;
    }
    dest
}
pub fn revert_organize(conn: &Connection, log_id: Option<i64>) -> AppResult<usize> {
    let logs: Vec<OrganizeLog> = match log_id {
        Some(id) => conn
            .query_row(
                &format!(
                    "SELECT {ORGANIZE_LOG_COLUMNS} FROM organize_logs WHERE id = ?1 AND reverted = 0"
                ),
                params![id],
                map_organize_log,
            )
            .optional()?
            .into_iter()
            .collect(),
        None => {
            let mut stmt = conn.prepare(&format!(
                "SELECT {ORGANIZE_LOG_COLUMNS} FROM organize_logs WHERE reverted = 0 ORDER BY id DESC"
            ))?;
            let rows = stmt.query_map([], map_organize_log)?;
            rows.collect::<Result<Vec<_>, _>>()?
        }
    };

    let mut count = 0;
    let mut vacated: Vec<std::path::PathBuf> = Vec::new();
    for log in logs {
        let (Some(from), Some(to)) = (&log.from_path, &log.to_path) else {
            continue;
        };
        if !Path::new(to).exists() {
            // 副本已经不在分类目录里了（用户自己挪走或删了），只补记状态
            mark_reverted(conn, log.id)?;
            continue;
        }
        // 原位置已被占用则删掉副本，否则移回原位置
        let restored = if Path::new(from).exists() {
            std::fs::remove_file(to).is_ok()
        } else {
            std::fs::rename(to, from).is_ok()
        };
        if restored {
            mark_reverted(conn, log.id)?;
            count += 1;
            if let Some(cat_dir) = Path::new(to).parent() {
                if cat_dir.parent().and_then(|p| p.file_name()).map(is_organize_base_name) == Some(true) {
                    vacated.push(cat_dir.to_path_buf());
                }
            }
        }
    }
    prune_empty_dirs(&vacated);
    Ok(count)
}

fn is_organize_base_name(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().eq_ignore_ascii_case(ORGANIZE_DIR)
}

/// 只删被搬空的分类目录，非空一律留着；顺带把只剩这些空目录的「桌面整理」也收掉
pub(crate) fn prune_empty_dirs(dirs: &[std::path::PathBuf]) {
    let is_empty = |p: &Path| p.is_dir() && p.read_dir().map(|mut r| r.next().is_none()).unwrap_or(false);
    let mut bases: Vec<&std::path::Path> = Vec::new();
    for dir in dirs {
        if is_empty(dir) && std::fs::remove_dir(dir).is_ok() {
            if let Some(base) = dir.parent() {
                bases.push(base);
            }
        }
    }
    for base in bases {
        if is_empty(base) {
            let _ = std::fs::remove_dir(base);
        }
    }
}

fn mark_reverted(conn: &Connection, log_id: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE organize_logs SET reverted = 1 WHERE id = ?1",
        params![log_id],
    )?;
    Ok(())
}
fn map_organize_log(r: &Row) -> rusqlite::Result<OrganizeLog> {
    let reverted: i64 = r.get(5)?;
    Ok(OrganizeLog {
        id: r.get(0)?,
        file_name: r.get(1)?,
        from_path: r.get(2)?,
        to_path: r.get(3)?,
        organized_at: r.get(4)?,
        reverted: reverted != 0,
    })
}

