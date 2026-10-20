import { convertFileSrc } from "@tauri-apps/api/core";
import { FolderOpen, ExternalLink } from "lucide-react";
import ReactMarkdown, { defaultUrlTransform, type Options as MarkdownOptions } from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeRaw from "rehype-raw";
import rehypeHighlight from "rehype-highlight";
import rehypeSanitize, { defaultSchema, type Options as SanitizeSchema } from "rehype-sanitize";
import "highlight.js/styles/github.css";
import { Empty, Tag, message, pressable } from "../../ui";
import { openPath, revealInExplorer } from "../../api/shell";
import OfficePreview from "./OfficePreview";
import TextPreview from "./TextPreview";
import { previewKind } from "../../utils/fileKind";
import type { FileNode, FileRecord, ReadTextResult } from "../../types";

interface Props {
  node: FileNode | null;
  record: FileRecord | null;
  preview: ReadTextResult | null;
}

/** 在 GitHub 那套默认白名单上，放开文档里常见的排版标签与排版属性（居中、图片尺寸等）。
    脚本、事件属性（on\*）、危险协议仍然被剥掉；`style` 不放行，避免把排版变成注入面。 */
const mdSchema: SanitizeSchema = {
  ...defaultSchema,
  tagNames: [
    ...(defaultSchema.tagNames ?? []),
    "center",
    "details",
    "summary",
    "kbd",
    "mark",
    "ins",
    "del",
    "sup",
    "sub",
    "figure",
    "figcaption",
  ],
  attributes: {
    ...defaultSchema.attributes,
    "*": [...(defaultSchema.attributes?.["*"] ?? []), "align", "title", "dir"],
    img: [...(defaultSchema.attributes?.["img"] ?? []), "width", "height", "loading", "decoding"],
    table: [...(defaultSchema.attributes?.["table"] ?? []), "width", "align"],
    video: [...(defaultSchema.attributes?.["video"] ?? []), "width", "height", "poster"],
  },
  /** 本地图片解析完会变成 asset 协议的绝对路径，得让它过净化 */
  protocols: {
    ...defaultSchema.protocols,
    src: [...(defaultSchema.protocols?.src ?? []), "asset", "data"],
  },
};

/** 顺序要紧：先把原始 HTML 解析成节点，再净化，最后才上色（上色加的 class 不会被净化削掉） */
const mdRehypePlugins: NonNullable<MarkdownOptions["rehypePlugins"]> = [
  rehypeRaw,
  [rehypeSanitize, mdSchema],
  rehypeHighlight,
];

const USABLE_URL_RE = /^(https?:|data:|asset:|blob:|mailto:)/i;
const DRIVE_PATH_RE = /^[a-z]:[\\/]/i;

function dirOf(filePath: string): string {
  const idx = Math.max(filePath.lastIndexOf("/"), filePath.lastIndexOf("\\"));
  return idx <= 0 ? "" : filePath.slice(0, idx);
}

/** md 里的相对路径是相对这篇 md 的，而预览页跑在应用自己的 origin 上，
    交给浏览器解析会指向 localhost:1420 而 404。这里手动拼成绝对路径再走 asset 协议。 */
