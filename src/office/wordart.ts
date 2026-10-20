/** docx-preview 0.4.1 只认 drawing 里的 pic（图片）和有限的 VML 形状：
    - DrawingML 的 wps:wsp（艺术字、文本框、形状）整块丢掉；
    - VML 的 v:textpath（老式艺术字）只画形状，字在 string 属性里，一起丢。
    这里在交给 docx-preview 之前先扫一遍 document.xml：把这些认不出来的图形摘掉，
    原位埋一个书签当锚点（docx-preview 会把 bookmarkStart 渲成 <span id>），
    页面渲好后按锚点把自画的 SVG 叠回去。 */

import JSZip from "jszip";

const NS_W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const EMU_PER_PX = 9525;
const PX_PER_PT = 4 / 3;
const SVG_NS = "http://www.w3.org/2000/svg";

const kids = (el: Element | null | undefined, local: string): Element[] =>
  el ? [...el.children].filter((c) => c.localName === local) : [];

const one = (el: Element | null | undefined, local: string): Element | null => kids(el, local)[0] ?? null;

const at = (el: Element | null | undefined, local: string): string | null => {
  if (!el) return null;
  for (const a of el.attributes) if (a.localName === local) return a.value;
  return null;
};

const valOf = (el: Element | null | undefined, fallback = 0): number => {
  const n = el ? Number(el.textContent) : NaN;
  return Number.isFinite(n) ? n : fallback;
};

const numAttr = (el: Element | null | undefined, local: string, fallback: number): number => {
  const v = at(el, local);
  const n = v === null ? NaN : Number(v);
  return Number.isFinite(n) ? n : fallback;
};

/** OOXML 绝大多数数值属性都写作 w:val="…" */
const numAt = (el: Element | null | undefined, fallback = 0): number => numAttr(el, "val", fallback);

/** 各家写法前缀不一，只能按 localName 递归找 */
function findAll(root: Element, local: string, out: Element[] = []): Element[] {
  for (const c of [...root.children]) {
    if (c.localName === local) out.push(c);
    findAll(c, local, out);
  }
  return out;
}

/** 常见写法：<w:drawing> 和 <w:pict> 成对出现在 mc:AlternateContent 里，只处理外层那一个容器 */
const topLevel = (el: Element): boolean => {
  for (let p = el.parentElement; p; p = p.parentElement) if (p.localName === "AlternateContent") return false;
  return true;
};

/* ------------------------------- 颜色 ------------------------------- */

interface ThemeFont {
  latin: string;
  ea: string;
  /** a:font/@script → typeface：中文按简繁、日文按假名各自的替代字体 */
  byScript: Record<string, string>;
}

interface Theme {
  colors: Record<string, string>;
  fonts: { majorFont: ThemeFont; minorFont: ThemeFont };
}

/** 没有 themePart（或只有局部主题）时按 Office 默认调色板走，避免把 tx1 认成某个强调色 */
const DEFAULT_COLORS: Theme["colors"] = {
  dk1: "000000",
  lt1: "FFFFFF",
  dk2: "44546A",
  lt2: "E7E6E6",
  accent1: "4472C4",
  accent2: "ED7D31",
  accent3: "A5A5A5",
  accent4: "FFC000",
  accent5: "5B9BD5",
  accent6: "70AD47",
  hlink: "0563C1",
  folHlink: "954F72",
};

const emptyFont = (): ThemeFont => ({ latin: "", ea: "", byScript: {} });

const emptyTheme = (): Theme => ({ colors: { ...DEFAULT_COLORS }, fonts: { majorFont: emptyFont(), minorFont: emptyFont() } });

function readTheme(doc: Document): Theme {
  const theme: Theme = { colors: { ...DEFAULT_COLORS }, fonts: { majorFont: emptyFont(), minorFont: emptyFont() } };
  const scheme = findAll(doc.documentElement, "clrScheme")[0];
  for (const entry of [...(scheme?.children ?? [])]) {
    const srgb = one(entry, "srgbClr");
    const val = srgb ? at(srgb, "val") : at(one(entry, "sysClr"), "lastClr");
    if (val) theme.colors[entry.localName] = val.toUpperCase().replace("#", "");
  }
  for (const scale of ["majorFont", "minorFont"] as const) {
    const el = one(findAll(doc.documentElement, "fontScheme")[0], scale);
    if (!el) continue;
    const font = theme.fonts[scale];
    font.latin = at(one(el, "latin"), "typeface") ?? "";
    font.ea = at(one(el, "ea"), "typeface") ?? "";
    for (const alt of kids(el, "font")) {
      const script = at(alt, "script");
      const typeface = at(alt, "typeface");
      if (script && typeface) font.byScript[script] = typeface;
    }
  }
  return theme;
}

const splitRgb = (h: string): [number, number, number] => {
  const s = h.replace("#", "");
  const full = s.length === 3 ? s.split("").map((c) => c + c).join("") : s;
  return [parseInt(full.slice(0, 2), 16) || 0, parseInt(full.slice(2, 4), 16) || 0, parseInt(full.slice(4, 6), 16) || 0];
};

const toHex = (c: [number, number, number]): string =>
  "#" + c.map((v) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, "0")).join("");

