import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { AlignJustify, ArrowUp, ChevronLeft, ChevronRight, Folder, List, Plus, RefreshCw, X } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useFilesStore } from "../../stores/files";
import type { FilePanel as FilePanelShape, FileTab } from "../../stores/files";
import useFileActions from "./useFileActions";
import { folderName, parentOf, samePath, typedPath } from "../../utils/paths";
import { Button, SearchBox, message, pressable } from "../../ui";
import type { DriveInfo } from "../../types";

const tabTitle = (tab: FileTab) => folderName(tab.cols[Math.min(tab.focus, tab.cols.length - 1)]?.path ?? tab.root);

/** 盘符那一段用侧栏同一个叫法（卷标优先），和资源管理器的地址栏一致 */
function crumbName(path: string, drives: DriveInfo[]): string {
  const drive = drives.find((d) => samePath(d.path, path));
  return drive ? `${drive.label ?? drive.kind} (${drive.letter}:)` : folderName(path);
}

/** 从盘符根一层层排到当前目录。`parentOf` 到盘符根就停了，所以这一段就是全路径 */
function crumbsOf(path: string, drives: DriveInfo[]): { path: string; name: string }[] {
  const out: { path: string; name: string }[] = [];
  let cur: string | null = path;
  while (cur) {
    out.unshift({ path: cur, name: crumbName(cur, drives) });
    cur = parentOf(cur);
  }
  return out;
}

/** 地址栏收起来之后把焦点还给文件列表，不然接着按 Ctrl+C、F2 就没人接了；
    不带动滚动，不然列表停在中间时会被拽回顶部 */
function focusFileList() {
  document
    .querySelector<HTMLElement>(".file-panel.active .miller-col.focused .file-row")
    ?.focus({ preventScroll: true });
}

interface Props {
  /** 表头只服务当前栏：点了哪栏，标签和地址就换成哪栏的 */
  panel: FilePanelShape | null;
  multi: boolean;
}

/**
 * 文件页顶上的表头：地址栏一行、标签栏一行，**横跨整块内容区**（预览栏也从它下面开始），
 * 不再是某一栏自己的头顶那两行。多栏时它跟着当前栏走，换栏即换地址。
 *
 * 地址栏是资源管理器那种面包屑：每一层都能点，点空白处（或 Ctrl+L / Alt+D）变成输入框，
 * 可以把一整条路径敲进去或粘进去。敲进去的路径和面包屑走同一个入口，
 * 已经在列链上的那一层只是把焦点挪过去，不把右边那几列收掉。
 */
