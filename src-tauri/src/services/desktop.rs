use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{AppError, AppResult};
use crate::models::{AppEntry, DesktopItem, DesktopRule, OrganizeLog, OrganizePlan};
use crate::services::categories;

/// 快捷方式、可执行文件、网址链接之外的东西也要整理，所以扫描不按扩展名筛
const ORGANIZE_DIR: &str = "桌面整理";
/** 往下翻几层：桌面根目录算 0 层，再深就可能是用户自己的大件结构了 */
const SCAN_MAX_DEPTH: usize = 2;
const ORGANIZE_LOG_COLUMNS: &str = "id, file_name, from_path, to_path, organized_at, reverted";

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

    // 一次查出启动器已有的 target，标好「这条已经整理过了」给前端的「只处理新增」用
    let known: std::collections::HashSet<String> = conn
        .prepare("SELECT target FROM apps")?
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .map(|t| t.to_lowercase())
        .collect();
    mark_registered(&mut items, &known);
    Ok(items)
}

/// 启动器里是不是已经有这一项：`.lnk` 自身和它解析出来的目标两种写法都算，
/// 因为登记时到底存的哪一个取决于当时是「只登记」还是「移动并添加」。
/// Windows 路径不分大小写，所以两边都折成小写再比。
fn mark_registered(items: &mut [DesktopItem], known: &std::collections::HashSet<String>) {
    for item in items {
        item.registered = known.contains(&item.path.to_lowercase())
            || known.contains(&item.target.to_lowercase());
    }
}

/// 扫描要绕开工作台自己：桌面上那个快捷方式指的就是这个程序，
/// 让它进启动器等于「用一个工作台去整理放着这个工作台的桌面」。
/// 开发时跑的是 `target\debug` 里那份、快捷方式指的是装机那份，两个路径不一样，
/// 所以除了完整路径相等，还按 exe 的文件名认——桌面这种场景里同名文件基本只有它自己。
struct OwnExe {
    file: String,
    full: String,
}

/// Windows 路径比较用：去正反斜杠、去 `\\?\` 前缀、折小写
fn norm_path(p: &Path) -> String {
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

    fn is_this(&self, path: &Path, target: &str) -> bool {
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
fn scan_directory(
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
        let category = match_category(&display_name, cat_ext, rules);
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
            registered: false,
        });
    }
}

pub fn organize_desktop(conn: &Connection, plans: &[OrganizePlan]) -> AppResult<Vec<OrganizeLog>> {
    let desktop = crate::win::desktop::user_desktop()
        .ok_or_else(|| AppError::msg("无法获取桌面目录"))?;
    organize_into(conn, plans, Path::new(&desktop))
}

/// 真正干活的一层：`desktop` 当桌面根目录用，测试传临时目录进来就能真的跑一遍搬移
pub(crate) fn organize_into(
    conn: &Connection,
    plans: &[OrganizePlan],
    desktop: &Path,
) -> AppResult<Vec<OrganizeLog>> {
    let base = desktop.join(ORGANIZE_DIR);

    let mut logs = Vec::new();
    for plan in plans {
        let src = Path::new(&plan.path);
        let file_name = src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if file_name.is_empty() {
            continue;
        }

        if plan.skip_move.unwrap_or(false) {
            upsert_app_from_desktop(
                conn,
                &file_name,
                &launch_target(src),
                std::slice::from_ref(&plan.path),
                &plan.category,
                plan.icon_path.as_deref(),
            )?;
            continue;
        }

        // 移动之前先把这个程序的「另一种写法」记下来：桌面上的原路径，以及快捷方式指向的真实目标。
        // 只登记模式存的是后者，搬移模式存的是归档后的 .lnk，不一起认就会插成两条。
        let resolved = launch_target(src);

        std::fs::create_dir_all(&base)?;
        let cat_dir = base.join(&plan.category);
        std::fs::create_dir_all(&cat_dir)?;

        let dest = free_dest(&cat_dir, src, &file_name);

        std::fs::rename(src, &dest)?;
        let to_path = dest.to_string_lossy().to_string();

        conn.execute(
            "INSERT INTO organize_logs (file_name, from_path, to_path) VALUES (?1, ?2, ?3)",
            params![file_name, plan.path, to_path],
        )?;
        let log_id = conn.last_insert_rowid();
        logs.push(OrganizeLog {
            id: log_id,
            file_name: Some(file_name.clone()),
            from_path: Some(plan.path.clone()),
            to_path: Some(to_path.clone()),
            organized_at: None,
            reverted: false,
        });

        upsert_app_from_desktop(
            conn,
            &file_name,
            &to_path,
            &[plan.path.clone(), resolved],
            &plan.category,
            plan.icon_path.as_deref(),
        )?;
    }
    Ok(logs)
}

/// 归档目录里已经有同名文件时按 (1)、(2) 递增改名，绝不覆盖用户的东西
fn free_dest(dir: &Path, src: &Path, file_name: &str) -> std::path::PathBuf {
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = src
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    let mut dest = dir.join(file_name);
    let mut n = 1;
    while dest.exists() {
        dest = dir.join(format!("{} ({}){}", stem, n, ext));
        n += 1;
    }
    dest
}

/// 改一条分类规则，并把改过的分类名一路带走。
/// 只写 `desktop_rules` 的话：分类表还是旧名（卡片和分类树不动），下次整理又会在旁边另开一个
/// 新名目录，旧目录成了没人管的孤儿，而 `organize_logs.to_path` 和卡片的 `target` 还指着旧路径
/// ——「恢复整理」和双击启动会一起坏掉。所以规则改名 = 规则 + 分类 + 目录 + 记着旧路径的行。
/// 新名撞车要**在写规则之前**就问掉，否则留下「规则改了、分类和目录没跟上」的半状态。
pub fn save_rule_with_rename(
    conn: &Connection,
    rule: &DesktopRule,
    desktop: &Path,
) -> AppResult<DesktopRule> {
    let old = categories::get_rules(conn)?
        .into_iter()
        .find(|r| r.id == rule.id)
        .map(|r| r.category);
    if let Some(old) = old.as_ref().filter(|o| o.trim() != rule.category.trim()) {
        check_rule_rename(conn, desktop, old, &rule.category, rule.id)?;
    }
    let saved = categories::save_rule(conn, rule)?;
    if let Some(old) = old.filter(|o| *o != saved.category) {
        propagate_rule_rename(conn, desktop, &old, &saved.category, rule.id)?;
    }
    Ok(saved)
}

