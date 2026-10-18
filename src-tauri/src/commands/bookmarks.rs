use tauri::State;

use crate::db::AppState;
use crate::models::{Bookmark, ImportResult};
use crate::services::bookmark_analysis::{self, AnalysisRow, AnalysisUpdate};
use crate::services::{ai::models, bookmarks};

/// 书签智能分析：先吃本机缓存，再按站点抓网页（GitHub 走 API），抓不到的才交给模型。
/// async 是有意的：一轮可能几十次网页请求外加一次模型调用，同步命令会跑在主线程上把界面钉住。
/// 连接只在取书签、写缓存、标重复这三步各开一小段，等网络的时候放锁。
/// 模型挂了不影响已经抓到的结果：那几条留空并标出原因，别为了一个失败把整轮丢掉。
#[tauri::command]
pub async fn analyze_bookmarks(
    state: State<'_, AppState>,
    ids: Vec<i64>,
    model_id: Option<i64>,
    refresh: Option<bool>,
) -> Result<Vec<AnalysisRow>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let refresh = refresh.unwrap_or(false);
    let (items, mut rows) = {
        let conn = state.conn();
        let items = bookmarks::get_bookmarks(&conn)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|b| ids.contains(&b.id))
            .collect::<Vec<_>>();
        let rows = bookmark_analysis::from_cache(&conn, &items, refresh);
        (items, rows)
    };

    let fetched = bookmark_analysis::gather(&rows, &items).await;
    if !fetched.infos.is_empty() {
        let conn = state.conn();
        bookmark_analysis::cache(&conn, &items, &fetched.infos);
        bookmark_analysis::fill(&mut rows, &fetched.infos);
    }
    // 本轮额度没排到的那些保持 queued=true：模型可能先猜一个，但下轮抓到真的会顶掉
    for id in &fetched.queued {
        if let Some(row) = rows.iter_mut().find(|r| r.id == *id) {
            row.queued = true;
        }
    }

    if rows.iter().any(|r| r.name.is_empty()) {
        let target = {
            let conn = state.conn();
            models::target_for(&conn, model_id)
        }
        .map_err(|e| e.to_string())?;
        match bookmark_analysis::ask(&target, &rows, &items).await {
            Ok(by_model) => {
                // 模型给的判断也缓存起来：同一条地址下次不再花 token
                let conn = state.conn();
                bookmark_analysis::cache(&conn, &items, &by_model);
                bookmark_analysis::fill(&mut rows, &by_model);
            }
            Err(e) => {
                for row in rows.iter_mut().filter(|r| r.name.is_empty()) {
                    row.source = format!("模型没接上：{}", e);
                }
            }
        }
    }

    {
        let conn = state.conn();
        bookmark_analysis::mark_duplicates(&conn, &mut rows).map_err(|e| e.to_string())?;
    }
    Ok(rows)
}

/// 应用分析结果：改名称和简介，删掉勾了要清的重复条目；一个事务，回 (改动数, 删除数)
#[tauri::command]
pub fn apply_bookmark_analysis(
    state: State<AppState>,
    updates: Vec<AnalysisUpdate>,
    delete_ids: Vec<i64>,
) -> Result<(usize, usize), String> {
    let conn = state.conn();
    bookmark_analysis::apply(&conn, &updates, &delete_ids).map_err(|e| e.to_string())
}

/// 黑名单：命中的地址不在页面上显示，导入收藏夹时也会跳过。
/// kind 分 url（多选拉黑的精确地址）和 domain（手输的域名，连子域一起挡）
#[tauri::command]
pub fn get_bookmark_blacklist(state: State<AppState>) -> Result<Vec<crate::services::bookmarks::BlacklistEntry>, String> {
    let conn = state.conn();
    bookmarks::list_blacklist(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_bookmark_blacklist(
    state: State<AppState>,
    pattern: String,
) -> Result<crate::services::bookmarks::BlacklistEntry, String> {
    let conn = state.conn();
    bookmarks::add_blacklist(&conn, &pattern).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_bookmark_blacklist(state: State<AppState>, pattern: String) -> Result<usize, String> {
    let conn = state.conn();
    bookmarks::remove_blacklist(&conn, &pattern).map_err(|e| e.to_string())
}

/// 多选拉黑：把选中书签的精确地址加进黑名单，回真加进去几条
#[tauri::command]
pub fn block_bookmarks(state: State<AppState>, ids: Vec<i64>) -> Result<usize, String> {
    let conn = state.conn();
    bookmarks::block_bookmarks(&conn, &ids).map_err(|e| e.to_string())
}

/// 回收表里还有多少条被分析删掉的重复书签
#[tauri::command]
pub fn get_bookmark_trash_count(state: State<AppState>) -> Result<usize, String> {
    let conn = state.conn();
    bookmark_analysis::trash_count(&conn).map_err(|e| e.to_string())
}

/// 撤销那次删除：整行放回书签表
#[tauri::command]
pub fn restore_bookmark_trash(state: State<AppState>) -> Result<usize, String> {
    let conn = state.conn();
    bookmark_analysis::restore_trash(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn empty_bookmark_trash(state: State<AppState>) -> Result<usize, String> {
    let conn = state.conn();
    bookmark_analysis::empty_trash(&conn).map_err(|e| e.to_string())
}

/// 本机存了多少条网页信息，以及一把清掉：网页改版后重抓要用
#[tauri::command]
pub fn get_page_cache_count(state: State<AppState>) -> Result<usize, String> {
    let conn = state.conn();
    crate::services::ai::pageinfo::cache_count(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_page_cache(state: State<AppState>) -> Result<usize, String> {
    let conn = state.conn();
    crate::services::ai::pageinfo::clear_cache(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_bookmarks(state: State<AppState>, browser: String) -> Result<ImportResult, String> {
    let conn = state.conn();
    crate::services::bookmarks::import_bookmarks(&conn, &browser).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_bookmarks(state: State<AppState>) -> Result<Vec<Bookmark>, String> {
    let conn = state.conn();
    crate::services::bookmarks::get_bookmarks(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_bookmark(state: State<AppState>, bookmark: Bookmark) -> Result<Bookmark, String> {
    let conn = state.conn();
    crate::services::bookmarks::save_bookmark(&conn, &bookmark).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_bookmark(state: State<AppState>, id: i64) -> Result<(), String> {
    let conn = state.conn();
    crate::services::bookmarks::delete_bookmark(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_url(state: State<AppState>, url: String, id: Option<i64>) -> Result<(), String> {
    crate::win::shell::open(&url).map_err(|e| e.to_string())?;
    // 开成了才计数；id 只为计数而来，别处开一个网页不给任何书签加次数，
    // 计数自己写不进去也不回头拦用户
    if let Some(id) = id {
        let conn = state.conn();
        let _ = crate::services::usage::note_use(&conn, crate::services::usage::BOOKMARK, id);
    }
    Ok(())
}