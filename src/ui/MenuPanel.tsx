import { useLayoutEffect, useRef, useState } from "react";
import { ChevronRight } from "lucide-react";
import { pressable } from "./pressable";

/** 菜单里的一项。下拉和右键共用这一种形状：
    checked 画左边那颗单选圆点，children 挂子菜单，divider 只画一条分隔线。 */
export interface MenuEntry {
  key?: string;
  label: string;
  divider?: boolean;
  checked?: boolean;
  danger?: boolean;
  disabled?: boolean;
  children?: MenuEntry[];
  onClick?: () => void;
}

interface Props {
  at: { x: number; y: number };
  entries: MenuEntry[];
  minWidth: number;
  /** 叶子项被点/被回车：跑动作并收掉整张菜单，由调用方收口 */
  onRun: (item: MenuEntry) => void;
  /** 子菜单专用：贴父行右边放不下时翻到父行左边 */
  flip?: { left: number; right: number };
}

const EDGE = 8;

/** 子菜单和父行横向重叠几个像素：指针从父行划进子菜单时不会先离开父面板，
    不然 onMouseLeave 会在半路把子菜单收掉 */
const SUB_OVERLAP = 6;

export function MenuPanel({ at, entries, minWidth, onRun, flip }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState(at);
  const [openKey, setOpenKey] = useState<string | null>(null);
  const [subAt, setSubAt] = useState<{ x: number; y: number } | null>(null);
  const [subFlip, setSubFlip] = useState<{ left: number; right: number } | undefined>(undefined);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    let x = at.x;
    if (flip && x + w > window.innerWidth - EDGE) x = flip.left - w + EDGE;
    x = Math.max(EDGE, Math.min(x, window.innerWidth - w - EDGE));
    const y = Math.max(EDGE, Math.min(at.y, window.innerHeight - h - EDGE));
    setPos({ x, y });
  }, [at, flip, entries]);

  const openSub = (item: MenuEntry, row: HTMLElement) => {
    const r = row.getBoundingClientRect();
    setSubAt({ x: r.right - SUB_OVERLAP, y: r.top - 5 });
    setSubFlip({ left: r.left, right: r.right });
    setOpenKey(item.key ?? item.label);
  };

  const activate = (item: MenuEntry, row: HTMLElement | null) => {
    if (item.disabled) return;
    if (item.children?.length) {
      const id = item.key ?? item.label;
      if (openKey === id) setOpenKey(null);
      else if (row) openSub(item, row);
    } else {
      onRun(item);
    }
  };

  return (
    <div
      ref={ref}
      role="menu"
      style={{
        position: "fixed",
        left: pos.x,
        top: pos.y,
        zIndex: 1000,
        minWidth,
        maxHeight: "70vh",
        overflowY: "auto",
        padding: 4,
        borderRadius: "var(--radius-control)",
        background: "var(--color-card-raised)",
        border: "1px solid var(--color-border)",
        boxShadow: "var(--shadow-floating)",
      }}
      onClick={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
      onMouseLeave={() => setOpenKey(null)}
    >
      {entries.map((item) => {
        const id = item.key ?? item.label;
        if (item.divider) {
          return <div key={id} style={{ height: 1, margin: "4px 6px", background: "var(--color-border)" }} />;
        }
        const hasSub = !!item.children?.length;
        return (
          <div
            key={id}
            className="menu-row"
            aria-disabled={item.disabled || undefined}
            aria-expanded={hasSub ? openKey === id : undefined}
            style={{
              height: 30,
              display: "flex",
              alignItems: "center",
              gap: 8,
              padding: "0 10px",
              borderRadius: "var(--radius-icon)",
              fontSize: 13,
              cursor: item.disabled ? "not-allowed" : "pointer",
              opacity: item.disabled ? 0.5 : 1,
              color: item.danger ? "var(--color-danger-text)" : "var(--color-text)",
            }}
            onMouseEnter={(e) => {
              if (item.disabled) return;
              if (hasSub) openSub(item, e.currentTarget);
              else setOpenKey(null);
            }}
            onClick={(e) => {
              e.stopPropagation();
              activate(item, e.currentTarget);
            }}
            {...pressable(() => activate(item, document.activeElement as HTMLElement | null), {
              role: "menuitem",
            })}
          >
            <span style={{ width: 14, flex: "0 0 auto", display: "inline-flex", justifyContent: "center" }}>
              {item.checked && (
                <span style={{ width: 5, height: 5, borderRadius: "50%", background: "var(--color-text)" }} />
              )}
            </span>
            <span style={{ flex: 1, whiteSpace: "nowrap" }}>{item.label}</span>
            {hasSub && <ChevronRight size={14} style={{ flex: "0 0 auto", opacity: 0.7 }} />}
            {hasSub && openKey === id && subAt && (
              <MenuPanel at={subAt} entries={item.children!} minWidth={minWidth} onRun={onRun} flip={subFlip} />
            )}
          </div>
        );
      })}
    </div>
  );
}