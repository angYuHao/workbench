import { Bot, Brain, Globe, Plus, ScrollText } from "lucide-react";
import { Button } from "../../ui";
import { SettingsLine } from "./shared";
import type { AiLogView, AiMemory, AiModel } from "../../types";

interface Props {
  models: AiModel[];
  /** 正在测连通性的那条，只有它显示 loading */
  probingId: number | null;
  aiLog: AiLogView | null;
  memory: AiMemory[];
  pageCache: number;
  onAddModel: () => void;
  onEditModel: (model: AiModel) => void;
  onProbe: (model: AiModel) => void;
  onDeleteModel: (model: AiModel) => void;
  onShowLog: () => void;
  onOpenWholeLog: () => void;
  onHideLog: () => void;
  onClearMemory: () => void;
  onClearPageCache: () => void;
}

/**
 * 模型配置 + 三样 AI 本机数据（调用日志、本机记忆、网页缓存）。
 * 它们都归在「大模型」这一组下，因为对使用者来说是同一件事：AI 在本机留下了什么。
 */
export default function ModelsSection({
  models,
  probingId,
  aiLog,
  memory,
  pageCache,
  onAddModel,
  onEditModel,
  onProbe,
  onDeleteModel,
  onShowLog,
  onOpenWholeLog,
  onHideLog,
  onClearMemory,
  onClearPageCache,
}: Props) {
  return (
    <>
      <div className="settings-toolbar">
        <span className="label">{models.length} 个模型配置，密钥按当前 Windows 用户加密后存本机</span>
        <Button size="small" type="primary" icon={<Plus size={14} />} onClick={onAddModel}>
          添加模型
        </Button>
      </div>
      <div className="settings-card">
        {models.length === 0 && (
          <div className="settings-empty">还没有模型，点右上「添加模型」填接口地址、模型名和密钥</div>
        )}
        {models.map((m) => (
          <SettingsLine
            key={m.id}
            icon={Bot}
            title={m.name}
            desc={<>{m.baseUrl} · {m.model} · {m.hasKey ? "密钥已保存" : "未填密钥"}</>}
          >
            <span className="settings-row-actions">
              <Button type="link" size="small" loading={probingId === m.id} onClick={() => onProbe(m)}>
                测试
              </Button>
              <Button type="link" size="small" onClick={() => onEditModel(m)}>
                编辑
              </Button>
              <Button type="link" size="small" danger onClick={() => onDeleteModel(m)}>
                删除
              </Button>
            </span>
          </SettingsLine>
        ))}
        <SettingsLine
          icon={ScrollText}
          title="调用日志"
          desc={
            <>
              每次 AI 请求记三行：地址、模型、状态码、耗时，加请求和响应的开头。密钥不写进日志；文件只留最近 600
              行，攒够就自动剪，启动时和每次写之前各查一次
            </>
          }
        >
          <Button icon={<ScrollText size={15} />} onClick={onShowLog}>
            {aiLog ? "刷新" : "最近 100 行"}
          </Button>
        </SettingsLine>
        {aiLog && (
          <>
            <pre className="settings-log">
              {aiLog.lines.length
                ? aiLog.lines.join("\n")
                : "还没有 AI 调用记录：发一次「测试调用」或「让 AI 建议」后再来看。"}
            </pre>
            <div className="settings-log-foot">
              <span className="label">
                {aiLog.path} · 文件共 {aiLog.total} 行，这里只显示最近 {aiLog.lines.length} 行
              </span>
              <span className="settings-row-actions">
                <Button type="link" size="small" onClick={onOpenWholeLog}>
                  编辑器看完整
                </Button>
                <Button type="link" size="small" onClick={onHideLog}>
                  收起
                </Button>
              </span>
            </div>
          </>
        )}
        <SettingsLine
          icon={Brain}
          title={<>本机记忆 · {memory.length} 个软件</>}
          desc={
            memory.length === 0
              ? "判过的软件记在这儿，下次既不问模型也不再联网搜"
              : `${memory
                  .slice(0, 6)
                  .map((m) => `${m.name}→${m.category}`)
                  .join("、")}${memory.length > 6 ? ` 等 ${memory.length} 条` : ""}`
          }
        >
          {memory.length > 0 && (
            <Button type="link" size="small" danger onClick={onClearMemory}>
              清空
            </Button>
          )}
        </SettingsLine>
        <SettingsLine
          icon={Globe}
          title={<>网页缓存 · {pageCache} 条</>}
          desc="网址智能分析抓回来的名称和简介按地址存本机，再点分析不再打网络；网页改版了就在这儿清掉重抓"
        >
          {pageCache > 0 && (
            <Button type="link" size="small" danger onClick={onClearPageCache}>
              清空
            </Button>
          )}
        </SettingsLine>
      </div>
    </>
  );
}
