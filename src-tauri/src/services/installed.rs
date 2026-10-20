//! 「应用」页的数据源：注册表里登记的已安装程序。
//!
//! `HKLM/HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall` 四个视图（两个 hive ×
//! 32/64 位视图）是 Windows 自己记「装了什么」的地方：装过的程序一个都跑不掉，还带得到
//! 版本和发行商；没装过的绿色版不在这里。
//!
//! 注册表的弱点在「怎么打开它」：`InstallLocation` 常常是空的，`DisplayIcon` 又经常指着
//! 卸载器图标（`…\uninstallerIcon.ico`、`app.exe,0`）。所以启动目标按六级找：
//! **DisplayIcon 里那个真存在的 exe → DisplayIcon 指的是快捷方式的 → 开始菜单里同名快捷方式 →
//! 安装目录自己就是 exe → 已知那几个目录（安装目录、卸载器所在目录、DisplayIcon 所在目录）里
//! 名字对得上的 exe → 卸载串里那个不是卸载器的 exe**，全落空就用目录本身（点开是文件夹），
//! 连目录都没有的算「找不到」，不收卡片、只在账单里报数。
//!
//! 同步之前先过一遍过滤名单（设置 → 应用过滤）：命中的一律不收。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::AppResult;
use crate::services::desktop::{extract_icon, extension_of, get_icon_dir, launch_target};
use crate::win::registry::{Arch, Hive, Key};

/// 开始菜单本来就有多层厂商目录，给三层的量
const MAX_DEPTH: usize = 3;
/// 试算里最多报几个被挡的名字，够看出规律就行
const SAMPLES: usize = 12;
/// 设置里那个「过滤系统应用」开关；没写过就当作开着
const SYSTEM_SWITCH: &str = "filter_system_apps";
/// Windows 记「装了什么东西」的那把键
const UNINSTALL_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
/// 32 位程序装的在 WOW6432Node 那份视图里，只读一个视图会漏一半
const VIEWS: [(Hive, Arch); 4] = [
    (Hive::Local, Arch::Native),
    (Hive::Local, Arch::Wow32),
    (Hive::Current, Arch::Native),
    (Hive::Current, Arch::Wow32),
];

/// 一次同步（或试算）的账单。界面照着它说话，不然「点了没反应」和「本来就没有新的」分不出来。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    /// 注册表里读到多少条有名字的卸载项（32/64 两份都算在里面）
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

/// 从四个视图把卸载项读出来。打不开的视图跳过，某一条读不动也跳过：
/// 注册表里本来就有半截的项，不该因为它们放弃整页
pub(crate) fn read_entries() -> Vec<Entry> {
    let mut out = Vec::new();
    for (hive, arch) in VIEWS {
        let Some(root) = Key::open(hive, UNINSTALL_SUBKEY, arch) else {
            continue;
        };
        for sub in root.child_names() {
            let path = format!(r"{}\{}", UNINSTALL_SUBKEY, sub);
            let Some(item) = Key::open(hive, &path, arch) else {
                continue;
            };
            let text = |name: &str| item.string(name).unwrap_or_default().trim().to_string();
            let name = text("DisplayName");
            if name.is_empty() {
                continue;
            }
            out.push(Entry {
                version: text("DisplayVersion"),
                publisher: text("Publisher"),
                install_location: text("InstallLocation"),
                display_icon: text("DisplayIcon"),
                uninstall: item
                    .string("UninstallString")
                    .or_else(|| item.string("QuietUninstallString"))
                    .unwrap_or_default(),
                reg_key: item.path().to_string(),
                system_component: item.dword("SystemComponent") == Some(1),
                is_patch: !text("ParentKeyName").is_empty() || !text("ReleaseType").is_empty(),
                name,
            });
        }
    }
    out
}

/// 把开始菜单收成一本「名字 → 快捷方式」的索引：注册表只管「装了什么」，
/// 「怎么打开」它说不准，而快捷方式的名字是给人看的、又能解析出真 exe
pub(crate) fn shortcut_index(roots: &[PathBuf]) -> Vec<Shortcut> {
    let mut out = Vec::new();
    for root in roots {
        collect_shortcuts(root, 0, &mut out);
    }
    out
}

/// 真正干活的一层：条目、快捷方式索引、图标缓存都从外面进来，测试就能全程离线跑
pub(crate) fn plan(
    conn: &Connection,
    entries: &[Entry],
    shortcuts: &[Shortcut],
    icon_dir: &Path,
    dry: bool,
) -> AppResult<SyncReport> {
    let filters = read_filters(conn)?;
    let system_root = system_root();
    let _ = std::fs::create_dir_all(icon_dir);

    // 「见过」按两种写法算：卡片存的目标，以及条目的名字
    //（开始菜单那批存的是解析出的 exe，注册表这条常常是同一个文件）
    let mut known: HashSet<String> = HashSet::new();
    let mut stmt = conn.prepare("SELECT name, target FROM apps")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows.flatten() {
        known.insert(name_key(&row.0));
        known.insert(path_key(&row.1));
    }

    let mut report = SyncReport {
        scanned: entries.len(),
        inserted: 0,
        filtered: 0,
        already: 0,
        unresolved: 0,
        total: 0,
        samples: Vec::new(),
    };

    // 顺序是有意为之：先挡（过滤名单/系统应用），再认（库里已有），最后才说「找不到目标」。
    // 反过来会把一百多条本来就不收的系统组件报成「认不出怎么打开」，那个数字就没意义了。
    for entry in entries {
        if entry.name.trim().is_empty() {
            continue;
        }
        let resolved = resolve(entry, shortcuts);
        if hits(entry, &resolved.target, &filters, &system_root) {
            report.filtered += 1;
            if report.samples.len() < SAMPLES {
                report.samples.push(entry.name.clone());
            }
            continue;
        }
        let key = name_key(&entry.name);
        let target_key = path_key(&resolved.target);
        let seen = (!key.is_empty() && known.contains(&key))
            || (!target_key.is_empty() && known.contains(&target_key));
        if seen {
            report.already += 1;
            continue;
        }
        if resolved.found == Found::Nothing {
            report.unresolved += 1;
            continue;
        }
        if !dry {
            let mut candidates: Vec<&str> = resolved.icons.iter().map(|s| s.as_str()).collect();
            candidates.push(resolved.target.as_str());
            let icon = extract_icon(icon_dir, &candidates);
            conn.execute(
                "INSERT INTO apps (name, description, target, source, icon_path)
                 VALUES (?1, ?2, ?3, 'registry', ?4)",
                params![entry.name, describe(entry), resolved.target, icon],
            )?;
        }
        known.insert(key);
        known.insert(target_key);
        report.inserted += 1;
    }

    report.total = if dry {
        count_rows(conn)? + report.inserted
    } else {
        count_rows(conn)?
    };
    Ok(report)
}

