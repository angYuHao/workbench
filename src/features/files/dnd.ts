import { parentOf, folderName, isUnder, samePath } from "../../utils/paths";
import { useFilesStore } from "../../stores/files";
import { message } from "../../ui";

/** 同一块盘才默认算「搬」；UNC 按 \\服务器\共享 这一段比 */
export function volumeOf(path: string): string {
  const unc = /^(\\\\[^\\/]+[\\/][^\\/]+)/.exec(path);
  if (unc) return unc[1].replace(/\//g, "\\").toLowerCase();
  const drive = /^([a-zA-Z]):/.exec(path);
  return drive ? drive[1].toLowerCase() : path.toLowerCase();
}

/** 拖拽落地是搬还是拷：Ctrl 强制复制、Shift 强制移动，别的跟资源管理器一样同卷搬、跨卷拷 */
export function dropMode(
  sourceDir: string,
  dest: string,
  e: { ctrlKey?: boolean; metaKey?: boolean; shiftKey?: boolean },
): "copy" | "move" {
  if (e.ctrlKey || e.metaKey) return "copy";
  if (e.shiftKey) return "move";
  return volumeOf(sourceDir) === volumeOf(dest) ? "move" : "copy";
}

/** 能不能落到 dest 上：拖的就是 dest 或它底下某一层（包括把文件夹拖进自己）、
    以及整串本来就待在 dest 里，都不接 —— 光标当场就摆成禁止，别等后端报错 */
export function canDropInto(paths: string[], dest: string): boolean {
  if (paths.some((p) => isUnder(dest, p))) return false;
  return paths.some((p) => !samePath(parentOf(p) ?? "", dest));
}

/** 落点统一交到这里：搬/拷、提示、清拖拽状态都只写一遍 */
export async function dropInto(
  dest: string,
  e: { ctrlKey: boolean; metaKey: boolean; shiftKey: boolean },
): Promise<void> {
  const store = useFilesStore.getState();
  const drag = store.dragFiles;
  if (!drag) return;
  store.setDragFiles(null);
  if (!canDropInto(drag.paths, dest)) return;
  const mode = dropMode(drag.sourceDir, dest, e);
  try {
    const res = await store.dropFilesInto(drag.paths, dest, mode);
    if (res) {
      message.success(`已${res.mode === "copy" ? "复制" : "移动"} ${res.count} 项到「${folderName(dest)}」`);
    }
  } catch (err) {
    message.error(String(err));
  }
}
