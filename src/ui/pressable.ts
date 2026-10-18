import type { KeyboardEvent } from "react";

/**
 * 给「长得像按钮但不是 <button>」的东西补上键盘那一半：Tab 能走到，Enter（可选再加空格）触发同一个动作。
 * 焦点圈不在这里管，`styles/layout.css` 里那条 `:focus-visible` 统一画。
 *
 * - `role` 默认按 button 报给读屏；表格行这类本身有别的语义的传 `null`。
 * - `space` 关掉是给「空格另有含义」的地方留的（比如以后要加「空格勾选」）。
 */
export function pressable(
  onActivate: () => void,
  opts: { role?: string | null; space?: boolean } = {},
) {
  const { role = "button", space = true } = opts;
  return {
    ...(role ? { role } : null),
    tabIndex: 0,
    onKeyDown: (e: KeyboardEvent) => {
      if (e.key === "Enter" || (space && e.key === " ")) {
        /** 空格默认会把页面往下滚一格，按钮不该跟着滚 */
        e.preventDefault();
        /** 图标按钮嵌在整行里时，键盘事件一路冒泡会把外层那个也触发一次 */
        e.stopPropagation();
        onActivate();
      }
    },
  };
}
