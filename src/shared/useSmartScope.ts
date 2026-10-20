import { useMemo, useState } from "react";
import { message } from "../ui";
import type { SmartCategorizeItem } from "../types";

interface Options<T extends { categoryId: number | null }> {
  /** 页面上当前看得见的条目：换了分类或关键词，勾选的范围也跟着换 */
  items: T[];
  /** 现有分类，用来找「其他」那一类的 id */
  categories: { id: number; name: string }[];
  /** 量词：「网址」「程序」——退回全量时那句提示要说清判的是什么 */
  unit: string;
  /** 智能分类窗口的 start */
  start: (list: SmartCategorizeItem[]) => void;
  /** 条目 → 发给模型的那一行 */
  toSmart: (item: T) => SmartCategorizeItem;
  /** 进模型前的额外筛选（应用页只放程序，文档卡片不发模型） */
  eligible?: (item: T) => boolean;
}

/**
 * 「只处理新增」那一圈：只有还没归类的（含堆在「其他」里的）才发给模型，进了真分类的跳过。
 * 一个都不剩时退回全量并把勾摘掉——勾是画在弹窗里的，弹窗不开他就没地方取消。
 */
export function useSmartScope<T extends { categoryId: number | null }>({
  items,
  categories,
  unit,
  start,
  toSmart,
  eligible,
}: Options<T>) {
  const [onlyNew, setOnlyNew] = useState(true);
  const otherCategoryId = useMemo(
    () => categories.find((c) => c.name === "其他")?.id ?? null,
    [categories],
  );

  const targets = (scope: boolean) =>
    items
      .filter((it) => !eligible || eligible(it))
      .filter((it) => !scope || it.categoryId === null || it.categoryId === otherCategoryId)
      .map(toSmart);

  const restart = (scope: boolean) => {
    const list = targets(scope);
    if (list.length) {
      setOnlyNew(scope);
      start(list);
      return;
    }
    setOnlyNew(false);
    message.info(`按「只处理新增」没有要判的${unit}了：已去掉勾，连已经归好类的一起判`);
    start(targets(false));
  };

  return { onlyNew, restart };
}
