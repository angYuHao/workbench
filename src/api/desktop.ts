import { invoke } from "@tauri-apps/api/core";
import { write } from "./notify";
import type { AppEntry, DesktopItem, OrganizeLog, OrganizePlan, SyncReport } from "../types";

export function scanDesktop() {
  return invoke<DesktopItem[]>("scan_desktop");
}

/** 整理动的是磁盘上的文件，落库的是那几行日志；文件本身的变化由 filewatcher 那条说 */
export function organizeDesktop(items: OrganizePlan[]) {
  return write<OrganizeLog[]>("organize_desktop", "desktop", { items });
}

/** logId 省略就是「把这次之前所有没恢复的整理全部撤销」，返回真正搬回原位的文件数 */
export function revertOrganize(logId?: number) {
  return write<number>("revert_organize", "desktop", { logId: logId ?? null });
}

/** 只给 id：后端按 id 取路径、记一次使用，再启动。广播走 usage，理由同 openUrl */
export function launchApp(id: number) {
  return write<void>("launch_app", "usage", { id });
}

/** 读一遍本机装了什么（注册表卸载项 + 开始菜单快捷方式），把没收录过的加成卡片 */
export function syncInstalledApps() {
  return write<SyncReport>("sync_installed_apps", "apps");
}

/** 同一次扫描但不写库，用来看过滤名单挡掉了谁 */
export function previewInstalledApps() {
  return invoke<SyncReport>("preview_installed_apps");
}

export function getAppFilters() {
  return invoke<string[]>("get_app_filters");
}

/** 回 false 表示名单里已经有这条了（不分大小写）。名单存在 settings 那张 KV 里 */
export function addAppFilter(pattern: string) {
  return write<boolean>("add_app_filter", "settings", { pattern });
}

export function removeAppFilter(pattern: string) {
  return write<boolean>("remove_app_filter", "settings", { pattern });
}

export function getApps() {
  return invoke<AppEntry[]>("get_apps");
}

export function saveApp(app: AppEntry) {
  return write<AppEntry>("save_app", "apps", { app });
}

export function deleteApp(id: number) {
  return write<void>("delete_app", "apps", { id });
}
