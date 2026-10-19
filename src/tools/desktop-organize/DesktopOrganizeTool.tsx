import { useEffect, useMemo, useState, type ReactNode } from "react";
import { ArrowLeft, ListFilter, Undo2, Wand2 } from "lucide-react";
import { organizeDesktop, revertOrganize, scanDesktop } from "../../api/desktop";
import { useSettingsStore } from "../../stores/settings";
import { Button, Checkbox, Modal, ModalConfirm, Select, Table, message } from "../../ui";
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
 * 桌面整理工具：只管桌面上那一堆文件。三个动作——扫一遍归位、搬回原处、改规则。
 * 卡片和分类在「应用」页，那儿只读开始菜单，不碰这里的规则。
 */
export default function DesktopOrganizeTool({ onBack }: ToolProps) {
  const { rules, loadRules } = useSettingsStore();
  const [plans, setPlans] = useState<DesktopItem[]>([]);
  const [organizeOpen, setOrganizeOpen] = useState(false);
  const [rulesOpen, setRulesOpen] = useState(false);
  const [edit, setEdit] = useState<DesktopRule | null>(null);
  const [onlyNew, setOnlyNew] = useState(true);
  const [registerOnly, setRegisterOnly] = useState(true);
  const [organizing, setOrganizing] = useState(false);
  const [reverting, setReverting] = useState(false);

  useEffect(() => {
    loadRules().catch((e) => message.error(String(e)));
  }, []);

  const categoryOptions = useMemo(
    () =>
      [...new Set(rules.map((r) => r.category.trim()).filter(Boolean))].map((n) => ({ label: n, value: n })),
    [rules]
  );
  const newPlans = useMemo(() => (onlyNew ? plans.filter((p) => !p.registered) : plans), [plans, onlyNew]);
  const skippedPlans = plans.length - newPlans.length;

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
        ? `已添加 ${newPlans.length} 个程序到「应用」页`
        : `已移动 ${logs.length} 个文件到「桌面整理」并加入「应用」页`,
    );
    setOrganizeOpen(false);
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
        } catch (e) {
          message.error(String(e));
        } finally {
          setReverting(false);
        }
      },
    });
  };

  return (
    <div className="page">
      <div className="toolbar">
        <Button type="text" icon={<ArrowLeft size={16} />} onClick={onBack}>
          工具
        </Button>
        <h2 className="toolbar-title">桌面整理</h2>
      </div>

      <div className="content-pane">
        <div className="tool-grid">
          <Action
            lead
            icon={<Wand2 size={22} strokeWidth={1.5} />}
            title="一键整理"
            sub="扫桌面，按规则归位"
            desc="扫一遍桌面（含子文件夹），按分类规则把文件归进 桌面\桌面整理\分类。也可以只登记程序、文件原地不动。"
            chip={rules.length ? `${rules.length} 条规则在用` : "无规则：全部落「其他」"}
            action="开始整理"
            onClick={handleOrganizeClick}
            loading={organizing}
          />
          <Action
            icon={<Undo2 size={22} strokeWidth={1.5} />}
            title="恢复整理"
            sub="把搬走的原样放回去"
            desc="把之前移动进「桌面整理」的文件搬回桌面原位置，「应用」页卡片的路径跟着改回来。"
            action="恢复"
            onClick={handleRevert}
            loading={reverting}
          />
          <Action
            icon={<ListFilter size={22} strokeWidth={1.5} />}
            title="分类规则"
            sub="什么文件进哪个分类"
            desc="关键词或扩展名命中就归某个分类。改分类名会连桌面上的那个文件夹一起改名。"
            chip={`${rules.length} 条规则`}
            action="查看规则"
            onClick={() => setRulesOpen(true)}
          />
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
                  // 这一项不在规则里（规则都没命中）时，框里说的是它默认会去哪儿，不是一个选项
                  placeholder={`默认「${record.category}」`}
                  options={categoryOptions}
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
              已跳过 {skippedPlans} 个登记过的，下面这 {newPlans.length} 个是新增的。要连旧的一起重抽图标或重新归类，就去掉上面这个勾。
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

      <Modal
        title="分类规则"
        open={rulesOpen}
        onCancel={() => setRulesOpen(false)}
        footer={null}
        width={760}
      >
        <RulesSection onAdd={() => setEdit(blankRule)} onEdit={(rule) => setEdit(rule)} />
      </Modal>

      {edit && <RuleEditModal rule={edit} onClose={() => setEdit(null)} />}
    </div>
  );
}

/**
 * 一张动作卡：实心图标砖 + 名字/一句角色 + 说明 + 底栏（状态 chip 与按钮同一条线）。
 * 三张卡等高，说明文字长短不会把按钮顶歪；lead 那张才是主行动，另外两张只上浅底。
 */
function Action({
  icon,
  title,
  sub,
  desc,
  chip,
  action,
  onClick,
  loading,
  lead,
}: {
  icon: ReactNode;
  title: string;
  sub: string;
  desc: string;
  chip?: string;
  action: string;
  onClick: () => void;
  loading?: boolean;
  lead?: boolean;
}) {
  return (
    <div className="card act-card">
      <div className="act-head">
        <span className={lead ? "act-icon lead" : "act-icon"}>{icon}</span>
        <div className="stack" style={{ minWidth: 0 }}>
          <div className="act-title ellipsis">{title}</div>
          <div className="act-sub ellipsis">{sub}</div>
        </div>
      </div>
      <p className="act-desc">{desc}</p>
      <div className="act-foot">
        {chip ? <span className="act-chip">{chip}</span> : <span />}
        <Button type={lead ? "primary" : "default"} onClick={onClick} loading={loading}>
          {action}
        </Button>
      </div>
    </div>
  );
}
