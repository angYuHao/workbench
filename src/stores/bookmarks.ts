import { create } from "zustand";
import * as api from "../api/bookmarks";
import * as categoriesApi from "../api/categories";
import type { Bookmark, Category, ImportResult } from "../types";

interface BookmarksStore {
  bookmarks: Bookmark[];
  categories: Category[];
  loading: boolean;
  load: () => Promise<void>;
  import: (browser: string) => Promise<ImportResult>;
  save: (bookmark: Bookmark) => Promise<void>;
  remove: (id: number) => Promise<void>;
}

export const useBookmarksStore = create<BookmarksStore>((set, get) => ({
  bookmarks: [],
  categories: [],
  loading: false,
  load: async () => {
    set({ loading: true });
    try {
      const [bookmarks, categories] = await Promise.all([
        api.getBookmarks(),
        categoriesApi.getCategories("bookmark"),
      ]);
      set({ bookmarks, categories });
    } finally {
      set({ loading: false });
    }
  },
  import: async (browser) => {
    const result = await api.importBookmarks(browser);
    await get().load();
    return result;
  },
  save: async (bookmark) => {
    await api.saveBookmark(bookmark);
    await get().load();
  },
  remove: async (id) => {
    await api.deleteBookmark(id);
    await get().load();
  },
}));
