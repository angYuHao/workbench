//! C 盘磁盘清理：只碰公认安全的几处垃圾位置，别的一律不碰。
//!
//! 扫描只统计不动磁盘；清理时逐个文件删，被占用/没权限的跳过并计数，
//! 符号链接和 junction 一律不跟（免得绕出 C 盘或绕成环），清理根目录本身永远留着。
//! 系统那几处（Windows\Temp、更新缓存、WER）不带管理员权限时多半删不动一部分，
//! 跳过多少如实报多少，不假装清干净了。

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    SHQueryRecycleBinW, SHEmptyRecycleBinW, SHQUERYRBINFO, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI,
    SHERB_NOSOUND,
};

use crate::error::{AppError, AppResult};

/// 扫描出来的一类：摆进表格让人勾
#[derive(Debug, Clone, Serialize)]
pub struct CategoryScan {
    pub id: String,
    pub label: String,
    pub desc: String,
    /// 这类垃圾实际住哪，摆出来让人知道删的是什么
    pub hint: String,
    pub bytes: u64,
    pub files: u64,
    pub available: bool,
    pub note: Option<String>,
}

/// 清理完一类的回执：清出多少、跳过多少
#[derive(Debug, Clone, Serialize)]
pub struct CategoryResult {
    pub id: String,
    pub freed_bytes: u64,
    pub removed_files: u64,
    pub skipped_files: u64,
    pub note: Option<String>,
}

enum Kind {
    Dirs(Vec<PathBuf>),
    /// 只删目录里认得的那几种缓存库文件，不动目录里别的东西
    CacheFiles(PathBuf),
    RecycleBin,
}

struct Spec {
    id: &'static str,
    label: &'static str,
    desc: &'static str,
    kind: Kind,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var(name).ok().filter(|s| !s.is_empty()).map(PathBuf::from)
}

/// 清理根的两道闸：必须在 C 盘，且至少往下两层。
/// 盘符根、C:\Windows、C:\Users 这种只有一层的都太宽，一律不许当根。
/// 固定名单本来就不会写出那种根，这道闸是防以后有人往名单里加错位置
fn safe_root(p: &Path) -> bool {
    let lower = p.to_string_lossy().to_ascii_lowercase();
    if !lower.starts_with("c:\\") {
        return false;
    }
    p.components()
        .filter(|c| matches!(c, std::path::Component::Normal(_)))
        .count()
        >= 2
}

fn specs() -> Vec<Spec> {
    let mut out = vec![
        Spec {
            id: "user-temp",
            label: "用户临时文件",
            desc: "当前用户 Temp 目录里的东西；正在被占用的会跳过",
            kind: Kind::Dirs(env_dir("TEMP").into_iter().collect()),
        },
        Spec {
            id: "win-temp",
            label: "系统临时文件",
            desc: "Windows\\Temp；没权限或被占用的会跳过",
            kind: Kind::Dirs(vec![PathBuf::from("C:\\Windows\\Temp")]),
        },
        Spec {
            id: "wu-cache",
            label: "Windows 更新缓存",
            desc: "更新下载完留下的安装包；没权限的会跳过",
            kind: Kind::Dirs(vec![PathBuf::from("C:\\Windows\\SoftwareDistribution\\Download")]),
        },
        Spec {
            id: "thumb-cache",
            label: "缩略图缓存",
            desc: "资源管理器的 thumbcache / iconcache 库；被占用的会跳过，删了会自己重建",
            kind: Kind::CacheFiles(
                env_dir("LOCALAPPDATA")
                    .map(|p| p.join("Microsoft\\Windows\\Explorer"))
                    .unwrap_or_default(),
            ),
        },
        Spec {
            id: "wer",
            label: "Windows 错误报告",
            desc: "程序崩溃攒下的报告队列与归档",
            kind: Kind::Dirs(vec![
                PathBuf::from("C:\\ProgramData\\Microsoft\\Windows\\WER\\ReportQueue"),
                PathBuf::from("C:\\ProgramData\\Microsoft\\Windows\\WER\\ReportArchive"),
            ]),
        },
        Spec {
            id: "recycle",
            label: "回收站（C 盘）",
            desc: "清空 C 盘回收站；清完找不回来",
            kind: Kind::RecycleBin,
        },
    ];
    out.retain(|s| match &s.kind {
        Kind::Dirs(roots) => !roots.is_empty(),
        Kind::CacheFiles(dir) => !dir.as_os_str().is_empty(),
        Kind::RecycleBin => true,
    });
    out
}

