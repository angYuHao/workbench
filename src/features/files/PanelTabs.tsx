import { useEffect, useState } from "react";
import { AlignJustify, List, Plus, X } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useFilesStore } from "../../stores/files";
import type { FilePanel as FilePanelShape, FileTab } from "../../stores/files";
import { folderName } from "../../utils/paths";
import { Button, message, pressable } from "../../ui";

/** 标签名只看这个标签最上层打开的目录，不跟着当前选中的列变 */
const tabTitle = (tab: FileTab) => folderName(tab.root);

/** 选文件夹的那一下，两处入口（栏里的 + 和关光之后的那条空栏）共用 */
async function pickFolder(): Promise<string | null> {
  try {
    const picked = await openDialog({ directory: true, multiple: false });
    return typeof picked === "string" ? picked : null;
  } catch (e) {
    message.error(String(e));
    return null;
  }
}

/** 每栏自己的标签条：跟 IDE 的分栏一样，加、切、关都只作用于这一栏 */
export default function PanelTabs({ panel }: { panel: FilePanelShape }) {
  const { activateTab, closeTab, closePanel, setViewIn, openTabIn, setDragTab, moveTabToPanel, splitDirToNewPanel } =
    useFilesStore.getState();
  const multi = useFilesStore((s) => s.panels.length > 1);
  const drag = useFilesStore((s) => s.dragTab);
  const tab = panel.tabs.find((t) => t.id === panel.activeTabId) ?? null;
  const detail = tab?.view === "detail";
  /** 拖的是本栏自己的标签时不接：原地松手没有意义，还会把整栏自己并一遍 */
  const canDrop = !!drag && drag.panelId !== panel.id;
  const [over, setOver] = useState(false);
  const dragFiles = useFilesStore((s) => s.dragFiles);
  /** 拖着文件夹落在标签条上的两种落点：落在某颗标签上拆新栏，落在空白处新开一个标签 */
  const [addOver, setAddOver] = useState(false);
  const [splitOver, setSplitOver] = useState<string | null>(null);
  /** 只认单个目录：文件当不了标签，一串目录往哪儿摆也没个准话 */
  const droppedDir = dragFiles?.dirs.length === 1 ? dragFiles.dirs[0] : null;

  useEffect(() => {
    if (!dragFiles) {
      setAddOver(false);
      setSplitOver(null);
    }
  }, [dragFiles]);

  const handleNewTab = async () => {
    const picked = await pickFolder();
    if (picked) await openTabIn(panel.id, picked);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setOver(false);
    setAddOver(false);
    const state = useFilesStore.getState();
    const d = state.dragTab;
    if (d && d.panelId !== panel.id) {
      void moveTabToPanel(d.panelId, d.tabId, panel.id);
      return;
    }
    /** 拖着文件夹落在标签条空白处：在这一栏新开一个标签 */
    const dir = state.dragFiles?.dirs.length === 1 ? state.dragFiles.dirs[0] : null;
    if (!dir) return;
    state.setDragFiles(null);
    void openTabIn(panel.id, dir);
  };

  return (
    <div
      className={[
        "file-tabs panel-tabs",
        over && canDrop ? "drop-target" : "",
        addOver && droppedDir ? "add-target" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      onDragOver={(e) => {
        /** 先管标签的拖拽：那套落点是「并进这一栏」 */
        if (canDrop) {
          e.preventDefault();
          e.stopPropagation();
          e.dataTransfer.dropEffect = "move";
          setOver(true);
          return;
        }
        if (!droppedDir) return;
        e.preventDefault();
        e.stopPropagation();
        e.dataTransfer.dropEffect = "copy";
        setSplitOver(null);
        if (!addOver) setAddOver(true);
      }}
      onDragLeave={(e) => {
        /** 在标签条内部的子元素之间移动时 relatedTarget 还在条里，不算离开 */
        if (e.currentTarget.contains(e.relatedTarget as Node | null)) return;
        setOver(false);
        setAddOver(false);
      }}
      onDrop={handleDrop}
    >
      {panel.tabs.map((t) => (
        <div
          key={t.id}
          className={[
            t.id === panel.activeTabId ? "file-tab active" : "file-tab",
            drag?.tabId === t.id ? "dragging" : "",
            splitOver === t.id && droppedDir ? "split-target" : "",
          ]
            .filter(Boolean)
            .join(" ")}
          title={t.root}
          draggable
          onDragStart={(e) => {
            e.dataTransfer.effectAllowed = "move";
            e.dataTransfer.setData("text/plain", t.root);
            setDragTab({ panelId: panel.id, tabId: t.id });
          }}
          onDragEnd={() => setDragTab(null)}
          /** 拖着文件夹停在某颗标签上：在这一栏右边拆出新的一栏来装它 */
          onDragOver={
            droppedDir
              ? (e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  e.dataTransfer.dropEffect = "copy";
                  setAddOver(false);
                  setSplitOver((x) => (x === t.id ? x : t.id));
                }
              : undefined
          }
          onDragLeave={
            droppedDir
              ? (e) => {
                  if (e.currentTarget.contains(e.relatedTarget as Node | null)) return;
                  setSplitOver((x) => (x === t.id ? null : x));
                }
              : undefined
          }
          onDrop={
            droppedDir
              ? (e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  setSplitOver(null);
                  useFilesStore.getState().setDragFiles(null);
                  void splitDirToNewPanel(droppedDir, panel.id);
                }
              : undefined
          }
          {...pressable(() => void activateTab(panel.id, t.id))}
          onClick={(e) => {
            e.stopPropagation();
            void activateTab(panel.id, t.id);
          }}
        >
          <span className="ellipsis">{tabTitle(t)}</span>
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
      {(addOver || splitOver) && droppedDir && (
        <span className="tab-drop-hint">
          {splitOver ? "松开：在右侧新开分栏" : "松开：新加标签"}
        </span>
      )}
      <span className="file-tab-actions">
        {tab && (
          <Button
            size="small"
            type="text"
            icon={detail ? <List size={14} /> : <AlignJustify size={14} />}
            aria-label={detail ? "回到紧凑列表" : "每项多摆一行大小和时间"}
            title={detail ? "回到紧凑列表" : "每项多摆一行大小、时间和说明"}
            onClick={() => setViewIn(panel.id, detail ? "list" : "detail")}
          />
        )}
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
  );
}

/** 栏都关光之后还留着的一条空标签栏：只剩一个 +，点了把新的目录叫回来 */
export function EmptyPanelTabs() {
  const handleNewTab = async () => {
    const picked = await pickFolder();
    if (picked) await useFilesStore.getState().openTab(picked);
  };

  return (
    <div className="file-tabs panel-tabs">
      <Plus
        className="icon-button"
        size={14}
        aria-label="打开一个文件夹"
        {...pressable(handleNewTab)}
        onClick={() => void handleNewTab()}
      />
    </div>
  );
}
