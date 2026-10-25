import { create } from "zustand";
import { getSettings, setSetting } from "../api/settings";
import { applySeed, resolveMode, watchSystemMode } from "../styles/apply";
import { DEFAULT_STYLE_ID, PRESETS, mergeSeed, presetById, seedOf, type Mode, type StyleSeed } from "../styles/seeds";

/** 用户自己存下来的风格：存整份种子，不存「预设 + 增量」，免得预设本身改了把它们带歪 */
export interface UserStyle {
  id: string;
  name: string;
  seed: StyleSeed;
}

const K_STYLE = "appearance.style_id";
const K_OVERRIDES = "appearance.overrides";
const K_USER = "appearance.user_styles";

interface AppearanceStore {
  styleId: string;
  overrides: Partial<StyleSeed>;
  userStyles: UserStyle[];
  loaded: boolean;
  load: () => Promise<void>;
  /** 当前状态展开成完整种子：面板预览和写库都用它 */
  seed: () => StyleSeed;
  setStyle: (id: string) => Promise<void>;
  patch: (part: Partial<StyleSeed>) => void;
  setMode: (mode: Mode) => Promise<void>;
  reset: () => Promise<void>;
  saveStyle: (name: string) => Promise<void>;
  removeStyle: (id: string) => Promise<void>;
  exportJson: () => string;
  importJson: (raw: string) => Promise<void>;
  watchSystem: () => () => void;
}

function isPresetOrUser(id: string, users: UserStyle[]): boolean {
  return !!presetById(id) || users.some((u) => u.id === id);
}

function parse<T>(raw: string | undefined, fallback: T): T {
  if (!raw) return fallback;
  try {
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

/** 系统设置里没有外观这几行时，用旧的 theme 那一行顶一下：只存过深浅的老用户不该一觉醒来发现风格变了 */
function migrate(settings: Record<string, string>): { styleId: string; overrides: Partial<StyleSeed> } {
  const styleId = settings[K_STYLE] || DEFAULT_STYLE_ID;
  const overrides = parse<Partial<StyleSeed>>(settings[K_OVERRIDES], {});
  if (!settings[K_STYLE] && !settings[K_OVERRIDES] && settings.theme === "dark") {
    return { styleId, overrides: { ...overrides, mode: "dark" } };
  }
  return { styleId, overrides };
}

export const useAppearanceStore = create<AppearanceStore>((set, get) => {
  /** 落库这件事不必追着取色器的每一次拖动做，界面先变、稍后写 */
  let saveTimer: number | undefined;
  const persist = () => {
    const { styleId, overrides, userStyles } = get();
    window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => {
      void Promise.all([
        setSetting(K_STYLE, styleId),
        setSetting(K_OVERRIDES, JSON.stringify(overrides)),
        setSetting(K_USER, JSON.stringify(userStyles)),
        // 旧键继续维护：它是「当前是深色还是浅色」的唯一历史口径，按解析后的种子写才不会在用户风格下报错值
        setSetting("theme", resolveMode(currentSeed().mode)),
      ]);
    }, 300);
  };

  /** 当前状态展开成完整种子：预设走「档位默认 + 覆盖」，用户存的风格直接带整份种子 */
  const currentSeed = (): StyleSeed => {
    const { styleId, overrides, userStyles } = get();
    const user = userStyles.find((u) => u.id === styleId);
    if (user) return mergeSeed(user.seed, overrides);
    const preset = presetById(styleId) ?? presetById(DEFAULT_STYLE_ID) ?? PRESETS[0];
    return seedOf(preset, overrides.mode ?? "light", overrides);
  };

  const render = () => {
    applySeed(currentSeed(), get().styleId);
  };

  return {
    styleId: DEFAULT_STYLE_ID,
    overrides: {},
    userStyles: [],
    loaded: false,
    load: async () => {
      const settings = await getSettings();
      const userStyles = parse<UserStyle[]>(settings[K_USER], []);
      const { styleId, overrides } = migrate(settings);
      set({
        styleId: isPresetOrUser(styleId, userStyles) ? styleId : DEFAULT_STYLE_ID,
        overrides,
        userStyles,
        loaded: true,
      });
      render();
    },
    seed: currentSeed,
    setStyle: async (id) => {
      // 换预设就把上一套手动改动清掉，否则「自定义过的紫」会挂在新的蓝色风格上说不清
      set({ styleId: id, overrides: { mode: get().overrides.mode } });
      render();
      persist();
    },
    patch: (part) => {
      set({ overrides: { ...get().overrides, ...part } });
      render();
      persist();
    },
    setMode: async (mode) => {
      set({ overrides: { ...get().overrides, mode } });
      render();
      persist();
    },
    /** 「回到预设默认」只清档位覆盖，mode 那一轴单独留着 —— 不然深色用户点一下重置就被扔回浅色 */
    reset: async () => {
      set({ overrides: { mode: get().overrides.mode } });
      render();
      persist();
    },
    saveStyle: async (name) => {
      const trimmed = name.trim();
      if (!trimmed) return;
      const id = `u-${Math.random().toString(36).slice(2, 10)}`;
      const style: UserStyle = { id, name: trimmed, seed: get().seed() };
      set({ styleId: id, userStyles: [...get().userStyles, style] });
      persist();
    },
    removeStyle: async (id) => {
      const rest = get().userStyles.filter((u) => u.id !== id);
      set({ userStyles: rest, styleId: get().styleId === id ? DEFAULT_STYLE_ID : get().styleId });
      render();
      persist();
    },
    exportJson: () => JSON.stringify({ version: 1, styleId: get().styleId, overrides: get().overrides, userStyles: get().userStyles }, null, 2),
    importJson: async (raw) => {
      const data = parse<{ styleId?: string; overrides?: Partial<StyleSeed>; userStyles?: UserStyle[] }>(raw, {});
      const userStyles = Array.isArray(data.userStyles) ? data.userStyles : [];
      const styleId = isPresetOrUser(data.styleId ?? "", userStyles) ? data.styleId! : DEFAULT_STYLE_ID;
      set({ styleId, overrides: data.overrides ?? {}, userStyles });
      render();
      persist();
    },
    watchSystem: () =>
      watchSystemMode(() => {
        if (get().overrides.mode === "auto") render();
      }),
  };
});
