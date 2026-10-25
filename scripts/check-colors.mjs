#!/usr/bin/env node
/** 第五道门：颜色只能有一个来源。
 *
 *  查两件事：
 *   1) token 层之外不许出现裸色值（#hex / rgb() / rgba()）—— 这条早就写在 src/README.md 里，
 *      只是一直没人拦，所以组件里能攒下二十个。
 *   2) CSS 和组件里引用的变量名，必须是派生层真的发出来的那些。写成 --color-text-muta 这种
 *      不会报错、只会静悄悄变成无色，这条就是拦这个的。
 *
 *  豁免的四个文件是「颜色的来源」，不是「颜色的抄写处」：color.ts 的算法常数、seeds.ts 的种子、
 *  derive.ts 的调色板、tokens.css 的兜底表。office/ 整个目录豁免，它读的是磁盘文档里的原始颜色。
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = process.argv[2] ?? "src";
const SOURCE_OF_TRUTH = [
  "styles/color.ts",
  "styles/seeds.ts",
  "styles/derive.ts",
  "styles/tokens.css",
];
const SKIP_DIRS = ["office", "types"];
const COLOR_RE = /#[0-9a-fA-F]{3,8}\b|\brgba?\(\s*[0-9]/;
const VAR_USE = /var\(\s*(--[A-Za-z0-9-]+)/g;
const VAR_DEF = /(--[A-Za-z0-9-]+)\s*:/g;
const DERIVE_EMIT = /"(--[A-Za-z0-9-]+)"/g;

function walk(dir, acc = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (SKIP_DIRS.some((s) => relative(ROOT, p).startsWith(s))) continue;
    const st = statSync(p);
    if (st.isDirectory()) walk(p, acc);
    else if (/\.(ts|tsx|css)$/.test(name)) acc.push(p);
  }
  return acc;
}

const files = walk(ROOT);
const rel = (p) => relative(ROOT, p).replace(/\\/g, "/");
const isSource = (p) => SOURCE_OF_TRUTH.includes(rel(p));

const problems = [];
const used = new Map();

for (const file of files) {
  const text = readFileSync(file, "utf8");
  text.split("\n").forEach((line, i) => {
    if (!isSource(file) && COLOR_RE.test(line)) {
      problems.push(`${rel(file)}:${i + 1} 裸色值 → ${line.trim().slice(0, 72)}`);
    }
    if (isSource(file)) return;
    for (const m of line.matchAll(VAR_USE)) {
      if (!used.has(m[1])) used.set(m[1], `${rel(file)}:${i + 1}`);
    }
  });
}

/** 允许的变量名 = 兜底表里定义的 + 派生层发出去的 */
const defined = new Set();
for (const p of SOURCE_OF_TRUTH) {
  const text = readFileSync(join(ROOT, p), "utf8");
  const re = p.endsWith(".css") ? VAR_DEF : DERIVE_EMIT;
  for (const m of text.matchAll(re)) defined.add(m[1]);
}

for (const [name, where] of used) {
  if (!defined.has(name)) problems.push(`${where} 引用了没人发的变量 ${name}`);
}

if (problems.length) {
  console.error("颜色守卫没过：\n" + problems.map((p) => "  " + p).join("\n"));
  console.error(`\n豁免名单：${SOURCE_OF_TRUTH.join(", ")}（它们才是颜色的来源）`);
  process.exit(1);
}
console.log(`颜色守卫：${files.length} 个文件、${used.size} 个被引用的变量，全部有来源。`);
