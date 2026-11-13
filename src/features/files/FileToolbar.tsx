import {
  ArrowUpDown,
  ClipboardPaste,
  Copy,
  Eye,
  EyeOff,
  List,
  MoreHorizontal,
  Pencil,
  Plus,
  Scissors,
  Trash2,
} from "lucide-react";
import { useFilesStore } from "../../stores/files";
import type { FilePanel as FilePanelShape } from "../../stores/files";
import { revealInExplorer } from "../../api/shell";
import { sortMenuEntries } from "./sortMenu";
import { Button, Dropdown, message } from "../../ui";
import type { DropdownItem } from "../../ui";
import useFileActions from "./useFileActions";
import type { FileNode } from "../../types";

interface Props {
  /** 工具栏作用于当前栏：地址栏跟着哪一栏走，它就跟着哪一栏走 */
  panel: FilePanelShape | null;
  onNewFolder: () => void;
  onRename: (node: FileNode) => void;
}

/** 当前栏、当前标签、当前焦点列。和 FilePanel 自己算的是同一套，不另立规矩 */
function focusedOf(panel: FilePanelShape | null) {
  const tab = panel?.tabs.find((t) => t.id === panel.activeTabId) ?? null;
  if (!panel || !tab) return { tab: null, col: null, index: 0 };
  const index = Math.min(Math.max(tab.focus, 0), tab.cols.length - 1);
  return { tab, col: tab.cols[index], index };
}

/**
 * 导航栏和标签栏中间那条命令栏，照 Windows 11 资源管理器那条排：
 * 左边一组建/剪/拷/粘/改名/删，中间排序和查看，右边一颗预览。
 *
 * 这里只是把原来藏在右键菜单和快捷键里的动作摆到明面上，**没有新加任何能力**：
 * 每个按钮都直接调 store 上现成的那一个动作，禁用态也跟选区、剪贴板走。
 */
