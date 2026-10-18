import { useState } from "react";
import { analyzeBookmarks, applyBookmarkAnalysis } from "../../api/bookmarks";
import { getAiModels } from "../../api/aiModels";
import { getSettings, setSetting } from "../../api/settings";
import { message } from "../../ui";
import type { AiModel, BookmarkAnalysisRow, BookmarkAnalysisUpdate } from "../../types";

export interface BookmarkAnalysis {
  open: boolean;
  loading: boolean;
  rows: BookmarkAnalysisRow[];
  models: AiModel[];
  modelId: number | null;
  /** 勾了要清的重复条目 id */
  dropping: number[];
  /** 待分析的条数：窗口刚开时表还没有数据，先让他看清这批是多少 */
  pending: number;
  start: (ids: number[]) => void;
  close: () => void;
  /** 再跑一轮：抓过的走本机缓存，没排到的接着抓 */
  run: () => Promise<void>;
  /** 跳过本机缓存，重新抓一遍 */
  refresh: boolean;
  setRefresh: (value: boolean) => void;
  setModel: (id: number) => void;
  setName: (id: number, value: string) => void;
  setSummary: (id: number, value: string) => void;
  toggleDrop: (id: number) => void;
  apply: () => Promise<void>;
}

interface Options {
  /** 应用成功后刷新列表 */
  onApplied: () => Promise<void>;
}

export function useBookmarkAnalysis({ onApplied }: Options): BookmarkAnalysis {
  const [ids, setIds] = useState<number[]>([]);
  const [rows, setRows] = useState<BookmarkAnalysisRow[]>([]);
  const [models, setModels] = useState<AiModel[]>([]);
  const [modelId, setModelId] = useState<number | null>(null);
  const [dropping, setDropping] = useState<number[]>([]);
  const [refresh, setRefresh] = useState(false);
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);

  const start = (next: number[]) => {
    if (!next.length) return;
    setIds(next);
    setRows([]);
    setDropping([]);
    setOpen(true);
    // 开窗只取模型列表，不发分析请求：抓网页和问模型都要等他按「开始分析」，
    // 不然一进来就转圈，连选哪个模型、要不要忽略缓存都来不及改
    void (async () => {
      try {
        const [list, settings] = await Promise.all([getAiModels(), getSettings()]);
        const remembered = Number(settings.smart_model_id);
        const picked = list.some((m) => m.id === remembered) ? remembered : list[0]?.id ?? null;
        setModels(list);
        setModelId((prev) => (prev && list.some((m) => m.id === prev) ? prev : picked));
      } catch (e) {
        message.error(String(e));
      }
    })();
  };

  const run = async (target: number[] = ids, useModel: number | null = modelId) => {
    if (!target.length) return;
    setLoading(true);
    try {
      // 没配模型时后端自己报错，这里不再挡一道，界面至少能拿到缓存和抓取那部分
      const list = await analyzeBookmarks(target, useModel, refresh);
      setRows(list);
      setDropping(list.filter((r) => r.duplicate && !r.keep).map((r) => r.id));
    } catch (e) {
      message.error(String(e));
    } finally {
      setLoading(false);
    }
  };

  const setModel = (id: number) => {
    setModelId(id);
    // 和智能分类共用同一个「上次用的模型」，写库失败不该打扰这次分析
    setSetting("smart_model_id", String(id)).catch(() => {});
  };

  const patch = (id: number, changes: Partial<BookmarkAnalysisRow>) =>
    setRows((prev) => prev.map((r) => (r.id === id ? { ...r, ...changes } : r)));

  const toggleDrop = (id: number) =>
    setDropping((prev) => (prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]));

  const apply = async () => {
    const updates: BookmarkAnalysisUpdate[] = rows
      .filter((r) => r.name.trim())
      .map((r) => ({ id: r.id, name: r.name.trim(), summary: r.summary.trim() }));
    if (!updates.length && !dropping.length) {
      message.error("还没有可应用的内容：名称都空着，也没勾要删的重复项");
      return;
    }
    try {
      const [changed, deleted] = await applyBookmarkAnalysis(updates, dropping);
      setOpen(false);
      const blanks = rows.length - updates.length;
      message.success(
        `已更新 ${changed} 条` +
          (deleted ? `，删掉 ${deleted} 条重复` : "") +
          (blanks ? `，${blanks} 条名称空着没动` : "")
      );
      await onApplied();
    } catch (e) {
      message.error(String(e));
    }
  };

  return {
    open,
    loading,
    rows,
    models,
    modelId,
    dropping,
    pending: ids.length,
    refresh,
    setRefresh,
    start,
    close: () => setOpen(false),
    run: () => run(),
    setModel,
    setName: (id, value) => patch(id, { name: value }),
    setSummary: (id, value) => patch(id, { summary: value }),
    toggleDrop,
    apply,
  };
}
