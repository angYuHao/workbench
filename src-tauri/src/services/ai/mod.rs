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
