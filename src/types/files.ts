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
  children?: FileNode[];
}

/** 文本预览：过大时只回开头，truncated 告诉界面后面还有 */
export interface ReadTextResult {
  content: string;
  truncated: boolean;
}
