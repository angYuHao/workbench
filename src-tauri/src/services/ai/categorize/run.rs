//! 三步编排拆成两半：[`prepare`] 只在库里做完不花钱的判断，[`execute`] 才碰网络。
//! 这么切是为了让「锁不跨 `.await`」这条能从结构上看出来——命令层取锁、调 prepare、放锁，
//! 然后才 await；早先整段编排写在命令层里，一不留神就是把 MutexGuard 带进了等待里。

use std::collections::HashMap;

use rusqlite::Connection;

use crate::error::AppResult;
use crate::models::{SmartCategorizeItem, SmartCategorizeResult};
use crate::services::ai::models::ChatTarget;
use crate::services::{ai::models, categories};

use super::{by_memory, classify, fill_by_search, names_for_model};

/// 本机记忆跑完了，剩下要不要花钱问模型、问的时候能用哪些分类名
pub struct RunPlan {
    /// 一条都没剩下时是 None：连模型配置都不取，也就不会因为「没配模型」报错
    target: Option<ChatTarget>,
    allowed: Vec<String>,
    remembered: Vec<SmartCategorizeResult>,
    rest: Vec<SmartCategorizeItem>,
    /// 只有本机程序值得联网搜：书签的标题本来就是网页自己的名字
    searchable: bool,
}

/// 只有本机记忆这一步是不花钱的，它拦下来的条目连名字都不用出本机。
pub fn prepare(
    conn: &Connection,
    items: &[SmartCategorizeItem],
    item_type: &str,
    model_id: Option<i64>,
) -> AppResult<RunPlan> {
    let names = categories::get_categories(conn, item_type)?
        .into_iter()
        .map(|c| c.name)
        .collect::<Vec<_>>();
    // 记忆只给本机程序记：书签判错一次记一次反而更乱
    let (remembered, rest) = if item_type == "app" {
        by_memory(conn, items)
    } else {
        (Vec::new(), items.to_vec())
    };
    let target = if rest.is_empty() {
        None
    } else {
        Some(models::target_for(conn, model_id)?)
    };
    Ok(RunPlan {
        // 报给模型的候选名单：程序给现有分类，书签一律不给
        allowed: names_for_model(item_type, &names),
        target,
        remembered,
        rest,
        searchable: item_type == "app",
    })
}

/// 模型 → 联网查，结果按进来的顺序排回去
pub async fn execute(
    plan: RunPlan,
    items: &[SmartCategorizeItem],
) -> AppResult<Vec<SmartCategorizeResult>> {
    let mut rows = plan.remembered;
    if let Some(target) = &plan.target {
        let judged = classify(target, &plan.rest, &plan.allowed).await?;
        rows.extend(judged.clone());

        // 模型留空的（判不出、或给了空分类）才值得联网搜
        if plan.searchable {
            let blanks: Vec<SmartCategorizeItem> = plan
                .rest
                .iter()
                .filter(|item| {
                    judged
                        .iter()
                        .find(|row| row.id == item.id)
                        .map(|row| row.suggested_category.trim().is_empty())
                        .unwrap_or(true)
                })
                .cloned()
                .collect();
            if !blanks.is_empty() {
                rows.extend(fill_by_search(target, &blanks, &plan.allowed).await);
            }
        }
    }
    Ok(in_input_order(items, rows))
}

