import { useEffect, useMemo, useState } from "react";
import { Plus, RefreshCw, Search, Sparkles, Square } from "lucide-react";
import { useDesktopStore } from "../../stores/desktop";
import { syncInstalledApps, saveApp } from "../../api/desktop";
import { deleteCategories, deleteCategory } from "../../api/categories";
import type { CategoryNode } from "../../shared/CategoryTree";
import CategoryTree from "../../shared/CategoryTree";
import { useCategoryScaffold } from "../../shared/useCategoryScaffold";
import AppIconGrid from "./AppIconGrid";
import SelectionBar from "../../shared/SelectionBar";
import SmartCategorizeModal from "../../shared/SmartCategorizeModal";
import { useIdSelection } from "../../shared/useIdSelection";
import { useSmartCategorize } from "../../shared/useSmartCategorize";
import { Button, Checkbox, Form, FormItem, Input, Modal, ModalConfirm, Select, Space, message, useForm } from "../../ui";
import { stripExt } from "../../utils/fileKind";
import type { AppEntry } from "../../types";

const emptyApp: AppEntry = {
  id: 0,
  name: "",
  alias: null,
  description: null,
  target: "",
  args: null,
  iconPath: null,
  categoryId: null,
  source: "manual",
  createdAt: null,
  useCount: 0,
};

/**
 * 「应用」页：把开始菜单里的快捷方式收成卡片，双击打开。
 * 数据在打开时同步一次（只增不改），分类按用途交给智能分类，跟桌面上的分类规则无关。
 */
