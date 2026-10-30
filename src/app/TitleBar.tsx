import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-react";
import { Tooltip, pressable } from "../ui";
// 标题栏那颗就是产品图标本身：换图标只改这一处，别再另画一个标记
import appIcon from "../../src-tauri/icons/128x128.png";

const noop = () => Promise.resolve();

/**
 * 自绘标题栏。主窗口在配置里关了原生装饰，所以这一条就是它唯一的拖动区。
 *
 * 拖动靠 `data-tauri-drag-region`：那条路走到 wry 是往窗口发 `WM_NCLBUTTONDOWN + HTCAPTION`，
 * 也就是交回系统的移动循环 —— **贴边吸附跟着一起回来**，不必自己算屏幕四分区。代价是这条
 * 权限得给（`core:window:allow-start-dragging`，它不在 `core:window:default` 那套默认里）。
 *
 * **双击最大化不要自己写**：tauri 注入的 drag.js 已经把落在拖动区上的第二次 mousedown
 * （detail === 2）翻成 internal_toggle_maximize 了；这里再 toggle 一次就是切过去又切回来，
 * 看起来就是「双击标题栏没反应」。只有右上角那颗按钮需要显式调 toggleMaximize。
 *
 * 关闭走 `win.close()`，和原来那个原生 X 是同一条路：`lib.rs` 的 `CloseRequested` 先看
 * 「关窗口进托盘」那一档，开着就 `prevent_close()`。所以这颗按钮不会绕过托盘设置把程序杀掉。
 */
export default function TitleBar() {
  const win = (() => {
    // 浏览器里直接开 vite 时拿不到窗口：整条不渲染，开发态看到的还是原来的界面
    try {
      return getCurrentWindow();
    } catch {
      return null;
    }
  })();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!win) return;
    let alive = true;
    const unlisten = win
      .onResized(async () => {
        if (alive) setMaximized(await win.isMaximized().catch(() => false));
      })
      .catch(() => undefined);
    void win
      .isMaximized()
      .then((m) => alive && setMaximized(m))
      .catch(noop);
    return () => {
      alive = false;
      void unlisten.then((f) => f?.());
    };
  }, [win]);

  const act = (run: () => Promise<void>) => () => void run().catch(noop);
  const minimize = act(() => win?.minimize() ?? noop());
  const toggleMax = act(() => win?.toggleMaximize() ?? noop());
  const close = act(() => win?.close() ?? noop());

  return (
    <div className="title-bar" data-tauri-drag-region>
      <span className="tb-title" data-tauri-drag-region>
        <img className="tb-icon" src={appIcon} alt="" draggable={false} data-tauri-drag-region />
        个人工作台
      </span>
      <span className="tb-actions">
        <Tooltip title="最小化" placement="bottom">
          <button type="button" className="tb-btn" aria-label="最小化" onClick={minimize} {...pressable(minimize)}>
            <Minus size={15} strokeWidth={1.8} />
          </button>
        </Tooltip>
        <Tooltip title={maximized ? "还原" : "最大化"} placement="bottom">
          <button type="button" className="tb-btn" aria-label={maximized ? "还原" : "最大化"} onClick={toggleMax} {...pressable(toggleMax)}>
            <Square size={13} strokeWidth={maximized ? 2.4 : 1.8} />
          </button>
        </Tooltip>
        <Tooltip title="关闭" placement="bottom">
          <button type="button" className="tb-btn close" aria-label="关闭" onClick={close} {...pressable(close)}>
            <X size={15} strokeWidth={1.8} />
          </button>
        </Tooltip>
      </span>
    </div>
  );
}
