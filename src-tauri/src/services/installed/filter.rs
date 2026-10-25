//! 判收不收：一次同步里两道闸——手动名单（设置 → 应用过滤）和「系统应用」总开关，
//! 再加一条按名字的判重 [find_program]。名单的增删改查也在这儿，改的就是这张名单。

use std::collections::HashMap;

use rusqlite::{params, Connection};

use crate::error::AppResult;

use super::{name_key, path_key, Entry, Filters};

/// 设置里那个「过滤系统应用」开关；没写过就当作开着
pub(crate) const SYSTEM_SWITCH: &str = "filter_system_apps";
pub(crate) fn read_filters(conn: &Connection) -> AppResult<Filters> {
    let patterns = conn
        .prepare("SELECT pattern FROM app_filters")?
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    let system = crate::services::settings::get_setting(conn, SYSTEM_SWITCH)?
        .map(|v| v != "0")
        .unwrap_or(true);
    Ok(Filters { patterns, system })
}
/// 名字或任意一条路径包含某个模式就算命中；模式空串不挡东西
pub(crate) fn hits(entry: &Entry, target: &str, filters: &Filters, system_root: &str) -> bool {
    if filters.system && is_system(entry, target, system_root) {
        return true;
    }
    // 名字两种写法都比：原样折小写（带标点，「7-Zip 23.01 (x64)」这种模式要能命中），
    // 以及只留字母数字的那份（快捷方式名字和注册表名字差个空格括号也算同一个）
    let mut haystacks: Vec<String> = [
        entry.install_location.as_str(),
        entry.display_icon.as_str(),
        entry.uninstall.as_str(),
        entry.reg_key.as_str(),
        target,
    ]
    .into_iter()
    .map(path_key)
    .collect();
    haystacks.push(entry.name.to_lowercase());
    haystacks.push(name_key(&entry.name));
    filters.patterns.iter().any(|p| {
        let p = p.trim().to_lowercase();
        !p.is_empty() && haystacks.iter().any(|h| h.contains(&p))
    })
}

/// 「系统应用」这个开关管的东西在注册表里有几种长相：明写自己是系统组件的、挂在别的程序底下的
/// 补丁、以及名字一听就是附属程序 / 驱动 / 运行环境的（见 `SYSTEM_NAME_MARKS`），
/// 再加上装在 Windows 目录里的那些。
/// 路径只按安装目录和真目标判：**MSI 的 DisplayIcon 全在 `C:\Windows\Installer` 里**，
/// 拿它判系统会把一堆正经软件一起挡掉。
pub(crate) fn is_system(entry: &Entry, target: &str, system_root: &str) -> bool {
    if entry.system_component || entry.is_patch {
        return true;
    }
    let lower = entry.name.to_lowercase();
    if SYSTEM_NAME_MARKS.iter().any(|m| lower.contains(m)) {
        return true;
    }
    let root = path_key(system_root);
    let root = root.trim_end_matches('/');
    if root.is_empty() {
        return false;
    }
    [entry.install_location.as_str(), target]
        .into_iter()
        .map(path_key)
        .any(|p| {
            !p.is_empty()
                && (p.starts_with(&format!("{root}/"))
                    || SYSTEM_MARKS.iter().any(|m| p.contains(m)))
        })
}

/// 「系统应用」这个开关名下的三类东西，都是「装过但没人会从启动器打开」的：
///
/// - 附属程序：更新程序、补丁、运行库、帮助组件
/// - 驱动：厂商驱动包和芯片组包（ODBC 这类数据库驱动也算）
/// - 运行环境：`.NET` / DirectX / WebView2 这些给别人用的底座
///
/// **故意不在这儿**：`python`、`node`、`java`、`sdk`、`audio` 这类词——那是他自己装来用（或就是
/// 一款软件）的东西，`Java` 那种名字里带 "Update" 的会顺带被挡掉，但不该按词根一刀切。
const SYSTEM_NAME_MARKS: &[&str] = &[
    // 附属程序
    "redistributable",
    "update",
    "updater",
    "hotfix",
    "runtime",
    "component",
    "helper",
    "更新",
    "运行库",
    // 驱动
    "driver",
    "驱动",
    "chipset",
    "odbc",
    // 运行环境
    ".net",
    "directx",
    "webview2",
    "运行环境",
    "组件",
];

const SYSTEM_MARKS: [&str; 4] = [
    "/windowsapps/",
    "/program files/windows nt/",
    "/program files/windows defender/",
    "/program files/windows security/",
];
pub(crate) fn system_root() -> String {
    std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string())
}
/// 「同一个程序」只按名字判：完全一样，或者去掉标点后一个是另一个的一段（注册表名字常多个版本号
/// 或厂商前缀），太短的段不算——`git` 是 `githubcli` 的一段，但它俩是两个程序。
/// 目标文件相同**不能**当判据：Steam 全家、Epic、UWP（都走 ApplicationFrameHost.exe）都是一个
/// 启动器带一片程序，按目标去重会只剩第一张。
///
/// 判据只这一份，而且返回的是那一行本身：「认出库里已有」和「已有卡片要顺手把路径图标追回来」
/// 用的是同一个问题，写成两个函数迟早漂开——图标不更新就是这么来的。
pub(crate) fn find_program<'k, T>(known: &'k HashMap<String, T>, key: &str) -> Option<&'k T> {
    if key.is_empty() {
        return None;
    }
    if let Some(hit) = known.get(key) {
        return Some(hit);
    }
    if key.len() < 4 {
        return None;
    }
    known
        .iter()
        .find(|(k, _)| k.len() >= 4 && (k.contains(key) || key.contains(k.as_str())))
        .map(|(_, row)| row)
}
/// 过滤名单：一条模式就是个小写子串，比对名字和路径两种写法
pub fn get_app_filters(conn: &Connection) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT pattern FROM app_filters ORDER BY added_at DESC")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// 回 true 是真加上了；重复加（不分大小写）回 false，让界面说一句「已经在名单里」
pub fn add_app_filter(conn: &Connection, pattern: &str) -> AppResult<bool> {
    let p = pattern.trim();
    if p.is_empty() {
        return Err(crate::error::AppError::msg("先写点东西，空的模式不挡任何程序"));
    }
    Ok(conn.execute(
        "INSERT OR IGNORE INTO app_filters (pattern) VALUES (?1)",
        params![p],
    )? > 0)
}

pub fn remove_app_filter(conn: &Connection, pattern: &str) -> AppResult<bool> {
    Ok(conn.execute("DELETE FROM app_filters WHERE pattern = ?1", params![pattern.trim()])? > 0)
}
