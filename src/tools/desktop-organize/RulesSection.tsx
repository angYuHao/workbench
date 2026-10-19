import { Plus } from "lucide-react";
import { Button, Switch, Table } from "../../ui";
import { useSettingsStore } from "../../stores/settings";
import type { DesktopRule } from "../../types";

interface Props {
  /** 弹窗开关状态在 shell 里，这里只负责把「要点谁」报上去 */
  onAdd: () => void;
  onEdit: (rule: DesktopRule) => void;
}

export default function RulesSection({ onAdd, onEdit }: Props) {
  const { rules, saveRule, removeRule } = useSettingsStore();

  return (
    <>
      <div className="settings-toolbar">
        <span className="label">{rules.length} 条规则，按排序从小到大命中</span>
        <Button size="small" type="primary" icon={<Plus size={14} />} onClick={onAdd}>
          新增规则
        </Button>
      </div>
      <Table
        dataSource={rules}
        rowKey={(r) => r.id}
        size="small"
        layout="fixed"
        columns={[
          { title: "分类名", dataIndex: "category", ellipsis: true },
          { title: "关键词", dataIndex: "keywords", ellipsis: true, render: (v: string | null) => v ?? "—" },
          { title: "扩展名", dataIndex: "extensions", ellipsis: true, render: (v: string | null) => v ?? "—" },
          { title: "排序", dataIndex: "sort", width: 60 },
          {
            title: "启用",
            dataIndex: "enabled",
            width: 60,
            render: (_: unknown, rule: DesktopRule) => (
              // store 的 saveRule 存完会重读列表，开关拨完表格就是新的
              <Switch size="small" checked={rule.enabled} onChange={(on) => saveRule({ ...rule, enabled: on })} />
            ),
          },
          {
            title: "操作",
            width: 96,
            render: (_: unknown, rule: DesktopRule) => (
              <span className="settings-row-actions">
                <Button type="link" size="small" onClick={() => onEdit(rule)}>
                  编辑
                </Button>
                <Button type="link" size="small" danger onClick={() => removeRule(rule.id)}>
                  删除
                </Button>
              </span>
            ),
          },
        ]}
      />
    </>
  );
}