/// 缩略图/图标缓存库的文件名形状；目录里别的东西（比如 explorer 自己的设置）不碰
fn is_thumb_cache(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    (lower.starts_with("thumbcache_") || lower.starts_with("iconcache_")) && lower.ends_with(".db")
}

/// 深度优先收文件和目录。dirs 按「发现顺序」记，倒过来删就是先子后父
fn walk(root: &Path, files: &mut Vec<(PathBuf, u64)>, dirs: &mut Vec<PathBuf>) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else { continue };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                dirs.push(path.clone());
                stack.push(path);
            } else {
                files.push((path, meta.len()));
            }
        }
    }
}

fn recycle_query() -> Option<(u64, u64)> {
    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        ..Default::default()
    };
    let root = wide("C:\\");
    unsafe { SHQueryRecycleBinW(PCWSTR(root.as_ptr()), &mut info) }.ok()?;
    Some((info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64))
}

/// 扫一遍六类位置，只统计不动磁盘
pub fn scan() -> Vec<CategoryScan> {
    specs()
        .into_iter()
        .map(|spec| {
            let mut bytes = 0u64;
            let mut files = 0u64;
            let mut available = true;
            let mut note = None;
            let hint = match &spec.kind {
                Kind::Dirs(roots) => {
                    let roots: Vec<&PathBuf> = roots.iter().filter(|p| safe_root(p)).collect();
                    if roots.is_empty() {
                        available = false;
                        note = Some("位置不在 C 盘或不够安全，这一类不参与".into());
                    }
                    for root in &roots {
                        let mut f = Vec::new();
                        let mut d = Vec::new();
                        walk(root, &mut f, &mut d);
                        bytes += f.iter().map(|(_, n)| n).sum::<u64>();
                        files += f.len() as u64;
                    }
                    roots.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("；")
                }
                Kind::CacheFiles(dir) => {
                    if !safe_root(dir) || !dir.is_dir() {
                        available = false;
                        note = Some("找不到缓存目录，这一类不参与".into());
                    } else if let Ok(rd) = fs::read_dir(dir) {
                        for entry in rd.flatten() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if !is_thumb_cache(&name) {
                                continue;
                            }
                            if let Ok(meta) = entry.metadata() {
                                bytes += meta.len();
                                files += 1;
                            }
                        }
                    }
                    dir.display().to_string()
                }
                Kind::RecycleBin => {
                    match recycle_query() {
                        Some((b, n)) => {
                            bytes = b;
                            files = n;
                        }
                        None => {
                            available = false;
                            note = Some("问不到回收站，这一类不参与".into());
                        }
                    }
                    "C:\\$Recycle.Bin".into()
                }
            };
            CategoryScan {
                id: spec.id.into(),
                label: spec.label.into(),
                desc: spec.desc.into(),
                hint,
                bytes,
                files,
                available,
                note,
            }
        })
        .collect()
}

fn clean_dirs(roots: &[PathBuf]) -> (u64, u64, u64) {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for root in roots {
        walk(root, &mut files, &mut dirs);
    }
    let mut freed = 0u64;
    let mut removed = 0u64;
    let mut skipped = 0u64;
    for (path, size) in &files {
        match fs::remove_file(path) {
            Ok(()) => {
                freed += size;
                removed += 1;
            }
            Err(_) => skipped += 1,
        }
    }
    for dir in dirs.iter().rev() {
        let _ = fs::remove_dir(dir);
    }
    (freed, removed, skipped)
}

