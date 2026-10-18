import { useEffect, useMemo, useState } from "react";
import { Plus, Search, Sparkles, Square, Undo2, Wand2 } from "lucide-react";
import { useDesktopStore } from "../../stores/desktop";
import { useSettingsStore } from "../../stores/settings";
import { organizeDesktop, revertOrganize, saveApp, scanDesktop } from "../../api/desktop";
import { deleteCategories, deleteCategory, deleteRule } from "../../api/categories";
import CategoryTree, { type CategoryNode } from "../../shared/CategoryTree";
import { useCategoryScaffold } from "../../shared/useCategoryScaffold";
import AppIconGrid from "./AppIconGrid";
import SelectionBar from "../../shared/SelectionBar";
import SmartCategorizeModal from "../../shared/SmartCategorizeModal";
import { useIdSelection } from "../../shared/useIdSelection";
import { useSmartCategorize } from "../../shared/useSmartCategorize";
import { Button, Checkbox, Form, FormItem, Input, Modal, ModalConfirm, Select, Space, Table, message, useForm } from "../../ui";
import { stripExt } from "../../utils/fileKind";
import type { AppEntry, DesktopItem, OrganizePlan } from "../../types";

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

const REMOVE_HINT = "仅从启动器移除，不会删除实际文件。";

export default function DesktopView() {
  const { apps, categories, loadApps, loadCategories, removeApp } = useDesktopStore();
  const { rules, loadRules } = useSettingsStore();
  const [activeCategory, setActiveCategory] = useState<number | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [organizeOpen, setOrganizeOpen] = useState(false);
  const [keyword, setKeyword] = useState("");
  const [organizing, setOrganizing] = useState(false);
  const [reverting, setReverting] = useState(false);
  const [registerOnly, setRegisterOnly] = useState(true);
  /** 只处理新增：桌面上那些已经在启动器里的项不再动它们 */
  const [onlyNew, setOnlyNew] = useState(true);
  const [plans, setPlans] = useState<DesktopItem[]>([]);
  const [catDel, setCatDel] = useState<CategoryNode | null>(null);
  const [catBatch, setCatBatch] = useState<CategoryNode[] | null>(null);
  const [form] = useForm();
  const selection = useIdSelection();

  const loadAll = async () => {
    await Promise.all([loadApps(), loadCategories(), loadRules()]);
  };
  const categoryNames = useMemo(() => categories.map((c) => c.name), [categories]);
  const smart = useSmartCategorize({
    itemType: "app",
    unit: "程序",
    categoryNames,
    onApplied: loadAll,
  });

  useEffect(() => {
    // 拉不到要说出来：不然页面一片空白，看着像「没数据」而不是「出错了」
    loadAll().catch((e) => message.error(String(e)));
  }, []);

  useEffect(() => {
    if (!addOpen) form.resetFields();
  }, [addOpen, form]);

  /** 规则里的分类名要一直挂在树上：分类行被删过、但规则还在的，下次整理仍会用到这个名字 */
  const ruleNames = useMemo(() => rules.map((r) => r.category), [rules]);
  const { catNodes, addCategory, renameCat, dropOnCategory } = useCategoryScaffold({
    type: "app",
    categories,
    items: apps,
    labelOf: (a) => stripExt(a.name),
    save: saveApp,
    reload: loadAll,
    ghostNames: ruleNames,
    // 规则是按名字挂分类的，所以改名要听后端报回来的条数，别说「只改了个名」
    renamed: (touched, name) =>
      touched ? `已改名为「${name}」，顺带改了 ${touched} 条分类规则` : `已改名为「${name}」`,
    // save_app 是整条更新，拖完要重新拉一次列表才看得见新归属
    afterMove: loadApps,
  });
  const rulesOf = (name: string) => rules.filter((r) => r.category === name);
  const ruleCount = catDel ? rulesOf(catDel.name).length : 0;

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

  const newPlans = useMemo(() => (onlyNew ? plans.filter((p) => !p.registered) : plans), [plans, onlyNew]);
  const skippedPlans = plans.length - newPlans.length;

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
    message.info("按「只处理新增」没有要判的项了：已去掉勾，连已经归好类的一起判");
    smart.start(smartTargets(false));
  };

  const handleOrganizeClick = async () => {
    setOrganizing(true);
    try {
      const items = await scanDesktop();
      if (items.length === 0) {
        message.info("桌面未发现可整理的程序");
        return;
      }
      // 全都登记过了也照常开弹窗：不然那个「只处理新增」的勾没地方取消，旧卡片再也重做不了
      setPlans(items);
      setRegisterOnly(true);
      setOrganizeOpen(true);
    } catch (e) {
      message.error(String(e));
    } finally {
      setOrganizing(false);
    }
  };

  const handleOrganize = async () => {
    if (!newPlans.length) {
      message.info("这次没有要处理的项：去掉「只处理新增」就能连已经登记的一起做");
      return;
    }
    const payload: OrganizePlan[] = newPlans.map((p) => ({
      path: p.path,
      category: p.category,
      iconPath: p.iconPath,
      skipMove: registerOnly,
    }));
    const logs = await organizeDesktop(payload);
    message.success(
      registerOnly
        ? `已添加 ${newPlans.length} 个程序到启动器`
        : `已移动 ${logs.length} 个文件到「桌面整理」并加入启动器`,
    );
    setOrganizeOpen(false);
    await loadAll();
  };

  const handleRevert = () => {
    ModalConfirm({
      title: "恢复桌面整理？",
      content:
        "把之前移动进「桌面\\桌面整理」的文件搬回桌面原位置，启动器卡片的路径会跟着改回来；只登记没移动过的程序不受影响。",
      onOk: async () => {
        setReverting(true);
        try {
          const n = await revertOrganize();
          message.success(n > 0 ? `已恢复 ${n} 个文件` : "没有可恢复的整理记录");
          await loadAll();
        } catch (e) {
          message.error(String(e));
        } finally {
          setReverting(false);
        }
      },
    });
  };

  const removeCategory = (node: CategoryNode) => setCatDel(node);

  // 批量删除：把「会动到什么」先算出来摆给用户看，再让他选要不要连规则一起删
  const batchRuleCount = (nodes: CategoryNode[]) => nodes.reduce((sum, n) => sum + rulesOf(n.name).length, 0);
  const batchItemCount = (nodes: CategoryNode[]) => nodes.reduce((sum, n) => sum + n.count, 0);
  const batchReal = (nodes: CategoryNode[]) => nodes.filter((n) => n.id > 0);
  /** 只有规则、没有分类行的名字：删它的办法就是删掉那条规则，本来也没有分类可删 */
  const batchGhosts = (nodes: CategoryNode[]) => nodes.filter((n) => n.id === 0);

  const doDeleteBatch = async (alsoRules: boolean) => {
    const nodes = catBatch;
    if (!nodes?.length) return;
    const real = batchReal(nodes);
    const ghosts = batchGhosts(nodes);
    try {
      let moved = 0;
      let droppedRules = 0;
      if (real.length) [moved, droppedRules] = await deleteCategories(real.map((n) => n.id), alsoRules);
      for (const g of ghosts) {
        for (const r of rulesOf(g.name)) await deleteRule(r.id);
      }
      setCatBatch(null);
      const parts = [
        real.length ? `已删除 ${real.length} 个分类，${moved} 个程序退回未分类` : "",
        alsoRules && droppedRules ? `顺带删了 ${droppedRules} 条分类规则` : "",
        ghosts.length ? `去掉了 ${ghosts.length} 个只在规则里的名字（连同那条规则）` : "",
      ].filter(Boolean);
      message.success(parts.join("，"));
      await loadAll();
    } catch (e) {
      message.error(String(e));
    }
  };

  const doDeleteCat = async (mode: "cat" | "both") => {
    const node = catDel;
    if (!node) return;
    if (node.id > 0) {
      const moved = await deleteCategory(node.id, mode === "both");
      message.success(moved ? `已删除，${moved} 项退回未分类` : "已删除");
    } else {
      /** 树上这条只来自规则，删掉规则就等于删掉它 */
      for (const r of rulesOf(node.name)) await deleteRule(r.id);
      message.success("已删除对应的分类规则");
    }
    setCatDel(null);
    await loadAll();
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
      content: REMOVE_HINT,
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
        <h2 className="toolbar-title toolbar-title-pane">桌面整理</h2>
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
            <Button
              type="primary"
              icon={<Wand2 size={16} />}
              loading={organizing}
              onClick={handleOrganizeClick}
            >
              一键整理
            </Button>
            <Button icon={<Plus size={16} />} onClick={() => setAddOpen(true)}>
              添加程序
            </Button>
            <Button icon={<Undo2 size={16} />} loading={reverting} onClick={handleRevert}>
              恢复整理
            </Button>
            <Button
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
            onDeleteMany={(nodes) => setCatBatch(nodes)}
            onDelete={removeCategory}
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
        title={`删除选中的 ${catBatch?.length ?? 0} 项？`}
        open={!!catBatch?.length}
        onCancel={() => setCatBatch(null)}
        width={460}
        footer={[
          <Button key="cancel" onClick={() => setCatBatch(null)}>
            取消
          </Button>,
          ...(catBatch && batchReal(catBatch).length
            ? [
                <Button key="cats" onClick={() => void doDeleteBatch(false)}>
                  只删分类
                </Button>,
              ]
            : []),
          <Button key="both" type="primary" danger onClick={() => void doDeleteBatch(true)}>
            {catBatch && !batchReal(catBatch).length
              ? `删除规则（${batchRuleCount(catBatch)} 条）`
              : `连规则一起删${catBatch && batchRuleCount(catBatch) > 0 ? `（${batchRuleCount(catBatch)} 条）` : ""}`}
          </Button>,
        ]}
      >
        <div className="label" style={{ marginBottom: 6 }}>
          {catBatch?.map((n) => n.name).join("、")}
        </div>
        <div className="label">
          分类下的 {catBatch ? batchItemCount(catBatch) : 0} 个程序只退回未分类，不会被删除。
          {catBatch && catBatch.length && batchGhosts(catBatch).length
            ? `其中 ${batchGhosts(catBatch).map((n) => n.name).join("、")} 只是分类规则里的名字（没有分类行），删它就是删那条规则。`
            : ""}
          {catBatch && batchRuleCount(catBatch) > 0
            ? `另有 ${batchRuleCount(catBatch)} 条分类规则指向这些名字：只删分类的话这些规则会留下，下次整理时又会把同名分类建回来。`
            : batchGhosts(catBatch ?? []).length
              ? ""
              : "这几个分类没有对应的分类规则。"}
        </div>
      </Modal>

      <Modal
        title="整理桌面"
        open={organizeOpen}
        onCancel={() => setOrganizeOpen(false)}
        onOk={handleOrganize}
        okText={registerOnly ? "添加到启动器" : "移动并添加"}
        width={760}
      >
        <Table
          dataSource={newPlans}
          rowKey={(r) => r.path}
          size="small"
          scroll={{ y: 360 }}
          columns={[
            { title: "文件名", dataIndex: "name", ellipsis: true, render: (v: string) => stripExt(v) },
            {
              title: "当前位置",
              dataIndex: "path",
              ellipsis: true,
              render: (v: string) => <span className="label">{v}</span>,
            },
            {
              title: "目标分类",
              dataIndex: "category",
              width: 150,
              render: (_: unknown, record: DesktopItem) => (
                <Select
                  size="small"
                  style={{ width: 130 }}
                  value={record.category}
                  options={categoryNames.map((n) => ({ label: n, value: n }))}
                  onChange={(v) =>
                    setPlans((prev) =>
                      prev.map((p) => (p.path === record.path ? { ...p, category: v as string } : p)),
                    )
                  }
                />
              ),
            },
          ]}
        />
        <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 4 }}>
          <Checkbox
            checked={onlyNew}
            onChange={setOnlyNew}
            label="只处理新增：已经在启动器里的项这次不动"
          />
          {onlyNew && skippedPlans > 0 && (
            <span className="label">
              已跳过 {skippedPlans} 个登记过的，下面这 {newPlans.length} 个是新增的。要连旧的一起重抽图标或重新归类，就去掉上面这个勾。
            </span>
          )}
          <Checkbox
            checked={registerOnly}
            onChange={setRegisterOnly}
            label="只把程序登记进启动器，桌面上的文件保持原地不动"
          />
          {!registerOnly && (
            <span className="label">
              取消勾选会把文件移进 桌面\桌面整理\分类 目录，同名会加 (1)(2)；移完可用「恢复整理」搬回去。
            </span>
          )}
        </div>
      </Modal>

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

      <Modal
        title={`删除分类「${catDel?.name ?? ""}」？`}
        open={!!catDel}
        onCancel={() => setCatDel(null)}
        width={440}
        footer={
          <Space>
            <Button onClick={() => setCatDel(null)}>取消</Button>
            {catDel && catDel.id > 0 && ruleCount > 0 && (
              <Button onClick={() => void doDeleteCat("cat")}>只删分类</Button>
            )}
            <Button type="primary" danger onClick={() => void doDeleteCat(ruleCount > 0 ? "both" : "cat")}>
              {catDel && catDel.id === 0 ? "删除规则" : ruleCount > 0 ? "删分类和规则" : "删除"}
            </Button>
          </Space>
        }
      >
        <div className="label" style={{ lineHeight: 1.7 }}>
          {catDel && catDel.id === 0
            ? "这个分类只存在于分类规则里（分类本身已经不在了）。删掉它，就是把那条整理规则一起去掉。"
            : ruleCount > 0
              ? `有 ${ruleCount} 条分类规则指向它。只删分类的话，规则还在，这个名字会继续留在树上；一起删掉规则，以后整理就不会再归到这里。`
              : catDel && catDel.count
                ? `${catDel.count} 个程序会退回未分类，程序本身不会被删除。`
                : "这个分类下没有内容，也没有规则指向它，删掉不影响别的。"}
        </div>
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
