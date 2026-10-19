import { invoke } from "@tauri-apps/api/core";

export function getSettings() {
  return invoke<Record<string, string>>("get_settings");
}

export function setSetting(key: string, value: string) {
  return invoke<void>("set_setting", { key, value });
}

/** 后端同时负责装上/摘掉托盘图标，所以不走通用的 set_setting */
export function setCloseToTray(on: boolean) {
  return invoke<void>("set_close_to_tray", { on });
}

/** 唤醒热键：注册和解绑都在后端做（同步命令会死锁，所以这三个都是 async 命令） */
export function loadWakeHotkey() {
  return invoke<string>("load_wake_hotkey");
}

export function setWakeHotkey(key: string) {
  return invoke<string>("set_wake_hotkey", { key });
}

export function clearWakeHotkey() {
  return invoke<void>("clear_wake_hotkey");
}

export function exportDb(dest: string) {
  return invoke<void>("export_db", { dest });
}

export function restoreDb(src: string) {
  return invoke<void>("restore_db", { src });
}