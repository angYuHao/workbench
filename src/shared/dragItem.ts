/** 拖动条目时挂在 dataTransfer 上的键：卡片拖到分类名上就是改分类，拖到「拖来过滤」上就是加过滤名单 */
export const ITEM_MIME = "application/x-workbench-item";

/** 拖来的得是自家的条目；选中的文字、从资源管理器拖进来的文件都不当落点 */
export function carriesItem(e: React.DragEvent): boolean {
  return e.dataTransfer.types.includes(ITEM_MIME);
}

/** 从 drop 事件里取出被拖条目的 id：不是自家载荷、或者 id 不合法就回 null */
export function draggedItemId(e: React.DragEvent): number | null {
  const id = Number(e.dataTransfer.getData(ITEM_MIME));
  return Number.isInteger(id) && id > 0 ? id : null;
}
