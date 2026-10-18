/** 零依赖读 OOXML（.docx/.xlsx 都是 zip 里装一堆 XML）。
    不装任何包：解压用浏览器原生 DecompressionStream，解析用原生 DOMParser。 */

export interface ZipFile {
  names: string[];
  read(name: string): Promise<Uint8Array>;
}

const EOCD_SIG = 0x06054b50;
const CD_SIG = 0x02014b50;
const LOCAL_SIG = 0x04034b50;

function u16(view: DataView, off: number): number {
  return view.getUint16(off, true);
}

function u32(view: DataView, off: number): number {
  return view.getUint32(off, true);
}

async function inflateRaw(data: Uint8Array): Promise<Uint8Array> {
  /** slice() 复制出一份独立 ArrayBuffer，类型上才能当 BlobPart */
  const chunk = data.slice().buffer;
  const stream = new Blob([chunk]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

export async function openZip(buffer: ArrayBuffer): Promise<ZipFile> {
  const bytes = new Uint8Array(buffer);
  const view = new DataView(buffer);
  if (bytes.length < 22) throw new Error("文件太小，不是有效的 zip");

  let eocd = -1;
  const scanFrom = Math.max(0, bytes.length - 66_000);
  for (let i = bytes.length - 22; i >= scanFrom; i--) {
    if (u32(view, i) === EOCD_SIG) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new Error("没找到 zip 结尾记录（可能是 zip64 或文件已损坏）");

  const count = u16(view, eocd + 10);
  let p = u32(view, eocd + 16);
  const entries = new Map<string, { method: number; offset: number; size: number }>();
  const names: string[] = [];

  for (let i = 0; i < count; i++) {
    if (u32(view, p) !== CD_SIG) break;
    const method = u16(view, p + 10);
    const compSize = u32(view, p + 20);
    const nameLen = u16(view, p + 28);
    const extraLen = u16(view, p + 30);
    const commentLen = u32(view, p + 32) & 0xffff;
    const localOffset = u32(view, p + 42);
    const name = new TextDecoder().decode(bytes.subarray(p + 46, p + 46 + nameLen));
    entries.set(name, { method, offset: localOffset, size: compSize });
    names.push(name);
    p += 46 + nameLen + extraLen + commentLen;
  }

  const read = async (name: string): Promise<Uint8Array> => {
    const entry = entries.get(name);
    if (!entry) throw new Error(`压缩包里找不到 ${name}`);
    const lh = entry.offset;
    if (u32(view, lh) !== LOCAL_SIG) throw new Error(`${name} 的本地文件头损坏`);
    const dataStart = lh + 30 + u16(view, lh + 26) + u16(view, lh + 28);
    const raw = bytes.subarray(dataStart, dataStart + entry.size);
    if (entry.method === 0) return raw;
    if (entry.method === 8) return inflateRaw(raw);
    throw new Error(`${name} 用了不支持的压缩方式 ${entry.method}`);
  };

  return { names, read };
}

const xmlDoc = (bytes: Uint8Array) => {
  const text = new TextDecoder("utf-8").decode(bytes);
  const doc = new DOMParser().parseFromString(text, "application/xml");
  if (doc.getElementsByTagName("parsererror").length) throw new Error("XML 解析失败");
  return doc;
};

/** OOXML 的元素都带命名空间前缀，按 localName 取才不受前缀影响 */
function children(el: Element, local: string): Element[] {
  return [...el.children].filter((c) => c.localName === local);
}

/** 同理，docx 的属性是 w:val 这种带前缀的写法，getAttribute("val") 取不到 */
function attr(el: Element, local: string): string | null {
  for (const a of el.attributes) {
    if (a.localName === local) return a.value;
  }
  return null;
}

function allText(el: Element): string {
  return [...el.querySelectorAll("*")]
    .filter((n) => n.localName === "t" || n.localName === "delText")
    .map((n) => n.textContent ?? "")
    .join("");
}

/* ------------------------------ Excel：数据结构 ------------------------------ */

export interface SheetCellView {
  text: string;
  bold: boolean;
  italic: boolean;
  align: "left" | "center" | "right" | null;
  colspan: number;
  rowspan: number;
}

export interface SheetRowView {
  /** Excel 里的真实行号；中间整行空的就不渲染，行号照旧往下跳 */
  n: number;
  height: number | null;
  /** 下标即列序；null 表示被左上角的合并单元格吃掉，不渲染 */
  cells: (SheetCellView | null)[];
}

export interface SheetTable {
  name: string;
  colCount: number;
  colWidths: number[];
  rows: SheetRowView[];
  truncated: boolean;
}

export const MAX_ROWS = 300;
export const MAX_COLS = 40;
export const DEFAULT_COL_PX = 64;
export const MIN_COL_PX = 30;
export const MAX_COL_PX = 420;

function colIndex(ref: string): number {
  let n = 0;
  for (const ch of ref) {
    if (ch >= "A" && ch <= "Z") n = n * 26 + (ch.charCodeAt(0) - 64);
    else if (ch >= "a" && ch <= "z") n = n * 26 + (ch.charCodeAt(0) - 96);
    else break;
  }
  return Math.max(0, n - 1);
}

/** 0 起算的列号 → A / B / … / AA */
export function colLabel(index: number): string {
  let n = index + 1;
  let s = "";
  while (n > 0) {
    const rem = (n - 1) % 26;
    s = String.fromCharCode(65 + rem) + s;
    n = Math.floor((n - 1) / 26);
  }
  return s;
}

const splitRef = (ref: string): { col: number; row: number } | null => {
  const m = /^([A-Za-z]+)(\d+)$/.exec(ref);
  return m ? { col: colIndex(m[1]), row: Number(m[2]) } : null;
};

/* ---------- 数字与日期：按 styles.xml 的 formatCode 还原成 Excel 里看到的样子 ---------- */

export interface CellStyle {
  fmt: string;
  bold: boolean;
  italic: boolean;
  align: "left" | "center" | "right" | null;
}

export const BUILTIN_FMT: Record<number, string> = {
  0: "General",
  1: "0",
  2: "0.00",
  3: "#,##0",
  4: "#,##0.00",
  9: "0%",
  10: "0.00%",
  11: "0.00E+00",
  14: "m/d/yyyy",
  15: "d-mmm",
  16: "mmm-d",
  17: "mmm-yy",
  18: "h:mm AM/PM",
  19: "h:mm:ss AM/PM",
  20: "h:mm",
  21: "h:mm:ss",
  22: "m/d/yy h:mm",
  27: "yyyy年m月",
  28: "m月d日",
  29: "yyyy年m月d日",
  30: "m/d/yy",
  31: "yyyy年m月d日",
  32: "h时m分",
  33: "h时m分s秒",
  34: "m分s秒",
  35: "h:mm:ss",
  37: "#,##0;(#,##0)",
  38: "#,##0;[Red](#,##0)",
  39: "#,##0.00;(#,##0.00)",
  40: "#,##0.00;[Red](#,##0.00)",
  45: "mm:ss",
  46: "h:mm:ss",
  47: "mm:ss.0",
};

/** 序列号 → 年月日；fDate1904 决定基准日（.xls 里可能开了 1904 日期系统） */
interface DateParts {
  y: number;
  mo: number;
  d: number;
  h: number;
  mi: number;
  s: number;
  wd: number;
}

function serialToParts(serial: number, date1904: boolean): DateParts | null {
  if (!Number.isFinite(serial) || serial < 0) return null;
  const base = date1904 ? Date.UTC(1904, 0, 0) : Date.UTC(1899, 11, serial >= 61 ? 30 : 31);
  const days = Math.floor(serial);
  if (!date1904 && (serial < 1 || serial > 2_958_465)) return null;
  const dt = new Date(base + days * 86_400_000 + Math.round((serial - days) * 86_400_000));
  if (Number.isNaN(dt.getTime())) return null;
  return {
    y: dt.getUTCFullYear(),
    mo: dt.getUTCMonth() + 1,
    d: dt.getUTCDate(),
    h: dt.getUTCHours(),
    mi: dt.getUTCMinutes(),
    s: dt.getUTCSeconds(),
    wd: dt.getUTCDay(),
  };
}

const MONTH_SHORT = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const MONTH_LONG = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];
const WEEK_SHORT = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

function renderPattern(code: string, p: DateParts): string {
  const bare = code.replace(/"[^"]*"/g, "");
  const dateish = /[yd]/i.test(bare);
  const clock12 = /am\/pm|a\/p/i.test(bare);
  let out = "";
  let sawHour = false;

  for (let i = 0; i < code.length; ) {
    const ch = code[i];
    if (ch === '"') {
      const j = code.indexOf('"', i + 1);
      if (j < 0) break;
      out += code.slice(i + 1, j);
      i = j + 1;
      continue;
    }
    if (ch === "\\") {
      out += code[i + 1] ?? "";
      i += 2;
      continue;
    }
    /** [Red]/[$-804] 是颜色与区域修饰，_ 和 * 是占位，渲染时整段丢掉 */
    if (ch === "[") {
      const j = code.indexOf("]", i);
      i = j < 0 ? code.length : j + 1;
      continue;
    }
    if (ch === "_" || ch === "*") {
      i += 2;
      continue;
    }
    const low = ch.toLowerCase();
    if (low === "a") {
      let j = i;
      while (j < code.length && code[j].toLowerCase() === "a") j++;
      const run = code.slice(i, j).toLowerCase();
      out += run === "am/pm" || run === "a/p" ? (p.h < 12 ? "AM" : "PM") : "";
      i = j;
      continue;
    }
    if (!"ymdhs".includes(low)) {
      out += ch;
      i++;
      continue;
    }
    let j = i;
    while (j < code.length && code[j].toLowerCase() === low) j++;
    const n = j - i;
    const pad = (v: number) => String(v).padStart(n >= 2 ? 2 : 0, "0");

    if (low === "y") out += n >= 4 ? String(p.y) : String(p.y).slice(-2);
    else if (low === "d") out += n >= 3 ? WEEK_SHORT[p.wd] : pad(p.d);
    else if (low === "h") {
      sawHour = true;
      out += pad(clock12 ? p.h % 12 || 12 : p.h);
    } else if (low === "s") out += pad(p.s);
    else {
      /** m 既可能是月份也可能是分钟：出现在 h 之后、或整串不含 y/d 的按分钟算 */
      const minute = sawHour || !dateish;
      if (minute) out += pad(p.mi);
      else if (n >= 4) out += MONTH_LONG[p.mo - 1];
      else if (n === 3) out += MONTH_SHORT[p.mo - 1];
      else out += pad(p.mo);
    }
    i = j;
  }
  return out;
}

const cleanNumber = (n: number): string => String(parseFloat(n.toPrecision(15)));

/** 格式码用 ; 分段（正;负;零;文本），引号里的 ; 不算分隔符 */
function splitSections(code: string): string[] {
  const out: string[] = [];
  let cur = "";
  let quoted = false;
  for (let i = 0; i < code.length; i++) {
    const ch = code[i];
    if (ch === "\\") {
      cur += ch + (code[i + 1] ?? "");
      i++;
      continue;
    }
    if (ch === '"') quoted = !quoted;
    if (ch === ";" && !quoted) {
      out.push(cur);
      cur = "";
      continue;
    }
    cur += ch;
  }
  out.push(cur);
  return out;
}

const quotedLiteral = (section: string): string | null => {
  const m = /"([^"]*)"/.exec(section);
  return m ? m[1] : null;
};

