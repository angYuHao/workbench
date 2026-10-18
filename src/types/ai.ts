/** 送进智能分类的一个条目：target 是本机路径，只有程序类才有，且不会发给模型 */
export interface SmartCategorizeItem {
  id: number;
  name: string;
  target?: string | null;
  url?: string | null;
}

export interface SmartCategorizeResult {
  id: number;
  name: string;
  /** 模型没答这条时为空串，等人工选 */
  suggestedCategory: string;
  reason: string;
}

export interface ApplyCategorizeUpdate {
  id: number;
  categoryName: string;
  /** 窗口里那条判断的理由，应用时交给后端写进本机记忆当依据 */
  reason?: string;
}

/** 智能分类的本机记忆：一个软件判过一次就记住，一条就一句话 */
export interface AiMemory {
  name: string;
  category: string;
  evidence: string;
  /** search = 搜索摘要里关键词命中；model = 带着资料问模型判的 */
  source: string;
  at: number;
}

/** 自定义大模型（OpenAI 兼容接口）。后端只回有没有存密钥，不回传密钥本身 */
export interface AiModel {
  id: number;
  name: string;
  baseUrl: string;
  model: string;
  hasKey: boolean;
  sort: number;
  /** 上次点「获取模型」拉回来并存在本机的列表，没获取过就是空 */
  remoteModels: string[];
  /** 那次获取的 unix 秒 */
  remoteAt: number | null;
}

export interface AiModelInput {
  id: number;
  name: string;
  baseUrl: string;
  model: string;
  /** 留空表示编辑时保持原密钥不变 */
  apiKey?: string | null;
  sort?: number;
}

/** 设置里看的 AI 日志：只有最近几十行，但总行数和路径一起给，界面能说清「还有更多」 */
export interface AiLogView {
  path: string;
  lines: string[];
  total: number;
}

/** 后端探测接口连通性后回的一句话，status 为 0 表示压根没连上 */
export interface ProbeResult {
  ok: boolean;
  status: number;
  message: string;
}
