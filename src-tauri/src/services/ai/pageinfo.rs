//! 书签的智能分析：拿「网址该叫什么 + 一句简介」。
//!
//! 来源按站点分派（用户 2026-10-14 选的）：github.com 走 `api.github.com`——这台机器上
//! `https://github.com/…` 网页根本连不通，但 API 通（实测 200 / 0.67s），而且给的
//! `full_name` 和官方 description 比模型靠谱；其余公开页抓 og/twitter meta 或 `<title>` +
//! meta description；两者都拿不到（登录墙、飞书那类协作页）才交给模型补。
//!
//! 结果按归一化地址存本机（`settings` 里 `pageinfo:<地址键>`）。这既是「点一次才拉一次」的
//! 口径，也让 GitHub 那 60 次/小时的匿名额度能分几轮跑完——跑过的不再打网络。

use std::time::Duration;

use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::services::bookmarks::url_key;
use crate::services::ai::search::{attr, between, clean};
use crate::services::ai::shorten;

const TIMEOUT: Duration = Duration::from_secs(6);
/// api.github.com 匿名限 60 次/小时：一轮最多用这么多，剩下的下轮接着跑
pub const GITHUB_BUDGET: usize = 60;
/// 一轮最多抓多少个网页：请求是串行的（不打爆对方站点），一条 ~0.5 秒，
/// 再多一次点击就要等上半分钟，剩下的下轮接着跑（跑过的已经缓存了）
pub const PAGE_BUDGET: usize = 40;
pub const NAME_CHARS: usize = 40;
/// 他要的简介上限
pub const SUMMARY_CHARS: usize = 50;
const MAX_HTML: usize = 512 * 1024;

/// 是不是 GitHub 的地址：网页在这台机器上连不通，只有 API 可用
pub fn looks_like_github(url: &str) -> bool {
    split_url(url)
        .map(|(host, _)| host.ends_with("github.com"))
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub name: String,
    pub summary: String,
    /// `github` / `page` / `model`；从缓存读出来时会另加一个「缓存」标注
    pub source: String,
}

fn cache_key(url: &str) -> String {
    format!("pageinfo:{}", url_key(url))
}

/// 缓存条目的外壳：单独放一层是为了记时间，又不让 PageInfo 多一个到处都要填的字段
#[derive(Serialize, Deserialize)]
struct Entry {
    at: i64,
    info: PageInfo,
}

/// 网页会改版：超过这么久就当没有缓存，下一轮重新抓（仍受每轮条数上限约束）
const CACHE_TTL_SECS: i64 = 7 * 24 * 3600;

/// 本机存过就直接用，一条网页请求都不发；太老了当作没有
pub fn cached(conn: &Connection, url: &str) -> Option<PageInfo> {
    let raw = crate::services::settings::get_setting(conn, &cache_key(url))
        .ok()
        .flatten()?;
    let entry = serde_json::from_str::<Entry>(&raw).ok()?;
    let now = Utc::now().timestamp();
    // at 为 0 是旧格式或时间没记下，按过期处理：重抓一次就会把时间补上
    if entry.at == 0 || now - entry.at > CACHE_TTL_SECS {
        return None;
    }
    Some(entry.info)
}

pub fn store(conn: &Connection, url: &str, info: &PageInfo) -> AppResult<()> {
    let entry = Entry { at: Utc::now().timestamp(), info: info.clone() };
    crate::services::settings::set_setting(conn, &cache_key(url), &serde_json::to_string(&entry)?)
}

/// 让缓存立刻作废，好让单测能验「过期当作没缓存」这条分支
#[cfg(test)]
pub fn age_cache(conn: &Connection, url: &str, secs: i64) {
    let key = cache_key(url);
    if let Ok(Some(raw)) = crate::services::settings::get_setting(conn, &key) {
        if let Ok(mut entry) = serde_json::from_str::<Entry>(&raw) {
            entry.at -= secs;
            if let Ok(text) = serde_json::to_string(&entry) {
                let _ = crate::services::settings::set_setting(conn, &key, &text);
            }
        }
    }
}

/// 本机存了多少条网页信息，界面上要说得出这个数
pub fn cache_count(conn: &Connection) -> AppResult<usize> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM settings WHERE key LIKE 'pageinfo:%'", [], |r| r.get(0))?;
    Ok(n as usize)
}

/// 网页改了标题、仓库改了描述，旧的那份就该能一把清掉重抓
pub fn clear_cache(conn: &Connection) -> AppResult<usize> {
    Ok(conn.execute("DELETE FROM settings WHERE key LIKE 'pageinfo:%'", [])?)
}

