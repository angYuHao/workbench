import type { ReactNode } from "react";
import type { AiModel } from "../../types";

/** 表单里就地显示的一句结果/错误，ok 决定它是绿是红 */
export interface Hint {
  ok: boolean;
  text: string;
}

export function HintView({ msg }: { msg: Hint | null }) {
  if (!msg) return null;
  return <div className={msg.ok ? "field-hint ok" : "field-hint bad"}>{msg.text}</div>;
}

/** 标签行右侧就地放动作，省掉一排只有按钮的表单项 */
export function LabelRow({ text, action }: { text: string; action: ReactNode }) {
  return (
    <span className="form-label-row">
      <span>{text}</span>
      {action}
    </span>
  );
}

/** 存在本机那份列表是什么时候取的，界面上要说清它有多旧 */
export function fetchedAt(at: number | null) {
  if (!at) return "时间没记下";
  return new Date(at * 1000).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** 「添加模型」的空白行：id 为 0 表示新增，保存和探测时都要转成 null 再发 */
export const blankModel: AiModel = {
  id: 0,
  name: "",
  baseUrl: "",
  model: "",
  hasKey: false,
  sort: 0,
  remoteModels: [],
  remoteAt: null,
};