export default function FileHeader({ panel, multi }: Props) {
  const {
    activateTab,
    closeTab,
    closePanel,
    backIn,
    forwardIn,
    upIn,
    focusColIn,
    enterIn,
    setKeywordIn,
    setViewIn,
    openTabIn,
  } = useFilesStore.getState();
  const drives = useFilesStore((s) => s.drives);
  const actions = useFileActions();
  const [editing, setEditing] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const tab = panel?.tabs.find((t) => t.id === panel.activeTabId) ?? null;
  const col = tab?.cols[Math.min(Math.max(tab.focus, 0), tab.cols.length - 1)] ?? null;
  const detail = tab?.view === "detail";
  const canBack = !!tab && (tab.cols.length > 1 || parentOf(tab.root) !== null);
  const here = col?.path ?? tab?.root ?? "";
  const crumbs = useMemo(() => crumbsOf(here, drives), [here, drives]);

  /** 换栏、换标签、从别处导航走了，正在编辑的那条路径就作废 */
  useEffect(() => {
    setEditing(null);
  }, [here, panel?.id, tab?.id]);

  /** 进入编辑那一刻把整条路径选中；每敲一个字都重选会把光标位置弄没，所以只看「在不在编辑」 */
  useEffect(() => {
    if (editing === null) return;
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [editing === null]);

  /** 资源管理器的老习惯：Ctrl+L 和 Alt+D 都把地址栏切成输入状态 */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      /** 弹着对话框时不抢：那时键盘该归框里的按钮 */
      if (document.querySelector('[role="dialog"]')) return;
      const isL = e.ctrlKey && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "l";
      const isD = e.altKey && !e.ctrlKey && e.key.toLowerCase() === "d";
      if (!isL && !isD) return;
      e.preventDefault();
      setEditing(here);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [here]);

  if (!panel) return null;

  const handleNewTab = async () => {
    try {
      const picked = await openDialog({ directory: true, multiple: false });
      if (typeof picked === "string") await openTabIn(panel.id, picked);
    } catch (e) {
      message.error(String(e));
    }
  };

  const goTo = (path: string) => {
    const index = tab?.cols.findIndex((c) => samePath(c.path, path)) ?? -1;
    if (index >= 0) {
      void focusColIn(panel.id, index);
      return;
    }
    void enterIn(panel.id, path);
  };

  const commit = (text: string) => {
    const target = typedPath(text, here);
    setEditing(null);
    if (!target) return;
    void enterIn(panel.id, target).finally(() => {
      requestAnimationFrame(focusFileList);
    });
  };

  return (
    <div className="file-header">
      <div className="sub-toolbar">
        <Button
          size="small"
          type="text"
          icon={<ChevronLeft size={14} />}
          aria-label="后退"
          disabled={!canBack}
          onClick={() => void backIn(panel.id)}
        />
        <Button
          size="small"
          type="text"
          icon={<ChevronRight size={14} />}
          aria-label="前进"
          disabled={!tab?.forward.length}
          onClick={() => void forwardIn(panel.id)}
        />
        <Button
          size="small"
          type="text"
          icon={<ArrowUp size={14} />}
          aria-label="上级（收掉最右一列）"
          disabled={!canBack}
          onClick={() => void upIn(panel.id)}
        />
        <div
          className={editing === null ? "crumb-bar" : "crumb-bar crumb-bar-editing"}
          title={editing === null ? "点空白处可以直接输入路径（Ctrl+L）" : undefined}
          onMouseDown={(e) => e.target === e.currentTarget && e.preventDefault()}
          onClick={(e) => {
            /** 点空白才变输入框（点在某一层上是导航，那条 click 会冒上来，所以要认 target）；
                放在 click 上而不是 mousedown：mouseup 的定位光标会把刚做的全选缩成一个点 */
            if (e.target === e.currentTarget) setEditing(here);
          }}
        >
          {editing !== null ? (
            <input
              className="crumb-input"
              value={editing}
              spellCheck={false}
              aria-label="输入路径"
              ref={inputRef}
              onChange={(e) => setEditing(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  commit(editing);
                } else if (e.key === "Escape") {
                  e.preventDefault();
                  setEditing(null);
                  requestAnimationFrame(focusFileList);
                }
              }}
              onBlur={() => setEditing(null)}
            />
          ) : (
            <>
              <Folder className="crumb-lead" size={14} aria-hidden />
              {crumbs.map((c, i) => (
                <Fragment key={c.path}>
                  {i > 0 && <ChevronRight className="crumb-sep" size={12} />}
                  {i === crumbs.length - 1 ? (
                    <span className="crumb crumb-here" title={c.path}>
                      {c.name}
                    </span>
                  ) : (
                    <button type="button" className="crumb" title={c.path} onClick={() => goTo(c.path)}>
                      {c.name}
                    </button>
                  )}
                </Fragment>
              ))}
            </>
          )}
        </div>
        <SearchBox
          placeholder="搜索名称 / 别名 / 说明"
          style={{ width: 200, minWidth: 0 }}
          value={col?.keyword ?? ""}
          onChange={(v) => setKeywordIn(panel.id, v)}
        />
      </div>

      <div className="file-tabs">
        {panel.tabs.map((t) => (
          <div
            key={t.id}
            className={t.id === panel.activeTabId ? "file-tab active" : "file-tab"}
            title={t.cols[Math.min(t.focus, t.cols.length - 1)]?.path ?? t.root}
            {...pressable(() => void activateTab(panel.id, t.id))}
            onClick={(e) => {
              e.stopPropagation();
              void activateTab(panel.id, t.id);
            }}
          >
            <span className="ellipsis">{tabTitle(t)}</span>
            {panel.tabs.length > 1 && (
              <X
                className="file-tab-close"
                size={13}
                aria-label={`关闭标签「${tabTitle(t)}」`}
                {...pressable(() => void closeTab(panel.id, t.id))}
                onClick={(e) => {
                  e.stopPropagation();
                  void closeTab(panel.id, t.id);
                }}
              />
            )}
          </div>
        ))}
        <Plus
          className="icon-button"
          size={14}
          aria-label="在本栏新打开一个文件夹"
          {...pressable(handleNewTab)}
          onClick={(e) => {
            e.stopPropagation();
            void handleNewTab();
          }}
        />
        <span className="file-tab-actions">
          <Button
            size="small"
            type="text"
            icon={detail ? <List size={14} /> : <AlignJustify size={14} />}
            aria-label={detail ? "回到紧凑列表" : "每项多摆一行大小和时间"}
            title={detail ? "回到紧凑列表" : "每项多摆一行大小、时间和说明"}
            onClick={() => setViewIn(panel.id, detail ? "list" : "detail")}
          />
          <RefreshCw
            className="icon-button"
            size={14}
            aria-label="重新读取这个目录（F5）"
            {...pressable(() => void actions.refresh())}
            onClick={() => void actions.refresh()}
          />
          {multi && (
            <X
              className="icon-button file-panel-close"
              size={14}
              aria-label="关闭这一栏"
              {...pressable(() => void closePanel(panel.id))}
              onClick={() => void closePanel(panel.id)}
            />
          )}
        </span>
      </div>
    </div>
  );
}
