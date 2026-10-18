use std::collections::HashMap;
use std::path::PathBuf;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::AppResult;
use crate::models::{Bookmark, ImportResult};

#[derive(serde::Deserialize)]
struct BNode {
    #[serde(rename = "type")]
    node_type: String,
    name: String,
    url: Option<String>,
    children: Option<Vec<BNode>>,
}

#[derive(serde::Deserialize)]
struct BRoots {
    roots: HashMap<String, BNode>,
}

/// 比对用的地址键：协议和域名小写、去掉结尾斜杠和 #片段，查询串留着
/// （`?page=2` 和 `?page=3` 是两个页面，不能并成一个）。导入查重和分析查重共用这一个口径，
/// 以前用整串精确比对，大小写和结尾斜杠的差别就漏成了重复条目。
pub fn url_key(url: &str) -> String {
    let trimmed = url.trim();
    let without_frag = trimmed.split('#').next().unwrap_or(trimmed);
    match without_frag.split_once("://") {
        Some((scheme, rest)) => {
            let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
            let host = host.to_lowercase();
            let path = path.trim_end_matches('/');
            format!("{scheme}://{host}/{path}")
        }
        None => without_frag.to_lowercase(),
    }
}

fn existing_url_keys(conn: &Connection) -> AppResult<std::collections::HashSet<String>> {
    let mut stmt = conn.prepare("SELECT url FROM bookmarks")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.filter_map(Result::ok).map(|u| url_key(&u)).collect())
}

/// 同一个地址被存了多次的组，按创建先后排（第一条是保留的那条）
pub fn duplicate_groups(conn: &Connection) -> AppResult<Vec<Vec<i64>>> {
    let mut by_key: HashMap<String, Vec<(i64, String)>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT id, url, COALESCE(created_at, '') FROM bookmarks ORDER BY created_at, id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows.flatten() {
        by_key.entry(url_key(&row.1)).or_default().push((row.0, row.2));
    }
    Ok(by_key
        .into_values()
        .filter(|v| v.len() > 1)
        .map(|v| v.into_iter().map(|(id, _)| id).collect())
        .collect())
}

pub fn import_bookmarks(conn: &Connection, browser: &str) -> AppResult<ImportResult> {
    let mut result = ImportResult {
        total: 0,
        imported: 0,
        skipped: 0,
    };
    let source = browser.to_lowercase();
    // 先在库里排查一遍，只剩没见过的地址才插
    let mut seen = existing_url_keys(conn)?;
    for file in find_bookmarks_files(&source) {
        let content = match std::fs::read_to_string(&file) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let parsed: BRoots = match serde_json::from_str(&content) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let blocked = list_blacklist(conn)?;
        for (_, root) in parsed.roots {
            import_node(conn, &root, None, &source, &mut result, &mut seen, &blocked)?;
        }
    }
    Ok(result)
}

fn find_bookmarks_files(browser: &str) -> Vec<PathBuf> {
    let key = if browser == "edge" {
        "Microsoft/Edge/User Data"
    } else {
        "Google/Chrome/User Data"
    };
    let local = match std::env::var("LOCALAPPDATA") {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let user_data = PathBuf::from(local).join(key);
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&user_data) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                let b = p.join("Bookmarks");
                if b.exists() {
                    files.push(b);
                }
            }
        }
    }
    files
}

fn import_node(
    conn: &Connection,
    node: &BNode,
    parent: Option<i64>,
    browser: &str,
    result: &mut ImportResult,
    seen: &mut std::collections::HashSet<String>,
    blocked: &[BlacklistEntry],
) -> AppResult<()> {
    if node.node_type == "url" {
        result.total += 1;
        let url = match &node.url {
            Some(u) if !u.is_empty() => u,
            _ => {
                result.skipped += 1;
                return Ok(());
            }
        };
        if is_blocked(blocked, url).is_some() {
            result.skipped += 1;
            return Ok(());
        }
        // 归一化后比对：库里已有的、以及这一份导出里前面出现过的，都算重复
        if !seen.insert(url_key(url)) {
            result.skipped += 1;
        } else {
            conn.execute(
                "INSERT INTO bookmarks (title, url, category_id, source) VALUES (?1, ?2, ?3, ?4)",
                params![node.name, url, parent, browser],
            )?;
            result.imported += 1;
        }
    } else if node.node_type == "folder" {
        let cat_id = ensure_category(conn, &node.name, parent)?;
        if let Some(children) = &node.children {
            for c in children {
                import_node(conn, c, Some(cat_id), browser, result, seen, blocked)?;
            }
        }
    }
    Ok(())
}

