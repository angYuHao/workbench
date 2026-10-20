import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, ListFilter, Undo2, Wand2 } from "lucide-react";
import { organizeDesktop, revertOrganize, scanDesktop } from "../../api/desktop";
import { useSettingsStore } from "../../stores/settings";
import { Button, Empty, Modal, ModalConfirm, Select, Space, Spin, Table, message, pressable } from "../../ui";
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
  /** 左边那一列只列分类规则里的分类名：规则没命中的那些不算一个分类，只在「全部」里看得见，
   *  要它也成一行就去分类规则里加一条 */
  const catRows = useMemo(() => {
    const counts = new Map<string, number>();
    for (const it of items) counts.set(it.category, (counts.get(it.category) ?? 0) + 1);
    return categoryOptions.map((o) => ({ name: o.value, count: counts.get(o.value) ?? 0 }));
  }, [items, categoryOptions]);

  const visible = useMemo(
    () => (activeCat === null ? items : items.filter((i) => i.category === activeCat)),
    [items, activeCat]
  );

  const handleOrganizeClick = async () => {
    // 屏上的东西可能已经放了一会儿了，执行前重新扫一遍，别按旧账搬
    const fresh = await scan();
    if (!fresh) return;
    if (fresh.length === 0) {
      message.info("桌面没有可整理的东西");
      return;
    }
    setOrganizeOpen(true);
  };

  const handleOrganize = async () => {
    if (!items.length) {
      message.info("没有要整理的项");
      return;
    }
    const payload: OrganizePlan[] = items.map((p) => ({ path: p.path, category: p.category }));
    const logs = await organizeDesktop(payload);
    message.success(`已移动 ${logs.length} 个文件到「桌面整理」`);
    setOrganizeOpen(false);
    await scan();
  };

  const handleRevert = () => {
    ModalConfirm({
      title: "恢复桌面整理？",
      content:
        "把之前移动进「桌面\\桌面整理」的文件搬回桌面原位置，被搬空的分类目录一并收掉；整理从来没往「应用」页登记过东西，那边不受影响。",
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
                还没有分类规则：没有规则命中就什么都不整理。点上面的「分类规则」加一条。
              </div>
            )}
          </div>
        </div>

        <div className="content-pane">
          <div className="org-summary">
            <span className="chip">桌面 {items.length} 项</span>
            <span className="label">
              只有撞上分类规则的项才会出现在这里；要改某一项归到哪儿，点「一键整理」在确认窗口里逐条改。
            </span>
          </div>
          {scanning ? (
            <div className="org-loading">
              <Spin />
              <span className="label">正在扫描桌面（含子文件夹）…</span>
            </div>
          ) : visible.length === 0 ? (
            <Empty
              description={
                items.length
                  ? `「${activeCat}」下暂时没有任何东西`
                  : "桌面上没有撞上分类规则的东西"
              }
            />
          ) : (
            <div className="org-grid">
              {visible.map((it) => (
                <div key={it.path} className="card org-tile" title={it.path}>
                  <IconWithFallback name={it.name} iconPath={it.iconPath} size={44} />
                  <div className="org-name ellipsis">{stripExt(it.name)}</div>
                  <div className="org-cat ellipsis">{it.category}</div>
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
        okText={`移动 ${items.length} 项`}
        width={760}
      >
        <Table
          dataSource={items}
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
                  options={categoryOptions}
                  onChange={(v) => setCategory(record, v as string)}
                />
              ),
            },
          ]}
        />
        <div style={{ marginTop: 12, display: "flex", flexDirection: "column", gap: 4 }}>
          <span className="label">
            确定后这些文件会移进 <b>桌面\桌面整理\分类</b> 目录，同名会加 (1)(2)；移完点「恢复整理」原路搬回。
            整理只搬文件，不会把它们登记到「应用」页。
          </span>
          <span className="label">
            只有撞上分类规则的项才会出现在这里：想多整理一类文件，去「分类规则」里加一条。
          </span>
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
