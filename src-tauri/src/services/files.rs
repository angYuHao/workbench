//! 「文件」页：受管文件夹清单、目录列表、每条文件的人工标注，以及预览要的文本读取。
//!
//! 目录内容不落库（每次现读磁盘），库里只存「这个路径的备注/分类」那一层标注。
//! `preview_roots` 同时是 webview 能看见哪些目录的唯一决定处：预览要读磁盘上的文件，
//! 但 asset scope 静态是空的，启动时按「两个桌面根 + 图标缓存 + 每个受管文件夹」逐条授权。
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::AppResult;
use crate::models::{FileNode, FileRecord, ReadTextResult};

pub fn get_managed_folders(conn: &Connection) -> AppResult<Vec<String>> {
    let raw = super::settings::get_setting(conn, "managed_folders")?
        .unwrap_or_else(|| "[]".to_string());
    let list: Vec<String> = serde_json::from_str(&raw)?;
    Ok(list)
}

pub fn add_managed_folder(conn: &Connection, path: &str) -> AppResult<Vec<String>> {
    let mut list = get_managed_folders(conn)?;
    if !list.iter().any(|p| p == path) {
        list.push(path.to_string());
        super::settings::set_setting(conn, "managed_folders", &serde_json::to_string(&list)?)?;
    }
    Ok(list)
}

pub fn remove_managed_folder(conn: &Connection, path: &str) -> AppResult<Vec<String>> {
    let mut list = get_managed_folders(conn)?;
    list.retain(|p| p != path);
    super::settings::set_setting(conn, "managed_folders", &serde_json::to_string(&list)?)?;
    Ok(list)
}

/// webview 能读磁盘的哪些地方：桌面（扫描和预览都在这儿）、抽出来的图标缓存、
/// 用户自己登记的受管文件夹。`tauri.conf.json` 里静态 scope 是空的，
/// 这份名单在启动时逐条 `allow_directory`，新登记文件夹时当场补一条。
pub fn preview_roots(conn: &Connection) -> AppResult<Vec<String>> {
    let mut roots: Vec<String> = crate::win::desktop::all_desktops()
        .into_iter()
        .chain(get_managed_folders(conn)?)
        .collect();
    if let Ok(icons) = crate::services::desktop::get_icon_dir() {
        roots.push(icons.to_string_lossy().to_string());
    }
    roots.sort();
    roots.dedup();
    Ok(roots)
}

pub fn list_files(conn: &Connection, root: &str) -> AppResult<Vec<FileNode>> {
    let mut nodes = Vec::new();
    let entries = std::fs::read_dir(root)?;
    let meta = all_file_meta(conn)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();
        let is_dir = path.is_dir();
        let (alias, description) = meta.get(&path_str).cloned().unwrap_or((None, None));
        nodes.push(FileNode {
            name: entry.file_name().to_string_lossy().to_string(),
            path: path_str,
            is_dir,
            alias,
            description,
            children: None,
        });
    }
    nodes.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(nodes)
}

/// 别名和说明整表一次读出来。挨个条目单查是 N+1：一个两千项的文件夹就得在库锁里跑两千条查询，
/// 而这张表本来就只存人工写的那点文字，全量读回来比逐条查更便宜
/// 库里给一个文件存的人工标注：别名 + 说明，两头都可能没填
type FileMeta = (Option<String>, Option<String>);

