export interface Bookmark {
  id: number;
  title: string;
  url: string;
  description: string | null;
  categoryId: number | null;
  source: string;
  createdAt: string | null;
  /** 双击打开过几次：列表按它从多到少排，分类树按底下的总数排 */
  useCount: number;
}

/** 从浏览器导入收藏夹的结果统计 */
export interface ImportResult {
  total: number;
  imported: number;
  skipped: number;
}

/** 一条网址黑名单：url = 精确地址（多选拉黑），domain = 域名及其所有子域（手输） */
export interface BlacklistEntry {
  pattern: string;
  kind: string;
  createdAt: number;
}

/** 书签智能分析的一行：名称 + 简介 + 这条是从哪儿来的 */
export interface BookmarkAnalysisRow {
  id: number;
  url: string;
  oldName: string;
  name: string;
  summary: string;
  /** github / page / model / *·缓存 / 模型没接上…；空串表示还没判出来 */
  source: string;
  duplicate: boolean;
  keep: boolean;
  /** 本轮抓取额度没排到它：名字可能是模型猜的，再跑一轮能用真网页信息顶掉 */
  queued: boolean;
}

export interface BookmarkAnalysisUpdate {
  id: number;
  name: string;
  summary: string;
}
