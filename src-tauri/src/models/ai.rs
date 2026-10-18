use serde::{Deserialize, Serialize};

/// 送进智能分类的一个条目：`target` 是本机路径，只有程序类才有，且不会发给模型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartCategorizeItem {
    pub id: i64,
    pub name: String,
    pub target: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartCategorizeResult {
    pub id: i64,
    pub name: String,
    pub suggested_category: String,
    /// 模型给的一句话理由；没答这条时是提示人工去选
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyCategorizeUpdate {
    pub id: i64,
    pub category_name: String,
    /// 窗口里那条判断的理由，应用时顺手带给记忆当依据；手打的分类可以不给
    #[serde(default)]
    pub reason: Option<String>,
}

/// 自定义大模型（OpenAI 兼容）。对外只说有没有存 key，不回传密钥本身。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiModel {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub has_key: bool,
    pub sort: i64,
    /// 上次点「获取模型」从接口拉回来的列表，存在本机：下次编辑直接用，不再打网络
    #[serde(default)]
    pub remote_models: Vec<String>,
    /// 那次获取的 unix 秒，界面要如实标出列表有多旧
    #[serde(default)]
    pub remote_at: Option<i64>,
}

/// 新增/编辑模型的入参。api_key 为空表示保持原密钥不变（编辑时不必重填）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiModelInput {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub sort: Option<i64>,
}

/// 设置里看日志用的：只回最近几行，但把文件路径和总行数一起给，界面能说清「还有更多」。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogView {
    pub path: String,
    pub lines: Vec<String>,
    pub total: usize,
}
