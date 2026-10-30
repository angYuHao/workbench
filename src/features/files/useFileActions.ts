import { ModalConfirm, message } from "../../ui";
import { useFilesStore } from "../../stores/files";
import { folderName } from "../../utils/paths";

/**
 * 这一页的写动作。右键菜单、键盘、按钮三条路都汇到这里，免得同一件事写三遍还各漏一个确认。
 * 出错一律弹到界面上：这些动作真会动磁盘上的文件，静悄悄失败最坏。
 */
async function run<T>(fn: () => Promise<T>): Promise<T | null> {
  try {
    return await fn();
  } catch (e) {
    message.error(String(e));
    return null;
  }
}

export default function useFileActions() {
  /** 拷的是系统剪贴板，不是本页自己记的那串：资源管理器、Outlook、浏览器都认 */
  const copy = async () => {
    const s = useFilesStore.getState();
    const paths = s.selectedPaths();
    if (!paths.length) return;
    const done = await run(() => s.copySelection());
    if (done !== null) message.success(`已复制 ${paths.length} 项，可以粘到资源管理器、Outlook 或浏览器里`);
  };

  const cut = async () => {
    const s = useFilesStore.getState();
    const paths = s.selectedPaths();
    if (!paths.length) return;
    const done = await run(() => s.cutSelection());
    if (done !== null) message.success(`已剪切 ${paths.length} 项，粘贴后原件会挪走`);
  };

  const paste = async () => {
    const res = await run(() => useFilesStore.getState().pasteIntoActive());
    if (res === null) return; // 出错了，已经弹过
    if (!res) {
      message.info("剪贴板里没有可以粘贴的文件");
      return;
    }
    message.success(`已${res.mode === "move" ? "移动" : "复制"} ${res.count} 项到这里`);
  };

  /** toTrash=false 就是 Shift+Delete 那种直接抹掉 */
  const remove = async (toTrash: boolean) => {
    const s = useFilesStore.getState();
    const paths = s.selectedPaths();
    if (!paths.length) return;
    const label = paths.length === 1 ? `「${folderName(paths[0])}」` : `选中的 ${paths.length} 项`;
    const ok = await ModalConfirm({
      title: toTrash ? "删除到回收站？" : "彻底删除？",
      content: toTrash
        ? `把${label}送进回收站，随时能还原。别名和说明也留着，从回收站还原回来那句话还在。`
        : `${label}直接抹掉，不进回收站。别名和说明一起删。`,
      okText: toTrash ? "删除" : "彻底删除",
    });
    if (!ok) return;
    const n = await run(() => s.deleteSelected(toTrash));
    if (typeof n === "number") message.success(`已删除 ${n} 项`);
  };

  const refresh = async () => {
    await run(() => useFilesStore.getState().refreshActive());
  };

  return { copy, cut, paste, remove, refresh };
}
