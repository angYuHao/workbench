import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { Spin, pressable } from "../../ui";
import { PreviewError } from "./PreviewError";
import {
  readWorkbook,
  looksLikeZip,
  colLabel,
  MAX_ROWS,
  MAX_COLS,
  type DocBlock,
  type SheetTable,
} from "../../office/ooxml";
import { isCompoundFile, readLegacyWorkbook } from "../../office/biff8";
import { readLegacyDocument } from "../../office/doc97";
import type { ArtShape } from "../../office/wordart";

type State =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "sheets"; sheets: SheetTable[] }
  | { status: "docx"; buffer: ArrayBuffer; arts: ArtShape[] }
  | { status: "doc"; blocks: DocBlock[]; truncated: boolean };

/** docx / xlsx 是 zip+XML，直接拉字节交给浏览器原生 API 解析，不经过后端的文本读取 */
export default function OfficePreview({ path, kind }: { path: string; kind: "doc" | "sheet" }) {
  const [state, setState] = useState<State>({ status: "loading" });

  useEffect(() => {
    let alive = true;
    setState({ status: "loading" });
    (async () => {
      try {
        const res = await fetch(convertFileSrc(path));
        if (!res.ok) throw new Error(`读取失败：HTTP ${res.status}`);
        const buffer = await res.arrayBuffer();
        const bytes = new Uint8Array(buffer);
        let next: State;
        if (kind === "sheet") {
          /** xlsx 是 zip，旧 .xls 是 OLE 复合文档，两条路解析出的结构一样 */
          if (looksLikeZip(bytes)) next = { status: "sheets", sheets: await readWorkbook(buffer) };
          else if (isCompoundFile(bytes)) next = { status: "sheets", sheets: readLegacyWorkbook(buffer) };
          else throw new Error("扩展名是表格文件，但内容既不是 xlsx 也不是旧版 .xls");
        } else {
          /** docx 是 zip：先把 docx-preview 认不了的艺术字/文本框摘出来，整包再交给它渲染 */
          if (looksLikeZip(bytes)) {
            const { prepareDocx } = await import("../../office/wordart");
            const prepared = await prepareDocx(buffer);
            next = { status: "docx", buffer: prepared.buffer, arts: prepared.arts };
          } else if (isCompoundFile(bytes)) {
            const doc = readLegacyDocument(buffer);
            next = { status: "doc", blocks: doc.blocks, truncated: doc.truncated };
          } else throw new Error("扩展名是 Word 文档，但内容既不是 docx 也不是旧版 .doc");
        }
        if (alive) setState(next);
      } catch (e) {
        if (alive) setState({ status: "error", message: e instanceof Error ? e.message : String(e) });
      }
    })();
    return () => {
      alive = false;
    };
  }, [path, kind]);

  if (state.status === "loading") {
    return (
      <div style={{ display: "flex", justifyContent: "center", padding: 24 }}>
        <Spin />
      </div>
    );
  }

  if (state.status === "error") return <PreviewError label="无法解析该文档" message={state.message} />;

  if (state.status === "docx") return <DocxPage key={path} buffer={state.buffer} arts={state.arts} />;

  if (state.status === "doc") {
    return (
      <div className="office-doc">
        {state.blocks.map((b, i) =>
          b.type === "heading" ? (
            <div key={i} className={`office-h office-h${b.level}`}>
              {b.text}
            </div>
          ) : b.type === "bullet" ? (
            <div key={i} className="office-li">
              • {b.text}
            </div>
          ) : b.type === "para" ? (
            <p key={i}>{b.text}</p>
          ) : (
            <table key={i} className="office-table">
              <tbody>
                {b.rows.map((row, ri) => (
                  <tr key={ri}>
                    {row.map((cell, ci) => (
                      <td key={ci}>{cell}</td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          ),
        )}
        {state.truncated && <div className="label">内容过长，仅显示前部分</div>}
      </div>
    );
  }

  return <SheetGrid sheets={state.sheets} />;
}

function SheetGrid({ sheets }: { sheets: SheetTable[] }) {
  const [active, setActive] = useState(0);
  const sheet = sheets[Math.min(active, sheets.length - 1)];

  if (!sheet) return <div className="label">这个工作簿里没有工作表</div>;

  return (
    <div className="sheet-view">
      {sheets.length > 1 && (
        <div className="sheet-tabs" role="tablist">
          {sheets.map((s, i) => (
            <div
              key={`${s.name}-${i}`}
              className={i === active ? "sheet-tab on" : "sheet-tab"}
              onClick={() => setActive(i)}
              {...pressable(() => setActive(i), { role: "tab" })}
              aria-selected={i === active}
            >
              {s.name}
            </div>
          ))}
        </div>
      )}

      <div className="sheet-scroll">
        <table className="sheet-grid">
          <colgroup>
            <col style={{ width: 46 }} />
            {sheet.colWidths.map((w, i) => (
              <col key={i} style={{ width: w }} />
            ))}
            {/* 尾巴上不设宽的一列：table-layout: fixed 会把剩下来的宽度都给它，网格就贴到容器右边 */}
            <col className="grid-fill" />
          </colgroup>
          <thead>
            <tr>
              <th className="grid-corner" />
              {Array.from({ length: sheet.colCount }, (_, i) => (
                <th key={i} className="grid-colh">
                  {colLabel(i)}
                </th>
              ))}
              <th className="grid-colh grid-fill" />
            </tr>
          </thead>
          <tbody>
            {sheet.rows.map((row) => (
              <tr key={row.n} style={row.height ? { height: row.height } : undefined}>
                <th className="grid-rowh">{row.n}</th>
                {row.cells.map((cell, ci) =>
                  cell === null ? null : (
                    <td
                      key={ci}
                      colSpan={cell.colspan > 1 ? cell.colspan : undefined}
                      rowSpan={cell.rowspan > 1 ? cell.rowspan : undefined}
                      title={cell.text || undefined}
                      style={{
                        textAlign: cell.align ?? undefined,
                        fontWeight: cell.bold ? 700 : undefined,
                        fontStyle: cell.italic ? "italic" : undefined,
                      }}
                    >
                      {cell.text}
                    </td>
                  ),
                )}
                <td className="grid-fill" />
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {sheet.truncated && <div className="label">表格较大，仅显示前 {MAX_ROWS} 行 / {MAX_COLS} 列</div>}
    </div>
  );
}

/** docx 整包交给 docx-preview：页面尺寸、页边距、页眉页脚、样式表、编号、图片都按 Word 的排法来。
    换文件时靠 key 整个重建，避免上一份还没渲完就往同一个容器里追加 */
function DocxPage({ buffer, arts }: { buffer: ArrayBuffer; arts: ArtShape[] }) {
  const host = useRef<HTMLDivElement>(null);
  const [failed, setFailed] = useState<string | null>(null);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    let alive = true;
    /** docx-preview 连着它那套 DOM 工具、还有 wordart 用的 JSZip，一共约 370 kB gzip，只在真要看 docx 时才拉 */
    (async () => {
      const [{ renderAsync }, { applyArtwork }] = await Promise.all([import("docx-preview"), import("../../office/wordart")]);
      if (!alive) return;
      /** useBase64URL：图片走 data: 而不是 createObjectURL，后者 docx-preview 不回收，换文件会攒着不放。
          experimental 打开的是制表位对齐，目录那条「文字 …… 页码」的前导点靠它 */
      await renderAsync(buffer, el, el, {
        inWrapper: true,
        breakPages: true,
        renderHeaders: true,
        renderFooters: true,
        useBase64URL: true,
        experimental: true,
      });
      if (alive && arts.length) applyArtwork(el, arts);
    })().catch((e: unknown) => {
      if (alive) setFailed(e instanceof Error ? e.message : String(e));
    });
    return () => {
      alive = false;
    };
  }, [buffer, arts]);

  if (failed) return <PreviewError label="该 Word 文档渲染失败" message={failed} />;
  return (
    <div className="docx-scroll">
      <div ref={host} />
    </div>
  );
}

