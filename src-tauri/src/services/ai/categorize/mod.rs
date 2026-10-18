//! 智能分类：把名字交给用户自己配的大模型去分。
//! 关键词表和域名表已经删掉（旧文件在仓库根 `.removed-smart-categorize/`），
//! 手写 HTTP 客户端和 JSON 容错解析也一并退役（`.removed-winhttp/`）——
//! 传输、错误归一化交给 rig，提示词和落库前的确认归我们。
//!
//! 四步顺序是定死的，别调：分类规则 → 本机记忆 → 模型 → 联网查。
//! 前两步不花钱也不出网，后两步才花钱、才出网。
//!
//! - [`preset`]   规则和本机记忆：不花钱的判断先做完
//! - [`prompt`]   提示词：什么能出去、什么不能出去
//! - [`client`]   额度、一发请求、调用日志
//! - [`parse`]    回复容错解析
//! - [`lookup`]   判不出来的最后一步：联网查，再不行带着摘要问一次
//! - [`run`]      整条流水线的两半：`prepare` 在本机判完，`execute` 才出网
//! - [`apply`]    落库：缺失的分类先建、条目改分类，顺手记进本机记忆
//!
//! 本文件只留一步的编排：`classify` 把上面几个串起来，`merge` 负责拦下模型越权用的规则分类名。

use crate::error::AppResult;
use crate::models::{SmartCategorizeItem, SmartCategorizeResult};
use crate::services::ai::models::ChatTarget;

pub(crate) use apply::apply;
pub(crate) use client::{chat, head};
pub(crate) use lookup::fill_by_search;
pub(crate) use parse::parse_json_array;
pub(crate) use preset::{by_memory, by_rules, stem};
pub(crate) use prompt::names_for_model;
pub(crate) use run::{execute, prepare};

mod apply;
mod client;
mod lookup;
mod parse;
mod preset;
mod prompt;
mod run;

use parse::{parse_rows, One};
use prompt::{system_prompt, user_prompt};


/// 只用标准 chat 字段发请求：实测表明 rig 的 `prompt_typed`／`reasoning` 会把请求
/// 挪到 Responses 线上，正文里多出 `include:["reasoning.encrypted_content"]`，
/// CodeArts 上游直接判 `malformed_json.400`。所以结构化输出改由我们读正文。
pub async fn classify(
    target: &ChatTarget,
    items: &[SmartCategorizeItem],
    category_names: &[String],
    reserved: &[String],
) -> AppResult<Vec<SmartCategorizeResult>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let prompt = user_prompt(items, category_names);
    let rows = ask_model(target, &system_prompt(reserved), &prompt, &format!("AI分类 {}条", items.len()), items.len()).await?;
    Ok(merge(items, &rows, reserved))
}

pub(crate) async fn ask_model(
    target: &ChatTarget,
    system: &str,
    prompt: &str,
    tag: &str,
    count: usize,
) -> AppResult<Vec<One>> {
    // 分类请求的正文只有名字和网址（本地路径已经不进提示词），可以整条进日志
    let text = chat(target, system, prompt, tag, count, false).await?;
    parse_rows(&text).ok_or_else(|| {
        crate::error::AppError::msg(format!("回复里读不出分类数组：{}", head(text.trim(), 80)))
    })
}

pub(crate) fn blank(item: &SmartCategorizeItem, reason: &str) -> SmartCategorizeResult {
    SmartCategorizeResult {
        id: item.id,
        name: item.name.clone(),
        suggested_category: String::new(),
        reason: reason.to_string(),
    }
}

pub(crate) fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

pub(crate) fn merge(items: &[SmartCategorizeItem], rows: &[One], reserved: &[String]) -> Vec<SmartCategorizeResult> {
    items
        .iter()
        .map(|item| {
            let blank = |reason: String| SmartCategorizeResult {
                id: item.id,
                name: item.name.clone(),
                suggested_category: String::new(),
                reason,
            };
            match rows.iter().find(|row| row.id == item.id) {
                // 提示词里禁过一遍，这里还得拦：对模型说话只是劝说，这一步才是保证
                Some(row) if reserved.iter().any(|r| same(r, &row.category)) => blank(format!(
                    "模型给的「{}」归本地规则管，已拦下",
                    row.category.trim()
                )),
                Some(row) if !row.category.trim().is_empty() => SmartCategorizeResult {
                    id: item.id,
                    name: item.name.clone(),
                    suggested_category: row.category.trim().to_string(),
                    reason: row.reason.clone(),
                },
                Some(_) => blank("模型给了空分类，请手动选".to_string()),
                None => blank("模型没给这条，请手动选".to_string()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
