import { Empty, Tooltip, message, pressable } from "../../ui";
import IconWithFallback from "../../shared/IconWithFallback";
import SelectTick from "../../shared/SelectTick";
import { ITEM_MIME } from "../../shared/CategoryTree";
import { launchApp } from "../../api/desktop";
import { stripExt } from "../../utils/fileKind";
import type { AppEntry } from "../../types";

interface Props {
  apps: AppEntry[];
  selectMode?: boolean;
  selectedIds?: Set<number>;
  onToggleSelect?: (id: number) => void;
  /** 启动完刷一遍列表：计数变了，卡片顺序和分类热度也得跟着变 */
  onLaunched?: () => void;
}

export default function AppIconGrid({ apps, selectMode, selectedIds, onToggleSelect, onLaunched }: Props) {
  if (apps.length === 0) {
    return <Empty description="还没收录到程序：点「重新同步」再读一遍（注册表 + 开始菜单），或用「添加程序」手动填" />;
  }

  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(auto-fill, minmax(104px, 1fr))",
        gap: 12,
      }}
    >
      {apps.map((app) => {
        const displayName = stripExt(app.name);
        const checked = selectedIds?.has(app.id) ?? false;
        const hint = [app.alias ?? app.description, app.useCount > 0 ? `用过 ${app.useCount} 次` : null]
          .filter(Boolean)
          .join(" · ");
        return (
          <Tooltip key={app.id} title={hint || "双击打开"} placement="bottom">
            <div
              className={
                selectMode && checked
                  ? "card card-hover tile app-tile selected"
                  : "card card-hover tile app-tile"
              }
              aria-label={displayName}
              draggable
              // 按住拖到左边的分类上就是改分类；打开改成双击，单击留给选择模式
              onDragStart={(e) => {
                e.dataTransfer.setData(ITEM_MIME, String(app.id));
                e.dataTransfer.effectAllowed = "move";
              }}
              onClick={() => selectMode && onToggleSelect?.(app.id)}
              onDoubleClick={() => {
                if (selectMode) return;
                launchApp(app.id)
                  .then(() => onLaunched?.())
                  .catch((e) => message.error(String(e)));
              }}
              /** 键盘没有「双击」这回事：Enter 就等于那一下双击 */
              {...pressable(() => {
                if (selectMode) {
                  onToggleSelect?.(app.id);
                  return;
                }
                launchApp(app.id)
                  .then(() => onLaunched?.())
                  .catch((e) => message.error(String(e)));
              })}
              // 卡片不放操作按钮：删除走顶部「选择」模式；右键只屏蔽，不挂菜单
              onContextMenu={(e) => e.preventDefault()}
            >
              {selectMode && <SelectTick selected={checked} />}
              <IconWithFallback name={displayName} iconPath={app.iconPath} size={46} />
              <div
                className="ellipsis"
                style={{
                  width: "100%",
                  fontSize: 12,
                  lineHeight: 1.3,
                  textAlign: "center",
                }}
              >
                {displayName}
              </div>
            </div>
          </Tooltip>
        );
      })}
    </div>
  );
}
