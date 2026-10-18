import { useEffect, useMemo, useState } from "react";
import { Globe, Pencil, Plus, Search, Sparkles, Square, Trash2, Wand2 } from "lucide-react";
import { useBookmarksStore } from "../../stores/bookmarks";
import { openUrl } from "../../api/bookmarks";
import { deleteCategories, deleteCategory } from "../../api/categories";
import {
  blockBookmarks,
  emptyBookmarkTrash,
  getBookmarkTrashCount,
  restoreBookmarkTrash,
} from "../../api/bookmarks";
import CategoryTree, { ITEM_MIME, type CategoryNode } from "../../shared/CategoryTree";
import { useCategoryScaffold } from "../../shared/useCategoryScaffold";
import SelectionBar from "../../shared/SelectionBar";
import SelectTick from "../../shared/SelectTick";
import BookmarkAnalysisModal from "./BookmarkAnalysisModal";
import SmartCategorizeModal from "../../shared/SmartCategorizeModal";
import { useIdSelection } from "../../shared/useIdSelection";
import { useBookmarkAnalysis } from "./useBookmarkAnalysis";
import { useSmartCategorize } from "../../shared/useSmartCategorize";
import { Button, Checkbox, Dropdown, Form, FormItem, Input, Modal, ModalConfirm, Select, Space, Spin, TextArea, message, pressable, useForm } from "../../ui";
import type { Bookmark } from "../../types";

const emptyBookmark: Bookmark = {
  id: 0,
  title: "",
  url: "",
  description: null,
  categoryId: null,
  source: "manual",
  createdAt: null,
  useCount: 0,
};

const BROWSERS = [
  { key: "chrome", label: "从 Chrome 导入" },
  { key: "edge", label: "从 Edge 导入" },
];

function Favicon({ url }: { url: string | null }) {
  const [error, setError] = useState(false);
  if (!url || error) {
    return <Globe size={20} color="var(--color-text-secondary)" />;
  }

  let domain = "";
  try {
    domain = new URL(url).hostname;
  } catch {
    // 非法网址时退化为图标占位
  }

  return (
    <img
      src={domain ? `https://www.google.com/s2/favicons?domain=${domain}&sz=64` : ""}
      width={20}
      height={20}
      alt=""
      draggable={false}
      style={{ borderRadius: "var(--radius-icon)", flexShrink: 0 }}
      onError={() => setError(true)}
    />
  );
}

