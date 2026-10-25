import { Pencil, Trash2 } from "lucide-react";
import { ITEM_MIME } from "../../shared/dragItem";
import { Space, pressable } from "../../ui";
import Favicon from "../../shared/Favicon";
import SelectTick from "../../shared/SelectTick";
import type { Bookmark } from "../../types";

/** 一张网址卡片：双击（或 Enter）打开，选择模式里单击是勾选，拖着可以丢到左边的分类上 */
export default function BookmarkCard({
  bookmark,
  selectMode,
  selected,
  onToggle,
  onOpen,
  onEdit,
  onDelete,
}: {
  bookmark: Bookmark;
  selectMode: boolean;
  selected: boolean;
  onToggle: () => void;
  onOpen: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <div
      className={
        selectMode && selected
          ? "card card-hover tile bookmark-card selected"
          : "card card-hover tile bookmark-card"
      }
      draggable
      // 按住拖到左边的分类上就是改分类；打开改成双击，单击只在选择模式里用来勾选
      // Enter 是键盘上等价的那一下双击（键盘没法双击）
      title="双击打开"
      aria-label={bookmark.title}
      onDragStart={(e) => {
        e.dataTransfer.setData(ITEM_MIME, String(bookmark.id));
        e.dataTransfer.effectAllowed = "move";
      }}
      onClick={() => selectMode && onToggle()}
      onDoubleClick={() => {
        if (selectMode) return;
        onOpen();
      }}
      {...pressable(() => {
        if (selectMode) {
          onToggle();
          return;
        }
        onOpen();
      })}
    >
      {selectMode && <SelectTick selected={selected} />}
      <Favicon url={bookmark.url} />
      <div style={{ flex: 1, minWidth: 0 }}>
        <div className="ellipsis" style={{ fontWeight: 600 }}>
          {bookmark.title}
          {bookmark.useCount > 0 && <span className="label"> · {bookmark.useCount} 次</span>}
        </div>
        <div className="ellipsis label">{bookmark.url}</div>
        {bookmark.description && (
          <div className="label" style={{ marginTop: 2 }}>
            {bookmark.description}
          </div>
        )}
      </div>
      {!selectMode && (
        <Space
          size={4}
          style={{ alignSelf: "flex-start" }}
          onClick={(e) => e.stopPropagation()}
          // 双击也拦在这里，不然在编辑按钮上按两下就把网址打开了
          onDoubleClick={(e) => e.stopPropagation()}
        >
          <Pencil className="icon-button" size={15} onClick={onEdit} />
          <Trash2 className="icon-button" size={15} onClick={onDelete} />
        </Space>
      )}
    </div>
  );
}
