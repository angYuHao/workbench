//! 规则的两半：改名往哪些表、哪些目录扩散（[save_rule_with_rename]），
//! 以及「一个文件算哪一类」的匹配（[match_category]）。

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{AppError, AppResult};
use crate::models::DesktopRule;
use crate::services::categories;

use super::ORGANIZE_DIR;

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
pub(crate) fn category_ext<'a>(is_lnk: bool, own_ext: &'a str, target_ext: &'a str) -> &'a str {
    if !is_lnk {
        return own_ext;
    }
    match target_ext {
        "" | "lnk" | "ico" | "cur" => own_ext,
        other => other,
    }
}

/// 关键词优先于扩展名（规则里的扩展名可带点）。**都不命中就是没归属** ——
/// 回 None，扫描那一步直接把这项丢掉：没规则的东西不该出现在整理清单里，
/// 更不该被塞进一个凭空造出来的「其他」分类。规则里真有一条叫「其他」的，它就是个普通分类名。
pub(crate) fn match_category(name: &str, ext: &str, rules: &[DesktopRule]) -> Option<String> {
    let name_lower = name.to_lowercase();
    for r in rules.iter().filter(|r| r.enabled) {
        if rule_tokens(&r.keywords).any(|k| name_lower.contains(&k)) {
            return Some(r.category.clone());
        }
    }
    for r in rules.iter().filter(|r| r.enabled) {
        if rule_tokens(&r.extensions).any(|e| e.trim_start_matches('.') == ext) {
            return Some(r.category.clone());
        }
    }
    None
}

