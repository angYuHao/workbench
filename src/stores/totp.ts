import { create } from "zustand";
import * as api from "../api/totp";
import type { TotpAccount, TotpAccountInput } from "../types";

interface TotpStore {
  accounts: TotpAccount[];
  loading: boolean;
  load: () => Promise<void>;
  save: (input: TotpAccountInput) => Promise<void>;
  remove: (id: number) => Promise<void>;
}

export const useTotpStore = create<TotpStore>((set, get) => ({
  accounts: [],
  loading: false,
  load: async () => {
    set({ loading: true });
    try {
      set({ accounts: await api.getTotpAccounts() });
    } finally {
      set({ loading: false });
    }
  },
  save: async (input) => {
    await api.saveTotpAccount(input);
    await get().load();
  },
  remove: async (id) => {
    await api.deleteTotpAccount(id);
    await get().load();
  },
}));
