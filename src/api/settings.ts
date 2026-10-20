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

/** 热键的真实样子：key 是库里/默认的写法（空串 = 不要热键），bound 是这会儿真挂上了没 */
export interface WakeHotkey {
  key: string;
  bound: boolean;
  /** 注册不上的原话；bound 为真时是空串 */
  error: string;
}

/** 唤醒热键：注册和解绑都在后端做（同步命令会死锁，所以这三个都是 async 命令）。
 *  三个都不抛「键位被占」这种业务失败，改成把 bound/error 带回来，界面自己说清楚 */
export function loadWakeHotkey() {
  return invoke<WakeHotkey>("load_wake_hotkey");
}

export function setWakeHotkey(key: string) {
  return invoke<WakeHotkey>("set_wake_hotkey", { key });
}

export function clearWakeHotkey() {
  return invoke<WakeHotkey>("clear_wake_hotkey");
}

export function exportDb(dest: string) {
  return invoke<void>("export_db", { dest });
}

export function restoreDb(src: string) {
  return invoke<void>("restore_db", { src });
}