/** 明度修饰近似处理：shade 压暗、tint 提亮、lumMod/lumOff 按 HSL 近似 */
function applyMods(base: string, holder: Element | null): string {
  if (!holder) return base;
  let [r, g, b] = splitRgb(base);
  for (const m of [...holder.children]) {
    const v = numAt(m, 100000) / 100000;
    if (m.localName === "shade") [r, g, b] = [r * (1 - v), g * (1 - v), b * (1 - v)];
    else if (m.localName === "tint") [r, g, b] = [r + (255 - r) * v, g + (255 - g) * v, b + (255 - b) * v];
    else if (m.localName === "lumMod") [r, g, b] = [r * v, g * v, b * v];
    else if (m.localName === "lumOff") [r, g, b] = [r + 255 * v, g + 255 * v, b + 255 * v];
  }
  return toHex([r, g, b]);
}

function colorOf(filler: Element | null | undefined, theme: Theme, fallback: string): string {
  if (!filler) return fallback;
  const srgb = one(filler, "srgbClr");
  if (srgb) return applyMods("#" + (at(srgb, "val") ?? "808080"), srgb);
  const sys = one(filler, "sysClr");
  if (sys) {
    const last = at(sys, "lastClr");
    if (last) return "#" + last;
    return at(sys, "val") === "windowText" ? "#000000" : fallback;
  }
  const scheme = one(filler, "schemeClr");
  if (scheme) {
    const key = at(scheme, "val") ?? "";
    const alias: Record<string, string> = { tx1: "dk1", tx2: "dk2", bg1: "lt1", bg2: "lt2" };
    const base = theme.colors[alias[key] ?? key] ?? theme.colors.accent1;
    return applyMods("#" + base, scheme);
  }
  const direct = at(filler, "val");
  return direct ? applyMods("#" + direct.replace("#", ""), null) : fallback;
}

interface Fill {
  stops: { pos: number; color: string }[];
  angleDeg: number;
}

const solid = (color: string): Fill => ({ stops: [{ pos: 0, color }, { pos: 100, color }], angleDeg: 90 });

/** a:gradFill 有就用它；只剩 wps:style 的 fillRef 时，按主题色编一段近似 Word 预设样式的三段渐变 */
function fillOf(wsp: Element, theme: Theme): Fill | null {
  const spPr = one(wsp, "spPr");
  const grad = one(spPr, "gradFill");
  const stops = kids(one(grad, "gsLst"), "gs").map((gs) => ({
    pos: numAttr(gs, "pos", 50) * 2,
    color: colorOf(gs.children[0], theme, "#808080"),
  }));
  if (stops.length >= 2) return { stops, angleDeg: numAttr(one(grad, "lin"), "ang", 90) / 60000 };
  if (one(spPr, "noFill")) return null;
  const solidFill = one(spPr, "solidFill");
  if (solidFill) return solid(colorOf(solidFill, theme, "#808080"));
  const fillRef = one(one(wsp, "style"), "fillRef");
  if (!fillRef) return null;
  const base = colorOf(fillRef, theme, "#4472C4");
  return {
    stops: [
      { pos: 0, color: tintUp(base, 0.65) },
      { pos: 50, color: base },
      { pos: 100, color: tintUp(base, -0.4) },
    ],
    angleDeg: 90,
  };
}

const tintUp = (color: string, ratio: number): string => {
  let [r, g, b] = splitRgb(color);
  if (ratio >= 0) [r, g, b] = [r + (255 - r) * ratio, g + (255 - g) * ratio, b + (255 - b) * ratio];
  else [r, g, b] = [r * (1 + ratio), g * (1 + ratio), b * (1 + ratio)];
  return toHex([r, g, b]);
};

/* ------------------------------ 形状数据 ------------------------------ */

interface ArtRun {
  text: string;
  font: string;
  sizePx: number;
  bold: boolean;
  italic: boolean;
  color: string | null;
  /** Word 的艺术字其实是字符级效果：w14:textFill / textOutline / shadow 挂在 rPr 上 */
  stroke: { color: string; wPx: number } | null;
  shadow: { color: string; dxPx: number; dyPx: number } | null;
}

export interface ArtShape {
  marker: string;
  wPx: number;
  hPx: number;
  rotDeg: number;
  warp: string;
  fill: Fill | null;
  outline: { color: string; wPx: number } | null;
  lines: ArtRun[][];
  /** inline 的图按文字流排，浮动图按相对页面的坐标摆 */
  floating: boolean;
  leftMode: string;
  topMode: string;
  leftPx: number;
  topPx: number;
  /** wp:align（没有 posOffset 时 Word 用 left/center/right、top/center/bottom 定位） */
  leftAlign: string;
  topAlign: string;
  /** 对齐是针对整个 anchor 框（group 的话是组的外框）算的，组内子形状再叠 dxPx/dyPx */
  alignW: number;
  alignH: number;
  dxPx: number;
  dyPx: number;
  /** wps:bodyPr 的对齐与内边距：anchor="t" 时文字贴顶，不是垂直居中 */
  anchorV: string;
  anchorH: string;
  insetL: number;
  insetT: number;
  insetR: number;
  lineAlign: string[];
}

const flagOn = (rPr: Element | null, local: string): boolean => {
  const el = one(rPr, local);
  if (!el) return false;
  const v = at(el, "val");
  return v === null || (v !== "0" && v !== "false");
};

