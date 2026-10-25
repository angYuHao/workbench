/** 风格的种子：一个主题预设只描述这几样东西，其余四十多个界面变量全部由 derive.ts 算出来。
 *  加一个新风格 = 加一条种子，不是抄一份 CSS。 */

export type Mode = "light" | "dark" | "auto";
export type RadiusStep = "sharp" | "soft" | "round" | "pill";
export type DensityStep = "compact" | "comfortable" | "spacious";
export type ShadowStep = "none" | "soft" | "strong";
export type RailStep = "dark" | "tinted" | "follow";
export type TextureStep = "none" | "grid" | "noise" | "wash";
export type TintStep = 0 | 1 | 2;
/** 文字浓度：淡 = 整条文字灰阶往面上提一点，浓 = 往纯黑/纯白压一点 */
export type InkStep = "soft" | "standard" | "strong";

export interface StyleSeed {
  mode: Mode;
  accent: string;
  bg: string;
  card: string;
  text: string;
  radius: RadiusStep;
  density: DensityStep;
  shadow: ShadowStep;
  rail: RailStep;
  texture: TextureStep;
  tint: TintStep;
  ink: InkStep;
}

export interface SeedColors {
  accent: string;
  bg: string;
  card: string;
  text: string;
}

export interface StylePreset {
  id: string;
  name: string;
  /** 一句话说明这个风格想表达什么，面板上直接显示 */
  note: string;
  light: SeedColors;
  dark: SeedColors;
  radius?: RadiusStep;
  density?: DensityStep;
  shadow?: ShadowStep;
  rail?: RailStep;
  texture?: TextureStep;
  tint?: TintStep;
  ink?: InkStep;
}

export const RADIUS: Record<RadiusStep, number> = { sharp: 2, soft: 6, round: 12, pill: 18 };
export const RADIUS_STEPS: RadiusStep[] = ["sharp", "soft", "round", "pill"];

/** 密度改的是外壳的间距、栏内边距、控件高度与字号。comfortable 就是这套界面一直用的数，
 *  pad 是侧栏那一档内边距，padLg 是内容区和工具栏的横向内边距 —— 两者必须同档联动，
 *  否则 layout.css 里「工具栏标题跨过侧栏那一列」的对齐就歪了。 */
export const DENSITY: Record<DensityStep, { gap: number; pad: number; padLg: number; ctrl: number; font: number }> = {
  compact: { gap: 6, pad: 12, padLg: 15, ctrl: 30, font: 13 },
  comfortable: { gap: 8, pad: 16, padLg: 20, ctrl: 34, font: 14 },
  spacious: { gap: 12, pad: 20, padLg: 26, ctrl: 38, font: 15 },
};
export const DENSITY_STEPS: DensityStep[] = ["compact", "comfortable", "spacious"];

export const SHADOW: Record<ShadowStep, number> = { none: 0, soft: 1, strong: 2 };
export const SHADOW_STEPS: ShadowStep[] = ["none", "soft", "strong"];

export const RAIL_STEPS: RailStep[] = ["dark", "tinted", "follow"];
export const TEXTURE_STEPS: TextureStep[] = ["none", "grid", "noise", "wash"];
export const TINT_STEPS: TintStep[] = [0, 1, 2];

export const INK_STEPS: InkStep[] = ["soft", "standard", "strong"];

export const MODE_STEPS: Mode[] = ["light", "dark", "auto"];

