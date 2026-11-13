import React, { useState, useRef, useEffect } from "react";
import { MenuPanel, type MenuEntry } from "./MenuPanel";

/** 下拉里的一项。动作可以自带在项上（各干各的），也可以由 menu.onClick 按 key 统一收口 ——
    两个都写了就都跑一遍，先项上那个 */
export type DropdownItem = MenuEntry;

interface DropdownProps {
  menu: {
    items: DropdownItem[];
    onClick?: (info: { key: string }) => void;
  };
  children: React.ReactElement;
}

export function Dropdown({ menu, children }: DropdownProps) {
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

  const handleClick = () => {
    const rect = ref.current?.getBoundingClientRect();
    if (rect) setPos({ x: rect.left, y: rect.bottom + 4 });
    setOpen(true);
  };

  const run = (item: DropdownItem) => {
    item.onClick?.();
    menu.onClick?.({ key: item.key ?? item.label });
    setOpen(false);
  };

  return (
    <div ref={ref} style={{ display: "inline-block" }} onClick={handleClick}>
      {children}
      {open && <MenuPanel at={pos} entries={menu.items} minWidth={120} onRun={run} />}
    </div>
  );
}