fn ensure_category(conn: &Connection, name: &str, parent: Option<i64>) -> AppResult<i64> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM categories WHERE type = 'bookmark' AND name = ?1 AND (parent_id IS ?2)",
            params![name, parent],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO categories (name, type, parent_id, sort) VALUES (?1, 'bookmark', ?2, 0)",
        params![name, parent],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_bookmarks(conn: &Connection) -> AppResult<Vec<Bookmark>> {
    let mut stmt = conn.prepare(
        "SELECT b.id, b.title, b.url, b.description, b.category_id, b.source, b.created_at,
                COALESCE(u.uses, 0)
         FROM bookmarks b
         LEFT JOIN usage_stats u ON u.kind = 'bookmark' AND u.id = b.id
         ORDER BY COALESCE(u.uses, 0) DESC, b.title",
    )?;
    // 拉黑不是删数据：行还在表里，只是不再显示；把黑名单去掉它们就原样回来（分类简介都不丢）
    let blocked = list_blacklist(conn)?;
    let rows = stmt.query_map([], move |r| {
        Ok(Bookmark {
            id: r.get(0)?,
            title: r.get(1)?,
            url: r.get(2)?,
            description: r.get(3)?,
            category_id: r.get(4)?,
            source: r.get(5)?,
            created_at: r.get(6)?,
            use_count: r.get(7)?,
        })
    })?;
    Ok(rows
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|b| is_blocked(&blocked, &b.url).is_none())
        .collect())
}

pub fn save_bookmark(conn: &Connection, b: &Bookmark) -> AppResult<Bookmark> {
    if let Some(hit) = is_blocked(&list_blacklist(conn)?, &b.url) {
        // 手动添加撞进黑名单要当场说，不然他会以为加了却看不见是 bug
        return Err(crate::error::AppError::msg(format!(
            "「{}」在黑名单里（{}）",
            b.url, hit.pattern
        )));
    }
    if b.id > 0 {
        conn.execute(
            "UPDATE bookmarks SET title = ?1, url = ?2, description = ?3, category_id = ?4 WHERE id = ?5",
            params![b.title, b.url, b.description, b.category_id, b.id],
        )?;
        Ok(b.clone())
    } else {
        conn.execute(
            "INSERT INTO bookmarks (title, url, description, category_id, source)
             VALUES (?1, ?2, ?3, ?4, 'manual')",
            params![b.title, b.url, b.description, b.category_id],
        )?;
        let id = conn.last_insert_rowid();
        Ok(Bookmark { id, ..b.clone() })
    }
}

pub fn delete_bookmark(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM bookmarks WHERE id = ?1", params![id])?;
    crate::services::usage::forget(conn, crate::services::usage::BOOKMARK, id)
}

/// 一条黑名单。`pattern` 是已经归一化好的匹配值。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlacklistEntry {
    pub pattern: String,
    /// `url`：归一化后的精确地址；`domain`：该域名及其所有子域的页面
    pub kind: String,
    pub created_at: i64,
}

/// 把用户输入归一化成 (kind, pattern)。
/// 带 `://` 或带路径的当精确地址（`?page=2` 和 `?page=3` 是两个页面，不会互相屏蔽）；
/// 纯主机名当域名，并且吃掉开头的 `www.` —— 否则「屏蔽 www.douban.com」挡不住直接访问 douban.com 的。
pub fn normalize_pattern(raw: &str) -> Option<(String, String)> {
    let raw = raw.trim().trim_end_matches('.');
    if raw.is_empty() || raw.chars().any(char::is_whitespace) {
        return None;
    }
    if raw.contains("://") {
        return Some(("url".into(), url_key(raw)));
    }
    let (host, path) = raw.split_once('/').unwrap_or((raw, ""));
    if !path.is_empty() {
        return Some(("url".into(), url_key(&format!("https://{raw}"))));
    }
    let domain = host.trim_start_matches("*.").to_lowercase();
    let domain = domain.strip_prefix("www.").unwrap_or(&domain).to_string();
    if domain.len() < 3 || !domain.contains('.') {
        return None;
    }
    Some(("domain".into(), domain))
}

fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let head = rest.split('/').next().unwrap_or("");
    head.split('@').next_back().unwrap_or(head).split(':').next().unwrap_or(head).to_lowercase()
}

/// 这条地址是否被拉黑。空列表当然全放过。
pub fn is_blocked<'a>(entries: &'a [BlacklistEntry], url: &str) -> Option<&'a BlacklistEntry> {
    let key = url_key(url);
    let host = host_of(url);
    entries.iter().find(|e| match e.kind.as_str() {
        "url" => e.pattern == key,
        _ => host == e.pattern || host.ends_with(&format!(".{}", e.pattern)),
    })
}

