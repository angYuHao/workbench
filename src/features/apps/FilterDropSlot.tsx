import { useState } from "react";
import { Filter } from "lucide-react";
import { addAppFilter } from "../../api/desktop";
import { useDesktopStore } from "../../stores/desktop";
import { carriesItem, draggedItemId } from "../../shared/dragItem";
import { stripExt } from "../../utils/fileKind";
import { message } from "../../ui";
import type { AppEntry } from "../../types";

/**
 * 拖一张卡片到这个落点上：把这个名字加进过滤名单，同时删掉卡片。
 * 模式取去扩展名的显示名（`Loomy.lnk` → `Loomy`），装到新版本也还认得；
 * 名单本身在 设置 → 应用过滤 里能改能删，也能试算它到底挡了谁。
 */
export default function FilterDropSlot({ apps }: { apps: AppEntry[] }) {
  const { removeApp } = useDesktopStore();
  const [over, setOver] = useState(false);

  const dropToFilter = async (id: number) => {
    const app = apps.find((a) => a.id === id);
    if (!app) return;
    const pattern = stripExt(app.name).trim();
    if (!pattern) {
      message.error("这张卡片的名字是空的，过滤了也挡不住东西");
      return;
    }
    try {
      const added = await addAppFilter(pattern);
      await removeApp(app.id);
      message.success(
        added
          ? `已过滤「${pattern}」：卡片移除了，以后同步不再收它（想改想去 设置 → 应用过滤）`
          : `「${pattern}」本来就在过滤名单里，卡片移除了`
      );
    } catch (e) {
      message.error(String(e));
    }
  };

  return (
    <div className="apps-filter-slot">
      <div
        className={over ? "filter-drop on" : "filter-drop"}
        aria-label="拖到此处过滤"
        title="把卡片拖到这里：加入过滤名单，同时移除这张卡片（名单在 设置 → 应用过滤 里能改能删）"
        onDragOver={(e) => {
          // 拖过来的只认自家卡片；从资源管理器拖文件进来仍由 App.tsx 那层吞掉
          if (!carriesItem(e)) return;
          e.preventDefault();
          // 卡片那边 effectAllowed 写的是 move，这里必须也用 move：
          // 两个不一致时 Chrome 直接把这次拖动判成 none，drop 根本不会发出来
          e.dataTransfer.dropEffect = "move";
          if (!over) setOver(true);
        }}
        onDragLeave={() => setOver(false)}
        onDrop={(e) => {
          if (!carriesItem(e)) return;
          e.preventDefault();
          setOver(false);
          const id = draggedItemId(e);
          if (id !== null) void dropToFilter(id);
        }}
      >
        <Filter size={22} strokeWidth={1.6} />
      </div>
    </div>
  );
}
