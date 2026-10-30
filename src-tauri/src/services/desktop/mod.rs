//! 桌面这一页的三件事：扫描桌面出卡片、按规则整理进文件夹、以及卡片（应用）本身的增删改查。
//!
//! 一条规则改分类名不止改那张表：分类表、桌面上的文件夹、`organize_logs.to_path`、卡片的 `target`
//! 都记着旧名字，所以改名要一路带走（[rules]），「恢复整理」才退得回去（[organize]）。
//!
//! - [scan]     扫桌面：翻两层、抽图标、按规则定分类，规则没命中的东西不进清单
//! - [organize] 挪文件与回退：归档到「桌面整理/<分类>」，记一笔日志，回退按日志原路搬回
//! - [rules]    规则改名往哪些表、哪些目录扩散，以及「一个文件算哪一类」的匹配
//! - [apps]     「应用」页卡片的读写与启动（数据由 `services::installed` 同步进来）
//! - [icons]    图标缓存与路径写法：`get_icon_dir` / `extract_icon` / `extension_of` / `launch_target`
//!
//! 这里 `pub use` 摊平，所以外面照旧写 `crate::services::desktop::scan_desktop(...)`。
//! 拆文件只搬不改：判定顺序、SQL、注释原文都没动。

pub use apps::{delete_app, get_apps, launch_by_id, save_app};
pub use organize::{organize_desktop, revert_organize};
pub use rules::save_rule_with_rename;
pub use scan::scan_desktop;

pub(crate) use icons::{extension_of, extract_icon, fnv1a, get_icon_dir, launch_target};

mod apps;
mod icons;
mod organize;
mod rules;
mod scan;

/// 快捷方式、可执行文件、网址链接之外的东西也要整理，所以扫描不按扩展名筛
pub(crate) const ORGANIZE_DIR: &str = "桌面整理";

#[cfg(test)]
mod tests;
