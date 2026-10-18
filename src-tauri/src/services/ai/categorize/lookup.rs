//! 模型没判出来的最后一步：联网查 → 摘要里关键词命中 → 还对不上就带着摘要再问一次模型。
//! 这里不写数据库：判出来的东西要等用户在窗口里确认应用了才算数。

use crate::models::{SmartCategorizeItem, SmartCategorizeResult};

use super::client::head;
use super::preset::stem;
use super::{ask_model, blank, merge};
use crate::services::ai::models::ChatTarget;
use super::prompt::{system_prompt, user_prompt};
use crate::services::ai::search;


/// 一次运行最多联网查几个：每个都要打一次外网请求（8 秒超时），全查太慢也太费
pub(crate) const MAX_SEARCHES: usize = 12;

/// 摘要里能对上哪个已有分类：把分类名拆成两字片段数命中。
/// 名字有两个以上片段（「开发工具」= 开发/发工/工具）要命中两处才算对上，只中一处多半是巧合；
/// 只有一个片段（「办公」「AI」）或就一个字，出现即算。
pub fn pick_by_keyword(text: &str, candidates: &[String]) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for name in candidates {
        let chars: Vec<char> = name.chars().collect();
        if chars.is_empty() {
            continue;
        }
        let grams: Vec<String> = chars.windows(2).map(|w| w.iter().collect()).collect();
        let hits = if grams.is_empty() {
            usize::from(text.contains(name.as_str()))
        } else {
            grams.iter().filter(|g| text.contains(g.as_str())).count()
        };
        let enough = if grams.len() <= 1 { hits == 1 } else { hits >= 2 };
        if enough && best.as_ref().map(|(top, _)| hits > *top).unwrap_or(true) {
            best = Some((hits, name.clone()));
        }
    }
    best.map(|(_, name)| name)
}

/// 模型没判出来的：联网查 → 摘要里关键词命中 → 还对不上就带着摘要再问一次模型。
/// 这里不写数据库：判出来的东西要等你在窗口里确认应用了才算数（写记忆归
/// `apply_categorize` 那一步），不然模型随口一个猜测就把这软件永久定死了。
/// 规则占用的分类名不算命中。
pub async fn fill_by_search(
    target: &ChatTarget,
    blanks: &[SmartCategorizeItem],
    // allowed：可以复用的已有分类（已经排掉规则占用的；书签传空）
    allowed: &[String],
    reserved: &[String],
) -> Vec<SmartCategorizeResult> {
    let mut out = Vec::new();
    let mut ask: Vec<(SmartCategorizeItem, String)> = Vec::new();

    for item in blanks.iter().take(MAX_SEARCHES) {
        let key = stem(&item.name);
        let snippets = search::search(&key).await.unwrap_or_default();
        if snippets.is_empty() {
            out.push(blank(item, "联网搜不到东西，请手动选"));
            continue;
        }
        let joined: String = snippets
            .iter()
            .map(|s| format!("{} {} {}", s.title, s.site, s.text))
            .collect::<Vec<_>>()
            .join(" ");
        match pick_by_keyword(&joined, allowed) {
            Some(category) if !reserved.iter().any(|r| r.trim() == category) => {
                let evidence = snippets[0].evidence();
                out.push(SmartCategorizeResult {
                    id: item.id,
                    name: item.name.clone(),
                    reason: format!("搜到「{category}」：{evidence}"),
                    suggested_category: category,
                });
            }
            _ => ask.push((item.clone(), render_evidence(&snippets))),
        }
    }
    for item in blanks.iter().skip(MAX_SEARCHES) {
        out.push(blank(
            item,
            &format!("一次最多联网查 {MAX_SEARCHES} 个，这条没轮到，请手动选"),
        ));
    }

    if ask.is_empty() {
        return out;
    }
    // 剩下的一起问，别再一条条发请求
    let items: Vec<SmartCategorizeItem> = ask.iter().map(|(item, _)| item.clone()).collect();
    let prompt = evidence_prompt(&ask, allowed);
    let tag = format!("AI分类带资料 {}条", items.len());
    let rows = match ask_model(target, &system_prompt(reserved), &prompt, &tag, items.len()).await {
        Ok(rows) => rows,
        Err(e) => {
            let why = format!("带着资料问模型也失败：{}", head(&e.to_string(), 80));
            out.extend(items.iter().map(|i| blank(i, &why)));
            return out;
        }
    };
    out.extend(merge(&items, &rows, reserved));
    out
}

pub(crate) fn render_evidence(snippets: &[search::Snippet]) -> String {
    snippets
        .iter()
        .map(|s| format!("- {}（{}）{}", s.title, s.site, s.text))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn evidence_prompt(rows: &[(SmartCategorizeItem, String)], category_names: &[String]) -> String {
    let items: Vec<SmartCategorizeItem> = rows.iter().map(|(item, _)| item.clone()).collect();
    let mut text = format!(
        "{}\n下面附上每个条目在网上查到的资料，按资料里写的用途判断它是什么，别看名字猜。",
        user_prompt(&items, category_names)
    );
    for (item, evidence) in rows {
        text.push_str(&format!("\n\n「{}」的资料：\n{}", item.name, evidence));
    }
    text
}
