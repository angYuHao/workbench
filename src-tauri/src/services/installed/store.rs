//! 写库与账单：把过完闸的条目插进 `apps`、给已有卡片追回更新过的路径与图标、清掉来源
//! 已经不在本机的卡片，并把「扫了多少、挡了多少、已有多少、刷新多少、清了多少、找不到多少」
//! 攒成一份 [SyncReport]。
//!
//! `dry = true` 是同一次扫描但不写库（设置页的试算），两条路共用 [plan]，差别只在插不插。

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::{params, Connection};

use crate::error::AppResult;
use crate::services::desktop::extract_icon;

use super::filter::{find_program, hits, read_filters, system_root};
use super::target::resolve;
use super::{core_key, name_key, path_key, Entry, Found, Shortcut, SyncReport};

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

    // 「见过」只按名字算（判据见 find_program）：一个启动器养一片程序的情况太多了——Steam 每个游戏的
    // DisplayIcon 都是 steam.exe，按目标文件去重会把除第一个之外的全都吞掉（Steam 自己就这么没的）。
    // 值里存上那一行的 id 和路径：认成「已有」之后还要顺手把更新过的启动路径与图标追回来。
    let mut known: HashMap<String, Known> = HashMap::new();
    // 第二把尺：按「去版本核」索引同一批卡片。软件一更新名字就换个版本号（OpenCode 2.2.0 →
    // 2.2.5），name_key 那把尺认不出是同一个，会再插一张；这把削掉版本号的尺子负责认回来。
    let mut known_core: HashMap<String, Known> = HashMap::new();
    // 本机还在的路径（含被过滤名单挡掉的）：给来源体检当第二把尺子用
    let mut live_targets: HashSet<String> = HashSet::new();
    let mut stmt = conn.prepare("SELECT id, name, target, icon_path FROM apps")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(1)?,
            Known {
                id: r.get(0)?,
                target: r.get(2)?,
                icon: r.get::<_, Option<String>>(3)?,
            },
        ))
    })?;
    for row in rows.flatten() {
        known.insert(name_key(&row.0), row.1.clone());
        known_core.insert(core_key(&row.0), row.1);
    }

    let mut report = SyncReport {
        scanned: 0,
        inserted: 0,
        filtered: 0,
        already: 0,
        updated: 0,
        unresolved: 0,
        total: 0,
        removed: 0,
        removed_names: Vec::new(),
        samples: Vec::new(),
    };

    // 顺序是有意为之：先挡（过滤名单/系统应用），再认（库里已有），最后才说「找不到目标」。
    // 反过来会把一百多条本来就不收的系统组件报成「认不出怎么打开」，那个数字就没意义了。
    for entry in entries {
        if entry.name.trim().is_empty() {
            continue;
        }
        let resolved = resolve(entry, shortcuts);
        // 来源体检要用：改过名字的卡片名字对不上注册表，但路径还是同一条 —— 先全量记下来
        live_targets.insert(path_key(&resolved.target));
        if hits(entry, &resolved.target, &filters, &system_root) {
            report.filtered += 1;
            if report.samples.len() < SAMPLES {
                report.samples.push(entry.name.clone());
            }
            continue;
        }
        let key = name_key(&entry.name);
        let hit = find_program(&known, &key)
            .cloned()
            .or_else(|| known_core.get(&core_key(&entry.name)).cloned());
        if let Some(hit) = hit {
            report.already += 1;
            // 「已有」不等于「什么都不用做」：程序更新后启动路径和图标都会变，那两样是机器上的
            // 事实，不是用户在界面上改过的东西，该顺着这一次扫描追回来（图标不更新就是这么来的）
            let mut icons: Vec<&str> = resolved.icons.iter().map(|s| s.as_str()).collect();
            icons.push(resolved.target.as_str());
            if refresh(conn, &hit, &resolved.target, &icons, icon_dir, dry)? {
                report.updated += 1;
            }
            continue;
        }
        if resolved.found == Found::Nothing {
            report.unresolved += 1;
            continue;
        }
        let mut candidates: Vec<&str> = resolved.icons.iter().map(|s| s.as_str()).collect();
        candidates.push(resolved.target.as_str());
        // 试算不碰文件也不碰库，那这一行就没有 id：占位记录只用来挡住同一遍里的重复
        let mut row = Known { id: 0, target: resolved.target.clone(), icon: None };
        if !dry {
            row.icon = extract_icon(icon_dir, &candidates);
            conn.execute(
                "INSERT INTO apps (name, description, target, source, icon_path)
                 VALUES (?1, ?2, ?3, 'registry', ?4)",
                params![entry.name, describe(entry), resolved.target, row.icon],
            )?;
            row.id = conn.last_insert_rowid();
        }
        known.insert(key, row.clone());
        known_core.insert(core_key(&entry.name), row);
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
        let hit = find_program(&known, &sc.key)
            .cloned()
            .or_else(|| known_core.get(&core_key(&sc.name)).cloned());
        if let Some(hit) = hit {
            report.already += 1;
            // 和注册表那一遍一样：认成已有之后，路径和图标还是要追一下更新
            if refresh(conn, &hit, &sc.target, &[sc.lnk.as_str(), sc.target.as_str()], icon_dir, dry)? {
                report.updated += 1;
            }
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
        let mut row = Known { id: 0, target: sc.target.clone(), icon: None };
        if !dry {
            row.icon = extract_icon(icon_dir, &[sc.lnk.as_str(), sc.target.as_str()]);
            conn.execute(
                "INSERT INTO apps (name, target, source, icon_path) VALUES (?1, ?2, 'startmenu', ?3)",
                params![sc.name, sc.target, row.icon],
            )?;
            row.id = conn.last_insert_rowid();
        }
        known.insert(sc.key.clone(), row.clone());
        known_core.insert(core_key(&sc.name), row);
        // .lnk 那条路径也占个名，免得同一遍里换个写法又收一次。它不是一行卡片，id 留 0
        known.insert(lnk_key, Known { id: 0, target: String::new(), icon: None });
        report.inserted += 1;
    }
    report.scanned += entries.len();

    // 来源体检：这一页是「本机装了什么」的镜子，镜子不该留着已经不在了的东西。
    // 程序更新会顶掉旧版本的卸载项、卸载会删掉卸载项，两种都会留下一张指向同一个程序的老卡片。
    let (removed, removed_names) = sweep(conn, entries, &live_targets, dry)?;
    report.removed = removed;
    report.removed_names = removed_names;

    // 存量收敛：以前每更新一次就插一张，库里已经攒下同一软件的多版本卡片。
    // 判重那把尺只挡「以后不再插」，这些老卡片得在这儿合并掉。
    let (dup_removed, dup_names) = collapse(conn, dry)?;
    report.removed += dup_removed;
    for n in dup_names {
        if report.removed_names.len() < SAMPLES {
            report.removed_names.push(n);
        }
    }

    report.total = if dry {
        (count_rows(conn)? + report.inserted).saturating_sub(report.removed)
    } else {
        count_rows(conn)?
    };
    Ok(report)
}

