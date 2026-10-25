import { useState, useEffect } from "react";

type MessageType = "info" | "success" | "error";

interface Toast {
  id: number;
  type: MessageType;
  text: string;
}

let toastId = 0;
const toasts: Toast[] = [];
const listeners: (() => void)[] = [];

function notify() {
  listeners.forEach((fn) => fn());
}

function addToast(type: MessageType, text: string) {
  const id = ++toastId;
  toasts.push({ id, type, text });
  notify();
  setTimeout(() => {
    const idx = toasts.findIndex((t) => t.id === id);
    if (idx >= 0) {
      toasts.splice(idx, 1);
      notify();
    }
  }, 3000);
}

export const message = {
  info: (text: string) => addToast("info", text),
  success: (text: string) => addToast("success", text),
  error: (text: string) => addToast("error", text),
};

const styles: Record<MessageType, { bg: string; fg: string; border: string }> = {
  info: { bg: "var(--color-info-soft)", fg: "var(--color-info-text)", border: "var(--color-info-edge)" },
  success: { bg: "var(--color-success-soft)", fg: "var(--color-success-text)", border: "var(--color-success-edge)" },
  error: { bg: "var(--color-danger-soft)", fg: "var(--color-danger-text)", border: "var(--color-danger-edge)" },
};

export function MessageContainer() {
  const [, forceUpdate] = useState(0);

  useEffect(() => {
    const fn = () => forceUpdate((n) => n + 1);
    listeners.push(fn);
    return () => {
      const idx = listeners.indexOf(fn);
      if (idx >= 0) listeners.splice(idx, 1);
    };
  }, []);

  return (
    <div
      style={{
        position: "fixed",
        top: 16,
        left: "50%",
        transform: "translateX(-50%)",
        zIndex: 2000,
        display: "flex",
        flexDirection: "column",
        gap: 8,
        pointerEvents: "none",
      }}
    >
      {toasts.map((t) => {
        const s = styles[t.type];
        return (
          <div
            key={t.id}
            style={{
              padding: "8px 16px",
              borderRadius: "var(--radius-control)",
              background: s.bg,
              color: s.fg,
              border: `1px solid ${s.border}`,
              fontSize: 14,
              boxShadow: "var(--shadow-floating)",
              animation: "msgIn 0.2s ease",
            }}
          >
            {t.text}
          </div>
        );
      })}
    </div>
  );
}