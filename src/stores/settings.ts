import { create } from "zustand";
import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import * as api from "../api/categories";
import { getSettings, setCloseToTray, setEdgeDrawer } from "../api/settings";
import { useDesktopStore } from "./desktop";
import type { DesktopRule } from "../types";

interface SettingsStore {
  rules: DesktopRule[];
  autostart: boolean;
  closeToTray: boolean;
  /** 平时藏着，鼠标顶到屏幕上沿才出来 */
  edgeDrawer: boolean;
  loadRules: () => Promise<void>;
  saveRule: (rule: DesktopRule) => Promise<void>;
  removeRule: (id: number) => Promise<void>;
  loadAutostart: () => Promise<void>;
  toggleAutostart: (on: boolean) => Promise<void>;
  loadLocalPrefs: () => Promise<void>;
  toggleCloseToTray: (on: boolean) => Promise<void>;
  toggleEdgeDrawer: (on: boolean) => Promise<void>;
}

export const useSettingsStore = create<SettingsStore>((set, get) => ({
  rules: [],
  autostart: false,
  closeToTray: true,
  edgeDrawer: true,
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
  /** 存在 settings 表里、影响界面本身的偏好（外观那几行归 stores/appearance.ts 管） */
  loadLocalPrefs: async () => {
    const settings = await getSettings();
    set({
      // 没存过就算开：关掉的话点窗口关闭键就是真退出，托盘这个入口也就没有了
      closeToTray: settings.close_to_tray !== "0",
      // 抽屉同理；后端第一次跑还会把窗口显示一次，别在这里跟着猜
      edgeDrawer: settings.edge_drawer !== "0",
    });
  },
  /** 后端会顺手把托盘图标装上/摘掉，这里只同步界面状态 */
  toggleCloseToTray: async (on) => {
    await setCloseToTray(on);
    set({ closeToTray: on });
  },
  /** 抽屉开关：后端当场收放那条只读信息条，主窗口不受影响 */
  toggleEdgeDrawer: async (on) => {
    await setEdgeDrawer(on);
    set({ edgeDrawer: on });
  },
}));