pub fn list_blacklist(conn: &Connection) -> AppResult<Vec<BlacklistEntry>> {
    let mut stmt = conn.prepare(
        "SELECT pattern, kind, created_at FROM bookmark_blacklist ORDER BY created_at DESC, pattern",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(BlacklistEntry { pattern: r.get(0)?, kind: r.get(1)?, created_at: r.get(2)? })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// 加一条。同一写法重复添加只保留最早那条时间，回真正生效的那条。
pub fn add_blacklist(conn: &Connection, raw: &str) -> AppResult<BlacklistEntry> {
    let (kind, pattern) =
        normalize_pattern(raw).ok_or_else(|| crate::error::AppError::msg("写个域名（如 douban.com）或完整网址"))?;
    conn.execute(
        "INSERT INTO bookmark_blacklist (pattern, kind, created_at) VALUES (?1, ?2, ?3) ON CONFLICT(pattern) DO NOTHING",
        params![pattern, kind, chrono::Utc::now().timestamp()],
    )?;
    let stored = list_blacklist(conn)?
        .into_iter()
        .find(|e| e.pattern == pattern)
        .ok_or_else(|| crate::error::AppError::msg("黑名单没写进去"))?;
    Ok(stored)
}

pub fn remove_blacklist(conn: &Connection, pattern: &str) -> AppResult<usize> {
    Ok(conn.execute("DELETE FROM bookmark_blacklist WHERE pattern = ?1", params![pattern])?)
}

/// 多选拉黑：按选中条目的精确地址入名单（不会因为一篇文章就把整站拖走）。
/// 想整站屏蔽去设置里手输域名。
pub fn block_bookmarks(conn: &Connection, ids: &[i64]) -> AppResult<usize> {
    let mut added = 0;
    for id in ids {
        let url: Option<String> = conn
            .query_row("SELECT url FROM bookmarks WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?;
        let Some(url) = url else { continue };
        let (_, pattern) = normalize_pattern(&url).unwrap_or_else(|| ("url".into(), url_key(&url)));
        let n = conn.execute(
            "INSERT INTO bookmark_blacklist (pattern, kind, created_at) VALUES (?1, 'url', ?2) ON CONFLICT(pattern) DO NOTHING",
            params![pattern, chrono::Utc::now().timestamp()],
        )?;
        added += n;
    }
    Ok(added)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::usage::{note_use, BOOKMARK};

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute("CREATE TABLE bookmarks (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, url TEXT NOT NULL, description TEXT, category_id INTEGER, source TEXT DEFAULT 'manual', created_at DATETIME DEFAULT CURRENT_TIMESTAMP)", []).unwrap();
        c.execute("CREATE TABLE bookmark_blacklist (pattern TEXT PRIMARY KEY, kind TEXT NOT NULL, created_at INTEGER NOT NULL)", []).unwrap();
        c.execute("CREATE TABLE usage_stats (kind TEXT NOT NULL, id INTEGER NOT NULL, uses INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (kind, id))", []).unwrap();
        c
    }

    fn titles(c: &Connection) -> Vec<String> {
        get_bookmarks(c)
            .unwrap()
            .into_iter()
            .map(|b| b.title)
            .collect()
    }

    fn add(c: &Connection, url: &str) {
        c.execute("INSERT INTO bookmarks (title, url) VALUES (?1, ?2)", params![format!("标题{}", c.last_insert_rowid()), url]).unwrap();
    }

    fn norm(raw: &str) -> (String, String) {
        let (kind, pattern) = normalize_pattern(raw).unwrap();
        (kind, pattern)
    }

    #[test]
    fn input_is_normalised_by_shape() {
        assert_eq!(norm(" www.douban.com "), ("domain".to_string(), "douban.com".to_string()));
        assert_eq!(norm("*.douban.com"), ("domain".to_string(), "douban.com".to_string()));
        assert_eq!(
            norm("https://movie.douban.com/subject/1/?src=xd"),
            ("url".to_string(), "https://movie.douban.com/subject/1/?src=xd".to_string())
        );
        // 光主机名后面还带路径，那是个地址不是域名
        assert_eq!(norm("douban.com/topics").0, "url");
        assert!(normalize_pattern("没有点").is_none());
        assert!(normalize_pattern("a b.com").is_none());
        assert!(normalize_pattern("").is_none());
        assert!(normalize_pattern("  ").is_none());
    }

    #[test]
    fn domain_rule_covers_subdomains_but_not_lookalikes() {
        let c = conn();
        add_blacklist(&c, "www.douban.com").unwrap();
        let rules = list_blacklist(&c).unwrap();
        assert_eq!(rules[0].pattern, "douban.com", "存的 www 要归一到根域名，否则挡不住直接访问根域");
        assert!(is_blocked(&rules, "https://douban.com/").is_some());
        assert!(is_blocked(&rules, "https://movie.douban.com/subject/1").is_some());
        assert!(is_blocked(&rules, "http://m.douban.com/movie").is_some());
        assert!(is_blocked(&rules, "https://notdouban.com/x").is_none(), "后缀巧合不能算命中");
        assert!(is_blocked(&rules, "https://douban.com.evil.cn/x").is_none());
    }

    #[test]
    fn url_rule_ignores_host_case_and_slash_but_keeps_path_case() {
        let c = conn();
        let added = add_blacklist(&c, "https://GITEE.com/A/B/").unwrap();
        assert_eq!(added.kind, "url");
        assert_eq!(added.pattern, "https://gitee.com/A/B", "主机小写、路径大小写要留着");
        let rules = list_blacklist(&c).unwrap();
        assert!(is_blocked(&rules, "https://gitee.com/A/B").is_some());
        assert!(is_blocked(&rules, "https://gitee.com/a/b").is_none(), "GitHub 上 /A 和 /a 是两个仓库，不能并");
        assert!(is_blocked(&rules, "https://gitee.com/A/B/c").is_none(), "子页面不该被一条地址带进去");
        assert!(is_blocked(&rules, "https://gitee.com/A/B?page=2").is_none(), "查询串不同就是另一个页面");
    }

    #[test]
    fn blocked_rows_vanish_from_the_list_but_stay_in_the_table() {
        let c = conn();
        add(&c, "https://douban.com/book");
        add(&c, "https://keep.me/x");
        assert_eq!(get_bookmarks(&c).unwrap().len(), 2);

        add_blacklist(&c, "douban.com").unwrap();

        let left = get_bookmarks(&c).unwrap();
        assert_eq!(left.len(), 1, "被拉黑的该从页面上消失");
        assert_eq!(left[0].url, "https://keep.me/x");
        let raw: i64 = c.query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get(0)).unwrap();
        assert_eq!(raw, 2, "只是过滤，没删数据");

        // 取消拉黑，原条目带著分类和简介回来
        remove_blacklist(&c, "douban.com").unwrap();
        assert_eq!(get_bookmarks(&c).unwrap().len(), 2);
    }

    #[test]
    fn saving_a_blocked_url_is_refused_out_loud() {
        let c = conn();
        add_blacklist(&c, "douban.com").unwrap();
        let b = Bookmark {
            id: 0,
            title: "豆瓣".into(),
            url: "https://www.douban.com/".into(),
            description: None,
            category_id: None,
            source: "manual".into(),
            created_at: None,
            use_count: 0,
        };
        let err = save_bookmark(&c, &b).unwrap_err().to_string();
        assert!(err.contains("黑名单") && err.contains("douban.com"), "{err}");
        let n: i64 = c.query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn clicked_rows_lead_the_list_and_the_rest_stay_by_title() {
        let c = conn();
        c.execute("INSERT INTO bookmarks (id, title, url) VALUES (1,'a','https://a.me/'),(2,'b','https://b.me/'),(3,'c','https://c.me/')", []).unwrap();
        assert_eq!(titles(&c), ["a", "b", "c"], "一次都没点的照旧按标题排");

        for _ in 0..2 {
            note_use(&c, BOOKMARK, 3).unwrap();
        }
        note_use(&c, BOOKMARK, 2).unwrap();

        let list = get_bookmarks(&c).unwrap();
        assert_eq!(
            list.iter().map(|b| b.title.clone()).collect::<Vec<_>>(),
            ["c", "b", "a"],
            "点得多的排前面"
        );
        assert_eq!(list[0].use_count, 2, "次数要一起带出去，前端拿它算分类的总热度");
    }

    #[test]
    fn multi_select_block_adds_exact_addresses_only() {
        let c = conn();
        c.execute("INSERT INTO bookmarks (id, title, url) VALUES (1, 'a', 'https://movie.douban.com/subject/1'), (2, 'b', 'https://keep.me/x')", []).unwrap();

        assert_eq!(block_bookmarks(&c, &[1, 2, 999]).unwrap(), 2, "不存在的 id 不该记账");
        let rules = list_blacklist(&c).unwrap();
        assert!(rules.iter().all(|r| r.kind == "url"), "多选拉黑不能顺手把整站拖走");
        assert_eq!(get_bookmarks(&c).unwrap().len(), 0);
        // 再拉一次同样两条，不该翻倍
        assert_eq!(block_bookmarks(&c, &[1, 2]).unwrap(), 0);
        assert_eq!(list_blacklist(&c).unwrap().len(), 2);
    }
}
