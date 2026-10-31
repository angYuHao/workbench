//! 「文件」页：整台机器的目录列表、每条文件的人工标注，以及预览要的字节/文本读取。
//!
//! 目录内容不落库（每次现读磁盘），库里只存「这个路径的备注/分类」那一层标注。
//! 这一页不再挑「哪些目录算受管的」——任何能 read_dir 的路径都能摆出来，侧栏只是给几个入口。
//! `preview_roots` 收窄成启动时先授权的那几处（两个桌面 + 图标缓存，桌面页要用），
//! 文件页其余目录是用户点到哪一次，`commands::files::authorize_preview` 当场授到哪一次。
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::time::SystemTime;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{AppError, AppResult};
use crate::models::{DriveInfo, FileNode, FileRecord, KnownFolder, ReadTextResult};

/// 本机有哪些盘，按盘符顺序
pub fn list_drives() -> Vec<DriveInfo> {
    crate::win::drive::list_drives()
}

/// 侧栏「常用文件夹」那几行：拿不到的（被组策略关掉、这台机器没建过）就不出现
pub fn known_folders() -> Vec<KnownFolder> {
    let mut out: Vec<KnownFolder> = Vec::new();
    for (name, path) in [
        ("桌面", crate::win::desktop::user_desktop()),
        ("下载", crate::win::desktop::user_downloads()),
        ("文档", crate::win::desktop::user_documents()),
        ("图片", crate::win::desktop::user_pictures()),
        ("音乐", crate::win::desktop::user_music()),
        ("视频", crate::win::desktop::user_videos()),
    ] {
        if let Some(path) = path {
            out.push(KnownFolder { name: name.to_string(), path });
        }
    }
    out
}

/// webview 一开始就能读的那几处：桌面（扫描和预览都在这儿）和抽出来的图标缓存。
/// `tauri.conf.json` 里静态 scope 是空的，这份名单在启动时逐条 `allow_directory`；
/// 用户后来在文件页点开的目录由 `authorize_preview` 当场补，不在这儿。
pub fn preview_roots(conn: &Connection) -> AppResult<Vec<String>> {
    let mut roots: Vec<String> = crate::win::desktop::all_desktops().into_iter().collect();
    if let Ok(icons) = crate::services::desktop::get_icon_dir() {
        roots.push(icons.to_string_lossy().to_string());
    }
    let _ = conn;
    roots.sort();
    roots.dedup();
    Ok(roots)
}

/// 隐藏 / 系统属性。值按 SDK 的 FILE_ATTRIBUTE_* 定义写死
#[cfg(windows)]
fn is_hidden(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    let attrs = meta.file_attributes();
    attrs & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0
}

/// 非 Windows 上没有这两个属性位，测试也就在这儿跑，别造一个假的不隐藏
#[cfg(not(windows))]
fn is_hidden(_: &std::fs::Metadata) -> bool {
    false
}

