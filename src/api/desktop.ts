import { invoke } from "@tauri-apps/api/core";
import type { AppEntry, DesktopItem, OrganizeLog, OrganizePlan } from "../types";

export function scanDesktop() {
  return invoke<DesktopItem[]>("scan_desktop");
}

export function organizeDesktop(items: OrganizePlan[]) {
  return invoke<OrganizeLog[]>("organize_desktop", { items });
}

/** logId 省略就是「把这次之前所有没恢复的整理全部撤销」，返回真正搬回原位的文件数 */
export function revertOrganize(logId?: number) {
  return invoke<number>("revert_organize", { logId: logId ?? null });
}

/** 只给 id：后端按 id 取路径、记一次使用，再启动 */
export function launchApp(id: number) {
  return invoke<void>("launch_app", { id });
}

export function getApps() {
  return invoke<AppEntry[]>("get_apps");
}

export function saveApp(app: AppEntry) {
  return invoke<AppEntry>("save_app", { app });
}

export function deleteApp(id: number) {
  return invoke<void>("delete_app", { id });
}