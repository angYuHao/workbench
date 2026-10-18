//! 查不认识的软件：抓 cn.bing.com 的公开结果页。
//!
//! 为什么是 Bing：这台机器实测 DuckDuckGo / Google / Wikipedia / Brave 的域名都连不通
//! （curl 直接 000），只有 Bing 通而且结果页能解析。Bing 没有免费对外接口，所以只能读
//! 结果页——它改版这边就得跟着改，因此解析全部写成纯函数，用真实页面片段做单测钉住。
//!
//! 查询词是软件的显示名（去掉扩展名），会出本机；每一次搜索都写进「调用日志」。

use std::time::Duration;

use serde::Serialize;

use crate::error::{AppError, AppResult};

const ENDPOINT: &str = "https://cn.bing.com/search";
/// Bing 对没有 UA 的请求会发一个精简版页面，拿不到结果块
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
const ITEM: &str = "<li class=\"b_algo\"";
const MAX_SNIPPETS: usize = 4;
const TITLE_CHARS: usize = 60;
const SNIPPET_CHARS: usize = 160;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub title: String,
    pub site: String,
    pub text: String,
}

impl Snippet {
    /// 记忆里就存这一句：够短，又能看出是谁说的
    pub fn evidence(&self) -> String {
        let mut out = self.title.clone();
        if !self.site.is_empty() {
            out.push('（');
            out.push_str(&self.site);
            out.push('）');
        }
        out
    }
}

/// 搜一个词，最多回 4 条摘要；一条都没解析出来当作「没查到」，由调用方决定怎么办
pub async fn search(query: &str) -> AppResult<Vec<Snippet>> {
    let url = format!("{ENDPOINT}?q={}", encode(query));
    let started = std::time::Instant::now();
    let page = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent(UA)
        .build()
        .map_err(|e| AppError::msg(format!("起不动 HTTP 客户端：{e}")))?
        .get(&url)
        .header("accept-language", "zh-CN,zh;q=0.9")
        .send()
        .await
        .map_err(|e| AppError::msg(format!("搜「{query}」没连上：{e}")))?
        .error_for_status()
        .map_err(|e| AppError::msg(format!("搜「{query}」被拒绝：{e}")))?
        .text()
        .await
        .map_err(|e| AppError::msg(format!("搜「{query}」正文读不出：{e}")));

    match page {
        Ok(page) => {
            let found = parse_results(&page);
            record(query, &url, started.elapsed().as_millis(), true, &summarize(&found));
            Ok(found)
        }
        Err(e) => {
            let why = e.to_string();
            record(query, &url, started.elapsed().as_millis(), false, &why);
            Err(e)
        }
    }
}

fn record(query: &str, url: &str, elapsed_ms: u128, ok: bool, text: &str) {
    crate::services::ai::log::record(&crate::services::ai::log::Call {
        tag: &format!("联网搜索「{query}」"),
        url,
        model: "bing",
        status: if ok { 200 } else { 0 },
        elapsed_ms,
        ok,
        request: query,
        response: text,
    });
}

fn summarize(found: &[Snippet]) -> String {
    if found.is_empty() {
        return "没解析出结果".to_string();
    }
    found
        .iter()
        .map(|s| format!("{}|{}", s.title, s.text))
        .collect::<Vec<_>>()
        .join(" ／ ")
}

fn parse_results(html: &str) -> Vec<Snippet> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(offset) = html[from..].find(ITEM) {
        let start = from + offset;
        let tail = start + ITEM.len();
        let next = html[tail..].find(ITEM).map(|o| tail + o).unwrap_or(html.len());
        if let Some(one) = parse_item(&html[start..next]) {
            out.push(one);
            if out.len() >= MAX_SNIPPETS {
                break;
            }
        }
        from = next;
    }
    out
}

fn parse_item(block: &str) -> Option<Snippet> {
    let heading = between(block, "<h2", "</h2>")?;
    let title = cut(&clean(&heading), TITLE_CHARS);
    if title.is_empty() {
        return None;
    }
    let site = attr(&heading, "href").map(|h| host(&h)).unwrap_or_default();
    let text = between(block, "class=\"b_caption\"", "</p>")
        .map(|p| cut(&clean(&p), SNIPPET_CHARS))
        .unwrap_or_default();
    Some(Snippet { title, site, text })
}

/// open 标签之后到 close 之前的一段
pub(crate) fn between(s: &str, open: &str, close: &str) -> Option<String> {
    let start = s.find(open)?;
    let body = s[start..].find('>').map(|o| start + o + 1)?;
    let end = s[body..].find(close)?;
    Some(s[body..body + end].to_string())
}