/// 把来源已经消失的卡片清掉。三种来源分开判，判据都只有一条：
/// - `registry`：注册表里那条卸载项还在不在（按名字，用的还是 [find_program] 那一把尺子）
/// - `startmenu`：卡片指向的那个路径还在不在（快捷方式或它解出来的 exe）
/// - `manual`：用户自己加的，**永远不动** —— 它没有「来源」可以对
///
/// 另一道闸：`entries` 空的时候直接返回。读不到注册表（权限、异常机器）不等于本机什么都没装，
/// 那种情况下最不该做的就是清空整页。
fn sweep(conn: &Connection, entries: &[Entry], live_targets: &HashSet<String>, dry: bool) -> AppResult<(usize, Vec<String>)> {
    if entries.is_empty() {
        return Ok((0, Vec::new()));
    }
    let mut live: HashMap<String, ()> = HashMap::new();
    for e in entries {
        // 注意是**过过滤之前**的全量：被名单挡掉的程序照样装在本机，它的卡片不该被当成孤儿
        live.insert(name_key(&e.name), ());
    }

    let mut gone: Vec<(i64, String)> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, name, target, source FROM apps WHERE source IN ('registry', 'startmenu')",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        for row in rows.flatten() {
            let (id, name, target, source) = row;
            let alive = if source == "startmenu" {
                Path::new(&target).exists()
            } else {
                // 名字对不上但路径还是本机在用的那一条：多半是用户改过名字，不算孤儿
                find_program(&live, &name_key(&name)).is_some() || live_targets.contains(&path_key(&target))
            };
            if !alive {
                gone.push((id, name));
            }
        }
    }

    if !dry {
        for (id, _) in &gone {
            conn.execute("DELETE FROM apps WHERE id = ?1", params![id])?;
        }
    }
    let count = gone.len();
    let names = gone.into_iter().map(|(_, n)| n).take(SAMPLES).collect();
    Ok((count, names))
}

