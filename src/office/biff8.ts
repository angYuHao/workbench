/** 旧版 .xls：OLE 复合文档（CFB）里装着 BIFF8 记录流。
    零依赖——自己解 CFB 的扇区链和目录，自己读 BIFF 记录。
    产出的中间结构和 xlsx 那条路完全一样，所以列标、列宽、合并、日期数字格式的渲染都能复用。 */

import {
  BUILTIN_FMT,
  MAX_COL_PX,
  MAX_COLS,
  MAX_ROWS,
  MIN_COL_PX,
  assembleSheet,
  blankCell,
  renderValue,
  type CellStyle,
  type SheetTable,
  type SheetCellView,
} from "./ooxml";

const CFB_MAGIC = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];

export function isCompoundFile(bytes: Uint8Array): boolean {
  return CFB_MAGIC.every((b, i) => bytes[i] === b);
}

/* ------------------------------ CFB ------------------------------ */

interface CfbEntry {
  name: string;
  start: number;
  size: number;
}

/** .xls 和 .doc 都住在同一个 CFB 容器里，所以目录读取单独导出 */
export function openCompoundFile(buffer: ArrayBuffer): (name: string) => Uint8Array | null {
  const bytes = new Uint8Array(buffer);
  const view = new DataView(buffer);
  const sectorSize = 1 << view.getUint16(30, true);
  const miniSize = 1 << view.getUint16(32, true);
  const miniCutoff = view.getUint32(56, true);
  const fatCount = view.getUint32(44, true);
  const dirStart = view.getInt32(48, true);
  const miniFatStart = view.getInt32(60, true);
  const difatStart = view.getInt32(68, true);
  const difatCount = view.getInt32(72, true);

  const sector = (id: number) => bytes.subarray((id + 1) * sectorSize, (id + 2) * sectorSize);
  const intsOf = (id: number): number[] => {
    const s = sector(id);
    const dv = new DataView(s.buffer, s.byteOffset, s.byteLength);
    const out: number[] = [];
    for (let i = 0; i + 4 <= s.length; i += 4) out.push(dv.getInt32(i, true));
    return out;
  };

  /** 前 109 个 FAT 扇区写在头部，再多就顺着 DIFAT 链找 */
  const fatIds: number[] = [];
  for (let i = 0; i < 109; i++) {
    const id = view.getInt32(76 + i * 4, true);
    if (id < 0) break;
    fatIds.push(id);
  }
  let difat = difatStart;
  for (let k = 0; k < difatCount && difat >= 0; k++) {
    const ids = intsOf(difat);
    for (let i = 0; i + 1 < ids.length; i++) {
      if (ids[i] < 0) break;
      fatIds.push(ids[i]);
    }
    difat = ids[ids.length - 1];
  }
  const fat: number[] = [];
  for (const id of (fatCount > 0 ? fatIds.slice(0, fatCount) : fatIds).filter((n) => n >= 0)) {
    fat.push(...intsOf(id));
  }

  /** 0xfffffffc 以上都是「结束/空闲/FAT 自身」标记，负数即停 */
  const walk = (start: number, next: number[], out: number[]) => {
    let id = start;
    for (let guard = 0; id >= 0 && id < next.length && guard <= next.length; guard++) {
      out.push(id);
      id = next[id];
    }
  };
  const chain = (start: number): number[] => {
    const out: number[] = [];
    walk(start, fat, out);
    return out;
  };

  const readChain = (start: number, size: number): Uint8Array => {
    const out = new Uint8Array(size);
    let filled = 0;
    for (const id of chain(start)) {
      if (filled >= size) break;
      const s = sector(id);
      const n = Math.min(s.length, size - filled);
      out.set(s.subarray(0, n), filled);
      filled += n;
    }
    return out;
  };

  const streams: CfbEntry[] = [];
  let root: CfbEntry | null = null;
  for (const id of chain(dirStart)) {
    const s = sector(id);
    for (let off = 0; off + 128 <= s.length; off += 128) {
      const base = (id + 1) * sectorSize + off;
      const nameLen = view.getUint16(base + 64, true);
      if (nameLen < 2 || nameLen > 64) continue;
      const name = new TextDecoder("utf-16le").decode(bytes.subarray(base, base + nameLen - 2));
      const type = bytes[base + 66];
      const entry = { name, start: view.getInt32(base + 116, true), size: view.getUint32(base + 120, true) };
      if (type === 5) root = entry;
      else if (type === 2) streams.push(entry);
    }
  }

  const miniFat: number[] = [];
  for (const id of chain(miniFatStart)) miniFat.push(...intsOf(id));
  /** 小于 4096 字节的流挤在 root 指向的「迷你流」里，按 miniSize 分块 */
  const miniStream = root ? readChain(root.start, root.size) : new Uint8Array(0);

  return (name: string) => {
    const hit = streams.find((s) => s.name.toLowerCase() === name.toLowerCase());
    if (!hit) return null;
    if (hit.size < miniCutoff && miniStream.length) {
      const out = new Uint8Array(hit.size);
      const ids: number[] = [];
      walk(hit.start, miniFat, ids);
      let filled = 0;
      for (const id of ids) {
        if (filled >= hit.size) break;
        const piece = miniStream.subarray(id * miniSize, (id + 1) * miniSize);
        const n = Math.min(piece.length, hit.size - filled);
        out.set(piece.subarray(0, n), filled);
        filled += n;
      }
      return out;
    }
    return readChain(hit.start, hit.size);
  };
}

