import { useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useFilesStore } from "../../stores/files";
import type { Clipboard, FilePanel as FilePanelShape } from "../../stores/files";
import useFileActions from "./useFileActions";
import FileColumn from "./FileColumn";
import { visibleNodes } from "./listing";
import { sortLabel } from "../../utils/sort";
import { formatBytes } from "../../utils/humanize";
import { revealInExplorer, openPath } from "../../api/shell";
import { ContextMenu, message } from "../../ui";
import type { MenuItem } from "../../ui";
import type { DriveInfo, FileNode } from "../../types";

interface Props {
  panel: FilePanelShape;
  active: boolean;
  /** 预览栏是不是正弹着：它一开一合，列区宽度就变，当前列可能被挤出视野 */
  previewOpen: boolean;
  pinned: string[];
  clipboard: Clipboard | null;
  icons: Record<string, string>;
  onActivate: () => void;
  onEdit: (node: FileNode) => void;
  /** 弹重命名/新建文件夹那个框由页面管，这里只负责说「我要改这个」 */
  onRename: (node: FileNode) => void;
  onNewFolder: () => void;
  registerRef: (el: HTMLDivElement | null) => void;
}

/** 这一栏站在哪个盘上，用来在状态条里报剩余空间 */
function driveOf(path: string | null, drives: DriveInfo[]): DriveInfo | null {
  if (!path) return null;
  const p = path.toLowerCase();
  return drives.find((d) => p.startsWith(d.path.toLowerCase())) ?? null;
}

