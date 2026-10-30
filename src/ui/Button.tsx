import React from "react";

type ButtonType = "primary" | "default" | "text" | "link";
type ButtonSize = "small" | "middle" | "large";

interface ButtonProps {
  type?: ButtonType;
  size?: ButtonSize;
  danger?: boolean;
  icon?: React.ReactNode;
  disabled?: boolean;
  loading?: boolean;
  onClick?: (e: React.MouseEvent) => void;
  children?: React.ReactNode;
  style?: React.CSSProperties;
  className?: string;
  /** 纯图标按钮的名字：JSX 允许写带连字符的属性，但这里不往下传就丢了，所以显式收进 props */
  title?: string;
  "aria-label"?: string;
}

/** 高度跟着密度档走；内边距和字号是这个控件自己的三档层级，不跟着密度动 */
const sizeStyles: Record<ButtonSize, React.CSSProperties> = {
  small: { padding: "4px 10px", fontSize: 12, height: "calc(var(--p-ctrl-h) - 6px)" },
  middle: { padding: "6px 16px", fontSize: 14, height: "var(--p-ctrl-h)" },
  large: { padding: "8px 20px", fontSize: 16, height: "calc(var(--p-ctrl-h) + 8px)" },
};

export function Button({
  type = "default",
  size = "middle",
  danger,
  icon,
  disabled,
  loading,
  onClick,
  children,
  style,
  className,
  title,
  "aria-label": ariaLabel,
}: ButtonProps) {
  const base: React.CSSProperties = {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    gap: 6,
    borderRadius: "var(--radius-control)",
    cursor: disabled ? "not-allowed" : "pointer",
    fontWeight: 400,
    lineHeight: 1,
    border: "none",
    outline: "none",
    transition: "all 0.15s ease",
    userSelect: "none",
    whiteSpace: "nowrap",
    ...sizeStyles[size],
  };

  let typeStyle: React.CSSProperties = {};
  if (type === "primary") {
    typeStyle = {
      background: danger ? "var(--color-danger-solid)" : "var(--color-primary-solid)",
      color: danger ? "var(--color-danger-fg)" : "var(--color-primary-fg)",
    };
  } else if (type === "text" || type === "link") {
    typeStyle = {
      background: "transparent",
      color: danger ? "var(--color-danger-text)" : type === "link" ? "var(--color-primary-text)" : "var(--color-text)",
      padding: size === "small" ? "4px 6px" : "6px 8px",
      height: "auto",
    };
  } else {
    typeStyle = {
      background: "var(--color-card)",
      color: danger ? "var(--color-danger)" : "var(--color-text)",
      border: "1px solid var(--color-border)",
    };
  }

  return (
    <button
      type="button"
      disabled={disabled || loading}
      onClick={onClick}
      title={title}
      aria-label={ariaLabel}
      className={className}
      style={{
        ...base,
        ...typeStyle,
        opacity: disabled || loading ? 0.6 : 1,
        ...style,
      }}
    >
      {loading ? (
        <span style={{ width: 14, height: 14, border: "2px solid currentColor", borderTopColor: "transparent", borderRadius: "50%", animation: "spin 0.6s linear infinite", display: "inline-block" }} />
      ) : icon}
      {children}
    </button>
  );
}