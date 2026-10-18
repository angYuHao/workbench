//! 一发请求：传输和响应解析交给 rig，这里只管组装、错误归一化和调用日志。

use rig::error::ProviderError;
use serde::Serialize;

use crate::error::{AppError, AppResult};

use crate::services::ai::log;
use super::target::ChatTarget;

/// 探测结果：给界面直接显示的一句话
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub ok: bool,
    pub status: u16,
    pub message: String,
}
/// 记一条调用日志。rig 不交出原始 HTTP 状态，成功按 200、失败按 0，
/// 失败原文（里面通常带状态码和厂商自己的话）整条进日志。
fn note_at(tag: &str, url: &str, model: &str, request: &str, started: std::time::Instant, ok: bool, text: &str) {
    log::record(&log::Call {
        tag,
        url,
        model,
        status: if ok { 200 } else { 0 },
        elapsed_ms: started.elapsed().as_millis(),
        ok,
        request,
        response: text,
    });
}

/// 补全类调用（分类、测试）共用的记法：地址和模型名都从 target 拿
pub(crate) fn note(
    target: &ChatTarget,
    tag: &str,
    request: &str,
    started: std::time::Instant,
    ok: bool,
    text: &str,
) {
    note_at(tag, &target.completion_url(), &target.model, request, started, ok, text);
}

/// rig 的错误已经是人话，这里只补一句「该怎么办」
fn advice(err: &ProviderError) -> String {
    match err {
        ProviderError::InvalidAuthentication(_) => "密钥被接口拒绝（401/403）".to_string(),
        ProviderError::Truncated => "回复被 max_tokens 截断：正文还没写出来额度就用完了".to_string(),
        ProviderError::Url(_) | ProviderError::Http(_) => format!("连不上这个地址：{err}"),
        other => other.to_string(),
    }
}
/// 拉接口给的模型 id：翻页和形状都交给 rig，不再自己拆 JSON
pub async fn fetch_models(target: &ChatTarget) -> AppResult<Vec<String>> {
    let url = format!("{}/models", target.base.trim_end_matches('/'));
    let started = std::time::Instant::now();
    match target.client().list_models().await {
        Ok(list) => {
            let ids: Vec<String> = list.data.into_iter().map(|m| m.id).collect();
            note_at("模型列表", &url, &target.model, "", started, true, &ids.join(", "));
            Ok(ids)
        }
        Err(e) => {
            let msg = advice(&e);
            note_at("模型列表", &url, &target.model, "", started, false, &msg);
            Err(AppError::msg(msg))
        }
    }
}

/// 测密钥能不能真的调用模型：发一条最短的补全，花几个 token，
/// 但鉴权、模型名、额度是不是都对，一次就见分晓。
/// 失败时再验一次鉴权，好把「密钥不行」和「模型名不对」分开说。
pub async fn probe(target: &ChatTarget) -> AppResult<ProbeResult> {
    let started = std::time::Instant::now();
    let result = target.agent("收到测试请求就只回一个字：好", 16).prompt("ping").await;

    let outcome = match result {
        Ok(response) => {
            let out = response.usage.output_tokens.unwrap_or(0);
            let thought = response.usage.reasoning_tokens.unwrap_or(0);
            let extra = if thought > 0 {
                format!("，其中思考占了 {thought} token")
            } else {
                String::new()
            };
            ProbeResult {
                ok: true,
                status: 200,
                message: format!(
                    "调用成功，{} ms，输出了 {out} token{extra}",
                    started.elapsed().as_millis()
                ),
            }
        }
        Err(e) => {
            let text = e.to_string();
            let message = match target.client().verify().await {
                Err(ProviderError::InvalidAuthentication(_)) => "密钥被接口拒绝（401/403）".to_string(),
                _ => text,
            };
            ProbeResult {
                ok: false,
                status: 0,
                message,
            }
        }
    };

    note(target, "测试调用", "ping", started, outcome.ok, &outcome.message);
    Ok(outcome)
}