/* ----------------------------- BIFF8 ------------------------------ */

const OP_ROW = 0x0003;
const OP_ROW8 = 0x0208;
const OP_EOF = 0x000a;
const OP_CONTINUE = 0x003c;
/** BIFF8 的自定义格式记录号是 041E（001E 是 BIFF3 时代的同号记录，这里不碰） */
const OP_FORMAT = 0x041e;
const OP_BLANK = 0x0201;
const OP_NUMBER = 0x0203;
const OP_BOOLERR = 0x0205;
const OP_STRING = 0x0207;
const OP_DATEMODE = 0x0022;
const OP_CODEPAGE = 0x0042;
const OP_FORMULA = 0x0006;
const OP_COLINFO = 0x007d;
const OP_BOUNDSHEET = 0x0085;
const OP_MULBLANK = 0x00be;
const OP_MULRK = 0x00bd;
const OP_FONT = 0x0031;
const OP_XF = 0x00e0;
const OP_SST = 0x00fc;
const OP_LABELSST = 0x00fd;
const OP_RK = 0x027e;
const OP_MERGEDCELLS = 0x00e5;

interface Rec {
  op: number;
  at: number;
  len: number;
}

/** 「压缩」写法下的高字节按代码页解释；中文 .xls 里出现的多半是 GBK */
const CP1252_HIGH = "€\uFFFD‚ƒ„…†‡ˆ‰Š‹Œ\uFFFDŽ\uFFFD\uFFFD‘’“”•–—˜™š›œ\uFFFDžŸ";

function decodeRun(run: number[], gbk: TextDecoder | null): string {
  if (!run.length) return "";
  if (gbk) return gbk.decode(Uint8Array.from(run));
  let s = "";
  for (const b of run) s += String.fromCharCode(b >= 0x80 ? CP1252_HIGH.charCodeAt(b - 0x80) || b : b);
  return s;
}

interface UniHead {
  cch: number;
  wide: boolean;
  next: number;
}

/** BIFF8 字符串头：cch(2) + flags(1)。实测（拿他的真实 .xls 逐条对齐）：位 0 = 1 才是 16 位字符，
    位 1/2/3（富文本、扩展、注音）在字符之前各多带 2 字节的计数字段，数据本体在字符之后 */
function uniHead(b: Uint8Array, at: number): UniHead {
  const cch = b[at] | (b[at + 1] << 8);
  const flags = b[at + 2];
  let p = at + 3;
  if (flags & 2) p += 2;
  if (flags & 4) p += 2;
  if (flags & 8) p += 2;
  return { cch, wide: (flags & 1) === 1, next: p };
}

