import { useCallback, useRef, useState } from "react";

/**
 * 文件栏和预览栏之间那条可拖的分栏。
 * null 表示还没拖动过，两边按 1:1 等分；拖过之后预览栏按像素宽走，上限不设——
 * 能拖多宽由 flex 自己按剩余空间夹，所以永远不会撑出横向滚动条。
 */
export default function useSplitter() {
  const [previewWidth, setPreviewWidth] = useState<number | null>(null);
  const previewRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ startX: number; startWidth: number } | null>(null);

  const startResize = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      /** 量当前实际渲染宽度而不是记下的数值：记下的可能比窗口允许的更宽（flex 会把它夹住），
          以渲染值起算才不会拖出「跟手但不长」的空转段 */
      const startWidth = Math.round(previewRef.current?.getBoundingClientRect().width ?? 0);
      dragRef.current = { startX: e.clientX, startWidth };
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";

      const onMove = (ev: MouseEvent) => {
        if (!dragRef.current) return;
        const next = dragRef.current.startWidth + dragRef.current.startX - ev.clientX;
        /** 不设上下限，只挡负宽；能拖多宽由 flex 自己按剩余空间夹，所以永远不会撑出横向滚动条 */
        setPreviewWidth(Math.max(0, next));
      };
      const onUp = () => {
        dragRef.current = null;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
        document.removeEventListener("mousemove", onMove);
        document.removeEventListener("mouseup", onUp);
      };
      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", onUp);
    },
    [],
  );

  return { previewWidth, previewRef, startResize };
}
