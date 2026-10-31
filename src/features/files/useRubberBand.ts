import { useState } from "react";
import type { MouseEvent as ReactMouseEvent, RefObject } from "react";

/** 选框在视口里的位置：用 fixed 画，列里怎么滚都不用换算 */
export interface BandRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * 空白处按下左键拖出来的框选。跟资源管理器一样：
 * 框到哪几行选哪几行（压到边就算），按着 Ctrl 是往已有选中上并；
 * 只点一下空白不拖，就算「取消选中」。
 */
export default function useRubberBand(
  bodyRef: RefObject<HTMLDivElement>,
  onPick: (hits: string[], additive: boolean, base: string[]) => void,
  onBlankClick: () => void,
) {
  const [band, setBand] = useState<BandRect | null>(null);

  const start = (e: ReactMouseEvent, base: string[]) => {
    if (e.button !== 0) return;
    const body = bodyRef.current;
    if (!body) return;
    /** 按在滚动条上不是框选，不然拖不动这一列的滚动条 */
    const bar = body.offsetWidth - body.clientWidth;
    if (bar > 0 && e.clientX >= body.getBoundingClientRect().right - bar) return;
    /** 不拦的话按住一拖会先选到页面上的文字 */
    e.preventDefault();
    const x0 = e.clientX;
    const y0 = e.clientY;
    let active = false;
    let frame = 0;

    const pick = (x: number, y: number, additive: boolean) => {
      const left = Math.min(x0, x);
      const right = Math.max(x0, x);
      const top = Math.min(y0, y);
      const bottom = Math.max(y0, y);
      const hits: string[] = [];
      for (const row of body.querySelectorAll<HTMLElement>(".file-row[data-path]")) {
        const r = row.getBoundingClientRect();
        if (r.bottom > top && r.top < bottom && r.right > left && r.left < right) {
          const p = row.dataset.path;
          if (p) hits.push(p);
        }
      }
      onPick(hits, additive, base);
    };

    const move = (ev: MouseEvent) => {
      const dx = ev.clientX - x0;
      const dy = ev.clientY - y0;
      /** 手抖几个像素不算框选，不然单击空白会闪一下框 */
      if (!active && Math.abs(dx) < 4 && Math.abs(dy) < 4) return;
      active = true;
      setBand({
        left: Math.min(x0, ev.clientX),
        top: Math.min(y0, ev.clientY),
        width: Math.abs(dx),
        height: Math.abs(dy),
      });
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        pick(ev.clientX, ev.clientY, ev.ctrlKey || ev.metaKey);
      });
    };

    const stop = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      window.removeEventListener("blur", abort);
      if (frame) cancelAnimationFrame(frame);
      setBand(null);
    };

    /** 手松在窗口外面时收不到 mouseup：按窗口失焦收尾，别留一条框挂在屏幕上 */
    const abort = () => stop();

    const up = (ev: MouseEvent) => {
      stop();
      if (active) {
        pick(ev.clientX, ev.clientY, ev.ctrlKey || ev.metaKey);
        return;
      }
      /** 按着 Ctrl 点空白不动选中：那是想留着这串再接着框 */
      if (!(ev.ctrlKey || ev.metaKey)) onBlankClick();
    };

    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
    window.addEventListener("blur", abort);
  };

  return { band, start };
}
