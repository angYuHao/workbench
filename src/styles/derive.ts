/** L2 派生层：种子 → 界面变量表。纯函数，不碰 DOM，可以单独测。
 *  规则是「只有这一层算颜色」：CSS 和组件一律只读这里发出去的变量名。
 *
 *  mix(a, b, w) 的含义是 a 占 w、b 占 1-w，所以「往基底里掺一点别的颜色」要写成
 *  mix(掺入色, 基底, 掺入比例)。写反的后果不是报错，是 light 主题算出近黑的底色 —— 检查脚本里有亮度断言兜着。 */

import { contrast, mix, pickForeground, readable, withAlpha } from "./color";
import { DENSITY, RADIUS, SHADOW, systemMode, type StyleSeed } from "./seeds";

export type Tokens = Record<string, string>;

/** 主色同时被当两件事用：铺在底下让字压得住、和直接当文字。这两个诉求对亮度的要求相反，必须拆成两个变量 */
const TEXT_TARGET = 4.5;
const FILL_TARGET = 3;

const BLACK = "#000000";
const WHITE = "#ffffff";
/** 中性阴影的底色：纯黑在浅色主题里发脏，压一点海军蓝进去 */
const SHADE = "#0f172a";

export function deriveTokens(seed: StyleSeed): Tokens {
  const dark = seed.mode === "dark" || (seed.mode === "auto" && systemMode() === "dark");
  const { accent: acc, bg, card } = seed;
  const ink = dark ? WHITE : BLACK;
  /** 文字浓度：整条文字灰阶一起挪 —— 描边、斑马纹本来就从文字算，会跟着挪，不会只淡一半。
   *  `?? "standard"` 是给这个字段加进来之前存下的用户风格兜底的。 */
  const inkStep = seed.ink ?? "standard";
  const text = inkStep === "standard" ? seed.text : inkStep === "strong" ? mix(ink, seed.text, 0.15) : mix(card, seed.text, 0.15);
  const dens = DENSITY[seed.density];
  const rad = RADIUS[seed.radius];
  const tintW = [0, 0.1, 0.2][seed.tint];
  /** 染力度越高：阴影越淡、扩散越大、越带主色相 —— 「轻」落在阴影上就是这几条 */
  const air = seed.tint > 1 ? 0.72 : 1;
  const level = SHADOW[seed.shadow];
  const t: Tokens = {};

  const primaryText = contrast(acc, card) >= TEXT_TARGET ? acc : readable(acc, card, TEXT_TARGET);
  /** 0.12 / 0.13 这两个权重是照着 classic 能复现改版前那圈灰校准的，动一下就会在深色里多出可见的一层边 */
  const baseBorder = mix(text, card, dark ? 0.13 : 0.12);
  const baseStripe = mix(text, card, 0.5);

  t["--color-primary"] = acc;
  /** 实心填充专用的主色：青绿、天蓝这类亮色压不住白字（只有 2:1），配黑字又发闷。
   *  所以「色块上要写字」的场合一律换成把它往深压到白字读得清的那一档，
   *  而描边、浅底、当文字用的地方仍然是用户挑的那个原色 —— 一个色相，两种浓度。 */
  const solid = contrast(acc, WHITE) >= TEXT_TARGET ? acc : readable(acc, WHITE, TEXT_TARGET);
  t["--color-primary-solid"] = solid;
  t["--color-primary-solid-hover"] = mix(solid, ink, dark ? 0.88 : 0.9);
  t["--color-primary-hover"] = mix(acc, ink, dark ? 0.88 : 0.9);
  t["--color-primary-active"] = mix(acc, ink, dark ? 0.8 : 0.82);
  t["--color-primary-soft"] = mix(acc, card, dark ? 0.22 : 0.1);
  t["--color-primary-faint"] = mix(acc, card, dark ? 0.14 : 0.055);
  t["--color-primary-fg"] = pickForeground(solid);
  t["--color-primary-text"] = primaryText;
  t["--color-primary-ring"] = withAlpha(acc, dark ? 0.45 : 0.32);

  t["--color-bg"] = bg;
  t["--color-card"] = card;
  t["--color-card-raised"] = dark ? mix(WHITE, card, 0.06) : WHITE;
  t["--color-sunken"] = mix(ink, bg, dark ? 0.28 : 0.035);

  t["--color-text"] = text;
  /** 次要文字是在文字之上又往面上挪了一次，「淡」这一档叠两遍会跌破 3:1，所以给它一道地板：
   *  只在真的不够时才往回压，其余情况原样放行 */
  t["--color-text-secondary"] = readable(mix(text, card, dark ? 0.66 : 0.6), card, 3);
  t["--color-text-muted"] = mix(text, card, dark ? 0.5 : 0.45);

  t["--color-border"] = mix(acc, baseBorder, tintW);
  t["--color-border-strong"] = mix(acc, mix(text, card, dark ? 0.3 : 0.26), tintW * 0.7);
  t["--color-row-stripe"] = withAlpha(mix(acc, baseStripe, tintW), dark ? 0.05 : 0.03);
  t["--color-hover-sunken"] = withAlpha(mix(acc, baseStripe, tintW), dark ? 0.07 : 0.055);

  const rail = deriveRail(seed, dark);
  t["--color-sidebar"] = rail.bg;
  t["--color-sidebar-text"] = rail.text;
  t["--color-sidebar-hover"] = rail.hover;

  const states: Array<[string, string]> = [
    ["info", dark ? "#38bdf8" : "#2563eb"],
    ["success", dark ? "#34d399" : "#16a34a"],
    ["warning", dark ? "#fbbf24" : "#d97706"],
    ["danger", dark ? "#f87171" : "#dc2626"],
  ];
  for (const [name, raw] of states) {
    // 深色主题里把状态色往白提一档，否则同饱和度的红绿压在深底上会糊成一团
    const c = dark ? mix(WHITE, raw, 0.06) : raw;
    // 状态文字是渲染在自己的浅底上的（提示泡、标签），所以达标要按那块底算，按主面板算会差一口气
    const soft = mix(c, card, dark ? 0.2 : 0.09);
    t[`--color-${name}`] = c;
    t[`--color-${name}-soft`] = soft;
    t[`--color-${name}-edge`] = mix(c, card, dark ? 0.36 : 0.26);
    t[`--color-${name}-text`] = contrast(c, soft) >= TEXT_TARGET ? c : readable(c, soft, TEXT_TARGET);
    /** 状态色也可能被当实心用（危险按钮、删除角标），同样配一条压深过的填充色 */
    const solidC = contrast(c, WHITE) >= TEXT_TARGET ? c : readable(c, WHITE, TEXT_TARGET);
    t[`--color-${name}-solid`] = solidC;
    t[`--color-${name}-fg`] = pickForeground(solidC);
  }

  t["--color-overlay"] = dark ? "rgba(0, 0, 0, 0.6)" : "rgba(0, 0, 0, 0.45)";
  // 提示泡反过来：浅主题压成深海军蓝、深主题提浅，永远和四周的面拉开（不是「近乎纯黑」）
  t["--color-tooltip-bg"] = dark ? mix(WHITE, card, 0.1) : mix(BLACK, SHADE, 0.15);
  t["--color-tooltip-fg"] = dark ? "#111827" : WHITE;
  /** 纸面不跟主题走：预览 Word/PDF 时底色就该是白的，否则会像被蒙了一层灰 */
  t["--color-paper"] = WHITE;
  t["--color-scrollbar"] = mix(text, card, dark ? 0.35 : 0.2);

  const shade = dark ? mix(BLACK, acc, 0.02 + tintW * 0.5) : mix(SHADE, acc, 0.06 + tintW * 0.5);
  const spread = (base: number) => Math.round(base + tintW * 34);
  t["--shadow-card"] = level === 0 ? "none" : `0 1px 2px ${withAlpha(shade, 0.05 * level * air)}, 0 ${2 + level}px ${spread(6 + 4 * level)}px ${withAlpha(shade, (0.05 * level + 0.015) * air)}`;
  t["--shadow-card-hover"] = level === 0 ? "none" : `0 2px 4px ${withAlpha(shade, 0.06 * level * air)}, 0 ${6 + 2 * level}px ${spread(14 + 6 * level)}px ${withAlpha(shade, (0.07 * level + 0.03) * air)}`;
  t["--shadow-floating"] = level === 0 ? "none" : `0 4px ${spread(12)}px ${withAlpha(shade, (0.12 + 0.04 * level) * air)}`;
  t["--shadow-modal"] = level === 0 ? "none" : `0 8px ${spread(32)}px ${withAlpha(shade, (0.16 + 0.05 * level) * air)}`;
  t["--shadow-control"] = level === 0 ? "none" : `0 1px 3px ${withAlpha(shade, 0.12 * air)}`;

  t["--radius-card"] = `${rad}px`;
  t["--radius-control"] = `${Math.max(2, rad - 4)}px`;
  t["--radius-icon"] = `${Math.max(2, Math.round(rad * 0.5))}px`;

  t["--p-gap"] = `${dens.gap}px`;
  t["--p-pad"] = `${dens.pad}px`;
  t["--p-pad-lg"] = `${dens.padLg}px`;
  t["--p-ctrl-h"] = `${dens.ctrl}px`;
  t["--p-font"] = `${dens.font}px`;
  t["--p-texture"] = deriveTexture(seed, t, dark);

  /** 代码高亮：不再引 highlight.js 的成品主题，改成吃派生色 —— 换风格时代码块自己跟着变 */
  const codeBg = dark ? mix(BLACK, bg, 0.3) : mix(SHADE, card, 0.03);
  t["--color-code-bg"] = codeBg;
  t["--color-code-fg"] = dark ? "#d6deeb" : readable(text, codeBg, TEXT_TARGET);
  t["--color-code-comment"] = mix(text, card, dark ? 0.45 : 0.42);
  t["--color-code-keyword"] = readable(dark ? "#c792ea" : "#8250df", codeBg, TEXT_TARGET);
  t["--color-code-string"] = readable(dark ? "#9ecb7f" : "#0a7d33", codeBg, TEXT_TARGET);
  t["--color-code-number"] = readable(dark ? "#d19a66" : "#9a5700", codeBg, TEXT_TARGET);
  t["--color-code-title"] = readable(dark ? "#61afef" : "#1d4ed8", codeBg, TEXT_TARGET);
  t["--color-code-builtin"] = readable(dark ? "#56b6c2" : "#0e7490", codeBg, TEXT_TARGET);
  t["--color-code-name"] = readable(dark ? "#e06c75" : "#b91c1c", codeBg, TEXT_TARGET);

  return t;
}

