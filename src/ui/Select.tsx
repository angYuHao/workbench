import React, { useState, useRef, useEffect, useLayoutEffect } from "react";
import { createPortal } from "react-dom";
import { ChevronDown, Search, X } from "lucide-react";
import { Input } from "./Input";
import { pressable } from "./pressable";

interface Option {
  label: string;
  value: string | number;
}

interface SelectProps {
  value?: string | number | null;
  onChange?: (value: string | number | null) => void;
  options: Option[];
  allowClear?: boolean;
  placeholder?: string;
  style?: React.CSSProperties;
  size?: "small" | "middle";
  disabled?: boolean;
  /** 面板顶部加搜索框，按 label / value 模糊过滤；选项上百条时才需要，默认关 */
  showSearch?: boolean;
  searchPlaceholder?: string;
}

interface DropPos {
  left: number;
  width: number;
  maxHeight: number;
  top?: number;
  bottom?: number;
}

export function Select({
  value,
  onChange,
  options,
  allowClear,
  placeholder = "请选择",
  style,
  size = "middle",
  disabled,
  showSearch,
  searchPlaceholder = "搜索",
}: SelectProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [dropPos, setDropPos] = useState<DropPos | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const dropRef = useRef<HTMLDivElement>(null);
  const selected = options.find((o) => o.value === value);

  const needle = query.trim().toLowerCase();
  const shown = needle
    ? options.filter(
        (o) => String(o.label).toLowerCase().includes(needle) || String(o.value).toLowerCase().includes(needle)
      )
    : options;

  // 下拉面板必须 portal 到 body。Modal 容器带 transform，会成为后代 fixed 元素的
  // 包含块，导致 getBoundingClientRect 的视口坐标被平移；它还有 overflow:hidden，
  // 留在弹窗子树内的面板会被裁掉。挂到 body 后才真正相对视口定位。
  useLayoutEffect(() => {
    if (!open) return;
    const recalc = () => {
      const el = ref.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      const spaceBelow = window.innerHeight - rect.bottom - 12;
      const spaceAbove = rect.top - 12;
      // 向下放不下且上方更宽裕时改为向上展开
      const placeAbove = spaceBelow < 240 && spaceAbove > spaceBelow;
      setDropPos({
        left: rect.left,
        width: rect.width,
        maxHeight: Math.max(96, placeAbove ? spaceAbove : spaceBelow),
        top: placeAbove ? undefined : rect.bottom + 4,
        bottom: placeAbove ? window.innerHeight - rect.top + 4 : undefined,
      });
    };
    recalc();
    window.addEventListener("resize", recalc);
    window.addEventListener("scroll", recalc, true);
    return () => {
      window.removeEventListener("resize", recalc);
      window.removeEventListener("scroll", recalc, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      const t = e.target as Node;
      if (ref.current?.contains(t) || dropRef.current?.contains(t)) return;
      setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const handleOpen = () => {
    if (disabled) return;
    const next = !open;
    if (next) setQuery("");
    setOpen(next);
  };

  const height = size === "small" ? 28 : 34;

  return (
    <div ref={ref} style={{ display: "inline-block", width: "100%", ...style }}>
      <div
        onClick={handleOpen}
        {...pressable(handleOpen, { role: "combobox" })}
        aria-expanded={open}
        aria-haspopup="listbox"
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          height,
          padding: `0 8px`,
          border: "1px solid var(--color-border)",
          borderRadius: "var(--radius-control)",
          background: "var(--color-card)",
          cursor: disabled ? "not-allowed" : "pointer",
          fontSize: size === "small" ? 12 : 14,
          opacity: disabled ? 0.5 : 1,
        }}
      >
        <span
          className="ellipsis"
          style={{ flex: 1, color: selected ? "var(--color-text)" : "var(--color-text-secondary)" }}
        >
          {selected ? selected.label : placeholder}
        </span>
        {allowClear && selected && (
          <span
            className="icon-button"
            aria-label="清空"
            onClick={(e) => {
              e.stopPropagation();
              onChange?.(null);
            }}
            {...pressable(() => onChange?.(null))}
          >
            <X size={14} />
          </span>
        )}
        <ChevronDown size={14} color="var(--color-text-secondary)" />
      </div>
      {open &&
        dropPos &&
        createPortal(
          <div
            ref={dropRef}
            style={{
              position: "fixed",
              top: dropPos.top,
              bottom: dropPos.bottom,
              left: dropPos.left,
              width: dropPos.width,
              zIndex: 9999,
              display: "flex",
              flexDirection: "column",
              overflow: "hidden",
              background: "var(--color-card)",
              border: "1px solid var(--color-border)",
              borderRadius: "var(--radius-control)",
              boxShadow: "var(--shadow-floating)",
              maxHeight: dropPos.maxHeight,
              padding: 4,
            }}
          >
            {showSearch && (
              <div
                style={{ flex: "none", marginBottom: 4 }}
                onKeyDown={(e) => {
                  if (e.key === "Escape") setOpen(false);
                }}
              >
                <Input
                  size="small"
                  autoFocus
                  allowClear
                  prefix={<Search size={13} />}
                  placeholder={searchPlaceholder}
                  value={query}
                  onChange={setQuery}
                  onEnter={() => {
                    const first = shown[0];
                    if (!first) return;
                    onChange?.(first.value);
                    setOpen(false);
                  }}
                />
              </div>
            )}
            <div role="listbox" style={{ flex: 1, minHeight: 0, overflow: "auto" }}>
              {shown.map((opt) => (
                <div
                  key={opt.value}
                  onClick={() => {
                    onChange?.(opt.value);
                    setOpen(false);
                  }}
                  {...pressable(
                    () => {
                      onChange?.(opt.value);
                      setOpen(false);
                    },
                    { role: "option" },
                  )}
                  aria-selected={opt.value === value}
                  style={{
                    padding: "6px 8px",
                    borderRadius: 6,
                    cursor: "pointer",
                    fontSize: 14,
                    background: opt.value === value ? "var(--color-primary-soft)" : "transparent",
                    color: opt.value === value ? "var(--color-primary)" : "var(--color-text)",
                    fontWeight: opt.value === value ? 600 : 400,
                  }}
                >
                  {opt.label}
                </div>
              ))}
              {/* 只在带搜索时给空态：否则选项本来为空的面板和以前一样是一片空白 */}
              {showSearch && !shown.length && (
                <div style={{ padding: "6px 8px", fontSize: 13, color: "var(--color-text-secondary)" }}>
                  没有匹配的选项
                </div>
              )}
            </div>
          </div>,
          document.body,
        )}
    </div>
  );
}