/** 从一段填充里取一个代表色：渐变取中间那档，实色直接读 */
function fillFrom(el: Element | null, theme: Theme, fallback: string): string | null {
  if (!el) return null;
  const grad = one(el, "gradFill");
  if (grad) {
    const gs = kids(one(grad, "gsLst"), "gs");
    const mid = gs[Math.floor((gs.length - 1) / 2)] ?? gs[0];
    return mid ? colorOf(mid.children[0], theme, fallback) : null;
  }
  return colorOf(one(el, "solidFill") ?? el, theme, fallback);
}

/** Word 的「艺术字」效果其实是字符级的：w14:textFill 覆盖字色，textOutline 描边，shadow 投影 */
function effectsOf(rPr: Element | null, theme: Theme): Pick<ArtRun, "color" | "stroke" | "shadow"> {
  const tf = one(rPr, "textFill");
  const color = fillFrom(tf ?? one(rPr, "color"), theme, "#000000");
  const to = one(rPr, "textOutline");
  const stroke = to ? { color: fillFrom(to, theme, "#000000") ?? "#000000", wPx: Math.max(0.5, emuPx(numAttr(to, "w", 9525))) } : null;
  const sh = one(rPr, "shadow");
  let shadow: ArtRun["shadow"] = null;
  if (sh) {
    const dist = emuPx(numAttr(sh, "dist", 38100));
    const dir = (numAttr(sh, "dir", 2700000) / 60000) * (Math.PI / 180);
    shadow = { color: fillFrom(sh, theme, "#808080") ?? "#808080", dxPx: dist * Math.cos(dir), dyPx: dist * Math.sin(dir) };
  }
  return { color, stroke, shadow };
}

/** 中日韩的字形字体是主题按「文字分型」给的，不是按 lang；这里按码位粗分，认不出当拉丁 */
const eaScript = (ch: string): string => {
  const c = ch.codePointAt(0) ?? 0;
  if (c >= 0xac00 && c <= 0xd7af) return "Hangul";
  if (c >= 0x30a0 && c <= 0x30ff) return "Katakana";
  if (c >= 0x3040 && c <= 0x309f) return "Hiragana";
  if ((c >= 0x3400 && c <= 0x4dbf) || (c >= 0x4e00 && c <= 0x9fff)) return "Hans";
  return "";
};

/** rFonts 显式给名就用，没给按 docDefaults 的惯例回落到主题字体；都拿不到就留空，让文字继承页面字体 */
function fontOf(rPr: Element | null, theme: Theme, text: string): string {
  const fonts = one(rPr, "rFonts");
  const script = [...text].map(eaScript).find(Boolean) ?? "";
  const ea = Boolean(script);
  const direct = fonts ? (ea ? at(fonts, "eastAsia") : at(fonts, "ascii") ?? at(fonts, "hAnsi")) : null;
  if (direct) return direct;
  const scale = (fonts ? (ea ? at(fonts, "eastAsiaTheme") : at(fonts, "asciiTheme")) : null) ?? "minor";
  const font = theme.fonts[scale.startsWith("major") ? "majorFont" : "minorFont"];
  /** 主题的 a:ea 常常是空的，实际替代字面记在逐分型的 a:font 里 */
  return (ea ? font.byScript[script] || font.ea : font.latin) || "";
}

function runsOf(content: Element, theme: Theme): ArtRun[][] {
  const lines: ArtRun[][] = [];
  for (const p of kids(content, "p")) {
    const line: ArtRun[] = [];
    for (const r of findAll(p, "r")) {
      const text = kids(r, "t").map((t) => t.textContent ?? "").join("");
      if (!text) continue;
      const rPr = one(r, "rPr") ?? one(one(p, "pPr"), "rPr");
      line.push({
        text,
        font: fontOf(rPr, theme, text),
        sizePx: (numAt(one(rPr, "sz"), 24) / 2) * PX_PER_PT,
        bold: flagOn(rPr, "b"),
        italic: flagOn(rPr, "i"),
        ...effectsOf(rPr, theme),
      });
    }
    if (line.length) lines.push(line);
  }
  return lines;
}

/** 每行的水平对齐：段落级 w:jc 优先，没有就按 bodyPr 的 anchorCtr */
function alignsOf(content: Element): string[] {
  return kids(content, "p").map((p) => (at(one(one(p, "pPr"), "jc"), "val") ?? "").toLowerCase());
}