/// 改名前先审一遍：名字没变、旧名还被别的规则用着、新名撞上已有分类、新名目录已存在，
/// 后三种都该问用户，不静默合并——怎么并只有他知道。`except_id` 是本次要改的那条规则自己。
fn check_rule_rename(
    conn: &Connection,
    desktop: &Path,
    old: &str,
    new: &str,
    except_id: i64,
) -> AppResult<Option<(std::path::PathBuf, std::path::PathBuf)>> {
    let (old, new) = (old.trim(), new.trim());
    if old.is_empty() || new.is_empty() || old == new {
        return Ok(None);
    }
    let still_used: i64 = conn.query_row(
        "SELECT COUNT(*) FROM desktop_rules WHERE category = ?1 AND id <> ?2",
        params![old, except_id],
        |r| r.get(0),
    )?;
    if still_used > 0 {
        // 还有别的规则叫旧名：分类名和目录都不该跟着这一条走
        return Ok(None);
    }
    let clash: Option<i64> = conn
        .query_row(
            "SELECT id FROM categories WHERE type = 'app' AND name = ?1 COLLATE NOCASE",
            params![new],
            |r| r.get(0),
        )
        .optional()?;
    if clash.is_some() {
        return Err(AppError::msg(format!("已经有分类「{new}」，先把它并掉再改规则名")));
    }
    let base = desktop.join(ORGANIZE_DIR);
    let (cat_dir, dest_dir) = (base.join(old), base.join(new));
    if cat_dir.is_dir() && dest_dir.exists() {
        return Err(AppError::msg(format!(
            "桌面整理\\{new} 已经存在，先把它换个名或清空再改"
        )));
    }
    Ok(Some((cat_dir, dest_dir)))
}

/// 真正动手：改分类表的名字，重命名 `桌面整理\<旧名>`，并把记着旧路径的行改到新路径上。
/// 单独调用也安全——它自己会先审一遍。
pub fn propagate_rule_rename(
    conn: &Connection,
    desktop: &Path,
    old: &str,
    new: &str,
    except_id: i64,
) -> AppResult<()> {
    let (cat_dir, dest_dir) = match check_rule_rename(conn, desktop, old, new, except_id)? {
        Some(dirs) => dirs,
        None => return Ok(()),
    };
    let moving_folder = cat_dir.is_dir();
    if moving_folder {
        std::fs::rename(&cat_dir, &dest_dir)?;
    }
    // 兜底：库没改成，目录也不该留在新名下
    let rollback_folder = || {
        if moving_folder {
            let _ = std::fs::rename(&dest_dir, &cat_dir);
        }
    };
    let from = cat_dir.to_string_lossy().to_string();
    let to = dest_dir.to_string_lossy().to_string();

    conn.execute_batch("BEGIN")?;
    let wrote = (|| -> AppResult<()> {
        // 分类名精确匹配：规则里存的就是 categories.name 的原文，大小写不同的两条分类本来就该分开
        conn.execute(
            "UPDATE categories SET name = ?1 WHERE type = 'app' AND name = ?2",
            params![new.trim(), old.trim()],
        )?;
        for (table, column) in [
            ("organize_logs", "to_path"),
            ("apps", "target"),
            ("files", "path"),
            ("files", "managed_root"),
        ] {
            reprefix_paths(conn, table, column, &from, &to)?;
        }
        Ok(())
    })();
    if let Err(e) = wrote {
        let _ = conn.execute_batch("ROLLBACK");
        rollback_folder();
        return Err(e);
    }
    if let Err(e) = crate::db::commit(conn) {
        rollback_folder();
        return Err(e);
    }
    Ok(())
}

/// 把某一列里以 `from` 开头的路径改写成以 `to` 开头。表名和列名都是调用处写死的字面量。
/// 分类名可能带中文和 `_`（LIKE 的通配符），所以逐行取出来在 Rust 里比前缀；
/// 截断按字符数而不是字节数，中文路径切一半会 panic。
fn reprefix_paths(
    conn: &Connection,
    table: &'static str,
    column: &'static str,
    from: &str,
    to: &str,
) -> AppResult<usize> {
    let lower_from = from.to_lowercase();
    let keep = from.chars().count();
    let mut stmt = conn.prepare(&format!(
        "SELECT id, {column} FROM {table} WHERE {column} IS NOT NULL"
    ))?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .filter_map(Result::ok)
        .collect();
    drop(stmt);

    let mut n = 0;
    for (id, path) in rows {
        if !path.to_lowercase().starts_with(&lower_from) {
            continue;
        }
        let tail: String = path.chars().skip(keep).collect();
        let next = format!("{to}{tail}");
        conn.execute(
            &format!("UPDATE {table} SET {column} = ?1 WHERE id = ?2"),
            params![next, id],
        )?;
        n += 1;
    }
    Ok(n)
}

pub fn revert_organize(conn: &Connection, log_id: Option<i64>) -> AppResult<usize> {
    let logs: Vec<OrganizeLog> = match log_id {
        Some(id) => conn
            .query_row(
                &format!(
                    "SELECT {ORGANIZE_LOG_COLUMNS} FROM organize_logs WHERE id = ?1 AND reverted = 0"
                ),
                params![id],
                map_organize_log,
            )
            .optional()?
            .into_iter()
            .collect(),
        None => {
            let mut stmt = conn.prepare(&format!(
                "SELECT {ORGANIZE_LOG_COLUMNS} FROM organize_logs WHERE reverted = 0 ORDER BY id DESC"
            ))?;
            let rows = stmt.query_map([], map_organize_log)?;
            rows.collect::<Result<Vec<_>, _>>()?
        }
    };

    let mut count = 0;
    let mut vacated: Vec<std::path::PathBuf> = Vec::new();
    for log in logs {
        let (Some(from), Some(to)) = (&log.from_path, &log.to_path) else {
            continue;
        };
        if !Path::new(to).exists() {
            // 副本已不在分类目录里，只补记状态；启动器那条还得跟回原位置
            mark_reverted(conn, log.id)?;
            repoint_app(conn, from, to)?;
            continue;
        }
        // 原位置已被占用则删掉副本，否则移回原位置
        let restored = if Path::new(from).exists() {
            std::fs::remove_file(to).is_ok()
        } else {
            std::fs::rename(to, from).is_ok()
        };
        if restored {
            mark_reverted(conn, log.id)?;
            repoint_app(conn, from, to)?;
            count += 1;
            if let Some(cat_dir) = Path::new(to).parent() {
                if cat_dir.parent().and_then(|p| p.file_name()).map(is_organize_base_name) == Some(true) {
                    vacated.push(cat_dir.to_path_buf());
                }
            }
        }
    }
    prune_empty_dirs(&vacated);
    Ok(count)
}