function uniString(b: Uint8Array, at: number, gbk: TextDecoder | null): string {
  const { cch, wide, next } = uniHead(b, at);
  let p = next;
  if (!wide) {
    const run: number[] = [];
    for (let i = 0; i < cch; i++) run.push(b[p++]);
    return decodeRun(run, gbk);
  }
  let s = "";
  for (let i = 0; i < cch; i++, p += 2) s += String.fromCharCode(b[p] | (b[p + 1] << 8));
  return s;
}

/** 把 SST 及其后续 CONTINUE 拼成一整段，并记下每段的结尾，供跨段时重读 flags 字节 */
function flatten(chunks: Uint8Array[]): { all: Uint8Array; bounds: number[] } {
  const total = chunks.reduce((n, c) => n + c.length, 0);
  const all = new Uint8Array(total);
  const bounds: number[] = [];
  let off = 0;
  for (const c of chunks) {
    all.set(c, off);
    off += c.length;
    bounds.push(off);
  }
  return { all, bounds };
}

function readSst(
  chunks: Uint8Array[],
  gbk: TextDecoder | null,
  extMode: 0 | 1,
): { strings: string[]; consumed: number; total: number } {
  const { all, bounds } = flatten(chunks);
  const out: string[] = [];
  if (all.length < 8) return { strings: out, consumed: 0, total: all.length };
  const unique = (all[4] | (all[5] << 8) | (all[6] << 16)) + all[7] * 0x1000000;
  let p = 8;
  let bi = 0;

  /** 头部（长度、flags、扩展长度）不跨段；正文字符才按 CONTINUE 规则在段首重读 1 字节 flags */
  const inHead = (n: number): boolean => {
    if (p + n <= bounds[bi]) return true;
    bi++;
    if (bi >= bounds.length) return false;
    p = bounds[bi - 1];
    return p + n <= bounds[bi];
  };
  const hu8 = (): number => (inHead(1) ? all[p++] : 0);
  const hu16 = (): number => {
    if (!inHead(2)) return 0;
    const x = all[p] | (all[p + 1] << 8);
    p += 2;
    return x;
  };
  const skip = (n: number) => {
    while (n > 0 && bi < bounds.length) {
      const room = bounds[bi] - p;
      if (room >= n) {
        p += n;
        return;
      }
      n -= room;
      p = bounds[bi];
      bi++;
    }
  };

  for (let i = 0; i < unique; i++) {
    const cch = hu16();
    const flags = hu8();
    /** 扩展数据的位置：字符前给条数/字节数，字符读完再跳过对应的数据块 */
    let tail = 0;
    if (flags & 2) tail += hu16() * 4;
    if (flags & 4) {
      const cb = hu16();
      if (extMode === 1) hu16();
      tail += cb;
    }
    if (flags & 8) {
      const n = hu16();
      tail += extMode === 0 ? n * 4 : hu16();
    }
    let wide = (flags & 1) === 1;
    let remaining = cch;
    let s = "";
    let run: number[] = [];
    const flush = () => {
      if (run.length) {
        s += decodeRun(run, gbk);
        run = [];
      }
    };
    while (remaining > 0) {
      if (p >= bounds[bi]) {
        flush();
        bi++;
        if (bi >= bounds.length) break;
        p = bounds[bi - 1];
        wide = (all[p] & 1) === 1;
        p++;
        continue;
      }
      if (wide) {
        if (p + 2 > bounds[bi]) break;
        flush();
        s += String.fromCharCode(all[p] | (all[p + 1] << 8));
        p += 2;
      } else {
        run.push(all[p++]);
      }
      remaining--;
    }
    flush();
    /** 富文本/拼音段：字符之后还跟着 run 表和注音数据 */
    skip(tail);
    out.push(s);
  }
  return { strings: out, consumed: p, total: all.length };
}

