//! 认目标，以及路径写法的归一。
//!
//! 注册表给的 `DisplayIcon`、`InstallLocation`、卸载串都有几种坑写法：`,0` 的图标序号、
//! 带引号带参数、`%ProgramFiles%` 变量、装完自己搬过目录留下的老路径、被自更新换成 0 字节的空壳。
//! 这些都在 [clean_path] / [relocated] / [runnable] 处理，判定只看归一以后的路径。

use std::path::{Path, PathBuf};

use crate::services::desktop::{extension_of, launch_target};

use super::{name_key, path_key, short_key, Entry, Found, Resolved, Shortcut};

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
    if let Some(exe) = exe_near(&dirs, &entry.name) {
        return Resolved { target: exe, icons, found: Found::Exe };
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
pub(crate) fn parent_dir(path: &str) -> Option<String> {
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
pub(crate) fn relocated(path: &str) -> Option<String> {
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
pub(crate) fn find_shortcut<'a>(name: &str, shortcuts: &'a [Shortcut]) -> Option<&'a Shortcut> {
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

/// 从注册表给的那几个目录往外找主程序：目录本身、它的上一层、这两层各自的直接子目录
/// （不往下递归）。WPS 的注册表只给到 `<版本>\utility\uninst.exe`，主程序却在隔壁的
/// `<版本>\office6\wps.exe`，不扩这一圈就找不到。
/// 判「哪个 exe 是主程序」由严到松：全等 → 去掉版本号后全等 / 产品名以 exe 名开头 /
/// exe 名是产品名里的一段；这几档里**取名字最短的那个**——WPS 的 `office6` 里同时有
/// `wps.exe`、`wpsoffice.exe`、`et.exe`、`wpp.exe`，最短的那个才是这个产品本体的入口，
/// 带后缀的是首页启动器和各组件。
pub(crate) fn exe_near(dirs: &[String], name: &str) -> Option<String> {
    let key = name_key(name);
    let short = short_key(name);
    let mut best: Option<(usize, String)> = None;
    let mut scanned = 0usize;
    for start in dirs {
        for dir in neighborhood(Path::new(start)) {
            if scanned >= MAX_SCAN_DIRS {
                return best.map(|(_, path)| path);
            }
            scanned += 1;
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let as_str = path.to_string_lossy().to_string();
                if !usable_exe(&as_str) {
                    continue;
                }
                let Some(stem_raw) = path.file_stem() else {
                    continue;
                };
                let stem = name_key(stem_raw.to_string_lossy().as_ref());
                if stem.is_empty() {
                    continue;
                }
                if stem == key {
                    return Some(as_str);
                }
                let named_after_it = (stem.len() >= 3 && key.starts_with(&stem))
                    || (stem.len() >= 5 && key.contains(&stem))
                    || (short.len() >= 4 && stem == short);
                if named_after_it && best.as_ref().is_none_or(|(len, _)| stem.len() < *len) {
                    best = Some((stem.len(), as_str));
                }
            }
        }
    }
    best.map(|(_, path)| path)
}

/// 一个目录的搜索圈：它自己、上一层、以及这两层的直接子目录（每层最多看 30 个）
pub(crate) fn neighborhood(dir: &Path) -> Vec<PathBuf> {
    let mut roots = vec![dir.to_path_buf()];
    if let Some(parent) = dir.parent() {
        roots.push(parent.to_path_buf());
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for root in roots {
        out.push(root.clone());
        if let Ok(entries) = std::fs::read_dir(&root) {
            out.extend(
                entries
                    .flatten()
                    .filter(|e| e.path().is_dir())
                    .take(30)
                    .map(|e| e.path()),
            );
        }
    }
    out
}
/// 一个条目最多翻多少个目录：宁可找不到目标，也不要在一次同步里把整块盘扫一遍
const MAX_SCAN_DIRS: usize = 60;

/// MSI 安装器缓存住的那个目录：里面的 exe 是重装/修复用的，不是程序本体
const PACKAGE_CACHE_MARK: &str = "/package cache/";
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
pub(crate) fn command_exe(cmd: &str) -> Option<String> {
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
pub(crate) fn expand_vars(s: &str) -> String {
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

pub(crate) fn is_exe_file(path: &str) -> bool {
    !path.is_empty() && extension_of(Path::new(path)) == "exe" && runnable(Path::new(path))
}

/// 存在、是文件、而且**不是 0 字节**：WPS 自更新会在安装根目录留一个空的 `ksolaunch.exe`
/// （真身被改名成 `ksolaunch.bak.exe`），注册表的 `DisplayIcon` 和开始菜单快捷方式都还指着
/// 那个空壳——文件在、点不动。空文件一律不算能跑的东西。
pub(crate) fn runnable(p: &Path) -> bool {
    p.metadata().map(|m| m.is_file() && m.len() > 0).unwrap_or(false)
}

/// 能拿去当启动目标的 exe：文件在、是 exe、不是卸载器/安装器，也不是 MSI 缓存在
/// `ProgramData\Package Cache` 里的那个外壳—— burn engine 存的是重装/修复程序，
/// 点它会弹出安装向导，Docker 和一堆华硕组件的 DisplayIcon 都写的这种路径。
pub(crate) fn usable_exe(path: &str) -> bool {
    is_exe_file(path) && !is_uninstaller(path) && !path_key(path).contains(PACKAGE_CACHE_MARK)
}

/// 卸载/安装器不是「打开应用」：`unins000.exe`、`Docker Desktop Installer.exe`、`某某卸载.exe` 都算
pub(crate) fn is_uninstaller(path: &str) -> bool {
    let name = Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    name.starts_with("unins") || name.contains("uninstall") || name.contains("installer") || name.contains("卸载")
}

/// 快捷方式解析出来的目标得是「能跑的东西」：exe、脚本、安装包，或者还是那个 .lnk
///（商店应用的快捷方式解不出真 exe，这时 .lnk 自己就是入口，ShellExecute 会替我们解析）。
/// 帮助文档 .chm、网页 .htm 那类附属文件不算应用：它们是「某程序附带的说明书」，
/// 排成一排只会把这一页变成文档目录。
pub(crate) fn program_target(target: &str) -> bool {
    if target.contains("://") {
        return true;
    }
    let p = Path::new(target);
    runnable(p)
        && matches!(
            extension_of(p).as_str(),
            "exe" | "lnk" | "bat" | "cmd" | "com" | "scr" | "msi" | "appref-ms"
        )
}

pub(crate) fn launchable(target: &str) -> bool {
    if target.contains("://") {
        return true;
    }
    let path = Path::new(target);
    if !runnable(path) {
        return false;
    }
    // 目标是图标文件的那种快捷方式是安装程序顺手放的（Charles、Cpolar 都这样），
    // 点它只会开一个图片查看器，不算一个应用
    !matches!(extension_of(path).as_str(), "ico" | "cur")
}
