//! 写库与账单：把过完闸的条目插进 `apps`，顺手抽图标，并把「扫了多少、挡了多少、
//! 已有多少、找不到多少」攒成一份 [SyncReport]。
//!
//! `dry = true` 是同一次扫描但不写库（设置页的试算），两条路共用 [plan]，差别只在插不插。

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{params, Connection};

use crate::error::AppResult;
use crate::services::desktop::extract_icon;

use super::filter::{hits, read_filters, same_program, system_root};
use super::target::resolve;
use super::{name_key, path_key, Entry, Found, Shortcut, SyncReport};

/// 试算里最多报几个被挡的名字，够看出规律就行
const SAMPLES: usize = 12;
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

    // 已经被注册表那一遍认领走的 .lnk：第二遍不能再把它当新程序
    let mut claimed: HashSet<String> = HashSet::new();

    // 「见过」只按名字算（判据见 same_program）：一个启动器养一片程序的情况太多了——Steam 每个游戏的
    // DisplayIcon 都是 steam.exe，按目标文件去重会把除第一个之外的全都吞掉（Steam 自己就这么没的）
    let mut known: HashSet<String> = HashSet::new();
    let mut stmt = conn.prepare("SELECT name FROM apps")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    for name in rows.flatten() {
        known.insert(name_key(&name));
    }

    let mut report = SyncReport {
        scanned: 0,
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
        let seen = same_program(&known, &key);
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
        if resolved.found == Found::Shortcut {
            claimed.insert(path_key(&resolved.icons[0]));
        }
        report.inserted += 1;
    }

    // 第二遍：注册表没认领的快捷方式也收成卡片。商店应用（WSL、各种 Store 包）在开始菜单里都有系统
    // 自己生成的 .lnk，可它们不写卸载项；没装过的绿色版也一样只有快捷方式。这两类是「设置里能看到、
    // 我们这页没有」的主要缺口。系统那条过滤和手动名单在这一遍一样生效，
    // 所以 system32 那些自带工具不会跟着进来。
    for sc in shortcuts {
        let lnk_key = path_key(&sc.lnk);
        if claimed.contains(&lnk_key) {
            continue;
        }
        report.scanned += 1;
        if same_program(&known, &sc.key) {
            report.already += 1;
            continue;
        }
        // 借用注册表那套判定：路径拿 .lnk 自己、目标拿解析出的真目标
        let probe = Entry {
            name: sc.name.clone(),
            display_icon: sc.lnk.clone(),
            reg_key: sc.lnk.clone(),
            ..Default::default()
        };
        if hits(&probe, &sc.target, &filters, &system_root) {
            report.filtered += 1;
            if report.samples.len() < SAMPLES {
                report.samples.push(sc.name.clone());
            }
            continue;
        }
        if !dry {
            let icon = extract_icon(icon_dir, &[sc.lnk.as_str(), sc.target.as_str()]);
            conn.execute(
                "INSERT INTO apps (name, target, source, icon_path) VALUES (?1, ?2, 'startmenu', ?3)",
                params![sc.name, sc.target, icon],
            )?;
        }
        known.insert(sc.key.clone());
        known.insert(lnk_key);
        report.inserted += 1;
    }
    report.scanned += entries.len();

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
pub(crate) fn count_rows(conn: &Connection) -> AppResult<usize> {
    Ok(conn.query_row("SELECT COUNT(*) FROM apps", [], |r| r.get::<_, i64>(0))? as usize)
}
