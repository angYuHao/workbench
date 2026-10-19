//! 提示词：什么能出去、什么不能出去，全写在这一个文件里。
//! `target`（本机路径）和网址只在书签那条路上出去，程序类只带名字。

use crate::models::SmartCategorizeItem;

use super::client::{head, HINT_CHARS};


/// 一次判断的规矩写死在这儿：复用优先、分类数要少、只看用途。
pub(crate) fn system_prompt() -> String {
    String::from(
        "你在给「个人工作台」里的桌面程序和书签网址分门别类。\n\
         每个条目给且只给一个分类名。优先复用「已有分类」里的名字，复用时必须一字不差；\
         确实没有合适的才新建，新名字用 2 到 6 个汉字的通用叫法，别起带厂商名的怪名。\n\
         分类总数越少越好：能并进已有分类的一律并进去，别按厂牌、语言、细分用途再拆新名；\
         要新建也只建一两个能兜住多个条目的大类，不要造只装得下一个条目的分类。\n\
         按用途判断，不要机械地看扩展名或域名；名字看不出来的条目给最接近的用途分类。\n\
         不要解释、不要思考过程，直接输出 JSON 数组本身，形如 \
         [{\"id\":1,\"category\":\"开发工具\",\"reason\":\"IDE\"}]，reason 不超过 12 个字。",
    )
}

/// 只给名字和网址：本地路径（`target`）不进提示词，模型不需要知道程序装在哪个盘。
pub(crate) fn user_prompt(items: &[SmartCategorizeItem], category_names: &[String]) -> String {
    let cats = if category_names.is_empty() {
        "（不给现成分类，名字你自己定；但整批要收到少数几个大类里，8 个封顶，宁宽勿细，不许一个条目开一个分类）".to_string()
    } else {
        category_names.join("、")
    };
    let list: Vec<_> = items
        .iter()
        .map(|item| {
            let mut obj = serde_json::Map::new();
            obj.insert("id".into(), item.id.into());
            obj.insert("name".into(), serde_json::Value::String(item.name.clone()));
            if let Some(url) = item.url.as_deref().filter(|u| !u.is_empty()) {
                obj.insert("url".into(), serde_json::Value::String(head(url, HINT_CHARS)));
            }
            serde_json::Value::Object(obj)
        })
        .collect();
    format!(
        "已有分类：{cats}\n待分类：{}",
        serde_json::to_string(&list).unwrap_or_default()
    )
}

/// 已有分类里哪些可以报给模型复用。
/// 应用：把现有分类名报上去，让它优先复用。
/// 书签：什么都不报 —— 用户 2026-10-15 明确要「不要将原有的分类加入提示词，让 AI 自己分类」，
/// 并让它在整批里尽量收着分类数。
pub fn names_for_model(item_type: &str, category_names: &[String]) -> Vec<String> {
    if item_type == "app" {
        category_names.to_vec()
    } else {
        Vec::new()
    }
}
