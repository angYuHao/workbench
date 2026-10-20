import { useEffect, useMemo, useState } from "react";
import { Plus, Sparkles, Square, Wand2 } from "lucide-react";
import { useBookmarksStore } from "../../stores/bookmarks";
import { blockBookmarks, openUrl } from "../../api/bookmarks";
import { deleteCategories, deleteCategory } from "../../api/categories";
import CategoryTree, { type CategoryNode } from "../../shared/CategoryTree";
import { useCategoryScaffold } from "../../shared/useCategoryScaffold";
import SelectionBar from "../../shared/SelectionBar";
import BookmarkAnalysisModal from "./BookmarkAnalysisModal";
import BookmarkCard from "./BookmarkCard";
import SmartCategorizeModal from "../../shared/SmartCategorizeModal";
import { useIdSelection } from "../../shared/useIdSelection";
import { useBookmarkAnalysis } from "./useBookmarkAnalysis";
import useBookmarkTrash from "./useBookmarkTrash";
import { useSmartCategorize } from "../../shared/useSmartCategorize";
import { useSmartScope } from "../../shared/useSmartScope";
import {
  Button,
  Checkbox,
  Dropdown,
  Form,
  FormItem,
  Input,
  message,
  Modal,
  ModalConfirm,
  SearchBox,
  Select,
  Space,
  Spin,
  TextArea,
  useForm,
} from "../../ui";
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

export default function BookmarksView() {
  const { bookmarks, categories, loading, load, import: doImport, save, remove } =
    useBookmarksStore();
  const [activeCategory, setActiveCategory] = useState<number | null>(null);
  const [keyword, setKeyword] = useState("");
  const [edit, setEdit] = useState<Bookmark | null>(null);
  const [form] = useForm();
  const selection = useIdSelection();
  const analysis = useBookmarkAnalysis({ onApplied: load });
  const trash = useBookmarkTrash({ revision: bookmarks.length, reload: load });
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

  const { onlyNew, restart } = useSmartScope({
    items: visible,
    categories,
    unit: "网址",
    start: smart.start,
    toSmart: (b) => ({ id: b.id, name: b.title, target: null, url: b.url }),
  });

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

  /** 双击和 Enter 走同一条：打开、记一次使用、回来刷新列表 */
  const open = (b: Bookmark) => {
    openUrl(b.url, b.id)
      .then(() => load())
      .catch((e) => message.error(String(e)));
  };

  return (
    <div className="page">
      <div className="toolbar">
        <h2 className="toolbar-title toolbar-title-pane">网页管理</h2>
        <SearchBox placeholder="搜索标题 / 网址 / 说明" value={keyword} onChange={setKeyword} />
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
              onClick={() => restart(onlyNew)}
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
          {trash.trash > 0 && (
            <div className="smart-foot" style={{ marginBottom: 8 }}>
              上次分析删掉了 {trash.trash} 条重复书签
              <Button size="small" type="link" onClick={() => void trash.restore()}>
                恢复
              </Button>
              <Button size="small" type="link" onClick={trash.drop}>
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
              {visible.map((b) => (
                <BookmarkCard
                  key={b.id}
                  bookmark={b}
                  selectMode={selection.active}
                  selected={selection.selectedIds.has(b.id)}
                  onToggle={() => selection.toggle(b.id)}
                  onOpen={() => open(b)}
                  onEdit={() => setEdit(b)}
                  onDelete={() =>
                    ModalConfirm({
                      title: "确认删除该网址？",
                      onOk: async () => remove(b.id),
                    })
                  }
                />
              ))}
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
            checked={onlyNew}
            onChange={(v) => restart(v)}
            label="只判「未分类」和落在「其他」里的，已经进了真分类的跳过"
          />
        }
      />
      <BookmarkAnalysisModal analysis={analysis} />
    </div>
  );
}
