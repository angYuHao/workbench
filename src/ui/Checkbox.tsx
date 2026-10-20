import type { ReactNode } from "react";

interface CheckboxProps {
  checked: boolean;
  onChange?: (checked: boolean) => void;
  label?: ReactNode;
  disabled?: boolean;
  /** 表格里那种没有可见文字的勾：读屏得有个名字 */
  ariaLabel?: string;
}

export function Checkbox({ checked, onChange, label, disabled, ariaLabel }: CheckboxProps) {
  return (
    <label
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        fontSize: 13,
        userSelect: "none",
        cursor: disabled ? "not-allowed" : "pointer",
        opacity: disabled ? 0.5 : 1,
      }}
    >
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        aria-label={ariaLabel}
        onChange={(e) => onChange?.(e.target.checked)}
        style={{ width: 14, height: 14, margin: 0, accentColor: "var(--color-primary)", cursor: "inherit" }}
      />
      {label}
    </label>
  );
}
