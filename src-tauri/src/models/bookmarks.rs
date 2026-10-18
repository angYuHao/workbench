use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub description: Option<String>,
    pub category_id: Option<i64>,
    pub source: String,
    pub created_at: Option<String>,
    /// 双击打开过几次，读列表时从 usage_stats 带出来
    #[serde(default)]
    pub use_count: i64,
}

/// 从浏览器导入收藏夹的结果统计
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub total: usize,
    pub imported: usize,
    pub skipped: usize,
}
