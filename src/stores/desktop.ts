import { create } from "zustand";
import * as api from "../api/desktop";
import * as categoriesApi from "../api/categories";
import type { AppEntry, Category } from "../types";

interface DesktopStore {
  apps: AppEntry[];
  categories: Category[];
  loadApps: () => Promise<void>;
  loadCategories: () => Promise<void>;
  removeApp: (id: number) => Promise<void>;
}

export const useDesktopStore = create<DesktopStore>((set, get) => ({
  apps: [],
  categories: [],
  loadApps: async () => set({ apps: await api.getApps() }),
  loadCategories: async () => set({ categories: await categoriesApi.getCategories("app") }),
  removeApp: async (id) => {
    await api.deleteApp(id);
    await get().loadApps();
  },
}));
