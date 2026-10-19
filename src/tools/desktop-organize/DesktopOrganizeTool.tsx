import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, ListFilter, Undo2, Wand2 } from "lucide-react";
import { organizeDesktop, revertOrganize, scanDesktop } from "../../api/desktop";
import { useSettingsStore } from "../../stores/settings";
import { Button, Checkbox, Empty, Modal, ModalConfirm, Select, Space, Spin, Table, message, pressable } from "../../ui";
import IconWithFallback from "../../shared/IconWithFallback";
import { stripExt } from "../../utils/fileKind";
import RulesSection from "./RulesSection";
import RuleEditModal from "./RuleEditModal";
import type { DesktopItem, DesktopRule, OrganizePlan } from "../../types";
import type { ToolProps } from "../registry";

const blankRule: DesktopRule = {
  id: 0,
  category: "",
  keywords: null,
  extensions: null,
  enabled: true,
  sort: 0,
};

/**
 * 桌面整理工具：外壳跟原来那一屏一致 —— 工具栏 + 左边分类栏 + 右边内容区。
 * 打开就扫一遍桌面，扫出来的东西直接铺在内容区，左边按分类规则给它们分组计数、点一下筛。
 * 三个动作按钮都在工具栏上：一键整理（重新扫 + 确认执行）、恢复整理、分类规则。
 */
