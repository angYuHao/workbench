import { useEffect, useRef } from "react";
import { pressable } from "./pressable";

export interface MenuItem {
  label: string;
  danger?: boolean;
  disabled?: boolean;
  onClick: () => void;
}

interface Props {
  /** 右键那一下的屏幕坐标；null 就是没开 */
  at: { x: number; y: number } | null;
  items: MenuItem[];
  onClose: () => void;
}

const WIDTH = 196;
const ROW = 28;

/**
 * 右键菜单。用 `position: fixed` 挂在调用处的末尾，这样不会被 `overflow: auto` 的那些栏剪掉，
 * 也就不用算「相对哪个容器」。超出视口的边往里收，靠右下角时整块仍然看得见。
 */
export function ContextMenu({ at, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!at) return;
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    /** 滚动会把 fixed 的菜单留在原地，跟它指着的那行错开——不如直接关掉 */
    const onScroll = () => onClose();
    document.addEventListener("mousedown", onDown, true);
    document.addEventListener("keydown", onKey, true);
    window.addEventListener("resize", onScroll);
    return () => {
      document.removeEventListener("mousedown", onDown, true);
      document.removeEventListener("keydown", onKey, true);
      window.removeEventListener("resize", onScroll);
    };
  }, [at, onClose]);

  if (!at) return null;
  const left = Math.max(8, Math.min(at.x, window.innerWidth - WIDTH - 8));
  const top = Math.max(8, Math.min(at.y, window.innerHeight - (items.length * ROW + 10) - 8));

  return (
    <div
      ref={ref}
      style={{
        position: "fixed",
        left,
        top,
        width: WIDTH,
        zIndex: 1000,
        padding: 5,
        borderRadius: "var(--radius-control)",
        background: "var(--color-card-raised)",
        border: "1px solid var(--color-border)",
        boxShadow: "var(--shadow-floating)",
      }}
      /** 右键菜单是「点一下就走」的，别让这一下冒泡去触发底下的栏切换或行的选中 */
      onMouseDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((item) => (
        <div
          key={item.label}
          style={{
            height: ROW,
            display: "flex",
            alignItems: "center",
            padding: "0 10px",
            borderRadius: 6,
            fontSize: 13,
            cursor: item.disabled ? "not-allowed" : "pointer",
            opacity: item.disabled ? 0.5 : 1,
            color: item.danger ? "var(--color-danger-text)" : "var(--color-text)",
          }}
          {...pressable(() => {
            if (item.disabled) return;
            item.onClick();
            onClose();
          })}
          onMouseEnter={(e) => {
            if (!item.disabled) e.currentTarget.style.background = "var(--color-hover-sunken)";
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = "transparent";
          }}
          onClick={() => {
            if (item.disabled) return;
            item.onClick();
            onClose();
          }}
        >
          {item.label}
        </div>
      ))}
    </div>
  );
}
