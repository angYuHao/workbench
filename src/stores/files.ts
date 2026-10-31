import { create } from "zustand";
import * as api from "../api/files";
import { getSettings, setSetting } from "../api/settings";
import { previewKind } from "../utils/fileKind";
import { parentOf, folderName, isUnder, samePath } from "../utils/paths";
import { SORT_ORDER } from "../utils/sort";
import type { DriveInfo, FileNode, FileRecord, KnownFolder, ReadTextResult } from "../types";

/** 上次开的那几栏、每栏那几个标签、各自走到哪个目录、拖过的栏宽 */
const LAYOUT_KEY = "files.layout";
/** 侧栏「收藏」那一节：用户自己顶上去的文件夹，纯粹是入口，不再影响任何权限 */
const PIN_KEY = "files.pinned";
/** 隐藏和系统项摆不摆 */
const HIDDEN_KEY = "files.showHidden";

/** 按哪一列排。大小和时间都从后端那两个字段来 */
export type SortKey = "name" | "size" | "modified";

/**
 * 一列 = 目录里的一屏。一个标签就是一串这样的列（Miller 列视图）：
 * 点开一个文件夹，它右边长出一列；再往深处走就是再长一列。
 * 选中项、搜索词、排序都跟着列走，左边那几列不会因为右边换了目录而乱掉。
 */
export interface FileColumn {
  id: string;
  path: string;
  /** 目录内容只在界面上缓存，每次现读磁盘 */
  nodes: FileNode[];
  /** 列不出来时的原因（没权限、目录没了）：整台机器都能点，光给空列表看着像目录真空了 */
  error: string | null;
  /** 正在读这个目录。手上还没有内容时界面才有用（重读时旧列表继续摆着，不闪「读取中」） */
  loading: boolean;
  /** 选中项按路径记（不按名字：同名两项在这页是常态）。视图片，loader 不许碰 */
  selected: string[];
  /** Shift 连选的那个起点，同样不入库 */
  anchor: string | null;
  keyword: string;
  sort: SortKey;
  sortDesc: boolean;
}

/** 一个标签 = 从某一层开始的一串列。标题永远跟着当前那一列走 */
export interface FileTab {
  id: string;
  root: string;
  cols: FileColumn[];
  /** 当前列的下标。点击、右键、搜索、粘贴都作用于这一列 */
  focus: number;
  /** 退回去时收掉的列，前进时再长回来 */
  forward: string[];
  /** list 只摆名称；detail 再加大小和时间两列（列拖窄的时候靠它腾地方） */
  view: "list" | "detail";
}

/** 一栏：横向并排的其中一个文件夹组 */
export interface FilePanel {
  id: string;
  tabs: FileTab[];
  activeTabId: string;
  /** 拖过分栏才有像素宽；null 表示和别的栏一起等分剩下的地方 */
  width: number | null;
}

/** 系统剪贴板上的那串文件在界面这边的镜像：状态条和右键菜单靠它知道「有东西可粘」 */
export interface Clipboard {
  mode: "copy" | "move";
  paths: string[];
}

/** 粘完了：搬/拷了几项，以及走的是复制还是移动 */
export interface PasteResult {
  count: number;
  mode: "copy" | "move";
}

/**
 * 能反着再做一遍的写操作。删除只留个记号：进回收站的后端还原不了，
 * 彻底删的更是没法回头，Ctrl+Z 遇到这两种就回一句话，不假装能撤回。
 */
type UndoOp =
  | { kind: "folder"; path: string }
  | { kind: "rename"; from: string; to: string }
  | { kind: "paste-copy"; paths: string[] }
  | { kind: "paste-move"; from: string[]; to: string[] }
  | { kind: "delete"; toTrash: boolean };

export interface UndoResult {
  /** 真退回去了就是 true；删除、非空文件夹这种只解释一句 */
  ok: boolean;
  text: string;
}

interface SavedColumn {
  path: string;
  selected?: string[];
  sort?: SortKey;
  sortDesc?: boolean;
}

interface SavedTab {
  cols?: SavedColumn[];
  /** 上一版的布局只存了一个 path，读到了就当成只有一列 */
  path?: string;
  focus?: number;
  forward?: string[];
  view?: "list" | "detail";
}

interface SavedPanel {
  tabs: SavedTab[];
  activeIndex: number;
  width?: number | null;
}

interface SavedLayout {
  panels: SavedPanel[];
  activeIndex: number;
}

let seq = 0;
/** id 只在一次会话里有效：布局按下标存，重启后重新生成，不存在跟旧 id 撞上的问题 */
function nextId(prefix: string): string {
  seq += 1;
  return `${prefix}-${seq}`;
}

function makeColumn(path: string): FileColumn {
  return {
    id: nextId("col"),
    path,
    nodes: [],
    error: null,
    loading: true,
    selected: [],
    anchor: null,
    keyword: "",
    sort: "name",
    sortDesc: false,
  };
}

function makeTab(path: string): FileTab {
  return {
    id: nextId("tab"),
    root: path,
    cols: [makeColumn(path)],
    focus: 0,
    forward: [],
    view: "list",
  };
}

function makePanel(path: string): FilePanel {
  const tab = makeTab(path);
  return { id: nextId("panel"), tabs: [tab], activeTabId: tab.id, width: null };
}

/** 当前列。focus 可能因为收列滞后一格，按列数夹一下 */
function focusedCol(tab: FileTab): FileColumn {
  return tab.cols[Math.min(Math.max(tab.focus, 0), tab.cols.length - 1)];
}

function toSavedLayout(panels: FilePanel[], activePanelId: string | null): SavedLayout {
  return {
    panels: panels.map((p) => ({
      tabs: p.tabs.map((t) => ({
        cols: t.cols.map((c) => ({
          path: c.path,
          selected: c.selected,
          sort: c.sort,
          sortDesc: c.sortDesc,
        })),
        focus: t.focus,
        forward: t.forward,
        view: t.view,
      })),
      activeIndex: Math.max(0, p.tabs.findIndex((t) => t.id === p.activeTabId)),
      width: p.width,
    })),
    activeIndex: Math.max(0, panels.findIndex((p) => p.id === activePanelId)),
  };
}

/** 三个来源都可能空：收藏第一个、不然常用第一个、不然第一个盘 */
function firstPlace(pinned: string[], known: KnownFolder[], drives: DriveInfo[]): string | null {
  return pinned[0] ?? known[0]?.path ?? drives[0]?.path ?? null;
}

function parseStringList(raw: string | undefined): string[] {
  if (!raw) return [];
  try {
    const list = JSON.parse(raw);
    return Array.isArray(list) ? list.filter((p): p is string => typeof p === "string") : [];
  } catch {
    return [];
  }
}

