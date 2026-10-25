import React, { useState } from "react";
import { X } from "lucide-react";

interface InputProps {
  value?: string;
  defaultValue?: string;
  onChange?: (value: string) => void;
  onBlur?: (e: React.FocusEvent<HTMLInputElement>) => void;
  placeholder?: string;
  prefix?: React.ReactNode;
  allowClear?: boolean;
  style?: React.CSSProperties;
  size?: "small" | "middle" | "large";
  disabled?: boolean;
  type?: "text" | "password";
  onEnter?: () => void;
  autoFocus?: boolean;
  /** 指向页面上某个 <datalist id>，给出候选但不限制手填 */
  list?: string;
  /** 字数上限，比如「简介不超过 50 字」这种口径 */
  maxLength?: number;
}

/** 高度和 Button 同一口径：跟着密度档走，横向内边距与字号是这个控件自己的层级 */
const inputSizes = {
  small: { paddingX: 8, fontSize: 12, height: "calc(var(--p-ctrl-h) - 6px)" },
  middle: { paddingX: 10, fontSize: 14, height: "var(--p-ctrl-h)" },
  large: { paddingX: 12, fontSize: 16, height: "calc(var(--p-ctrl-h) + 8px)" },
};

export function Input({
  value,
  defaultValue,
  onChange,
  onBlur,
  placeholder,
  prefix,
  allowClear,
  style,
  size = "middle",
  disabled,
  type = "text",
  onEnter,
  autoFocus,
  list,
  maxLength,
}: InputProps) {
  const [innerVal, setInnerVal] = useState(defaultValue ?? "");
  const val = value !== undefined ? value : innerVal;
  const sz = inputSizes[size];

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const v = e.target.value;
    setInnerVal(v);
    onChange?.(v);
  };

  const handleClear = () => {
    setInnerVal("");
    onChange?.("");
  };

  return (
    <div
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        background: "var(--color-card)",
        border: "1px solid var(--color-border)",
        borderRadius: "var(--radius-control)",
        padding: `0 ${sz.paddingX}px`,
        height: sz.height,
        fontSize: sz.fontSize,
        width: "100%",
        ...style,
      }}
    >
      {prefix && <span style={{ display: "flex", alignItems: "center", color: "var(--color-text-secondary)" }}>{prefix}</span>}
      <input
        type={type}
        value={val}
        autoFocus={autoFocus}
        list={list}
        maxLength={maxLength}
        onChange={handleChange}
        onBlur={onBlur}
        onKeyDown={(e) => {
          if (e.key === "Enter") onEnter?.();
        }}
        placeholder={placeholder}
        disabled={disabled}
        style={{
          flex: 1,
          minWidth: 0,
          border: "none",
          outline: "none",
          background: "transparent",
          color: "var(--color-text)",
          fontSize: sz.fontSize,
          padding: 0,
          height: "100%",
        }}
      />
      {allowClear && val && (
        <span
          onClick={handleClear}
          style={{ cursor: "pointer", display: "flex", alignItems: "center", color: "var(--color-text-secondary)" }}
        >
          <X size={14} />
        </span>
      )}
    </div>
  );
}

interface TextAreaProps {
  value?: string;
  defaultValue?: string;
  onChange?: (value: string) => void;
  onBlur?: (e: React.FocusEvent<HTMLTextAreaElement>) => void;
  placeholder?: string;
  autoSize?: { minRows?: number; maxRows?: number };
  style?: React.CSSProperties;
}

export function TextArea({
  value,
  defaultValue,
  onChange,
  onBlur,
  placeholder,
  autoSize,
  style,
}: TextAreaProps) {
  const [innerVal, setInnerVal] = useState(defaultValue ?? "");
  const val = value !== undefined ? value : innerVal;
  const minRows = autoSize?.minRows ?? 2;
  const maxRows = autoSize?.maxRows ?? 6;

  return (
    <textarea
      value={val}
      onChange={(e) => {
        setInnerVal(e.target.value);
        onChange?.(e.target.value);
      }}
      onBlur={onBlur}
      placeholder={placeholder}
      rows={minRows}
      style={{
        width: "100%",
        border: "1px solid var(--color-border)",
        borderRadius: "var(--radius-control)",
        padding: "6px 10px",
        fontSize: 14,
        background: "var(--color-card)",
        color: "var(--color-text)",
        outline: "none",
        resize: "none",
        minHeight: minRows * 22 + 12,
        maxHeight: maxRows * 22 + 12,
        overflow: "auto",
        ...style,
      }}
    />
  );
}

interface InputNumberProps {
  value?: number;
  onChange?: (value: number) => void;
  min?: number;
  max?: number;
  style?: React.CSSProperties;
  placeholder?: string;
}

export function InputNumber({ value, onChange, min, max, style, placeholder }: InputNumberProps) {
  return (
    <input
      type="number"
      value={value ?? ""}
      min={min}
      max={max}
      placeholder={placeholder}
      onChange={(e) => {
        const v = e.target.value;
        onChange?.(v === "" ? 0 : Number(v));
      }}
      style={{
        width: "100%",
        border: "1px solid var(--color-border)",
        borderRadius: "var(--radius-control)",
        padding: "6px 10px",
        fontSize: 14,
        background: "var(--color-card)",
        color: "var(--color-text)",
        outline: "none",
        ...style,
      }}
    />
  );
}