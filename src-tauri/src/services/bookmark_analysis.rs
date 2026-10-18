//! 书签的智能分析：给每条网址补「该叫什么」和一句 ≤50 字的简介，并挑出重复地址。
//!
//! 来源按站点分派（`pageinfo`：GitHub 用 API，其他抓 meta），抓不到才交给模型。
//! 每轮打网络有条数上限（GitHub 匿名 60 次/小时、网页 40 条），抓到的都缓存进本机，
//! 所以「这轮没排到」不是丢数据 —— 再点一次接着跑，跑过的不再发请求。

use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::models::Bookmark;
use crate::services::ai::models::ChatTarget;
use crate::services::{ai::{categorize, pageinfo}, bookmarks};
use crate::services::bookmarks::url_key;
use pageinfo::PageInfo;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisRow {
    pub id: i64,
    pub url: String,
    /// 库里现在的名字，界面上好对比
    pub old_name: String,
    pub name: String,
    pub summary: String,
    /// github / page / model / cache；空字符串表示这条还没判出来
    pub source: String,
    /// 同一个地址存了多次
    pub duplicate: bool,
    /// 本轮抓取额度没排到它：名字可能是模型猜的，再跑一轮能用网页信息顶掉
    pub queued: bool,
    /// 重复组里建议保留的那条（创建最早的一条）
    pub keep: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisUpdate {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub summary: String,
}

/// 先吃本机缓存：命中的直接可用，没命中的留给网络那一步。
/// `refresh` 为真时跳过缓存全部重抓（网页改版、仓库改了描述就靠这个）。
pub fn from_cache(conn: &Connection, items: &[Bookmark], refresh: bool) -> Vec<AnalysisRow> {
    items
        .iter()
        .map(|b| {
            let hit = if refresh { None } else { pageinfo::cached(conn, &b.url) };
            match hit {
                Some(info) => AnalysisRow {
                    id: b.id,
                    url: b.url.clone(),
                    old_name: b.title.clone(),
                    name: info.name,
                    summary: info.summary,
                    source: format!("{}·缓存", info.source),
                    duplicate: false,
                    keep: true,
                    queued: false,
                },
                None => AnalysisRow {
                    id: b.id,
                    url: b.url.clone(),
                    old_name: b.title.clone(),
                    name: String::new(),
                    summary: String::new(),
                    source: String::new(),
                    duplicate: false,
                    keep: true,
                    queued: true,
                },
            }
        })
        .collect()
}

/// 抓回来的资料 + 本轮没排到抓取的条目 id（额度用完了）
#[derive(Default)]
pub struct Gathered {
    pub infos: Vec<(i64, PageInfo)>,
    pub queued: Vec<i64>,
}

/// 逐条抓（串行，不打爆对方站点）。`rows` 里没判出来的才会被真请求，
/// GitHub 和网页各自有本轮上限，排不进来的记在 queued 里，下一轮接着抓。
pub async fn gather(rows: &[AnalysisRow], items: &[Bookmark]) -> Gathered {
    let mut got = Gathered::default();
    let mut github_left = pageinfo::GITHUB_BUDGET;
    let mut page_left = pageinfo::PAGE_BUDGET;

    for row in rows.iter().filter(|r| r.name.is_empty()) {
        let Some(item) = items.iter().find(|b| b.id == row.id) else { continue };
        let is_github = pageinfo::looks_like_github(&row.url);
        let left = if is_github { github_left } else { page_left };
        if left == 0 {
            got.queued.push(row.id);
            continue;
        }
        if is_github {
            github_left -= 1;
        } else {
            page_left -= 1;
        }
        match pageinfo::gather(&item.url).await {
            pageinfo::Outcome::Found(info) => got.infos.push((row.id, info)),
            // 撞上限流就别再撞：这一路的本轮额度清零，剩下的算「没排到」，下轮再来
            pageinfo::Outcome::Throttled => {
                got.queued.push(row.id);
                if is_github {
                    github_left = 0;
                }
            }
            pageinfo::Outcome::Nothing => {}
        }
    }
    got
}

pub fn cache(conn: &Connection, items: &[Bookmark], fetched: &[(i64, PageInfo)]) -> usize {
    let mut saved = 0;
    for (id, info) in fetched {
        let Some(item) = items.iter().find(|b| b.id == *id) else { continue };
        if pageinfo::store(conn, &item.url, info).is_ok() {
            saved += 1;
        }
    }
    saved
}

const SYSTEM: &str = "你在给浏览器书签补显示名和简介。\n\
每条给一个不超过 20 个字的名称，和一句不超过 50 个字的简介。\n\
同一个域名下不同页面的名称和简介必须各不相同：名称里留住能认出这个页面的关键词\
（文章标题、仓库名、文档章节、栏目名），不要整批写成站点名。\n\
只知道域名而看不出页面内容时，简介留空字符串，不要编。\n\
不要解释、不要思考过程，直接输出 JSON 数组本身，形如 \
[{\"id\":1,\"name\":\"掘金·前端运行时\",\"summary\":\"一个前端性能话题的合集页\"}]。";

/// 抓不到的那些交给模型，一次问完
pub async fn ask(
    target: &ChatTarget,
    rows: &[AnalysisRow],
    items: &[Bookmark],
) -> AppResult<Vec<(i64, PageInfo)>> {
    let need: Vec<&AnalysisRow> = rows.iter().filter(|r| r.name.is_empty()).collect();
    if need.is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<serde_json::Value> = need
        .iter()
        .map(|r| {
            let title = items.iter().find(|b| b.id == r.id).map(|b| b.title.as_str()).unwrap_or("");
            serde_json::json!({ "id": r.id, "现有名称": title, "地址": r.url })
        })
        .collect();
    let prompt = format!("要判断的书签：{}", serde_json::to_string(&list).unwrap_or_default());
    let tag = format!("书签分析 {}条", need.len());

    // 这条 prompt 每条都带网址，正文不写进调用日志
    let text = categorize::chat(target, SYSTEM, &prompt, &tag, need.len(), true).await?;
    let rows_json = categorize::parse_json_array(&text)
        .ok_or_else(|| AppError::msg(format!("书签分析的回复读不出数组：{}", categorize::head(&text, 80))))?;

    let mut out = Vec::new();
    for value in rows_json {
        let Some(id) = value.get("id").and_then(|v| v.as_i64()) else { continue };
        let name = value.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if name.is_empty() {
            continue;
        }
        let summary = value.get("summary").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        out.push((
            id,
            PageInfo {
                name: pageinfo::fit(&name, pageinfo::NAME_CHARS),
                summary: pageinfo::fit(&summary, pageinfo::SUMMARY_CHARS),
                source: "model".into(),
            },
        ));
    }
    Ok(out)
}

/// 把抓到的和模型给的填回行里
pub fn fill(rows: &mut [AnalysisRow], infos: &[(i64, PageInfo)]) {
    for row in rows.iter_mut() {
        if let Some((_, info)) = infos.iter().find(|(id, _)| *id == row.id) {
            row.name = info.name.clone();
            row.summary = info.summary.clone();
            row.source = info.source.clone();
            // 抓到东西了，就不算「排队等抓」了；抓了但没内容的保持原样
            if !row.name.is_empty() {
                row.queued = false;
            }
        }
    }
}

/// 同地址的多条要标出来：保留创建最早的那条，其余默认勾掉待删
pub fn mark_duplicates(conn: &Connection, rows: &mut [AnalysisRow]) -> AppResult<usize> {
    let groups = bookmarks::duplicate_groups(conn)?;
    let mut extra = 0;
    for group in groups {
        for row in rows.iter_mut().filter(|r| group.contains(&r.id)) {
            row.duplicate = true;
            row.keep = Some(row.id) == group.first().copied();
            if !row.keep {
                extra += 1;
            }
        }
    }
    Ok(extra)
}

/// 应用：改名称和简介，删掉勾了要清的重复条目（整行先搬进回收表，删错能拿回来）。
/// 一个事务，别留下改了一半的状态。回 (改动数, 删除数)。
pub fn apply(
    conn: &Connection,
    updates: &[AnalysisUpdate],
    delete_ids: &[i64],
) -> AppResult<(usize, usize)> {
    conn.execute_batch("BEGIN")?;
    let done = (|| -> AppResult<(usize, usize)> {
        let mut changed = 0;
        for update in updates {
            let name = update.name.trim();
            if name.is_empty() {
                continue;
            }
            conn.execute(
                "UPDATE bookmarks SET title = ?1, description = ?2 WHERE id = ?3",
                rusqlite::params![name, empty_to_none(update.summary.trim()), update.id],
            )?;
            changed += 1;
        }
        let mut deleted = 0;
        // 只允许删「同一个地址确实还有别条在库里」的行：这是真删数据的口子，
        // 不能只信前端勾了什么 —— 万一传进来的是唯一一条，删掉就是丢书签。
        let mut still_there = url_counts(conn)?;
        for id in delete_ids {
            let Some(key) = key_of(conn, *id)? else { continue };
            let Some(count) = still_there.get_mut(&key) else { continue };
            if *count <= 1 {
                continue;
            }
            let moved = conn.execute(
                "INSERT OR REPLACE INTO bookmark_trash
                 (id, title, url, description, category_id, source, deleted_at)
                 SELECT id, title, url, description, category_id, source, ?2 FROM bookmarks WHERE id = ?1",
                rusqlite::params![id, Utc::now().timestamp()],
            )?;
            if moved > 0 {
                deleted += conn.execute("DELETE FROM bookmarks WHERE id = ?1", rusqlite::params![id])?;
                *count -= 1;
            }
        }
        Ok((changed, deleted))
    })();
    match done {
        Ok(counts) => {
            crate::db::commit(conn)?;
            Ok(counts)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// 回收里有多少条，网页管理顶部提示用
pub fn trash_count(conn: &Connection) -> AppResult<usize> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM bookmark_trash", [], |r| r.get(0))?;
    Ok(n as usize)
}

/// 撤销那次删除：整行放回 bookmarks（新的 id），回收表清空。
/// 放回来就会再次被识别成重复地址，界面上会重新标出来，这是正常的。
pub fn restore_trash(conn: &Connection) -> AppResult<usize> {
    conn.execute_batch("BEGIN")?;
    let done = (|| -> AppResult<usize> {
        let n = conn.execute(
            "INSERT INTO bookmarks (title, url, description, category_id, source)
             SELECT title, url, description, category_id, source FROM bookmark_trash",
            [],
        )?;
        conn.execute("DELETE FROM bookmark_trash", [])?;
        Ok(n)
    })();
    match done {
        Ok(n) => {
            crate::db::commit(conn)?;
            Ok(n)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// 确认过了，回收表不要了
pub fn empty_trash(conn: &Connection) -> AppResult<usize> {
    Ok(conn.execute("DELETE FROM bookmark_trash", [])?)
}

/// 每个地址键当前还有几条
fn url_counts(conn: &Connection) -> AppResult<std::collections::HashMap<String, i64>> {
    let mut map = std::collections::HashMap::new();
    let mut stmt = conn.prepare("SELECT url FROM bookmarks")?;
    for row in stmt.query_map([], |r| r.get::<_, String>(0))?.flatten() {
        *map.entry(url_key(&row)).or_insert(0) += 1;
    }
    Ok(map)
}

fn key_of(conn: &Connection, id: i64) -> AppResult<Option<String>> {
    use rusqlite::OptionalExtension;
    Ok(conn
        .query_row("SELECT url FROM bookmarks WHERE id = ?1", rusqlite::params![id], |r| r.get::<_, String>(0))
        .optional()?
        .map(|u| url_key(&u)))
}

fn empty_to_none(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute(
            "CREATE TABLE bookmarks (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, url TEXT NOT NULL, description TEXT, category_id INTEGER, source TEXT DEFAULT 'manual', created_at DATETIME DEFAULT CURRENT_TIMESTAMP)",
            [],
        )
        .unwrap();
        c.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT)", [])
            .unwrap();
        c.execute(
            "CREATE TABLE bookmark_trash (id INTEGER PRIMARY KEY, title TEXT NOT NULL, url TEXT NOT NULL, description TEXT, category_id INTEGER, source TEXT, deleted_at INTEGER NOT NULL)",
            [],
        )
        .unwrap();
        c
    }

    fn row(id: i64, url: &str) -> AnalysisRow {
        AnalysisRow {
            id,
            url: url.into(),
            old_name: "旧名".into(),
            name: String::new(),
            summary: String::new(),
            source: String::new(),
            duplicate: false,
            keep: true,
            queued: true,
        }
    }

    fn bookmark(id: i64, url: &str) -> Bookmark {
        Bookmark {
            id,
            title: format!("书签{id}"),
            url: url.into(),
            description: None,
            category_id: None,
            source: "edge".into(),
            created_at: None,
            use_count: 0,
        }
    }

    #[test]
    fn filled_rows_stop_being_queued() {
        let mut rows = vec![row(1, "https://a.cn/1"), row(2, "https://b.cn/2")];
        assert!(rows.iter().all(|r| r.queued), "新起的行都算等抓");

        fill(
            &mut rows,
            &[(1, PageInfo { name: "有名字".into(), summary: String::new(), source: "page".into() })],
        );

        assert!(!rows[0].queued, "抓到东西了就不该再标等抓");
        assert!(rows[1].queued, "没抓到的那条要继续等下一轮");
    }

    #[test]
    fn deleted_duplicates_can_be_restored() {
        let c = conn();
        c.execute("INSERT INTO bookmarks (id, title, url, description, category_id, source) VALUES (1, '留着', 'https://x.cn/p', NULL, 5, 'edge'), (2, '重复的', 'https://x.cn/p/', '旧简介', NULL, 'edge')", []).unwrap();

        let (changed, deleted) = apply(&c, &[], &[2]).unwrap();
        assert_eq!((changed, deleted), (0, 1));
        assert_eq!(trash_count(&c).unwrap(), 1);
        let left: i64 = c.query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 1, "被删的那条真的没了");

        // 放回原表并清空回收：整行的标题、简介、分类都带回来
        assert_eq!(restore_trash(&c).unwrap(), 1);
        assert_eq!(trash_count(&c).unwrap(), 0);
        let back = c.query_row("SELECT title, description, category_id, source FROM bookmarks WHERE title = '重复的'", [], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<String>>(3)?))
        }).unwrap();
        assert_eq!(back, ("重复的".to_string(), Some("旧简介".to_string()), None, Some("edge".to_string())), "{back:?}");
    }

    #[test]
    fn deleting_a_missing_row_trashes_nothing() {
        let c = conn();
        let (_, deleted) = apply(&c, &[], &[999]).unwrap();
        assert_eq!(deleted, 0);
        assert_eq!(trash_count(&c).unwrap(), 0, "库里没有的行不该在回收里冒出一条");
    }

    #[test]
    fn emptying_trash_is_final_and_only_touches_trash() {
        let c = conn();
        c.execute("INSERT INTO bookmarks (id, title, url) VALUES (1, 'a', 'https://x.cn/1'), (2, 'b', 'https://x.cn/1/')", []).unwrap();
        apply(&c, &[], &[2]).unwrap();
        assert_eq!(empty_trash(&c).unwrap(), 1);
        assert_eq!(trash_count(&c).unwrap(), 0);
        assert_eq!(empty_trash(&c).unwrap(), 0, "再清一次不该报错");
    }

    /// 删的是唯一一条时，前端勾了也不许删 —— 真删数据的口子不能只信界面
    #[test]
    fn the_last_copy_of_a_url_is_not_deletable() {
        let c = conn();
        c.execute("INSERT INTO bookmarks (id, title, url) VALUES (1, '唯一', 'https://only.cn/p')", []).unwrap();

        let (changed, deleted) = apply(&c, &[], &[1]).unwrap();
        assert_eq!((changed, deleted), (0, 0), "唯一一条被删掉了");
        assert_eq!(trash_count(&c).unwrap(), 0);
        let left: i64 = c.query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 1);
    }

    /// 一对重复里勾了两次也只该删一条，留下的那条是底线
    #[test]
    fn a_pair_shrinks_to_one_not_zero() {
        let c = conn();
        c.execute("INSERT INTO bookmarks (id, title, url) VALUES (1, 'a', 'https://x.cn/p'), (2, 'b', 'https://x.cn/p/')", []).unwrap();

        let (_, deleted) = apply(&c, &[], &[1, 2]).unwrap();
        assert_eq!(deleted, 1, "把同一地址的两条都删了");
        let left: i64 = c.query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 1);
    }

    #[test]
    fn cached_pages_are_used_without_a_request() {
        let c = conn();
        pageinfo::store(
            &c,
            "https://gitee.com/a/b",
            &PageInfo { name: "a/b".into(), summary: "一个仓库".into(), source: "page".into() },
        )
        .unwrap();

        let rows = from_cache(&c, &[bookmark(1, "https://GITEE.com/a/b/"), bookmark(2, "https://x.cn/y")], false);
        assert!(from_cache(&c, &[bookmark(1, "https://gitee.com/a/b")], true)[0].name.is_empty(), "勾了重抓就该跳过缓存");

        assert_eq!(rows[0].name, "a/b", "大小写和结尾斜杠不同的同一地址该命中缓存");
        assert!(rows[0].source.contains("缓存"), "{}", rows[0].source);
        assert_eq!(rows[1].name, "", "没缓存的不能被当成判过了");
    }

    #[test]
    fn duplicates_keep_the_earliest_row() {
        let c = conn();
        c.execute("INSERT INTO bookmarks (id, title, url, created_at) VALUES (1, 'a', 'https://x.cn/p', '2026-01-01'), (2, 'b', 'https://x.cn/p/', '2026-02-01'), (3, 'c', 'https://y.cn/q', '2026-01-01')", []).unwrap();
        let mut rows = vec![row(1, "https://x.cn/p"), row(2, "https://x.cn/p/"), row(3, "https://y.cn/q")];

        assert_eq!(mark_duplicates(&c, &mut rows).unwrap(), 1, "只该多出一条待删");
        assert!(rows[0].duplicate && rows[0].keep);
        assert!(rows[1].duplicate && !rows[1].keep);
        assert!(!rows[2].duplicate);
    }

    #[test]
    fn apply_writes_name_and_summary_and_deletes_the_ones_ticked() {
        let c = conn();
        c.execute("INSERT INTO bookmarks (id, title, url) VALUES (1, '旧', 'https://x.cn/p'), (2, '旧', 'https://x.cn/p/')", []).unwrap();

        let (changed, deleted) = apply(
            &c,
            &[AnalysisUpdate { id: 1, name: "  某页面  ".into(), summary: "  ".into() }],
            &[2],
        )
        .unwrap();

        assert_eq!((changed, deleted), (1, 1));
        let (title, desc): (String, Option<String>) = c
            .query_row("SELECT title, description FROM bookmarks WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(title, "某页面");
        assert_eq!(desc, None, "空简介要存成 NULL，别留一个空串");
        let left: i64 = c.query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 1);
    }

    #[test]
    fn an_empty_name_writes_nothing_at_all() {
        let c = conn();
        c.execute("INSERT INTO bookmarks (id, title, url) VALUES (1, '旧', 'https://x.cn/p')", []).unwrap();
        let (changed, deleted) = apply(
            &c,
            &[AnalysisUpdate { id: 1, name: "   ".into(), summary: "有简介也没用".into() }],
            &[],
        )
        .unwrap();
        assert_eq!((changed, deleted), (0, 0));
        let title: String = c.query_row("SELECT title FROM bookmarks WHERE id = 1", params![], |r| r.get(0)).unwrap();
        assert_eq!(title, "旧");
    }

    /// 模型的回复形状五花八门：少一条、多一条都不该连累同批别的条目
    #[test]
    fn model_reply_is_read_leniently() {
        let text = r#"好的：
```json
[{"id":1,"name":"掘金·前端运行时","summary":"一个前端性能话题的合集页"},{"name":"没有 id 的丢掉"},{"id":2,"name":"  ","summary":"空的也丢"}]
```"#;
        let parsed = categorize::parse_json_array(text).unwrap();
        let kept: Vec<i64> = parsed
            .iter()
            .filter_map(|v| {
                let id = v.get("id")?.as_i64()?;
                let name = v.get("name")?.as_str()?.trim();
                (!name.is_empty()).then_some(id)
            })
            .collect();
        assert_eq!(kept, vec![1], "{parsed:?}");
    }
}