/** DrawingML：一个 anchor/inline 里可能套 group；单图形用外层框尺寸，组里的子形状按组的 xfrm 换算回页面坐标 */
function collectDrawing(drawing: Element, theme: Theme): ArtShape[] {
  const box = kids(drawing, "anchor")[0] ?? kids(drawing, "inline")[0];
  if (!box) return [];
  const wsps = findAll(box, "wsp").filter((w) => one(w, "txbx"));
  if (!wsps.length) return [];
  const outer = one(box, "extent");
  const posH = one(box, "positionH");
  const posV = one(box, "positionV");
  const grp = findAll(box, "grpSp")[0] ?? null;
  const gxfrm = grp ? one(one(grp, "grpSpPr"), "xfrm") : null;
  const gExt = one(gxfrm, "ext");
  const gChOff = one(gxfrm, "chOff");
  const gChExt = one(gxfrm, "chExt");
  /** 组的缩放比：子空间的 chExt 映射到外部的 ext */
  const sx = scaleOf(gExt, gChExt, "cx");
  const sy = scaleOf(gExt, gChExt, "cy");
  const many = wsps.length > 1;
  const out: ArtShape[] = [];
  for (const wsp of wsps) {
    const content = one(one(wsp, "txbx"), "txbxContent");
    if (!content) continue;
    const lines = runsOf(content, theme);
    if (!lines.length) continue;
    const xfrm = one(one(wsp, "spPr"), "xfrm");
    const own = one(xfrm, "ext");
    const off = one(xfrm, "off");
    const bp = one(wsp, "bodyPr");
    const inGrp = Boolean(gxfrm);
    out.push({
      marker: "",
      wPx: Math.round(emuPx((many && own ? numAttr(own, "cx", 0) : numAttr(outer, "cx", 0)) * (inGrp ? sx : 1))),
      hPx: Math.round(emuPx((many && own ? numAttr(own, "cy", 0) : numAttr(outer, "cy", 0)) * (inGrp ? sy : 1))),
      rotDeg: numAttr(xfrm, "rot", 0) / 60000,
      warp: at(one(bp, "prstTxWarp"), "prst") ?? "",
      fill: fillOf(wsp, theme),
      outline: outlineOf(wsp, theme),
      lines,
      floating: box.localName === "anchor",
      leftMode: at(posH, "relativeFrom") ?? "margin",
      topMode: at(posV, "relativeFrom") ?? "paragraph",
      /** anchor 自带页面坐标；组内子形状另加换算后的组内偏移 */
      leftPx: Math.round(emuPx(valOf(one(posH, "posOffset"), 0))),
      topPx: Math.round(emuPx(valOf(one(posV, "posOffset"), 0))),
      leftAlign: alignOf(posH),
      topAlign: alignOf(posV),
      alignW: Math.round(emuPx(numAttr(outer, "cx", 0))),
      alignH: Math.round(emuPx(numAttr(outer, "cy", 0))),
      dxPx: inGrp ? Math.round(emuPx((numAttr(off, "x", 0) - numAttr(gChOff, "x", 0)) * sx)) : 0,
      dyPx: inGrp ? Math.round(emuPx((numAttr(off, "y", 0) - numAttr(gChOff, "y", 0)) * sy)) : 0,
      anchorV: at(bp, "anchor") ?? "ctr",
      anchorH: at(bp, "anchorCtr") === "1" ? "ctr" : "l",
      insetL: emuPx(numAttr(bp, "lIns", 91440)),
      insetT: emuPx(numAttr(bp, "tIns", 45720)),
      insetR: emuPx(numAttr(bp, "rIns", 91440)),
      lineAlign: alignsOf(content),
    });
  }
  return out;
}

const scaleOf = (ext: Element | null, chExt: Element | null, local: string): number => {
  const a = numAttr(ext, local, 0);
  const b = numAttr(chExt, local, 0);
  return b > 0 ? a / b : 1;
};

/** <wp:align>center</wp:align>；同级的 posOffset 不能混进来，所以只取 align 子节点的文本 */
const alignOf = (pos: Element | null): string => (pos ? (one(pos, "align")?.textContent ?? "").trim().toLowerCase() : "");

const emuPx = (v: number) => v / EMU_PER_PX;

function outlineOf(wsp: Element, theme: Theme): ArtShape["outline"] {
  const ln = one(one(wsp, "spPr"), "ln");
  if (ln) {
    if (one(ln, "noFill")) return null;
    return { color: colorOf(ln, theme, "#404040"), wPx: Math.max(0.5, emuPx(numAttr(ln, "w", 12700))) };
  }
  const lnRef = one(one(wsp, "style"), "lnRef");
  return lnRef ? { color: tintUp(colorOf(lnRef, theme, "#4472C4"), -0.35), wPx: 1 } : null;
}

/** VML 的预设变形编号（MS-ODRAW 的 wordart shapetype），认不出的按直线 */
const VML_WARP: Record<string, string> = {
  "136": "textArchUp", "137": "textArchDown", "138": "textArchUp", "139": "textArchDown",
  "143": "textWave1", "144": "textWave2", "145": "textWave3", "146": "textWave1",
  "147": "textInflate", "148": "textDeflate", "149": "textInflate", "150": "textDeflate",
  "151": "textStop", "152": "textStop", "153": "textFadeUp", "154": "textFadeDown",
  "155": "textSlantUp", "156": "textSlantDown", "157": "textCurveUp", "158": "textCurveDown",
};

/** VML 的 style 串里量纲混着 pt/px，统一换成 px */
function styleVal(style: string | null, key: string): number | null {
  if (!style) return null;
  const m = new RegExp("(?:^|;)\\s*" + key + "\\s*:\\s*(-?[\\d.]+)\\s*(pt|px)?", "i").exec(style);
  if (!m) return null;
  const v = Number(m[1]);
  if (!Number.isFinite(v)) return null;
  return m[2] === "pt" || !m[2] ? v * PX_PER_PT : v;
}

