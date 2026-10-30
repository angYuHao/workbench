import type { SortKey } from "../stores/files";

/** 排序按钮上循环的顺序：名称↑ → 名称↓ → 大小↑ → 大小↓ → 时间↑ → 时间↓ → 名称↑ */
export const SORT_ORDER: { key: SortKey; desc: boolean }[] = [
  { key: "name", desc: false },
  { key: "name", desc: true },
  { key: "size", desc: false },
  { key: "size", desc: true },
  { key: "modified", desc: false },
  { key: "modified", desc: true },
];

export function sortLabel(key: SortKey, desc: boolean): string {
  const name = key === "name" ? "名称" : key === "size" ? "大小" : "时间";
  return `${name} ${desc ? "↓" : "↑"}`;
}
