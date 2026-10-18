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

/** 扫描桌面看到的待整理项（还没入库） */
export interface DesktopItem {
  name: string;
  path: string;
  target: string;
  isLnk: boolean;
  category: string;
  iconPath: string | null;
  /** 启动器里已经有这一条了，「只处理新增」靠它跳过 */
  registered: boolean;
}

/** 确认整理时提交的一条计划 */
export interface OrganizePlan {
  path: string;
  category: string;
  iconPath: string | null;
  /** 只登记进启动器、不搬桌面文件 */
  skipMove?: boolean;
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