/** RK 是压缩过的数：首字节 bit1=1 是「30 位有符号整数」，否则是双精度的高 32 位（低 30 位当作 0）；
    bit0=1 表示整个值还要再除以 100 */
const rkNumber = (b: Uint8Array, at: number): number => {
  const flags = b[at];
  const lo = (b[at] | (b[at + 1] << 8) | (b[at + 2] << 16)) + b[at + 3] * 0x1000000;
  let x: number;
  if (flags & 2) x = (lo | 0) >> 2;
  else {
    const buf = new DataView(new ArrayBuffer(8));
    buf.setUint32(4, lo & 0xfffffffc, true);
    x = buf.getFloat64(0, true);
  }
  return flags & 1 ? x / 100 : x;
};

const realNumber = (b: Uint8Array, at: number): number =>
  new DataView(b.buffer, b.byteOffset + at, 8).getFloat64(0, true);

/** BIFF 内置格式号和 xlsx 的 numFmtId 同源，缺的那些按「不认识的编号」当 General 处理 */
function fmtCode(id: number, custom: Map<number, string>): string {
  return custom.get(id) ?? BUILTIN_FMT[id] ?? "General";
}

/** 字体表里 4 号是保留空位（Excel 从不引用它），所以索引 5 往后都要往前挪一格才对得上记录 */
function fontAt(fonts: { bold: boolean; italic: boolean }[], ix: number) {
  return ix === 4 ? undefined : fonts[ix > 4 ? ix - 1 : ix];
}

interface BiffCtx {
  styles: CellStyle[];
  shared: string[];
  date1904: boolean;
  gbk: TextDecoder | null;
}

interface SheetCells {
  cells: Map<number, Map<number, SheetCellView>>;
  heights: Map<number, number>;
  merges: { r1: number; c1: number; r2: number; c2: number }[];
  colWidths: Map<number, number>;
  truncated: boolean;
}

const newSheetCells = (): SheetCells => ({
  cells: new Map(),
  heights: new Map(),
  merges: [],
  colWidths: new Map(),
  truncated: false,
});

function put(out: SheetCells, row: number, col: number, view: SheetCellView): void {
  if (row > MAX_ROWS || col >= MAX_COLS) {
    out.truncated = true;
    return;
  }
  let byCol = out.cells.get(row);
  if (!byCol) {
    byCol = new Map();
    out.cells.set(row, byCol);
  }
  byCol.set(col, view);
}

function styledView(ctx: BiffCtx, ixfe: number): SheetCellView {
  const st = ctx.styles[ixfe];
  return { ...blankCell(), bold: st?.bold ?? false, italic: st?.italic ?? false, align: st?.align ?? null };
}

function numberView(ctx: BiffCtx, ixfe: number, n: number): SheetCellView {
  const base = styledView(ctx, ixfe);
  const { text, numeric } = renderValue(String(n), ctx.styles[ixfe], ctx.date1904);
  return { ...base, text, align: base.align ?? (numeric ? "right" : null) };
}

