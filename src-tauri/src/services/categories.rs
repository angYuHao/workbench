use rusqlite::{params, Connection, OptionalExtension};

use crate::error::AppResult;
use crate::models::{Category, DesktopRule};

pub fn get_categories(conn: &Connection, type_: &str) -> AppResult<Vec<Category>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, type, parent_id, sort, icon FROM categories WHERE type = ?1 ORDER BY sort, id",
    )?;
    let rows = stmt.query_map(params![type_], |r| {
        Ok(Category {
            id: r.get(0)?,
            name: r.get(1)?,
            r#type: r.get(2)?,
            parent_id: r.get(3)?,
            sort: r.get(4)?,
            icon: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn save_category(conn: &Connection, cat: &Category) -> AppResult<Category> {
    if cat.id > 0 {
        conn.execute(
            "UPDATE categories SET name = ?1, type = ?2, parent_id = ?3, sort = ?4, icon = ?5 WHERE id = ?6",
            params![cat.name, cat.r#type, cat.parent_id, cat.sort, cat.icon, cat.id],
        )?;
        Ok(cat.clone())
    } else {
        conn.execute(
            "INSERT INTO categories (name, type, parent_id, sort, icon) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![cat.name, cat.r#type, cat.parent_id, cat.sort, cat.icon],
        )?;
        let id = conn.last_insert_rowid();
        Ok(Category { id, ..cat.clone() })
    }
}

/// 桌面分类改名要把规则一起带上：`desktop_rules.category` 存的是名字不是 id，
/// 只改 categories 那一行，整理时规则就会去开一个旧名的目录，等于改名没生效。
/// 书签没有规则，改它只动 categories。返回被改写的规则条数，界面好如实说「顺带改了 N 条规则」。
pub fn rename_category(conn: &Connection, id: i64, name: &str) -> AppResult<usize> {
    let name = name.trim();
    if name.is_empty() {
        return Err(crate::error::AppError::msg("分类名不能为空"));
    }
    let current: Option<(String, String)> = conn
        .query_row(
            "SELECT name, type FROM categories WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (old, type_) = current.ok_or_else(|| crate::error::AppError::msg("分类不存在"))?;
    if old == name {
        return Ok(0);
    }
    let clash: Option<i64> = conn
        .query_row(
            "SELECT id FROM categories WHERE type = ?1 AND name = ?2 COLLATE NOCASE AND id <> ?3",
            params![type_, name, id],
            |r| r.get(0),
        )
        .optional()?;
    if clash.is_some() {
        return Err(crate::error::AppError::msg(format!("「{name}」已经存在")));
    }

    // 规则只跟桌面分类的名字挂钩（书签没有规则），改书签的分类名不该动规则；
    // 桌面这边两处必须一起改：只改了分类名没改规则，整理时规则又去开一个旧名的目录
    conn.execute_batch("BEGIN")?;
    let written = conn
        .execute("UPDATE categories SET name = ?1 WHERE id = ?2", params![name, id])
        .and_then(|_| {
            if type_ == "app" {
                conn.execute(
                    "UPDATE desktop_rules SET category = ?1 WHERE category = ?2",
                    params![name, old],
                )
            } else {
                Ok(0)
            }
        });
    match written {
        Ok(rules) => {
            crate::db::commit(conn)?;
            Ok(rules)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e.into())
        }
    }
}

/// 批量删分类：一个事务，要么全删要么都不删。回 (退回未分类的项目数, 被一起删掉的规则条数)。
/// 规则默认不动 —— 和单条删除一样由界面问一次；`also_rules` 为真时按名字连带删。
pub fn delete_categories(conn: &Connection, ids: &[i64], also_rules: bool) -> AppResult<(usize, usize)> {
    let mut moved = 0;
    let mut rules = 0;
    conn.execute_batch("BEGIN")?;
    let done = (|| -> AppResult<()> {
        for id in ids {
            let name: Option<String> = conn
                .query_row(
                    "SELECT name FROM categories WHERE id = ?1",
                    params![id],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(name) = name else { continue };
            for table in ["apps", "bookmarks", "files"] {
                moved += conn.execute(
                    &format!("UPDATE {table} SET category_id = NULL WHERE category_id = ?1"),
                    params![id],
                )?;
            }
            conn.execute("DELETE FROM categories WHERE id = ?1", params![id])?;
            if also_rules {
                rules += conn.execute("DELETE FROM desktop_rules WHERE category = ?1", params![name])?;
            }
        }
        Ok(())
    })();
    match done {
        Ok(()) => {
            crate::db::commit(conn)?;
            Ok((moved, rules))
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// 删分类不删东西：先把挂在里面的项目挪回「未分类」，再删分类本身。
/// also_rules 为真时，顺手删掉指向这个名字的整理规则（规则按名字关联，不按 id）。
/// 返回被移走的项目数，界面好如实告诉用户「N 项已回到未分类」。
pub fn delete_category(conn: &Connection, id: i64, also_rules: bool) -> AppResult<usize> {
    // 走批量那一个事务实现：单条也一样要「要么全做完要么都不动」，
    // 不能出现项目已退回未分类、分类本身还在的半截状态。
    delete_categories(conn, &[id], also_rules).map(|(moved, _)| moved)
}

pub fn get_rules(conn: &Connection) -> AppResult<Vec<DesktopRule>> {
    let mut stmt = conn.prepare(
        "SELECT id, category, keywords, extensions, enabled, sort FROM desktop_rules ORDER BY sort, id",
    )?;
    let rows = stmt.query_map([], |r| {
        let enabled: i64 = r.get(4)?;
        Ok(DesktopRule {
            id: r.get(0)?,
            category: r.get(1)?,
            keywords: r.get(2)?,
            extensions: r.get(3)?,
            enabled: enabled != 0,
            sort: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn save_rule(conn: &Connection, rule: &DesktopRule) -> AppResult<DesktopRule> {
    let enabled = if rule.enabled { 1 } else { 0 };
    if rule.id > 0 {
        conn.execute(
            "UPDATE desktop_rules SET category = ?1, keywords = ?2, extensions = ?3, enabled = ?4, sort = ?5 WHERE id = ?6",
            params![rule.category, rule.keywords, rule.extensions, enabled, rule.sort, rule.id],
        )?;
        Ok(rule.clone())
    } else {
        conn.execute(
            "INSERT INTO desktop_rules (category, keywords, extensions, enabled, sort) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![rule.category, rule.keywords, rule.extensions, enabled, rule.sort],
        )?;
        let id = conn.last_insert_rowid();
        Ok(DesktopRule { id, ..rule.clone() })
    }
}

pub fn delete_rule(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM desktop_rules WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute(
            "CREATE TABLE categories (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               name TEXT NOT NULL,
               type TEXT NOT NULL,
               parent_id INTEGER,
               sort INTEGER DEFAULT 0,
               icon TEXT
             )",
            [],
        )
        .unwrap();
        c.execute(
            "CREATE TABLE desktop_rules (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               category TEXT NOT NULL,
               keywords TEXT,
               extensions TEXT,
               enabled INTEGER DEFAULT 1,
               sort INTEGER DEFAULT 0
             )",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO categories (id, name, type) VALUES (1, '文档', 'app'), (2, '图片', 'app'), (3, '文档', 'bookmark')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO desktop_rules (category, extensions) VALUES ('文档', '.pdf'), ('图片', '.png')",
            [],
        )
        .unwrap();
        c
    }

    /// 批量删除要碰到 apps/bookmarks/files 三张表，所以给一套齐的表
    fn full_conn() -> Connection {
        let c = conn();
        c.execute("CREATE TABLE apps (id INTEGER PRIMARY KEY, name TEXT, category_id INTEGER)", []).unwrap();
        c.execute("CREATE TABLE bookmarks (id INTEGER PRIMARY KEY, title TEXT, category_id INTEGER)", []).unwrap();
        c.execute("CREATE TABLE files (id INTEGER PRIMARY KEY, path TEXT, category_id INTEGER)", []).unwrap();
        c.execute("INSERT INTO apps (id, name, category_id) VALUES (10, 'a.docx', 1), (11, 'b.png', 2), (12, 'c.txt', NULL)", []).unwrap();
        c
    }

    fn left(c: &Connection, sql: &str) -> i64 {
        c.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn rule_names(c: &Connection) -> Vec<String> {
        let mut stmt = c.prepare("SELECT category FROM desktop_rules ORDER BY id").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    fn name_of(c: &Connection, id: i64) -> String {
        c.query_row("SELECT name FROM categories WHERE id = ?1", params![id], |r| r.get(0))
            .unwrap()
    }

    fn rule_cat_of(c: &Connection, exts: &str) -> String {
        c.query_row("SELECT category FROM desktop_rules WHERE extensions = ?1", params![exts], |r| r.get(0))
            .unwrap()
    }

    /// 规则和分类只靠名字连着：改了名不改规则，整理时就会再开一个旧名的目录
    #[test]
    fn renaming_a_category_renames_its_rules_too() {
        let c = conn();
        let touched = rename_category(&c, 1, "办公文档").unwrap();

        assert_eq!(touched, 1, "指向旧名的规则没跟着走");
        assert_eq!(name_of(&c, 1), "办公文档");
        assert_eq!(rule_cat_of(&c, ".pdf"), "办公文档");
        assert_eq!(rule_cat_of(&c, ".png"), "图片", "别的规则不该被捎带改掉");
    }

    #[test]
    fn a_name_that_is_already_taken_moves_nothing() {
        let c = conn();
        let err = rename_category(&c, 1, "图片").unwrap_err().to_string();

        assert!(err.contains("已经存在"), "{err}");
        assert_eq!(name_of(&c, 1), "文档");
        assert_eq!(rule_cat_of(&c, ".pdf"), "文档");
    }

    /// 重复只在同一类里算，规则也只跟桌面分类的名字挂钩：书签改名不该动规则
    #[test]
    fn the_other_flavour_may_keep_the_same_name() {
        let c = conn();
        // id 3 是书签「文档」，改成桌面那边已有的名字，不该被判定重名
        assert_eq!(rename_category(&c, 3, "图片").unwrap(), 0, "书签改名动了规则");
        assert_eq!(name_of(&c, 3), "图片");
        assert_eq!(name_of(&c, 1), "文档", "桌面的「文档」不该被这条改名捎带上");
        assert_eq!(rule_cat_of(&c, ".pdf"), "文档", "书签改名把桌面的规则带跑了");
    }

    /// 批量删除的口径：项目退回未分类、分类删掉，规则默认不动，also_rules 才连带删
    #[test]
    fn batch_delete_moves_items_and_can_drop_rules() {
        let c = full_conn();
        assert_eq!(delete_categories(&c, &[1, 2], false).unwrap(), (2, 0), "两个分类下各一个项目没被退回未分类");
        // 只剩 id 3 那条书签分类「文档」：删桌面分类不该伸手到另一个类型里
        assert_eq!(left(&c, "SELECT COUNT(*) FROM categories"), 1, "分类没删掉");
        assert_eq!(left(&c, "SELECT COUNT(*) FROM apps WHERE category_id IS NULL"), 3, "程序本身不该被删，只是退回未分类");
        assert_eq!(rule_names(&c), vec!["文档".to_string(), "图片".to_string()], "没让删规则就不该动规则");
    }

    #[test]
    fn batch_delete_can_take_the_rules_with_it() {
        let c = full_conn();
        assert_eq!(delete_categories(&c, &[1], true).unwrap(), (1, 1));
        assert_eq!(rule_names(&c), vec!["图片".to_string()], "只该带走被删分类同名的那条规则");
        // 不存在的 id 安静跳过，不算失败也不报错
        assert_eq!(delete_categories(&c, &[99], true).unwrap(), (0, 0));
    }

    /// 一半存在一半不存在时不出现「删了一半」：整批要么成要么回滚
    #[test]
    fn batch_delete_is_all_or_nothing() {
        let c = full_conn();
        c.execute("INSERT INTO apps (id, name, category_id) VALUES (7, 'x', 1)", []).unwrap();
        // 制造一个会失败的情况：files 表缺列（模拟库不全），整批必须回滚
        c.execute("DROP TABLE files", []).unwrap();
        let err = delete_categories(&c, &[1, 2], true).unwrap_err().to_string();
        assert!(err.contains("files"), "{err}");
        assert_eq!(left(&c, "SELECT COUNT(*) FROM categories"), 3, "失败了还留着删掉的分类，就是删了一半");
        assert_eq!(left(&c, "SELECT COUNT(*) FROM desktop_rules"), 2, "回滚后规则也该原样在");
        assert_eq!(left(&c, "SELECT COUNT(*) FROM apps WHERE category_id IS NULL"), 1, "回滚后不该有项目被挪走");
    }

    #[test]
    fn renaming_needs_an_existing_row_and_a_real_name() {
        let c = conn();
        assert!(rename_category(&c, 99, "新名").unwrap_err().to_string().contains("不存在"));
        assert!(rename_category(&c, 1, "   ").unwrap_err().to_string().contains("不能为空"));
        // 改名前后的名字只差空格：等于没改，别写库也别动规则
        assert_eq!(rename_category(&c, 1, "文档 ").unwrap(), 0);
        assert_eq!(name_of(&c, 1), "文档");
    }
}