/// 收敛存量重复：同一软件的多版本卡片（去版本核相同）只留一张。
/// 留哪张：目标还在本机的那张优先，其次 id 小的（先收进来的）。`manual` 是用户自己加的，不动。
/// 判重那把尺（known_core）只保证「以后不再插」，历史上攒下的重复靠这一遍合并。
fn collapse(conn: &Connection, dry: bool) -> AppResult<(usize, Vec<String>)> {
    let mut groups: HashMap<String, Vec<(i64, String, String)>> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, name, target FROM apps WHERE source IN ('registry', 'startmenu') ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })?;
        for (id, name, target) in rows.flatten() {
            groups.entry(core_key(&name)).or_default().push((id, name, target));
        }
    }
    let mut gone: Vec<(i64, String)> = Vec::new();
    for members in groups.into_values() {
        if members.len() < 2 {
            continue;
        }
        let mut members = members;
        // 目标还在本机的排前面；都在或都不在就按 id 小的留
        members.sort_by_key(|(id, _, target)| (!Path::new(target).exists(), *id));
        for (id, name, _) in members.into_iter().skip(1) {
            gone.push((id, name));
        }
    }
    if !dry {
        for (id, _) in &gone {
            conn.execute("DELETE FROM apps WHERE id = ?1", params![id])?;
        }
    }
    let count = gone.len();
    let names = gone.into_iter().map(|(_, n)| n).take(SAMPLES).collect();
    Ok((count, names))
}

/// 库里已有的一行，只带「要不要顺手刷新」用得上的三列。
/// id 为 0 是占位（试算时新增的、或只是认领走的一个 .lnk 路径）：它不是一行真卡片，刷新时跳过。
#[derive(Clone)]
struct Known {
    id: i64,
    target: String,
    icon: Option<String>,
}

/// 已有卡片的启动路径和图标追一下更新。名字、别名、分类一概不碰——那是用户在界面上改过的，
/// 一次同步不该抹掉；但「从哪儿启动、长什么样」是机器上的事实，程序更新后它就是变了。
/// 回 true 表示真改了库（试算时只表示「会改」）。
fn refresh(
    conn: &Connection,
    hit: &Known,
    target: &str,
    icons: &[&str],
    icon_dir: &Path,
    dry: bool,
) -> AppResult<bool> {
    if hit.id == 0 {
        return Ok(false);
    }
    let target_changed = path_key(&hit.target) != path_key(target);
    if dry {
        // 试算不抽图（那会往缓存目录写文件）也不写库，只看路径这条事实变没变
        return Ok(target_changed);
    }
    // 抽不到就留着库里那份：旧图标也比把它清空强
    let icon = extract_icon(icon_dir, icons).or_else(|| hit.icon.clone());
    if !target_changed && icon == hit.icon {
        return Ok(false);
    }
    conn.execute(
        "UPDATE apps SET target = ?2, icon_path = ?3 WHERE id = ?1",
        params![hit.id, target, icon],
    )?;
    Ok(true)
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
