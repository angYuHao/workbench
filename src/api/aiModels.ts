import { invoke } from "@tauri-apps/api/core";
import { write } from "./notify";
import type { AiLogView, AiModel, AiModelInput, ProbeResult } from "../types";

export function getAiModels() {
  return invoke<AiModel[]>("get_ai_models");
}

/** 模型配置显示在设置里，域归 settings；订阅端目前没有要为此重读的 store，
 *  但留着广播，等第二个窗口要显示「用的哪个模型」时就不用回来补 */
export function saveAiModel(model: AiModelInput) {
  return write<AiModel>("save_ai_model", "settings", { model });
}

export function deleteAiModel(id: number) {
  return write<void>("delete_ai_model", "settings", { id });
}

/**
 * 拉接口那边的模型列表。id 传已存配置的编号，密钥留空时沿用库里那份，
 * 所以编辑界面上不必重填密钥也能取。
 */
export function listRemoteAiModels(target: { id?: number | null; baseUrl: string; apiKey?: string | null }) {
  return invoke<string[]>("list_remote_ai_models", {
    id: target.id ?? null,
    baseUrl: target.baseUrl,
    apiKey: target.apiKey ?? null,
  });
}

/**
 * 测密钥能不能真的调用模型：后端发一条 max_tokens=1 的补全请求，会花掉几个 token。
 * id 传已存配置的编号，地址/密钥/模型名留空的项沿用库里那份。
 */
export function probeAiModel(target: {
  id?: number | null;
  baseUrl: string;
  apiKey?: string | null;
  model?: string | null;
}) {
  return invoke<ProbeResult>("probe_ai_model", {
    id: target.id ?? null,
    baseUrl: target.baseUrl,
    apiKey: target.apiKey ?? null,
    model: target.model ?? null,
  });
}

/** 用系统默认编辑器打开本机 AI 调用日志（整个文件），返回它的路径 */
export function openAiLog() {
  return invoke<string>("open_ai_log");
}

/** 设置里内嵌看的那一屏：行数由后端定，前端只管显示拿到的这些 */
export function readAiLog() {
  return invoke<AiLogView>("read_ai_log");
}
