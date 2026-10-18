use serde::{Deserialize, Serialize};

/// 分类表：桌面程序、网址、文件三个模块共用一张表，靠 `type` 区分。
/// 这里不校验 `type` 的取值，前端 `ModuleType` 才是约束点。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub r#type: String,
    pub parent_id: Option<i64>,
    pub sort: i64,
    pub icon: Option<String>,
}
