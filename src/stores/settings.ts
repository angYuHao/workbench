import { create } from "zustand";
import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import * as api from "../api/categories";
import {
  clearWakeHotkey,
  getSettings,
  loadWakeHotkey,
  setCloseToTray,
  setSetting,
  setWakeHotkey,
} from "../api/settings";
import type { WakeHotkey } from "../api/settings";
import { useDesktopStore } from "./desktop";
import type { DesktopRule } from "../types";

type Theme = "light" | "dark";

interface SettingsStore {
  rules: DesktopRule[];
  autostart: boolean;
  theme: Theme;
  closeToTray: boolean;
  /** 界面上该显示哪个唤醒热键写法；空串 = 没有（没设过、或明确清掉了） */
  wakeHotkey: string;
  /** 这一会儿真挂着生效的是不是它 */
  wakeHotkeyBound: boolean;
  /** 注册不上的原话；生效时是空串 */
  wakeHotkeyError: string;
  loadRules: () => Promise<void>;
  saveRule: (rule: DesktopRule) => Promise<void>;
  removeRule: (id: number) => Promise<void>;
  loadAutostart: () => Promise<void>;
  toggleAutostart: (on: boolean) => Promise<void>;
  loadLocalPrefs: () => Promise<void>;
  toggleTheme: () => Promise<void>;
  toggleCloseToTray: (on: boolean) => Promise<void>;
  loadWakeHotkey: () => Promise<WakeHotkey>;
  bindWakeHotkey: (key: string) => Promise<WakeHotkey>;
  dropWakeHotkey: () => Promise<WakeHotkey>;
}

export const useSettingsStore = create<SettingsStore>((set, get) => ({
  rules: [],
  autostart: false,
  theme: "light",
  closeToTray: true,
  wakeHotkey: "",
  wakeHotkeyBound: false,
  wakeHotkeyError: "",
  loadRules: async () => set({ rules: await api.getRules() }),
  saveRule: async (rule) => {
    await api.saveRule(rule);
    // 规则里的分类名改了，后端会连分类表和桌面整理目录一起改名；
    // 分类树不重读的话，卡片还顶着旧名字，看着像改名没生效
    await Promise.all([get().loadRules(), useDesktopStore.getState().loadCategories()]);
  },
  removeRule: async (id) => {
    await api.deleteRule(id);
    await get().loadRules();
  },
  loadAutostart: async () => set({ autostart: await isAutostartEnabled() }),
  toggleAutostart: async (on) => {
    if (on) await enableAutostart();
    else await disableAutostart();
    set({ autostart: on });
  },
  /** 存在 settings 表里、影响界面本身的偏好 */
  loadLocalPrefs: async () => {
    const settings = await getSettings();
    set({
      theme: settings.theme === "dark" ? "dark" : "light",
      // 没存过就算开：唤醒热键要靠窗口能隐藏，默认关着的话热键没意义
      closeToTray: settings.close_to_tray !== "0",
    });
  },
  /** 装载存的热键：注册不上不抛错，后端把「挂的是哪个、生效没有」一起带回来 */
  loadWakeHotkey: async () => {
    const r = await loadWakeHotkey();
    set({ wakeHotkey: r.key, wakeHotkeyBound: r.bound, wakeHotkeyError: r.error });
    return r;
  },
  bindWakeHotkey: async (key) => {
    const r = await setWakeHotkey(key);
    set({ wakeHotkey: r.key, wakeHotkeyBound: r.bound, wakeHotkeyError: r.error });
    return r;
  },
  dropWakeHotkey: async () => {
    const r = await clearWakeHotkey();
    set({ wakeHotkey: r.key, wakeHotkeyBound: r.bound, wakeHotkeyError: r.error });
    return r;
  },
  toggleTheme: async () => {
    const nextTheme: Theme = get().theme === "dark" ? "light" : "dark";
    await setSetting("theme", nextTheme);
    set({ theme: nextTheme });
  },
  /** 后端会顺手把托盘图标装上/摘掉，这里只同步界面状态 */
  toggleCloseToTray: async (on) => {
    await setCloseToTray(on);
    set({ closeToTray: on });
  },
}));
