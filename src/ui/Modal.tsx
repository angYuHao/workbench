import React, { useState, useEffect } from "react";
import { X } from "lucide-react";
import { Button } from "./Button";
import { pressable } from "./pressable";
import { message } from "./Message";

const overlayStyle: React.CSSProperties = {
  position: "fixed",
  inset: 0,
  background: "rgba(0,0,0,0.45)",
};

const panelStyle: React.CSSProperties = {
  position: "fixed",
  top: "50%",
  left: "50%",
  transform: "translate(-50%, -50%)",
  background: "var(--color-card)",
  borderRadius: "var(--radius-card)",
  boxShadow: "0 8px 32px rgba(0,0,0,0.16)",
};

interface ModalProps {
  title?: React.ReactNode;
  open: boolean;
  onCancel?: () => void;
  onOk?: () => void;
  okText?: string;
  cancelText?: string;
  width?: number;
  /** 传 null 就是不要底部按钮（设置窗口这种「改完即生效」的面板不需要「确定」） */
  footer?: React.ReactNode;
  /** 需要自己排版时把内边距去掉，比如左导航右内容的分栏 */
  bodyStyle?: React.CSSProperties;
  children?: React.ReactNode;
}

export function Modal({
  title,
  open,
  onCancel,
  onOk,
  okText = "确定",
  cancelText = "取消",
  width = 520,
  footer,
  bodyStyle,
  children,
}: ModalProps) {
  // Esc 关对话框：鼠标能点遮罩关掉，键盘那条路得单独给
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel?.();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onCancel]);

  if (!open) return null;

  const footerContent =
    footer === undefined ? (
      <>
        <Button onClick={onCancel}>{cancelText}</Button>
        <Button type="primary" onClick={onOk}>
          {okText}
        </Button>
      </>
    ) : (
      footer
    );

  return (
    <>
      <div style={{ ...overlayStyle, zIndex: 1000 }} onClick={onCancel} />
      <div
        role="dialog"
        aria-modal="true"
        style={{
          ...panelStyle,
          width,
          maxWidth: "calc(100vw - 32px)",
          maxHeight: "calc(100vh - 64px)",
          zIndex: 1001,
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "16px 20px",
            borderBottom: "1px solid var(--color-border)",
          }}
        >
          <div style={{ fontSize: 16, fontWeight: 600 }}>{title}</div>
          <X
            className="icon-button"
            size={18}
            aria-label="关闭"
            {...pressable(() => onCancel?.())}
            onClick={onCancel}
          />
        </div>
        <div style={{ padding: 20, overflow: "auto", flex: 1, ...bodyStyle }}>{children}</div>
        {footer !== null && (
          <div
            style={{
              display: "flex",
              justifyContent: "flex-end",
              gap: 8,
              padding: "12px 20px",
              borderTop: "1px solid var(--color-border)",
            }}
          >
            {footerContent}
          </div>
        )}
      </div>
    </>
  );
}

interface ConfirmConfig {
  title: React.ReactNode;
  content?: React.ReactNode;
  onOk?: () => void | Promise<void>;
  okText?: string;
  cancelText?: string;
}

interface ConfirmState {
  config: ConfirmConfig;
  resolve: (confirmed: boolean) => void;
}

let confirmState: ConfirmState | null = null;
const confirmListeners: (() => void)[] = [];

export function ModalConfirm(config: ConfirmConfig): Promise<boolean> {
  return new Promise((resolve) => {
    confirmState = { config, resolve };
    confirmListeners.forEach((fn) => fn());
  });
}

export function ConfirmContainer() {
  const [, forceUpdate] = useState(0);

  useEffect(() => {
    const fn = () => forceUpdate((n) => n + 1);
    confirmListeners.push(fn);
    return () => {
      const idx = confirmListeners.indexOf(fn);
      if (idx >= 0) confirmListeners.splice(idx, 1);
    };
  }, []);

  // 确认框也吃 Esc，等价于点「取消」；监听常驻，弹出来的那一刻才有意义
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || !confirmState) return;
      const { resolve } = confirmState;
      confirmState = null;
      confirmListeners.forEach((fn) => fn());
      resolve(false);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  if (!confirmState) return null;
  const { config, resolve } = confirmState;

  const close = (confirmed: boolean) => {
    confirmState = null;
    confirmListeners.forEach((fn) => fn());
    resolve(confirmed);
  };

  return (
    <>
      <div style={{ ...overlayStyle, zIndex: 1100 }} onClick={() => close(false)} />
      <div style={{ ...panelStyle, width: 420, zIndex: 1101, padding: 20 }}>
        <div style={{ fontSize: 16, fontWeight: 600, marginBottom: 8 }}>{config.title}</div>
        {config.content && <div style={{ color: "var(--color-text-secondary)", marginBottom: 16 }}>{config.content}</div>}
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
          <Button onClick={() => close(false)}>{config.cancelText ?? "取消"}</Button>
          <Button
            type="primary"
            onClick={async () => {
              try {
                await config.onOk?.();
              } catch (e) {
                // 报错就把窗口留着：里面写的东西还在，改一下能直接重试。
                // 悄悄关掉等于把失败藏起来，用户只会以为「点了没反应」
                message.error(String(e));
                return;
              }
              close(true);
            }}
          >
            {config.okText ?? "确定"}
          </Button>
        </div>
      </div>
    </>
  );
}