export default function FilePanel({
  panel,
  active,
  previewOpen,
  pinned,
  clipboard,
  icons,
  onActivate,
  onEdit,
  onRename,
  onNewFolder,
  registerRef,
}: Props) {
  const tab = panel.tabs.find((t) => t.id === panel.activeTabId) ?? null;
  const col = tab?.cols[Math.min(Math.max(tab.focus, 0), tab.cols.length - 1)] ?? null;
  const focusIndex = tab ? Math.min(Math.max(tab.focus, 0), tab.cols.length - 1) : 0;
  /** 动作是稳定的，取一次就行；面板数据由父组件订阅后传下来 */
  const {
    drives,
    openTabIn,
    backIn,
    forwardIn,
    upIn,
    cycleSortIn,
    selectAllIn,
    clearSelection,
    togglePin,
  } = useFilesStore.getState();
  const actions = useFileActions();
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const millerRef = useRef<HTMLDivElement>(null);

  const visible = useMemo(() => (col ? visibleNodes(col) : []), [col]);
  /** 连选和全选都按筛过的这串算：搜索框里缩到 3 项时 Ctrl+A 不该把没显示的那几百项也选上 */
  const visiblePaths = useMemo(() => visible.map((n) => n.path), [visible]);
  const selected = col?.selected ?? [];
  const only = selected.length === 1 ? visible.find((n) => n.path === selected[0]) ?? null : null;
  const detail = tab?.view === "detail";
  const drive = driveOf(col?.path ?? null, drives);

  /** 列区的滚动：往右长出新列时把它推出来；切标签或换焦点时把当前那一列摆进视野。
      重启恢复多列时不能一头滚到最右边，不然当前列反而看不见了 */
  const scrollState = useRef<{ tab?: string; len: number }>({ len: 0 });
  useEffect(() => {
    const el = millerRef.current;
    if (!el || !tab) return;
    const switched = scrollState.current.tab !== tab.id;
    const grew = !switched && tab.cols.length > scrollState.current.len;
    scrollState.current = { tab: tab.id, len: tab.cols.length };
    const target = el.querySelector<HTMLElement>(grew ? ".miller-col:last-child" : ".miller-col.focused");
    if (!target) return;
    const left = target.offsetLeft;
    const right = left + target.offsetWidth;
    if (left < el.scrollLeft) el.scrollLeft = left;
    else if (right > el.scrollLeft + el.clientWidth) el.scrollLeft = right - el.clientWidth;
  }, [tab?.id, tab?.focus, tab?.cols.length, previewOpen]);

  /** 右键里那一条「在新标签打开一个文件夹…」：表头那个加号管当前栏，这里也一样 */
  const handleNewTab = async () => {
    try {
      const picked = await openDialog({ directory: true, multiple: false });
      if (typeof picked === "string") await openTabIn(panel.id, picked);
    } catch (e) {
      message.error(String(e));
    }
  };

  const menuItems = (): MenuItem[] => {
    const items: MenuItem[] = [];
    if (only) {
      items.push({
        label: only.isDir ? "打开" : "用系统程序打开",
        onClick: () => {
          if (only.isDir) void useFilesStore.getState().enterIn(panel.id, only.path);
          else void openPath(only.path).catch((e) => message.error(String(e)));
        },
      });
    }
    items.push({ label: "新建文件夹", onClick: onNewFolder });
    if (only) {
      items.push({ label: "重命名", onClick: () => onRename(only) });
      items.push({ label: "别名与说明", onClick: () => onEdit(only) });
    }
    if (selected.length) {
      items.push({ label: "复制", onClick: () => void actions.copy() });
      items.push({ label: "剪切", onClick: () => void actions.cut() });
    }
    if (clipboard?.paths.length) {
      items.push({ label: `粘贴 ${clipboard.paths.length} 项`, onClick: () => void actions.paste() });
    }
    if (selected.length) {
      items.push({ label: "删除到回收站", danger: true, onClick: () => void actions.remove(true) });
      items.push({ label: "彻底删除", danger: true, onClick: () => void actions.remove(false) });
    }
    if (only) items.push({ label: "在资源管理器里显示", onClick: () => void revealInExplorer(only.path).catch((e) => message.error(String(e))) });
    items.push({
      label: visible.length && !selected.length ? "全选" : "取消选择",
      onClick: () => (selected.length ? clearSelection(panel.id) : selectAllIn(panel.id, focusIndex, visiblePaths)),
    });
    items.push({ label: "重新读取", onClick: () => void actions.refresh() });
    if (col) {
      const here = col.path;
      /** 列顶上那条头行撤掉了，排序入口挪到这里：点一次换下一档，名称/大小/时间各自正反 */
      items.push({
        label: `排序：${sortLabel(col.sort, col.sortDesc)}`,
        onClick: () => cycleSortIn(panel.id, focusIndex),
      });
      items.push({ label: pinned.includes(here) ? "从收藏里移除这里" : "收藏这里", onClick: () => void togglePin(here) });
    }
    items.push({ label: "在新标签打开一个文件夹…", onClick: () => void handleNewTab() });
    return items;
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    const el = e.target as HTMLElement;
    /** 搜索框里打字不该触发删除：输入控件收到就交还给浏览器 */
    if (el.tagName === "INPUT" || el.tagName === "TEXTAREA") return;
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === "a") {
      e.preventDefault();
      selectAllIn(panel.id, focusIndex, visiblePaths);
    } else if (mod && e.key.toLowerCase() === "c") {
      e.preventDefault();
      void actions.copy();
    } else if (mod && e.key.toLowerCase() === "x") {
      e.preventDefault();
      void actions.cut();
    } else if (mod && e.key.toLowerCase() === "v") {
      e.preventDefault();
      void actions.paste();
    } else if (e.key === "Escape") {
      clearSelection(panel.id, focusIndex);
    } else if (e.key === "Delete") {
      e.preventDefault();
      void actions.remove(!e.shiftKey);
    } else if (e.key === "F2") {
      e.preventDefault();
      if (only) onRename(only);
    } else if (e.key === "F5") {
      e.preventDefault();
      void actions.refresh();
    } else if (e.altKey && e.key === "ArrowLeft") {
      void backIn(panel.id);
    } else if (e.altKey && e.key === "ArrowRight") {
      void forwardIn(panel.id);
    } else if (e.key === "Backspace") {
      e.preventDefault();
      void upIn(panel.id);
    }
  };

  return (
    <div
      className={active ? "column file-panel active" : "column file-panel"}
      ref={registerRef}
      style={
        panel.width === null
          ? { flex: "1 1 0", minWidth: 200 }
          : { flex: "0 1 auto", minWidth: 200, width: panel.width }
      }
      onMouseDown={onActivate}
      /** Tab 走到这一栏里也算「到了这一栏」：不然键盘选中的文件全跑到预览栏外面去了 */
      onFocusCapture={onActivate}
      onKeyDown={onKeyDown}
    >
      <div className="miller" ref={millerRef}>
        {tab?.cols.map((c, i) => (
          <FileColumn
            key={c.id}
            panelId={panel.id}
            tab={tab}
            index={i}
            focused={i === focusIndex}
            detail={detail}
            icons={icons}
            onEdit={onEdit}
            onMenu={setMenu}
            onActivate={onActivate}
          />
        ))}
      </div>

      <div className="file-status">
        <span>{visible.length} 项</span>
        {!!selected.length && <span>已选 {selected.length} 项</span>}
        {!!clipboard?.paths.length && (
          <span>
            剪贴板 {clipboard.paths.length} 项（{clipboard.mode === "copy" ? "复制" : "剪切"}）
          </span>
        )}
        {drive?.free != null && drive?.total != null && (
          <span className="file-status-right">
            {drive.letter}: 剩余 {formatBytes(drive.free)} / 共 {formatBytes(drive.total)}
          </span>
        )}
      </div>

      <ContextMenu at={active ? menu : null} items={menuItems()} onClose={() => setMenu(null)} />
    </div>
  );
}