/// 清掉勾上的那几类。没勾、或勾了名单外的 id 都直接报错，不默默干别的
pub fn run(ids: &[String]) -> AppResult<Vec<CategoryResult>> {
    if ids.is_empty() {
        return Err(AppError::msg("没选要清理的项目"));
    }
    let all = specs();
    for id in ids {
        if !all.iter().any(|s| s.id == id) {
            return Err(AppError::msg(format!("不认识的清理项目：{id}")));
        }
    }
    let mut out = Vec::new();
    for spec in all.iter().filter(|s| ids.iter().any(|i| i == s.id)) {
        let (freed_bytes, removed_files, skipped_files, note) = match &spec.kind {
            Kind::Dirs(roots) => {
                let roots: Vec<PathBuf> = roots.iter().filter(|p| safe_root(p)).cloned().collect();
                let (f, r, s) = clean_dirs(&roots);
                (f, r, s, None)
            }
            Kind::CacheFiles(dir) => {
                let mut freed = 0u64;
                let mut removed = 0u64;
                let mut skipped = 0u64;
                if let Ok(rd) = fs::read_dir(dir) {
                    for entry in rd.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if !is_thumb_cache(&name) {
                            continue;
                        }
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        match fs::remove_file(entry.path()) {
                            Ok(()) => {
                                freed += size;
                                removed += 1;
                            }
                            Err(_) => skipped += 1,
                        }
                    }
                }
                (freed, removed, skipped, None)
            }
            Kind::RecycleBin => {
                let before = recycle_query().unwrap_or((0, 0));
                let root = wide("C:\\");
                let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
                match unsafe { SHEmptyRecycleBinW(HWND(std::ptr::null_mut()), PCWSTR(root.as_ptr()), flags) } {
                    Ok(()) => (before.0, before.1, 0, None),
                    Err(e) => (0, 0, 0, Some(format!("回收站没清成：{e}"))),
                }
            }
        };
        out.push(CategoryResult {
            id: spec.id.into(),
            freed_bytes,
            removed_files,
            skipped_files,
            note,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 清理根的两道闸：盘符根、整棵系统树、别的盘，都不许当根
    #[test]
    fn only_deep_c_drive_folders_can_be_roots() {
        assert!(!safe_root(Path::new("C:\\")));
        assert!(!safe_root(Path::new("C:\\Windows")));
        assert!(!safe_root(Path::new("C:\\Windows\\")));
        assert!(!safe_root(Path::new("D:\\Temp")));
        assert!(!safe_root(Path::new("C:\\Users")));
        assert!(safe_root(Path::new("C:\\Windows\\Temp")));
        assert!(safe_root(Path::new("C:\\Users\\x\\AppData\\Local\\Temp")));
    }

    /// 缓存目录里只认那两种库文件，别的不碰
    #[test]
    fn only_the_cache_dbs_match() {
        assert!(is_thumb_cache("thumbcache_16.db"));
        assert!(is_thumb_cache("iconcache_32.db"));
        assert!(is_thumb_cache("ThumbCache_96.DB"));
        assert!(!is_thumb_cache("thumbcache_16.txt"));
        assert!(!is_thumb_cache("explorer.db"));
        assert!(!is_thumb_cache("thumbcache.db"));
    }

    /// 真删一遍临时fixture：文件清掉、根留着、占用/删不掉的计跳过
    #[test]
    fn cleaning_a_fixture_tree_frees_bytes_and_keeps_the_root() {
        let root = std::env::temp_dir().join(format!("wb-cleanup-{}", std::process::id()));
        let nested = root.join("a").join("b");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("top.txt"), vec![0u8; 3]).unwrap();
        fs::write(nested.join("deep.txt"), vec![0u8; 2]).unwrap();
        let (freed, removed, skipped) = clean_dirs(std::slice::from_ref(&root));
        assert_eq!((freed, removed, skipped), (5, 2, 0));
        assert!(root.is_dir(), "清理根自己不能被删");
        assert!(!root.join("top.txt").exists());
        assert!(!nested.exists(), "空掉的子目录该一起收掉");
        let again = clean_dirs(std::slice::from_ref(&root));
        assert_eq!(again, (0, 0, 0));
        let _ = fs::remove_dir_all(&root);
    }

    /// 名单外的 id、空勾选都报错，不默默干别的
    #[test]
    fn unknown_or_empty_selection_is_refused() {
        assert!(run(&[]).is_err());
        assert!(run(&["not-a-category".into()]).is_err());
    }

    /// 六类都在、id 稳定；本机至少有一类扫得动（用户 Temp 总在 C 盘）
    #[test]
    fn the_six_categories_scan() {
        let rows = scan();
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            ["user-temp", "win-temp", "wu-cache", "thumb-cache", "wer", "recycle"]
        );
        assert!(rows.iter().any(|r| r.available));
        for r in &rows {
            assert!(!r.label.is_empty());
            assert!(!r.hint.is_empty());
        }
    }
}