function collectVml(pict: Element, theme: Theme): ArtShape[] {
  const out: ArtShape[] = [];
  for (const shape of findAll(pict, "shape")) {
    const tp = one(shape, "textpath");
    const text = tp ? (at(tp, "string") ?? "").trim() : "";
    /** 兜底写法里还有一种：文字在 v:textbox/w:txbxContent 里（老式文本框），没有 textpath */
    const content = one(one(shape, "textbox"), "txbxContent");
    const lines: ArtRun[][] = text ? [[]] : content ? runsOf(content, theme) : [];
    if (text) lines[0] = [vmlRun(tp, text)];
    if (!lines.some((l) => l.length)) continue;
    const style = at(shape, "style");
    const fill = one(shape, "fill");
    const c1 = at(shape, "fillcolor") ?? at(fill, "color");
    const c2 = at(fill, "color2");
    const stroke = at(shape, "strokecolor") ?? at(one(shape, "stroke"), "color");
    out.push({
      marker: "",
      wPx: Math.round(styleVal(style, "width") ?? 200),
      hPx: Math.round(styleVal(style, "height") ?? 70),
      rotDeg: -(styleVal(style, "rotation") ?? 0),
      warp: VML_WARP[/#_x0000_t(\d+)/.exec(at(shape, "type") ?? "")?.[1] ?? ""] ?? "",
      fill: c1 ? { stops: [{ pos: 0, color: sharp(c1) }, { pos: 100, color: sharp(c2 ?? c1) }], angleDeg: numAttr(fill, "angle", 45) } : null,
      outline: stroke ? { color: sharp(stroke), wPx: 1 } : null,
      lines,
      floating: true,
      leftMode: "margin",
      topMode: "paragraph",
      leftPx: Math.round(styleVal(style, "margin-left") ?? styleVal(style, "left") ?? 0),
      topPx: Math.round(styleVal(style, "margin-top") ?? styleVal(style, "top") ?? 0),
      leftAlign: "",
      topAlign: "",
      alignW: 0,
      alignH: 0,
      dxPx: 0,
      dyPx: 0,
      anchorV: "ctr",
      anchorH: "ctr",
      insetL: 0,
      insetT: 0,
      insetR: 0,
      lineAlign: lines.map(() => ""),
    });
  }
  return out;
}

/** v:textpath 的 style 串里带字体字号，颜色沿用形状自己的 */
function vmlRun(tp: Element | null, text: string): ArtRun {
  const tpStyle = tp ? at(tp, "style") : null;
  return {
    text,
    font: /font-family\s*:\s*([^;]+)/i.exec(tpStyle ?? "")?.[1]?.replace(/["']/g, "").trim() ?? "",
    sizePx: styleVal(tpStyle, "font-size") ?? 40 * PX_PER_PT,
    bold: /font-weight\s*:\s*bold/i.test(tpStyle ?? ""),
    italic: /font-style\s*:\s*italic/i.test(tpStyle ?? ""),
    color: null,
    stroke: null,
    shadow: null,
  };
}

const sharp = (v: string): string => (v.startsWith("#") ? v : "#" + v.replace("#", ""));

/* ------------------------------ 摘除 + 埋点 ------------------------------ */

/** 只接管「带文字的图形」：文本框和艺术字自己画，图片继续交给 docx-preview */
const carriesText = (part: Element): boolean =>
  findAll(part, "wsp").some((w) => one(w, "txbx")) ||
  findAll(part, "shape").some((s) => one(s, "textpath") || one(s, "textbox"));

export async function prepareDocx(buffer: ArrayBuffer): Promise<{ buffer: ArrayBuffer; arts: ArtShape[] }> {
  const zip = await JSZip.loadAsync(buffer);
  const main = zip.file("word/document.xml");
  if (!main) return { buffer, arts: [] };
  const doc = parse(await main.async("string"));
  const body = one(doc.documentElement, "body");
  if (!body) return { buffer, arts: [] };

  const themePart = zip.file("word/theme/theme1.xml");
  const theme = themePart ? readTheme(parse(await themePart.async("string"))) : emptyTheme();

  const prefix = prefixOf(doc.documentElement, NS_W) ?? "w";
  const arts: ArtShape[] = [];
  const containers = [
    ...findAll(body, "AlternateContent"),
    ...findAll(body, "drawing").filter(topLevel),
    ...findAll(body, "pict").filter(topLevel),
  ];

  let seq = 0;
  for (const box of containers) {
    const parts = box.localName === "AlternateContent" ? [...findAll(box, "drawing"), ...findAll(box, "pict")] : [box];
    let found: ArtShape[] = [];
    for (const part of parts) {
      const hit = part.localName === "drawing" ? collectDrawing(part, theme) : collectVml(part, theme);
      if (hit.length) {
        found = hit;
        break;
      }
    }
    /** 空文本框（WPS 拿它显示「请在此放置您的文字」这类占位提示）也得摘掉，不然 docx-preview 会画出一个灰底框；纯图片不能碰 */
    if (!found.length && !parts.some(carriesText)) continue;
    const marker = found.length ? `qpArt${++seq}` : "";
    /** 摘掉的得是 docx-preview 自己认的那一层：mc:AlternateContent 整块摘（Choice/Fallback 它不解析，书签埋进去就找不到了） */
    const removed = box.parentElement?.localName === "r" ? box.parentElement : box;
    const parent = removed.parentElement;
    if (parent) {
      if (marker) spliceBookmark(doc, prefix, parent, removed.nextSibling, marker, 9000 + seq);
      parent.removeChild(removed);
    }
    if (!found.length) continue;
    for (const art of found) art.marker = marker;
    arts.push(...found);
  }

  if (!arts.length) return { buffer, arts: [] };
  zip.file("word/document.xml", '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n' + new XMLSerializer().serializeToString(doc.documentElement));
  return { buffer: await zip.generateAsync({ type: "arraybuffer", compression: "DEFLATE" }), arts };
}

function parse(text: string): Document {
  const doc = new DOMParser().parseFromString(text, "application/xml");
  if (doc.getElementsByTagName("parsererror").length) throw new Error("document.xml 解析失败");
  return doc;
}

function prefixOf(root: Element, ns: string): string | null {
  for (const a of root.attributes) if (a.value === ns) return a.name.startsWith("xmlns:") ? a.name.slice(6) : a.prefix;
  return null;
}

/** 在原来放图形的位置插一对书签，渲染后就是 <span id="qpArtN"> */
function spliceBookmark(doc: Document, prefix: string, parent: Element, ref: Node | null, marker: string, id: number) {
  const mk = (local: string) => {
    const el = doc.createElementNS(NS_W, `${prefix}:${local}`);
    el.setAttribute(`${prefix}:id`, String(id));
    return el;
  };
  const start = mk("bookmarkStart");
  start.setAttribute(`${prefix}:name`, marker);
  parent.insertBefore(start, ref);
  parent.insertBefore(mk("bookmarkEnd"), start.nextSibling);
}

/* ------------------------------ 画回去 ------------------------------ */

/** 常见预设变形用一条路径近似；表达不了的就退回直线 */
function warpOf(prst: string, w: number, h: number): { d: string | null; skew: number } {
  const pad = Math.max(8, w * 0.06);
  const mid = w / 2;
  const q = (y0: number, yc: number, y1: number) => `M ${pad},${y0} Q ${mid},${yc} ${w - pad},${y1}`;
  if (!prst) return { d: null, skew: 0 };
  if (/Arch(Up|Down)(Invariant)?$/i.test(prst) || /Rainbow/i.test(prst)) {
    const up = /Down/i.test(prst) ? false : true;
    return { d: q(h * (up ? 0.92 : 0.12), h * (up ? -0.25 : 1.25), h * (up ? 0.92 : 0.12)), skew: 0 };
  }
  if (/Curve(Up|Down)$/i.test(prst)) {
    const up = !/Down/i.test(prst);
    return { d: q(h * (up ? 0.85 : 0.2), h * (up ? 0.3 : 0.78), h * (up ? 0.85 : 0.2)), skew: 0 };
  }
  if (/Inflate/i.test(prst)) return { d: q(h * 0.72, h * 0.12, h * 0.72), skew: 0 };
  if (/Deflate/i.test(prst)) return { d: q(h * 0.45, h * 0.98, h * 0.45), skew: 0 };
  if (/Wave/i.test(prst)) {
    const n = /Triple/i.test(prst) ? 3 : /Double/i.test(prst) ? 2 : 1;
    const seg = (w - 2 * pad) / n;
    const amp = h * 0.24;
    let d = `M ${pad},${h * 0.6}`;
    for (let i = 0; i < n; i++) {
      const x = pad + seg * i;
      d += ` C ${x + seg * 0.25},${h * 0.6 - amp} ${x + seg * 0.75},${h * 0.6 + amp} ${x + seg},${h * 0.6}`;
    }
    return { d, skew: 0 };
  }
  if (/FadeUp$/i.test(prst)) return { d: `M ${pad},${h * 0.82} L ${w - pad},${h * 0.34}`, skew: 0 };
  if (/FadeDown$/i.test(prst)) return { d: `M ${pad},${h * 0.34} L ${w - pad},${h * 0.82}`, skew: 0 };
  if (/Stop$/i.test(prst)) return { d: null, skew: -12 };
  if (/SlantUp$/i.test(prst)) return { d: null, skew: -10 };
  if (/SlantDown$/i.test(prst)) return { d: null, skew: 10 };
  return { d: null, skew: 0 };
}

let uid = 0;

function svgEl(name: string, attrs: Record<string, string | number | undefined> = {}): SVGElement {
  const el = document.createElementNS(SVG_NS, name);
  for (const [k, v] of Object.entries(attrs)) if (v !== undefined) el.setAttribute(k, String(v));
  return el;
}

/** 一「画出来的行」：Word 的文本框会按框宽把一段文字断成好几行，这里是断完的结果 */
interface VLine {
  text: string;
  run: ArtRun;
  align: string;
}

const planOf = (art: ArtShape): VLine[] =>
  art.lines.map((line, i) => ({ text: line.map((r) => r.text).join(""), run: line[0], align: art.lineAlign[i] ?? "" }));

const fontAttrs = (r: ArtRun): Record<string, string | number | undefined> => ({
  "font-family": r.font || undefined,
  "font-size": r.sizePx,
  "font-weight": r.bold ? "700" : "400",
  "font-style": r.italic ? "italic" : "normal",
});

/** 这些标点不能顶在行首 */
const NO_LINE_HEAD = "，。、；：！？）》〉」』】…—～·";

/**
 * SVG 的 text 不会自己换行：整段排下去就冲出框外，看着像少了字。
 * Word 是按框的内容宽（框宽减左右内边距）断行——中日韩逐字可断，拉丁只在空格/标点后面断。
 * 字宽得实测，所以要等 svg 挂进文档后才能调；不需要断行（或变形字）时返回 null。
 */
function wrapPlan(art: ArtShape, svg: SVGSVGElement): VLine[] | null {
  if (art.warp) return null;
  const avail = art.wPx - art.insetL - art.insetR;
  if (avail <= 12) return null;
  const measurer = svgEl("text") as SVGTextContentElement;
  measurer.style.cssText = "visibility:hidden;white-space:pre";
  svg.appendChild(measurer);
  const charWidth = (r: ArtRun, ch: string): number => {
    for (const [k, v] of Object.entries(fontAttrs(r))) {
      if (v === undefined) measurer.removeAttribute(k);
      else measurer.setAttribute(k, String(v));
    }
    measurer.textContent = ch;
    return measurer.getComputedTextLength();
  };
  const plan: VLine[] = [];
  const cache = new Map<string, number>();
  art.lines.forEach((line, li) => {
    const align = art.lineAlign[li] ?? "";
    let cur = "";
    let used = 0;
    let head = line[0];
    const flush = () => {
      if (!cur) return;
      plan.push({ text: cur, run: head, align });
      cur = "";
      used = 0;
    };
    for (const r of line) {
      for (const ch of r.text) {
        const key = `${r.font}|${r.sizePx}|${r.bold}|${r.italic}|${ch}`;
        let w = cache.get(key);
        if (w === undefined) {
          w = charWidth(r, ch);
          cache.set(key, w);
        }
        const over = cur !== "" && used + w > avail;
        const breakable = eaScript(ch) !== "" || /[\s\-,.;:!?]/.test(cur.slice(-1));
        if (over && breakable && !NO_LINE_HEAD.includes(ch)) {
          flush();
          if (/\s/.test(ch)) continue;
        }
        if (!cur) head = r;
        cur += ch;
        used += w;
      }
    }
    flush();
  });
  measurer.remove();
  return plan.length > art.lines.length ? plan : null;
}

function renderArt(art: ArtShape, input?: VLine[]): SVGSVGElement {
  const plan = input ?? planOf(art);
  const id = `qpa${++uid}`;
  const svg = svgEl("svg", { width: art.wPx, height: art.hPx, viewBox: `0 0 ${art.wPx} ${art.hPx}` }) as SVGSVGElement;
  svg.style.cssText = "display:block;overflow:visible";
  svg.dataset.artMarker = art.marker;
  const defs = svg.appendChild(svgEl("defs"));

  let paint = "#333";
  if (art.fill) {
    const rad = (art.fill.angleDeg * Math.PI) / 180;
    const grad = defs.appendChild(
      svgEl("linearGradient", {
        id,
        x1: (0.5 - Math.cos(rad) / 2).toFixed(3),
        y1: (0.5 - Math.sin(rad) / 2).toFixed(3),
        x2: (0.5 + Math.cos(rad) / 2).toFixed(3),
        y2: (0.5 + Math.sin(rad) / 2).toFixed(3),
      }),
    );
    for (const s of art.fill.stops) grad.appendChild(svgEl("stop", { offset: (s.pos / 100).toFixed(3), "stop-color": s.color }));
    paint = `url(#${id})`;
  }

  const sizePx = Math.max(...art.lines.flat().map((r) => r.sizePx), 12);
  const { d, skew } = warpOf(art.warp, art.wPx, art.hPx);
  const g = svg.appendChild(svgEl("g"));
  const transform = [skew ? `skewX(${skew})` : "", art.rotDeg ? `rotate(${art.rotDeg} ${art.wPx / 2} ${art.hPx / 2})` : ""].filter(Boolean).join(" ");
  if (transform) g.setAttribute("transform", transform);

  /** 字色/描边/投影优先看字符级效果（Word 的艺术字就挂在这里），没有再退回形状自身的填充描边 */
  const runPaint = (r: ArtRun) => ({
    fill: r.color ?? paint,
    stroke: r.stroke ? r.stroke.color : art.outline?.color,
    "stroke-width": r.stroke ? r.stroke.wPx : art.outline?.wPx,
    "paint-order": "stroke" as const,
  });
  const shadowOf = (r: ArtRun) => (r.shadow ? { dx: r.shadow.dxPx, dy: r.shadow.dyPx, color: r.shadow.color } : null);

  if (d) {
    const pathId = `${id}p`;
    const path = defs.appendChild(svgEl("path", { id: pathId, d, fill: "none" }));
    path.style.visibility = "hidden";
    const head = art.lines[0]?.[0];
    const mk = (fill: string, stroke: string | undefined, wPx: number | undefined) => {
      const text = g.appendChild(
        svgEl("text", {
          ...(head ? fontAttrs(head) : { "font-size": sizePx }),
          fill,
          stroke,
          "stroke-width": wPx,
          "paint-order": "stroke",
        }),
      );
      const tp = text.appendChild(svgEl("textPath", { startOffset: "50%", "text-anchor": "middle" }));
      tp.setAttribute("href", `#${pathId}`);
      tp.setAttributeNS("http://www.w3.org/1999/xlink", "xlink:href", `#${pathId}`);
      tp.textContent = art.lines.flat().map((r) => r.text).join(" ");
      return text;
    };
    const sh = head ? shadowOf(head) : null;
    if (sh) mk(sh.color, undefined, undefined);
    if (head) {
      const p = runPaint(head);
      mk(p.fill, p.stroke, p["stroke-width"]);
    }
  } else {
    const lineH = (r: ArtRun) => r.sizePx * 1.15;
    const blockH = plan.reduce((s, v) => s + lineH(v.run), 0);
    const yTop =
      art.anchorV === "b" ? art.hPx - art.insetT - blockH : art.anchorV === "t" ? art.insetT : (art.hPx - blockH) / 2;
    let yCursor = yTop;
    plan.forEach((v) => {
      const r0 = v.run;
      const jc = v.align || (art.anchorH === "ctr" ? "center" : "");
      const anchorMid = jc === "center";
      const toRight = jc === "right" || jc === "end";
      const x = anchorMid ? art.wPx / 2 : toRight ? art.wPx - art.insetR : art.insetL;
      const y = yCursor + lineH(r0) / 2;
      yCursor += lineH(r0);
      const attrs = {
        x,
        y,
        "dominant-baseline": "central",
        "text-anchor": anchorMid ? "middle" : toRight ? "end" : "start",
        ...fontAttrs(r0),
      };
      const sh = shadowOf(r0);
      if (sh) {
        const shadowText = g.appendChild(svgEl("text", { ...attrs, x: x + sh.dx, y: y + sh.dy, fill: sh.color }));
        shadowText.textContent = v.text;
      }
      const text = g.appendChild(svgEl("text", { ...attrs, ...runPaint(r0) }));
      text.textContent = v.text;
    });
  }
  return svg;
}

/** Word 的锚点坐标：relativeFrom 决定原点和可用范围，align 在这个范围里贴边，posOffset 再偏移，组内子形状叠 dx/dy */
function place(
  art: ArtShape,
  secBox: DOMRect,
  mBox: DOMRect,
  padL: number,
  padT: number,
  padR: number,
  padB: number,
): { left: number; top: number } {
  const hPage = art.leftMode === "page";
  const vPage = art.topMode === "page";
  const hOrigin = hPage ? 0 : art.leftMode === "margin" || art.leftMode === "column" ? padL : mBox.left - secBox.left;
  const vOrigin = vPage ? 0 : art.topMode === "margin" || art.topMode === "topMargin" ? padT : mBox.top - secBox.top;
  const hAvail = (hPage ? secBox.width : secBox.width - padL - padR) || 0;
  const vAvail = (vPage ? secBox.height : secBox.height - padT - padB) || 0;
  const boxW = art.alignW || art.wPx;
  const boxH = art.alignH || art.hPx;
  const shift = (align: string, avail: number, size: number) =>
    align === "center" ? (avail - size) / 2 : align === "right" || align === "bottom" ? avail - size : 0;
  return {
    left: hOrigin + shift(art.leftAlign, hAvail, boxW) + art.leftPx + art.dxPx,
    top: vOrigin + shift(art.topAlign, vAvail, boxH) + art.topPx + art.dyPx,
  };
}

/** 页面渲完后按锚点放回：浮动的按页面坐标摆，行内的接在锚点位置 */
export function applyArtwork(container: HTMLElement, arts: ArtShape[]): number {
  const nodes: SVGSVGElement[] = [];
  /** 一组里的多个形状共用同一个锚点：按 marker 计数，最后一个摆完才把锚点删掉 */
  const pending = new Map<string, number>();
  for (const a of arts) pending.set(a.marker, (pending.get(a.marker) ?? 0) + 1);
  const last = new Map<string, SVGSVGElement>();
  /** 挂进文档后才能实测字宽，断完行重画一张，位置样式照搬 */
  const reflow = (node: SVGSVGElement, art: ArtShape): SVGSVGElement => {
    const plan = wrapPlan(art, node);
    if (!plan) return node;
    const fresh = renderArt(art, plan);
    fresh.setAttribute("style", node.getAttribute("style") ?? fresh.getAttribute("style") ?? "");
    node.replaceWith(fresh);
    nodes[nodes.indexOf(node)] = fresh;
    return fresh;
  };
  for (const art of arts) {
    const marker = container.querySelector(`#${art.marker}`);
    const prev = last.get(art.marker);
    const anchor = marker ?? prev ?? null;
    let node = renderArt(art);
    nodes.push(node);
    last.set(art.marker, node);
    const isLast = (pending.get(art.marker) ?? 1) === 1;
    pending.set(art.marker, (pending.get(art.marker) ?? 1) - 1);
    if (!anchor) continue;

    if (!art.floating) {
      node.style.verticalAlign = "bottom";
      if (marker) marker.replaceWith(node);
      else if (prev) prev.after(node);
      last.set(art.marker, reflow(node, art));
      continue;
    }
    const section = anchor.closest("section.docx");
    if (!(section instanceof HTMLElement)) continue;
    const secBox = section.getBoundingClientRect();
    const mBox = anchor.getBoundingClientRect();
    const style = getComputedStyle(section);
    const padL = parseFloat(style.paddingLeft) || 0;
    const padT = parseFloat(style.paddingTop) || 0;
    const padR = parseFloat(style.paddingRight) || 0;
    const padB = parseFloat(style.paddingBottom) || 0;
    const { left, top } = place(art, secBox, mBox, padL, padT, padR, padB);
    node.style.position = "absolute";
    node.style.left = `${Math.round(left)}px`;
    node.style.top = `${Math.round(top)}px`;
    node.style.zIndex = "3";
    section.appendChild(node);
    node = reflow(node, art);
    last.set(art.marker, node);
    if (marker && isLast) marker.remove();
  }
  /** Word 的 fitshape：字比路径长就整体缩到刚好铺满 */
  for (const svg of nodes) {
    const text = svg.querySelector("text");
    const path = svg.querySelector("path");
    if (!(text instanceof SVGTextContentElement) || !(path instanceof SVGGeometryElement)) continue;
    const len = path.getTotalLength();
    const need = text.getComputedTextLength();
    if (need > len && need > 0) {
      const base = Number(text.getAttribute("font-size") || 24);
      text.setAttribute("font-size", String(Math.max(8, base * (len / need) * 0.98)));
    }
  }
  return nodes.length;
}
