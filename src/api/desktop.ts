import { invoke } from "@tauri-apps/api/core";
import type { AppEntry, DesktopItem, OrganizeLog, OrganizePlan, SyncReport } from "../types";

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

/** 扫一遍开始菜单，把没收录过的程序收进卡片（过滤名单命中的不收） */
export function syncInstalledApps() {
  return invoke<SyncReport>("sync_installed_apps");
}

/** 同一次扫描但不写库，用来看过滤名单挡掉了谁 */
export function previewInstalledApps() {
  return invoke<SyncReport>("preview_installed_apps");
}

export function getAppFilters() {
  return invoke<string[]>("get_app_filters");
}

/** 回 false 表示名单里已经有这条了（不分大小写） */
export function addAppFilter(pattern: string) {
  return invoke<boolean>("add_app_filter", { pattern });
}

export function removeAppFilter(pattern: string) {
  return invoke<boolean>("remove_app_filter", { pattern });
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