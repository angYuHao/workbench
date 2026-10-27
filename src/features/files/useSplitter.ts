import { useCallback, useEffect, useRef, useState } from "react";
import { getSettings, setSetting } from "../../api/settings";

/** 这张 KV 里的键名。只有文件页在乎这条分栏，所以跟着页面命名，不占一个顶层键 */
const WIDTH_KEY = "files.preview_width";

/**
 * 文件栏和预览栏之间那条可拖的分栏。
 * null 表示还没拖动过，两边按 1:1 等分；拖过之后预览栏按像素宽走，上限不设——
 * 能拖多宽由 flex 自己按剩余空间夹，所以永远不会撑出横向滚动条。
 *
 * 拖完松手那一下把宽度写进 settings，下次进来直接恢复。宽度存的是**渲染出来的**那个值而不是
 * 手指拖到的值：窗口窄的时候 flex 会把它夹短，记渲染值才不会存下一个装不下的宽度。
 */
export default function useSplitter() {
  const [previewWidth, setPreviewWidth] = useState<number | null>(null);
  const previewRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ startX: number; startWidth: number; moved: boolean } | null>(null);

  // 没拖过就没有这一行，读不到就照常 1:1；这一页之外没人写它
  useEffect(() => {
    let alive = true;
    getSettings()
      .then((s) => {
        const raw = s[WIDTH_KEY];
        if (raw === undefined) return;
        const saved = Number(raw);
        if (alive && Number.isFinite(saved) && saved >= 0) setPreviewWidth(saved);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, []);

  const startResize = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      /** 量当前实际渲染宽度而不是记下的数值：记下的可能比窗口允许的更宽（flex 会把它夹住），
          以渲染值起算才不会拖出「跟手但不长」的空转段 */
      const startWidth = Math.round(previewRef.current?.getBoundingClientRect().width ?? 0);
      dragRef.current = { startX: e.clientX, startWidth, moved: false };
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";

      const onMove = (ev: MouseEvent) => {
        if (!dragRef.current) return;
        dragRef.current.moved = true;
        const next = dragRef.current.startWidth + dragRef.current.startX - ev.clientX;
        /** 不设上下限，只挡负宽；能拖多宽由 flex 自己按剩余空间夹，所以永远不会撑出横向滚动条 */
        setPreviewWidth(Math.max(0, next));
      };
      const onUp = () => {
        const moved = dragRef.current?.moved ?? false;
        dragRef.current = null;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
        document.removeEventListener("mousemove", onMove);
        document.removeEventListener("mouseup", onUp);
        // 只在真拖过之后落库，而且只落松手这一次：拖动过程中每帧都写会把这张 KV 刷爆；
        // 在分栏上按一下就松开（没动）也不该改用户的偏好
        if (!moved) return;
        const final = Math.round(previewRef.current?.getBoundingClientRect().width ?? 0);
        void setSetting(WIDTH_KEY, String(final)).catch(() => undefined);
      };
      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", onUp);
    },
    [],
  );

  return { previewWidth, previewRef, startResize };
}
