//! AI 域：所有要花钱、要打外网、会不可预期失败的东西都集中在这里。
//!
//! - [`categorize`] 智能分类的四步流水线：分类规则 → 本机记忆 → 模型 → 联网查
//! - [`log`]        调用日志（`%APPDATA%\workbench\ai.log`，按行数自己滚）
//! - [`memory`]     本机记忆：查过的软件记一次，下次不再问模型也不再搜
//! - [`models`]     自定义 OpenAI 兼容接口的配置、DPAPI 密钥和远端模型列表缓存
//! - [`search`]     抓 Bing 搜索结果（唯一能出去的外网口子之一）
//! - [`pageinfo`]   抓网页自身的标题和简介，带 7 天本机缓存
//!
//! 边界：密钥只在本机 DPAPI 下解开，任何接口不回传、任何日志不落盘；
//! 书签分析的 prompt 连网址都不进调用日志（见 [`categorize`] 里 `chat` 的 `redact`）。

pub(crate) mod categorize;
pub(crate) mod log;
pub(crate) mod memory;
pub(crate) mod models;
pub(crate) mod pageinfo;
pub(crate) mod search;

use std::time::Duration;

/// 出网的请求都用同一个 UA：Bing 对没有 UA 的请求只发精简版结果页，抓不到结果块。
/// 两处调用（搜索、抓网页）以前各写一份，改一处忘另一处就悄悄失败在对方站点上。
pub(crate) const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

/// 起一个带超时的客户端。「起不动」怎么处理两处不一样（搜索直接报错、抓网页退回默认客户端），
/// 所以这里只交回 `Result`，不替调用方决定
pub(crate) fn http_client(timeout: Duration) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .build()
}

/// 按**字符**数收进上限，超了补一个省略号。
/// 中文一个字三个字节，按字节切会落在字中间；日志、错误文案、给模型的摘要都从这儿走。
pub(crate) fn shorten(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}