/// 按进来的顺序回，界面不至于乱跳；重复 id 只留第一条
fn in_input_order(
    items: &[SmartCategorizeItem],
    rows: Vec<SmartCategorizeResult>,
) -> Vec<SmartCategorizeResult> {
    let mut by_id: HashMap<i64, SmartCategorizeResult> = rows.into_iter().map(|r| (r.id, r)).collect();
    items.iter().filter_map(|i| by_id.remove(&i.id)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(include_str!("../../../db/schema.sql")).unwrap();
        c
    }

    fn item(id: i64, name: &str, target: Option<&str>) -> SmartCategorizeItem {
        SmartCategorizeItem {
            id,
            name: name.to_string(),
            target: target.map(str::to_string),
            url: None,
        }
    }

    fn add_category(c: &Connection, name: &str, kind: &str) {
        c.execute(
            "INSERT INTO categories (name, type) VALUES (?1, ?2)",
            rusqlite::params![name, kind],
        )
        .unwrap();
    }

    /// 本机记忆判得掉的条目根本不该走到模型那儿——库里一条模型配置都没有也不报错
    #[test]
    fn memory_alone_need_no_model_at_all() {
        let c = conn();
        add_category(&c, "开发工具", "app");
        c.execute(
            "INSERT INTO ai_memory (name, category, evidence, source, at) VALUES ('Loomy', '开发工具', '录屏', '搜索', 1)",
            [],
        )
        .unwrap();

        let plan = prepare(&c, &[item(1, "Loomy.lnk", None)], "app", None).unwrap();
        assert_eq!(plan.remembered.len(), 1, "记忆该直接判定");
        assert!(plan.rest.is_empty());
        assert!(plan.target.is_none(), "没剩下的就不该去取模型配置");
    }

    #[test]
    fn memory_hit_is_used_before_the_model_and_the_rest_still_waits() {
        let c = conn();
        add_category(&c, "开发工具", "app");
        c.execute(
            "INSERT INTO ai_models (id, name, base_url, model, key_enc) VALUES (1, '本地', 'http://127.0.0.1:1/v1', 'm', NULL)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO ai_memory (name, category, evidence, source, at) VALUES ('Loomy', '开发工具', '录屏', '搜索', 1)",
            [],
        )
        .unwrap();

        let plan = prepare(
            &c,
            &[item(10, "Loomy.lnk", Some("C:/loom.exe")), item(11, "新软件.exe", Some("C:/x.exe"))],
            "app",
            None,
        )
        .unwrap();
        assert_eq!(plan.remembered.len(), 1, "`Loomy.lnk` 该按去扩展名的键对上记忆");
        assert_eq!(plan.remembered[0].id, 10);
        assert_eq!(plan.rest.len(), 1, "没记着的才要问模型");
        assert!(plan.target.is_some(), "有剩下的就得取模型配置");
    }

    #[test]
    fn existing_app_categories_are_offered_and_bookmarks_get_none() {
        let c = conn();
        add_category(&c, "开发工具", "app");
        add_category(&c, "文档", "app");
        add_category(&c, "技术社区", "bookmark");
        c.execute(
            "INSERT INTO ai_models (id, name, base_url, model, key_enc) VALUES (1, '本地', 'http://127.0.0.1:1/v1', 'm', NULL)",
            [],
        )
        .unwrap();

        let plan = prepare(&c, &[item(1, "Loomy.exe", None)], "app", None).unwrap();
        assert_eq!(plan.allowed, vec!["开发工具".to_string(), "文档".to_string()]);

        // 书签这边压根不给候选名单，也不查记忆
        let bm = prepare(&c, &[item(2, "掘金", None)], "bookmark", None).unwrap();
        assert!(bm.allowed.is_empty());
        assert!(bm.remembered.is_empty());
    }

    /// 分类规则插不进这一脚：它只决定桌面文件挪去哪儿，应用页照常把「文档」这类
    /// 已有分类报给模型
    #[test]
    fn desktop_rules_do_not_steer_app_categorizing() {
        let c = conn();
        add_category(&c, "文档", "app");
        add_category(&c, "开发工具", "app");
        c.execute(
            "INSERT INTO desktop_rules (category, extensions) VALUES ('文档', '.pdf')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO ai_models (id, name, base_url, model, key_enc) VALUES (1, '本地', 'http://127.0.0.1:1/v1', 'm', NULL)",
            [],
        )
        .unwrap();

        let plan = prepare(&c, &[item(1, "报告.pdf", Some("C:/desk/报告.pdf"))], "app", None).unwrap();
        assert!(plan.remembered.is_empty(), "规则不该替程序判分类");
        assert_eq!(plan.rest.len(), 1, "有扩展名也照样要问模型");
        assert_eq!(plan.allowed, vec!["文档".to_string(), "开发工具".to_string()]);
    }

    #[test]
    fn leftovers_without_any_model_config_explain_what_to_do() {
        let c = conn();
        match prepare(&c, &[item(1, "陌生的.exe", None)], "app", None) {
            Ok(_) => panic!("库里一条模型都没配时该报错，不该悄悄跳过剩下的条目"),
            Err(e) => assert!(e.to_string().contains("模型"), "报错要说清去哪儿配，got {e}"),
        }
    }

    #[test]
    fn results_come_back_in_the_order_they_came_in() {
        let rows = vec![
            SmartCategorizeResult {
                id: 3,
                name: String::new(),
                suggested_category: "文档".into(),
                reason: String::new(),
            },
            SmartCategorizeResult {
                id: 1,
                name: String::new(),
                suggested_category: "工具".into(),
                reason: String::new(),
            },
        ];
        let items = [item(1, "a", None), item(2, "b", None), item(3, "c", None)];
        let ordered = in_input_order(&items, rows);
        assert_eq!(
            ordered.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![1, 3],
            "按进来的顺序排，没判出来的那条（id=2）不该冒出来"
        );
    }
}
