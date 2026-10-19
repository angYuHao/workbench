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
import { useDesktopStore } from "./desktop";
import type { DesktopRule } from "../types";

type Theme = "light" | "dark";

interface SettingsStore {
  rules: DesktopRule[];
  autostart: boolean;
  theme: Theme;
  closeToTray: boolean;
  /** 生效中的唤醒热键；空串 = 没有（没设过、或被占用后清掉了） */
  wakeHotkey: string;
  loadRules: () => Promise<void>;
  saveRule: (rule: DesktopRule) => Promise<void>;
  removeRule: (id: number) => Promise<void>;
  loadAutostart: () => Promise<void>;
  toggleAutostart: (on: boolean) => Promise<void>;
  loadLocalPrefs: () => Promise<void>;
  toggleTheme: () => Promise<void>;
  toggleCloseToTray: (on: boolean) => Promise<void>;
  loadWakeHotkey: () => Promise<void>;
  bindWakeHotkey: (key: string) => Promise<void>;
  dropWakeHotkey: () => Promise<void>;
}

export const useSettingsStore = create<SettingsStore>((set, get) => ({
  rules: [],
  autostart: false,
  theme: "light",
  closeToTray: true,
  wakeHotkey: "",
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
  /** 装载设置里存的热键（没存过后端用默认键）。失败会抛，让调用方决定怎么提示 */
  loadWakeHotkey: async () => set({ wakeHotkey: await loadWakeHotkey() }),
  bindWakeHotkey: async (key) => set({ wakeHotkey: await setWakeHotkey(key) }),
  dropWakeHotkey: async () => {
    await clearWakeHotkey();
    set({ wakeHotkey: "" });
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
