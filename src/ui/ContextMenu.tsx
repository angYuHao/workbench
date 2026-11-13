import { useEffect, useRef } from "react";
import { MenuPanel, type MenuEntry } from "./MenuPanel";

export type MenuItem = MenuEntry;

interface Props {
  /** 右键那一下的屏幕坐标；null 就是没开 */
  at: { x: number; y: number } | null;
  items: MenuItem[];
  onClose: () => void;
}

/**
 * 右键菜单。面板本体（圆点、分隔线、子菜单、视口夹取）和下拉共用 MenuPanel，
 * 这里只管「右键那一下开在哪、什么时候收」：点外面、Esc、滚动都收。
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

  return (
    <div ref={ref}>
      <MenuPanel
        at={at}
        entries={items}
        minWidth={196}
        onRun={(item) => {
          item.onClick?.();
          onClose();
        }}
      />
    </div>
  );
}