function resolveMdSrc(url: string, baseDir: string): string {
  if (USABLE_URL_RE.test(url)) return url;
  if (/^#/.test(url)) return defaultUrlTransform(url);
  /** 剩下的都是相对路径或本地绝对路径，先剥掉 #anchor / ?query 再解码 */
  const raw = url.split("#")[0].split("?")[0];
  let decoded = raw;
  try {
    decoded = decodeURIComponent(raw);
  } catch {
    decoded = raw;
  }
  if (DRIVE_PATH_RE.test(decoded)) return convertFileSrc(decoded);
  if (!baseDir) return defaultUrlTransform(url);
  /** javascript: 一类能过上面两条检查的怪写法，直接丢掉 */
  if (/^[a-z][a-z\d+\-.]*:/i.test(decoded)) return "";
  const sep = baseDir.includes("\\") ? "\\" : "/";
  const stack = baseDir.split(/[\\/]+/).filter(Boolean);
  for (const part of decoded.split(/[\\/]+/)) {
    if (!part || part === ".") continue;
    if (part === "..") stack.pop();
    else stack.push(part);
  }
  return stack.length ? convertFileSrc(stack.join(sep)) : "";
}

export default function PreviewPanel({ node, record, preview }: Props) {
  if (!node) {
    return <Empty style={{ marginTop: 80, padding: 16 }} description="在文件列表中选择一个文件或文件夹" />;
  }

  const kind = previewKind(node.name);

  const content = node.isDir ? (
    <Empty description="目录" />
  ) : kind === "image" ? (
    <img
      src={convertFileSrc(node.path)}
      alt={node.name}
      style={{ maxWidth: "100%", borderRadius: "var(--radius-control)" }}
    />
  ) : kind === "pdf" || kind === "html" ? (
    <iframe
      src={convertFileSrc(node.path)}
      title={node.name}
      /** html 自己带脚本，沙箱里不放行 allow-scripts；pdf 不能加沙箱，否则 WebView2 内置查看器不接管 */
      sandbox={kind === "html" ? "allow-same-origin" : undefined}
      style={{
        display: "block",
        width: "100%",
        height: "100%",
        minHeight: 240,
        border: "1px solid var(--color-border)",
        borderRadius: "var(--radius-control)",
        background: "#fff",
      }}
    />
  ) : kind === "sheet" || kind === "doc" ? (
    <OfficePreview path={node.path} kind={kind} />
  ) : kind === "legacy" ? (
    <div style={{ color: "var(--color-text-secondary)", fontSize: 13, lineHeight: 1.7 }}>
      这种格式不在解析范围内（幻灯片 .ppt/.pptx，或出版物 .pub）。用上方「打开」按钮交给系统程序查看。
    </div>
  ) : kind === "text" ? (
    <TextPreview path={node.path} name={node.name} />
  ) : preview ? (
    <>
      <div className="md-body">
        <ReactMarkdown
          remarkPlugins={[remarkGfm]}
          rehypePlugins={mdRehypePlugins}
          urlTransform={(url, key) => (key === "src" ? resolveMdSrc(url, dirOf(node.path)) : defaultUrlTransform(url))}
        >
          {preview.content}
        </ReactMarkdown>
      </div>
      {preview.truncated && (
        <Tag color="warning" style={{ marginTop: 8 }}>
          文件过大，仅显示部分
        </Tag>
      )}
    </>
  ) : kind === "markdown" ? (
    <Empty description="无法预览" />
  ) : (
    <div style={{ color: "var(--color-text-secondary)" }}>该类型暂不支持预览，可点击上方按钮打开</div>
  );

  const description = record?.description ?? node.description;

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        gap: 12,
        padding: 16,
      }}
    >
      <div style={{ display: "flex", alignItems: "flex-start", gap: 8 }}>
        <div
          style={{
            flex: 1,
            minWidth: 0,
            fontSize: 13,
            lineHeight: 1.6,
            whiteSpace: "pre-wrap",
            wordBreak: "break-word",
            color: "var(--color-text-secondary)",
          }}
        >
          {description}
        </div>
        {!node.isDir && (
          <div style={{ display: "flex", gap: 4, flexShrink: 0 }}>
            <FolderOpen
              className="icon-button"
              size={16}
              aria-label="用系统程序打开"
              {...pressable(() => openPath(node.path).catch((e) => message.error(String(e))))}
              onClick={() => openPath(node.path).catch((e) => message.error(String(e)))}
            />
            <ExternalLink
              className="icon-button"
              size={16}
              aria-label="在资源管理器里显示"
              {...pressable(() =>
                revealInExplorer(node.path).catch((e) => message.error(String(e)))
              )}
              onClick={() => revealInExplorer(node.path).catch((e) => message.error(String(e)))}
            />
          </div>
        )}
      </div>

      <div className="pane-body" style={{ borderTop: "1px solid var(--color-border)", paddingTop: 12 }}>
        {content}
      </div>
    </div>
  );
}
