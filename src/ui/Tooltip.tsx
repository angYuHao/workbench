import React, { useState, useRef } from "react";

interface TooltipProps {
  title?: React.ReactNode;
  placement?: "top" | "bottom" | "left" | "right";
  children: React.ReactElement;
}

export function Tooltip({ title, placement = "top", children }: TooltipProps) {
  const [visible, setVisible] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  if (!title) return children;

  const posStyles: Record<string, React.CSSProperties> = {
    top: { bottom: "calc(100% + 6px)", left: "50%", transform: "translateX(-50%)" },
    bottom: { top: "calc(100% + 6px)", left: "50%", transform: "translateX(-50%)" },
    left: { right: "calc(100% + 6px)", top: "50%", transform: "translateY(-50%)" },
    right: { left: "calc(100% + 6px)", top: "50%", transform: "translateY(-50%)" },
  };

  return (
    <div
      ref={ref}
      style={{ position: "relative", display: "inline-flex" }}
      onMouseEnter={() => setVisible(true)}
      onMouseLeave={() => setVisible(false)}
    >
      {children}
      {visible && (
        <div
          style={{
            position: "absolute",
            zIndex: 999,
            background: "rgba(0,0,0,0.8)",
            color: "#fff",
            padding: "4px 8px",
            borderRadius: 4,
            fontSize: 12,
            whiteSpace: "nowrap",
            pointerEvents: "none",
            ...posStyles[placement],
          }}
        >
          {title}
        </div>
      )}
    </div>
  );
}