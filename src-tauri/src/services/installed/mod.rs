//! 「应用」页的数据源：注册表里登记的已安装程序。
//!
//! `HKLM/HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall` 四个视图（两个 hive ×
//! 32/64 位视图）是 Windows 自己记「装了什么」的地方：装过的程序一个都跑不掉，还带得到
//! 版本和发行商；但装过没写卸载项的那些不在这里（下一段就是补这个的）。
//!
//! 但注册表只记「装过的」：商店应用（WSL 那类）和从没跑过安装程序的绿色版压根没有卸载项。
//! 所以还有**第二遍**——开始菜单里没被第一遍认领走的 .lnk 也收成卡片，来源仍标 `startmenu`。
//! 已经被某个注册表条目当过目标的 .lnk 不会再收一遍；解出来的目标不是「能跑的东西」（.chm 帮助、
//! .htm 说明这种附属文件）也不收；系统那条过滤和手动名单在这一遍一样生效。
//!
//! 注册表的弱点在「怎么打开它」：`InstallLocation` 常常是空的，`DisplayIcon` 又经常指着
//! 卸载器图标（`…\uninstallerIcon.ico`、`app.exe,0`）。所以启动目标按六级找：
//! **DisplayIcon 里那个真存在的 exe → DisplayIcon 指的是快捷方式的 → 开始菜单里同名快捷方式 →
//! 安装目录自己就是 exe → 已知那几个目录（安装目录、卸载器所在目录、DisplayIcon 所在目录）里
//! 名字对得上的 exe → 卸载串里那个不是卸载器的 exe**，全落空就用目录本身（点开是文件夹），
//! 连目录都没有的算「找不到」，不收卡片、只在账单里报数。
//!
//! 同步之前先过一遍过滤名单（设置 → 应用过滤）：命中的一律不收。
//!
//! 这一层拆成四个子模块，按数据流读下来就是扫描的顺序：
//!
//! - [scan]    读源头：注册表四个视图的卸载项、开始菜单的 .lnk 索引
//! - [target]  认目标：六级顺序找「怎么打开」，顺带归一路径写法
//! - [filter]  判收不收：手动名单 +「系统应用」总开关 + 按名字判重
//! - [store]   写库与账单：只插不改，被挡/已有/找不到的都报数
//!
//! 本文件只留三种共用的数据结构、比对名字和路径用的写法，以及一次同步的编排。
//! `pub(crate) use` 摊平，所以外面照旧写 `crate::services::installed::sync_installed(...)`，
//! 不用关心它落在哪个子文件里。拆文件只搬不改：判定顺序、常量的值、注释原文都没动。

use std::path::PathBuf;

use rusqlite::Connection;
use serde::Serialize;

use crate::error::AppResult;
use crate::services::desktop::get_icon_dir;

mod filter;
mod scan;
mod store;
mod target;

use scan::{read_entries, shortcut_index};
use store::plan;

pub use filter::{add_app_filter, get_app_filters, remove_app_filter};

/// 一次同步（或试算）的账单。界面照着它说话，不然「点了没反应」和「本来就没有新的」分不出来。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    /// 候选总数：注册表读到的卸载项 + 没被注册表认领的开始菜单快捷方式
    pub scanned: usize,
    /// 本次收进来的（试算时是「会收进来的」）
    pub inserted: usize,
    /// 命中过滤名单没收的
    pub filtered: usize,
    /// 库里已经有的
    pub already: usize,
    /// 一条路径都认不出来的：它在注册表里挂着名字，但不知道打开什么，不收卡片
    pub unresolved: usize,
    /// 库里现在一共多少张卡片
    pub total: usize,
    /// 被挡掉的名字，最多 12 个
    pub samples: Vec<String>,
}

/// 过滤条件：手加的模式（名字和路径都做小写包含匹配）+「系统应用」那个总开关
pub struct Filters {
    pub patterns: Vec<String>,
    pub system: bool,
}

/// 注册表读出来的一条卸载项。测试直接手搭这个，不用碰真注册表
#[derive(Debug, Clone, Default)]
pub(crate) struct Entry {
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub install_location: String,
    pub display_icon: String,
    pub uninstall: String,
    /// `HKLM\Software\...\Uninstall\{GUID}`，用来让被挡的项看得出是谁
    pub reg_key: String,
    pub system_component: bool,
    /// 补丁/热修复：挂在别的程序底下的那种条目，不是一「个应用」
    pub is_patch: bool,
}

/// 快捷方式索引里的一条：名字（已归一化）、.lnk 自己、解析出的真目标
#[derive(Debug, Clone)]
pub(crate) struct Shortcut {
    /// .lnk 的文件名（不带后缀）：第二遍采集时它就是卡片名字
    pub name: String,
    pub key: String,
    pub lnk: String,
    pub target: String,
}

/// 一个条目的目标找到在第几级上——卡片点开是什么东西由它决定
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Found {
    /// DisplayIcon 或安装目录/卸载串里那个 exe
    Exe,
    /// 开始菜单的同名快捷方式
    Shortcut,
    /// 只有安装目录：点开是文件夹
    Folder,
    /// 什么路径都没认出来
    Nothing,
}

#[derive(Debug, Clone)]
pub(crate) struct Resolved {
    pub target: String,
    /// 抽图标的候选，越靠前越像「桌面上看到的那张」
    pub icons: Vec<String>,
    pub found: Found,
}
/// 比对用的写法：老卡片存的是 `F:\\Program\\...`，注册表里可能是 `F:/Program/...`，
/// 同一个程序不该因为斜杠方向被收两遍
fn path_key(p: &str) -> String {
    p.trim().to_lowercase().replace('\\', "/")
}
/// 名字比对用的写法：折小写、只留字母数字（中文留着），这样「Foo 1.2 (64-bit)」
/// 和「Foo1264bit」才算得上同一个东西
fn name_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// 再去掉尾巴上的版本号和括号残留：注册表名字常比快捷方式多个「3.11.7」
fn short_key(s: &str) -> String {
    let full = name_key(s);
    let keep = full
        .trim_end_matches(|c: char| c.is_ascii_digit() || c == '.')
        .trim_end_matches("bit");
    keep.to_string()
}
/// 同步之前先过一遍过滤名单（设置 → 应用过滤）：命中的一律不收。
/// 只插不改：用户改过的名字、手动归好的分类、抽好的图标都不该被一次同步抹掉。
/// 删掉的卡片下次同步照样收回来——这一页是「本机装了什么」的镜子，不做拉黑名单。
pub fn sync_installed(conn: &Connection) -> AppResult<SyncReport> {
    run(conn, false)
}

/// 同一次扫描但不写库，用来看过滤名单挡掉了谁
pub fn preview_installed(conn: &Connection) -> AppResult<SyncReport> {
    run(conn, true)
}

fn run(conn: &Connection, dry: bool) -> AppResult<SyncReport> {
    let icon_dir = get_icon_dir()?;
    let entries = read_entries();
    let shortcuts = shortcut_index(
        &crate::win::desktop::all_program_menus()
            .into_iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>(),
    );
    plan(conn, &entries, &shortcuts, &icon_dir, dry)
}

#[cfg(test)]
mod tests;
