import { invoke } from "@tauri-apps/api/core";
import { write } from "./notify";

export function getSettings() {
  return invoke<Record<string, string>>("get_settings");
}

export function setSetting(key: string, value: string) {
  return write<void>("set_setting", "settings", { key, value });
}

/** 后端同时负责装上/摘掉托盘图标，所以不走通用的 set_setting */
export function setCloseToTray(on: boolean) {
  return write<void>("set_close_to_tray", "settings", { on });
}

/** 抽屉同理：后端当场改无边框/置顶，并停掉或恢复顶边轮询 */
export function setEdgeDrawer(on: boolean) {
  return write<void>("set_edge_drawer", "settings", { on });
}

/** 导出不改库，只是把文件拷出去 */
export function exportDb(dest: string) {
  return invoke<void>("export_db", { dest });
}

/** 整库被换掉了：这不是哪个域脏了的问题，是所有域都得重读，所以单独一个 restore 域 */
export function restoreDb(src: string) {
  return write<void>("restore_db", "restore", { src });
}