/// 发行商和版本记在描述里：注册表比快捷方式多的就是这两个信息
fn describe(entry: &Entry) -> Option<String> {
    let parts: Vec<&str> = [&entry.publisher, &entry.version]
        .into_iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

/// 按六级顺序找启动目标；前一级落空才走下一级
pub(crate) fn resolve(entry: &Entry, shortcuts: &[Shortcut]) -> Resolved {
    let icon = relocated(&clean_path(&entry.display_icon)).unwrap_or_default();
    let mut icons = Vec::new();
    if !icon.is_empty() {
        icons.push(icon.clone());
    }

    // 1. DisplayIcon 自己就是个真存在的 exe（`app.exe,0` 那种已经去过序号）
    if usable_exe(&icon) {
        return Resolved { target: icon.clone(), icons, found: Found::Exe };
    }
    // 1b. DisplayIcon 指的是一个快捷方式：那 shell 会把它的 IconLocation 和目标都算好
    if extension_of(Path::new(&icon)) == "lnk" && Path::new(&icon).is_file() {
        let target = launch_target(Path::new(&icon));
        if launchable(&target) {
            return Resolved { target, icons, found: Found::Exe };
        }
    }

    // 2. 开始菜单的同名快捷方式：目标准，图标也是桌面上那张
    if let Some(sc) = find_shortcut(&entry.name, shortcuts) {
        let mut both = vec![sc.lnk.clone()];
        both.extend(icons.iter().cloned());
        return Resolved { target: sc.target.clone(), icons: both, found: Found::Shortcut };
    }

    let location = relocated(&clean_path(&entry.install_location)).unwrap_or_default();
    // 3. 安装目录自己就是个 exe
    if usable_exe(&location) {
        return Resolved { target: location.clone(), icons, found: Found::Exe };
    }
    // 4. 注册表不写 InstallLocation 是常事，但卸载器和 DisplayIcon 走不了，于是它们所在的
    //    目录就是唯一说到位置的地方——进去找那个和程序名对得上的 exe
    let mut dirs: Vec<String> = Vec::new();
    if !location.is_empty() && Path::new(&location).is_dir() {
        dirs.push(location.clone());
    }
    let uninstall = relocated(&clean_path(&entry.uninstall)).unwrap_or_default();
    for hint in [icon.as_str(), uninstall.as_str()] {
        if let Some(dir) = parent_dir(hint) {
            if !dirs.iter().any(|d| path_key(d) == dir) && !path_key(&dir).contains(PACKAGE_CACHE_MARK) {
                dirs.push(dir);
            }
        }
    }
    for dir in &dirs {
        if let Some(exe) = exe_named_like(dir, &entry.name) {
            return Resolved { target: exe, icons, found: Found::Exe };
        }
    }

    // 5. 卸载串里那个不是卸载器的 exe（有的程序把主程序和卸载器放一起）
    if let Some(exe) = command_exe(&entry.uninstall) {
        if usable_exe(&exe) {
            return Resolved { target: exe, icons, found: Found::Exe };
        }
    }

    // 6. 兜底：点开是安装目录，总比一张点不动的卡片有用
    match dirs.first() {
        Some(dir) => Resolved { target: dir.clone(), icons, found: Found::Folder },
        None => Resolved { target: String::new(), icons, found: Found::Nothing },
    }
}

/// 一个路径的所在目录（文件才算，目录本身不算）
fn parent_dir(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let p = Path::new(path);
    if !p.is_file() {
        return None;
    }
    Some(p.parent()?.to_string_lossy().to_string())
}

/// 装完又自己搬过目录的软件，注册表里那条还是老写法：整条路径不存在时，往上退一层再认一次。
/// Navicat 就是这样——`InstallLocation` 写成 `F:\Program\Navicat Premium 15\Navicat Premium 15\`，
/// 东西实际在上一层。只退一层：再往上就会撞进无关的兄弟目录了。
fn relocated(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let p = Path::new(path);
    if p.exists() {
        return Some(path.to_string());
    }
    let up = p.parent()?;
    let candidate = if p.extension().is_some() {
        // 看着像文件：去掉最后那段目录，文件名留着
        up.parent()?.join(p.file_name()?)
    } else {
        // 看着像目录：直接少最后一段
        up.to_path_buf()
    };
    // 退一层以后要么整条不存在，要么就是个空目录——那不算救回来
    if !candidate.exists() {
        return None;
    }
    Some(candidate.to_string_lossy().to_string())
}

/// 名字对上就取它；注册表里的名字常带厂商前缀和版本号，所以先严格相等、再去掉版本号、
/// 最后才允许互相包含（短的当名字主体）
fn find_shortcut<'a>(name: &str, shortcuts: &'a [Shortcut]) -> Option<&'a Shortcut> {
    let key = name_key(name);
    if key.is_empty() {
        return None;
    }
    if let Some(hit) = shortcuts.iter().find(|s| s.key == key) {
        return Some(hit);
    }
    let short = short_key(name);
    if short.len() >= 4 {
        if let Some(hit) = shortcuts.iter().find(|s| s.key == short) {
            return Some(hit);
        }
    }
    let mut best: Option<&Shortcut> = None;
    for s in shortcuts {
        if s.key.len() < 4 {
            continue;
        }
        if (s.key.contains(&key) || key.contains(&s.key))
            && best.map(|b| s.key.len() < b.key.len()).unwrap_or(true)
        {
            best = Some(s);
        }
    }
    best
}

