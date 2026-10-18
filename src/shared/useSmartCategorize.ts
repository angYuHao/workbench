import { useState } from "react";
import { applySmartCategorize, smartCategorize } from "../api/smart";
import { getAiModels } from "../api/aiModels";
import { getSettings, setSetting } from "../api/settings";
import { message } from "../ui";
import type { AiModel, SmartCategorizeItem, SmartCategorizeResult } from "../types";

export interface SmartCategorize {
  open: boolean;
  loading: boolean;
  results: SmartCategorizeResult[];
  models: AiModel[];
  modelId: number | null;
  categoryNames: string[];
  /** 打开确认窗口，先把待分类条目存着，模型由窗口里选完再发 */
  start: (items: SmartCategorizeItem[]) => void;
  close: () => void;
  generate: () => Promise<void>;
  setModel: (id: number) => void;
  suggest: (id: number, category: string) => void;
  apply: () => Promise<void>;
}

interface Options {
  itemType: "app" | "bookmark";
  /** 结果提示用的量词，如「程序」「网址」 */
  unit: string;
  /** 现有分类名：模型只会优先复用它们，不够就新起一个 */
  categoryNames: string[];
  /** 应用分类成功后刷新列表 */
  onApplied: () => Promise<void>;
}

export function useSmartCategorize({ itemType, unit, categoryNames, onApplied }: Options): SmartCategorize {
  const [items, setItems] = useState<SmartCategorizeItem[]>([]);
  const [results, setResults] = useState<SmartCategorizeResult[]>([]);
  const [models, setModels] = useState<AiModel[]>([]);
  const [modelId, setModelId] = useState<number | null>(null);
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);

  const start = (next: SmartCategorizeItem[]) => {
    if (!next.length) return;
    setItems(next);
    setResults([]);
    setOpen(true);
    // 每次打开都重新取：设置里改了模型名、加了新配置，不该等页面重进才看得见
    Promise.all([getAiModels(), getSettings()])
      .then(([list, settings]) => {
        setModels(list);
        setModelId((prev) => {
          // 还选得动就沿用，被改名删掉了就退回上次记住的那个，再退回列表第一个
          if (prev && list.some((m) => m.id === prev)) return prev;
          const remembered = Number(settings.smart_model_id);
          return list.some((m) => m.id === remembered) ? remembered : list[0]?.id ?? null;
        });
      })
      .catch((e: unknown) => message.error(String(e)));
  };

  const setModel = (id: number) => {
    setModelId(id);
    // 记住选过哪个只是偏好，写库失败不该打扰这次分类
    setSetting("smart_model_id", String(id)).catch(() => {});
  };

  const generate = async () => {
    if (!items.length) return;
    if (!models.length) {
      message.error("还没有可用的模型，先去 设置 → 模型 添加一个");
      return;
    }
    setLoading(true);
    try {
      setResults(await smartCategorize(items, itemType, modelId));
    } catch (e) {
      message.error(String(e));
    } finally {
      setLoading(false);
    }
  };

  const suggest = (id: number, category: string) =>
    setResults((prev) => prev.map((r) => (r.id === id ? { ...r, suggestedCategory: category } : r)));

  const apply = async () => {
    const updates = results
      .filter((r) => r.suggestedCategory.trim())
      .map((r) => ({ id: r.id, categoryName: r.suggestedCategory.trim(), reason: r.reason }));
    if (!updates.length) {
      message.error("还没有可应用的分类，先让模型建议一轮或自己填上");
      return;
    }
    try {
      const count = await applySmartCategorize(itemType, updates);
      const skipped = results.length - updates.length;
      setOpen(false);
      message.success(
        skipped ? `已分类 ${count} 个${unit}，${skipped} 条没选分类已跳过` : `已分类 ${count} 个${unit}`
      );
      await onApplied();
    } catch (e) {
      message.error(String(e));
    }
  };

  return {
    open,
    loading,
    results,
    models,
    modelId,
    categoryNames,
    start,
    close: () => setOpen(false),
    generate,
    setModel,
    suggest,
    apply,
  };
}
