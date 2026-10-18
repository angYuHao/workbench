//! 怎么拿到一个「能发请求」的目标：地址、密钥（可能没填）、模型名。
//! 先把三样取出来再发请求，数据库锁就能在网络等待之前放掉。

use rig::providers::openai::{OpenAI, OpenAIConfig, Route};
use rig::{Agent, AgentBuilder};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{AppError, AppResult};
use crate::win::dpapi;


/// 打一次调用需要的三样：地址、密钥（可能没填）、模型名。
/// 先把它们取出来再发请求，数据库锁就能在网络等待之前放掉。
pub struct ChatTarget {
    pub base: String,
    pub key: Option<String>,
    pub model: String,
}

/// 密钥不进 Debug：日志或断言失败信息里带出来等于泄露
impl std::fmt::Debug for ChatTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ChatTarget {{ base: {:?}, model: {:?}, key: {} }}",
            self.base,
            self.model,
            if self.key.is_some() { "<redacted>" } else { "None" }
        )
    }
}

impl ChatTarget {
    /// rig 的 OpenAI 兼容 provider：base_url 换成用户填的地址，密钥只活在这一层。
    /// 必须钉住 `Route::Chat`——rig 对 OpenAI 默认走 `/responses`，
    /// 而我们承诺的是「任何 OpenAI 兼容接口」，各家都有的只有 `/chat/completions`。
    pub(crate) fn client(&self) -> OpenAI {
        let mut config =
            OpenAIConfig::new(self.key.clone().unwrap_or_default()).with_route(Route::Chat);
        config.base_url = self.base.clone();
        config.client()
    }

    /// 一次只干一件事的 agent。温度锁 0，不然同一批应用每次给出不同分类。
    pub fn agent(&self, preamble: &str, max_tokens: u64) -> Agent {
        self.builder(preamble, max_tokens).build()
    }

    /// 还要往正文里掺厂商特定字段的调用方从这儿拿 builder。
    /// 注意别用 `reasoning(..)` 或 `prompt_typed`：那会把请求挪到 Responses 线，
    /// 实测 CodeArts 上游对多出 `include` 的正文回 `malformed_json.400`。
    pub fn builder(&self, preamble: &str, max_tokens: u64) -> AgentBuilder {
        AgentBuilder::new(self.client().completion(self.model.clone()))
            .preamble(preamble)
            .max_tokens(max_tokens)
            .temperature(0.0)
    }

    pub(crate) fn completion_url(&self) -> String {
        format!("{}/chat/completions", self.base.trim_end_matches('/'))
    }
}
/// 没指定用哪个模型时的默认值：列表里的第一个（按 sort, id）
pub fn first_model_id(conn: &Connection) -> AppResult<i64> {
    conn.query_row(
        "SELECT id FROM ai_models ORDER BY sort, id LIMIT 1",
        [],
        |r| r.get::<_, i64>(0),
    )
    .optional()?
    .ok_or_else(|| AppError::msg("要先在设置 → 模型里添加一个大模型"))
}

/// agent 只需要编号：其余全从库里取。编号为 0／没给／已被删除，都退回第一个模型，
/// 这样「上次用的模型」这个偏好失效时也不会把整个功能卡死。
pub fn target_for(conn: &Connection, id: Option<i64>) -> AppResult<ChatTarget> {
    let id = match id {
        Some(i) if i > 0 && model_exists(conn, i)? => i,
        _ => first_model_id(conn)?,
    };
    into_target(resolve_target(conn, Some(id), "", None, None)?)
}

fn model_exists(conn: &Connection, id: i64) -> AppResult<bool> {
    Ok(conn
        .query_row("SELECT 1 FROM ai_models WHERE id = ?1", params![id], |_| Ok(()))
        .optional()?
        .is_some())
}

fn into_target((base, key, model): (String, Option<String>, Option<String>)) -> AppResult<ChatTarget> {
    Ok(ChatTarget {
        base,
        key,
        model: model.ok_or_else(|| AppError::msg("要先填或先选模型名"))?,
    })
}

/// 目标可以来自已存的配置（传 id），也可以来自还没保存的表单（传 base_url + api_key + model）。
/// 留空的项沿用库里那份，所以编辑界面上不必重填就能测。
fn resolve_target(
    conn: &Connection,
    id: Option<i64>,
    base_url: &str,
    api_key: Option<&str>,
    model: Option<&str>,
) -> AppResult<(String, Option<String>, Option<String>)> {
    let mut base = base_url.trim().trim_end_matches('/').to_string();
    let mut key = api_key
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(str::to_string);
    let mut chosen = model.map(str::trim).filter(|m| !m.is_empty()).map(str::to_string);

    if let Some(id) = id {
        let row: Option<(String, Option<Vec<u8>>, String)> = conn
            .query_row(
                "SELECT base_url, key_enc, model FROM ai_models WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get::<_, Option<Vec<u8>>>(1)?, r.get(2)?)),
            )
            .optional()?;
        let (stored_base, stored_key, stored_model) = row.ok_or_else(|| AppError::msg("模型不存在"))?;
        if base.is_empty() {
            base = stored_base;
        }
        if key.is_none() {
            key = stored_key
                .as_deref()
                .map(dpapi::unprotect)
                .transpose()?
                .map(|b| String::from_utf8_lossy(&b).to_string());
        }
        if chosen.is_none() {
            chosen = Some(stored_model);
        }
    }

    if base.is_empty() {
        return Err(AppError::msg("要先填接口地址"));
    }
    Ok((base, key, chosen))
}

/// 「测试调用」用的目标：地址、密钥、模型名，留空的沿用库里那一行
pub fn call_target(
    conn: &Connection,
    id: Option<i64>,
    base_url: &str,
    api_key: Option<&str>,
    model: Option<&str>,
) -> AppResult<ChatTarget> {
    into_target(resolve_target(conn, id, base_url, api_key, model)?)
}

/// 「获取模型列表」只需要地址和密钥，模型名这时可以还空着
pub fn list_target(
    conn: &Connection,
    id: Option<i64>,
    base_url: &str,
    api_key: Option<&str>,
) -> AppResult<ChatTarget> {
    let (base, key, chosen) = resolve_target(conn, id, base_url, api_key, None)?;
    Ok(ChatTarget {
        base,
        key,
        model: chosen.unwrap_or_default(),
    })
}