/// 目录顶层那个和程序名对得上的 exe：先是全等，再去掉版本号，最后允许「exe 名是产品名的
/// 一段」——`navicat.exe` 对 `PremiumSoft Navicat Premium 15.0`、`charles.exe` 对 `Charles 4.6.7`
/// 都是靠这一段才对上的
fn exe_named_like(dir: &str, name: &str) -> Option<String> {
    let key = name_key(name);
    let short = short_key(name);
    let entries = std::fs::read_dir(dir).ok()?;
    let mut loose: Option<String> = None;
    let mut contains: Option<String> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let as_str = path.to_string_lossy().to_string();
        if !usable_exe(&as_str) {
            continue;
        }
        let stem = name_key(Path::new(&as_str).file_stem()?.to_string_lossy().as_ref());
        if stem.is_empty() {
            continue;
        }
        if stem == key {
            return Some(as_str);
        }
        if short.len() >= 4 && stem == short {
            loose = Some(as_str.clone());
        } else if stem.len() >= 5 && (key.contains(&stem) || stem.contains(&key)) {
            contains = Some(as_str);
        }
    }
    loose.or(contains)
}

fn count_rows(conn: &Connection) -> AppResult<usize> {
    Ok(conn.query_row("SELECT COUNT(*) FROM apps", [], |r| r.get::<_, i64>(0))? as usize)
}

fn read_filters(conn: &Connection) -> AppResult<Filters> {
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
fn hits(entry: &Entry, target: &str, filters: &Filters, system_root: &str) -> bool {
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
fn is_system(entry: &Entry, target: &str, system_root: &str) -> bool {
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

/// MSI 安装器缓存住的那个目录：里面的 exe 是重装/修复用的，不是程序本体
const PACKAGE_CACHE_MARK: &str = "/package cache/";

fn system_root() -> String {
    std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string())
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

/// `C:\P\app.exe,0`、`"C:\P\a b.exe" /X`、`%ProgramFiles%\Foo\foo.exe` 这些写法
/// 收成一条能拿去判存在的路径
pub(crate) fn clean_path(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    // 末尾的 `,0` / `,-1` 是图标序号，只有确实是整数时才剥
    let body = match raw.rsplit_once(',') {
        Some((head, tail)) if tail.trim().parse::<i32>().is_ok() => head.trim_end(),
        _ => raw,
    };
    // 带引号取引号里那段（`"C:\Program Files\App\a.exe" /arg`）
    let picked = match body.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(rest),
        None => body,
    };
    expand_vars(picked.trim()).trim().to_string()
}

/// 卸载/安装串里的第一个可执行文件路径。MSI 那种 `MsiExec.exe /X{GUID}` 不带目录，
/// 展开后文件不存在，调用方按「没找到」处理
fn command_exe(cmd: &str) -> Option<String> {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return None;
    }
    let head = match cmd.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(rest),
        None => cmd.split_whitespace().next().unwrap_or(cmd),
    };
    let expanded = expand_vars(head.trim()).trim().to_string();
    if expanded.is_empty() {
        None
    } else {
        Some(expanded)
    }
}