fn is_organize_base_name(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().eq_ignore_ascii_case(ORGANIZE_DIR)
}

/// 撤销整理后，启动器里指向分类目录的那条要跟回原位置，否则点开是死链；
/// 原位置已经有一条（先前只登记过同一个文件）就删重复的那条，不制造两个同名卡片
fn repoint_app(conn: &Connection, from: &str, to: &str) -> AppResult<()> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT id FROM apps WHERE target = ?1",
            params![from],
            |r| r.get(0),
        )
        .optional()?;
    if exists.is_some() {
        conn.execute("DELETE FROM apps WHERE target = ?1", params![to])?;
    } else {
        conn.execute(
            "UPDATE apps SET target = ?1 WHERE target = ?2",
            params![from, to],
        )?;
    }
    Ok(())
}

/// 只删被搬空的分类目录，非空一律留着；顺带把只剩这些空目录的「桌面整理」也收掉
fn prune_empty_dirs(dirs: &[std::path::PathBuf]) {
    let is_empty = |p: &Path| p.is_dir() && p.read_dir().map(|mut r| r.next().is_none()).unwrap_or(false);
    let mut bases: Vec<&std::path::Path> = Vec::new();
    for dir in dirs {
        if is_empty(dir) && std::fs::remove_dir(dir).is_ok() {
            if let Some(base) = dir.parent() {
                bases.push(base);
            }
        }
    }
    for base in bases {
        if is_empty(base) {
            let _ = std::fs::remove_dir(base);
        }
    }
}

fn mark_reverted(conn: &Connection, log_id: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE organize_logs SET reverted = 1 WHERE id = ?1",
        params![log_id],
    )?;
    Ok(())
}

pub fn launch_by_id(conn: &Connection, id: i64) -> AppResult<()> {
    let target: Option<String> = conn
        .query_row("SELECT target FROM apps WHERE id = ?1", params![id], |r| r.get(0))
        .optional()?;
    // 走 id 不走 target：计数得落在数据库这一行上，前端传回来的路径不能当键用
    let Some(target) = target else {
        return Err(AppError::msg(format!("没有 id 为 {id} 的程序")));
    };
    // 先真打开，再记数：计数写不进去（磁盘满、库被占）不该挡住用户要的那一下
    crate::win::shell::open(&target)?;
    let _ = crate::services::usage::note_use(conn, crate::services::usage::APP, id);
    Ok(())
}

pub fn get_apps(conn: &Connection) -> AppResult<Vec<AppEntry>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.name, a.alias, a.description, a.target, a.args, a.icon_path, a.category_id, a.source, a.created_at,
                COALESCE(u.uses, 0)
         FROM apps a
         LEFT JOIN usage_stats u ON u.kind = 'app' AND u.id = a.id
         ORDER BY COALESCE(u.uses, 0) DESC, a.name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(AppEntry {
            id: r.get(0)?,
            name: r.get(1)?,
            alias: r.get(2)?,
            description: r.get(3)?,
            target: r.get(4)?,
            args: r.get(5)?,
            icon_path: r.get(6)?,
            category_id: r.get(7)?,
            source: r.get(8)?,
            created_at: r.get(9)?,
            use_count: r.get(10)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn save_app(conn: &Connection, app: &AppEntry) -> AppResult<AppEntry> {
    if app.id > 0 {
        conn.execute(
            "UPDATE apps SET name = ?1, alias = ?2, description = ?3, target = ?4, args = ?5,
             icon_path = ?6, category_id = ?7 WHERE id = ?8",
            params![
                app.name, app.alias, app.description, app.target, app.args, app.icon_path,
                app.category_id, app.id
            ],
        )?;
        Ok(app.clone())
    } else {
        conn.execute(
            "INSERT INTO apps (name, alias, description, target, args, icon_path, category_id, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'manual')",
            params![
                app.name, app.alias, app.description, app.target, app.args, app.icon_path,
                app.category_id
            ],
        )?;
        let id = conn.last_insert_rowid();
        Ok(AppEntry { id, ..app.clone() })
    }
}

pub fn delete_app(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM apps WHERE id = ?1", params![id])?;
    crate::services::usage::forget(conn, crate::services::usage::APP, id)
}

pub(crate) fn get_icon_dir() -> AppResult<std::path::PathBuf> {
    let data_dir = dirs::data_dir().ok_or_else(|| AppError::msg("无法获取 APPDATA 目录"))?;
    Ok(data_dir.join("workbench").join("icons"))
}

/// 抽图标要按「桌面上看到的那张」为准，所以候选顺序是：**快捷方式自己 → 它解析出的目标**。
/// 顺序反了就会踩 Ollama 那种坑：快捷方式用 IconLocation 指着 app.ico，而 `ollama app.exe`
/// 自己压根没内嵌图标，抽解析目标只能拿到 Windows 的白纸占位图。
/// 走 .lnk 时 shell 已经把 IconLocation（含指向 dll 第 N 个图标的写法）算好了，不用我们再解。
/// 缓存键按「真正用的那个写法」算，换写法就是换键，不会串图。
fn extract_icon(icon_dir: &Path, candidates: &[&str]) -> Option<String> {
    let mut tried: Vec<&str> = Vec::new();
    for target in candidates.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if tried.contains(&target) {
            continue;
        }
        tried.push(target);
        if !Path::new(target).exists() {
            continue;
        }
        let out = icon_dir.join(format!("{:x}.png", fnv1a(target.as_bytes())));
        let png = out.to_string_lossy().to_string();
        if out.exists() {
            return Some(png);
        }
        if crate::win::icon::extract_icon_to_png(target, &out.to_string_lossy()).is_ok() {
            return Some(png);
        }
        // 抽失败可能留下半截文件：不删掉的话，下次一看「文件在」就当成功了
        let _ = std::fs::remove_file(&out);
    }
    None
}

/// 小写扩展名（不含点），无扩展名时为空串
pub(crate) fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// .lnk 解析为真实启动目标；解析失败或非快捷方式时就是文件自身路径
fn launch_target(path: &Path) -> String {
    let own = path.to_string_lossy().to_string();
    if extension_of(path) == "lnk" {
        crate::win::lnk::resolve_lnk(&own).unwrap_or(own)
    } else {
        own
    }
}

/// 规则里逗号分隔的字段，逐项去空白并转小写
fn rule_tokens(field: &Option<String>) -> impl Iterator<Item = String> + '_ {
    field
        .iter()
        .flat_map(|f| f.split(','))
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
}

