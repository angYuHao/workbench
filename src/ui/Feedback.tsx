import React from "react";

export function Spin() {
  return (
    <div
      style={{
        width: 24,
        height: 24,
        border: "3px solid var(--color-border)",
        borderTopColor: "var(--color-primary)",
        borderRadius: "50%",
        animation: "spin 0.6s linear infinite",
      }}
    />
  );
}

export function Empty({ description, style }: { description?: React.ReactNode; style?: React.CSSProperties }) {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: 8,
        color: "var(--color-text-secondary)",
        fontSize: 14,
        padding: 32,
        ...style,
      }}
    >
      <svg width="48" height="48" viewBox="0 0 48 48" fill="none" opacity={0.4}>
        <rect x="8" y="8" width="32" height="32" rx="4" stroke="currentColor" strokeWidth="2" />
        <path d="M16 24h16M24 16v16" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
      </svg>
      <span>{description ?? "暂无数据"}</span>
    </div>
  );
}

interface TagProps {
  color?: "default" | "warning" | "success" | "danger";
  children: React.ReactNode;
  style?: React.CSSProperties;
}

const tagColors: Record<string, { bg: string; fg: string }> = {
  default: { bg: "var(--color-border)", fg: "var(--color-text)" },
  warning: { bg: "var(--color-warning-soft)", fg: "var(--color-warning-text)" },
  success: { bg: "var(--color-success-soft)", fg: "var(--color-success-text)" },
  danger: { bg: "var(--color-danger-soft)", fg: "var(--color-danger-text)" },
};

export function Tag({ color = "default", children, style }: TagProps) {
  const c = tagColors[color] ?? tagColors.default;
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        padding: "2px 8px",
        borderRadius: 4,
        fontSize: 12,
        background: c.bg,
        color: c.fg,
        ...style,
      }}
    >
      {children}
    </span>
  );
}

interface SpaceProps {
  size?: number | "small" | "middle" | "large";
  align?: "start" | "center";
  style?: React.CSSProperties;
  onClick?: (e: React.MouseEvent) => void;
  onDoubleClick?: (e: React.MouseEvent) => void;
  children: React.ReactNode;
}

const spaceSizes = { small: 4, middle: 8, large: 16 };

export function Space({
  size = 8,
  align = "center",
  style,
  onClick,
  onDoubleClick,
  children,
}: SpaceProps) {
  const gap = typeof size === "number" ? size : spaceSizes[size];
  return (
    <div
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      style={{
        display: "flex",
        flexDirection: "row",
        gap,
        alignItems: align === "start" ? "flex-start" : "center",
        ...style,
      }}
    >
      {children}
    </div>
  );
}