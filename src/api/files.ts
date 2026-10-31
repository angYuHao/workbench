import { invoke } from "@tauri-apps/api/core";
import { write } from "./notify";
import type { DriveInfo, FileNode, FileRecord, KnownFolder, ReadTextResult } from "../types";

/** 侧栏「此电脑」：这台机器现在有哪些盘 */
export function getDrives() {
  return invoke<DriveInfo[]>("get_drives");
}

/** 侧栏「常用文件夹」：桌面 / 下载 / 文档 / 图片 / 音乐 / 视频 */
export function getKnownFolders() {
  return invoke<KnownFolder[]>("get_known_folders");
}

/** 列一个目录。整台机器都能列，`includeHidden` 决定隐藏和系统项摆不摆 */
export function listFiles(root: string, includeHidden: boolean) {
  return invoke<FileNode[]>("list_files", { root, includeHidden });
}

/** 让系统给这些路径各画一张图标（磁盘上的真图标，带文件名也认）。
    不存在的路径不会返回；没画过的后端会当场画一张落进缓存 */
export function getShellIcons(paths: string[], size = 32) {
  return invoke<Record<string, string>>("get_shell_icons", { paths, size });
}

/** 预览要读的字节走 asset 协议，得先让后端把这一个文件所在的目录授出去。
    不改库，所以是 invoke 而不是 write */
export function authorizePreview(path: string) {
  return invoke<void>("authorize_preview", { path });
}

/** 监视哪几个目录由界面说（当前几个标签停着的目录）。同样不改库 */
export function watchFolders(folders: string[]) {
  return invoke<void>("watch_folders", { folders });
}

export function getFile(path: string) {
  return invoke<FileRecord>("get_file", { path });
}

/** 别名和说明是唯一进库的东西：磁盘上的文件名一个字都不动 */
export function saveFileMeta(path: string, alias: string | null, description: string | null) {
  return write<FileRecord>("save_file_meta", "files", { path, alias, description });
}

export function readTextFile(path: string) {
  return invoke<ReadTextResult>("read_text_file", { path });
}

/** 文本预览读开头的原始字节。不走 asset 协议：它会把整份文件读进内存，
    一个几 GB 的日志就可能让 wry 装响应时溢出 u32 把进程带走。
    后端多读一个字节，这里靠它判断后面还有没有 */
export async function readFileHead(
  path: string,
  limit: number,
): Promise<{ bytes: Uint8Array; truncated: boolean }> {
  const buf = await invoke<ArrayBuffer>("read_file_head", { path, limit });
  const all = new Uint8Array(buf);
  return all.byteLength > limit
    ? { bytes: all.subarray(0, limit), truncated: true }
    : { bytes: all, truncated: false };
}

/** 新建文件夹。同名自己让位，返回真正建出来的那个路径 */
export function createFolder(parent: string, name: string) {
  return invoke<string>("create_folder", { parent, name });
}

/** 改名。库里那条别名/说明跟着搬，所以算改了库 */
export function renamePath(path: string, newName: string) {
  return write<string>("rename_path", "files", { path, newName });
}

/** 删除。toTrash=false 是彻底删，连标注一起摘 */
export function deletePaths(paths: string[], toTrash: boolean) {
  return write<number>("delete_paths", "files", { paths, toTrash });
}

/** 拷进某个目录。只动磁盘，不碰库。回的是新落地的路径，撤销粘贴照着删 */
export function copyPaths(paths: string[], dest: string) {
  return invoke<string[]>("copy_paths", { paths, dest });
}

/** 搬进某个目录。标注跟着走，算改了库。回的是新落地的路径，撤销时再搬回去 */
export function movePaths(paths: string[], dest: string) {
  return write<string[]>("move_paths", "files", { paths, dest });
}

/** 系统剪贴板上的那串文件。`cut` 为真就是剪切：粘完原件要挪走 */
export interface ClipboardFiles {
  paths: string[];
  cut: boolean;
}

/** 把这几项放进 Windows 剪贴板（CF_HDROP + Preferred DropEffect）：
    在资源管理器里粘是复制/移动，在 Outlook 和浏览器里粘是附件。
    `none` 是「已经搬过、别再粘」，资源管理器自己粘完之后也这么标 */
export function writeClipboardFiles(paths: string[], mode: "copy" | "move" | "none") {
  return invoke<void>("clipboard_write", { paths, mode });
}

/** 读系统剪贴板里的文件清单；拷的是文字或图片就回 null */
export function readClipboardFiles() {
  return invoke<ClipboardFiles | null>("clipboard_read");
}
