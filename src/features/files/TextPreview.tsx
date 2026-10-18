import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Empty, Spin, Tag } from "../../ui";
import { PreviewError } from "./PreviewError";
import { MAX_TEXT_BYTES, decodeText, languageOf, readCapped } from "../../utils/textFile";

type State =
  | { status: "loading" }
  | { status: "binary" }
  | { status: "error"; message: string }
  | { status: "text"; text: string; html: string | null; encoding: string; lang: string | null; truncated: boolean };

/** 一整套 highlight.js 语言接近 180 kB，只有真要看代码时才拉，拉过一次就留着 */
let hljsOnce: Promise<typeof import("highlight.js/lib/common").default> | null = null;
const loadHljs = () => (hljsOnce ??= import("highlight.js/lib/common").then((m) => m.default));

/** 任何文本文件都按文本排：能不能预览由内容嗅探决定，扩展名只管要不要上色。
    这里读的是字节而不是后端转好的字符串——中文机上 GBK 的 txt/csv/log 太常见，
    后端 from_utf8_lossy 会把它们变成一串替换符。 */
export default function TextPreview({ path, name }: { path: string; name: string }) {
  const [state, setState] = useState<State>({ status: "loading" });

  useEffect(() => {
    let alive = true;
    setState({ status: "loading" });
    (async () => {
      const { bytes, truncated } = await readCapped(convertFileSrc(path));
      if (!alive) return;
      const decoded = decodeText(bytes);
      if (!decoded) {
        setState({ status: "binary" });
        return;
      }
      /** 语言认不出就不上色，也不做自动判定——中文散文很容易被猜成某种代码 */
      const lang = languageOf(name);
      let html: string | null = null;
      if (lang) {
        const hljs = await loadHljs();
        if (hljs.getLanguage(lang)) html = hljs.highlight(decoded.text, { language: lang, ignoreIllegals: true }).value;
      }
      if (alive) {
        setState({
          status: "text",
          text: decoded.text,
          html,
          encoding: decoded.encoding,
          lang: html ? lang : null,
          truncated,
        });
      }
    })().catch((e: unknown) => {
      if (alive) setState({ status: "error", message: e instanceof Error ? e.message : String(e) });
    });
    return () => {
      alive = false;
    };
  }, [path, name]);

  if (state.status === "loading") {
    return (
      <div style={{ display: "flex", justifyContent: "center", padding: 24 }}>
        <Spin />
      </div>
    );
  }
  if (state.status === "error") return <PreviewError label="无法读取该文件" message={state.message} />;
  if (state.status === "binary") {
    return <Empty description="内容看起来不是文本，无法预览，可用上方「打开」按钮交给系统程序" />;
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8, height: "100%", minHeight: 0 }}>
      {state.html === null ? (
        <pre className="text-view">{state.text || "（空文件）"}</pre>
      ) : (
        <pre className="text-view">
          <code className="hljs" dangerouslySetInnerHTML={{ __html: state.html }} />
        </pre>
      )}
      <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
        {state.lang && <Tag>{state.lang}</Tag>}
        {state.encoding !== "utf-8" && (
          <Tag color="warning">{state.encoding === "gbk" ? "按 GBK 解码" : `按 ${state.encoding.toUpperCase()} 解码`}</Tag>
        )}
        {state.truncated && <Tag color="warning">文件超过 {MAX_TEXT_BYTES / 1024} KB，仅显示开头部分</Tag>}
      </div>
    </div>
  );
}