export const PRESETS: StylePreset[] = [
  {
    id: "classic",
    name: "默认",
    note: "改版前的样子，留着做对照",
    light: { accent: "#4f6bed", bg: "#f5f7fa", card: "#ffffff", text: "#1f2937" },
    dark: { accent: "#4f6bed", bg: "#111827", card: "#1f2937", text: "#e5e7eb" },
    radius: "round",
    rail: "dark",
    tint: 0,
    texture: "none",
    shadow: "soft",
  },
  {
    id: "fresh",
    name: "清亮",
    note: "同一条蓝提亮，重色导轨换成轻染",
    light: { accent: "#5b7cff", bg: "#fafbff", card: "#ffffff", text: "#2b3352" },
    dark: { accent: "#8ba3ff", bg: "#141826", card: "#1e2434", text: "#e3e8f5" },
    radius: "pill",
    rail: "tinted",
    tint: 2,
    texture: "wash",
    shadow: "soft",
  },
  {
    id: "grape",
    name: "汽水紫",
    note: "紫得发亮，卡片浮起来",
    light: { accent: "#7c5cff", bg: "#faf9ff", card: "#ffffff", text: "#2f2a55" },
    dark: { accent: "#a78bfa", bg: "#14121f", card: "#1d1a2b", text: "#e6e2fb" },
    radius: "pill",
    rail: "tinted",
    tint: 2,
    texture: "wash",
    shadow: "soft",
  },
  {
    id: "mist",
    name: "晨雾蓝",
    note: "偏天空的冷调，导轨跟着主题走",
    light: { accent: "#2f9bff", bg: "#f6fbff", card: "#ffffff", text: "#22384d" },
    dark: { accent: "#6bc6ff", bg: "#0d1620", card: "#14202c", text: "#dbe9f5" },
    radius: "round",
    rail: "follow",
    tint: 2,
    texture: "wash",
    shadow: "soft",
  },
  {
    id: "mint",
    name: "薄荷",
    note: "青绿，长时间看不累",
    light: { accent: "#10c9a6", bg: "#f4fdfa", card: "#ffffff", text: "#1b3d38" },
    dark: { accent: "#4ee6c0", bg: "#0a1614", card: "#10201d", text: "#d5f2ea" },
    radius: "round",
    rail: "follow",
    tint: 1,
    texture: "none",
    shadow: "soft",
  },
  {
    id: "sakura",
    name: "樱粉",
    note: "粉底深字，对比靠字色不靠压黑",
    light: { accent: "#f5568f", bg: "#fff6f9", card: "#ffffff", text: "#4a2436" },
    dark: { accent: "#ff8ab5", bg: "#1a1017", card: "#241521", text: "#fbe2ec" },
    radius: "pill",
    rail: "tinted",
    tint: 2,
    texture: "wash",
    shadow: "soft",
  },
  {
    id: "soda",
    name: "橘子水",
    note: "高饱和暖调，最跳的一档",
    light: { accent: "#ff7a35", bg: "#fff9f4", card: "#ffffff", text: "#45291b" },
    dark: { accent: "#ffa06a", bg: "#1a120d", card: "#251a13", text: "#ffe6d5" },
    radius: "round",
    rail: "tinted",
    tint: 1,
    texture: "none",
    shadow: "soft",
  },
  {
    id: "paper",
    name: "暖纸",
    note: "纸感暖底，明度抬高但色相收着",
    light: { accent: "#c2610a", bg: "#faf6ef", card: "#fffdf8", text: "#33291d" },
    dark: { accent: "#e0a45e", bg: "#17140f", card: "#231e17", text: "#ece5d6" },
    radius: "round",
    rail: "tinted",
    tint: 1,
    texture: "noise",
    shadow: "soft",
  },
  {
    id: "graphite",
    name: "石墨",
    note: "无彩，靠层级不靠颜色",
    light: { accent: "#3f4b5f", bg: "#f3f5f7", card: "#ffffff", text: "#161c26" },
    dark: { accent: "#93a4bd", bg: "#0f1419", card: "#171d24", text: "#dde5ef" },
    radius: "soft",
    rail: "follow",
    tint: 0,
    texture: "none",
    shadow: "soft",
  },
  {
    id: "contrast",
    name: "高对比",
    note: "无障碍退路，轻盈是选择，看得见不是",
    light: { accent: "#0031d6", bg: "#ffffff", card: "#ffffff", text: "#000000" },
    dark: { accent: "#ffd400", bg: "#000000", card: "#0a0a0a", text: "#ffffff" },
    radius: "sharp",
    rail: "follow",
    tint: 0,
    texture: "none",
    shadow: "none",
  },
];

export const DEFAULT_STYLE_ID = "fresh";

/** 面板上的档位中文名分控件给，因为 soft 既可能是圆角也可能是阴影档 */
export const MODE_LABELS: Record<Mode, string> = { light: "浅色", dark: "深色", auto: "跟随系统" };
export const RADIUS_LABELS: Record<RadiusStep, string> = { sharp: "直角", soft: "小圆角", round: "圆角", pill: "大圆角" };
export const DENSITY_LABELS: Record<DensityStep, string> = { compact: "紧凑", comfortable: "舒适", spacious: "宽松" };
export const SHADOW_LABELS: Record<ShadowStep, string> = { none: "无", soft: "淡", strong: "重" };
export const RAIL_LABELS: Record<RailStep, string> = { dark: "深色导轨", tinted: "主色轻染", follow: "跟随主题" };
export const TEXTURE_LABELS: Record<TextureStep, string> = { none: "无", grid: "细网格", noise: "颗粒", wash: "主色渐染" };
export const TINT_LABELS: Record<TintStep, string> = { 0: "中性灰阶", 1: "轻染", 2: "明显" };
export const INK_LABELS: Record<InkStep, string> = { soft: "淡", standard: "标准", strong: "浓" };

export function presetById(id: string): StylePreset | undefined {
  return PRESETS.find((p) => p.id === id);
}

/** 把覆盖项并到种子上。undefined 表示「这一项改回预设的」，必须剔掉再合，
 *  否则 {...base, ...{accent: undefined}} 会把种子值直接抹成 undefined。 */
export function mergeSeed(base: StyleSeed, overrides: Partial<StyleSeed>): StyleSeed {
  const clean: Partial<StyleSeed> = {};
  for (const [k, v] of Object.entries(overrides)) if (v !== undefined) (clean as Record<string, unknown>)[k] = v;
  return { ...base, ...clean };
}

/** 预设的档位默认 + 用户覆盖 → 完整种子。mode 单列，因为它决定取 light 还是 dark 那组色 */
export function seedOf(preset: StylePreset, mode: Mode, overrides: Partial<StyleSeed>): StyleSeed {
  const resolvedMode: "light" | "dark" = mode === "auto" ? systemMode() : mode;
  const colors = preset[resolvedMode];
  const base: StyleSeed = {
    mode,
    accent: colors.accent,
    bg: colors.bg,
    card: colors.card,
    text: colors.text,
    radius: preset.radius ?? "round",
    density: preset.density ?? "comfortable",
    shadow: preset.shadow ?? "soft",
    rail: preset.rail ?? "follow",
    texture: preset.texture ?? "none",
    tint: preset.tint ?? 1,
    ink: preset.ink ?? "standard",
  };
  return { ...mergeSeed(base, overrides), mode };
}

export function systemMode(): "light" | "dark" {
  return typeof matchMedia === "function" && matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}
