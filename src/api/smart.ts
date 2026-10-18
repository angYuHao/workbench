import { invoke } from "@tauri-apps/api/core";
import type { AiMemory, ApplyCategorizeUpdate, SmartCategorizeItem, SmartCategorizeResult } from "../types";

/** 建议由大模型给：itemType 决定参考哪套分类，modelId 为空时后端用列表里的第一个模型 */
export function smartCategorize(items: SmartCategorizeItem[], itemType: string, modelId: number | null) {
  return invoke<SmartCategorizeResult[]>("smart_categorize", { items, itemType, modelId });
}

export function applySmartCategorize(itemType: string, updates: ApplyCategorizeUpdate[]) {
  return invoke<number>("apply_smart_categorize", { itemType, updates });
}

/** 本机记忆：判过一次就记住的软件，下次既不问模型也不联网 */
export function getAiMemory() {
  return invoke<AiMemory[]>("get_ai_memory");
}

/** 回被清掉的条数 */
export function clearAiMemory() {
  return invoke<number>("clear_ai_memory");
}
