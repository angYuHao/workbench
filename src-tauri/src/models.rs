//! 跨层共用的数据结构，按功能域分文件。
//! 这里 `pub use` 汇总，所以全仓库仍然写 `crate::models::AppEntry`，
//! 新增一个域只要多一行 `mod` + 一行 `pub use`，不用改任何调用点。
//!
//! 约定：字段一律 `#[serde(rename_all = "camelCase")]`，因为前端 `src/types/` 是对面；
//! 两边同时改字段名会打断 JSON 协议，所以拆文件时**只搬不改**。

mod ai;
mod bookmarks;
mod common;
mod desktop;
mod files;
mod totp;

pub use ai::{
    AiModel, AiModelInput, ApplyCategorizeUpdate, LogView, SmartCategorizeItem,
    SmartCategorizeResult,
};
pub use bookmarks::{Bookmark, ImportResult};
pub use common::Category;
pub use desktop::{AppEntry, DesktopItem, DesktopRule, OrganizeLog, OrganizePlan};
pub use files::{FileNode, FileRecord, ReadTextResult};
pub use totp::{TotpAccount, TotpAccountInput};
