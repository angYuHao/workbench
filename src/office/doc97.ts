import { openCompoundFile } from "./biff8";
import type { DocBlock } from "./ooxml";

const MAX_DOC_BLOCKS = 1500;

interface Piece {
  cpStart: number;
  cpEnd: number;
  off: number;
  compressed: boolean;
}

const ansi = new TextDecoder("windows-1252");
const utf16le = new TextDecoder("utf-16le");

/**
 * Word 97/2000 的 .doc：CFB 容器里 WordDocument 流开头是 FIB，
 * 正文按「字符位置」存在片段表（Clx）指向的位置上，位置既可能在 WordDocument 也可能在表流。
 * 只取正文段（ccpText），页眉页脚/脚注那些字符段一律不碰。
 */
export function readLegacyDocument(buffer: ArrayBuffer): { blocks: DocBlock[]; truncated: boolean } {
  const cf = openCompoundFile(buffer);
  const wd = cf("WordDocument");
  if (!wd) throw new Error("这个 .doc 里没有 WordDocument 流（比 Word 97 更早的格式，或文件已损坏）");
  const wdv = new DataView(wd.buffer, wd.byteOffset, wd.byteLength);
  if (wdv.getUint16(0, true) !== 0xa5ec) throw new Error("文件头不是 Word 文档");

  /** FIB 各段长度是自描述的，得按 csw/cslw 一路加过去才知道 FC/LCB 配对从哪开始 */
  const csw = wdv.getUint16(0x20, true);
  const cslw = wdv.getUint16(0x22 + csw * 2, true);
  const rgFc = 0x26 + csw * 2 + cslw * 4;
  const pairCount = wdv.getUint16(rgFc - 2, true);
  if (rgFc + pairCount * 8 > wd.length) throw new Error("FIB 声明的字段表超出了流长度，文件不完整");
  const pair = (index: number): [number, number] => [wdv.getUint32(rgFc + index * 8, true), wdv.getUint32(rgFc + index * 8 + 4, true)];

  const useOne = (wdv.getUint16(0x0a, true) & 0x0200) !== 0;
  const tb = cf(useOne ? "1Table" : "0Table") ?? cf(useOne ? "0Table" : "1Table");
  if (!tb) throw new Error(`缺少 ${useOne ? "1Table" : "0Table"} 流`);
  const tdv = new DataView(tb.buffer, tb.byteOffset, tb.byteLength);

  const ccpText = wdv.getUint32(0x4c, true);
  const [fcClx, lcbClx] = pairCount > 33 ? pair(33) : [0, 0];
  const pieces = lcbClx >= 16 && fcClx + lcbClx <= tb.length ? readPieces(tdv, tb, fcClx, fcClx + lcbClx) : [];
  if (!pieces.length) throw new Error("没找到片段表，正文位置无法定位（可能只是空文档或已损坏）");

  let text = "";
  for (const pc of pieces) {
    const count = pc.cpEnd - pc.cpStart;
    /** 压缩片段是一个字节一个字符（英文/标点），否则是 UTF-16 小端 */
    const bytes = pc.compressed
      ? wd.subarray(pc.off, Math.min(pc.off + count, wd.length))
      : wd.subarray(pc.off, Math.min(pc.off + count * 2, wd.length));
    text += pc.compressed ? ansi.decode(bytes) : utf16le.decode(bytes);
  }
  return blocksFromText(text.slice(0, ccpText || text.length));
}

/** Clx = 若干 Prc（样式草稿）+ 一个 Pcdt；Pcdt 里是 PlcPcd：先 n+1 个字符位置，再 n 个 8 字节片段描述 */
function readPieces(tdv: DataView, tb: Uint8Array, at: number, end: number): Piece[] {
  const out: Piece[] = [];
  let p = at;
  while (p < end) {
    const clxt = tb[p];
    if (clxt === 0x01) {
      p += 3 + tdv.getUint16(p + 1, true);
      continue;
    }
    if (clxt !== 0x02) break;
    const lcb = tdv.getUint32(p + 1, true);
    const count = (lcb - 4) / 12;
    if (!Number.isInteger(count) || count < 1) break;
    const base = p + 5;
    for (let i = 0; i < count; i++) {
      const fc = tdv.getUint32(base + 4 * (count + 1) + 8 * i + 2, true);
      const compressed = (fc & 0x40000000) !== 0;
      out.push({
        cpStart: tdv.getUint32(base + 4 * i, true),
        cpEnd: tdv.getUint32(base + 4 * (i + 1), true),
        off: (fc & 0x3ffffffe) >> (compressed ? 1 : 0),
        compressed,
      });
    }
    break;
  }
  return out;
}

/** 控制字符在 .doc 正文里各有含义：段落 0D、单元格/行 07、软换行 0B、图片 01/08、自动编号 04、域 13/14/15 */
function blocksFromText(text: string): { blocks: DocBlock[]; truncated: boolean } {
  const blocks: DocBlock[] = [];
  let truncated = false;
  let rows: string[][] | null = null;
  const flush = () => {
    if (rows && rows.length) blocks.push({ type: "table", rows });
    rows = null;
  };

  for (const raw of text.split("\r")) {
    const line = sanitize(raw);
    if (line.cells) {
      (rows ??= []).push(line.cells);
      continue;
    }
    flush();
    if (!line.text) continue;
    blocks.push(line.bullet ? { type: "bullet", text: line.text } : { type: "para", text: line.text });
    if (blocks.length >= MAX_DOC_BLOCKS) {
      truncated = true;
      break;
    }
  }
  flush();
  return { blocks, truncated };
}

function sanitize(raw: string): { text: string; cells?: string[]; bullet: boolean } {
  const noFields = dropFieldCodes(raw);
  let bullet = false;
  let out = "";
  for (const ch of noFields) {
    const c = ch.charCodeAt(0);
    if (c === 0x07) continue;
    if (c === 0x04) {
      bullet = true;
      continue;
    }
    if (c >= 0x20 && c !== 0x7f) {
      out += ch;
      continue;
    }
    out += c === 0x09 ? " " : c === 0x0b || c === 0x0c ? "\n" : c === 0x01 || c === 0x08 ? "[图片]" : c === 0x1e || c === 0x1f ? "-" : "";
  }
  if (raw.includes("\x07")) {
    /** 每个单元格以一个 07 结尾，行尾还有一个单独的 07，所以丢掉末尾的空段就是这一行的单元格 */
    const cells = raw.split("\x07").slice(0, -1).map((x) => sanitize(x).text.trim());
    return { text: "", cells, bullet };
  }
  return { text: out.replace(/[ \t]+\n/g, "\n").trim(), bullet };
}

/** 域代码（13 开始 / 14 指令与结果的分界 / 15 结束）：只保留算出来的结果，指令本身不显示 */
function dropFieldCodes(s: string): string {
  if (!s.includes("\x13")) return s;
  let out = "";
  let i = 0;
  while (i < s.length) {
    if (s.charCodeAt(i) !== 0x13) {
      out += s[i++];
      continue;
    }
    let depth = 1;
    let result = "";
    let inResult = false;
    i++;
    while (i < s.length && depth > 0) {
      const c = s.charCodeAt(i);
      if (c === 0x13) depth++;
      else if (c === 0x15 && --depth === 0) {
        i++;
        break;
      } else if (c === 0x14 && depth === 1) inResult = true;
      if (inResult && c !== 0x14 && c !== 0x15) result += s[i];
      i++;
    }
    out += result;
  }
  return out;
}
