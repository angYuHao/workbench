import { invoke } from "@tauri-apps/api/core";
import type { BlacklistEntry, Bookmark, BookmarkAnalysisRow, BookmarkAnalysisUpdate, ImportResult } from "../types";

export function importBookmarks(browser: string) {
  return invoke<ImportResult>("import_bookmarks", { browser });
}

export function getBookmarks() {
  return invoke<Bookmark[]>("get_bookmarks");
}

export function saveBookmark(bookmark: Bookmark) {
  return invoke<Bookmark>("save_bookmark", { bookmark });
}

export function deleteBookmark(id: number) {
  return invoke<void>("delete_bookmark", { id });
}

/** id 只用于计一次使用；不带 id 就只是开网页，不动任何书签的计数 */
export function openUrl(url: string, id?: number) {
  return invoke<void>("open_url", { url, id: id ?? null });
}

/** 智能分析：先吃本机缓存，再抓网页/GitHub API，抓不到的交给模型。refresh 为真就跳过缓存重抓 */
export function analyzeBookmarks(ids: number[], modelId: number | null, refresh = false) {
  return invoke<BookmarkAnalysisRow[]>("analyze_bookmarks", { ids, modelId, refresh });
}

/** 应用分析结果：回 [改动条数, 删除条数] */
export function applyBookmarkAnalysis(updates: BookmarkAnalysisUpdate[], deleteIds: number[]) {
  return invoke<[number, number]>("apply_bookmark_analysis", { updates, deleteIds });
}

export function getPageCacheCount() {
  return invoke<number>("get_page_cache_count");
}

/** 清掉本机存的网页信息，回被清的条数 */
export function clearPageCache() {
  return invoke<number>("clear_page_cache");
}

/** 回收表里被分析删掉的重复书签条数 */
/** 黑名单：命中的不在页面上显示，导入收藏夹时也会跳过（不删数据，取消拉黑就回来） */
export function getBookmarkBlacklist() {
  return invoke<BlacklistEntry[]>("get_bookmark_blacklist");
}

export function addBookmarkBlacklist(pattern: string) {
  return invoke<BlacklistEntry>("add_bookmark_blacklist", { pattern });
}

export function removeBookmarkBlacklist(pattern: string) {
  return invoke<number>("remove_bookmark_blacklist", { pattern });
}

/** 多选拉黑：只加选中条目的精确地址，回真加进去几条 */
export function blockBookmarks(ids: number[]) {
  return invoke<number>("block_bookmarks", { ids });
}

export function getBookmarkTrashCount() {
  return invoke<number>("get_bookmark_trash_count");
}

/** 撤销那次删除，整行放回书签表 */
export function restoreBookmarkTrash() {
  return invoke<number>("restore_bookmark_trash");
}

export function emptyBookmarkTrash() {
  return invoke<number>("empty_bookmark_trash");
}