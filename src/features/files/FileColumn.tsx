import { useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { FileText, Folder } from "lucide-react";
import { openPath } from "../../api/shell";
import { useFilesStore } from "../../stores/files";
import type { FileTab } from "../../stores/files";
import { visibleNodes } from "./listing";
import { canDropInto, dropInto, dropMode } from "./dnd";
import useRubberBand from "./useRubberBand";
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
  onMenu,
  onActivate,
}: Props) {
  const col = tab.cols[index];
  const { pickIn, enterIn, focusColIn, selectPathsIn, clearSelection, setDragFiles } = useFilesStore.getState();
  const visible = useMemo(() => (col ? visibleNodes(col) : []), [col]);
  const visiblePaths = useMemo(() => visible.map((n) => n.path), [visible]);
  const selected = col?.selected ?? [];
  /** 正被拖着的那串文件：这一列里那几行要跟着变淡 */
  const dragFiles = useFilesStore((s) => s.dragFiles);
  const bodyRef = useRef<HTMLDivElement>(null);
  /** 拖过来的文件正悬在哪个目录行上、或者悬在这一列空白处 */
  const [dropPath, setDropPath] = useState<string | null>(null);
  const [overCol, setOverCol] = useState(false);
  /** 框选：只在空白处按下去才起框，行上的按下留给拖拽 */
  const { band, start: startBand } = useRubberBand(
    bodyRef,
    (hits, additive, base) => void selectPathsIn(panelId, index, hits, additive, base),
    () => void clearSelection(panelId, index),
  );

  /** 拖拽一结束就把悬停记号收掉：不然下一次拖别的东西时，这一列会先亮一下上次的落点 */
  useEffect(() => {
    if (!dragFiles) {
      setDropPath(null);
      setOverCol(false);
    }
  }, [dragFiles]);

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
        className={["miller-body", overCol && dragFiles ? "drop-into" : ""].filter(Boolean).join(" ")}
        ref={bodyRef}
        onMouseDown={(e) => {
          if (e.target !== e.currentTarget) return;
          focusHere();
          startBand(e, selected);
        }}
        /** 空白处也接落点：就是放进这个目录 */
        onDragOver={(e) => {
          const d = useFilesStore.getState().dragFiles;
          if (!d || !canDropInto(d.paths, col.path)) return;
          e.preventDefault();
          e.stopPropagation();
          e.dataTransfer.dropEffect = dropMode(d.sourceDir, col.path, e);
          if (dropPath) setDropPath(null);
          if (!overCol) setOverCol(true);
        }}
        onDragLeave={(e) => {
          if (e.currentTarget.contains(e.relatedTarget as Node | null)) return;
          setOverCol(false);
        }}
        onDrop={(e) => {
          if (!useFilesStore.getState().dragFiles) return;
          e.preventDefault();
          e.stopPropagation();
          setOverCol(false);
          void dropInto(col.path, e);
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          focusHere();
          onMenu({ x: e.clientX, y: e.clientY });
        }}
      >
        {col.error ? (
          <div className="miller-error">这个目录读不出来：{col.error}</div>
        ) : col.loading && !col.nodes.length ? (
          <div className="miller-empty">读取中…</div>
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
                  dragFiles?.paths.includes(node.path) ? "row-dragging" : "",
                  dragFiles && dropPath === node.path ? "drop-target" : "",
                ]
                  .filter(Boolean)
                  .join(" ")}
                data-path={node.path}
                title={tipOf(node)}
                {...pressable(() => open(node), { role: null, space: false })}
                draggable
                onDragStart={(e) => {
                  /** 拖的是没选中的那项就先把它选中：跟资源管理器一样，拖这一下就是选它这一下 */
                  const paths = selected.includes(node.path) ? [...selected] : [node.path];
                  /** 里面哪几项是目录：落到标签上开标签/分栏靠它 */
                  const dirs = col.nodes.filter((n) => n.isDir && paths.includes(n.path)).map((n) => n.path);
                  if (!selected.includes(node.path)) void pickIn(panelId, index, node.path, "select", visiblePaths);
                  setDragFiles({ paths, dirs, sourceDir: col.path });
                  e.dataTransfer.effectAllowed = "copyMove";
                  e.dataTransfer.setData("text/plain", paths.join("\r\n"));
                }}
                onDragEnd={() => {
                  setDragFiles(null);
                  setDropPath(null);
                  setOverCol(false);
                }}
                /** 目录行自己就是落点：别的文件停在这儿，就是放进这个目录 */
                onDragOver={
                  node.isDir
                    ? (e) => {
                        const d = useFilesStore.getState().dragFiles;
                        if (!d || !canDropInto(d.paths, node.path)) return;
                        e.preventDefault();
                        e.stopPropagation();
                        e.dataTransfer.dropEffect = dropMode(d.sourceDir, node.path, e);
                        setOverCol(false);
                        setDropPath((p) => (p === node.path ? p : node.path));
                      }
                    : undefined
                }
                onDragLeave={
                  node.isDir
                    ? (e) => {
                        if (e.currentTarget.contains(e.relatedTarget as Node | null)) return;
                        setDropPath((p) => (p === node.path ? null : p));
                      }
                    : undefined
                }
                onDrop={
                  node.isDir
                    ? (e) => {
                        if (!useFilesStore.getState().dragFiles) return;
                        e.preventDefault();
                        e.stopPropagation();
                        setDropPath(null);
                        void dropInto(node.path, e);
                      }
                    : undefined
                }
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
              </div>
            );
          })
        )}
      </div>
      {band && (
        <div
          className="file-band"
          style={{ left: band.left, top: band.top, width: band.width, height: band.height }}
        />
      )}
    </div>
  );
}
