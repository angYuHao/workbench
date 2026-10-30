import type { FileNode } from "../../types";
import type { SortKey } from "../../stores/files";

/** 中文按拼音排，`1.jpg / 2.jpg / 10.jpg` 按数值排 */
const collator = new Intl.Collator("zh-Hans-CN", { numeric: true, sensitivity: "base" });

export function sortNodes(nodes: FileNode[], key: SortKey, desc: boolean): FileNode[] {
  const dir = desc ? -1 : 1;
  return [...nodes].sort((a, b) => {
    // 目录永远排在文件前面，换哪一列都不打乱这一层（资源管理器也是这个口径）
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    if (key === "size") return dir * ((a.size ?? 0) - (b.size ?? 0));
    if (key === "modified") return dir * ((a.modifiedAt ?? 0) - (b.modifiedAt ?? 0));
    return dir * collator.compare(a.name, b.name);
  });
}

/** 一个搜索词同时搜名称、别名、说明，再按那一列的排序摆好 */
export function visibleNodes(col: { nodes: FileNode[]; keyword: string; sort: SortKey; sortDesc: boolean }): FileNode[] {
  const k = col.keyword.trim().toLowerCase();
  const rows = k
    ? col.nodes.filter(
        (n) =>
          n.name.toLowerCase().includes(k) ||
          (n.alias ?? "").toLowerCase().includes(k) ||
          (n.description ?? "").toLowerCase().includes(k),
      )
    : col.nodes;
  return sortNodes(rows, col.sort, col.sortDesc);
}