function parseLayout(raw: string | undefined): SavedLayout | null {
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as SavedLayout;
    return Array.isArray(parsed.panels) ? parsed : null;
  } catch {
    return null;
  }
}

/** 一屏里最多先要这么多图标：一目录上千项时逐张走 shell 太慢，滚到后面再补 */
const ICON_BATCH = 150;
/** 同一次会话里问过的路径不再问第二遍（哪怕后端说没有，比如关机时盘拔了） */
const iconAsked = new Set<string>();

interface FilesStore {
  drives: DriveInfo[];
  knownFolders: KnownFolder[];
  pinned: string[];
  showHidden: boolean;
  panels: FilePanel[];
  activePanelId: string | null;
  /** 正被拖着的那颗标签。只在拖拽期间有值，不进布局也不落库 */
  dragTab: { panelId: string; tabId: string } | null;
  /** 正被拖着的那串文件（列表行）。sourceDir 用来判同卷跨卷，
      dirs 是其中挑得出来的目录 —— 落到标签上时只有它能开成标签/分栏。同样只在拖拽期间有值 */
  dragFiles: { paths: string[]; sourceDir: string; dirs: string[] } | null;
  /** 路径 → 系统画好的 png。侧栏和列表都从这里取，取不到再退回线条图标 */
  icons: Record<string, string>;
  record: FileRecord | null;
  preview: ReadTextResult | null;
  /** 后端已经把哪个路径所在的目录授权给 webview 了；预览那几块要等它等于当前项才挂上去 */
  authorizedFor: string | null;
  clipboard: Clipboard | null;
  /** 读盘符、常用文件夹，再按上次存的布局把栏和标签摆回来 */
  boot: () => Promise<void>;
  /** 让系统画这几项的图标。已经问过的不再问，几十项一次 IPC */
  ensureIcons: (paths: string[]) => void;
  enterIn: (panelId: string, path: string) => Promise<void>;
  openTabIn: (panelId: string, path: string) => Promise<void>;
  /** 侧栏和地址栏不知道用户点的是哪一栏，都作用于当前这栏 */
  enter: (path: string) => Promise<void>;
  openTab: (path: string) => Promise<void>;
  backIn: (panelId: string) => Promise<void>;
  forwardIn: (panelId: string) => Promise<void>;
  upIn: (panelId: string) => Promise<void>;
  focusColIn: (panelId: string, colIndex: number) => Promise<void>;
  setKeywordIn: (panelId: string, text: string) => void;
  /** 点排序按钮：名称↑ → 名称↓ → 大小↑ → 大小↓ → 时间↑ → 时间↓，再回到名称↑ */
  cycleSortIn: (panelId: string, colIndex: number) => void;
  setViewIn: (panelId: string, view: "list" | "detail") => void;
  activatePanel: (panelId: string) => Promise<void>;
  activateTab: (panelId: string, tabId: string) => Promise<void>;
  closeTab: (panelId: string, tabId: string) => Promise<void>;
  addPanel: () => Promise<void>;
  closePanel: (panelId: string) => Promise<void>;
  setPanelWidth: (panelId: string, width: number) => void;
  setDragTab: (drag: { panelId: string; tabId: string } | null) => void;
  /** 标签拖进另一栏的标签条：并进那一栏；目标栏里已经有同目录的标签就直接切过去 */
  moveTabToPanel: (fromPanelId: string, tabId: string, toPanelId: string) => Promise<void>;
  /** 标签拖到某一栏的列区：在这一栏右边拆出一栏，只装这颗标签 */
  splitTabToNewPanel: (fromPanelId: string, tabId: string, afterPanelId: string) => Promise<void>;
  setDragFiles: (drag: { paths: string[]; sourceDir: string; dirs: string[] } | null) => void;
  /** 把某个目录作为新的一栏，摆在 afterPanelId 右边 */
  splitDirToNewPanel: (path: string, afterPanelId: string) => Promise<void>;
  /** 框选：hits 是框住的那串，additive 时并进按下框那一刻的 base，不并就别管以前选了啥 */
  selectPathsIn: (
    panelId: string,
    colIndex: number,
    hits: string[],
    additive: boolean,
    base: string[],
  ) => Promise<void>;
  /** 拖拽落地：把这串搬/拷进 dest。本来就在 dest 里的、拖进自己的先摘掉；落地后在目标目录里选中它们 */
  dropFilesInto: (paths: string[], dest: string, mode: "copy" | "move") => Promise<PasteResult | null>;
  /** 重读每栏当前那颗标签的目录（磁盘变更、切了隐藏项开关都走这里）。
      后台标签不在这儿读，等切过去时 activateTab 补 */
  reloadTabs: () => Promise<void>;
  toggleShowHidden: (on: boolean) => Promise<void>;
  togglePin: (path: string) => Promise<void>;
  /** 重新读一遍当前列：F5 和写操作之后用 */
  refreshActive: () => Promise<void>;
  /** 点一行：single 换选（目录会往右长一列）、toggle 加减、range 从起点连选到这儿、
      select 只选中不导航（右键用）。visible 是筛过的那串路径，连选按它算 */
  pickIn: (
    panelId: string,
    colIndex: number,
    path: string,
    mode: "single" | "toggle" | "range" | "select",
    visible: string[],
  ) => Promise<void>;
  selectAllIn: (panelId: string, colIndex: number, visible: string[]) => void;
  clearSelection: (panelId: string, colIndex?: number) => void;
  copySelection: () => Promise<void>;
  cutSelection: () => Promise<void>;
  /** 当前列选中的那串路径 */
  selectedPaths: () => string[];
  createFolderHere: (name: string) => Promise<void>;
  renameSelected: (newName: string) => Promise<void>;
  /** 多选就是删这几种；toTrash 走回收站，false 是彻底删 */
  deleteSelected: (toTrash: boolean) => Promise<number>;
  /** 把系统剪贴板里那串文件粘进当前列；剪贴板里没有文件时回 undefined */
  pasteIntoActive: () => Promise<PasteResult | undefined>;
  /** Ctrl+Z：把上一步能反着做的写操作退回去；栈空回 null */
  undoLast: () => Promise<UndoResult | null>;
  /** 系统剪贴板才是真的那份：窗口重新拿到焦点时照着它更新镜像 */
  syncClipboard: () => Promise<void>;
  saveMeta: (path: string, alias: string | null, description: string | null) => Promise<void>;
}

