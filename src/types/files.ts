/** 已入库的文件/文件夹记录 */
export interface FileRecord {
  id: number;
  path: string;
  name: string;
  alias: string | null;
  description: string | null;
  isDir: boolean;
  managedRoot: string | null;
  categoryId: number | null;
  createdAt: string | null;
}

/** 列目录时的一项：来自磁盘，别名和说明从库里补 */
export interface FileNode {
  name: string;
  path: string;
  isDir: boolean;
  alias: string | null;
  description: string | null;
  /** 字节数。目录没有可靠大小，后端给 null */
  size: number | null;
  /** 最后修改时间，Unix 秒；文件系统没给就 null */
  modifiedAt: number | null;
  /** 隐藏或系统属性：默认不摆出来，工具栏那个开关才显示 */
  hidden: boolean;
  children?: FileNode[];
}

/** 一个盘符：侧栏「此电脑」那一节 */
export interface DriveInfo {
  /** 能直接列的路径，形如 `C:\` */
  path: string;
  letter: string;
  /** 卷标；没命名过的盘是 null，界面拿 kind 兜底 */
  label: string | null;
  kind: string;
  isRemovable: boolean;
  free: number | null;
  total: number | null;
}

/** 已知文件夹（桌面/下载/文档/图片/音乐/视频），name 是后端给的中文名 */
export interface KnownFolder {
  name: string;
  path: string;
}

/** 文本预览：过大时只回开头，truncated 告诉界面后面还有 */
export interface ReadTextResult {
  content: string;
  truncated: boolean;
}
