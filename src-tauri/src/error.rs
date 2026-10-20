//! 后端唯一的错误类型：库、IO、JSON 三类由 `?` 自动带上，第四类 `Msg` 是给用户的中文原话。
//!
//! 它 `impl Serialize`，所以命令层交给前端的永远是一个字符串（错误文案直接可读），
//! `AppResult<T>` 就是 `Result<T, AppError>`。
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("数据库错误: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Msg(String),
}

impl AppError {
    pub fn msg<T: Into<String>>(s: T) -> Self {
        AppError::Msg(s.into())
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;