/// 抓一条网址的结果。`Throttled` 单独分出来是为了让调用方别再撞额度：
/// GitHub 匿名限流回 403/429，一次运行里撞到就该把这一路的额度清零，剩下的下轮再来。
pub enum Outcome {
    Found(PageInfo),
    Throttled,
    Nothing,
}

pub fn throttled(status: u16) -> bool {
    status == 403 || status == 429
}

/// 取一条网址的名称和简介。取不到回 Nothing，交给调用方决定（丢给模型或留空）。
pub async fn gather(url: &str) -> Outcome {
    let Some((host, path)) = split_url(url) else {
        return Outcome::Nothing;
    };
    if host.ends_with("github.com") {
        return github(&path).await;
    }
    match page(url, &host).await {
        Some(info) => Outcome::Found(info),
        None => Outcome::Nothing,
    }
}

/// `https://host/a/b` → `("host", "a/b")`。项目里没留 url crate，手工切就够用。
fn split_url(url: &str) -> Option<(String, String)> {
    let (scheme, rest) = url.trim().split_once("://")?;
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let (head, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = head.split('@').next_back().unwrap_or(head).split(':').next().unwrap_or(head).to_lowercase();
    if host.is_empty() {
        return None;
    }
    Some((host, path.to_string()))
}

/// 这些一级路径不是仓库，`/owner/repo` 的猜法会对不上
const NOT_REPOS: &[&str] = &[
    "search", "features", "topics", "collections", "trending", "gist", "orgs", "settings", "login",
    "signup", "about", "showcase", "sponsors", "marketplace", "enterprise", "customer", "security",
    "privacy", "terms", "team", "issues", "pulls", "discussions", "codespaces", "copilot",
];

async fn github(path: &str) -> Outcome {
    let segs: Vec<&str> = path.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 || NOT_REPOS.contains(&segs[0].to_lowercase().as_str()) {
        return Outcome::Nothing;
    }
    let owner = segs[0];
    let repo = segs[1].trim_end_matches(".git");
    if repo.contains('.') || owner.contains('.') {
        return Outcome::Nothing;
    }
    let response = client()
        .get(format!("https://api.github.com/repos/{owner}/{repo}"))
        .header("accept", "application/vnd.github+json")
        .send()
        .await;
    let Ok(response) = response else { return Outcome::Nothing };
    // 额度撞墙要说得出来，别让调用方以为只是这条地址没仓库
    if throttled(response.status().as_u16()) {
        return Outcome::Throttled;
    }
    let Ok(response) = response.error_for_status() else {
        return Outcome::Nothing;
    };
    let Ok(body) = response.text().await else { return Outcome::Nothing };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) else {
        return Outcome::Nothing;
    };
    let Some(full) = json.get("full_name").and_then(|v| v.as_str()) else {
        return Outcome::Nothing;
    };
    let desc = json.get("description").and_then(|v| v.as_str()).unwrap_or("");

    let mut name = full.to_string();
    if segs.len() > 2 {
        // 同一个仓库的不同页面，名字上要看得出是哪个页面
        name.push_str(" · ");
        name.push_str(&segs[2..].join("/"));
    }
    Outcome::Found(PageInfo {
        name: shorten(&one_line(&name), NAME_CHARS),
        summary: shorten(&one_line(desc), SUMMARY_CHARS),
        source: "github".into(),
    })
}

async fn page(url: &str, host: &str) -> Option<PageInfo> {
    let response = client()
        .get(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    // 有些页面（图片墙、错误页跳巨型列表）会给几十 MB，先按声明的长度挡一下，
    // 不给它把整个响应读进内存的机会
    let too_big = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        .is_some_and(|len| len > MAX_HTML);
    if too_big {
        return None;
    }
    let html = response.text().await.ok()?;
    let head = clip_bytes(&html, MAX_HTML);

    let raw_title = meta_content(head, "property", "og:title")
        .or_else(|| meta_content(head, "property", "twitter:title"))
        .or_else(|| between(head, "<title", "</title>").map(|t| clean(&t)))?;
    let title = tidy_title(&raw_title, host);
    // 标题空着或者就是个登录页，拿它当名字反而更糟
    if title.is_empty() || is_gate(&title) {
        return None;
    }
    let summary = meta_content(head, "property", "og:description")
        .or_else(|| meta_content(head, "property", "twitter:description"))
        .or_else(|| meta_content(head, "name", "description"))
        .unwrap_or_default();

    Some(PageInfo {
        name: shorten(&title, NAME_CHARS),
        summary: shorten(&one_line(&summary), SUMMARY_CHARS),
        source: "page".into(),
    })
}

fn client() -> reqwest::Client {
    crate::services::ai::http_client(TIMEOUT).unwrap_or_default()
}

/// `<meta property="og:title" content="…">` 里的 content，属性顺序无所谓，单双引号都认
fn meta_content(html: &str, attr_name: &str, value: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("{attr_name}={quote}{value}{quote}");
        let at = html.find(&needle)?;
        let open = html[..at].rfind('<')?;
        let close = html[at..].find('>').map(|c| at + c)?;
        let tag = &html[open..=close];
        if let Some(content) = attr(tag, "content") {
            let text = clean(&content);
            if !text.is_empty() {
                return Some(text);
            }
        }
        // 双引号没找到再试单引号，不能在这里直接 return None
    }
    None
}