/// 拿去和规则里的扩展名比的那个扩展名。
/// 普通文件用自己的；快捷方式按它真正启动的东西算，这样指向 `报告.pdf` 的快捷方式能进文档。
/// 但目标本身就是图标文件（.ico/.cur）、又是快捷方式时例外：那是安装程序把快捷方式指到了
/// 自己的 `app.ico` 上（Cpolar Web UI 就是这样），桌面上这个东西仍然是个程序入口，
/// 不能因为目标是 .ico 就被图片规则吃掉。
fn category_ext<'a>(is_lnk: bool, own_ext: &'a str, target_ext: &'a str) -> &'a str {
    if !is_lnk {
        return own_ext;
    }
    match target_ext {
        "" | "lnk" | "ico" | "cur" => own_ext,
        other => other,
    }
}

/// 关键词优先于扩展名（规则里的扩展名可带点），都不命中则归入「其他」
pub(crate) fn match_category(name: &str, ext: &str, rules: &[DesktopRule]) -> String {
    let name_lower = name.to_lowercase();
    for r in rules.iter().filter(|r| r.enabled) {
        if rule_tokens(&r.keywords).any(|k| name_lower.contains(&k)) {
            return r.category.clone();
        }
    }
    for r in rules.iter().filter(|r| r.enabled) {
        if rule_tokens(&r.extensions).any(|e| e.trim_start_matches('.') == ext) {
            return r.category.clone();
        }
    }
    "其他".to_string()
}

/// 登记或更新一个桌面程序。同一个程序在两种整理模式下写法不一样：只登记时存的是快捷方式
/// 解析出来的真实目标，搬进归档后存的是那条 .lnk 的新路径。所以「是不是同一条」要把这几个
/// 写法一起查（`aliases` 带上桌面原路径和解析目标）：命中就改那一行、连 target 一起改过来，
/// 不再插第二条 —— 否则改一下勾选再整理一次，页面上就多出一个一模一样的图标。
fn upsert_app_from_desktop(
    conn: &Connection,
    name: &str,
    target: &str,
    aliases: &[String],
    category: &str,
    icon_path: Option<&str>,
) -> AppResult<()> {
    let cat_id = categories::get_categories(conn, "app")?
        .into_iter()
        .find(|c| c.name == category)
        .map(|c| c.id);
    let mut hits: Vec<i64> = Vec::new();
    for key in std::iter::once(target).chain(aliases.iter().map(String::as_str)) {
        if let Some(id) = conn
            .query_row("SELECT id FROM apps WHERE target = ?1", params![key], |r| r.get(0))
            .optional()?
        {
            hits.push(id);
        }
    }
    hits.sort_unstable();
    hits.dedup();
    match hits.first() {
        // 老库里万一已经有好几条，只改最早的那条，剩下的交给用户自己删，不偷偷动他的东西。
        // icon_path 用 COALESCE：这一轮没抽到图标（快捷方式指向的程序被挪走、U 盘没插）
        // 不该把以前抽好存着的那张抹掉 —— 抹了卡片就退化成字母块，看着像图标丢了。
        Some(id) => {
            conn.execute(
                "UPDATE apps SET name = ?1, category_id = ?2, source = 'desktop',
                 icon_path = COALESCE(?3, icon_path), target = ?4 WHERE id = ?5",
                params![name, cat_id, icon_path, target, id],
            )?;
        }
        None => {
            conn.execute(
                "INSERT INTO apps (name, target, category_id, source, icon_path) VALUES (?1, ?2, ?3, 'desktop', ?4)",
                params![name, target, cat_id, icon_path],
            )?;
        }
    }
    Ok(())
}

