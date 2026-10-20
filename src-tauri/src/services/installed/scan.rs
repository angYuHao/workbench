//! 读源头：注册表的卸载项和开始菜单的快捷方式。这一层只负责读出来，
//! 判收不收、找目标都在下游。

use std::path::{Path, PathBuf};

use crate::services::desktop::launch_target;
use crate::win::registry::{Arch, Hive, Key};

use super::target::{is_uninstaller, program_target};
use super::{name_key, Entry, Shortcut};

/// 开始菜单本来就有多层厂商目录，给三层的量
const MAX_DEPTH: usize = 3;
/// Windows 记「装了什么东西」的那把键
const UNINSTALL_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
/// 32 位程序装的在 WOW6432Node 那份视图里，只读一个视图会漏一半
const VIEWS: [(Hive, Arch); 4] = [
    (Hive::Local, Arch::Native),
    (Hive::Local, Arch::Wow32),
    (Hive::Current, Arch::Native),
    (Hive::Current, Arch::Wow32),
];
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
        // 解不出「能跑的东西」的不进索引：目标文件被删了、指向目录、或只是某个程序的帮助文档
        if !program_target(&target) {
            continue;
        }
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(file_name);
        out.push(Shortcut {
            name: stem.clone(),
            key: name_key(&stem),
            lnk: path.to_string_lossy().to_string(),
            target,
        });
    }
}
