//! 扫描桌面：两个桌面根目录（本机 + OneDrive 那种）各翻两层，
//! 每条候选先问规则「这算哪一类」，没命中就直接丢掉——不凭空造分类。

use std::path::Path;

use rusqlite::Connection;

use crate::error::AppResult;
use crate::models::{DesktopItem, DesktopRule};
use crate::services::categories;

use super::icons::{extension_of, extract_icon, get_icon_dir, launch_target};
use super::rules::{category_ext, match_category};
use super::ORGANIZE_DIR;

/// 往下翻几层：桌面根目录算 0 层，再深就可能是用户自己的大件结构了
const SCAN_MAX_DEPTH: usize = 2;
pub fn scan_desktop(conn: &Connection) -> AppResult<Vec<DesktopItem>> {
    let rules = categories::get_rules(conn)?;
    let icon_dir = get_icon_dir()?;
    let _ = std::fs::create_dir_all(&icon_dir);

    let mut items = Vec::new();
    let own = OwnExe::this();
    for desktop in crate::win::desktop::all_desktops() {
        scan_directory(&desktop, 0, &rules, &icon_dir, own.as_ref(), &mut items);
    }
    items.sort_by_key(|a| a.name.to_lowercase());
    items.dedup_by(|a, b| a.path == b.path);
    Ok(items)
}

/// 扫描要绕开工作台自己：桌面上那个快捷方式指的就是这个程序，
/// 让它进启动器等于「用一个工作台去整理放着这个工作台的桌面」。
/// 开发时跑的是 `target\debug` 里那份、快捷方式指的是装机那份，两个路径不一样，
/// 所以除了完整路径相等，还按 exe 的文件名认——桌面这种场景里同名文件基本只有它自己。
pub(crate) struct OwnExe {
    pub(crate) file: String,
    pub(crate) full: String,
}

/// Windows 路径比较用：去正反斜杠、去 `\\?\` 前缀、折小写
pub(crate) fn norm_path(p: &Path) -> String {
    let raw = p.to_string_lossy();
    let raw = raw.strip_prefix(r"\\?\").unwrap_or(&raw);
    raw.replace('\\', "/").to_ascii_lowercase()
}

impl OwnExe {
    fn this() -> Option<OwnExe> {
        let path = std::env::current_exe().ok()?;
        Some(OwnExe {
            file: path.file_name()?.to_string_lossy().to_ascii_lowercase(),
            full: norm_path(&path),
        })
    }

    pub(crate) fn is_this(&self, path: &Path, target: &str) -> bool {
        [path, Path::new(target)].into_iter().any(|cand| {
            norm_path(cand) == self.full
                || cand
                    .file_name()
                    .map(|n| n.to_string_lossy().to_ascii_lowercase() == self.file)
                    .unwrap_or(false)
        })
    }
}

/// 桌面根目录连同里面的子文件夹一起翻，任何类型的文件都算待整理内容；
/// 文件夹本身只往里走不单独成项，避免「连文件夹一起搬」和「搬它里面的文件」重复。
pub(crate) fn scan_directory(
    dir: &str,
    depth: usize,
    rules: &[DesktopRule],
    icon_dir: &Path,
    own: Option<&OwnExe>,
    items: &mut Vec<DesktopItem>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.eq_ignore_ascii_case("desktop.ini") || file_name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            // 「桌面整理」是上一次的归档结果，不再往里翻
            if file_name.eq_ignore_ascii_case(ORGANIZE_DIR) || depth >= SCAN_MAX_DEPTH {
                continue;
            }
            scan_directory(
                &path.to_string_lossy(),
                depth + 1,
                rules,
                icon_dir,
                own,
                items,
            );
            continue;
        }

        let ext = extension_of(&path);
        let is_lnk = ext == "lnk";
        let target = launch_target(&path);
        if own.map(|o| o.is_this(&path, &target)).unwrap_or(false) {
            continue;
        }
        let display_name = if is_lnk {
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or(file_name)
        } else {
            file_name
        };

        // lnk 按真实目标的扩展名分类
        let target_ext = if is_lnk {
            extension_of(Path::new(&target))
        } else {
            String::new()
        };
        let cat_ext = category_ext(is_lnk, &ext, &target_ext);
        // 没有规则命中就不算一个待整理项：不凭空造分类，也不在页面上留一张没归属的卡片
        let Some(category) = match_category(&display_name, cat_ext, rules) else {
            continue;
        };
        let own = path.to_string_lossy();
        // 先抽快捷方式自己（那才是桌面上显示的那张），抽不到再退回解析出来的目标
        let icon_path = extract_icon(icon_dir, &[own.as_ref(), target.as_str()]);

        items.push(DesktopItem {
            name: display_name,
            path: path.to_string_lossy().to_string(),
            target,
            is_lnk,
            category,
            icon_path,
        });
    }
}
