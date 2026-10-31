import { Fragment, useEffect, useState } from "react";
import type { ReactNode } from "react";
import { Columns2, Eye, Folder, HardDrive, Pin, Server, X } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useFilesStore } from "../../stores/files";
import FileHeader from "./FileHeader";
import FilePanel from "./FilePanel";
import { EmptyPanelTabs } from "./PanelTabs";
import PreviewPanel from "./PreviewPanel";
import useSplitter, { useColumnResize } from "./useSplitter";
import useFileActions from "./useFileActions";
import { canDropInto, dropInto, dropMode } from "./dnd";
import { formatBytes } from "../../utils/humanize";
import { folderName } from "../../utils/paths";
import { Button, Form, FormItem, Input, Modal, Switch, TextArea, message, pressable, useForm } from "../../ui";
import type { FileNode } from "../../types";

/** 侧栏一行：点它就是在当前标签里换过去，和地址栏面包屑一样，不叠新标签。
    要另开一个标签走标签条上的 `+`，或者把文件夹拖到标签条上 */
function SideItem({
  path,
  label,
  hint,
  icon,
  iconPath,
  active,
  onOpen,
  onRemove,
}: {
  path: string;
  label: string;
  hint?: string;
  icon: ReactNode;
  iconPath?: string;
  active: boolean;
  onOpen: (path: string) => void;
  onRemove?: (path: string) => void;
}) {
  /** 从列表里拖出来的文件可以直接放到侧栏这几个位置：收藏、常用、盘符 */
  const dragFiles = useFilesStore((s) => s.dragFiles);
  const [over, setOver] = useState(false);
  const dropping = over && !!dragFiles && canDropInto(dragFiles.paths, path);

  useEffect(() => {
    if (!dragFiles) setOver(false);
  }, [dragFiles]);

  return (
    <div
      className={["nav-item", active ? "active" : "", dropping ? "drop-target" : ""].filter(Boolean).join(" ")}
      title={hint ? `${label}\n${hint}` : path}
      onClick={() => onOpen(path)}
      {...pressable(() => onOpen(path))}
      onDragOver={(e) => {
        const d = useFilesStore.getState().dragFiles;
        if (!d || !canDropInto(d.paths, path)) return;
        e.preventDefault();
        e.stopPropagation();
        e.dataTransfer.dropEffect = dropMode(d.sourceDir, path, e);
        if (!over) setOver(true);
      }}
      onDragLeave={(e) => {
        if (e.currentTarget.contains(e.relatedTarget as Node | null)) return;
        setOver(false);
      }}
      onDrop={(e) => {
        if (!useFilesStore.getState().dragFiles) return;
        e.preventDefault();
        e.stopPropagation();
        setOver(false);
        void dropInto(path, e);
      }}
    >
      {iconPath ? (
        <img className="file-icon" src={convertFileSrc(iconPath)} alt="" draggable={false} />
      ) : (
        icon
      )}
      <span className="ellipsis" style={{ flex: 1 }}>
        {label}
      </span>
      {onRemove && (
        <X
          className="icon-button"
          size={13}
          aria-label={`从收藏里移除「${label}」`}
          {...pressable(() => onRemove(path))}
          onClick={(e) => {
            e.stopPropagation();
            onRemove(path);
          }}
        />
      )}
    </div>
  );
}

