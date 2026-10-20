import { useEffect, useMemo, useState } from "react";
import { Plus, RefreshCw, Sparkles, Square } from "lucide-react";
import { useDesktopStore } from "../../stores/desktop";
import { syncInstalledApps, saveApp } from "../../api/desktop";
import { deleteCategories, deleteCategory } from "../../api/categories";
import type { CategoryNode } from "../../shared/CategoryTree";
import CategoryTree from "../../shared/CategoryTree";
import { useCategoryScaffold } from "../../shared/useCategoryScaffold";
import AppIconGrid from "./AppIconGrid";
import FilterDropSlot from "./FilterDropSlot";
import SelectionBar from "../../shared/SelectionBar";
import SmartCategorizeModal from "../../shared/SmartCategorizeModal";
import { useIdSelection } from "../../shared/useIdSelection";
import { useSmartCategorize } from "../../shared/useSmartCategorize";
import { useSmartScope } from "../../shared/useSmartScope";
import {
  Button,
  Checkbox,
  Form,
  FormItem,
  Input,
  message,
  Modal,
  ModalConfirm,
  SearchBox,
  Select,
  Space,
  useForm,
} from "../../ui";
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

/** 只有程序进模型。老桌面登记那批里混着文档卡片（工资表这种），它们的名字一旦进提示词就出本机；
 * 而「按功能分类」说的是应用，文档本来就不该由模型猜——要归类自己拖到分类上。 */
const isProgram = (a: AppEntry) => {
  const t = a.target.toLowerCase();
  if (t.includes("://")) return true;
  const dot = t.lastIndexOf(".");
  return dot < 0 || PROGRAM_EXTS.includes(t.slice(dot + 1));
};

/**
 * 「应用」页：注册表里的已安装程序 + 开始菜单里没人认领的快捷方式收成卡片，双击打开。
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
      const lost = r.unresolved > 0 ? `，${r.unresolved} 个认不出要打开什么` : "";
      if (r.inserted > 0) message.success(`从注册表和开始菜单新收录 ${r.inserted} 个程序${skipped}${lost}`);
      else if (manual) message.info(`注册表和开始菜单都没有新程序，当前 ${r.total} 个${skipped}${lost}`);
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

  const { onlyNew, restart } = useSmartScope({
    items: visibleApps,
    categories,
    unit: "程序",
    start: smart.start,
    eligible: isProgram,
    toSmart: (a) => ({ id: a.id, name: a.name, target: a.target, url: null }),
  });

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
        <SearchBox placeholder="搜索程序名 / 路径" value={keyword} onChange={setKeyword} />
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
              onClick={() => restart(onlyNew)}
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
        <div className="side-pane apps-side">
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
          {/* 落点钉在这一栏最底下：分类多到要滚动时也留在视线里，图标大到能手能落在上面。
              提示用原生 title 不用 Tooltip —— 浮层绝对定位会撑大这一栏的横向滚动区，一悬停就出滚动条 */}
          <FilterDropSlot apps={apps} />
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
              checked={onlyNew}
              onChange={(v) => restart(v)}
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