/// 只做 `%NAME%` 替换；展开不出来的变量原样留着，不能把路径改成空串
fn expand_vars(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        match rest[start + 1..].find('%') {
            Some(end) => {
                let span = &rest[start..start + end + 2];
                let name = &span[1..span.len() - 1];
                match std::env::var(name) {
                    Ok(value) => out.push_str(&value),
                    Err(_) => out.push_str(span),
                }
                rest = &rest[start + end + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn is_exe_file(path: &str) -> bool {
    !path.is_empty() && extension_of(Path::new(path)) == "exe" && Path::new(path).is_file()
}

/// 能拿去当启动目标的 exe：文件在、是 exe、不是卸载器/安装器，也不是 MSI 缓存在
/// `ProgramData\Package Cache` 里的那个外壳—— burn engine 存的是重装/修复程序，
/// 点它会弹出安装向导，Docker 和一堆华硕组件的 DisplayIcon 都写的这种路径。
fn usable_exe(path: &str) -> bool {
    is_exe_file(path) && !is_uninstaller(path) && !path_key(path).contains(PACKAGE_CACHE_MARK)
}

/// 卸载/安装器不是「打开应用」：`unins000.exe`、`Docker Desktop Installer.exe`、`某某卸载.exe` 都算
fn is_uninstaller(path: &str) -> bool {
    let name = Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    name.starts_with("unins") || name.contains("uninstall") || name.contains("installer") || name.contains("卸载")
}

fn launchable(target: &str) -> bool {
    if target.contains("://") {
        return true;
    }
    let path = Path::new(target);
    if !path.is_file() {
        return false;
    }
    // 目标是图标文件的那种快捷方式是安装程序顺手放的（Charles、Cpolar 都这样），
    // 点它只会开一个图片查看器，不算一个应用
    !matches!(extension_of(path).as_str(), "ico" | "cur")
}

fn collect_shortcuts(dir: &Path, depth: usize, out: &mut Vec<Shortcut>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_shortcuts(&path, depth + 1, out);
            continue;
        }
        let file_name = match path.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };
        if !file_name.to_ascii_lowercase().ends_with(".lnk") || is_uninstaller(&file_name) {
            continue;
        }
        let target = launch_target(&path);
        // 解析不出可启动目标的（目标文件被删了、指向一个目录）不进索引
        if !launchable(&target) {
            continue;
        }
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(file_name);
        out.push(Shortcut {
            key: name_key(&stem),
            lnk: path.to_string_lossy().to_string(),
            target,
        });
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(include_str!("../db/schema.sql")).unwrap();
        c
    }

    fn sandbox(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("workbench-registry-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(dir: &Path, name: &str) -> String {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&p, b"MZ\x90\x00not a real binary").unwrap();
        p.to_string_lossy().to_string()
    }

    fn entry(name: &str) -> Entry {
        Entry {
            name: name.to_string(),
            reg_key: format!(r"HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\{name}"),
            ..Default::default()
        }
    }

    fn rows(c: &Connection) -> Vec<String> {
        c.prepare("SELECT name || '|' || source || '|' || COALESCE(description,'') FROM apps ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn plan_with(conn: &Connection, entries: &[Entry], shortcuts: &[Shortcut], icons: &Path, dry: bool) -> SyncReport {
        plan(conn, entries, shortcuts, icons, dry).unwrap()
    }

    fn inserted(report: SyncReport) -> usize {
        report.inserted
    }

    /// DisplayIcon 里那个真存在的 exe 是最准的目标，`,0` 那种序号得剥掉
    #[test]
    fn a_display_icon_exe_becomes_the_target() {
        let dir = sandbox("display-icon");
        let exe = touch(&dir, r"Foo\foo.exe");
        let e = Entry { display_icon: format!("{exe},0"), ..entry("Foo") };

        let r = resolve(&e, &[]);
        assert_eq!(r.found, Found::Exe);
        assert_eq!(r.target, exe);
        assert_eq!(r.icons, vec![exe.clone()]);

        let c = conn();
        let report = plan_with(&c, std::slice::from_ref(&e), &[], &dir.join("icons"), false);
        assert_eq!(report.inserted, 1, "{report:?}");
        assert_eq!(rows(&c), vec!["Foo|registry|".to_string()]);
    }

    /// 卸载器图标不能当目标：MSI 那一堆 `C:\Windows\Installer\{GUID}\ARPIcon.ico`    /// 和 `uninstallerIcon.ico` 都只是图标，卡片点开该是安装目录
    #[test]
    fn an_uninstaller_icon_is_an_icon_not_a_target() {
        let dir = sandbox("uninstaller-icon");
        let ico = touch(&dir, r"Foo\uninstallerIcon.ico");
        let unins = touch(&dir, r"Foo\unins000.exe");
        let install = dir.join("Foo");
        let e = Entry {
            display_icon: ico.clone(),
            uninstall: format!(r#""{unins}" /SILENT"#),
            install_location: install.to_string_lossy().to_string(),
            ..entry("Foo")
        };

        let r = resolve(&e, &[]);
        assert_eq!(r.found, Found::Folder, "卸载器不该被当成打开应用");
        assert_eq!(r.target, install.to_string_lossy().to_string());
        // 图标还是从 DisplayIcon 抽：那张就是桌面上看到的
        assert_eq!(r.icons, vec![ico]);
    }

    /// 名单里的模式常是从卡片名字照抄的，带着括号和版本号：这种写法也得命中名字
    #[test]
    fn a_pattern_copied_from_the_card_name_still_blocks_it() {
        let dir = sandbox("punct-pattern");
        let exe = touch(&dir, r"Zip\7zFM.exe");
        let c = conn();
        add_app_filter(&c, "7-Zip 23.01 (x64)").unwrap();
        let e = Entry { display_icon: exe, ..entry("7-Zip 23.01 (x64)") };

        let report = plan_with(&c, &[e], &[], &dir.join("icons"), false);
        assert_eq!(report.filtered, 1, "{report:?}");
        assert_eq!(report.inserted, 0);
        assert_eq!(report.samples, vec!["7-Zip 23.01 (x64)".to_string()]);
    }

    /// 卸载器、`Docker Desktop Installer.exe` 这种安装器、还有 Package Cache 里那个
    /// setup 外壳，点开都是安装/修复向导，不是程序本体：一律不算目标
    #[test]
    fn an_installer_exe_is_never_the_target() {
        let dir = sandbox("installer");
        let installer = touch(&dir, r"Docker\Docker Desktop Installer.exe");
        let cached = touch(&dir, r"Package Cache\{62b10058}\AuraServiceSetup.exe");
        std::fs::create_dir_all(dir.join("Aura")).unwrap();

        let by_name = resolve(&Entry { display_icon: installer, ..entry("Docker Desktop") }, &[]);
        assert_eq!(by_name.found, Found::Folder, "安装器 exe 不该被当成目标");
        assert_eq!(by_name.target, dir.join("Docker").to_string_lossy().to_string());

        let by_path = resolve(
            &Entry {
                display_icon: cached,
                install_location: dir.join("Aura").to_string_lossy().to_string(),
                ..entry("AURA Service")
            },
            &[],
        );
        assert_eq!(by_path.found, Found::Folder, "Package Cache 里那个是重装程序");
        assert_eq!(by_path.target, dir.join("Aura").to_string_lossy().to_string());
    }

    #[test]
    fn a_moved_install_folder_still_resolves() {
        let dir = sandbox("moved");
        let real = touch(&dir, r"Navicat Premium 15\navicat.exe");
        let stale = dir.join(r"Navicat Premium 15\Navicat Premium 15\navicat.exe");
        let e = Entry {
            display_icon: stale.to_string_lossy().to_string(),
            install_location: stale.parent().unwrap().to_string_lossy().to_string(),
            ..entry("PremiumSoft Navicat Premium 15.0")
        };

        let r = resolve(&e, &[]);
        assert_eq!(r.found, Found::Exe, "搬过目录留下的旧路径要能退一层认出来");
        assert_eq!(path_key(&r.target), path_key(&real));
    }

    #[test]
    fn the_uninstaller_folder_holds_the_real_program() {
        let dir = sandbox("unins-folder");
        let exe = touch(&dir, r"Navicat\navicat.exe");
        let unins = touch(&dir, r"Navicat\unins000.exe");
        // Inno Setup 那一类：InstallLocation 是空的，只有卸载串说到这个目录
        let e = Entry {
            uninstall: format!(r#""{unins}""#),
            ..entry("PremiumSoft Navicat Premium 15.0")
        };

        let r = resolve(&e, &[]);
        assert_eq!(r.found, Found::Exe, "卸载器旁边就是主程序，得进去找");
        assert_eq!(path_key(&r.target), path_key(&exe));
    }

    #[test]
    fn the_package_cache_folder_is_not_scanned_for_a_program() {
        let dir = sandbox("cache-scan");
        touch(&dir, r"Package Cache\{62b10058}\tool.exe");
        let unins = touch(&dir, r"Package Cache\{62b10058}\unins000.exe");
        let e = Entry { uninstall: format!(r#""{unins}""#), ..entry("Some MSI Thing") };

        let r = resolve(&e, &[]);
        assert_eq!(r.found, Found::Nothing, "MSI 缓存目录里不该被翻出一个「主程序」");
    }

    /// 开始菜单的同名快捷方式排在安装目录前面：目标准、图标也是 shell 算好的那张
    #[test]
    fn a_same_name_shortcut_beats_the_folder_fallback() {
        let dir = sandbox("shortcut");
        let real = touch(&dir, r"Loomy\loomy.exe");
        let install = dir.join("Loomy");
        let sc = Shortcut {
            key: name_key("Loomy"),
            lnk: dir.join("Loomy.lnk").to_string_lossy().to_string(),
            target: real.clone(),
        };
        let e = Entry {
            install_location: install.to_string_lossy().to_string(),
            ..entry("Loomy")
        };

        let r = resolve(&e, std::slice::from_ref(&sc));
        assert_eq!(r.found, Found::Shortcut);
        assert_eq!(r.target, real);
        assert_eq!(r.icons[0], sc.lnk, "抽图标要从快捷方式来");
    }

    /// 安装目录顶层那个和程序同名的 exe 也算找到了
    #[test]
    fn an_exe_named_like_the_program_inside_the_install_dir_is_used() {
        let dir = sandbox("dir-exe");
        let exe = touch(&dir, r"Gimp\gimp.exe");
        touch(&dir, r"Gimp\unins000.exe");
        let e = Entry { install_location: dir.join("Gimp").to_string_lossy().to_string(), ..entry("GIMP") };

        let r = resolve(&e, &[]);
        assert_eq!(r.found, Found::Exe);
        assert_eq!(path_key(&r.target), path_key(&exe));
    }

    /// 什么路径都没有的：报数，不收一张点不动的卡片
    #[test]
    fn an_entry_without_any_path_is_counted_not_collected() {
        let dir = sandbox("nothing");
        let c = conn();
        let report = plan_with(&c, &[entry("幽灵程序")], &[], &dir.join("icons"), false);
        assert_eq!(report.unresolved, 1, "{report:?}");
        assert_eq!(report.inserted, 0);
        assert_eq!(rows(&c), Vec::<String>::new());
    }

    /// 同一个程序在 64 位和 32 位视图里各登记一次，只该收一张
    #[test]
    fn the_same_product_in_two_views_is_collected_once() {
        let dir = sandbox("two-views");
        let exe = touch(&dir, r"Two\two.exe");
        let mut first = Entry { display_icon: exe.clone(), ..entry("Two Tool") };
        first.version = "1.2.3".into();
        first.publisher = "某某科技".into();
        let second = Entry { display_icon: exe.clone(), ..entry("Two Tool") };

        let c = conn();
        let report = plan_with(&c, &[first, second], &[], &dir.join("icons"), false);
        assert_eq!(report.inserted, 1, "{report:?}");
        assert_eq!(report.already, 1, "第二条得认出是同一个");
        assert_eq!(rows(&c), vec!["Two Tool|registry|某某科技 · 1.2.3"]);
    }

    /// 「应用」页那批开始菜单卡片存的是解析出的 exe：名字或路径任一对上就不重复收
    #[test]
    fn a_card_from_the_start_menu_page_blocks_the_registry_row() {
        let dir = sandbox("startmenu-block");
        let exe = touch(&dir, r"Docker\docker-desktop.exe");
        let c = conn();
        c.execute(
            "INSERT INTO apps (name, target, source) VALUES ('Docker Desktop', ?1, 'startmenu')",
            params![path_key(&exe).replace('/', r"\")],
        )
        .unwrap();
        let e = Entry { display_icon: exe, ..entry("Docker Desktop") };

        let report = plan_with(&c, &[e], &[], &dir.join("icons"), false);
        assert_eq!(report.inserted, 0, "{report:?}");
        assert_eq!(report.already, 1);
        assert_eq!(rows(&c), vec!["Docker Desktop|startmenu|"]);
    }

    /// 老卡片只改过名字、路径认不出时，新条目照常收，但那一行一个字都不动
    #[test]
    fn sync_never_touches_an_existing_row() {
        let dir = sandbox("no-clobber");
        let exe = touch(&dir, r"Loomy\loomy.exe");
        let c = conn();
        c.execute(
            "INSERT INTO apps (name, target, source, category_id, alias) VALUES ('我的洛米', 'F:/other/Loomy.exe', 'manual', NULL, 'l')",
            [],
        )
        .unwrap();
        let e = Entry { display_icon: exe, ..entry("Loomy") };

        plan_with(&c, &[e], &[], &dir.join("icons"), false);
        let (name, target): (String, String) = c
            .query_row("SELECT name, target FROM apps WHERE alias = 'l'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((name.as_str(), target.as_str()), ("我的洛米", "F:/other/Loomy.exe"));
        assert!(rows(&c).contains(&"Loomy|registry|".to_string()));
    }

    /// 删掉的卡片下次同步照样收回来：过滤名单在 app_filters，不是删一次的副产品
    #[test]
    fn a_deleted_card_comes_back_on_the_next_sync() {
        let dir = sandbox("resync");
        let exe = touch(&dir, r"Game\launcher.exe");
        let e = Entry { display_icon: exe.clone(), ..entry("Game Launcher") };
        let icons = dir.join("icons");
        let c = conn();

        assert_eq!(inserted(plan_with(&c, std::slice::from_ref(&e), &[], &icons, false)), 1);
        let id: i64 = c.query_row("SELECT id FROM apps", [], |r| r.get(0)).unwrap();
        crate::services::desktop::delete_app(&c, id).unwrap();
        assert_eq!(rows(&c), Vec::<String>::new());
        assert_eq!(inserted(plan_with(&c, &[e], &[], &icons, false)), 1);
    }

    /// 名单能挡名字，也能挡安装目录、注册表键和 DisplayIcon 路径
    #[test]
    fn a_filtered_entry_is_counted_not_collected() {
        let dir = sandbox("filtered");
        let exe = touch(&dir, r"助手\x.exe");
        let icons = dir.join("icons");
        let c = conn();
        c.execute("INSERT INTO app_filters (pattern) VALUES ('助手')", []).unwrap();

        let by_name = Entry { display_icon: exe.clone(), ..entry("某某助手") };
        let report = plan_with(&c, &[by_name, entry("Loomy")], &[], &icons, false);
        assert_eq!(report.inserted, 0, "{report:?}");
        assert_eq!(report.filtered, 1, "名字里带模式的这条要挡住");
        assert_eq!(report.unresolved, 1, "Loomy 什么路径都没有");
        assert_eq!(report.samples, vec!["某某助手".to_string()]);
        assert_eq!(rows(&c), Vec::<String>::new());

        // 路径写法也能挡：DisplayIcon 里那段目录就在名单里
        let mut second = Entry { display_icon: exe.clone(), ..entry("另一个") };
        second.install_location = dir.join("助手").to_string_lossy().to_string();
        let c2 = conn();
        c2.execute("INSERT INTO app_filters (pattern) VALUES ('助手')", []).unwrap();
        let report = plan_with(&c2, &[second], &[], &icons, false);
        assert_eq!(report.filtered, 1, "{report:?}");
    }

    /// 「系统应用」在注册表里的三种长相都要挡；但 MSI 的图标住在 Windows 目录里，
    /// 那不能算系统应用——否则正经软件会被成片挡掉
    #[test]
    fn system_entries_are_filtered_and_msi_icons_are_not() {
        let dir = sandbox("system");
        let exe = touch(&dir, r"Real\real.exe");
        let root = r"C:\Windows";

        let with_system = Filters { patterns: vec![], system: true };
        let no_system = Filters { patterns: vec![], system: false };

        let mut component = entry("某系统组件");
        component.system_component = true;
        assert!(hits(&component, &exe, &with_system, root));

        let mut patch = entry("某程序 1.2 补丁");
        patch.is_patch = true;
        assert!(hits(&patch, &exe, &with_system, root));

        assert!(hits(&entry("Microsoft Edge Update"), &exe, &with_system, root), "更新程序是附属程序");
        assert!(hits(&entry("Microsoft Visual C++ 2015  redistributable - x64"), &exe, &with_system, root));
        // 驱动
        assert!(hits(&entry("Realtek High Definition Audio Driver"), &exe, &with_system, root));
        assert!(hits(&entry("英特尔(R) 无线显示器驱动"), &exe, &with_system, root));
        assert!(hits(&entry("Microsoft ODBC Driver 17 for SQL Server"), &exe, &with_system, root));
        assert!(hits(&entry("AMD Chipset Software"), &exe, &with_system, root));
        // 运行环境
        assert!(hits(&entry("Microsoft .NET Runtime 8.0.5 (x64)"), &exe, &with_system, root));
        assert!(hits(&entry("DirectX 运行库"), &exe, &with_system, root));
        assert!(hits(&entry("Microsoft Edge WebView2 Runtime"), &exe, &with_system, root));
        assert!(hits(&entry("某某常用组件"), &exe, &with_system, root));
        // 他自己装来用的东西不算环境：这些要是被挡了，就是词根切得太狠
        assert!(!hits(&entry("Python 3.11.7 (64-bit)"), &exe, &with_system, root));
        assert!(!hits(&entry("Node.js 20.11.0"), &exe, &with_system, root));
        assert!(!hits(&entry("Audacity 3.5.1"), &exe, &with_system, root));
        assert!(!hits(&entry("Android Studio"), &exe, &with_system, root));
        let mut under_windows = entry("资源管理器附带");
        under_windows.install_location = format!(r"{root}\System32");
        assert!(hits(&under_windows, &exe, &with_system, root));

        // 正经软件：图标在 C:\Windows\Installer 里，安装在别处
        let msi = Entry {
            display_icon: format!(r"{root}\Installer\{{GUID}}\ARPIcon.ico,0"),
            install_location: r"F:\Program\Real".to_string(),
            uninstall: format!(r#""{exe}" /uninstall"#),
            ..entry("Real Thing")
        };
        assert!(!hits(&msi, &exe, &with_system, root), "MSI 图标在 Windows 目录里不该被当成系统应用");
        // 关掉开关，系统组件也不挡了
        assert!(!hits(&component, &exe, &no_system, root));
    }

    /// 试算只看不动库：数字要和真同步一致，但一行都不写
    #[test]
    fn a_preview_counts_without_writing_anything() {
        let dir = sandbox("preview");
        let a = touch(&dir, "A\\a.exe");
        let b = touch(&dir, "B\\b.exe");
        let entries = vec![
            Entry { display_icon: a, ..entry("Aaaa") },
            Entry { display_icon: b, ..entry("Bbbb") },
        ];
        let icons = dir.join("icons");
        let c = conn();

        let preview = plan_with(&c, &entries, &[], &icons, true);
        assert_eq!(preview.inserted, 2, "{preview:?}");
        assert_eq!(preview.total, 2, "试算要把「同步之后会长成多少张」说出来");
        assert_eq!(rows(&c), Vec::<String>::new(), "试算不该写库");

        let real = plan_with(&c, &entries, &[], &icons, false);
        assert_eq!(real.inserted, preview.inserted);
        assert_eq!(real.total, 2);
    }

    /// 名单本身要能加、能去重、能删；空模式挡不住任何东西，直接拒绝
    #[test]
    fn the_filter_list_adds_once_and_deletes() {
        let c = conn();
        assert!(add_app_filter(&c, " 助手 ").unwrap());
        assert!(!add_app_filter(&c, "助手").unwrap(), "同一条不该记两遍");
        assert!(add_app_filter(&c, "   ").is_err(), "空模式不挡任何东西");
        assert_eq!(get_app_filters(&c).unwrap(), vec!["助手".to_string()]);
        assert!(remove_app_filter(&c, "助手").unwrap());
        assert!(get_app_filters(&c).unwrap().is_empty());
    }

    /// 「过滤系统应用」这个开关默认是开的，只有明写成 "0" 才算关
    #[test]
    fn the_system_switch_is_on_unless_explicitly_cleared() {
        let c = conn();
        assert!(read_filters(&c).unwrap().system, "没写过就该当开着");
        crate::services::settings::set_setting(&c, SYSTEM_SWITCH, "0").unwrap();
        assert!(!read_filters(&c).unwrap().system);
        crate::services::settings::set_setting(&c, SYSTEM_SWITCH, "1").unwrap();
        assert!(read_filters(&c).unwrap().system);
    }

    /// 注册表里的路径写法五花八门：图标序号、引号包着带空格的目录、后面还跟着参数、
    /// `%变量%` 没展开
    #[test]
    fn messy_registry_paths_come_out_as_one_usable_path() {
        std::env::set_var("WORKBENCH_TEST_DIR", r"F:\Program\Two Words");
        assert_eq!(
            clean_path(r#"%WORKBENCH_TEST_DIR%\app.exe, 0"#),
            r"F:\Program\Two Words\app.exe"
        );
        assert_eq!(
            clean_path(r#""F:\Program\Two Words\app.exe" --silent"#),
            r"F:\Program\Two Words\app.exe"
        );
        assert_eq!(clean_path(r"F:\a,b\c.exe"), r"F:\a,b\c.exe", "路径里真有逗号时别乱剥");
        assert_eq!(clean_path(r"%这个变量不存在%\x.exe"), r"%这个变量不存在%\x.exe", "展开不了的原样留着");
        assert_eq!(clean_path(""), "");
        assert_eq!(command_exe(r#""F:\P\app.exe" /uninstall"#).as_deref(), Some(r"F:\P\app.exe"));
        assert_eq!(command_exe("MsiExec.exe /X{GUID}").as_deref(), Some("MsiExec.exe"));
        assert_eq!(command_exe(""), None);
        assert!(is_uninstaller(r"F:\P\unins000.exe"));
        assert!(is_uninstaller(r"F:\P\某某卸载.exe"));
        assert!(!is_uninstaller(r"F:\P\app.exe"));
    }

    /// 名字比对：版本号和括号那些尾巴不该把同一个程序拆成两个
    #[test]
    fn names_match_across_version_tails_and_slashes() {
        assert_eq!(name_key(" 某某 助手 2.1 (x64) "), name_key("某某助手21x64"));
        assert_eq!(short_key("Python 3.11.7"), short_key("Python"));
        assert_eq!(path_key(r"F:\Program\App.exe"), path_key("F:/Program/App.exe"));

        let sc = [Shortcut {
            key: name_key("Loomy"),
            lnk: "L:\\Loomy.lnk".into(),
            target: "L:\\Loomy\\loomy.exe".into(),
        }];
        // 注册表那条多了版本号：严格相等对不上，去版本号才对上
        assert!(find_shortcut("Loomy 1.2.3", &sc).is_some());
        assert!(find_shortcut("NVIDIA Loomy 1.2", &sc).is_some());
        assert!(find_shortcut("完全不同的东西", &sc).is_none());
    }

    /// 快捷方式索引只收真能启动的：卸载器、目录、解析不出目标的都不进
    #[test]
    fn the_shortcut_index_keeps_only_launchable_entries() {
        let dir = sandbox("index");
        let programs = dir.join("Programs");
        std::fs::create_dir_all(programs.join("厂商")).unwrap();
        touch(&programs, "Loomy.lnk");
        touch(&programs.join("厂商"), "Nested.lnk");
        touch(&programs, "uninstall.lnk");
        touch(&programs, "某某 卸载.lnk");
        touch(&programs, "readme.txt");
        std::fs::create_dir_all(programs.join("Gone.lnk")).unwrap();

        let index = shortcut_index(std::slice::from_ref(&programs));
        let mut keys: Vec<String> = index.iter().map(|s| s.key.clone()).collect();
        keys.sort();
        assert_eq!(keys, vec!["loomy".to_string(), "nested".to_string()], "{index:?}");
    }

    /// 真机核对一：拿这台机器的注册表跑一遍，看四个视图一共读出多少条、
    /// 各级目标找到多少、图标抽到几张。库是内存库，不动他的数据。
    /// 只有解析快捷方式那步要 COM，测试线程得自己初始化。
    #[test]
    #[ignore]
    fn the_real_registry_lists_installed_programs() {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let entries = read_entries();
        assert!(entries.len() > 20, "注册表只读出 {} 条，多半是视图没开对", entries.len());
        let shortcuts = shortcut_index(
            &crate::win::desktop::all_program_menus()
                .into_iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>(),
        );
        let mut tiers = [0usize; 4];
        for e in &entries {
            tiers[match resolve(e, &shortcuts).found {
                Found::Exe => 0,
                Found::Shortcut => 1,
                Found::Folder => 2,
                Found::Nothing => 3,
            }] += 1;
        }
        println!(
            "注册表读出 {} 条（快捷方式索引 {} 条）：exe {} / 快捷方式 {} / 只剩目录 {} / 什么都没找到 {}",
            entries.len(),
            shortcuts.len(),
            tiers[0],
            tiers[1],
            tiers[2],
            tiers[3]
        );
        // 「哪些条目连目录都没给出来」是最容易挨问的一类：名字和卸载键末段一起打出来，
        // 下次他说某个软件没进卡片，先在这里看它是不是压根没登记卸载项
        println!("  什么都没找到的那些：");
        for e in entries.iter().filter(|e| resolve(e, &shortcuts).found == Found::Nothing).take(30) {
            println!(
                "    - {} | {}",
                e.name,
                e.reg_key.rsplit('\\').next().unwrap_or("")
            );
        }
        for e in &entries[..entries.len().min(15)] {
            let r = resolve(e, &shortcuts);
            println!("  {} [{:?}] -> {}", e.name, r.found, r.target);
        }

        // 把「按系统/驱动/环境挡掉了谁」整片打出来：这条规则最怕的就是切太狠，
        // 光看个数看不出来，得逐个名字过一遍
        let root = system_root();
        let blocked: Vec<&str> = entries
            .iter()
            .filter(|e| {
                let target = resolve(e, &shortcuts).target;
                e.system_component || e.is_patch || is_system(e, &target, &root)
            })
            .map(|e| e.name.as_str())
            .collect();
        println!("被系统/驱动/环境这条挡掉的 {} 个：", blocked.len());
        for name in &blocked {
            println!("  - {name}");
        }

        let c = conn();
        let report = plan(&c, &entries, &shortcuts, &get_icon_dir().unwrap(), false).unwrap();
        let with_icon: i64 = c
            .query_row("SELECT COUNT(*) FROM apps WHERE icon_path IS NOT NULL", [], |r| r.get(0))
            .unwrap();
        let with_meta: i64 = c
            .query_row("SELECT COUNT(*) FROM apps WHERE description IS NOT NULL", [], |r| r.get(0))
            .unwrap();
        println!(
            "收进来 {} 个、库里原有 {} 个、被挡 {} 个、找不到目标的 {} 个；有图标 {with_icon} 张、有版本/发行商 {with_meta} 条",
            report.inserted, report.already, report.filtered, report.unresolved
        );
        assert!(report.inserted > 10, "只收进 {} 个", report.inserted);
        assert!(with_icon * 2 > report.inserted as i64, "只抽到 {with_icon} 张图标，抽图标这步多半整片失败了");
        assert!(
            report.unresolved * 2 <= report.scanned,
            "{} / {} 条连一条路径都认不出来，比例太高",
            report.unresolved,
            report.scanned
        );

        // 收进来的名字整片打出来：过滤这条最怕的是切错东西，光看个数看不出来
        let examples: Vec<(String, String, Option<String>)> = c
            .prepare("SELECT name, target, description FROM apps ORDER BY name LIMIT 80")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for (name, target, description) in examples {
            println!("  {name} | {description:?} | {target}");
        }
    }

    /// 真机核对二：过滤名单在这台机器上到底挡不挡东西。
    /// 关开关 → 系统组件回来了；拿其中一个名字进名单、照拖动那样删掉卡片 → 再同步不该回来。
    #[test]
    #[ignore]
    fn the_filter_list_actually_blocks_on_this_machine() {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let entries = read_entries();
        let shortcuts = shortcut_index(
            &crate::win::desktop::all_program_menus()
                .into_iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>(),
        );
        let icons = get_icon_dir().unwrap();
        let c = conn();

        let on_first = plan(&c, &entries, &shortcuts, &icons, false).unwrap();
        assert!(on_first.filtered > 0, "默认开着系统应用过滤却一个都没挡：{on_first:?}");
        crate::services::settings::set_setting(&c, SYSTEM_SWITCH, "0").unwrap();
        let system_off = plan(&c, &entries, &shortcuts, &icons, false).unwrap();
        assert_eq!(system_off.scanned, on_first.scanned, "开关不该改变读到的总数");
        println!(
            "默认开：收 {} / 挡 {}；关掉开关后又收回 {} 个系统应用",
            on_first.inserted, on_first.filtered, system_off.inserted
        );
        assert!(system_off.inserted > 0, "关掉开关一个都没多出来，说明那条过滤没生效");

        let (vid, victim): (i64, String) = c
            .query_row("SELECT id, name FROM apps LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        add_app_filter(&c, &victim).unwrap();
        crate::services::desktop::delete_app(&c, vid).unwrap();
        let after = plan(&c, &entries, &shortcuts, &icons, false).unwrap();
        let still: i64 = c
            .query_row("SELECT COUNT(*) FROM apps WHERE name = ?1", params![victim], |r| r.get(0))
            .unwrap();
        println!("把「{victim}」加进名单再同步：库里现在 {} 个，本次收 {}、挡 {}", after.total, after.inserted, after.filtered);
        assert_eq!(still, 0, "名单里那个又被收回来了，过滤没生效");
        assert!(after.filtered >= 1, "{after:?}");
    }
}