export default function FileToolbar({ panel, onNewFolder, onRename }: Props) {
  const clipboard = useFilesStore((s) => s.clipboard);
  const previewPinned = useFilesStore((s) => s.previewPinned);
  const pinned = useFilesStore((s) => s.pinned);
  const newTypes = useFilesStore((s) => s.newTypes);
  const loadNewTypes = useFilesStore((s) => s.loadNewTypes);
  const createFileHere = useFilesStore((s) => s.createFileHere);
  const setSortIn = useFilesStore((s) => s.setSortIn);
  const setViewIn = useFilesStore((s) => s.setViewIn);
  const togglePreview = useFilesStore((s) => s.togglePreview);
  const togglePin = useFilesStore((s) => s.togglePin);
  const addPanel = useFilesStore((s) => s.addPanel);
  const closePanel = useFilesStore((s) => s.closePanel);
  const actions = useFileActions();

  const { tab, col, index } = focusedOf(panel);
  const selectedCount = col?.selected.length ?? 0;
  const only =
    selectedCount === 1 ? col?.nodes.find((n) => n.path === col.selected[0]) ?? null : null;
  const here = col?.path ?? "";
  const writable = !!panel && !!col;

  /** 「新建」那张单子：文件夹 + 这台机器登记过的文件类型（读的是注册表 ShellNew，
      和资源管理器右键「新建」同一张表，所以装了什么就能建什么）。第一次点开才去读 */
  const createItems: DropdownItem[] = [
    { key: "folder", label: "文件夹", onClick: onNewFolder },
    ...newTypes.map((t) => ({
      key: t.ext,
      label: t.label,
      onClick: () => void createFileHere(t.ext),
    })),
  ];

  const sortItems: DropdownItem[] = sortMenuEntries(col?.sort ?? "name", col?.sortDesc ?? false, (k, d) => {
    if (panel) setSortIn(panel.id, index, k, d);
  });

  const viewItems: DropdownItem[] = [
    { key: "list", label: `${tab?.view !== "detail" ? "✓ " : "   "}列表（只看名称）` },
    { key: "detail", label: `${tab?.view === "detail" ? "✓ " : "   "}详细信息（加大小、时间）` },
  ];

  const moreItems: DropdownItem[] = [
    {
      key: "pin",
      label: pinned.includes(here) ? "取消收藏这里" : "收藏这里",
      onClick: () => void togglePin(here),
    },
    {
      key: "reveal",
      label: "在资源管理器里显示",
      onClick: () =>
        only ? void revealInExplorer(only.path).catch((e) => message.error(String(e))) : undefined,
    },
    { key: "refresh", label: "重新读取（F5）", onClick: () => void actions.refresh() },
    { key: "split", label: "新建分栏", onClick: () => void addPanel() },
    { key: "close", label: "关闭这一栏", onClick: () => (panel ? void closePanel(panel.id) : undefined) },
  ];

  return (
    <div className="file-toolbar">
      <div className="file-toolbar-group">
        <Dropdown menu={{ items: createItems }}>
          <Button
            size="small"
            type="text"
            icon={<Plus size={14} />}
            onClick={() => void loadNewTypes()}
            disabled={!writable}
            title="在当前目录新建文件夹或一个文件（单子读的是系统的登记表）"
          >
            新建
          </Button>
        </Dropdown>
        <Button
          size="small"
          type="text"
          icon={<Scissors size={14} />}
          aria-label="剪切（Ctrl+X）"
          title="剪切（Ctrl+X）"
          onClick={() => void actions.cut()}
          disabled={!selectedCount}
        />
        <Button
          size="small"
          type="text"
          icon={<Copy size={14} />}
          aria-label="复制（Ctrl+C）"
          title="复制（Ctrl+C）"
          onClick={() => void actions.copy()}
          disabled={!selectedCount}
        />
        <Button
          size="small"
          type="text"
          icon={<ClipboardPaste size={14} />}
          aria-label={clipboard?.paths.length ? `粘贴 ${clipboard.paths.length} 项（Ctrl+V）` : "粘贴（Ctrl+V）"}
          title={
            clipboard?.paths.length
              ? `粘贴 ${clipboard.paths.length} 项（Ctrl+V，${clipboard.mode === "move" ? "剪切" : "复制"}的原件${clipboard.mode === "move" ? "会挪走" : "留着"}）`
              : "剪贴板里没有文件"
          }
          onClick={() => void actions.paste()}
          disabled={!clipboard?.paths.length}
        />
        <Button
          size="small"
          type="text"
          icon={<Pencil size={14} />}
          aria-label="重命名（F2）"
          title="重命名（F2，只改文件名，别名和说明跟着走）"
          onClick={() => (only ? onRename(only) : undefined)}
          disabled={!only}
        />
        <Button
          size="small"
          type="text"
          icon={<Trash2 size={14} />}
          aria-label="删除到回收站（Delete）"
          title="删除到回收站（Delete；Shift+Delete 是彻底删，走右键）"
          onClick={() => void actions.remove(true)}
          disabled={!selectedCount}
        />
      </div>

      <div className="file-toolbar-group">
        <Dropdown menu={{ items: sortItems }}>
          <Button size="small" type="text" icon={<ArrowUpDown size={14} />} disabled={!writable} title="按哪一列排这一栏">
            排序
          </Button>
        </Dropdown>
        <Dropdown
          menu={{
            items: viewItems,
            onClick: ({ key }) => {
              if (panel) setViewIn(panel.id, key === "detail" ? "detail" : "list");
            },
          }}
        >
          <Button size="small" type="text" icon={<List size={14} />} disabled={!tab} title="每项要不要多摆一行大小和时间">
            查看
          </Button>
        </Dropdown>
        <Dropdown menu={{ items: moreItems }}>
          <Button
            size="small"
            type="text"
            icon={<MoreHorizontal size={14} />}
            aria-label="更多操作"
            title="更多操作"
            disabled={!writable}
          />
        </Dropdown>
      </div>

      <span className="file-toolbar-gap" />

      <Button
        size="small"
        type="text"
        className={previewPinned ? "file-toolbar-on" : undefined}
        icon={previewPinned ? <Eye size={14} /> : <EyeOff size={14} />}
        aria-label="预览面板"
        title={previewPinned ? "预览面板常驻中，点这里收掉" : "点这里让预览面板常驻（默认只在选中文件时弹一下）"}
        onClick={() => togglePreview(!previewPinned)}
      >
        预览
      </Button>
    </div>
  );
}