export default function AppsView() {
  const { apps, categories, loadApps, loadCategories, removeApp } = useDesktopStore();
  const [activeCategory, setActiveCategory] = useState<number | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [keyword, setKeyword] = useState("");
  const [syncing, setSyncing] = useState(false);
  const [form] = useForm();
  const selection = useIdSelection();

  const categoryNames = useMemo(() => categories.map((c) => c.name), [categories]);
  const reloadAll = async () => {
    await Promise.all([loadApps(), loadCategories()]);
  };
  const smart = useSmartCategorize({
    itemType: "app",
    unit: "程序",
    categoryNames,
    // 模型现编的分类名会当场建出来，两边都要重读
    onApplied: reloadAll,
  });

  /** 只往库里添新面孔：改过名、归好类的卡片不会被一次同步抹掉 */
  const sync = async () => {
    setSyncing(true);
    try {
      const n = await syncInstalledApps();
      await loadApps();
      if (n > 0) message.success(`从开始菜单新收录 ${n} 个程序`);
    } catch (e) {
      message.error(String(e));
    } finally {
      setSyncing(false);
    }
  };

  useEffect(() => {
    // 拉不到要说出来：不然页面一片空白，看着像「没数据」而不是「出错了」
    void sync();
    loadCategories().catch((e) => message.error(String(e)));
  }, []);

  useEffect(() => {
    if (!addOpen) form.resetFields();
  }, [addOpen, form]);

  const { catNodes, addCategory, renameCat, dropOnCategory } = useCategoryScaffold({
    type: "app",
    categories,
    items: apps,
    labelOf: (a) => stripExt(a.name),
    save: saveApp,
    reload: reloadAll,
    // 改名在后端会顺带改写分类规则和桌面上的分类文件夹，提示要说清它动了什么
    renamed: (touched, name) =>
      touched ? `已改名为「${name}」，顺带改了 ${touched} 条分类规则` : `已改名为「${name}」`,
    // save_app 是整条更新，拖完要重新拉一次列表才看得见新归属
    afterMove: loadApps,
  });

  const visibleApps = useMemo(() => {
    const k = keyword.trim().toLowerCase();
    return apps
      .filter((a) => activeCategory === null || a.categoryId === activeCategory)
      .filter(
        (a) =>
          !k ||
          stripExt(a.name).toLowerCase().includes(k) ||
          (a.alias ?? "").toLowerCase().includes(k) ||
          a.target.toLowerCase().includes(k),
      );
  }, [apps, activeCategory, keyword]);

  /** 智能分类的「只处理新增」：未分类和堆在「其他」里的才判，已经进了真分类的跳过 */
  const [smartOnlyNew, setSmartOnlyNew] = useState(true);
  const otherCategoryId = useMemo(() => categories.find((c) => c.name === "其他")?.id ?? null, [categories]);
  const smartTargets = (onlyNew: boolean) =>
    visibleApps
      .filter((a) => !onlyNew || a.categoryId === null || a.categoryId === otherCategoryId)
      .map((a) => ({ id: a.id, name: a.name, target: a.target, url: null }));

  /** 打开（或换口径重开）智能分类窗口。一个都不剩时退回全量：勾画在窗口里，窗口不开他就没地方取消 */
  const restartSmart = (onlyNew: boolean) => {
    const list = smartTargets(onlyNew);
    if (list.length) {
      setSmartOnlyNew(onlyNew);
      smart.start(list);
      return;
    }
    setSmartOnlyNew(false);
    message.info("按「只处理新增」没有要判的程序了：已去掉勾，连已经归好类的一起判");
    smart.start(smartTargets(false));
  };

  const deleteCat = (node: CategoryNode) =>
    ModalConfirm({
      title: `删除分类「${node.name}」？`,
      content: node.count
        ? `这个分类下 ${node.count} 个程序会退回未分类，程序本身不会被删除。`
        : "这个分类下没有程序，删掉不影响别的。",
      onOk: async () => {
        const moved = await deleteCategory(node.id);
        message.success(moved ? `已删除，${moved} 个程序退回未分类` : "已删除");
        await Promise.all([loadApps(), loadCategories()]);
      },
    });

  const deleteCats = (nodes: CategoryNode[]) => {
    const count = nodes.reduce((sum, n) => sum + n.count, 0);
    ModalConfirm({
      title: `删除选中的 ${nodes.length} 个分类？`,
      content: count
        ? `分类下的 ${count} 个程序只退回未分类，不会被删除。`
        : "这几个分类下都没有程序。",
      onOk: async () => {
        const [moved] = await deleteCategories(nodes.map((n) => n.id));
        message.success(`已删除 ${nodes.length} 个分类，${moved} 个程序退回未分类`);
        await Promise.all([loadApps(), loadCategories()]);
      },
    });
  };

  const handleSaveApp = async () => {
    try {
      const values = await form.validateFields();
      await saveApp({ ...emptyApp, ...values });
      message.success("已添加");
      setAddOpen(false);
      await loadApps();
    } catch (e) {
      if (String(e).includes("必填")) message.error(String(e));
    }
  };

  const handleDelete = (ids: number[]) => {
    if (ids.length === 0) return;
    ModalConfirm({
      title: `确认删除选中的 ${ids.length} 个程序？`,
      content: "仅从启动器移除，不会卸载本机软件。下次同步也不会再把它收回来。",
      onOk: async () => {
        for (const id of ids) await removeApp(id);
        message.success(`已删除 ${ids.length} 个程序`);
        selection.stop();
      },
    });
  };

  return (
    <div className="page">
      <div className="toolbar">
        <h2 className="toolbar-title">应用</h2>
        <Input
          allowClear
          prefix={<Search size={14} />}
          placeholder="搜索程序名 / 路径"
          style={{ width: 220 }}
          value={keyword}
          onChange={setKeyword}
        />
        <div className="toolbar-spacer" />
        {selection.active ? (
          <SelectionBar
            selection={selection}
            allIds={visibleApps.map((a) => a.id)}
            onDelete={handleDelete}
          />
        ) : (
          <Space>
            <Button icon={<RefreshCw size={16} />} loading={syncing} onClick={sync}>
              重新同步
            </Button>
            <Button icon={<Plus size={16} />} onClick={() => setAddOpen(true)}>
              添加程序
            </Button>
            <Button
              type="primary"
              icon={<Sparkles size={16} />}
              onClick={() => restartSmart(smartOnlyNew)}
              disabled={visibleApps.length === 0}
            >
              智能分类
            </Button>
            <Button icon={<Square size={16} />} onClick={selection.start} disabled={visibleApps.length === 0}>
              选择
            </Button>
          </Space>
        )}
      </div>

      <div className="panes">
        <div className="side-pane">
          <CategoryTree
            categories={catNodes}
            total={apps.length}
            selected={activeCategory}
            onSelect={setActiveCategory}
            onCreate={addCategory}
            onRename={renameCat}
            onDropItem={dropOnCategory}
            onDeleteMany={deleteCats}
            onDelete={deleteCat}
          />
        </div>
        <div className="content-pane">
          <AppIconGrid
            apps={visibleApps}
            selectMode={selection.active}
            selectedIds={selection.selectedIds}
            onToggleSelect={selection.toggle}
            onLaunched={loadApps}
          />
        </div>
      </div>

      <Modal
        title="添加程序"
        open={addOpen}
        onCancel={() => setAddOpen(false)}
        onOk={handleSaveApp}
        okText="保存"
      >
        <Form form={form}>
          <FormItem name="name" label="名称" rules={[{ required: true }]}>
            <Input />
          </FormItem>
          <FormItem name="target" label="目标路径 (exe/lnk)" rules={[{ required: true }]}>
            <Input />
          </FormItem>
          <FormItem name="categoryId" label="分类">
            <Select
              allowClear
              placeholder="未分类"
              options={categories.map((c) => ({ label: c.name, value: c.id }))}
            />
          </FormItem>
          <FormItem name="args" label="启动参数">
            <Input placeholder="可选" />
          </FormItem>
        </Form>
      </Modal>

      <SmartCategorizeModal
        smart={smart}
        nameTitle="名称"
        stripExtension
        extra={
          <Checkbox
            checked={smartOnlyNew}
            onChange={(v) => restartSmart(v)}
            label="只判「未分类」和落在「其他」里的，已经进了真分类的跳过"
          />
        }
      />
    </div>
  );
}