export const useFilesStore = create<FilesStore>((set, get) => {
  let settleTimer: number | undefined;
  /** 预览请求的序号：后点的覆盖先点的，先点的那次回来时自己作废 */
  let previewSeq = 0;
  /** 最近几步写操作的反向动作。只在本会话里留着，重启就清空 */
  const undos: UndoOp[] = [];
  const UNDO_LIMIT = 20;
  const pushUndo = (op: UndoOp) => {
    undos.push(op);
    if (undos.length > UNDO_LIMIT) undos.shift();
  };

  /** 眼下真正看得见的那几条目录：每栏当前标签的整串列。
      藏起来的标签既不读盘也不监视 —— 几十个标签挂在后台时，全挂监视器的代价是每动一下
      磁盘就要把整页重扫一遍，而这页只画当前标签那几列。切回去时 activateTab 会补读 */
  const visibleDirs = (panels: FilePanel[]) =>
    [
      ...new Set(
        panels.flatMap((p) => {
          const t = p.tabs.find((x) => x.id === p.activeTabId);
          return t ? t.cols.map((c) => c.path) : [];
        }),
      ),
    ];

  /** 上次发出去的监视清单。目录没变就不再发：选一项、拖一下也会 settle，
      但那不该让后端把整套监视拆了重建 */
  let watchedKey: string | null = null;

  /** 布局改一次写一次会把这张 KV 和别的窗口刷个不停，攒一下再落。
      监视哪几个目录也在这一下里一起说给后端 */
  const settle = () => {
    window.clearTimeout(settleTimer);
    settleTimer = window.setTimeout(() => {
      const { panels, activePanelId } = get();
      void setSetting(LAYOUT_KEY, JSON.stringify(toSavedLayout(panels, activePanelId))).catch(() => undefined);
      const dirs = visibleDirs(panels);
      const key = dirs.join("\n");
      if (key === watchedKey) return;
      watchedKey = key;
      void api.watchFolders(dirs).catch(() => undefined);
    }, 400);
  };

  const patchTab = (panelId: string, tabId: string, fn: (tab: FileTab) => FileTab) =>
    set((state) => ({
      panels: state.panels.map((p) =>
        p.id === panelId ? { ...p, tabs: p.tabs.map((t) => (t.id === tabId ? fn(t) : t)) } : p,
      ),
    }));

  const patchCol = (panelId: string, tabId: string, colId: string, fn: (col: FileColumn) => FileColumn) =>
    patchTab(panelId, tabId, (t) => ({ ...t, cols: t.cols.map((c) => (c.id === colId ? fn(c) : c)) }));

  const find = (panelId: string | null) => {
    const panel = get().panels.find((p) => p.id === panelId) ?? null;
    const tab = panel?.tabs.find((t) => t.id === panel.activeTabId) ?? null;
    return { panel, tab };
  };

  /** 当前栏当前标签的当前列 */
  const currentCol = (): { panel: FilePanel; tab: FileTab; col: FileColumn } | null => {
    const { panel, tab } = find(get().activePanelId);
    return panel && tab ? { panel, tab, col: focusedCol(tab) } : null;
  };

  /** 某个标签里某一列是不是用户正看着的那一列（预览只跟它走） */
  const isFocused = (panelId: string, tabId: string, colId: string) => {
    const { panel, tab } = find(panelId);
    return !!panel && tab?.id === tabId && focusedCol(tab).id === colId;
  };

  const clearPreview = () => {
    /** 让还在路上的 loadPreview 作废：清空之后不该再被它把预览塞回来 */
    previewSeq += 1;
    set({ record: null, preview: null, authorizedFor: null });
  };

  const loadPreview = async (node: FileNode) => {
    const token = ++previewSeq;
    /** 预览要读的字节走 asset 协议，得先让后端把这个文件所在的目录授出去，不然一律 403。
        授完才记 authorizedFor：图片/iframe 是渲染时自己读盘的，早一帧挂上去就是一张碎图 */
    await api.authorizePreview(node.path).catch(() => undefined);
    /** 连着点了好几项时只有最后那一次算数，不然慢的那次回来会把预览盖成别的文件 */
    if (token !== previewSeq) return;
    set({ authorizedFor: node.path });
    /** 只有 markdown 走这条 IPC：它需要后端转好的字符串。文本/代码由 TextPreview 自己取字节解码 */
    const needsText = !node.isDir && previewKind(node.name) === "markdown";
    const [record, preview] = await Promise.all([
      api.getFile(node.path).catch(() => null),
      needsText ? api.readTextFile(node.path).catch(() => null) : Promise.resolve(null),
    ]);
    if (token !== previewSeq) return;
    set({ record, preview });
  };

  const previewFor = async (node: FileNode | null) => {
    if (!node) {
      clearPreview();
      return;
    }
    await loadPreview(node);
  };

  /** 预览只跟「当前列单选的那一项」走；多选时右侧那块空着，别显示一个说不清是谁的预览 */
  const syncPreview = async () => {
    const cur = currentCol();
    if (!cur) {
      clearPreview();
      return;
    }
    const node = cur.col.selected.length === 1 ? cur.col.nodes.find((n) => n.path === cur.col.selected[0]) : null;
    /** 预览栏只给具体文件弹：目录不读记录也不授权，宁可让它收着 */
    await previewFor(node && !node.isDir ? node : null);
  };

  const rememberIcons = (nodes: FileNode[]) => {
    get().ensureIcons(nodes.slice(0, ICON_BATCH).map((n) => n.path));
  };

  const loadColumn = async (panelId: string, tabId: string, colId: string) => {
    const tab = get().panels.find((p) => p.id === panelId)?.tabs.find((t) => t.id === tabId);
    const col = tab?.cols.find((c) => c.id === colId);
    if (!col) return;
    if (!col.loading) patchCol(panelId, tabId, colId, (c) => ({ ...c, loading: true }));
    let nodes: FileNode[] = [];
    let error: string | null = null;
    try {
      nodes = await api.listFiles(col.path, get().showHidden);
    } catch (e) {
      // 没权限、盘拔了、目录被删了：这一列摆原因，别把整页带走
      error = String(e);
    }
    /** 读盘期间用户可能又切走了，路径对不上说明这份结果已经过期，丢掉 */
    const still = get().panels
      .find((p) => p.id === panelId)
      ?.tabs.find((t) => t.id === tabId)
      ?.cols.find((c) => c.id === colId);
    if (!still || still.path !== col.path) return;
    const alive = nodes.filter((n) => still.selected.includes(n.path)).map((n) => n.path);
    patchCol(panelId, tabId, colId, (c) => ({
      ...c,
      nodes,
      error,
      loading: false,
      selected: alive,
      anchor: alive[0] ?? null,
    }));
    rememberIcons(nodes);
    /** 选中的那几项没了（被删、被隐藏）才动预览；只是内容变了不动它 */
    if (alive.length !== still.selected.length && isFocused(panelId, tabId, colId)) await syncPreview();
  };

  /** 一个标签里所有列一起读：切标签、启动恢复、整页重读都走这里 */
  const loadTabColumns = async (panelId: string, tabId: string) => {
    const tab = get().panels.find((p) => p.id === panelId)?.tabs.find((t) => t.id === tabId);
    if (!tab) return;
    await Promise.all(tab.cols.map((c) => loadColumn(panelId, tabId, c.id)));
  };

  /** 只重读眼下看得见的那些列：删一个文件不该把整页的目录都扫一遍。
      后台标签里的同目录列先不动，切回那颗标签时 activateTab 会整串补读 */
  const reloadDirs = async (dirs: string[]) => {
    const targets = new Set(dirs);
    const jobs: Promise<void>[] = [];
    for (const p of get().panels) {
      const t = p.tabs.find((x) => x.id === p.activeTabId);
      if (!t) continue;
      for (const c of t.cols) {
        if (targets.has(c.path)) jobs.push(loadColumn(p.id, t.id, c.id));
      }
    }
    await Promise.all(jobs);
  };

  /**
   * 点开一个目录：把它右边那一列换成它，深度再往后的列收掉。
   * 同一列已经站在这个目录上就不再读盘，只把更深的列剪掉。
   */
  const revealChild = async (
    panelId: string,
    tabId: string,
    colIndex: number,
    path: string,
    selection: string[],
    anchor: string | null,
  ) => {
    const tab = get().panels.find((p) => p.id === panelId)?.tabs.find((t) => t.id === tabId);
    if (!tab) return;
    const existing = tab.cols[colIndex + 1];
    const same = existing?.path === path;
    const col = same ? existing : makeColumn(path);
    patchTab(panelId, tabId, (t) => ({
      ...t,
      cols: [...t.cols.slice(0, colIndex + 1), col].map((c, i) =>
        i === colIndex ? { ...c, selected: selection, anchor } : c,
      ),
      focus: colIndex,
      forward: [],
    }));
    clearPreview();
    settle();
    if (!same) await loadColumn(panelId, tabId, col.id);
  };

  /** 把整个标签挪到某个目录：旧的一串列全收掉，从这一层重新开始 */
  const reroot = async (panelId: string, tabId: string, path: string) => {
    const col = makeColumn(path);
    patchTab(panelId, tabId, (t) => ({ ...t, root: path, cols: [col], focus: 0, forward: [] }));
    clearPreview();
    settle();
    await loadColumn(panelId, tabId, col.id);
  };

  /**
   * 「就在当前标签里换过去」。侧栏、地址栏、右键里的「打开」都走这里，所以哪怕记着的当前栏已经对不上
   * （栏被拆掉、全关光之后状态落后），也绝不为此另叠一颗标签 —— 有栏就落到第一栏上，
   * 真的一栏都没有才建。用户点侧栏那一下的意思永远是「换个地方看」，不是「再开一颗」。
   */
  const enterIn = async (panelId: string, path: string) => {
    const { panel, tab } = find(panelId);
    if (panel && tab) {
      if (focusedCol(tab).path === path) return;
      await reroot(panel.id, tab.id, path);
      return;
    }
    const fallback = get().panels.find((p) => p.tabs.length) ?? null;
    if (fallback) {
      const target = fallback.tabs.find((t) => t.id === fallback.activeTabId) ?? fallback.tabs[0];
      set({ activePanelId: fallback.id });
      settle();
      if (focusedCol(target).path === path) return;
      await reroot(fallback.id, target.id, path);
      return;
    }
    const created = makePanel(path);
    set((state) => ({ panels: [...state.panels, created], activePanelId: created.id }));
    settle();
    await loadTabColumns(created.id, created.activeTabId);
  };

  const backIn = async (panelId: string) => {
    const { panel, tab } = find(panelId);
    if (!panel || !tab) return;
    if (tab.cols.length > 1) {
      const popped = tab.cols[tab.cols.length - 1];
      patchTab(panel.id, tab.id, (t) => ({
        ...t,
        cols: t.cols.slice(0, -1),
        focus: Math.min(t.focus, t.cols.length - 2),
        forward: [popped.path, ...t.forward],
      }));
      settle();
      await syncPreview();
      return;
    }
    const parent = parentOf(tab.root);
    if (!parent) return;
    const col = makeColumn(parent);
    patchTab(panel.id, tab.id, (t) => ({
      ...t,
      root: parent,
      cols: [col],
      focus: 0,
      forward: [t.root, ...t.forward],
    }));
    clearPreview();
    settle();
    await loadColumn(panel.id, tab.id, col.id);
  };

  const forwardIn = async (panelId: string) => {
    const { panel, tab } = find(panelId);
    if (!panel || !tab || !tab.forward.length) return;
    const target = tab.forward[0];
    const focus = Math.min(Math.max(tab.focus, 0), tab.cols.length - 1);
    const existing = tab.cols[focus + 1];
    if (existing?.path === target) {
      patchTab(panel.id, tab.id, (t) => ({ ...t, focus: focus + 1, forward: t.forward.slice(1) }));
      settle();
      await syncPreview();
      return;
    }
    const col = makeColumn(target);
    patchTab(panel.id, tab.id, (t) => ({
      ...t,
      cols: [...t.cols.slice(0, focus + 1), col],
      focus: focus + 1,
      forward: t.forward.slice(1),
    }));
    clearPreview();
    settle();
    await loadColumn(panel.id, tab.id, col.id);
  };

  return {
    drives: [],
    knownFolders: [],
    pinned: [],
    showHidden: false,
    panels: [],
    activePanelId: null,
    dragTab: null,
    dragFiles: null,
    icons: {},
    record: null,
    preview: null,
    authorizedFor: null,
    clipboard: null,
    boot: async () => {
      const [settings, drives, knownFolders] = await Promise.all([
        getSettings(),
        api.getDrives(),
        api.getKnownFolders(),
      ]);
      const pinned = parseStringList(settings[PIN_KEY]);
      const showHidden = settings[HIDDEN_KEY] === "1";
      set({ drives, knownFolders, pinned, showHidden });

      const saved = parseLayout(settings[LAYOUT_KEY]);
      const panels: FilePanel[] = [];
      if (saved) {
        for (const p of saved.panels) {
          const tabs: FileTab[] = [];
          for (const st of p.tabs) {
            const savedCols: SavedColumn[] = st.cols?.length
              ? st.cols
              : typeof st.path === "string" && st.path
                ? [{ path: st.path }]
                : [];
            const cols = savedCols
              .filter((c) => typeof c.path === "string" && c.path)
              .map((c) => {
                const col = makeColumn(c.path);
                col.selected = (c.selected ?? []).filter((x): x is string => typeof x === "string");
                col.anchor = col.selected[0] ?? null;
                col.sort = c.sort ?? "name";
                col.sortDesc = c.sortDesc ?? false;
                return col;
              });
            if (!cols.length) continue;
            tabs.push({
              id: nextId("tab"),
              root: cols[0].path,
              cols,
              focus: Math.min(Math.max(0, st.focus ?? 0), cols.length - 1),
              forward: (st.forward ?? []).filter((x): x is string => typeof x === "string"),
              view: st.view ?? "list",
            });
          }
          if (!tabs.length) continue;
          const index = Math.min(Math.max(0, p.activeIndex), tabs.length - 1);
          panels.push({ id: nextId("panel"), tabs, activeTabId: tabs[index].id, width: p.width ?? null });
        }
      }
      const first = firstPlace(pinned, knownFolders, drives);
      if (!panels.length && first) panels.push(makePanel(first));
      /** 这里不 settle：布局本来就是刚从库里读回来的，再写一遍只是给别的窗口发一发空广播 */
      set({ panels, activePanelId: panels[0]?.id ?? null });
      /** 只读每栏当前那颗标签：上次退出时挂着的后台标签等切过去再读，
          否则标签一多，启动就要把几十个目录同时扫一遍 */
      await Promise.all(panels.map((p) => loadTabColumns(p.id, p.activeTabId)));
      const dirs = visibleDirs(panels);
      watchedKey = dirs.join("\n");
      void api.watchFolders(dirs).catch(() => undefined);
      /** 上次关窗时选着的那一项要接着预览；不做这一步，右边会一直停在授权中的转圈上 */
      await syncPreview();
      /** 侧栏那几节也要系统图标：盘符、常用文件夹、收藏 */
      get().ensureIcons([...pinned, ...knownFolders.map((k) => k.path), ...drives.map((d) => d.path)]);
      /** 剪贴板里可能躺着别的程序（或上一次本程序）拷的文件，进来先按它摆状态条 */
      void get().syncClipboard();
    },
    ensureIcons: (paths) => {
      const wanted = paths.filter((p) => p && !iconAsked.has(p) && !(p in get().icons));
      if (!wanted.length) return;
      for (const p of wanted) iconAsked.add(p);
      for (let i = 0; i < wanted.length; i += 100) {
        const chunk = wanted.slice(i, i + 100);
        void api
          .getShellIcons(chunk)
          .then((map) => {
            if (!Object.keys(map).length) return;
            set((state) => ({ icons: { ...state.icons, ...map } }));
          })
          .catch(() => undefined);
      }
    },
    enterIn,
    openTabIn: async (panelId, path) => {
      const panel = get().panels.find((p) => p.id === panelId);
      if (!panel) {
        const created = makePanel(path);
        set((state) => ({ panels: [...state.panels, created], activePanelId: created.id }));
        settle();
        await loadTabColumns(created.id, created.activeTabId);
        return;
      }
      /** 同目录不叠第二个标签，跳过去就行 */
      const existing = panel.tabs.find((t) => focusedCol(t).path === path);
      if (existing) {
        await get().activateTab(panel.id, existing.id);
        return;
      }
      const tab = makeTab(path);
      set((state) => ({
        activePanelId: panel.id,
        panels: state.panels.map((p) =>
          p.id === panel.id ? { ...p, tabs: [...p.tabs, tab], activeTabId: tab.id } : p,
        ),
      }));
      settle();
      clearPreview();
      await loadTabColumns(panel.id, tab.id);
    },
    enter: (path) => enterIn(get().activePanelId ?? "", path),
    openTab: (path) => get().openTabIn(get().activePanelId ?? "", path),
    backIn,
    forwardIn,
    /** 往上走和「收掉最右那一列」是同一件事；只剩一列时才真的回上级目录 */
    upIn: (panelId) => backIn(panelId),
    focusColIn: async (panelId, colIndex) => {
      const { panel, tab } = find(panelId);
      if (!panel || !tab || !tab.cols[colIndex] || tab.focus === colIndex) return;
      patchTab(panel.id, tab.id, (t) => ({ ...t, focus: colIndex }));
      settle();
      await syncPreview();
    },
    setKeywordIn: (panelId, text) => {
      const { panel, tab } = find(panelId);
      if (!panel || !tab) return;
      patchCol(panel.id, tab.id, focusedCol(tab).id, (c) => ({ ...c, keyword: text }));
    },
    cycleSortIn: (panelId, colIndex) => {
      const { panel, tab } = find(panelId);
      const col = tab?.cols[colIndex];
      if (!panel || !tab || !col) return;
      const at = SORT_ORDER.findIndex((o) => o.key === col.sort && o.desc === col.sortDesc);
      const next = SORT_ORDER[(at + 1) % SORT_ORDER.length];
      patchCol(panel.id, tab.id, col.id, (c) => ({ ...c, sort: next.key, sortDesc: next.desc }));
      settle();
    },
    setViewIn: (panelId, view) => {
      const { panel, tab } = find(panelId);
      if (!panel || !tab) return;
      patchTab(panel.id, tab.id, (t) => ({ ...t, view }));
      settle();
    },
    activatePanel: async (panelId) => {
      if (get().activePanelId === panelId) return;
      set({ activePanelId: panelId });
      settle();
      await syncPreview();
    },
    activateTab: async (panelId, tabId) => {
      const { panels, activePanelId } = get();
      const panel = panels.find((p) => p.id === panelId);
      const tab = panel?.tabs.find((t) => t.id === tabId);
      if (!panel || !tab) return;
      /** 点的是已经摆在眼前的这颗标签：什么都不会变，别再 settle 一次、更不用读盘 */
      if (panel.activeTabId === tabId && activePanelId === panelId) return;
      const switching = panel.activeTabId !== tabId;
      set((state) => ({
        activePanelId: panelId,
        panels: state.panels.map((p) => (p.id === panelId ? { ...p, activeTabId: tabId } : p)),
      }));
      settle();
      /** 后台标签没在监视、也没读过：切回来先把这一串列重读一遍，预览再照结果对。
          旧内容在重读期间继续摆着，所以不会闪空白 */
      if (switching) await loadTabColumns(panelId, tabId);
      await syncPreview();
    },
    closeTab: async (panelId, tabId) => {
      const panel = get().panels.find((p) => p.id === panelId);
      const index = panel?.tabs.findIndex((t) => t.id === tabId) ?? -1;
      if (!panel || index < 0) return;
      const rest = panel.tabs.filter((t) => t.id !== tabId);
      if (!rest.length) {
        await get().closePanel(panelId);
        return;
      }
      /** 关掉的是当前标签时往左邻一个，全关完了才收栏 */
      const wasActive = panel.activeTabId === tabId;
      const nextActiveId = wasActive ? rest[Math.min(index, rest.length - 1)].id : panel.activeTabId;
      set((state) => ({
        panels: state.panels.map((p) =>
          p.id === panelId ? { ...p, tabs: rest, activeTabId: nextActiveId } : p,
        ),
      }));
      settle();
      if (wasActive) {
        /** 接替的那颗可能一直待在后台、内容都还没读过：先把它读出来 */
        await loadTabColumns(panelId, nextActiveId);
        if (get().activePanelId === panelId) await syncPreview();
      }
    },
    addPanel: async () => {
      const { drives, knownFolders, pinned, panels } = get();
      const shown = new Set(panels.flatMap((p) => p.tabs.flatMap((t) => t.cols.map((c) => c.path))));
      /** 优先摆一个还没在任何栏里出现过的入口，摆不出来就用第一个盘 */
      const candidate =
        pinned.find((p) => !shown.has(p)) ??
        knownFolders.find((k) => !shown.has(k.path))?.path ??
        drives.find((d) => !shown.has(d.path))?.path ??
        firstPlace(pinned, knownFolders, drives);
      if (!candidate) return;
      const created = makePanel(candidate);
      set((state) => ({ panels: [...state.panels, created], activePanelId: created.id }));
      settle();
      await loadTabColumns(created.id, created.activeTabId);
    },
    closePanel: async (panelId) => {
      const rest = get().panels.filter((p) => p.id !== panelId);
      /** 关的不是当前栏就别抢当前：用户点的是旁边那栏的叉，视图不该跳走 */
      const wasActive = get().activePanelId === panelId;
      set({ panels: rest, activePanelId: wasActive ? rest[0]?.id ?? null : get().activePanelId });
      settle();
      await syncPreview();
    },
    setPanelWidth: (panelId, width) => {
      set((state) => ({ panels: state.panels.map((p) => (p.id === panelId ? { ...p, width } : p)) }));
      settle();
    },
    setDragTab: (drag) => {
      set({ dragTab: drag });
    },
    moveTabToPanel: async (fromPanelId, tabId, toPanelId) => {
      if (fromPanelId === toPanelId) return;
      const { panels } = get();
      const from = panels.find((p) => p.id === fromPanelId);
      const to = panels.find((p) => p.id === toPanelId);
      const index = from?.tabs.findIndex((t) => t.id === tabId) ?? -1;
      if (!from || !to || index < 0) return;
      const tab = from.tabs[index];
      const rest = from.tabs.filter((t) => t.id !== tabId);
      /** 目标栏里已经有同目录的标签，就不再叠一颗：切过去，等于把这颗并掉 */
      const dup = to.tabs.find((t) => focusedCol(t).path === focusedCol(tab).path) ?? null;
      const sourceNextId =
        rest.length && from.activeTabId === tabId ? rest[Math.min(index, rest.length - 1)].id : null;
      const targetActiveId = dup ? dup.id : tab.id;
      const next: FilePanel[] = [];
      for (const p of panels) {
        if (p.id === from.id) {
          if (!rest.length) continue;
          next.push({
            ...p,
            tabs: rest,
            activeTabId: from.activeTabId === tabId ? rest[Math.min(index, rest.length - 1)].id : p.activeTabId,
          });
          continue;
        }
        if (p.id === to.id) {
          next.push(dup ? { ...p, activeTabId: dup.id } : { ...p, tabs: [...p.tabs, tab], activeTabId: tab.id });
          continue;
        }
        next.push(p);
      }
      set({ panels: next, activePanelId: to.id, dragTab: null });
      settle();
      /** 拖过来的标签、以及顶替它位置的那颗，都可能一直在后台没读过 */
      await loadTabColumns(to.id, targetActiveId);
      if (sourceNextId) await loadTabColumns(from.id, sourceNextId);
      await syncPreview();
    },
    splitTabToNewPanel: async (fromPanelId, tabId, afterPanelId) => {
      const { panels } = get();
      const from = panels.find((p) => p.id === fromPanelId);
      const index = from?.tabs.findIndex((t) => t.id === tabId) ?? -1;
      if (!from || index < 0) return;
      const tab = from.tabs[index];
      const rest = from.tabs.filter((t) => t.id !== tabId);
      const sourceNextId =
        rest.length && from.activeTabId === tabId ? rest[Math.min(index, rest.length - 1)].id : null;
      const created: FilePanel = { id: nextId("panel"), tabs: [tab], activeTabId: tab.id, width: null };
      const next: FilePanel[] = [];
      for (const p of panels) {
        if (p.id === from.id) {
          /** 这颗标签是源栏的最后一颗时，源栏直接消失，别留一条空壳 */
          if (!rest.length) continue;
          next.push({
            ...p,
            tabs: rest,
            activeTabId: from.activeTabId === tabId ? rest[Math.min(index, rest.length - 1)].id : p.activeTabId,
          });
          continue;
        }
        next.push(p);
      }
      let at: number;
      if (afterPanelId !== fromPanelId) {
        const found = next.findIndex((p) => p.id === afterPanelId);
        at = found >= 0 ? found + 1 : next.length;
      } else {
        const found = next.findIndex((p) => p.id === fromPanelId);
        /** 源栏还留着：新栏排在它右边。源栏被拆空了：新栏就顶在它原来的位置 */
        at = found >= 0 ? found + 1 : Math.min(Math.max(panels.findIndex((p) => p.id === fromPanelId), 0), next.length);
      }
      next.splice(at, 0, created);
      set({ panels: next, activePanelId: created.id, dragTab: null });
      settle();
      /** 拆出来的标签可能来自后台、内容还没读过；源栏顶替上来的那颗同理 */
      await loadTabColumns(created.id, tab.id);
      if (sourceNextId) await loadTabColumns(from.id, sourceNextId);
      await syncPreview();
    },
    setDragFiles: (drag) => {
      set({ dragFiles: drag });
    },
    splitDirToNewPanel: async (path, afterPanelId) => {
      const created = makePanel(path);
      set((state) => {
        const at = state.panels.findIndex((p) => p.id === afterPanelId);
        const next = [...state.panels];
        /** 落在某一栏的标签上：新栏就摆在那一栏右边 */
        next.splice(at >= 0 ? at + 1 : next.length, 0, created);
        return { panels: next, activePanelId: created.id };
      });
      settle();
      await loadTabColumns(created.id, created.activeTabId);
      await syncPreview();
    },
    selectPathsIn: async (panelId, colIndex, hits, additive, base) => {
      const { panel, tab } = find(panelId);
      const col = tab?.cols[colIndex];
      if (!panel || !tab || !col) return;
      /** 框选每挪一下都会调到这里，没变就别 set：整页跟着刷新一遍太浪费 */
      const next = additive ? [...base, ...hits.filter((p) => !base.includes(p))] : hits;
      const now = new Set(col.selected);
      if (next.length === col.selected.length && next.every((p) => now.has(p))) return;
      patchTab(panel.id, tab.id, (t) => ({
        ...t,
        cols: t.cols.map((c) =>
          c.id === col.id ? { ...c, selected: next, anchor: additive ? c.anchor : next[0] ?? null } : c,
        ),
        focus: colIndex,
      }));
      settle();
      if (get().activePanelId !== panel.id) return;
      await syncPreview();
    },
    dropFilesInto: async (paths, dest, mode) => {
      /** 拖到它自己（或它底下某一层）身上、或者拖回它本来就待着的目录，都不算动作 */
      const usable = paths.filter(
        (p) => !isUnder(dest, p) && !samePath(parentOf(p) ?? "", dest),
      );
      set({ dragFiles: null });
      if (!usable.length) return null;
      const made = mode === "copy" ? await api.copyPaths(usable, dest) : await api.movePaths(usable, dest);
      if (mode === "copy") pushUndo({ kind: "paste-copy", paths: made });
      else pushUndo({ kind: "paste-move", from: usable, to: made });
      const srcDirs = usable.map((p) => parentOf(p)).filter((p): p is string => !!p);
      await reloadDirs([...new Set([...srcDirs, dest])]);
      /** 资源管理器的习惯：搬/拷完在新的那一层里把它们选中，接着就能继续动它们 */
      for (const p of get().panels) {
        for (const t of p.tabs) {
          for (const c of t.cols) {
            if (!samePath(c.path, dest)) continue;
            const alive = made.filter((m) => c.nodes.some((n) => samePath(n.path, m)));
            if (alive.length) patchCol(p.id, t.id, c.id, (x) => ({ ...x, selected: alive, anchor: alive[0] }));
          }
        }
      }
      /** 落下之后选中的是新的那几项，布局也跟着落一下 */
      settle();
      await syncPreview();
      return { count: made.length, mode };
    },
    reloadTabs: async () => {
      /** 只重读眼下看得见的那几颗标签。磁盘事件、隐藏项开关、别的窗口改文件都走这里：
          后台标签等切过去时 activateTab 会自己补读，没必要每次让几十个目录一起重扫 */
      const { panels } = get();
      await Promise.all(panels.map((p) => loadTabColumns(p.id, p.activeTabId)));
    },
    refreshActive: async () => {
      const cur = currentCol();
      if (cur) await loadColumn(cur.panel.id, cur.tab.id, cur.col.id);
    },
    toggleShowHidden: async (on) => {
      set({ showHidden: on });
      void setSetting(HIDDEN_KEY, on ? "1" : "0").catch(() => undefined);
      await get().reloadTabs();
    },
    togglePin: async (path) => {
      const list = get().pinned;
      const next = list.includes(path) ? list.filter((p) => p !== path) : [...list, path];
      set({ pinned: next });
      void setSetting(PIN_KEY, JSON.stringify(next)).catch(() => undefined);
      if (!list.includes(path)) get().ensureIcons([path]);
    },
    pickIn: async (panelId, colIndex, path, mode, visible) => {
      const { panel, tab } = find(panelId);
      const col = tab?.cols[colIndex];
      if (!panel || !tab || !col) return;
      let next = [path];
      let anchor = path;
      if (mode === "toggle") {
        next = col.selected.includes(path) ? col.selected.filter((p) => p !== path) : [...col.selected, path];
      } else if (mode === "range" && col.anchor) {
        const from = visible.indexOf(col.anchor);
        const to = visible.indexOf(path);
        if (from >= 0 && to >= 0) {
          const [lo, hi] = from <= to ? [from, to] : [to, from];
          next = visible.slice(lo, hi + 1);
          anchor = col.anchor;
        }
      }
      const node = col.nodes.find((n) => n.path === path) ?? null;

      /** 单击目录 = 往右长一列；双击才真的把这个标签挪进去（见 FileColumn 的 open） */
      if (mode === "single" && node?.isDir) {
        await revealChild(panel.id, tab.id, colIndex, path, next, anchor);
        return;
      }

      /** 单击文件是「要打开这一项」的前一步：右边那些列跟着作废。右键只选不导航 */
      const navigate = mode === "single";
      patchTab(panel.id, tab.id, (t) => ({
        ...t,
        cols: (navigate ? t.cols.slice(0, colIndex + 1) : t.cols).map((c) =>
          c.id === col.id ? { ...c, selected: next, anchor } : c,
        ),
        focus: colIndex,
        forward: navigate ? [] : t.forward,
      }));
      settle();
      if (get().activePanelId !== panel.id) return;
      await previewFor(next.length === 1 ? node : null);
    },
    selectAllIn: (panelId, colIndex, visible) => {
      const { panel, tab } = find(panelId);
      const col = tab?.cols[colIndex];
      if (!panel || !tab || !col) return;
      patchTab(panel.id, tab.id, (t) => ({
        ...t,
        cols: t.cols.map((c) => (c.id === col.id ? { ...c, selected: visible, anchor: visible[0] ?? null } : c)),
        focus: colIndex,
      }));
      clearPreview();
    },
    clearSelection: (panelId, colIndex) => {
      const { panel, tab } = find(panelId);
      if (!panel || !tab) return;
      const col = colIndex === undefined ? focusedCol(tab) : tab.cols[colIndex];
      if (!col) return;
      patchCol(panel.id, tab.id, col.id, (c) => ({ ...c, selected: [], anchor: null }));
      clearPreview();
    },
    copySelection: async () => {
      const cur = currentCol();
      if (!cur?.col.selected.length) return;
      const paths = [...cur.col.selected];
      await api.writeClipboardFiles(paths, "copy");
      set({ clipboard: { mode: "copy", paths } });
    },
    cutSelection: async () => {
      const cur = currentCol();
      if (!cur?.col.selected.length) return;
      const paths = [...cur.col.selected];
      await api.writeClipboardFiles(paths, "move");
      set({ clipboard: { mode: "move", paths } });
    },
    selectedPaths: () => currentCol()?.col.selected ?? [],
    createFolderHere: async (name) => {
      const cur = currentCol();
      if (!cur) return;
      const made = await api.createFolder(cur.col.path, name);
      pushUndo({ kind: "folder", path: made });
      await loadColumn(cur.panel.id, cur.tab.id, cur.col.id);
    },
    renameSelected: async (newName) => {
      const cur = currentCol();
      if (!cur || cur.col.selected.length !== 1) return;
      const before = cur.col.selected[0];
      const after = await api.renamePath(before, newName);
      if (after !== before) pushUndo({ kind: "rename", from: before, to: after });
      /** 改的是目录名的话，右边那几列的路径全作废 */
      patchTab(cur.panel.id, cur.tab.id, (t) => ({ ...t, cols: t.cols.slice(0, t.focus + 1), forward: [] }));
      await loadColumn(cur.panel.id, cur.tab.id, cur.col.id);
    },
    deleteSelected: async (toTrash) => {
      const cur = currentCol();
      if (!cur || !cur.col.selected.length) return 0;
      const paths = [...cur.col.selected];
      const n = await api.deletePaths(paths, toTrash);
      pushUndo({ kind: "delete", toTrash });
      /** 剪贴板里要是也有这几项，把删掉的那些摘出去：留着它们，下一次粘贴就是在往不存在的路径上做动作 */
      const clip = get().clipboard;
      if (clip?.mode === "move" && clip.paths.some((p) => paths.includes(p))) {
        const left = clip.paths.filter((p) => !paths.includes(p));
        void api.writeClipboardFiles(left, left.length ? "move" : "none").catch(() => undefined);
        set({ clipboard: left.length ? { mode: "move", paths: left } : null });
      }
      /** 删掉的可能是某一列自己的目录，右边那几列一并收掉 */
      patchTab(cur.panel.id, cur.tab.id, (t) => ({ ...t, cols: t.cols.slice(0, t.focus + 1), forward: [] }));
      /** 选中的清空和预览的收掉都交给 loadColumn：它按「还在不在目录里」判，比在这里再清一遍准 */
      await loadColumn(cur.panel.id, cur.tab.id, cur.col.id);
      return n;
    },
    undoLast: async () => {
      const op = undos.pop();
      if (!op) return null;
      /** 反做完要把碰过的目录都重读一遍，不然栏里摆的还是撤销前的样子 */
      const dirs = new Set<string>();
      const touch = (p: string | null) => {
        if (p) dirs.add(p);
      };
      let result: UndoResult;
      switch (op.kind) {
        case "folder": {
          /** 只撤空文件夹：用户要是已经往里放了东西，替他把整包删掉不是「撤销」 */
          const inside = await api.listFiles(op.path, true);
          if (inside.length) return { ok: false, text: "新建的那个文件夹里已经放了东西，没有删" };
          await api.deletePaths([op.path], false);
          touch(parentOf(op.path));
          result = { ok: true, text: "已撤销新建文件夹" };
          break;
        }
        case "rename": {
          const back = await api.renamePath(op.to, folderName(op.from));
          touch(parentOf(op.from));
          touch(parentOf(back));
          result = { ok: true, text: `已撤销重命名，名字改回「${folderName(back)}」` };
          break;
        }
        case "paste-copy": {
          /** 删的是刚拷出来的副本，原件都还在；仍走回收站，万一删错了还能捞回来 */
          await api.deletePaths(op.paths, true);
          op.paths.forEach((p) => touch(parentOf(p)));
          result = { ok: true, text: `已撤销，${op.paths.length} 项副本送进了回收站` };
          break;
        }
        case "paste-move": {
          const failed: string[] = [];
          for (let i = 0; i < op.to.length; i++) {
            const home = parentOf(op.from[i]);
            touch(parentOf(op.to[i]));
            if (!home) {
              failed.push(folderName(op.from[i]));
              continue;
            }
            try {
              await api.movePaths([op.to[i]], home);
              touch(home);
            } catch {
              failed.push(folderName(op.to[i]));
            }
          }
          result = failed.length
            ? { ok: false, text: `撤销没做完，${failed.join("、")} 挪不回原处` }
            : { ok: true, text: `已撤销，${op.to.length} 项挪回原处` };
          break;
        }
        case "delete":
          return {
            ok: false,
            text: op.toTrash ? "这几项在回收站里，去回收站还原就行" : "彻底删除的项目没法撤销",
          };
      }
      await reloadDirs([...dirs]);
      return result;
    },
    pasteIntoActive: async () => {
      const cur = currentCol();
      if (!cur) return undefined;
      /** 以系统剪贴板为准：资源管理器、Outlook 拷过来的也接得住，
          本页自己拷的那串在窗口失焦后可能早被别的程序换掉了 */
      const got = await api.readClipboardFiles();
      if (!got?.paths.length) {
        set({ clipboard: null });
        return undefined;
      }
      const mode = got.cut ? "move" : "copy";
      /** 「不能粘进它自己里面」由后端逐条检查，这里不再判一遍。回的是新落地的路径 */
      const made =
        mode === "copy"
          ? await api.copyPaths(got.paths, cur.col.path)
          : await api.movePaths(got.paths, cur.col.path);
      const count = made.length;
      if (mode === "copy") pushUndo({ kind: "paste-copy", paths: made });
      else pushUndo({ kind: "paste-move", from: got.paths, to: made });
      if (mode === "move") {
        /** 搬完就照资源管理器那样把这串标成作废：留着它，下一次谁粘都是往不存在的路径上做动作 */
        try {
          await api.writeClipboardFiles(got.paths, "none");
        } catch {
          // 剪贴板这会儿被别人占着也不要紧，文件已经搬到位了
        }
        set({ clipboard: null });
        const srcDirs = got.paths.map((p) => parentOf(p)).filter((p): p is string => !!p);
        await reloadDirs([...new Set([...srcDirs, cur.col.path])]);
      } else {
        set({ clipboard: { mode, paths: got.paths } });
        await loadColumn(cur.panel.id, cur.tab.id, cur.col.id);
      }
      return { count, mode };
    },
    syncClipboard: async () => {
      try {
        const got = await api.readClipboardFiles();
        set({ clipboard: got?.paths.length ? { mode: got.cut ? "move" : "copy", paths: got.paths } : null });
      } catch {
        // 读不到（剪贴板正被别的程序占着）就先按原样留着，不值得为这一下弹错误
      }
    },
    saveMeta: async (path, alias, description) => {
      const record = await api.saveFileMeta(path, alias, description);
      set((state) => ({
        panels: state.panels.map((p) => ({
          ...p,
          tabs: p.tabs.map((t) => ({
            ...t,
            cols: t.cols.map((c) => ({
              ...c,
              nodes: c.nodes.map((n) =>
                n.path === path ? { ...n, alias: record.alias, description: record.description } : n,
              ),
            })),
          })),
        })),
        record: state.record?.path === path ? record : state.record,
      }));
    },
  };
});