export default function FilesView() {
  const {
    drives,
    knownFolders,
    pinned,
    showHidden,
    panels,
    activePanelId,
    icons,
    record,
    preview,
    authorizedFor,
    clipboard,
    boot,
    enter,
    addPanel,
    activatePanel,
    setPanelWidth,
    togglePin,
    toggleShowHidden,
    saveMeta,
  } = useFilesStore();
  const [edit, setEdit] = useState<FileNode | null>(null);
  const [form] = useForm();
  const { previewWidth, previewRef, startResize: startPreviewResize } = useSplitter();
  const { register: registerPanel, startResize: startColumnResize } = useColumnResize(setPanelWidth);
  const actions = useFileActions();

  useEffect(() => {
    void boot();
  }, []);

  useEffect(() => {
    let timer: number | undefined;
    const subscribed = listen("files-changed", () => {
      /** 拷一个大文件夹会连着炸出几十条事件，攒一小会儿再读；重读本身也只碰当前看得见的标签 */
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        void useFilesStore.getState().reloadTabs();
      }, 250);
    });
    return () => {
      window.clearTimeout(timer);
      subscribed.then((unlisten) => unlisten());
    };
  }, []);

  /** 在资源管理器 / Outlook 里拷完再切回来，状态条和右键菜单里的「粘贴」得立刻跟着变 */
  useEffect(() => {
    const sync = () => void useFilesStore.getState().syncClipboard();
    window.addEventListener("focus", sync);
    return () => window.removeEventListener("focus", sync);
  }, []);

  /** 快捷键挂在页面上而不是某一行上：点过侧栏、表头之后焦点不在文件列表里，
      Ctrl+Z / Ctrl+V 也照样管用。输入框、弹窗里不抢，文件列表自己接过的也不重复执行 */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return;
      const el = e.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      if (document.querySelector('[role="dialog"]')) return;
      if (e.defaultPrevented) return;
      const key = e.key.toLowerCase();
      if (key === "z") {
        e.preventDefault();
        void actions.undo();
      } else if (key === "v") {
        e.preventDefault();
        if (!useFilesStore.getState().activePanelId) {
          message.info("先打开一个文件夹，再粘贴");
          return;
        }
        void actions.paste();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    if (!edit) {
      form.resetFields();
      return;
    }
    form.setFieldsValue({ alias: edit.alias ?? "", description: edit.description ?? "" });
  }, [edit, form]);

  const handleSaveMeta = async () => {
    if (!edit) return;
    try {
      const values = await form.validateFields();
      await saveMeta(
        edit.path,
        (values.alias ?? "").trim() || null,
        (values.description ?? "").trim() || null,
      );
      message.success("已保存");
      setEdit(null);
    } catch (e) {
      message.error(String(e));
    }
  };

  /** 预览跟着「当前栏、当前标签、当前列单选的那一项」走：点别的列会先把焦点挪过去 */
  const activePanel = panels.find((p) => p.id === activePanelId) ?? null;
  const activeTab = activePanel?.tabs.find((t) => t.id === activePanel.activeTabId) ?? null;
  const activeCol = activeTab
    ? activeTab.cols[Math.min(Math.max(activeTab.focus, 0), activeTab.cols.length - 1)]
    : null;
  const selectedNode: FileNode | null =
    activeCol?.selected.length === 1
      ? activeCol.nodes.find((n) => n.path === activeCol.selected[0]) ?? null
      : null;
  /** 预览栏只在选中具体文件时弹出来：选目录是「往下翻」的一步，多选也没法只讲一个，都收起 */
  const previewNode = selectedNode && !selectedNode.isDir ? selectedNode : null;

  /** 重命名和新建文件夹共用一个输入框弹窗：两者要问的就是一句话 */
  const [nameEdit, setNameEdit] = useState<{ mode: "new" | "rename" } | null>(null);
  const [nameValue, setNameValue] = useState("");

  const openNewFolder = () => {
    setNameValue("新建文件夹");
    setNameEdit({ mode: "new" });
  };
  const openRename = (node: FileNode) => {
    setNameValue(node.name);
    setNameEdit({ mode: "rename" });
  };
  const submitName = async () => {
    const store = useFilesStore.getState();
    const value = nameValue.trim();
    if (!value) {
      message.error("名字没填");
      return;
    }
    try {
      if (nameEdit?.mode === "new") await store.createFolderHere(value);
      else await store.renameSelected(value);
      setNameEdit(null);
    } catch (e) {
      message.error(String(e));
    }
  };

  const handlePinCurrent = () => {
    if (!activeCol) return;
    const already = pinned.includes(activeCol.path);
    void togglePin(activeCol.path);
    message.success(already ? "已从收藏移除" : "已收藏到侧栏");
  };

  const open = (path: string) => void enter(path);

  return (
    <div className="page">
      <div className="panes">
        <div className="side-pane files-side">
          <div className="stack">
            {!!pinned.length && <div className="label side-section">收藏</div>}
            {pinned.map((p) => (
              <SideItem
                key={p}
                path={p}
                label={folderName(p)}
                hint={p}
                icon={<Pin size={15} />}
                iconPath={icons[p]}
                active={activeCol?.path === p}
                onOpen={open}
                onRemove={(path) => void togglePin(path)}
              />
            ))}

            <div className="label side-section">常用</div>
            {knownFolders.map((k) => (
              <SideItem
                key={k.path}
                path={k.path}
                label={k.name}
                hint={k.path}
                icon={<Folder size={15} />}
                iconPath={icons[k.path]}
                active={activeCol?.path === k.path}
                onOpen={open}
              />
            ))}

            <div className="label side-section">此电脑</div>
            {drives.map((d) => (
              <SideItem
                key={d.path}
                path={d.path}
                label={`${d.label ?? d.kind} (${d.letter}:)`}
                hint={
                  d.free !== null && d.total !== null
                    ? `${d.path}\n剩余 ${formatBytes(d.free)} / 共 ${formatBytes(d.total)}`
                    : d.path
                }
                icon={d.kind === "网络驱动器" ? <Server size={15} /> : <HardDrive size={15} />}
                iconPath={icons[d.path]}
                active={activeCol?.path === d.path}
                onOpen={open}
              />
            ))}
            {!drives.length && <div className="label" style={{ padding: 16 }}>还没读到盘符</div>}
          </div>

          <div className="side-footer">
            <div className="side-toggle" title="隐藏和系统项摆不摆出来（.git、desktop.ini 这类）">
              <Eye size={14} />
              <span style={{ flex: 1 }}>隐藏项</span>
              <Switch size="small" checked={showHidden} onChange={(on) => void toggleShowHidden(on)} />
            </div>
            <div style={{ display: "flex", gap: 6 }}>
              <Button
                size="small"
                type="text"
                icon={<Pin size={14} />}
                onClick={handlePinCurrent}
                disabled={!activeCol}
                title={pinned.includes(activeCol?.path ?? "") ? "从收藏里移除当前目录" : "把当前目录放进收藏"}
              >
                {pinned.includes(activeCol?.path ?? "") ? "取消收藏" : "收藏这里"}
              </Button>
              <Button size="small" type="text" icon={<Columns2 size={14} />} onClick={() => void addPanel()}>
                新建分栏
              </Button>
            </div>
          </div>
        </div>

        <div className="file-main">
          <FileHeader panel={activePanel} />
          {!panels.length && <EmptyPanelTabs />}

          <div className="file-panes">
            <div className="file-columns">
              {panels.map((p, i) => (
                <Fragment key={p.id}>
                  {i > 0 && <div className="divider-v" onMouseDown={(e) => startColumnResize(e, panels[i - 1].id)} />}
                  <FilePanel
                    panel={p}
                    active={p.id === activePanelId}
                    previewOpen={!!previewNode}
                    pinned={pinned}
                    clipboard={clipboard}
                    icons={icons}
                    onActivate={() => void activatePanel(p.id)}
                    onEdit={setEdit}
                    onRename={openRename}
                    onNewFolder={openNewFolder}
                    registerRef={registerPanel(p.id)}
                  />
                </Fragment>
              ))}
              {!panels.length && (
                <div className="label" style={{ padding: 16 }}>
                  左边选一个位置，或点「新建分栏」
                </div>
              )}
            </div>

            {previewNode && (
              <>
                <div className="divider-v" onMouseDown={startPreviewResize} />

                <div
                  className="preview-pane"
                  ref={previewRef}
                  style={
                    previewWidth === null
                      ? { flex: "1 1 0", minWidth: 0 }
                      : { flex: "0 1 auto", minWidth: 0, width: previewWidth }
                  }
                >
                  <PreviewPanel
                    node={previewNode}
                    record={record}
                    preview={preview}
                    ready={authorizedFor === previewNode.path}
                  />
                </div>
              </>
            )}
          </div>
        </div>
      </div>

      <Modal
        title={`编辑 ${edit ? edit.name : ""}`}
        open={!!edit}
        onCancel={() => setEdit(null)}
        onOk={handleSaveMeta}
        okText="保存"
      >
        <Form form={form}>
          <FormItem name="alias" label="别名">
            <Input placeholder="留空则显示文件原名" />
          </FormItem>
          <FormItem name="description" label="说明">
            <TextArea autoSize={{ minRows: 3 }} placeholder="记录这个文件的用途、来源等信息" />
          </FormItem>
        </Form>
      </Modal>

      <Modal
        title={nameEdit?.mode === "new" ? "新建文件夹" : "重命名"}
        open={!!nameEdit}
        onCancel={() => setNameEdit(null)}
        onOk={() => void submitName()}
        okText="就这么定"
      >
        <Input
          value={nameValue}
          onChange={setNameValue}
          onEnter={() => void submitName()}
          autoFocus
          placeholder={nameEdit?.mode === "new" ? "文件夹名字" : "新名字"}
        />
        <div className="label" style={{ marginTop: 8, lineHeight: 1.7 }}>
          {nameEdit?.mode === "new"
            ? "撞名不覆盖，自动加 (1)(2) 让位。"
            : "撞名同样让位；别名和说明跟着新名字一起走。"}
        </div>
      </Modal>
    </div>
  );
}