fn modified_secs(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

/// 列一个目录。`include_hidden` 关掉时把隐藏/系统项滤掉（默认关），
/// 这样这页打开 `C:\` 不至于先看见一屏 `$Recycle.Bin`。
pub fn list_files(conn: &Connection, root: &str, include_hidden: bool) -> AppResult<Vec<FileNode>> {
    let mut nodes = Vec::new();
    let entries = std::fs::read_dir(root)?;
    let meta = all_file_meta(conn)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();
        let stat = entry.metadata().ok();
        let is_dir = stat.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let hidden = stat.as_ref().map(is_hidden).unwrap_or(false);
        if hidden && !include_hidden {
            continue;
        }
        let size = if is_dir { None } else { stat.as_ref().map(|m| m.len()) };
        let modified_at = stat.as_ref().and_then(modified_secs);
        let (alias, description) = meta.get(&path_str).cloned().unwrap_or((None, None));
        nodes.push(FileNode {
            name: entry.file_name().to_string_lossy().to_string(),
            path: path_str,
            is_dir,
            alias,
            description,
            size,
            modified_at,
            hidden,
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

/// 文件页的行图标：跟启动器走同一条 shell 抽图路径，所以文件夹、盘符、每种文件拿到的
/// 就是资源管理器里那张图（.lnk 的 IconLocation 那套 shell 已经算好了）。
/// 缓存键比桌面那份多带一个尺寸，两种尺寸不互相顶掉；不做「源比图新就重抽」——
/// 那套是给会就地更新的 exe 用的，普通文件的关联图标基本不动。
pub fn shell_icons(paths: &[String], size: u32) -> HashMap<String, String> {
    let dir = match crate::services::desktop::get_icon_dir() {
        Ok(d) => d,
        Err(_) => return HashMap::new(),
    };
    let _ = std::fs::create_dir_all(&dir);
    let mut out = HashMap::new();
    for p in paths {
        if !Path::new(p).exists() {
            continue;
        }
        let stem = crate::services::desktop::fnv1a(p.as_bytes());
        let file = dir.join(format!("{stem:x}-{size}.png"));
        let png = file.to_string_lossy().to_string();
        if !file.exists() && crate::win::icon::extract_icon_at(p, &png, size).is_err() {
            continue;
        }
        out.insert(p.clone(), png);
    }
    out
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

/// 文本预览要的只是开头那一段原始字节，所以由这里直接读盘，不让 asset 协议去碰整个文件。
/// asset 协议的响应是先整份读进内存再包成 `SHCreateMemStream`，长度放不进 u32 时
/// wry 里那个 `u32::try_from(len).unwrap()` 会 panic，进程直接 abort。
/// 多读一个字节：调用方靠它判断后面还有没有。
pub fn read_file_head(path: &str, limit: u64) -> AppResult<Vec<u8>> {
    let mut buf = Vec::new();
    std::fs::File::open(path)?
        .take(limit.saturating_add(1))
        .read_to_end(&mut buf)?;
    Ok(buf)
}

// ---------------- 写盘的那几个动作 ----------------

fn split_ext(name: &str) -> (String, String) {
    match name.rfind('.') {
        None | Some(0) => (name.to_string(), String::new()),
        Some(i) => (name[..i].to_string(), name[i..].to_string()),
    }
}

/// 同名不覆盖：`名字 (1).扩展名` 一路加到不撞为止，跟资源管理器一个口径
fn free_name(dir: &Path, name: &str) -> String {
    let (base, ext) = split_ext(name);
    let mut candidate = name.to_string();
    let mut i = 1;
    while dir.join(&candidate).exists() {
        candidate = format!("{base} ({i}){ext}");
        i += 1;
    }
    candidate
}

/// 库里给这条路径（含它底下那一棵）记过的标注。整表不大，在 Rust 里按前缀筛比写 LIKE 稳
/// （路径里的 `%` 和 `_` 是 LIKE 的通配符，转义写法反而更容易错）
fn meta_paths_under(conn: &Connection, root: &str) -> AppResult<Vec<String>> {
    let prefix_a = format!("{root}\\");
    let prefix_b = format!("{root}/");
    let rows: Vec<String> = {
        let mut stmt = conn.prepare("SELECT path FROM files")?;
        let mapped = stmt.query_map([], |r| r.get::<_, String>(0))?;
        mapped.collect::<Result<_, _>>()?
    };
    Ok(rows
        .into_iter()
        .filter(|p| {
            p == root || p.starts_with(prefix_a.as_str()) || p.starts_with(prefix_b.as_str())
        })
        .collect())
}

/// 改名/搬走之后把标注带到新路径上。不带的话「这个文件为什么留着」那句话就原地留在了旧路径，
/// 对用户看就是名字一改别名没了
fn repoint_meta(conn: &Connection, from: &str, to: &str) -> AppResult<()> {
    let old_list = meta_paths_under(conn, from)?;
    let from_a = format!("{from}\\");
    let from_b = format!("{from}/");
    for old in old_list {
        let new = if old == from {
            to.to_string()
        } else if let Some(rest) = old.strip_prefix(from_a.as_str()) {
            format!("{to}\\{rest}")
        } else if let Some(rest) = old.strip_prefix(from_b.as_str()) {
            format!("{to}\\{rest}")
        } else {
            continue;
        };
        conn.execute(
            "UPDATE OR IGNORE files SET path = ?1 WHERE path = ?2",
            params![new, old],
        )?;
        // UPDATE OR IGNORE 撞上目标已存在的行会什么都不做，那行标注就还挂在旧路径上；
        // 真撞了就把旧行删掉——目标那份以它自己的标注为准
        conn.execute("DELETE FROM files WHERE path = ?1", params![old])?;
    }
    Ok(())
}

fn drop_meta(conn: &Connection, root: &str) -> AppResult<()> {
    for old in meta_paths_under(conn, root)? {
        conn.execute("DELETE FROM files WHERE path = ?1", params![old])?;
    }
    Ok(())
}

fn copy_dir_tree(from: &Path, to: &Path) -> AppResult<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        // 链接和 junction 不当目录递归进去拷：那玩意儿能转回自己，一不留神就是无限套娃
        if src.is_dir() && !entry.file_type()?.is_symlink() {
            copy_dir_tree(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// 目标目录不能是源本身或源底下的一层，不然拷着自己套娃。分隔符两种都当同一回事
fn check_not_inside(src: &str, dest_dir: &str) -> AppResult<()> {
    let norm = |p: &str| p.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let (s, d) = (norm(src), norm(dest_dir));
    if d == s || d.starts_with(&format!("{s}\\")) {
        return Err(AppError::msg(format!("不能把「{src}」放进它自己里面")));
    }
    Ok(())
}

/// 新建文件夹，返回建出来的完整路径（同名会自动让位，所以路径可能不等于传进来的名字）。
/// 不落标注：别名和说明是用户自己写的，空一行进库没有意义
pub fn create_folder(parent: &str, name: &str) -> AppResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("名字没填"));
    }
    let dir = Path::new(parent).join(free_name(Path::new(parent), trimmed));
    std::fs::create_dir(&dir)?;
    Ok(dir.to_string_lossy().to_string())
}

/// 重命名（只在它所在的目录里改名字）。返回新路径
pub fn rename_path(conn: &Connection, path: &str, new_name: &str) -> AppResult<String> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("名字没填"));
    }
    let from = Path::new(path);
    let parent = from.parent().ok_or_else(|| AppError::msg("这个路径没有上一层"))?;
    if from.file_name().map(|n| n == trimmed).unwrap_or(false) {
        return Ok(path.to_string());
    }
    let target = parent.join(free_name(parent, trimmed));
    std::fs::rename(from, &target)?;
    let to = target.to_string_lossy().to_string();
    repoint_meta(conn, path, &to)?;
    Ok(to)
}

/// 删除。`to_trash` 走回收站（标注留着，万一从回收站还原回来还在）；
/// 勾了彻底删除才连标注一起清掉
pub fn delete_paths(conn: &Connection, paths: &[String], to_trash: bool) -> AppResult<usize> {
    if paths.is_empty() {
        return Ok(0);
    }
    if to_trash {
        crate::win::recycle::to_recycle_bin(paths)?;
        return Ok(paths.len());
    }
    for p in paths {
        let path = Path::new(p);
        if path.is_dir() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
        drop_meta(conn, p)?;
    }
    Ok(paths.len())
}

/// 拷进某个目录。同名让位，返回拷贝落地的路径（撤销粘贴要照着删）。
/// 拷贝出来的那份**不带**别名和说明——那些话是讲给原件的，刚生出来的副本还没什么可交代的
pub fn copy_into(paths: &[String], dest_dir: &str) -> AppResult<Vec<String>> {
    for p in paths {
        check_not_inside(p, dest_dir)?;
    }
    let mut created = Vec::with_capacity(paths.len());
    for p in paths {
        let src = Path::new(p);
        let name = src
            .file_name()
            .ok_or_else(|| AppError::msg(format!("「{p}」没有名字，拷不了")))?;
        let target = Path::new(dest_dir).join(free_name(Path::new(dest_dir), &name.to_string_lossy()));
        if src.is_dir() {
            copy_dir_tree(src, &target)?;
        } else {
            std::fs::copy(src, &target)?;
        }
        created.push(target.to_string_lossy().to_string());
    }
    Ok(created)
}

/// 搬进某个目录。跨盘的 move 系统会失败，那就先拷再删源。返回落地后的路径
pub fn move_into(conn: &Connection, paths: &[String], dest_dir: &str) -> AppResult<Vec<String>> {
    for p in paths {
        check_not_inside(p, dest_dir)?;
    }
    let mut created = Vec::with_capacity(paths.len());
    for p in paths {
        let src = Path::new(p);
        let name = src
            .file_name()
            .ok_or_else(|| AppError::msg(format!("「{p}」没有名字，搬不了")))?;
        let target = Path::new(dest_dir).join(free_name(Path::new(dest_dir), &name.to_string_lossy()));
        let same_volume = match std::fs::rename(src, &target) {
            Ok(()) => true,
            // 只有「不在同一个卷」这一种失败才走拷贝+摘源（ERROR_NOT_SAME_DEVICE = 17，SDK 定义）。
            // 别的原因（占用、没权限）照原样报出去：那种情况下悄悄拷一份再删源，
            // 用户看到的是一次成功，实际做的事却比系统允许的更多
            Err(e) if e.raw_os_error() == Some(17) => {
                if src.is_dir() {
                    copy_dir_tree(src, &target)?;
                    std::fs::remove_dir_all(src)?;
                } else {
                    std::fs::copy(src, &target)?;
                    std::fs::remove_file(src)?;
                }
                false
            }
            Err(e) => return Err(e.into()),
        };
        let to = target.to_string_lossy().to_string();
        if same_volume {
            repoint_meta(conn, p, &to)?;
        } else {
            // 跨盘搬的那份不带标注：跟复制一个口径，源那边连同行一起摘掉
            drop_meta(conn, p)?;
        }
        created.push(to);
    }
    Ok(created)
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

        let nodes = list_files(&c, &dir.to_string_lossy(), false).unwrap();
        assert_eq!(
            nodes.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
            vec!["Z目录", "a.txt"],
            "目录排前面，名字按不区分大小写排"
        );
        assert_eq!(nodes[1].alias.as_deref(), Some("甲"));
        assert_eq!(nodes[1].description.as_deref(), Some("周报底稿"));
        assert_eq!(nodes[0].alias, None, "没登记过的就是空，不该串到别条上");
        assert_eq!(nodes[0].size, None, "目录不给大小");
        assert_eq!(nodes[1].size, Some(1), "文件带字节数，详情视图和按大小排序靠它");
        assert!(nodes[1].modified_at.is_some(), "修改时间同理");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 隐藏项的开关是这页替代资源管理器之后最容易翻脸的一条：默认不摆，摆了才看得见
    #[test]
    #[cfg(windows)]
    fn hidden_items_stay_out_until_the_list_asks_for_them() {
        let dir = sandbox("hidden");
        std::fs::write(dir.join("正常.txt"), b"x").unwrap();
        std::fs::write(dir.join("秘密.txt"), b"x").unwrap();
        set_hidden(&dir.join("秘密.txt"));
        let c = conn();

        let quiet = list_files(&c, &dir.to_string_lossy(), false).unwrap();
        assert_eq!(
            quiet.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
            vec!["正常.txt"],
            "默认那份里不该有隐藏项"
        );
        let loud = list_files(&c, &dir.to_string_lossy(), true).unwrap();
        assert_eq!(loud.len(), 2, "开关打开就得摆出来");
        assert!(loud.iter().any(|n| n.name == "秘密.txt" && n.hidden), "而且告诉界面它是隐藏的");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 盖隐藏属性位（FILE_ATTRIBUTE_HIDDEN = 0x2）
    #[cfg(windows)]
    fn set_hidden(path: &Path) {
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_FLAGS_AND_ATTRIBUTES};
        let wide: Vec<u16> = path.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
        unsafe { SetFileAttributesW(PCWSTR(wide.as_ptr()), FILE_FLAGS_AND_ATTRIBUTES(0x2)).unwrap() };
    }

    #[test]
    fn preview_roots_lists_desktops_and_the_icon_cache_only() {
        let c = conn();
        let roots = preview_roots(&c).unwrap();

        let icons = crate::services::desktop::get_icon_dir().unwrap();
        assert!(
            roots.iter().any(|r| r == &icons.to_string_lossy().to_string()),
            "图标缓存要在名单里，不然启动器一片没有图标"
        );
        assert!(
            !roots.iter().any(|r| r == "C:\\"),
            "整块盘不在启动名单里：文件页是点到哪个目录才临时授权哪个"
        );
        assert!(
            roots.windows(2).all(|w| w[0] <= w[1]),
            "排好去重，免得同一个目录授权两遍"
        );
    }

    #[test]
    fn creating_a_folder_twice_does_not_eat_the_first_one() {
        let dir = sandbox("mkdir");
        let first = create_folder(&dir.to_string_lossy(), "新建文件夹").unwrap();
        let second = create_folder(&dir.to_string_lossy(), "新建文件夹").unwrap();
        assert_ne!(first, second, "同名要让位，不然第二次把第一次的盖了");
        assert!(second.ends_with("新建文件夹 (1)"), "让位的写法跟系统一致");
        assert!(Path::new(&second).is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn renaming_a_file_takes_its_note_along() {
        let dir = sandbox("rename");
        std::fs::write(dir.join("旧名.txt"), b"x").unwrap();
        let c = conn();
        let from = dir.join("旧名.txt").to_string_lossy().to_string();
        save_file_meta(&c, &from, Some("留着有用".into()), Some("发票底".into())).unwrap();

        let to = rename_path(&c, &from, "新名.txt").unwrap();
        assert!(!Path::new(&from).exists(), "旧名字该没了");
        let moved = get_file(&c, &to).unwrap();
        assert_eq!(
            moved.alias.as_deref(),
            Some("留着有用"),
            "改名不该把别名甩在原路径上"
        );
        assert_eq!(moved.description.as_deref(), Some("发票底"));
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM files", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1,
            "旧行是搬走的，不是留了一份孤儿"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copying_a_tree_lands_beside_the_original() {
        let dir = sandbox("copy");
        std::fs::create_dir_all(dir.join("甲/里")).unwrap();
        std::fs::write(dir.join("甲/a.txt"), b"1").unwrap();
        std::fs::write(dir.join("甲/里/b.txt"), b"2").unwrap();
        std::fs::create_dir_all(dir.join("乙")).unwrap();
        let from = dir.join("甲").to_string_lossy().to_string();
        let dest = dir.join("乙").to_string_lossy().to_string();

        copy_into(std::slice::from_ref(&from), &dest).unwrap();
        assert!(dir.join("乙").join("甲").join("里").join("b.txt").exists(), "整棵子树一起过去");
        assert!(Path::new(&from).exists(), "复制不动源");
        assert!(
            copy_into(std::slice::from_ref(&from), &from).is_err(),
            "往自己里面拷是套娃，得报错"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn moving_a_file_repoints_the_note_and_removes_the_source() {
        let dir = sandbox("move");
        std::fs::create_dir_all(dir.join("乙")).unwrap();
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        let c = conn();
        let from = dir.join("a.txt").to_string_lossy().to_string();
        save_file_meta(&c, &from, Some("甲".into()), None).unwrap();
        let dest = dir.join("乙").to_string_lossy().to_string();

        let landed = dir.join("乙").join("a.txt").to_string_lossy().to_string();
        assert_eq!(
            move_into(&c, std::slice::from_ref(&from), &dest).unwrap(),
            vec![landed.clone()],
            "回的是落地后的路径"
        );
        assert!(!Path::new(&from).exists(), "源要摘掉");
        assert_eq!(get_file(&c, &landed).unwrap().alias.as_deref(), Some("甲"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_permanent_delete_also_drops_the_note() {
        let dir = sandbox("del");
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        let c = conn();
        let p = dir.join("a.txt").to_string_lossy().to_string();
        save_file_meta(&c, &p, Some("甲".into()), None).unwrap();

        delete_paths(&c, std::slice::from_ref(&p), false).unwrap();
        assert!(!Path::new(&p).exists());
        let left: i64 = c
            .query_row("SELECT COUNT(*) FROM files WHERE path = ?1", params![p], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "彻底删掉的文件不该在库里留一行");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 真往回收站里扔一个文件。跑它会在本机回收站留一条记录，所以默认不跑：
    /// `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn the_recycle_bin_route_actually_moves_the_file() {
        let dir = sandbox("recycle");
        let f = dir.join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        let c = conn();
        let p = f.to_string_lossy().to_string();
        save_file_meta(&c, &p, Some("甲".into()), None).unwrap();

        delete_paths(&c, std::slice::from_ref(&p), true).unwrap();
        assert!(!Path::new(&p).exists(), "文件进了回收站，原位置就该空");
        let left: i64 = c
            .query_row("SELECT COUNT(*) FROM files WHERE path = ?1", params![p], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 1, "进回收站的标注要留着，还原回来那句话还在");
        let _ = std::fs::remove_dir_all(&dir);
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

    #[test]
    fn the_head_read_stops_one_byte_past_the_limit() {
        let dir = sandbox("head");
        let f = dir.join("big.log");
        std::fs::write(&f, b"0123456789").unwrap();
        let p = f.to_string_lossy().to_string();

        assert_eq!(read_file_head(&p, 4).unwrap().len(), 5, "多读的那一个字节是给前端判断截断用的");
        assert_eq!(read_file_head(&p, 10).unwrap().len(), 10, "整份装得下就一个字节都不多读");
        assert!(read_file_head(&dir.join("nope").to_string_lossy(), 4).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}