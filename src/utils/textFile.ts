/** 文本文件的读取与判定：扩展名只能给提示，最终得看内容——中文名、ANSI(GBK) 编码、
    没扩展名的 Makefile 都算常见，按白名单放行的做法会把这些一律挡在预览外面。 */

export const MAX_TEXT_BYTES = 1024 * 1024;

/** 一眼就不是文本的格式，读它们只是浪费一次磁盘 IO；判据仍是内容嗅探，所以宁可少列 */
export const BINARY_EXTENSIONS = new Set([
  "exe", "dll", "ocx", "sys", "drv", "msi", "scr", "cab", "lnk", "apk", "ipa", "so", "dylib",
  "class", "jar", "war", "pdb", "obj", "lib", "a", "o", "pyc", "pyd", "pyo", "swp",
  "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "zst", "tgz", "iso", "img", "vhd", "vmdk",
  "ttf", "otf", "woff", "woff2", "eot", "cur", "icns", "theme",
  "mp3", "mp4", "m4a", "aac", "flac", "ogg", "opus", "wav", "wma", "amr",
  "mpg", "mpeg", "avi", "mkv", "mov", "wmv", "flv", "webm", "3gp",
  "docm", "xlsb", "xps", "epub", "mobi", "azw3", "cbz", "swf", "fla", "blend", "psd", "sketch",
]);

/** 前 8 KB 里有 NUL，或者控制字符密得过半，就认成二进制（UTF-16 靠 BOM 先走另一条路） */
function looksBinary(bytes: Uint8Array): boolean {
  const n = Math.min(bytes.length, 8192);
  let controls = 0;
  for (let i = 0; i < n; i++) {
    const c = bytes[i];
    if (c === 0) return true;
    if (c < 9 || (c > 13 && c < 32)) controls++;
  }
  return controls * 40 > n;
}

export interface DecodedText {
  text: string;
  /** 解码用的编码名，非 UTF-8 时提示一下，免得以为字面就是乱码 */
  encoding: string;
}

export function decodeText(bytes: Uint8Array): DecodedText | null {
  if (bytes.byteLength === 0) return { text: "", encoding: "utf-8" };
  const [a, b] = bytes;
  if (a === 0xff && b === 0xfe) return { text: new TextDecoder("utf-16le").decode(bytes.subarray(2)), encoding: "utf-16" };
  if (a === 0xfe && b === 0xff) return { text: new TextDecoder("utf-16be").decode(bytes.subarray(2)), encoding: "utf-16" };
  const bom = a === 0xef && b === 0xbb && bytes[2] === 0xbf;
  const body = bom ? bytes.subarray(3) : bytes;
  if (looksBinary(body)) return null;
  try {
    return { text: new TextDecoder("utf-8", { fatal: true }).decode(body), encoding: "utf-8" };
  } catch {
    /** Windows 中文机上「ANSI」的 txt/csv/log 基本都是 GBK；gb18030 是它的超集 */
    return { text: new TextDecoder("gb18030").decode(body), encoding: "gbk" };
  }
}

/** 扩展名 → highlight.js 的语言 id（只对打包进来的 common 那批，认不出就不上色） */
const LANGUAGES: Record<string, string> = {
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript", vue: "html",
  ts: "typescript", tsx: "typescript",
  py: "python", rs: "rust", go: "go", java: "java", kt: "kotlin", kts: "kotlin",
  c: "c", h: "c", cpp: "cpp", cc: "cpp", cxx: "cpp", hpp: "cpp", hh: "cpp",
  cs: "csharp", rb: "ruby", php: "php", pl: "perl", lua: "lua", swift: "swift",
  m: "objectivec", mm: "objectivec", scala: "scala", groovy: "groovy", dart: "dart",
  sh: "bash", bash: "bash", zsh: "bash", profile: "bash", bat: "dos", cmd: "dos", ps1: "powershell",
  sql: "sql", plsql: "sql",
  json: "json", jsonc: "json", json5: "json", ndjson: "json",
  yml: "yaml", yaml: "yaml", toml: "ini", ini: "ini", conf: "ini", cfg: "ini",
  properties: "properties", env: "ini", gitignore: "ini", editorconfig: "ini",
  xml: "xml", xsd: "xml", svg: "xml", xaml: "xml", fxml: "xml", pom: "xml",
  html: "html", htm: "html", shtml: "html", txt: "plaintext", log: "plaintext",
  md: "markdown", diff: "diff", patch: "diff", makefile: "makefile", cmake: "cmake",
  graphql: "graphql", gql: "graphql", wasm: "wasm", vb: "vbnet", bas: "vbnet",
  r: "r", sass: "scss", less: "less", styl: "stylus", csv: "plaintext",
};

const NAME_LANGUAGES: Record<string, string> = {
  makefile: "makefile", gnumakefile: "makefile", dockerfile: "dockerfile", cmakelists: "cmake",
};

export function languageOf(name: string): string | null {
  const lower = name.toLowerCase();
  const dot = lower.lastIndexOf(".");
  const bare = dot < 0 ? lower : lower.slice(0, dot);
  /** .gitignore / .eslintrc 这类点开头文件，整名就是它的类型 */
  if (dot === 0) return LANGUAGES[lower.slice(1)] ?? null;
  const byName = NAME_LANGUAGES[bare];
  if (byName) return byName;
  return LANGUAGES[dot < 0 ? lower : lower.slice(dot + 1)] ?? null;
}

/** asset 协议会把整个文件推过来，读流读到上限就停，避免手滑点开一个几百 MB 的日志 */
export async function readCapped(url: string, limit = MAX_TEXT_BYTES): Promise<{ bytes: Uint8Array; truncated: boolean }> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`读取失败：HTTP ${res.status}`);
  if (!res.body) {
    const buf = await res.arrayBuffer();
    return { bytes: new Uint8Array(buf, 0, Math.min(buf.byteLength, limit)), truncated: buf.byteLength > limit };
  }
  const reader = res.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  let truncated = false;
  while (total < limit) {
    const { done, value } = await reader.read();
    if (done) break;
    if (!value) continue;
    const room = limit - total;
    if (value.byteLength > room) {
      chunks.push(value.subarray(0, room));
      total += room;
      truncated = true;
      break;
    }
    chunks.push(value);
    total += value.byteLength;
  }
  if (truncated) await reader.cancel().catch(() => undefined);
  const out = new Uint8Array(total);
  let at = 0;
  for (const c of chunks) {
    out.set(c, at);
    at += c.byteLength;
  }
  return { bytes: out, truncated };
}
