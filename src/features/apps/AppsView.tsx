import { useEffect, useMemo, useState } from "react";
import { Filter, Plus, RefreshCw, Search, Sparkles, Square } from "lucide-react";
import { useDesktopStore } from "../../stores/desktop";
import { addAppFilter, syncInstalledApps, saveApp } from "../../api/desktop";
import { deleteCategories, deleteCategory } from "../../api/categories";
import type { CategoryNode } from "../../shared/CategoryTree";
import CategoryTree, { ITEM_MIME } from "../../shared/CategoryTree";
import { useCategoryScaffold } from "../../shared/useCategoryScaffold";
import AppIconGrid from "./AppIconGrid";
import SelectionBar from "../../shared/SelectionBar";
import SmartCategorizeModal from "../../shared/SmartCategorizeModal";
import { useIdSelection } from "../../shared/useIdSelection";
import { useSmartCategorize } from "../../shared/useSmartCategorize";
import { Button, Checkbox, Form, FormItem, Input, Modal, ModalConfirm, Select, Space, Tooltip, message, useForm } from "../../ui";
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

/** 智能分类只碰这些后缀，其余（文档、压缩包、文件夹）留在本机人工归 */
const PROGRAM_EXTS = ["lnk", "exe", "app", "msi", "bat", "cmd", "com", "scr"];

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

  /**
   * 只往库里添新面孔：改过名、归好类的卡片不会被一次同步抹掉。
   * 自动那次（进页面）只在真有新东西时吭声，手动点的必须有回话，不然看着像没反应。
   */
  const sync = async (manual: boolean) => {
    setSyncing(true);
    try {
      const r = await syncInstalledApps();
      await loadApps();
      const skipped = r.filtered > 0 ? `，按过滤名单跳过 ${r.filtered} 个` : "";
      if (r.inserted > 0) message.success(`从开始菜单新收录 ${r.inserted} 个程序${skipped}`);
      else if (manual) message.info(`开始菜单里没有新程序，当前 ${r.total} 个${skipped}`);
    } catch (e) {
      message.error(String(e));
    } finally {
      setSyncing(false);
    }
  };

  useEffect(() => {
    // 拉不到要说出来：不然页面一片空白，看着像「没数据」而不是「出错了」
    void sync(false);
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
  /**
   * 只有程序进模型。老桌面登记那批里混着文档卡片（工资表这种），它们的名字一旦进提示词就出本机；
   * 而「按功能分类」说的是应用，文档本来就不该由模型猜——要归类自己拖到分类上。
   */
  const isProgram = (a: AppEntry) => {
    const t = a.target.toLowerCase();
    if (t.includes("://")) return true;
    const dot = t.lastIndexOf(".");
    return dot < 0 || PROGRAM_EXTS.includes(t.slice(dot + 1));
  };
  const smartTargets = (onlyNew: boolean) =>
    visibleApps
      .filter((a) => isProgram(a))
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

  const [filterOver, setFilterOver] = useState(false);

  /** 拖过来的只认自家卡片；从资源管理器拖文件进来仍由 App.tsx 那层吞掉 */
  const carriesItem = (e: React.DragEvent) => e.dataTransfer.types.includes(ITEM_MIME);

  /**
   * 拖一张卡片到「拖来过滤」上：把这个名字加进过滤名单，同时删掉卡片。
   * 模式取去扩展名的显示名（`Loomy.lnk` → `Loomy`），装到新版本也还认得；
   * 名单本身在 设置 → 应用过滤 里能改能删，也能试算它到底挡了谁。
   */
  const dropToFilter = async (id: number) => {
    const app = apps.find((a) => a.id === id);
    if (!app) return;
    const pattern = stripExt(app.name).trim();
    if (!pattern) {
      message.error("这张卡片的名字是空的，过滤了也挡不住东西");
      return;
    }
    try {
      const added = await addAppFilter(pattern);
      await removeApp(app.id);
      message.success(
        added
          ? `已过滤「${pattern}」：卡片移除了，以后同步不再收它（想改想去 设置 → 应用过滤）`
          : `「${pattern}」本来就在过滤名单里，卡片移除了`
      );
    } catch (e) {
      message.error(String(e));
    }
  };

  const handleDelete = (ids: number[]) => {
    if (ids.length === 0) return;
    ModalConfirm({
      title: `确认删除选中的 ${ids.length} 个程序？`,
      content: "只从这一页移除，不卸载软件；下次同步（重进这页或点「重新同步」）会再把它收回来。",
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
            <Button icon={<RefreshCw size={16} />} loading={syncing} onClick={() => void sync(true)}>
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
          <Tooltip title="把一张卡片拖到这里：名字进过滤名单，卡片当场消失，以后同步不再收它。名单在 设置 → 应用过滤 里能改能删，那里还能试算挡了谁。" placement="right">
            <div
              className={filterOver ? "filter-drop on" : "filter-drop"}
              aria-label="拖到此处过滤"
              onDragOver={(e) => {
                if (!carriesItem(e)) return;
                e.preventDefault();
                // 卡片那边 effectAllowed 写的是 move，这里必须也用 move：
                // 两个不一致时 Chrome 直接把这次拖动判成 none，drop 根本不会发出来
                e.dataTransfer.dropEffect = "move";
                if (!filterOver) setFilterOver(true);
              }}
              onDragLeave={() => setFilterOver(false)}
              onDrop={(e) => {
                if (!carriesItem(e)) return;
                e.preventDefault();
                setFilterOver(false);
                const id = Number(e.dataTransfer.getData(ITEM_MIME));
                if (Number.isInteger(id) && id > 0) void dropToFilter(id);
              }}
            >
              <Filter size={15} strokeWidth={1.6} />
            </div>
          </Tooltip>
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
          <>
            <Checkbox
              checked={smartOnlyNew}
              onChange={(v) => restartSmart(v)}
              label="只判「未分类」和落在「其他」里的，已经进了真分类的跳过"
            />
            <span className="label">
              只把程序发给模型：从老桌面登记里留下的文档、压缩包那类卡片不在这儿判，要归类就把它拖到左边的分类上。
            </span>
          </>
        }
      />
    </div>
  );
}