function readBiffSheet(recs: Rec[], from: number, wb: Uint8Array, ctx: BiffCtx): SheetCells {
  const out = newSheetCells();
  /** FORMULA 的结果是字符串时，文本在紧跟的一条 STRING 记录里 */
  let pendingString: { row: number; col: number; ixfe: number } | null = null;

  for (let i = from; i < recs.length; i++) {
    const r = recs[i];
    if (r.op === OP_EOF) break;
    const at = r.at;
    const u16 = (o: number) => wb[at + o] | (wb[at + o + 1] << 8);
    switch (r.op) {
      case OP_LABELSST: {
        const isst = (wb[at + 6] | (wb[at + 7] << 8) | (wb[at + 8] << 16)) + wb[at + 9] * 0x1000000;
        const view = styledView(ctx, u16(4));
        view.text = ctx.shared[isst] ?? "";
        put(out, u16(0) + 1, u16(2), view);
        break;
      }
      case OP_RK:
        put(out, u16(0) + 1, u16(2), numberView(ctx, u16(4), rkNumber(wb, at + 6)));
        break;
      case OP_MULRK: {
        const row = u16(0) + 1;
        const last = u16(r.len - 2);
        for (let c = u16(2), o = 4; c <= last && o + 6 <= r.len - 2; c++, o += 6) {
          put(out, row, c, numberView(ctx, u16(o), rkNumber(wb, at + o + 2)));
        }
        break;
      }
      case OP_NUMBER:
        put(out, u16(0) + 1, u16(2), numberView(ctx, u16(4), realNumber(wb, at + 6)));
        break;
      case OP_FORMULA: {
        const cell = { row: u16(0) + 1, col: u16(2), ixfe: u16(4) };
        const lo = wb[at + 12];
        const hi = wb[at + 13];
        if (hi === 0xff) {
          if (lo === 0xff) pendingString = cell;
          else {
            const view = styledView(ctx, cell.ixfe);
            view.text = lo === 0 ? "FALSE" : "TRUE";
            view.align = view.align ?? "center";
            put(out, cell.row, cell.col, view);
          }
        } else {
          put(out, cell.row, cell.col, numberView(ctx, cell.ixfe, realNumber(wb, at + 6)));
        }
        break;
      }
      case OP_STRING: {
        if (!pendingString) break;
        const view = styledView(ctx, pendingString.ixfe);
        view.text = uniString(wb, at, ctx.gbk);
        put(out, pendingString.row, pendingString.col, view);
        pendingString = null;
        break;
      }
      case OP_BOOLERR: {
        const view = styledView(ctx, u16(4));
        view.text = wb[at + 7] ? "#N/A" : wb[at + 6] ? "TRUE" : "FALSE";
        view.align = view.align ?? "center";
        put(out, u16(0) + 1, u16(2), view);
        break;
      }
      case OP_BLANK:
        put(out, u16(0) + 1, u16(2), styledView(ctx, u16(4)));
        break;
      case OP_MULBLANK: {
        const row = u16(0) + 1;
        const last = u16(r.len - 2);
        for (let c = u16(2), o = 4; c <= last && o + 2 <= r.len - 2; c++, o += 2) {
          put(out, row, c, styledView(ctx, u16(o)));
        }
        break;
      }
      case OP_ROW: {
        const twips = u16(2);
        if (twips) out.heights.set(u16(0) + 1, Math.round((twips / 20) * 1.333));
        break;
      }
      case OP_ROW8: {
        /** 0008 号字段是「列分级数」，行高在第 6 字节起，单位 1/20 磅 */
        const twips = u16(6);
        if (twips) out.heights.set(u16(0) + 1, Math.round((twips / 20) * 1.333));
        break;
      }
      case OP_COLINFO: {
        const chars = u16(4) / 256;
        if (!chars) break;
        /** 宽度单位同样是「字符」，和 xlsx 的 <cols> 走同一套换算 */
        const px = Math.min(MAX_COL_PX, Math.max(MIN_COL_PX, Math.round(chars * 7) + 5));
        for (let c = u16(0); c <= u16(2); c++) out.colWidths.set(c + 1, px);
        break;
      }
      case OP_MERGEDCELLS:
        /** 开头 2 字节是矩形个数，之后每条是「上下左右」四个闭区间坐标 */
        for (let o = 2; o + 8 <= r.len; o += 8) {
          out.merges.push({ r1: u16(o) + 1, r2: u16(o + 2) + 1, c1: u16(o + 4), c2: u16(o + 6) });
        }
        break;
      default:
        break;
    }
  }
  return out;
}