/** 侧边导轨三态。深色那一档的两个值是改版前实测能读的，留着让 classic 还能复现今天 */
function deriveRail(seed: StyleSeed, dark: boolean): { bg: string; text: string; hover: string } {
  const acc = seed.accent;
  if (seed.rail === "follow") {
    const bg = dark ? mix(BLACK, seed.bg, 0.45) : mix(BLACK, seed.bg, 0.04);
    return { bg, text: mix(seed.text, bg, dark ? 0.62 : 0.72), hover: withAlpha(dark ? WHITE : BLACK, dark ? 0.08 : 0.05) };
  }
  if (seed.rail === "tinted") {
    const bg = dark ? mix(acc, seed.bg, 0.22) : mix(acc, seed.bg, 0.16);
    // 图标色用「刚好读得清的主色」而不是「掺了七成黑的主色」：前者还是同一个色相，后者压在亮导轨上就是一团黑
    return { bg, text: readable(acc, bg, 4.5), hover: withAlpha(dark ? WHITE : acc, dark ? 0.1 : 0.14) };
  }
  return {
    bg: dark ? "#0b1220" : "#1e293b",
    text: dark ? "#94a3b8" : "#cbd5e1",
    hover: withAlpha(WHITE, dark ? 0.08 : 0.1),
  };
}