export default function BookmarksView() {
  const { bookmarks, categories, loading, load, import: doImport, save, remove } =
    useBookmarksStore();
  const [activeCategory, setActiveCategory] = useState<number | null>(null);
  const [keyword, setKeyword] = useState("");
  const [edit, setEdit] = useState<Bookmark | null>(null);
  const [form] = useForm();
  const selection = useIdSelection();
  const analysis = useBookmarkAnalysis({ onApplied: load });
  const [trash, setTrash] = useState(0);
  const smart = useSmartCategorize({
    itemType: "bookmark",
    unit: "网址",
    categoryNames: categories.map((c) => c.name),
    onApplied: load,
  });

  useEffect(() => {
    // 拉不到要说出来：不然页面一片空白，看着像「没数据」而不是「出错了」
    load().catch((e) => message.error(String(e)));
  }, []);

  // 分析删掉的重复书签进的是回收表：列表一有变化就重新看一眼有没有得撤销
  useEffect(() => {
    getBookmarkTrashCount().then(setTrash).catch(() => setTrash(0));
  }, [bookmarks.length]);

  const restoreTrash = async () => {
    try {
      const n = await restoreBookmarkTrash();
      message.success(`已放回 ${n} 条书签，重复标记会重新出现`);
      setTrash(0);
      await load();
    } catch (e) {
      message.error(String(e));
    }
  };

  // 多选拉黑只挡这几条地址本身；整站屏蔽要在 设置 → 网址黑名单 里写域名
  const blockSelected = (ids: number[]) => {
    ModalConfirm({
      title: `把选中的 ${ids.length} 条网址拉黑？`,
      content: "拉黑是从页面上藏起来，不删数据：以后导入浏览器收藏夹也不会再进来，取消拉黑原条目带分类和简介一起回来。这里挡的是这几条地址本身，要屏蔽整站请去 设置 → 网址黑名单 填域名（如 douban.com，连子页面一起挡）。",
      onOk: async () => {
        try {
          const n = await blockBookmarks(ids);
          selection.stop();
          await load();
          message.success(`已拉黑，新增 ${n} 条规则`);
        } catch (e) {
          message.error(String(e));
        }
      },
    });
  };

  const dropTrash = () => {
    ModalConfirm({
      title: `确认不要这 ${trash} 条被删的重复书签？`,
      content: "清掉后就只能从数据库备份里找回了。",
      onOk: async () => {
        await emptyBookmarkTrash();
        setTrash(0);
        message.success("已清掉");
      },
    });
  };

  useEffect(() => {
    if (!edit) {
      form.resetFields();
      return;
    }
    form.setFieldsValue({
      title: edit.title,
      url: edit.url,
      description: edit.description,
      categoryId: edit.categoryId,
    });
  }, [edit, form]);

  const { catNodes, addCategory, renameCat, dropOnCategory } = useCategoryScaffold({
    type: "bookmark",
    categories,
    items: bookmarks,
    labelOf: (b) => b.title,
    // save 是整条更新，所以拖拽时把原来那条原样带回去只换 categoryId
    save,
    // store 的 save 自己会刷新，所以这里不给 afterMove
    reload: load,
  });

  const visible = useMemo(() => {
    const k = keyword.toLowerCase();
    return bookmarks.filter(
      (b) =>
        (activeCategory === null || b.categoryId === activeCategory) &&
        (!k ||
          b.title.toLowerCase().includes(k) ||
          b.url.toLowerCase().includes(k) ||
          (b.description ?? "").toLowerCase().includes(k)),
    );
  }, [bookmarks, activeCategory, keyword]);

  /** 智能分类的「只处理新增」：还没归类的（含堆在「其他」里的）才判，进了真分类的跳过 */
  const [smartOnlyNew, setSmartOnlyNew] = useState(true);
  const otherCategoryId = categories.find((c) => c.name === "其他")?.id ?? null;
  const smartTargets = (onlyNew: boolean) =>
    visible
      .filter((b) => !onlyNew || b.categoryId === null || b.categoryId === otherCategoryId)
      .map((b) => ({ id: b.id, name: b.title, target: null, url: b.url }));

  /** 打开（或换口径重开）智能分类窗口。一个都不剩时退回全量：勾画在窗口里，窗口不开他就没地方取消 */
  const restartSmart = (onlyNew: boolean) => {
    const list = smartTargets(onlyNew);
    if (list.length) {
      setSmartOnlyNew(onlyNew);
      smart.start(list);
      return;
    }
    setSmartOnlyNew(false);
    message.info("按「只处理新增」没有要判的网址了：已去掉勾，连已经归好类的一起判");
    smart.start(smartTargets(false));
  };

  // 书签这边没有分类规则，一次确认就够
  const deleteBatch = (nodes: CategoryNode[]) => {
    const items = nodes.reduce((sum, n) => sum + n.count, 0);
    ModalConfirm({
      title: `删除选中的 ${nodes.length} 个分类？`,
      content: `${items} 条网址退回未分类，网址本身不会被删除。${nodes.map((n) => n.name).join("、")}`,
      okText: "删除",
      onOk: async () => {
        try {
          const [moved] = await deleteCategories(nodes.map((n) => n.id));
          message.success(`已删除 ${nodes.length} 个分类，${moved} 条网址退回未分类`);
          await load();
        } catch (e) {
          message.error(String(e));
        }
      },
    });
  };

  const removeCategory = (node: CategoryNode) => {
    ModalConfirm({
      title: `删除分类「${node.name}」？`,
      content: node.count
        ? `${node.count} 条网址会退回未分类，网址本身不会被删除。`
        : "这个分类下没有内容，删掉不影响别的。",
      onOk: async () => {
        const moved = await deleteCategory(node.id);
        message.success(moved ? `已删除，${moved} 项退回未分类` : "已删除");
        await load();
      },
    });
  };

  const handleImport = async (browser: string) => {
    try {
      const r = await doImport(browser);
      message.success(`导入完成：新增 ${r.imported}，跳过 ${r.skipped}`);
    } catch (e) {
      message.error(String(e));
    }
  };

  const handleSave = async () => {
    try {
      const values = await form.validateFields();
      await save({ ...(edit ?? emptyBookmark), ...values });
      message.success("已保存");
      setEdit(null);
    } catch (e) {
      if (String(e).includes("必填") || String(e).includes("有效")) message.error(String(e));
    }
  };

  const handleDelete = (ids: number[]) => {
    if (ids.length === 0) return;
    ModalConfirm({
      title: `确认删除选中的 ${ids.length} 个网址？`,
      onOk: async () => {
        for (const id of ids) await remove(id);
        message.success(`已删除 ${ids.length} 个网址`);
        selection.stop();
      },
    });
  };

  return (
    <div className="page">
      <div className="toolbar">
        <h2 className="toolbar-title toolbar-title-pane">网页管理</h2>
        <Input
          allowClear
          prefix={<Search size={14} />}
          placeholder="搜索标题 / 网址 / 说明"
          style={{ width: 220 }}
          value={keyword}
          onChange={setKeyword}
        />
        <div className="toolbar-spacer" />
        {selection.active ? (
          <SelectionBar
            selection={selection}
            allIds={visible.map((b) => b.id)}
            onDelete={handleDelete}
            onBlock={blockSelected}
          />
        ) : (
          <Space>
            <Dropdown menu={{ items: BROWSERS, onClick: ({ key }) => handleImport(key) }}>
              <Button>导入书签</Button>
            </Dropdown>
            <Button
              type="primary"
              icon={<Plus size={16} />}
              onClick={() => setEdit({ ...emptyBookmark, categoryId: activeCategory })}
            >
              添加网址
            </Button>
            <Button
              icon={<Sparkles size={16} />}
              onClick={() => restartSmart(smartOnlyNew)}
              disabled={visible.length === 0}
            >
              智能分类
            </Button>
            <Button
              icon={<Wand2 size={16} />}
              onClick={() => analysis.start(visible.map((b) => b.id))}
              disabled={visible.length === 0}
            >
              智能分析
            </Button>
            <Button
              icon={<Square size={16} />}
              onClick={selection.start}
              disabled={visible.length === 0}
            >
              选择
            </Button>
          </Space>
        )}
      </div>

      <div className="panes">
        <div className="side-pane">
          <CategoryTree
            categories={catNodes}
            total={bookmarks.length}
            selected={activeCategory}
            onSelect={setActiveCategory}
            onCreate={addCategory}
            onRename={renameCat}
            onDropItem={dropOnCategory}
            onDeleteMany={deleteBatch}
            onDelete={removeCategory}
          />
        </div>

        <div className="content-pane">
          {trash > 0 && (
            <div className="smart-foot" style={{ marginBottom: 8 }}>
              上次分析删掉了 {trash} 条重复书签
              <Button size="small" type="link" onClick={() => void restoreTrash()}>
                恢复
              </Button>
              <Button size="small" type="link" onClick={dropTrash}>
                不再恢复
              </Button>
            </div>
          )}
          {loading ? (
            <div style={{ textAlign: "center", padding: 40 }}>
              <Spin />
            </div>
          ) : visible.length === 0 ? (
            <div className="label" style={{ textAlign: "center", padding: 60 }}>
              暂无网址，点击「导入书签」或「添加网址」
            </div>
          ) : (
            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fill, minmax(300px, 1fr))",
                gap: 12,
              }}
            >
              {visible.map((b) => {
                const checked = selection.selectedIds.has(b.id);
                return (
                  <div
                    key={b.id}
                    className={
                      selection.active && checked
                        ? "card card-hover tile bookmark-card selected"
                        : "card card-hover tile bookmark-card"
                    }
                    draggable
                    // 按住拖到左边的分类上就是改分类；打开改成双击，单击只在选择模式里用来勾选
                    // Enter 是键盘上等价的那一下双击（键盘没法双击）
                    title="双击打开"
                    aria-label={b.title}
                    onDragStart={(e) => {
                      e.dataTransfer.setData(ITEM_MIME, String(b.id));
                      e.dataTransfer.effectAllowed = "move";
                    }}
                    onClick={() => selection.active && selection.toggle(b.id)}
                    onDoubleClick={() => {
                      if (selection.active) return;
                      openUrl(b.url, b.id)
                        .then(() => load())
                        .catch((e) => message.error(String(e)));
                    }}
                    {...pressable(() => {
                      if (selection.active) {
                        selection.toggle(b.id);
                        return;
                      }
                      openUrl(b.url, b.id)
                        .then(() => load())
                        .catch((e) => message.error(String(e)));
                    })}
                  >
                    {selection.active && <SelectTick selected={checked} />}
                    <Favicon url={b.url} />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div className="ellipsis" style={{ fontWeight: 600 }}>
                        {b.title}
                        {b.useCount > 0 && <span className="label"> · {b.useCount} 次</span>}
                      </div>
                      <div className="ellipsis label">{b.url}</div>
                      {b.description && (
                        <div className="label" style={{ marginTop: 2 }}>
                          {b.description}
                        </div>
                      )}
                    </div>
                    {!selection.active && (
                      <Space
                        size={4}
                        style={{ alignSelf: "flex-start" }}
                        onClick={(e) => e.stopPropagation()}
                        // 双击也拦在这里，不然在编辑按钮上按两下就把网址打开了
                        onDoubleClick={(e) => e.stopPropagation()}
                      >
                        <Pencil className="icon-button" size={15} onClick={() => setEdit(b)} />
                        <Trash2
                          className="icon-button"
                          size={15}
                          onClick={() =>
                            ModalConfirm({
                              title: "确认删除该网址？",
                              onOk: async () => remove(b.id),
                            })
                          }
                        />
                      </Space>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>

      <Modal
        title={edit?.id ? "编辑网址" : "添加网址"}
        open={!!edit}
        onCancel={() => setEdit(null)}
        onOk={handleSave}
        okText="保存"
      >
        <Form form={form}>
          <FormItem name="title" label="标题" rules={[{ required: true }]}>
            <Input />
          </FormItem>
          <FormItem name="url" label="网址" rules={[{ required: true, type: "url" }]}>
            <Input />
          </FormItem>
          <FormItem name="description" label="说明">
            <TextArea autoSize={{ minRows: 2 }} />
          </FormItem>
          <FormItem name="categoryId" label="分类">
            <Select
              allowClear
              placeholder="未分类"
              options={categories.map((c) => ({ label: c.name, value: c.id }))}
            />
          </FormItem>
        </Form>
      </Modal>

      <SmartCategorizeModal
        smart={smart}
        nameTitle="标题"
        hint="书签不给模型看现有分类，分类名它自己定，并要求整批收到少数几个大类里（8 个封顶、宁宽勿细）。它只拿到标题和网址；名字或分类不满意，直接在这行改。"
        extra={
          <Checkbox
            checked={smartOnlyNew}
            onChange={(v) => restartSmart(v)}
            label="只判「未分类」和落在「其他」里的，已经进了真分类的跳过"
          />
        }
      />
      <BookmarkAnalysisModal analysis={analysis} />
    </div>
  );
}
