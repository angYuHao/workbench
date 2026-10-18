import React, { useState, useRef, useEffect } from "react";
import { pressable } from "./pressable";

interface MenuItem {
  key: string;
  label: string;
  danger?: boolean;
}

interface DropdownProps {
  trigger?: ("click" | "contextMenu")[];
  menu: {
    items: MenuItem[];
    onClick?: (info: { key: string }) => void;
  };
  children: React.ReactElement;
}

// 菜单是 fixed 定位、不会翻转，贴右边框的锚点（网格最右列）会被视口截掉，这里把它夹回视口内。
// 120 是菜单的 minWidth，按它估算足够贴边。
const MENU_EDGE_GAP = 8;
const MENU_MIN_WIDTH = 120;

function clampX(left: number): number {
  const max = window.innerWidth - MENU_MIN_WIDTH - MENU_EDGE_GAP;
  return Math.max(MENU_EDGE_GAP, Math.min(left, max));
}

export function Dropdown({ trigger = ["click"], menu, children }: DropdownProps) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState({ x: 0, y: 0 });
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  const handleContextMenu = (e: React.MouseEvent) => {
    if (trigger.includes("contextMenu")) {
      e.preventDefault();
      setPos({ x: e.clientX, y: e.clientY });
      setOpen(true);
    }
  };

  const handleClick = () => {
    if (trigger.includes("click")) {
      const rect = ref.current?.getBoundingClientRect();
      if (rect) setPos({ x: clampX(rect.left), y: rect.bottom + 4 });
      setOpen(true);
    }
  };

  return (
    <div ref={ref} style={{ display: "inline-block" }} onContextMenu={handleContextMenu} onClick={handleClick}>
      {children}
      {open && (
        <div
          role="menu"
          style={{
            position: "fixed",
            left: pos.x,
            top: pos.y,
            zIndex: 1000,
            background: "var(--color-card)",
            border: "1px solid var(--color-border)",
            borderRadius: "var(--radius-control)",
            boxShadow: "0 4px 12px rgba(0,0,0,0.12)",
            padding: 4,
            minWidth: 120,
          }}
        >
          {menu.items.map((item) => (
            <div
              key={item.key}
              onClick={(e) => {
                e.stopPropagation();
                menu.onClick?.({ key: item.key });
                setOpen(false);
              }}
              {...pressable(() => {
                menu.onClick?.({ key: item.key });
                setOpen(false);
              }, { role: "menuitem" })}
              style={{
                padding: "6px 12px",
                borderRadius: "var(--radius-icon)",
                cursor: "pointer",
                fontSize: 14,
                color: item.danger ? "var(--color-danger)" : "var(--color-text)",
                whiteSpace: "nowrap",
              }}
            >
              {item.label}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}