use serde::{Deserialize, Serialize};

/// 已入库的文件/文件夹记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub alias: Option<String>,
    pub description: Option<String>,
    pub is_dir: bool,
    pub managed_root: Option<String>,
    pub category_id: Option<i64>,
    pub created_at: Option<String>,
}

/// 列目录时的一项：来自磁盘，别名和说明从库里补
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub alias: Option<String>,
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileNode>>,
}

/// 文本预览：过大时只回开头，`truncated` 告诉界面后面还有
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadTextResult {
    pub content: String,
    pub truncated: bool,
}