const decimalsOf = (section: string): number => {
  const dot = section.indexOf(".");
  if (dot < 0) return 0;
  let k = dot + 1;
  while (k < section.length && (section[k] === "0" || section[k] === "?")) k++;
  return k - dot - 1;
};

function renderNumber(raw: string, code: string): string {
  const n = Number(raw);
  if (!Number.isFinite(n)) return raw;
  if (!code || /^general$/i.test(code)) return cleanNumber(n);

  const sections = splitSections(code);
  const section =
    n === 0 && sections.length >= 3 ? sections[2] : n < 0 ? sections[1] ?? sections[0] : sections[0];
  if (/^\@$/.test(section)) return raw;

  const symbol = quotedLiteral(section);
  /** 会计格式里 `_ * "-"_ ` 这种只带字面量的段：0 就显示那个字面量（一串横线） */
  if (n === 0 && symbol && !/[#0?]/.test(section.replace(/"[^"]*"/g, ""))) return symbol;

  const abs = Math.abs(n);
  let text: string;
  if (/E/i.test(section)) {
    text = abs.toExponential(decimalsOf(section.split(/E/i)[0]) || 2).toUpperCase().replace("+", "");
  } else {
    text = abs.toFixed(decimalsOf(section));
    if (/,##/.test(section) || /,0/.test(section.split(".")[0])) {
      const [ip, fp] = text.split(".");
      text = ip.replace(/\B(?=(\d{3})+(?!\d))/g, ",") + (fp ? "." + fp : "");
    }
  }
  if (/%/.test(section)) text += "%";
  if (symbol && symbol !== "-") text = symbol + text;
  if (n < 0) text = /\(/.test(section) ? `(${text})` : `-${text}`;
  return text;
}

const dateTokensRe = /[ymdhs]/i;

export function renderValue(raw: string, style: CellStyle | undefined, date1904: boolean): { text: string; numeric: boolean } {
  const fmt = style?.fmt ?? "General";
  /** 日期格式码常写成 `yyyy-mm-dd;@`，只有第一段管数字，后面是给文本用的 */
  const numericPart = splitSections(fmt)[0];
  const parts = serialToParts(Number(raw), date1904);
  if (parts !== null && numericPart !== "General" && dateTokensRe.test(numericPart.replace(/"[^"]*"/g, ""))) {
    return { text: renderPattern(numericPart, parts), numeric: false };
  }
  return { text: renderNumber(raw, fmt), numeric: true };
}

export const blankCell = (): SheetCellView => ({
  text: "",
  bold: false,
  italic: false,
  align: null,
  colspan: 1,
  rowspan: 1,
});

/** 拼一行网格数据：合并单元格里非左上角的置 null，列宽按 xlsx 的 <cols> 记录换算 */
export function assembleSheet(input: {
  name: string;
  cells: Map<number, Map<number, SheetCellView>>;
  heights?: Map<number, number>;
  merges?: { r1: number; c1: number; r2: number; c2: number }[];
  colWidths?: Map<number, number>;
  truncated: boolean;
}): SheetTable {
  const nums = [...input.cells.keys()].sort((a, b) => a - b);
  let colCount = 0;
  for (const cells of input.cells.values()) {
    for (const c of cells.keys()) colCount = Math.max(colCount, c + 1);
  }
  for (const m of input.merges ?? []) colCount = Math.max(colCount, Math.min(m.c2 + 1, MAX_COLS));
  colCount = Math.min(colCount, MAX_COLS);

  const rows: SheetRowView[] = nums.map((n) => {
    const cells: (SheetCellView | null)[] = Array.from({ length: colCount }, blankCell);
    input.cells.get(n)!.forEach((view, c) => {
      if (c < colCount) cells[c] = view;
    });
    return { n, height: input.heights?.get(n) ?? null, cells };
  });

  for (const m of input.merges ?? []) {
    const inRange = rows.filter((r) => r.n >= m.r1 && r.n <= m.r2);
    const anchor = inRange.find((r) => r.n === m.r1)?.cells[m.c1];
    if (!anchor) continue;
    anchor.colspan = Math.min(m.c2 - m.c1 + 1, colCount - m.c1);
    anchor.rowspan = Math.max(1, inRange.length);
    for (const r of inRange) {
      /** 左上角留着自己占位，其余被覆盖的格子要整个删掉：上面这行若不从 c1+1 开始删，合并格右侧就会多出一串空格子 */
      const from = r.n === m.r1 ? m.c1 + 1 : m.c1;
      for (let c = from; c <= Math.min(m.c2, colCount - 1); c++) r.cells[c] = null;
    }
  }

  const widths = Array.from({ length: colCount }, (_, i) => input.colWidths?.get(i + 1) ?? DEFAULT_COL_PX);
  return { name: input.name, colCount, colWidths: widths, rows, truncated: input.truncated };
}

/* ------------------------------ xlsx ------------------------------ */

const flagOn = (el: Element, local: string): boolean => {
  const node = children(el, local)[0];
  if (!node) return false;
  const v = attr(node, "val");
  return v === null || (v !== "0" && v !== "false");
};

function parseStyles(doc: Document): CellStyle[] {
  const root = doc.documentElement;
  const custom = new Map<number, string>();
  const numFmts = children(root, "numFmts")[0];
  for (const nf of numFmts ? children(numFmts, "numFmt") : []) {
    const id = Number(attr(nf, "numFmtId"));
    const code = attr(nf, "formatCode");
    if (id && code) custom.set(id, code);
  }

  const fontsEl = children(root, "fonts")[0];
  const fonts = (fontsEl ? children(fontsEl, "font") : []).map((f) => ({
    bold: flagOn(f, "b"),
    italic: flagOn(f, "i"),
  }));

  const xfsEl = children(root, "cellXfs")[0];
  return (xfsEl ? children(xfsEl, "xf") : []).map((xf) => {
    const id = Number(attr(xf, "numFmtId") ?? "0");
    const fontId = Number(attr(xf, "fontId") ?? "0");
    const align = children(xf, "alignment")[0];
    const h = align ? attr(align, "horizontal") : null;
    return {
      fmt: custom.get(id) ?? BUILTIN_FMT[id] ?? "General",
      bold: fonts[fontId]?.bold ?? false,
      italic: fonts[fontId]?.italic ?? false,
      align: h === "left" || h === "center" || h === "right" ? h : null,
    };
  });
}

function cellView(cell: Element, shared: string[], styles: CellStyle[], date1904: boolean): SheetCellView {
  const type = attr(cell, "t") ?? "n";
  const style = styles[Number(attr(cell, "s") ?? "0")];
  const base: SheetCellView = {
    ...blankCell(),
    bold: style?.bold ?? false,
    italic: style?.italic ?? false,
    align: style?.align ?? null,
  };
  const v = children(cell, "v")[0]?.textContent ?? "";
  if (type === "s") return { ...base, text: shared[Number(v)] ?? "" };
  if (type === "inlineStr") return { ...base, text: allText(cell) };
  if (type === "b") return { ...base, text: v === "1" ? "TRUE" : "FALSE", align: base.align ?? "center" };
  if (type === "e" || type === "str") return { ...base, text: v };
  if (!v) return base;
  const { text, numeric } = renderValue(v, style, date1904);
  return { ...base, text, align: base.align ?? (numeric ? "right" : null) };
}

function parseRange(ref: string) {
  const [a, b] = ref.split(":");
  const p1 = a ? splitRef(a) : null;
  const p2 = b ? splitRef(b) : null;
  return p1 && p2 ? { r1: p1.row, c1: p1.col, r2: p2.row, c2: p2.col } : null;
}

function buildSheet(doc: Document, shared: string[], styles: CellStyle[], date1904: boolean) {
  const root = doc.documentElement;
  const data = children(root, "sheetData")[0];
  const cells = new Map<number, Map<number, SheetCellView>>();
  const heights = new Map<number, number>();
  let truncated = false;
  let prevRow = 0;

  for (const row of data ? children(data, "row") : []) {
    if (cells.size >= MAX_ROWS) {
      truncated = true;
      break;
    }
    const n = Number(attr(row, "r")) || prevRow + 1;
    prevRow = n;
    if (attr(row, "customHeight") === "1") {
      const ht = Number(attr(row, "ht"));
      if (ht) heights.set(n, Math.round(ht * 1.333));
    }
    const byCol = new Map<number, SheetCellView>();
    let run = -1;
    for (const cell of children(row, "c")) {
      const ref = attr(cell, "r");
      const at = ref ? splitRef(ref) : null;
      const idx = at ? at.col : run + 1;
      run = idx;
      if (idx >= MAX_COLS) {
        truncated = true;
        continue;
      }
      byCol.set(idx, cellView(cell, shared, styles, date1904));
    }
    cells.set(n, byCol);
  }

  const merges: { r1: number; c1: number; r2: number; c2: number }[] = [];
  const mergeSec = children(root, "mergeCells")[0];
  for (const m of mergeSec ? children(mergeSec, "mergeCell") : []) {
    const range = parseRange(attr(m, "ref") ?? "");
    if (range) merges.push(range);
  }

  /** <cols> 的宽度单位是「字符」，1 字符约 7px，再加 5px 边距 */
  const colWidths = new Map<number, number>();
  const colsSec = children(root, "cols")[0];
  for (const col of colsSec ? children(colsSec, "col") : []) {
    const min = Number(attr(col, "min"));
    const max = Number(attr(col, "max")) || min;
    const w = Number(attr(col, "width"));
    if (!min || !w) continue;
    const px = Math.min(MAX_COL_PX, Math.max(MIN_COL_PX, Math.round(w * 7) + 5));
    for (let i = min; i <= max; i++) colWidths.set(i, px);
  }

  return { cells, heights, merges, colWidths, truncated };
}

export async function readWorkbook(buffer: ArrayBuffer): Promise<SheetTable[]> {
  const zip = await openZip(buffer);
  const want = (n: string) => zip.read(n).then(xmlDoc);

  const shared = zip.names.includes("xl/sharedStrings.xml")
    ? Array.from((await want("xl/sharedStrings.xml")).getElementsByTagName("si"), allText)
    : [];
  const styles = zip.names.includes("xl/styles.xml") ? parseStyles(await want("xl/styles.xml")) : [];

  const workbook = await want("xl/workbook.xml");
  /** 1904 日期系统是 workbookPr 上的属性，不是子元素 */
  const wbPr = children(workbook.documentElement, "workbookPr")[0];
  const flag = wbPr ? attr(wbPr, "date1904") : null;
  const date1904 = flag === "1" || flag === "true";
  const rels = zip.names.includes("xl/_rels/workbook.xml.rels") ? await want("xl/_rels/workbook.xml.rels") : null;
  const relTarget = new Map<string, string>();
  if (rels) {
    for (const rel of [...rels.children].filter((c) => c.localName === "Relationship")) {
      const id = rel.getAttribute("Id");
      const target = rel.getAttribute("Target");
      if (id && target) relTarget.set(id, target.replace(/^\/?xl\//, "").replace(/^\//, ""));
    }
  }

  const sheets: SheetTable[] = [];
  const sheetEls = children(children(workbook.documentElement, "sheets")[0] ?? workbook.documentElement, "sheet");
  for (const sheet of sheetEls) {
    const name = attr(sheet, "name") ?? `工作表${sheets.length + 1}`;
    const rid =
      sheet.getAttributeNS("http://schemas.openxmlformats.org/officeDocument/2006/relationships", "id") ??
      attr(sheet, "id");
    const target = rid ? relTarget.get(rid) : undefined;
    const path = target ? `xl/${target}` : `xl/worksheets/sheet${sheets.length + 1}.xml`;
    if (!zip.names.includes(path)) continue;
    const built = buildSheet(await want(path), shared, styles, date1904);
    sheets.push(assembleSheet({ name, ...built }));
  }
  return sheets;
}

/* ------------------------------- Word -------------------------------- */

/** 只有旧版 .doc（OLE 复合文档）走自研抽取，docx 交给 docx-preview 原样排版 */
export type DocBlock =
  | { type: "heading"; level: number; text: string }
  | { type: "para"; text: string }
  | { type: "bullet"; text: string }
  | { type: "table"; rows: string[][] };

/** OOXML 文件都是 zip，头两个字节固定是 PK */
export function looksLikeZip(bytes: Uint8Array): boolean {
  return bytes[0] === 0x50 && bytes[1] === 0x4b;
}
