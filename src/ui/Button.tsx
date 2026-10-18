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
}

const sizeStyles: Record<ButtonSize, React.CSSProperties> = {
  small: { padding: "4px 10px", fontSize: 12, height: 28 },
  middle: { padding: "6px 16px", fontSize: 14, height: 34 },
  large: { padding: "8px 20px", fontSize: 16, height: 42 },
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
      background: danger ? "var(--color-danger)" : "var(--color-primary)",
      color: "#fff",
    };
  } else if (type === "text" || type === "link") {
    typeStyle = {
      background: "transparent",
      color: danger ? "var(--color-danger)" : type === "link" ? "var(--color-primary)" : "var(--color-text)",
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