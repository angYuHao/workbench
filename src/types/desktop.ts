export interface AppEntry {
  id: number;
  name: string;
  alias: string | null;
  description: string | null;
  target: string;
  args: string | null;
  iconPath: string | null;
  categoryId: number | null;
  source: string;
  createdAt: string | null;
  /** 双击启动过几次：列表按它从多到少排，分类树按底下的总数排 */
  useCount: number;
}

/** 扫描桌面看到的一项（还没整理） */
export interface DesktopItem {
  name: string;
  path: string;
  target: string;
  isLnk: boolean;
  category: string;
  iconPath: string | null;
}

/** 确认整理时提交的一条计划：整理只搬文件，不往「应用」里登记任何东西 */
export interface OrganizePlan {
  path: string;
  category: string;
}

/** 整理流水，「恢复整理」靠它把文件搬回原位 */
export interface OrganizeLog {
  id: number;
  fileName: string | null;
  fromPath: string | null;
  toPath: string | null;
  organizedAt: string | null;
}

export interface DesktopRule {
  id: number;
  category: string;
  keywords: string | null;
  extensions: string | null;
  enabled: boolean;
  sort: number;
}

/** 一次开始菜单同步（或试算）的账单 */
export interface SyncReport {
  /** 开始菜单里扫出多少个可启动的快捷方式 */
  scanned: number;
  /** 本次收进来的；试算时是「会收进来的」 */
  inserted: number;
  /** 命中过滤名单没收的 */
  filtered: number;
  /** 库里已经有的 */
  already: number;
  /** 库里现在一共多少张卡片 */
  total: number;
  /** 被挡掉的名字，最多 12 个 */
  samples: string[];
}