/// 站点爱在标题尾巴上挂「_CSDN博客」「 - 知乎」这类后缀。只切最后一个分隔符，
/// 后段确实带着站名才去掉——像「Rust - 中文入门」这种标题里的短横不是分隔符，不能乱剪。
/// 按字节数掐头部，但一定退回成一个完整字符再切：
/// 中文页每个字 3 字节，直接 `[..n]` 会落在字符中间，Rust 当场 panic。
/// 分块传输的响应没有 Content-Length，上面那道闸门拦不住，所以这里必须自己防。
fn clip_bytes(s: &str, max: usize) -> &str {
    let mut end = s.len().min(max);
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn tidy_title(title: &str, host: &str) -> String {
    let title = title.trim();
    let label = host.split('.').rev().nth(1).unwrap_or("").to_lowercase();
    if label.len() < 3 {
        return title.to_string();
    }
    let mut best: Option<usize> = None;
    for sep in ['-', '_', '|', '｜', '·', '–', '—'] {
        if let Some(pos) = title.rfind(sep) {
            best = Some(best.map_or(pos, |b: usize| b.max(pos)));
        }
    }
    match best {
        Some(pos) => {
            let width = title[pos..].chars().next().map(char::len_utf8).unwrap_or(1);
            let tail = title[pos + width..].to_lowercase();
            if !tail.trim().is_empty() && tail.contains(&label) {
                title[..pos].trim().to_string()
            } else {
                title.to_string()
            }
        }
        None => title.to_string(),
    }
}

fn is_gate(title: &str) -> bool {
    let low = title.to_lowercase();
    ["登录", "登入", "请登录", "扫码登录", "授权", "sign in", "log in", "login", "unified login", "oauth"]
        .iter()
        .any(|k| low.contains(k))
}

/// meta 里常见换行和缩进，压成一行
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_never_lands_mid_char() {
        // 「知」「识」各占 3 字节：切点落在字中间就是旧写法的 panic
        let s = "abc知识def";
        assert_eq!(clip_bytes(s, 4), "abc");
        assert_eq!(clip_bytes(s, 3), "abc");
        assert_eq!(clip_bytes(s, 100), s);
        assert_eq!(clip_bytes("", 5), "");
    }

    const CSDN: &str = r#"<html><head>
      <title>Rust 异步运行时怎么选_CSDN博客</title>
      <meta name="description" content="  对比 tokio、async-std 与 smol 的调度差别，
      附基准测试数据。 &#38; 结论。  " />
    </head><body>x</body></html>"#;

    const OG: &str = r#"<html><head>
      <meta property="og:title" content="掘金· 一个掘金用户的主页" />
      <meta property="og:description" content='每日发布挖掘好内容的创作者主页，分享前端与 Rust 文章。' />
      <title>不该用这个</title>
    </head></html>"#;

    const GATE: &str = r#"<html><head><title>飞书 - 登录</title><meta name="description" content="登录" /></head></html>"#;

    #[test]
    fn strips_the_site_suffix_but_keeps_real_dashes() {
        let raw = between(CSDN, "<title", "</title>").map(|t| clean(&t)).unwrap();
        assert_eq!(raw, "Rust 异步运行时怎么选_CSDN博客");
        assert_eq!(tidy_title(&raw, "blog.csdn.net"), "Rust 异步运行时怎么选");
        // 标题自带的短横不是站点后缀，剪了就毁掉区分度
        assert_eq!(tidy_title("Rust - 异步入门 - 手册", "gitee.com"), "Rust - 异步入门 - 手册");

        let desc = meta_content(CSDN, "name", "description").unwrap();
        assert!(desc.starts_with("对比 tokio、async-std 与 smol"), "{desc}");
        assert!(desc.contains("& 结论"), "实体没解：{desc}");
        assert!(!desc.contains('\n'), "简介里不该留换行");
    }

    #[test]
    fn og_tags_win_over_title_and_survive_single_quotes() {
        assert_eq!(meta_content(CSDN, "property", "og:title"), None);
        assert_eq!(meta_content(OG, "property", "og:title").as_deref(), Some("掘金· 一个掘金用户的主页"));
        assert_eq!(
            meta_content(OG, "property", "og:description").as_deref(),
            Some("每日发布挖掘好内容的创作者主页，分享前端与 Rust 文章。")
        );
    }

    #[test]
    fn login_pages_are_not_a_name() {
        let title = clean(&between(GATE, "<title", "</title>").unwrap());
        assert!(is_gate(&title), "{title}");
        assert!(!is_gate("Rust 异步运行时怎么选"));
    }

    #[test]
    fn url_key_makes_case_and_slash_variants_equal() {
        assert_eq!(url_key("https://GitHub.com/u/r/"), url_key("https://github.com/u/r"));
        assert_eq!(url_key("  http://a.cn/x#frag"), url_key("http://a.cn/x"));
        // 查询串是不同页面，不能并掉
        assert_ne!(url_key("http://a.cn/x?page=1"), url_key("http://a.cn/x?page=2"));
    }

    #[test]
    fn splitting_urls_only_accepts_http_and_needs_a_host() {
        assert_eq!(split_url("https://gitee.com/a/b").unwrap().0, "gitee.com");
        assert_eq!(split_url("https://gitee.com:443/a").unwrap().0, "gitee.com");
        assert!(split_url("file:///C:/x").is_none());
        assert!(split_url("不是地址").is_none());
        assert!(split_url("https:///x").is_none());
    }

    fn settings_conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT)", [])
            .unwrap();
        c
    }

    #[test]
    fn cache_can_be_counted_and_cleared() {
        let c = settings_conn();
        crate::services::settings::set_setting(&c, "smart_model_id", "7").unwrap();
        store(&c, "https://a.cn/1", &PageInfo { name: "一".into(), summary: String::new(), source: "page".into() }).unwrap();
        store(&c, "https://a.cn/2", &PageInfo { name: "二".into(), summary: String::new(), source: "github".into() }).unwrap();

        assert_eq!(cache_count(&c).unwrap(), 2, "只该数到 pageinfo 自己的键");
        assert_eq!(clear_cache(&c).unwrap(), 2);
        assert_eq!(cache_count(&c).unwrap(), 0);
        // 别的设置项不能被顺手清了
        assert_eq!(
            crate::services::settings::get_setting(&c, "smart_model_id").unwrap().as_deref(),
            Some("7")
        );
        assert!(cached(&c, "https://a.cn/1").is_none());
    }

    /// 真联网一条：默认不跑。改版、限额、证书出问题的时候手动跑一下看还合不合得上
    #[tokio::test]
    #[ignore = "要打外网，日常测试不跑"]
    async fn one_live_gather_each_source() {
        let repo = match gather("https://github.com/torvalds/linux").await {
            Outcome::Found(info) => info,
            other => panic!("GitHub API 没取到，结果是 {:?}", matches!(other, Outcome::Throttled)),
        };
        assert_eq!(repo.name, "torvalds/linux");
        assert!(!repo.summary.is_empty());

        let page = match gather("https://gitee.com/").await {
            Outcome::Found(info) => info,
            _ => panic!("gitee 页面没取到"),
        };
        assert!(!page.name.is_empty() && page.source == "page", "{page:?}");

        // 同仓库不同页面要在名字上分得开
        let blob = match gather("https://github.com/torvalds/linux/blob/master/README").await {
            Outcome::Found(info) => info,
            _ => panic!("仓库子页面没取到"),
        };
        assert!(blob.name.contains("README"), "{blob:?}");
    }

    #[test]
    fn stale_cache_is_treated_as_missing() {
        let c = settings_conn();
        let info = PageInfo { name: "老名字".into(), summary: String::new(), source: "page".into() };
        store(&c, "https://a.cn/x", &info).unwrap();
        assert_eq!(cached(&c, "https://a.cn/x").unwrap().name, "老名字", "刚存的要用得上");

        // 网页改版不能永远冻在旧值：过期的当作没缓存，下一轮重抓并把时间补回来
        age_cache(&c, "https://a.cn/x", CACHE_TTL_SECS + 60);
        assert!(cached(&c, "https://a.cn/x").is_none(), "过期了还当命中");
        assert_eq!(cache_count(&c).unwrap(), 1, "过期只是不读，不该偷偷删");
        store(&c, "https://a.cn/x", &PageInfo { name: "新名字".into(), summary: String::new(), source: "page".into() }).unwrap();
        assert_eq!(cached(&c, "https://a.cn/x").unwrap().name, "新名字");
    }

    #[test]
    fn rate_limit_is_its_own_answer() {
        assert!(throttled(403) && throttled(429), "GitHub 的限流就这两个码");
        assert!(!throttled(404), "仓库不存在不是额度问题，别把整轮额度清零");
        assert!(!throttled(500), "上游炸了也不算额度");
    }
}