export function readLegacyWorkbook(buffer: ArrayBuffer): SheetTable[] {
  const find = openCompoundFile(buffer);
  const wb = find("Workbook") ?? find("Book");
  if (!wb) return [];
  const view = new DataView(wb.buffer, wb.byteOffset, wb.byteLength);

  const recs: Rec[] = [];
  for (let off = 0; off + 4 <= wb.length; ) {
    const op = view.getUint16(off, true);
    const len = view.getUint16(off + 2, true);
    if (off + 4 + len > wb.length) break;
    recs.push({ op, at: off + 4, len });
    off += 4 + len;
  }

  const fonts: { bold: boolean; italic: boolean }[] = [];
  const custom = new Map<number, string>();
  const styles: CellStyle[] = [];
  const sheetRefs: { name: string; pos: number }[] = [];
  let shared: string[] = [];
  let date1904 = false;
  let gbk: TextDecoder | null = null;

  let firstSheetPos = Infinity;
  for (const r of recs) if (r.op === OP_BOUNDSHEET) firstSheetPos = Math.min(firstSheetPos, view.getUint32(r.at, true));

  for (let i = 0; i < recs.length; i++) {
    const r = recs[i];
    if (r.at - 4 >= firstSheetPos) break;
    const u16 = (o: number) => view.getUint16(r.at + o, true);
    if (r.op === OP_CODEPAGE) {
      const cp = u16(0);
      if (cp === 936 || cp === 20936 || cp === 54936) {
        try {
          gbk = new TextDecoder("gbk");
        } catch {
          gbk = null;
        }
      }
    } else if (r.op === OP_DATEMODE) date1904 = u16(0) === 1;
    else if (r.op === OP_FONT) {
      const flags = u16(2);
      /** 加粗有两处来源：grbit 的 bold 位，和 BIFF4 时代沿用到的字重字段（400 常规 / 700 加粗） */
      fonts.push({ bold: (flags & 1) === 1 || u16(6) >= 600, italic: (flags & 2) === 2 });
    } else if (r.op === OP_FORMAT) {
      const id = u16(0);
      const code = uniString(wb, r.at + 2, gbk);
      if (id) custom.set(id, code);
    } else if (r.op === OP_XF) {
      /** 实测 + 规范一致：字体索引在 0，格式索引在 2，对齐是第 6 字节的一个位段 */
      const f = fontAt(fonts, u16(0));
      const hor = view.getUint8(r.at + 6) & 7;
      styles.push({
        fmt: fmtCode(u16(2), custom),
        bold: f?.bold ?? false,
        italic: f?.italic ?? false,
        align: hor === 1 ? "left" : hor === 2 ? "center" : hor === 3 ? "right" : null,
      });
    } else if (r.op === OP_SST) {
      const chunks = [wb.subarray(r.at, r.at + r.len)];
      for (let k = i + 1; k < recs.length && recs[k].op === OP_CONTINUE; k++) {
        chunks.push(wb.subarray(recs[k].at, recs[k].at + recs[k].len));
      }
      /** 先按实测这套「条数×4」的写法读；没能正好吃完再退回规范布局试一次 */
      const a = readSst(chunks, gbk, 0);
      shared = (a.consumed === a.total ? a : readSst(chunks, gbk, 1)).strings;
    } else if (r.op === OP_BOUNDSHEET) {
      /** 实测这类文件的写法：位置(4) + 保留(2) + 名称；名称长度就是剩下的字节数 */
      const wide = (view.getUint8(r.at + 7) & 1) === 1;
      const bytes = wb.subarray(r.at + 8, r.at + r.len);
      let name = "";
      if (wide) {
        for (let k = 0; k + 1 < bytes.length; k += 2) name += String.fromCharCode(bytes[k] | (bytes[k + 1] << 8));
      } else {
        name = decodeRun([...bytes], gbk);
      }
      sheetRefs.push({ name, pos: view.getUint32(r.at, true) });
    }
  }

  const ctx: BiffCtx = { styles, shared, date1904, gbk };
  const out: SheetTable[] = [];
  for (const ref of sheetRefs) {
    const from = recs.findIndex((r) => r.at - 4 >= ref.pos);
    if (from < 0) continue;
    out.push(assembleSheet({ name: ref.name || `工作表${out.length + 1}`, ...readBiffSheet(recs, from, wb, ctx) }));
  }
  return out;
}
