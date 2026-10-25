/** 颜色数学。派生层只依赖这里，所以它能单独推理、单独验证。
 *  这个文件和 seeds.ts 允许出现裸色值：它们是颜色的来源，不是颜色的抄写处。 */

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

const clamp = (v: number) => Math.max(0, Math.min(255, Math.round(v)));

export function parseHex(input: string): Rgb {
  const raw = input.replace("#", "").slice(0, 6);
  const h = raw.length === 3 ? raw.split("").map((c) => c + c).join("") : raw.padEnd(6, "0");
  const n = parseInt(h, 16);
  return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
}

export function toHex({ r, g, b }: Rgb): string {
  const p = (v: number) => clamp(v).toString(16).padStart(2, "0");
  return `#${p(r)}${p(g)}${p(b)}`;
}

/** a 占 weight，b 占 1-weight。0 就是纯 b，1 就是纯 a */
export function mix(a: string, b: string, weight: number): string {
  const x = parseHex(a);
  const y = parseHex(b);
  const w = Math.max(0, Math.min(1, weight));
  return toHex({ r: x.r * w + y.r * (1 - w), g: x.g * w + y.g * (1 - w), b: x.b * w + y.b * (1 - w) });
}

export function withAlpha(hex: string, alpha: number): string {
  const { r, g, b } = parseHex(hex);
  return `rgba(${r}, ${g}, ${b}, ${Math.round(Math.max(0, Math.min(1, alpha)) * 1000) / 1000})`;
}

/** sRGB 相对亮度（WCAG 定义那套） */
export function luminance(hex: string): number {
  const { r, g, b } = parseHex(hex);
  const ch = [r, g, b].map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2];
}

export function contrast(a: string, b: string): number {
  const x = luminance(a);
  const y = luminance(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

/** 前景压在 bg 上读不清时，往黑或白一侧一档一档推到达标。
 *  这一步是静态 color-mix 表达不了的循环，也正是「用户随便挑一个亮主色也不会瞎」的关键。 */
export function readable(fg: string, bg: string, target: number): string {
  if (contrast(fg, bg) >= target) return fg;
  const towardDark = luminance(bg) > 0.35;
  const stop = towardDark ? "#000000" : "#ffffff";
  let out = fg;
  for (let i = 0; i < 22 && contrast(out, bg) < target; i++) {
    out = mix(out, stop, 0.94 - i * 0.042);
  }
  return contrast(out, bg) >= target ? out : stop;
}

/** 铺在某个底色上的字该用白还是用近黑。
 *  不是「谁对比高选谁」：琥珀、朱红这类中低明度的饱和色，黑的对比常常只比白高一点点，
 *  选出来却是近黑的字压在亮色上，发闷不和谐。约定是白优先，只有底色真的发亮（薄荷、柠檬黄、天蓝）
 *  才换成深字 —— 那时候深字本来也是压倒性的赢家。 */
export function pickForeground(bg: string): string {
  const white = contrast(bg, "#ffffff");
  if (white >= 3) return "#ffffff";
  return white * 1.25 >= contrast(bg, "#0b0b0b") ? "#ffffff" : "#0b0b0b";
}
