//! 把确认过的判断落库：分类不存在就先建，然后改条目那条的 category_id。
//! 记忆只在落库这一刻写——记的是他确认过的分类，不是模型随口那句。

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::AppResult;
use crate::models::{ApplyCategorizeUpdate, Category};
use crate::services::ai::memory;
use crate::services::categories;

use super::stem;

/// 回被应用掉的条数
pub fn apply(
    conn: &Connection,
    item_type: &str,
    updates: &[ApplyCategorizeUpdate],
) -> AppResult<usize> {
    let mut cats = categories::get_categories(conn, item_type)?;
    let mut count = 0;
    for update in updates {
        apply_one(conn, item_type, update, &mut cats)?;
        count += 1;
    }
    Ok(count)
}

fn apply_one(
    conn: &Connection,
    item_type: &str,
    update: &ApplyCategorizeUpdate,
    cats: &mut Vec<Category>,
) -> AppResult<()> {
    // 模型或人改出来的分类不存在时先建再用，建完记进 cats 以免同一份里重复建
    let cat_id = match cats.iter().find(|c| c.name == update.category_name) {
        Some(c) => c.id,
        None => {
            let created = categories::save_category(
                conn,
                &Category {
                    id: 0,
                    r#type: item_type.to_string(),
                    name: update.category_name.clone(),
                    parent_id: None,
                    sort: 0,
                    icon: None,
                },
            )?;
            cats.push(created.clone());
            created.id
        }
    };

    let table = if item_type == "app" { "apps" } else { "bookmarks" };
    conn.execute(
        &format!("UPDATE {} SET category_id = ?1 WHERE id = ?2", table),
        params![cat_id, update.id],
    )?;
    if item_type == "app" {
        let name: Option<String> = conn
            .query_row("SELECT name FROM apps WHERE id = ?1", params![update.id], |r| r.get(0))
            .optional()?;
        if let Some(name) = name {
            memory::remember(
                conn,
                &stem(&name),
                &update.category_name,
                update.reason.as_deref().unwrap_or("人工确认的分类"),
                "确认",
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(include_str!("../../../db/schema.sql")).unwrap();
        c.execute("INSERT INTO categories (id, name, type) VALUES (1, '文档', 'app'), (2, '开发工具', 'app')", [])
            .unwrap();
        c.execute("INSERT INTO desktop_rules (category, extensions) VALUES ('文档', '.pdf')", [])
            .unwrap();
        c.execute("INSERT INTO apps (id, name, target, category_id) VALUES (10, 'Loomy.lnk', 'C:/loom/Loomy.exe', NULL), (11, '报告.pdf', 'C:/desk/报告.pdf', NULL)", [])
            .unwrap();
        c
    }

    fn update(id: i64, category: &str, reason: Option<&str>) -> ApplyCategorizeUpdate {
        ApplyCategorizeUpdate {
            id,
            category_name: category.into(),
            reason: reason.map(Into::into),
        }
    }

    fn assigned(c: &Connection, sql: &str) -> i64 {
        c.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    /// 记忆只在应用那一刻写：记的是他确认过的分类，不是模型随口那句
    #[test]
    fn applying_a_category_memorizes_the_app() {
        let c = conn();
        apply(&c, "app", &[update(10, "开发工具", Some("AI 编程助手"))]).unwrap();

        let hit = memory::lookup(&c, "Loomy").unwrap().expect("应用完没记进记忆");
        assert_eq!(hit.category, "开发工具");
        assert_eq!(hit.evidence, "AI 编程助手");
        assert_eq!(hit.source, "确认");
        assert_eq!(assigned(&c, "SELECT category_id FROM apps WHERE id = 10"), 2);
    }

    /// 同名分类规则不拦记忆：规则只管桌面文件挪去哪儿，程序落到哪个分类都该记住
    #[test]
    fn a_rule_named_category_is_still_memorized() {
        let c = conn();
        assert_eq!(apply(&c, "app", &[update(11, "文档", None)]).unwrap(), 1);

        let hit = memory::lookup(&c, "报告").unwrap().expect("应用完没记进记忆");
        assert_eq!(hit.category, "文档");
        assert_eq!(hit.source, "确认");
    }

    #[test]
    fn bookmarks_are_classified_but_not_memorized() {
        let c = conn();
        c.execute("INSERT INTO categories (id, name, type) VALUES (5, '技术社区', 'bookmark')", [])
            .unwrap();
        c.execute("INSERT INTO bookmarks (id, title, url) VALUES (7, '掘金', 'https://juejin.cn')", [])
            .unwrap();
        apply(&c, "bookmark", &[update(7, "技术社区", Some("网页浏览"))]).unwrap();

        assert!(memory::list(&c).unwrap().is_empty(), "书签不进记忆");
        assert_eq!(
            assigned(&c, "SELECT category_id FROM bookmarks WHERE id = 7"),
            5,
            "书签本身还是要归类的"
        );
    }

    /// 名单里还没有的分类是模型现编的：先建再用，同一批里也不该建两遍
    #[test]
    fn a_brand_new_category_is_created_once_per_batch() {
        let c = conn();
        let n = apply(
            &c,
            "app",
            &[update(10, "新建的", None), update(11, "新建的", None)],
        )
        .unwrap();
        assert_eq!(n, 2);
        let created: i64 = c
            .query_row("SELECT COUNT(*) FROM categories WHERE name = '新建的' AND type = 'app'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(created, 1, "同一份里重复的分类名只建一条");
        assert_eq!(
            assigned(&c, "SELECT category_id FROM apps WHERE id = 11"),
            assigned(&c, "SELECT category_id FROM apps WHERE id = 10"),
            "两条该落到同一个新分类上"
        );
    }
}
