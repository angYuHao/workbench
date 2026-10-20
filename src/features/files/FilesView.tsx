import { useEffect, useMemo, useState } from "react";
import { ArrowUp, FileText, Folder, FolderOpen, Pencil, Plus, Trash2 } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { useFilesStore } from "../../stores/files";
import PreviewPanel from "./PreviewPanel";
import useSplitter from "./useSplitter";
import { openPath } from "../../api/shell";
import { Button, Form, FormItem, Input, Modal, SearchBox, Table, TextArea, message, pressable, useForm } from "../../ui";
import type { FileNode } from "../../types";

/** 文件栏的地板：缩到这里就顶住，分栏再也拖不动（溢出交给 flex，不出横向滚动条） */
const LIST_MIN_WIDTH = 240;

/** 目录名：取路径最后一段 */
function folderName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** 父目录路径，已在受管根目录时返回 null */
function parentOf(path: string, roots: string[]): string | null {
  if (roots.includes(path)) return null;
  const parent = path.replace(/[\\/][^\\/]*$/, "");
  return parent && parent !== path ? parent : null;
}

export default function FilesView() {
  const {
    roots,
    nodes,
    currentRoot,
    record,
    preview,
    loadRoots,
    addRoot,
    removeRoot,
    loadNodes,
    select,
    saveMeta,
  } = useFilesStore();
  const [keyword, setKeyword] = useState("");
  const [edit, setEdit] = useState<FileNode | null>(null);
  const [form] = useForm();
  const { previewWidth, previewRef, startResize } = useSplitter();

  useEffect(() => {
    loadRoots().then((r) => {
      if (r.length) loadNodes(r[0]);
    });
  }, []);

  useEffect(() => {
    const subscribed = listen("files-changed", () => {
      const { currentRoot: root, loadNodes: reload } = useFilesStore.getState();
      if (root) reload(root);
    });
    return () => {
      subscribed.then((unlisten) => unlisten());
    };
  }, []);

  const handleAddFolder = async () => {
    try {
      const selected = await openDialog({ directory: true, multiple: false });
      if (typeof selected === "string") await addRoot(selected);
    } catch (e) {
      message.error(String(e));
    }
  };

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

  const visible = useMemo(() => {
    const k = keyword.toLowerCase();
    if (!k) return nodes;
    return nodes.filter(
      (n) =>
        n.name.toLowerCase().includes(k) ||
        (n.alias ?? "").toLowerCase().includes(k) ||
        (n.description ?? "").toLowerCase().includes(k),
    );
  }, [nodes, keyword]);

  const parent = currentRoot ? parentOf(currentRoot, roots) : null;

  /** 双击和 Enter 都走这里：目录就是进去，文件交给系统程序打开 */
  const open = (node: FileNode) => {
    if (node.isDir) {
      loadNodes(node.path);
      return;
    }
    // 打不开现在会报错回来了（文件被删、没有关联程序），不接住就只是控制台里一条红字
    openPath(node.path).catch((e) => message.error(String(e)));
  };

  const selectedNode: FileNode | null = record
    ? {
        name: record.name,
        path: record.path,
        isDir: record.isDir,
        alias: record.alias,
        description: record.description,
      }
    : null;

  return (
    <div className="page">
      <div className="toolbar">
        <h2 className="toolbar-title">文件管理</h2>
        <Button icon={<Plus size={16} />} onClick={handleAddFolder}>
          添加文件夹
        </Button>
      </div>

      <div className="panes">
        <div className="side-pane">
          <div className="label" style={{ padding: "4px 8px" }}>
            受管文件夹
          </div>
          <div className="stack">
            {roots.map((r) => (
              <div
                key={r}
                className={currentRoot === r ? "nav-item active" : "nav-item"}
                onClick={() => loadNodes(r)}
                {...pressable(() => loadNodes(r))}
              >
                <FolderOpen size={15} />
                <span className="ellipsis" style={{ flex: 1 }}>
                  {folderName(r)}
                </span>
                <Trash2
                  className="icon-button"
                  size={13}
                  aria-label={`移除受管文件夹「${folderName(r)}」`}
                  {...pressable(() => removeRoot(r))}
                  onClick={(e) => {
                    e.stopPropagation();
                    removeRoot(r);
                  }}
                />
              </div>
            ))}
          </div>
          {roots.length === 0 && <div className="label" style={{ padding: 16 }}>尚未添加受管文件夹</div>}
        </div>

        <div className="column" style={{ minWidth: LIST_MIN_WIDTH }}>
          <div className="sub-toolbar">
            {parent && (
              <Button size="small" type="text" icon={<ArrowUp size={14} />} onClick={() => loadNodes(parent)}>
                上级
              </Button>
            )}
            <span className="ellipsis label" style={{ flex: 1, minWidth: 0 }}>
              {currentRoot ?? ""}
            </span>
            <SearchBox placeholder="搜索名称 / 别名 / 说明" style={{ width: 220, minWidth: 0 }} value={keyword} onChange={setKeyword} />
          </div>
          <div className="pane-body" style={{ padding: 8 }}>
            <Table
              dataSource={visible}
              rowKey={(r) => r.path}
              size="middle"
              onRow={(node) => ({
                onClick: () => select(node),
                onDoubleClick: () => open(node),
                /** 表格行不套 button 语义，键盘只用 Enter（空格留给以后可能的「勾选」） */
                ...pressable(() => open(node), { role: null, space: false }),
              })}
              rowClassName={(node) => (record?.path === node.path ? "row-selected" : "")}
              layout="fixed"
              columns={[
                {
                  title: "名称",
                  dataIndex: "name",
                  ellipsis: true,
                  render: (_: unknown, node: FileNode) => (
                    <span style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
                      {node.isDir ? (
                        <Folder size={16} style={{ flexShrink: 0 }} color="var(--color-warning)" />
                      ) : (
                        <FileText size={16} style={{ flexShrink: 0 }} color="var(--color-text-secondary)" />
                      )}
                      <span className="ellipsis" title={node.name}>
                        {node.name}
                      </span>
                    </span>
                  ),
                },
                {
                  title: "别名",
                  dataIndex: "alias",
                  width: "32%",
                  ellipsis: true,
                  render: (v: string | null) => <span title={v ?? undefined}>{v ?? "—"}</span>,
                },
                {
                  title: "",
                  width: 44,
                  render: (_: unknown, node: FileNode) => (
                    <Pencil
                      className="icon-button"
                      size={15}
                      aria-label={`编辑「${node.name}」`}
                      {...pressable(() => setEdit(node))}
                      onClick={(e) => {
                        e.stopPropagation();
                        setEdit(node);
                      }}
                      // 这一行双击是「打开」，不拦一下就会在编辑按钮上按两下顺手把文件也开了
                      onDoubleClick={(e) => e.stopPropagation()}
                    />
                  ),
                },
              ]}
            />
          </div>
        </div>

        <div className="divider-v" onMouseDown={startResize} />

        <div
          className="preview-pane"
          ref={previewRef}
          style={
            previewWidth === null
              ? { flex: "1 1 0", minWidth: 0 }
              : { flex: "0 1 auto", minWidth: 0, width: previewWidth }
          }
        >
          <PreviewPanel node={selectedNode} record={record} preview={preview} />
        </div>
      </div>

      <Modal
        title={`编辑 ${edit ? folderName(edit.path) : ""}`}
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
    </div>
  );
}
