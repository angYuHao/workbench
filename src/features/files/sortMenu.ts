import type { MenuEntry } from "../../ui";
import type { SortKey } from "../../stores/files";

/** 资源管理器那张排序单子的形状：上面一组排序键（圆点标当前键），
    一条分隔线，下面递增/递减单独一组（圆点标当前方向）。工具栏下拉和右键共用。 */
export function sortMenuEntries(
  sort: SortKey,
  desc: boolean,
  pick: (key: SortKey, desc: boolean) => void,
): MenuEntry[] {
  const key = (k: SortKey, label: string): MenuEntry => ({
    key: k,
    label,
    checked: sort === k,
    onClick: () => pick(k, desc),
  });
  return [
    key("name", "名称"),
    key("modified", "修改日期"),
    key("size", "大小"),
    { key: "sep-dir", label: "", divider: true },
    { key: "asc", label: "递增", checked: !desc, onClick: () => pick(sort, false) },
    { key: "desc", label: "递减", checked: desc, onClick: () => pick(sort, true) },
  ];
}