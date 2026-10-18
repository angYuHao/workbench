//! 模型回的都是文本：剥代码围栏、掐第一段 JSON 数组、认一行一条的 `One`。
//! 截断的回复也要能救出前半截，那是唯一还能拿到结果的时机。

use serde::Deserialize;



#[derive(Debug, Clone, Deserialize)]
pub(crate) struct One {
    pub id: i64,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub reason: String,
}

/// 模型回的都是文本：剥掉代码围栏、掐出第一段 JSON 数组；
/// 认不出数组再试 `{"items":[…]}` 这种包一层的写法。
/// 书签分析也要用，所以认到 Value 为止，由各条链路自己取字段。
pub(crate) fn parse_json_array(text: &str) -> Option<Vec<serde_json::Value>> {
    let body = match (text.find("```"), text.rfind("```")) {
        (Some(open), Some(close)) if close > open + 3 => &text[open + 3..close],
        _ => text,
    };
    let body = body.trim().trim_start_matches("json").trim();

    if let Some(start) = body.find('[') {
        let array = match body[start..].rfind(']') {
            Some(close) => &body[start..start + close + 1],
            None => &body[start..],
        };
        if let Ok(rows) = serde_json::from_str::<Vec<serde_json::Value>>(array) {
            return Some(rows);
        }
        // 整份一次提交后回复可能被额度截断：留完整的开头几条，比整判失败划算
        if let Some(cut) = array.rfind('}') {
            if let Ok(rows) = serde_json::from_str::<Vec<serde_json::Value>>(&format!("{}]", &array[..=cut])) {
                return Some(rows);
            }
        }
    }
    if let (Some(open), Some(close)) = (body.find('{'), body.rfind('}')) {
        if close > open {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&body[open..=close]) {
                let candidate = value.get("items").cloned().or_else(|| value.as_array().cloned().map(serde_json::Value::Array));
                if let Some(serde_json::Value::Array(rows)) = candidate {
                    return Some(rows);
                }
            }
        }
    }
    None
}

pub(crate) fn parse_rows(text: &str) -> Option<Vec<One>> {
    // 认不出的那一条单独丢掉就是「模型没给这条」，不该连累同批别的条目
    Some(
        parse_json_array(text)?
            .iter()
            .filter_map(|v| serde_json::from_value::<One>(v.clone()).ok())
            .collect(),
    )
}
