use serde::{Deserialize, Serialize};

/// 2FA 账户（对外不暴露密钥）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpAccount {
    pub id: i64,
    pub issuer: String,
    pub account: String,
    pub algorithm: String,
    pub digits: i64,
    pub period: i64,
    pub sort: i64,
    pub created_at: Option<String>,
}

/// 新增/编辑 2FA 账户的入参，secret 为 base32 原文
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpAccountInput {
    pub id: i64,
    pub issuer: String,
    pub account: String,
    pub secret: String,
    #[serde(default)]
    pub algorithm: Option<String>,
    #[serde(default)]
    pub digits: Option<i64>,
    #[serde(default)]
    pub period: Option<i64>,
}