pub(crate) fn attr(s: &str, name: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let key = format!("{name}={quote}");
        if let Some(at) = s.find(&key) {
            let start = at + key.len();
            if let Some(end) = s[start..].find(quote) {
                return Some(s[start..start + end].to_string());
            }
        }
    }
    None
}

/// 去掉标签和解实体：Bing 的标题里常见 &#0183;（·）和 &amp;
pub(crate) fn clean(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            depth += 1;
            continue;
        }
        if depth > 0 {
            if c == '>' {
                depth -= 1;
            }
            continue;
        }
        if c == '&' {
            let entity: String = chars.clone().by_ref().take(10).collect();
            if let Some(semi) = entity.find(';') {
                let body = &entity[..semi];
                if let Some(code) = body.strip_prefix('#') {
                    let decimal = code.strip_prefix('x').unwrap_or(code);
                    if let Ok(v) = u32::from_str_radix(decimal, if code.starts_with('x') { 16 } else { 10 }) {
                        if let Some(ch) = char::from_u32(v) {
                            out.push(ch);
                            for _ in 0..semi + 1 {
                                chars.next();
                            }
                            continue;
                        }
                    }
                }
                let named = match body {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    "middot" => Some('·'),
                    _ => None,
                };
                if let Some(ch) = named {
                    out.push(ch);
                    for _ in 0..semi + 1 {
                        chars.next();
                    }
                    continue;
                }
            }
        }
        out.push(c);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn host(url: &str) -> String {
    let after = url.split("://").nth(1).unwrap_or(url);
    after.split('/').next().unwrap_or("").trim_start_matches("www.").to_string()
}

pub(crate) fn cut(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}

/// 只用于查询词：不依赖 url 编码 crate，非保留字符一律 %XX，空格按表单习惯给 +
fn encode(q: &str) -> String {
    let mut out = String::with_capacity(q.len() * 3);
    for byte in q.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*byte as char),
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真页片段改小：两条 b_algo，一条带 &#0183; 和 &amp;
    const PAGE: &str = r#"
    <ol id="b_results">
      <li class="b_algo"><h2><a href="https://ffmpeg.org/">FFmpeg&#0183;A Complete, Cross&#45;platform Solution</a></h2>
        <div class="b_caption"><p>FFmpeg 是领先的多媒体框架，能够解码、编码、转码、混流、播放任意格式。</p></div></li>
      <li class="b_algo"><h2><a href="https://ffmpeg.org/download.html">FFmpeg 下载</a> &amp; more</h2>
        <div class="b_caption"><p>Get the binaries for Windows &amp; macOS.</p></div></li>
      <li class="b_algo"><h2><a href="https://blog.csdn.net/x/y">CSDN 博客</a></h2>
        <div class="b_caption"><p>记录一次安装过程。</p></div></li>
    </ol>"#;

    #[test]
    fn parses_title_site_and_caption() {
        let found = parse_results(PAGE);

        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!(found[0].title, "FFmpeg·A Complete, Cross-platform Solution");
        assert_eq!(found[0].site, "ffmpeg.org");
        assert!(found[0].text.starts_with("FFmpeg 是领先的多媒体框架"), "{}", found[0].text);
        assert_eq!(found[1].title, "FFmpeg 下载 & more", "标题外的实体也要解");
        assert_eq!(found[2].site, "blog.csdn.net");
    }

    #[test]
    fn stops_at_the_cap_and_survives_junk_pages() {
        let many = "<li class=\"b_algo\"><h2><a href=\"https://a.cn/\">t</a></h2><div class=\"b_caption\"><p>x</p></div></li>".repeat(10);
        assert_eq!(parse_results(&many).len(), MAX_SNIPPETS);
        assert!(parse_results("").is_empty());
        assert!(parse_results("<html>验证码，没结果</html>").is_empty());
    }

    #[test]
    fn query_encoding_covers_chinese_and_spaces() {
        assert_eq!(encode("Loomy code"), "Loomy+code");
        assert_eq!(encode("微信"), "%E5%BE%AE%E4%BF%A1");
        assert_eq!(encode("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn long_pieces_get_cut_not_wrapped() {
        assert_eq!(cut("一二三四五六七八九十", 4), "一二三四…");
        assert_eq!(cut("短", 4), "短");
    }

    /// 真联网一条：默认不跑，验证 Bing 结构和解析还合不合得上时手动执行
    #[tokio::test]
    #[ignore = "要打外部搜索引擎，日常测试不跑"]
    async fn one_live_search() {
        let found = search("ffmpeg").await.expect("搜索没通");
        assert!(!found.is_empty(), "解析不出任何结果，Bing 大概改版了");
        assert!(found[0].title.to_lowercase().contains("ffmpeg"), "{:?}", found[0]);
    }
}