function deriveTexture(seed: StyleSeed, t: Tokens, dark: boolean): string {
  if (seed.texture === "none") return "none";
  if (seed.texture === "grid") {
    const line = withAlpha(t["--color-border"], 0.5);
    return `repeating-linear-gradient(0deg, ${line} 0 1px, transparent 1px 22px), repeating-linear-gradient(90deg, ${line} 0 1px, transparent 1px 22px)`;
  }
  if (seed.texture === "noise") return `radial-gradient(${withAlpha(t["--color-text-muted"], 0.35)} 0.5px, transparent 0.6px)`;
  return `linear-gradient(160deg, ${withAlpha(seed.accent, dark ? 0.12 : 0.07)}, transparent 55%)`;
}

/** 面板上的对比度读数。单独出函数，免得把非样式的值混进变量表里注入到 DOM 上 */
export function contrastReport(seed: StyleSeed): Array<{ label: string; value: number; target: number }> {
  const t = deriveTokens(seed);
  return [
    { label: "实心主色上的字", value: contrast(t["--color-primary-fg"], t["--color-primary-solid"]), target: TEXT_TARGET },
    { label: "主色当文字", value: contrast(t["--color-primary-text"], t["--color-card"]), target: TEXT_TARGET },
    { label: "正文", value: contrast(t["--color-text"], t["--color-card"]), target: TEXT_TARGET },
    { label: "次要文字", value: contrast(t["--color-text-secondary"], t["--color-card"]), target: FILL_TARGET },
    { label: "导轨图标", value: contrast(t["--color-sidebar-text"], t["--color-sidebar"]), target: FILL_TARGET },
    { label: "状态提示字", value: contrast(t["--color-info-text"], t["--color-info-soft"]), target: TEXT_TARGET },
  ];
}