fn all_file_meta(conn: &Connection) -> AppResult<HashMap<String, FileMeta>> {
    let mut stmt = conn.prepare("SELECT path, alias, description FROM files")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            (r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?),
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get_file(conn: &Connection, path: &str) -> AppResult<FileRecord> {
    let row = conn
        .query_row(
            "SELECT id, path, name, alias, description, is_dir, managed_root, category_id, created_at
             FROM files WHERE path = ?1",
            params![path],
            |r| {
                let is_dir: i64 = r.get(5)?;
                Ok(FileRecord {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    name: r.get(2)?,
                    alias: r.get(3)?,
                    description: r.get(4)?,
                    is_dir: is_dir != 0,
                    managed_root: r.get(6)?,
                    category_id: r.get(7)?,
                    created_at: r.get(8)?,
                })
            },
        )
        .optional()?;

    // 没登记过的文件返回一条 id=0 的空记录，前端仍可预览
    Ok(row.unwrap_or_else(|| FileRecord {
        id: 0,
        path: path.to_string(),
        name: file_name_of(path),
        alias: None,
        description: None,
        is_dir: Path::new(path).is_dir(),
        managed_root: None,
        category_id: None,
        created_at: None,
    }))
}

/// 路径最后一段，空路径时退回原串
fn file_name_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

pub fn save_file_meta(
    conn: &Connection,
    path: &str,
    alias: Option<String>,
    description: Option<String>,
) -> AppResult<FileRecord> {
    let p = Path::new(path);
    conn.execute(
        "INSERT INTO files (path, name, alias, description, is_dir) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(path) DO UPDATE SET
           name = excluded.name,
           alias = excluded.alias,
           description = excluded.description,
           is_dir = excluded.is_dir",
        params![
            path,
            file_name_of(path),
            alias,
            description,
            p.is_dir() as i64
        ],
    )?;
    get_file(conn, path)
}

pub fn read_text_file(path: &str) -> AppResult<ReadTextResult> {
    const LIMIT: u64 = 1024 * 1024;
    let size = std::fs::metadata(path)?.len();
    let mut buf = Vec::with_capacity(size.min(LIMIT) as usize);
    std::fs::File::open(path)?
        .take(size.min(LIMIT))
        .read_to_end(&mut buf)?;
    Ok(ReadTextResult {
        content: String::from_utf8_lossy(&buf).to_string(),
        truncated: size > LIMIT,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(include_str!("../db/schema.sql")).unwrap();
        c
    }

    fn sandbox(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("workbench-files-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_listed_dir_shows_the_alias_registered_for_its_files() {
        let dir = sandbox("list");
        std::fs::create_dir_all(dir.join("Z目录")).unwrap();
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        let c = conn();
        let a = dir.join("a.txt").to_string_lossy().to_string();
        save_file_meta(&c, &a, Some("甲".into()), Some("周报底稿".into())).unwrap();

        let nodes = list_files(&c, &dir.to_string_lossy()).unwrap();
        assert_eq!(
            nodes.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
            vec!["Z目录", "a.txt"],
            "目录排前面，名字按不区分大小写排"
        );
        assert_eq!(nodes[1].alias.as_deref(), Some("甲"));
        assert_eq!(nodes[1].description.as_deref(), Some("周报底稿"));
        assert_eq!(nodes[0].alias, None, "没登记过的就是空，不该串到别条上");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_roots_list_desktops_icons_and_managed_folders() {
        let c = conn();
        add_managed_folder(&c, "D:/资料").unwrap();
        let roots = preview_roots(&c).unwrap();

        assert!(
            roots.iter().any(|r| r == "D:/资料"),
            "登记的受管文件夹要在授权名单里，否则预览读不到它"
        );
        let icons = crate::services::desktop::get_icon_dir().unwrap();
        assert!(
            roots.iter().any(|r| r == &icons.to_string_lossy().to_string()),
            "图标缓存要在名单里，不然启动器一片没有图标"
        );
        assert!(
            roots.windows(2).all(|w| w[0] <= w[1]),
            "排好去重，免得同一个目录授权两遍"
        );
    }

    #[test]
    fn saving_meta_twice_updates_the_same_row() {
        let dir = sandbox("meta");
        std::fs::write(dir.join("b.md"), b"# hi").unwrap();
        let c = conn();
        let p = dir.join("b.md").to_string_lossy().to_string();
        save_file_meta(&c, &p, Some("第一版".into()), None).unwrap();
        save_file_meta(&c, &p, Some("第二版".into()), Some("说明".into())).unwrap();

        let rows: i64 = c
            .query_row("SELECT COUNT(*) FROM files WHERE path = ?1", params![p], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1, "path 有唯一约束，重复保存得落在同一行上");
        let rec = get_file(&c, &p).unwrap();
        assert_eq!(rec.alias.as_deref(), Some("第二版"));
        assert_eq!(rec.name, "b.md");
        let _ = std::fs::remove_dir_all(&dir);
    }
}