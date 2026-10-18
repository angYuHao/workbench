import { Sparkles } from "lucide-react";
import type { ReactNode } from "react";
import { Button, Input, Modal, Select, Table } from "../ui";
import { stripExt } from "../utils/fileKind";
import type { SmartCategorize } from "./useSmartCategorize";
import type { SmartCategorizeResult } from "../types";

interface Props {
  smart: SmartCategorize;
  nameTitle: string;
  stripExtension?: boolean;
  /** 空状态里那句"怎么判的"的说明：桌面和书签走的链路不一样，各说各的 */
  hint?: string;
  /** 调用方自己的开关（比如桌面的「只判新增」）；书签那边没有就不传 */
  extra?: ReactNode;
}

/** 一行一个输入框，候选项共用同一个 datalist：既能点选也能现场写新分类 */
const DATALIST = "smart-category-options";

export default function SmartCategorizeModal({ smart, nameTitle, stripExtension, hint, extra }: Props) {
  const { open, results, models, modelId, categoryNames, loading, close, generate, setModel, suggest, apply } = smart;

  // 模型给出的新名字也要能选回来改，所以候选是「已有分类 ∪ 当前建议」
  const candidates = Array.from(
    new Set([...categoryNames, ...results.map((r) => r.suggestedCategory.trim()).filter(Boolean)])
  );
  const pending = results.filter((r) => r.suggestedCategory.trim());
  const brandNew = new Set(pending.map((r) => r.suggestedCategory.trim()).filter((c) => !categoryNames.includes(c)));

  return (
    <Modal
      title="AI 分类建议"
      open={open}
      onCancel={close}
      onOk={apply}
      okText="应用分类"
      width={720}
    >
      <div className="smart-bar">
        <span className="smart-bar-label">用哪个模型</span>
        <Select
          size="small"
          style={{ width: 240 }}
          value={modelId}
          placeholder={models.length ? "选一个模型" : "没有可用模型"}
          options={models.map((m) => ({ label: `${m.name} · ${m.model}`, value: m.id }))}
          onChange={(v) => setModel(Number(v))}
        />
        <Button size="small" type="primary" icon={<Sparkles size={14} />} loading={loading} onClick={generate}>
          {results.length ? "重新建议" : "让 AI 建议"}
        </Button>
      </div>

      {extra && <div style={{ marginBottom: 10 }}>{extra}</div>}

      {!models.length && (
        <div className="settings-empty">还没有可用的模型，先去「设置 → 模型」添加一个接口地址、模型名和密钥</div>
      )}

      {!!results.length && (
        <>
          <Table
            dataSource={results}
            rowKey={(r) => r.id}
            size="small"
            layout="fixed"
            scroll={{ y: 360 }}
            columns={[
              {
                title: nameTitle,
                dataIndex: "name",
                ellipsis: true,
                render: (v: string) => (stripExtension ? stripExt(v) : v),
              },
              {
                title: "分类",
                dataIndex: "suggestedCategory",
                width: 190,
                render: (_: unknown, r: SmartCategorizeResult) => (
                  <Input
                    size="small"
                    list={DATALIST}
                    placeholder="点选或写新的"
                    value={r.suggestedCategory}
                    onChange={(v) => suggest(r.id, v)}
                  />
                ),
              },
              {
                title: "理由",
                dataIndex: "reason",
                ellipsis: true,
                render: (v: string) => <span className="smart-reason">{v || "—"}</span>,
              },
            ]}
          />
          <datalist id={DATALIST}>
            {candidates.map((name) => (
              <option key={name} value={name} />
            ))}
          </datalist>
          <div className="smart-foot">
            共 {results.length} 条：{pending.length} 条已给分类
            {brandNew.size > 0 && `（其中 ${brandNew.size} 个分类还不存在，应用时会顺手新建）`}
            {results.length - pending.length > 0 && `，${results.length - pending.length} 条没填分类、不会处理`}
          </div>
        </>
      )}

      {!!models.length && !results.length && !loading && (
        <div className="settings-empty">
          {hint ??
            "顺序是：分类规则 → 本机记忆 → 模型 → 联网搜。规则判掉的和记忆里有的都不出本机；发给模型只带名字和网址，不带本地路径；模型判不出来的才上网查，查完判出来的记进本机。"}
        </div>
      )}
    </Modal>
  );
}
