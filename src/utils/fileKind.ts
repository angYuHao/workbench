import { BINARY_EXTENSIONS } from "./textFile";

const IMAGE_EXTENSIONS = new Set(["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "avif"]);

/** 交给 WebView2 自己渲染：pdf 用内置查看器，html 用 iframe（相对资源按文件所在目录解析） */
const PDF_EXTENSIONS = new Set(["pdf"]);
const HTML_EXTENSIONS = new Set(["html", "htm"]);

/** OOXML（zip 包）自研解析；旧版 .xls 是 OLE 复合文档 + BIFF8 记录，.doc 是 OLE + FIB 片段表，都自研 */
const SHEET_EXTENSIONS = new Set(["xlsx", "xlsm", "xls"]);
const DOC_EXTENSIONS = new Set(["docx", "doc"]);
const LEGACY_OFFICE_EXTENSIONS = new Set(["ppt", "pptx", "pub"]);

const LAUNCHER_EXTENSIONS = [".lnk", ".exe", ".url", ".bat", ".cmd", ".msi", ".app", ".scr"];

export type PreviewKind = "markdown" | "text" | "image" | "pdf" | "html" | "sheet" | "doc" | "legacy" | "other";

function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot < 0 ? "" : name.slice(dot + 1).toLowerCase();
}

/** 文本不再靠扩展名白名单放行：`.mjs`、`.srt`、`.env`、没后缀的 Makefile、GBK 编码的 txt 都得能看，
    是不是文本由 TextPreview 读字节后按内容判定；这里只负责先挡掉一眼就不是文本的格式。 */
export function previewKind(name: string): PreviewKind {
  const ext = extensionOf(name);
  if (ext === "md") return "markdown";
  if (IMAGE_EXTENSIONS.has(ext)) return "image";
  if (PDF_EXTENSIONS.has(ext)) return "pdf";
  /** html 同时也算文本，必须排在 text 判断之前，否则会被当源码显示 */
  if (HTML_EXTENSIONS.has(ext)) return "html";
  if (SHEET_EXTENSIONS.has(ext)) return "sheet";
  if (DOC_EXTENSIONS.has(ext)) return "doc";
  if (LEGACY_OFFICE_EXTENSIONS.has(ext)) return "legacy";
  if (BINARY_EXTENSIONS.has(ext)) return "other";
  return "text";
}

export function stripExt(name: string): string {
  const lower = name.toLowerCase();
  for (const ext of LAUNCHER_EXTENSIONS) {
    if (lower.endsWith(ext)) return name.slice(0, name.length - ext.length);
  }
  return name;
}