export default function DesktopOrganizeTool({ onBack }: ToolProps) {
  const { rules, loadRules } = useSettingsStore();
  const [items, setItems] = useState<DesktopItem[]>([]);
  const [scanning, setScanning] = useState(true);
  const [activeCat, setActiveCat] = useState<string | null>(null);
  const [organizeOpen, setOrganizeOpen] = useState(false);
  const [rulesOpen, setRulesOpen] = useState(false);
  const [edit, setEdit] = useState<DesktopRule | null>(null);
  const [onlyNew, setOnlyNew] = useState(true);
  const [registerOnly, setRegisterOnly] = useState(true);
  const [reverting, setReverting] = useState(false);

  /** 扫桌面要解析 .lnk、抽图标，是这一屏唯一慢的一步；回扫到的东西，出错回 null */
  const scan = async (): Promise<DesktopItem[] | null> => {
    setScanning(true);
    try {
      const fresh = await scanDesktop();
      setItems(fresh);
      return fresh;
    } catch (e) {
      message.error(String(e));
      return null;
    } finally {
      setScanning(false);
    }
  };

  useEffect(() => {
    loadRules().catch((e) => message.error(String(e)));
    void scan();
  }, []);

  const categoryOptions = useMemo(
    () =>
      [...new Set(rules.map((r) => r.category.trim()).filter(Boolean))].map((n) => ({ label: n, value: n })),
    [rules]
  );
  /** 左边那一列：规则里的分类在前，扫出来却不在规则里的（比如「其他」）跟在后面 */
  const catRows = useMemo(() => {
    const counts = new Map<string, number>();
    for (const it of items) counts.set(it.category, (counts.get(it.category) ?? 0) + 1);
    const names = [...categoryOptions.map((o) => o.value), ...[...counts.keys()].filter((c) => !categoryOptions.some((o) => o.value === c))];
    return names.map((name) => ({ name, count: counts.get(name) ?? 0 }));
  }, [items, categoryOptions]);

  const visible = useMemo(
    () => (activeCat === null ? items : items.filter((i) => i.category === activeCat)),
    [items, activeCat]
  );
  const newCount = items.filter((i) => !i.registered).length;

  const plans = useMemo(() => (onlyNew ? items.filter((i) => !i.registered) : items), [items, onlyNew]);
  const skippedPlans = items.length - plans.length;

  const handleOrganizeClick = async () => {
    // 屏上的东西可能已经放了一会儿了，执行前重新扫一遍，别按旧账搬
    const fresh = await scan();
    if (!fresh) return;
    if (fresh.length === 0) {
      message.info("桌面没有可整理的东西");
      return;
    }
    // 全都登记过了也照常开弹窗：不然那个「只处理新增」的勾没地方取消，旧卡片再也重做不了
    setRegisterOnly(true);
    setOrganizeOpen(true);
  };

  const handleOrganize = async () => {
    if (!plans.length) {
      message.info("这次没有要处理的项：去掉「只处理新增」就能连已经登记的一起做");
      return;
    }
    const payload: OrganizePlan[] = plans.map((p) => ({
      path: p.path,
      category: p.category,
      iconPath: p.iconPath,
      skipMove: registerOnly,
    }));
    const logs = await organizeDesktop(payload);
    message.success(
      registerOnly
        ? `已添加 ${plans.length} 个程序到「应用」页`
        : `已移动 ${logs.length} 个文件到「桌面整理」并加入「应用」页`,
    );
    setOrganizeOpen(false);
    await scan();
  };

  const handleRevert = () => {
    ModalConfirm({
      title: "恢复桌面整理？",
      content:
        "把之前移动进「桌面\\桌面整理」的文件搬回桌面原位置，「应用」页卡片的路径会跟着改回来；只登记没移动过的程序不受影响。",
      onOk: async () => {
        setReverting(true);
        try {
          const n = await revertOrganize();
          message.success(n > 0 ? `已恢复 ${n} 个文件` : "没有可恢复的整理记录");
          await scan();
        } catch (e) {
          message.error(String(e));
        } finally {
          setReverting(false);
        }
      },
    });
  };

  const setCategory = (record: DesktopItem, category: string) =>
    setItems((prev) => prev.map((i) => (i.path === record.path ? { ...i, category } : i)));

  return (
    <div className="page">
      <div className="toolbar">
        <Button type="text" icon={<ArrowLeft size={16} />} onClick={onBack}>
          工具
        </Button>
        <h2 className="toolbar-title toolbar-title-pane">桌面整理</h2>
        <div className="toolbar-spacer" />
        <Space>
          <Button type="primary" icon={<Wand2 size={16} />} loading={scanning} onClick={() => void handleOrganizeClick()}>
            一键整理
          </Button>
          <Button icon={<Undo2 size={16} />} loading={reverting} onClick={handleRevert}>
            恢复整理
          </Button>
          <Button icon={<ListFilter size={16} />} onClick={() => setRulesOpen(true)}>
            分类规则
          </Button>
        </Space>
      </div>

      <div className="panes">
        <div className="side-pane">
          <div className="stack">
            <CatRow label="全部" count={items.length} active={activeCat === null} onClick={() => setActiveCat(null)} />
            {catRows.map((c) => (
              <CatRow
                key={c.name}
                label={c.name}
                count={c.count}
                active={activeCat === c.name}
                onClick={() => setActiveCat(c.name)}
              />
            ))}
            {!rules.length && (
              <div className="label" style={{ marginTop: 8 }}>
                还没有分类规则，扫出来的东西都会落到「其他」。
              </div>
            )}
          </div>
        </div>

        <div className="content-pane">
          <div className="org-summary">
            <span className="chip">桌面 {items.length} 项</span>
            <span className="chip">新增 {newCount} 项</span>
            <span className="label">
              下面是按分类规则算出来的去向；要改某一项归到哪儿，点「一键整理」在确认窗口里逐条改。
            </span>
          </div>
          {scanning ? (
            <div className="org-loading">
              <Spin />
              <span className="label">正在扫描桌面（含子文件夹）…</span>
            </div>
          ) : visible.length === 0 ? (
            <Empty description={items.length ? `「${activeCat}」下暂时没有任何东西` : "桌面是空的，没什么可整理"} />
          ) : (
            <div className="org-grid">
              {visible.map((it) => (
                <div
                  key={it.path}
                  className={it.registered ? "card org-tile done" : "card org-tile"}
                  title={it.path}
                >
                  <IconWithFallback name={it.name} iconPath={it.iconPath} size={44} />
                  <div className="org-name ellipsis">{stripExt(it.name)}</div>
                  <div className="org-cat ellipsis">
                    {it.category}
                    {it.registered ? " · 已登记" : ""}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      <Modal
        title="整理桌面"
        open={organizeOpen}
        onCancel={() => setOrganizeOpen(false)}
        onOk={handleOrganize}
        okText={registerOnly ? "添加到应用页" : "移动并添加"}
        width={760}
      >
        <Table
          dataSource={plans}
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
                  // 这一项不在规则里（规则都没命中）时，框里说的是它默认会去哪儿，不是一个选项
                  placeholder={`默认「${record.category}」`}
                  options={categoryOptions}
                  onChange={(v) => setCategory(record, v as string)}
                />
              ),
            },
          ]}
        />
        <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 4 }}>
          <span className="label">
            下拉里只有分类规则的那些分类名。某一项一条规则都没撞上时，框里显示的是它默认的去向，不是选项；
            想让这种文件也归到某个分类，去「分类规则」里加一条。
          </span>
          <Checkbox
            checked={onlyNew}
            onChange={setOnlyNew}
            label="只处理新增：已经在「应用」页里的项这次不动"
          />
          {onlyNew && skippedPlans > 0 && (
            <span className="label">
              已跳过 {skippedPlans} 个登记过的，下面这 {plans.length} 个是新增的。要连旧的一起重抽图标或重新归类，就去掉上面这个勾。
            </span>
          )}
          <Checkbox
            checked={registerOnly}
            onChange={setRegisterOnly}
            label="只把程序登记进「应用」页，桌面上的文件保持原地不动"
          />
          {!registerOnly && (
            <span className="label">
              取消勾选会把文件移进 桌面\桌面整理\分类 目录，同名会加 (1)(2)；移完可用「恢复整理」搬回去。
            </span>
          )}
        </div>
      </Modal>

      <Modal title="分类规则" open={rulesOpen} onCancel={() => setRulesOpen(false)} footer={null} width={760}>
        <RulesSection onAdd={() => setEdit(blankRule)} onEdit={(rule) => setEdit(rule)} />
      </Modal>

      {edit && <RuleEditModal rule={edit} onClose={() => setEdit(null)} />}
    </div>
  );
}

/** 分类栏里的一行：长得和「应用」页那列一模一样，点一下筛右边 */
function CatRow({
  label,
  count,
  active,
  onClick,
}: {
  label: string;
  count: number;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <div
      className={active ? "nav-item active" : "nav-item"}
      aria-label={label}
      onClick={onClick}
      {...pressable(onClick)}
    >
      <span className="ellipsis">{label}</span>
      <span className="label">{count}</span>
    </div>
  );
}
