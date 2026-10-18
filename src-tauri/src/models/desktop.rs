use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    pub id: i64,
    pub name: String,
    pub alias: Option<String>,
    pub description: Option<String>,
    pub target: String,
    pub args: Option<String>,
    pub icon_path: Option<String>,
    pub category_id: Option<i64>,
    pub source: String,
    pub created_at: Option<String>,
    /// 双击启动过几次，读列表时从 usage_stats 带出来
    #[serde(default)]
    pub use_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopRule {
    pub id: i64,
    pub category: String,
    pub keywords: Option<String>,
    pub extensions: Option<String>,
    pub enabled: bool,
    pub sort: i64,
}

/// 扫描桌面时看到的一个待整理项（还没入库）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopItem {
    pub name: String,
    pub path: String,
    pub target: String,
    pub is_lnk: bool,
    pub category: String,
    pub icon_path: Option<String>,
    /// 启动器里已经有这一条了（按 .lnk 自身和解析出的目标两种写法认）
    pub registered: bool,
}

/// 用户确认整理时提交的一条计划
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePlan {
    pub path: String,
    pub category: String,
    pub icon_path: Option<String>,
    /// 只登记进启动器、不搬桌面文件
    pub skip_move: Option<bool>,
}

/// 整理流水，用于「恢复整理」把文件搬回原位
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeLog {
    pub id: i64,
    pub file_name: Option<String>,
    pub from_path: Option<String>,
    pub to_path: Option<String>,
    pub organized_at: Option<String>,
    pub reverted: bool,
}
