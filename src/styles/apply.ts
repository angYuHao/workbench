/** 派生结果落地：写进文档，并留一份「上次算好的变量表」给首帧前的脚本重放。
 *  重放脚本只做 JSON.parse + setProperty，不再算一遍颜色，所以两边不会有两套口径。 */

import { deriveTokens, type Tokens } from "./derive";
import { systemMode, type Mode, type StyleSeed } from "./seeds";

export const CACHE_KEY = "appearance.cache.v1";

export interface AppearanceCache {
  styleId: string;
  mode: "light" | "dark";
  tokens: Tokens;
}

export function resolveMode(mode: Mode): "light" | "dark" {
  return mode === "auto" ? systemMode() : mode;
}

function writeTokens(root: HTMLElement, tokens: Tokens) {
  for (const [name, value] of Object.entries(tokens)) root.style.setProperty(name, value);
}

export function applySeed(seed: StyleSeed, styleId: string): Tokens {
  const mode = resolveMode(seed.mode);
  const tokens = deriveTokens({ ...seed, mode });
  const root = document.documentElement;
  writeTokens(root, tokens);
  root.dataset.mode = mode;
  root.dataset.style = styleId;
  saveCache({ styleId, mode, tokens });
  return tokens;
}

export function saveCache(cache: AppearanceCache) {
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify(cache));
  } catch {
    /** 镜像写不进去只是会闪一下，不该把界面拖垮 */
  }
}

/** 首帧前脚本和界面启动共用这一份读法；坏 JSON 一律当没有，回落到 tokens.css 的兜底值 */
export function readCache(): AppearanceCache | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as AppearanceCache;
    if (!parsed || typeof parsed.tokens !== "object" || !parsed.mode) return null;
    return parsed;
  } catch {
    return null;
  }
}

export function replayCache(): boolean {
  const cache = readCache();
  if (!cache) return false;
  const root = document.documentElement;
  writeTokens(root, cache.tokens);
  root.dataset.mode = cache.mode;
  root.dataset.style = cache.styleId;
  return true;
}

export function watchSystemMode(onChange: () => void): () => void {
  if (typeof matchMedia !== "function") return () => {};
  const mq = matchMedia("(prefers-color-scheme: dark)");
  mq.addEventListener("change", onChange);
  return () => mq.removeEventListener("change", onChange);
}
