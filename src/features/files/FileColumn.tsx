import { useMemo } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { FileText, Folder, Pencil } from "lucide-react";
import { openPath } from "../../api/shell";
import { useFilesStore } from "../../stores/files";
import type { FileTab } from "../../stores/files";
import { visibleNodes } from "./listing";
import { formatBytes, formatWhen } from "../../utils/humanize";
import { message, pressable } from "../../ui";
import type { FileNode } from "../../types";

interface Props {
  panelId: string;
  tab: FileTab;
  index: number;
  focused: boolean;
  detail: boolean;
  icons: Record<string, string>;
  onEdit: (node: FileNode) => void;
  onMenu: (at: { x: number; y: number }) => void;
  onActivate: () => void;
}

function tipOf(node: FileNode): string {
  const lines = [node.path];
  if (node.alias) lines.push(`别名：${node.alias}`);
  if (node.description) lines.push(`说明：${node.description}`);
  if (!node.isDir) lines.push(formatBytes(node.size));
  if (node.modifiedAt) lines.push(`修改于 ${formatWhen(node.modifiedAt)}`);
  return lines.join("\n");
}

/** Miller 列里的一列：一个目录的全部内容。不带头行 —— 目录名看左边一栏选中的那项，
    空白处右键弹这一列的菜单，排序也在里面 */
export default function FileColumn({
  panelId,
  tab,
  index,
  focused,
  detail,
  icons,
  onEdit,
  onMenu,
  onActivate,
}: Props) {
  const col = tab.cols[index];
  const { pickIn, enterIn, focusColIn } = useFilesStore.getState();
  const visible = useMemo(() => (col ? visibleNodes(col) : []), [col]);
  const visiblePaths = useMemo(() => visible.map((n) => n.path), [visible]);
  const selected = col?.selected ?? [];

  if (!col) return null;

  /** 双击和 Enter 都走这里：目录就是把这个标签挪进去，文件交给系统程序打开 */
  const open = (node: FileNode) => {
    if (node.isDir) {
      void enterIn(panelId, node.path);
      return;
    }
    // 打不开现在会报错回来了（文件被删、没有关联程序），不接住就只是控制台里一条红字
    openPath(node.path).catch((e) => message.error(String(e)));
  };

  const focusHere = () => {
    onActivate();
    if (!focused) void focusColIn(panelId, index);
  };

  return (
    <div
      className={focused ? "miller-col focused" : "miller-col"}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) focusHere();
      }}
    >
      <div
        className="miller-body"
        onMouseDown={(e) => {
          if (e.target === e.currentTarget) focusHere();
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          focusHere();
          onMenu({ x: e.clientX, y: e.clientY });
        }}
      >
        {col.error ? (
          <div className="miller-error">这个目录读不出来：{col.error}</div>
        ) : !visible.length ? (
          <div className="miller-empty">{col.keyword.trim() ? "没有匹配的项" : "空文件夹"}</div>
        ) : (
          visible.map((node) => {
            const png = icons[node.path];
            return (
              <div
                key={node.path}
                className={[
                  "file-row",
                  selected.includes(node.path) ? "row-selected" : "",
                  node.hidden ? "row-hidden-item" : "",
                ]
                  .filter(Boolean)
                  .join(" ")}
                title={tipOf(node)}
                {...pressable(() => open(node), { role: null, space: false })}
                onClick={(e) => {
                  const mode = e.shiftKey ? "range" : e.ctrlKey || e.metaKey ? "toggle" : "single";
                  void pickIn(panelId, index, node.path, mode, visiblePaths);
                }}
                onDoubleClick={() => open(node)}
                /** 右键选中它再弹菜单；已经在多选里的那几项别被右键打散 */
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  if (!selected.includes(node.path)) void pickIn(panelId, index, node.path, "select", visiblePaths);
                  else if (!focused) void focusColIn(panelId, index);
                  onMenu({ x: e.clientX, y: e.clientY });
                }}
              >
                <span className="file-icon-slot">
                  {png ? (
                    <img className="file-icon" src={convertFileSrc(png)} alt="" draggable={false} />
                  ) : node.isDir ? (
                    <Folder size={16} color="var(--color-warning)" />
                  ) : (
                    <FileText size={16} color="var(--color-text-secondary)" />
                  )}
                </span>
                <span className="file-row-main">
                  <span className="file-row-line">
                    <span className="ellipsis file-row-name">{node.name}</span>
                    {node.alias && <span className="ellipsis file-row-alias">{node.alias}</span>}
                  </span>
                  {detail && (
                    <span className="ellipsis file-row-meta">
                      {node.isDir ? "" : formatBytes(node.size)}
                      {node.modifiedAt ? `${node.isDir ? "" : " · "}${formatWhen(node.modifiedAt)}` : ""}
                      {node.description ? ` · ${node.description}` : ""}
                    </span>
                  )}
                </span>
                <Pencil
                  className="icon-button row-edit"
                  size={14}
                  aria-label={`编辑「${node.name}」`}
                  {...pressable(() => onEdit(node))}
                  onClick={(e) => {
                    e.stopPropagation();
                    onEdit(node);
                  }}
                  // 这一行双击是「打开」，不拦一下就会在编辑按钮上按两下顺手把文件也开了
                  onDoubleClick={(e) => e.stopPropagation()}
                />
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
