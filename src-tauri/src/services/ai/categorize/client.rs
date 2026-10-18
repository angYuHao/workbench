//! 一发请求和它的周边：额度、裸 chat、调用日志、掐文本。
//!
//! 额度不是按「正文要多长」给的，是按「模型要产出多少」给的，详见 reply_budget 上面那段。


use crate::error::AppResult;
use crate::services::ai::models::{note, ChatTarget};


/// 整份一次请求，不分批。
///
/// 额度不是按「正文要多长」给的，是按「模型要产出多少」给的：上游那个 glm 即使
/// 收到 `thinking:{type:disabled}` 也照样先想一遍（日志实测 6 条一批要 3.5~13 秒，
/// 正文才 150 token），思考和正文挤在同一个 max_tokens 里。实测平摊每条 ~130 token
/// 思考 + ~30 token 正文，所以给 160；上限 8000 是怕撞上游自己的输出封顶。
/// 给少了就是这个样子：finish_reason=Length，正文一个字都没有。
pub(crate) const TOKENS_PER_ITEM: u64 = 160;
pub(crate) const MIN_REPLY_TOKENS: u64 = 2000;
pub(crate) const MAX_REPLY_TOKENS: u64 = 8000;
/// 路径和网址只用来帮模型认名字，长了就是白烧 token
pub(crate) const HINT_CHARS: usize = 120;

pub(crate) fn reply_budget(items: usize) -> u64 {
    (items as u64 * TOKENS_PER_ITEM).clamp(MIN_REPLY_TOKENS, MAX_REPLY_TOKENS)
}

/// 一发请求 + 读正文。`tag` 是写进调用日志的那句话，`count` 用来给回复额度
/// 请求正文要不要进日志：书签分析的正文里每条都带网址，那些地址不该落到 ai.log 里过夜
/// （调用日志会记 512KB 内的历史，旧日志里漏过敏感文件名，用户主动清过一轮）。
pub(crate) fn log_request(prompt: &str, redact: bool) -> String {
    if redact {
        return "（请求正文含书签地址，按口径不写进日志）".to_string();
    }
    prompt.to_string()
}

/// 一发请求，回正文。`tag` 是写进调用日志的那句话，`count` 用来给回复额度。
/// 书签分析也要用这一条，所以留在 crate 内可见。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn chat(
    target: &ChatTarget,
    system: &str,
    prompt: &str,
    tag: &str,
    count: usize,
    redact: bool,
) -> AppResult<String> {
    let started = std::time::Instant::now();
    let logged = log_request(prompt, redact);

    // 思考关掉是没办法的办法：这个模型能把整个额度花在 reasoning_content 上，
    // 正文一个字都没有。走 chat 线就只能用厂商自己的 thinking 字段。
    let outcome = target
        .builder(system, reply_budget(count))
        .additional_params(serde_json::json!({ "thinking": { "type": "disabled" } }))
        .build()
        .prompt(prompt.to_string())
        .await;

    match outcome {
        Ok(response) => {
            let text = response.output();
            // 把两种 token 分开记：这个上游的 max_tokens 是思考和正文共用一个池子，
            // 下次被截断时日志能直接说出是思考吃光的还是正文写不完的
            let used = format!(
                "正文 {} token，思考 {} token",
                response.usage.output_tokens.unwrap_or(0),
                response.usage.reasoning_tokens.unwrap_or(0)
            );
            note(target, tag, &logged, started, true, &format!("{used} → {}", head(text.trim(), 300)));
            Ok(text)
        }
        Err(e) => {
            let why = e.to_string();
            note(target, tag, &logged, started, false, &why);
            Err(crate::error::AppError::msg(why))
        }
    }
}

pub(crate) fn head(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut cut: String = s.chars().take(max).collect();
    cut.push('…');
    cut
}
