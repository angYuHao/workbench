//! 应用在这台机器上的数据目录：`%APPDATA%\workbench`。
//!
//! 库文件、图标缓存、AI 日志都住在它下面，所以目录名和「取不到 APPDATA」这句话只写一遍。

use std::path::PathBuf;

use crate::error::{AppError, AppResult};

pub(crate) fn app_dir() -> AppResult<PathBuf> {
    let data_dir = dirs::data_dir().ok_or_else(|| AppError::msg("无法获取 APPDATA 目录"))?;
    Ok(data_dir.join("workbench"))
}
