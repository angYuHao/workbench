//! 自定义大模型（OpenAI 兼容接口）的存储与调用。
//! 接口地址、模型名、密钥都由用户自己填，不预设任何厂商；
//! 密钥用当前 Windows 用户的 DPAPI 加密后存库，对外只说「有没有存」。
//! 传输和响应解析交给 rig（OpenAI 兼容 provider），不再自己写 HTTP 客户端。
//!
//! - [`store`]   库里的模型行
//! - [`cache`]   拉回来的远端模型列表，缓存在 settings 那张 KV
//! - [`target`]  拿一个能发请求的目标（地址 + 密钥 + 模型名）
//! - [`call`]    发一次请求：组装、错误归一化、记调用日志
//!
//! 这里 `pub use` 摊平，所以外面照旧写 `crate::services::ai::models::probe(...)`，
//! 不用关心它落在哪个子文件里。

mod cache;
mod store;
mod target;

pub(crate) use cache::remember_models;
pub(crate) use store::{delete_model, get_models, save_model};
pub(crate) use target::{call_target, list_target, target_for, ChatTarget};

mod call;

pub(crate) use call::{fetch_models, note, probe, ProbeResult};

#[cfg(test)]
mod tests;