fn map_organize_log(r: &Row) -> rusqlite::Result<OrganizeLog> {
    let reverted: i64 = r.get(5)?;
    Ok(OrganizeLog {
        id: r.get(0)?,
        file_name: r.get(1)?,
        from_path: r.get(2)?,
        to_path: r.get(3)?,
        organized_at: r.get(4)?,
        reverted: reverted != 0,
    })
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in data {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::usage::{note_use, APP};
    use std::path::PathBuf;

    /// 用真 schema.sql 建库：手抄一遍建表语句迟早和生产库走偏，那样的绿色没意义
    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(include_str!("../db/schema.sql")).unwrap();
        c
    }

    /// 每个用例一个临时目录，进去先清干净；真桌面只有 win::desktop 知道，测试碰不到
    fn sandbox(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("workbench-desktop-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, b"x").unwrap();
        p
    }

    fn s(p: &Path) -> String {
        p.to_string_lossy().to_string()
    }

    fn rule(category: &str, keywords: Option<&str>, extensions: Option<&str>, enabled: bool) -> DesktopRule {
        DesktopRule {
            id: 0,
            category: category.to_string(),
            keywords: keywords.map(str::to_string),
            extensions: extensions.map(str::to_string),
            enabled,
            sort: 0,
        }
    }

    fn plan(path: &Path, category: &str, skip_move: Option<bool>) -> OrganizePlan {
        OrganizePlan {
            path: s(path),
            category: category.to_string(),
            icon_path: None,
            skip_move,
        }
    }

    fn entry(name: &str, target: &str) -> AppEntry {
        AppEntry {
            id: 0,
            name: name.to_string(),
            alias: None,
            description: None,
            target: target.to_string(),
            args: None,
            icon_path: None,
            category_id: None,
            source: "manual".to_string(),
            created_at: None,
            use_count: 0,
        }
    }

    /// 按插入顺序取 target，断言启动器指向哪儿
    fn targets(c: &Connection) -> Vec<String> {
        let mut stmt = c.prepare("SELECT target FROM apps ORDER BY id").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn keyword_rule_wins_over_extension_and_disabled_rules_are_skipped() {
        let rules = vec![
            rule("文档", None, Some("pdf, .docx"), true),
            rule("报销", Some(" 发票 , 报销单 ,"), None, true),
            rule("废弃", Some("pdf"), None, false),
        ];
        // 名字里带「发票」就先按关键词归报销，哪怕扩展名也命中文档
        assert_eq!(match_category("12月发票.pdf", "pdf", &rules), "报销");
        assert_eq!(match_category("手册.pdf", "pdf", &rules), "文档");
        // 规则里的扩展名带不带点、大小写都算同一个
        assert_eq!(match_category("手册.DOCX", "docx", &rules), "文档");
        assert_eq!(match_category("看不懂的", "bin", &rules), "其他");
        // 关键词只有空格时不该把什么都吃掉
        let blank = vec![rule("空规则", Some(" , "), None, true)];
        assert_eq!(match_category("任意", "txt", &blank), "其他");
    }

    #[test]
    fn a_shortcut_pointing_at_an_icon_is_not_a_picture() {
        let rules = vec![
            rule("图片", None, Some(".jpg,.png,.gif,.svg,.ico,.cur"), true),
            rule("文档", None, Some(".doc,.pdf"), true),
            rule("其他", None, None, true),
        ];
        // Cpolar Web UI.lnk 就是这样：Windows 自己报的 TargetPath 是安装器缓存里的 app.ico
        assert_eq!(category_ext(true, "lnk", "ico"), "lnk");
        assert_eq!(
            match_category("Cpolar Web UI", category_ext(true, "lnk", "ico"), &rules),
            "其他"
        );
        // 指向文档的快捷方式照旧跟着目标走
        assert_eq!(category_ext(true, "lnk", "pdf"), "pdf");
        assert_eq!(match_category("报告", category_ext(true, "lnk", "pdf"), &rules), "文档");
        // 解析不出扩展名、或者快捷方式套快捷方式，都按 .lnk 自己算
        assert_eq!(category_ext(true, "lnk", ""), "lnk");
        assert_eq!(category_ext(true, "lnk", "lnk"), "lnk");
        // 真正的图片文件不受这条影响
        assert_eq!(category_ext(false, "ico", ""), "ico");
        assert_eq!(match_category("logo", category_ext(false, "ico", ""), &rules), "图片");
    }

    #[test]
    fn the_scanner_skips_its_own_executable_and_any_shortcut_pointing_at_it() {
        let dir = sandbox("self-exe");
        let own_file = touch(&dir, "workbench.exe");
        touch(&dir, "别的.exe");
        let rules = vec![rule("其他", None, None, true)];
        let icon_dir = dir.join("icons");
        std::fs::create_dir_all(&icon_dir).unwrap();
        let own = OwnExe {
            file: "workbench.exe".to_string(),
            full: norm_path(&own_file),
        };

        let mut items = Vec::new();
        scan_directory(&s(&dir), 0, &rules, &icon_dir, Some(&own), &mut items);
        assert_eq!(
            items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
            vec!["别的.exe"],
            "自己那份不该进扫描结果"
        );

        // 开发跑 target\\debug 那份、快捷方式指装机那份：路径不同但同名，只比路径会漏
        let installed = sandbox("self-installed");
        let same_name = touch(&installed, "WorkBench.EXE");
        assert!(own.is_this(&same_name, &s(&same_name)), "换个目录的同名 exe 也该认出来");
        let mut second = Vec::new();
        scan_directory(&s(&installed), 0, &rules, &icon_dir, Some(&own), &mut second);
        assert!(second.is_empty(), "{:?}", second);

        // 拿不到自身路径（current_exe 失败）时宁可多列，不能凭空跳过东西
        let mut blind = Vec::new();
        scan_directory(&s(&dir), 0, &rules, &icon_dir, None, &mut blind);
        assert_eq!(blind.len(), 2, "认不出自己时一项都不该被跳掉");
    }

    /// 造一套「已经整理过」的样子：`桌面整理\文档\报告.pdf` 摆在那儿，
    /// 卡片、恢复日志、文件备注、规则四边都记着这个名字
    fn organized_as_docs(desktop: &Path) -> (Connection, PathBuf) {
        let c = conn();
        let cat = desktop.join(ORGANIZE_DIR).join("文档");
        std::fs::create_dir_all(&cat).unwrap();
        let file = cat.join("报告.pdf");
        std::fs::write(&file, b"x").unwrap();
        let (to, root) = (s(&file), s(&cat));
        c.execute("INSERT INTO categories (id, name, type) VALUES (1, '文档', 'app')", [])
            .unwrap();
        c.execute(
            "INSERT INTO desktop_rules (id, category, extensions) VALUES (30, '文档', '.pdf')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO apps (id, name, target, category_id) VALUES (10, '报告.pdf', ?1, 1)",
            params![to],
        )
        .unwrap();
        c.execute(
            "INSERT INTO organize_logs (id, file_name, from_path, to_path) VALUES (1, '报告.pdf', 'C:/Users/me/Desktop/报告.pdf', ?1)",
            params![to],
        )
        .unwrap();
        c.execute(
            "INSERT INTO files (id, path, name, managed_root) VALUES (20, ?1, '报告.pdf', ?2)",
            params![to, root],
        )
        .unwrap();
        (c, cat)
    }

    fn scalar(c: &Connection, sql: &str) -> String {
        c.query_row(sql, [], |r| r.get::<_, String>(0)).unwrap()
    }

    #[test]
    fn renaming_a_rule_category_moves_the_folder_and_every_row_that_remembers_it() {
        let desktop = sandbox("rule-rename");
        let (c, cat) = organized_as_docs(&desktop);
        let mut renamed = rule("资料", None, Some(".pdf"), true);
        renamed.id = 30;

        save_rule_with_rename(&c, &renamed, &desktop).unwrap();

        let new_file = desktop.join(ORGANIZE_DIR).join("资料").join("报告.pdf");
        assert!(!cat.exists(), "旧目录该跟着改名，不该留个空壳");
        assert!(new_file.exists(), "目录里的文件没跟着走");
        let (to, root) = (s(&new_file), s(new_file.parent().unwrap()));
        assert_eq!(scalar(&c, "SELECT name FROM categories WHERE id = 1"), "资料");
        assert_eq!(scalar(&c, "SELECT category FROM desktop_rules WHERE id = 30"), "资料");
        // 卡片双击要靠 target、恢复整理要靠 to_path、文件备注要靠 path：三条都得换到新路径
        assert_eq!(scalar(&c, "SELECT target FROM apps WHERE id = 10"), to);
        assert_eq!(scalar(&c, "SELECT to_path FROM organize_logs WHERE id = 1"), to);
        assert_eq!(scalar(&c, "SELECT path FROM files WHERE id = 20"), to);
        assert_eq!(scalar(&c, "SELECT managed_root FROM files WHERE id = 20"), root);
    }

    #[test]
    fn a_rule_name_still_used_by_another_rule_leaves_the_folder_alone() {
        let desktop = sandbox("rule-rename-shared");
        let (c, cat) = organized_as_docs(&desktop);
        c.execute(
            "INSERT INTO desktop_rules (id, category, keywords) VALUES (31, '文档', '合同')",
            [],
        )
        .unwrap();
        let mut renamed = rule("资料", None, Some(".pdf"), true);
        renamed.id = 30;

        save_rule_with_rename(&c, &renamed, &desktop).unwrap();

        // 31 号规则还叫「文档」，动分类名和目录等于替它改名
        assert!(cat.exists());
        assert_eq!(scalar(&c, "SELECT name FROM categories WHERE id = 1"), "文档");
        assert_eq!(scalar(&c, "SELECT category FROM desktop_rules WHERE id = 30"), "资料");
    }

    #[test]
    fn a_rule_rename_onto_an_existing_category_is_refused_before_anything_moves() {
        let desktop = sandbox("rule-rename-clash");
        let (c, cat) = organized_as_docs(&desktop);
        c.execute("INSERT INTO categories (id, name, type) VALUES (40, '图片', 'app')", [])
            .unwrap();
        let mut renamed = rule("图片", None, Some(".pdf"), true);
        renamed.id = 30;

        let err = save_rule_with_rename(&c, &renamed, &desktop).unwrap_err().to_string();
        assert!(err.contains("图片"), "{err}");
        // 拒绝要拒在写之前：规则、分类、目录三者都还是原样
        assert!(cat.exists());
        assert_eq!(scalar(&c, "SELECT name FROM categories WHERE id = 1"), "文档");
        assert_eq!(scalar(&c, "SELECT category FROM desktop_rules WHERE id = 30"), "文档");
    }

    #[test]
    fn a_rule_rename_onto_an_existing_folder_is_refused() {
        let desktop = sandbox("rule-rename-folder-taken");
        let (c, cat) = organized_as_docs(&desktop);
        std::fs::create_dir_all(desktop.join(ORGANIZE_DIR).join("资料")).unwrap();
        let mut renamed = rule("资料", None, Some(".pdf"), true);
        renamed.id = 30;

        let err = save_rule_with_rename(&c, &renamed, &desktop).unwrap_err().to_string();
        assert!(err.contains("已经存在"), "{err}");
        assert!(cat.exists(), "目录被占了就不该动旧目录");
        assert_eq!(scalar(&c, "SELECT category FROM desktop_rules WHERE id = 30"), "文档");
    }

    #[test]
    fn extension_is_lowercased_and_only_the_last_dot_counts() {
        assert_eq!(extension_of(Path::new("a/b/PDF.PDF")), "pdf");
        assert_eq!(extension_of(Path::new("a/archive.tar.GZ")), "gz");
        assert_eq!(extension_of(Path::new("Makefile")), "");
        // 开头那个点算隐藏位，不是扩展名分隔符
        assert_eq!(extension_of(Path::new(".gitignore")), "");
    }

    #[test]
    fn an_occupied_archive_name_gets_a_numbered_copy() {
        let dir = sandbox("free-dest");
        let cat = dir.join("文档");
        std::fs::create_dir_all(&cat).unwrap();
        let src = Path::new("/desktop/报告.pdf");
        assert_eq!(free_dest(&cat, src, "报告.pdf"), cat.join("报告.pdf"));
        touch(&cat, "报告.pdf");
        assert_eq!(free_dest(&cat, src, "报告.pdf"), cat.join("报告 (1).pdf"));
        touch(&cat, "报告 (1).pdf");
        assert_eq!(free_dest(&cat, src, "报告.pdf"), cat.join("报告 (2).pdf"));
        // 没扩展名也一样递增，别把后缀丢了
        let bare = Path::new("/desktop/Makefile");
        touch(&cat, "Makefile");
        assert_eq!(free_dest(&cat, bare, "Makefile"), cat.join("Makefile (1)"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn organize_moves_files_into_category_dirs_and_registers_them() {
        let desktop = sandbox("organize");
        let pdf = touch(&desktop, "报告.pdf");
        let txt = touch(&desktop, "笔记.txt");
        let c = conn();
        c.execute("INSERT INTO categories (name, type) VALUES ('文档','app')", []).unwrap();

        let logs = organize_into(
            &c,
            &[plan(&pdf, "文档", None), plan(&txt, "其他", None)],
            &desktop,
        )
        .unwrap();

        assert_eq!(logs.len(), 2);
        let moved_pdf = desktop.join("桌面整理").join("文档").join("报告.pdf");
        let moved_txt = desktop.join("桌面整理").join("其他").join("笔记.txt");
        assert!(moved_pdf.exists(), "文件要落到 桌面整理/<分类>/ 下");
        assert!(!pdf.exists(), "原位置该空出来");
        assert_eq!(targets(&c), vec![s(&moved_pdf), s(&moved_txt)]);
        let rows = get_apps(&c).unwrap();
        assert_eq!(rows[0].category_id, Some(1), "分类按名字对上库里那条");
        assert_eq!(rows[0].source, "desktop");
        assert_eq!(rows[1].category_id, None, "「其他」没建分类就是未分类");
        assert!(!logs[0].reverted);
        let _ = std::fs::remove_dir_all(&desktop);
    }

    #[test]
    fn organize_never_overwrites_a_file_already_sitting_in_the_category_dir() {
        let desktop = sandbox("organize-clash");
        let cat = desktop.join("桌面整理").join("文档");
        std::fs::create_dir_all(&cat).unwrap();
        let keeper = touch(&cat, "报告.pdf");
        let pdf = touch(&desktop, "报告.pdf");
        let c = conn();

        let logs = organize_into(&c, &[plan(&pdf, "文档", None)], &desktop).unwrap();

        assert_eq!(logs[0].to_path.as_deref(), Some(s(&cat.join("报告 (1).pdf")).as_str()));
        assert!(keeper.exists(), "归档里原来那份不能被动掉");
        assert!(cat.join("报告 (1).pdf").exists());
        let _ = std::fs::remove_dir_all(&desktop);
    }

    #[test]
    fn register_only_plan_leaves_the_file_on_the_desktop() {
        let desktop = sandbox("register-only");
        let pdf = touch(&desktop, "报告.pdf");
        let c = conn();

        let logs = organize_into(&c, &[plan(&pdf, "文档", Some(true))], &desktop).unwrap();

        assert!(logs.is_empty());
        assert!(pdf.exists(), "只登记不该搬走桌面文件");
        assert_eq!(targets(&c), vec![s(&pdf)]);
        let logged: i64 = c
            .query_row("SELECT COUNT(*) FROM organize_logs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(logged, 0, "没搬就不该留下可撤销的流水");
        let _ = std::fs::remove_dir_all(&desktop);
    }

    #[test]
    fn revert_puts_everything_back_and_repoints_the_launcher() {
        let desktop = sandbox("revert");
        let pdf = touch(&desktop, "报告.pdf");
        let c = conn();
        organize_into(&c, &[plan(&pdf, "文档", None)], &desktop).unwrap();
        let moved = desktop.join("桌面整理").join("文档").join("报告.pdf");
        assert!(moved.exists());

        assert_eq!(revert_organize(&c, None).unwrap(), 1);
        assert!(pdf.exists(), "撤销要搬回原位置");
        assert!(!moved.exists());
        assert!(
            !desktop.join("桌面整理").exists(),
            "被搬空的分类目录和只剩空目录的「桌面整理」都该收掉"
        );
        assert_eq!(targets(&c), vec![s(&pdf)], "启动器不能留成指向归档的死链");
        assert_eq!(revert_organize(&c, None).unwrap(), 0, "撤销过的流水不该再算一遍");
        let _ = std::fs::remove_dir_all(&desktop);
    }

    #[test]
    fn a_copy_that_already_vanished_is_marked_but_not_counted() {
        let desktop = sandbox("revert-gone");
        let from = desktop.join("报告.pdf");
        // 归档那份压根不存在：用户自己删了或移走了
        let to = desktop.join("桌面整理").join("文档").join("报告.pdf");
        let c = conn();
        c.execute(
            "INSERT INTO apps (name, target, source) VALUES ('报告', ?1, 'desktop')",
            params![s(&to)],
        )
        .unwrap();
        c.execute(
            "INSERT INTO organize_logs (file_name, from_path, to_path) VALUES ('报告.pdf', ?1, ?2)",
            params![s(&from), s(&to)],
        )
        .unwrap();
        let id = c.last_insert_rowid();

        assert_eq!(revert_organize(&c, Some(id)).unwrap(), 0, "副本都没了，不该谎报搬回一个");
        let reverted: i64 = c
            .query_row("SELECT reverted FROM organize_logs WHERE id = ?1", params![id], |r| r.get(0))
            .unwrap();
        assert_eq!(reverted, 1, "状态还是得补记，否则每次恢复都白跑一遍");
        assert_eq!(targets(&c), vec![s(&from)], "启动器要跟回原位置");
        let _ = std::fs::remove_dir_all(&desktop);
    }

    #[test]
    fn repoint_drops_the_archive_row_when_home_already_has_one() {
        let c = conn();
        c.execute("INSERT INTO apps (name, target) VALUES ('原位','C:/d/报告.pdf')", []).unwrap();
        c.execute("INSERT INTO apps (name, target) VALUES ('归档','C:/d/桌面整理/文档/报告.pdf')", []).unwrap();

        repoint_app(&c, "C:/d/报告.pdf", "C:/d/桌面整理/文档/报告.pdf").unwrap();

        assert_eq!(targets(&c), vec!["C:/d/报告.pdf".to_string()], "同一份文件只该留一张卡片");
    }

    #[test]
    fn prune_only_removes_dirs_that_emptied_out() {
        let root = sandbox("prune");
        let base = root.join("桌面整理");
        let empty_cat = base.join("文档");
        let full_cat = base.join("其他");
        std::fs::create_dir_all(&empty_cat).unwrap();
        std::fs::create_dir_all(&full_cat).unwrap();
        touch(&full_cat, "a.txt");

        prune_empty_dirs(&[empty_cat.clone(), full_cat.clone()]);

        assert!(!empty_cat.exists(), "搬空的分类目录该删掉");
        assert!(full_cat.exists(), "非空目录一律留着");
        assert!(base.exists(), "归档根目录里还有东西就不该动");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn registering_the_same_target_twice_updates_instead_of_doubling_up() {
        let c = conn();
        c.execute("INSERT INTO categories (name, type) VALUES ('文档','app')", []).unwrap();
        c.execute("INSERT INTO categories (name, type) VALUES ('办公','app')", []).unwrap();

        upsert_app_from_desktop(&c, "报告", "C:/x/报告.pdf", &[], "文档", Some("i.png")).unwrap();
        upsert_app_from_desktop(&c, "报告 修订", "C:/x/报告.pdf", &[], "办公", None).unwrap();

        let rows = get_apps(&c).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "报告 修订");
        assert_eq!(rows[0].category_id, Some(2));
    }

    /// 换个勾选再整理一次，不该多出一张一模一样的卡片
    #[test]
    fn organizing_twice_with_different_modes_keeps_one_card() {
        let desktop = sandbox("organize-twice");
        let tool = touch(&desktop, "Tool.exe");
        let c = conn();

        organize_into(&c, &[plan(&tool, "其他", Some(true))], &desktop).unwrap();
        let only = targets(&c);
        assert_eq!(only.len(), 1, "只登记一次就该有一条");
        assert_eq!(only[0], s(&tool), "只登记时记的是桌面上那个文件");

        // 第二次改成「移动并添加」：文件搬走了，同一条要跟着改路径，而不是再插一条
        organize_into(&c, &[plan(&tool, "其他", Some(false))], &desktop).unwrap();
        let rows = get_apps(&c).unwrap();
        assert_eq!(rows.len(), 1, "换了模式也不该出现第二张卡片");
        assert_eq!(
            rows[0].target,
            s(&desktop.join("桌面整理").join("其他").join("Tool.exe")),
            "卡片要跟去归档后的新位置"
        );
        let _ = std::fs::remove_dir_all(&desktop);
    }

    /// 别名命中时（归档里的 .lnk 与它解析出的 exe 是同一个程序）改最早那条，并换成新写法
    #[test]
    fn an_alias_match_reuses_the_existing_row() {
        let c = conn();
        upsert_app_from_desktop(&c, "App.lnk", "C:/archived/App.lnk", &[], "其他", None).unwrap();
        upsert_app_from_desktop(
            &c,
            "App.lnk",
            "C:/real/App.exe",
            &["C:/archived/App.lnk".to_string()],
            "其他",
            None,
        )
        .unwrap();

        let rows = get_apps(&c).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].target, "C:/real/App.exe");
    }

    /// 这一轮没抽到图标（目标被卸载、U 盘没插）不该把以前存着的那张抹掉
    #[test]
    fn a_failed_extraction_keeps_the_stored_icon() {
        let c = conn();
        upsert_app_from_desktop(&c, "U盾工具", "C:/u/tool.exe", &[], "其他", Some("C:/icons/a.png")).unwrap();
        upsert_app_from_desktop(&c, "U盾工具", "C:/u/tool.exe", &[], "其他", None).unwrap();

        let rows = get_apps(&c).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].icon_path.as_deref(),
            Some("C:/icons/a.png"),
            "抽图标失败不该把已有图标清空"
        );

        upsert_app_from_desktop(&c, "U盾工具", "C:/u/tool.exe", &[], "其他", Some("C:/icons/b.png")).unwrap();
        assert_eq!(
            get_apps(&c).unwrap()[0].icon_path.as_deref(),
            Some("C:/icons/b.png"),
            "这次抽到了，还是要换成新的"
        );
    }

    /// 「只处理新增」的判断依据：.lnk 自身或它解析出的目标，任一种写法在启动器里都算已登记
    #[test]
    fn already_registered_items_are_recognized_either_way() {
        let item = |path: &str, target: &str| DesktopItem {
            name: "x".into(),
            path: path.into(),
            target: target.into(),
            is_lnk: true,
            category: "其他".into(),
            icon_path: None,
            registered: false,
        };
        let mut items = vec![
            item(r"C:\d\A.lnk", r"C:\app\a.exe"),
            item(r"C:\d\B.lnk", r"C:\app\b.exe"),
            item(r"C:\d\C.lnk", r"C:\app\c.exe"),
        ];
        let known = ["c:\\d\\a.lnk", "C:\\APP\\b.exe"]
            .into_iter()
            .map(|s| s.to_lowercase())
            .collect::<std::collections::HashSet<_>>();

        mark_registered(&mut items, &known);

        assert_eq!(
            items.iter().map(|i| i.registered).collect::<Vec<_>>(),
            vec![true, true, false],
            "大小写不同、存的是 .lnk 还是 exe，都该认出来"
        );
    }

    #[test]
    fn save_app_inserts_on_zero_id_and_updates_otherwise() {
        let c = conn();
        let saved = save_app(&c, &entry("新程序", "C:/a.exe")).unwrap();
        assert_eq!(saved.id, 1, "新增要带回真的 id，前端下一轮要靠它计数");
        let edited = AppEntry {
            id: saved.id,
            name: "改了名".to_string(),
            target: "C:/b.exe".to_string(),
            ..saved.clone()
        };
        save_app(&c, &edited).unwrap();
        let rows = get_apps(&c).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "改了名");
        assert_eq!(rows[0].target, "C:/b.exe");
    }

    #[test]
    fn clicked_apps_lead_the_list_and_deleting_forgets_the_count() {
        let c = conn();
        let a = save_app(&c, &entry("WPS", "C:/wps.exe")).unwrap();
        let b = save_app(&c, &entry("Terminal", "C:/wt.exe")).unwrap();
        note_use(&c, APP, b.id).unwrap();
        note_use(&c, APP, b.id).unwrap();
        note_use(&c, APP, a.id).unwrap();

        let rows = get_apps(&c).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            vec!["Terminal", "WPS"],
            "点得多的排前面"
        );
        assert_eq!(rows[0].use_count, 2);

        delete_app(&c, b.id).unwrap();
        let left: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM usage_stats WHERE kind = 'app' AND id = ?1",
                params![b.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 0, "删了程序要把计数一起带走，不然 id 复用会继承旧账");
    }

    #[test]
    fn launch_by_id_reports_the_missing_row_with_its_id() {
        let c = conn();
        let err = launch_by_id(&c, 4242).unwrap_err().to_string();
        assert!(err.contains("4242"), "报错要带上那个 id，got {err}");
    }
}