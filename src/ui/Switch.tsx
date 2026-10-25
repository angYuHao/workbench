import { pressable } from "./pressable";

interface SwitchProps {
  checked: boolean;
  onChange?: (checked: boolean) => void;
  size?: "small" | "default";
  disabled?: boolean;
}

export function Switch({ checked, onChange, size = "default", disabled }: SwitchProps) {
  const w = size === "small" ? 28 : 40;
  const h = size === "small" ? 16 : 22;
  const knob = h - 4;

  return (
    <div
      onClick={() => !disabled && onChange?.(!checked)}
      {...pressable(() => !disabled && onChange?.(!checked), { role: "switch" })}
      aria-checked={checked}
      aria-disabled={disabled}
      style={{
        width: w,
        height: h,
        borderRadius: h / 2,
        background: checked ? "var(--color-primary-solid)" : "var(--color-border)",
        position: "relative",
        cursor: disabled ? "not-allowed" : "pointer",
        transition: "background 0.2s ease",
        opacity: disabled ? 0.5 : 1,
        flexShrink: 0,
      }}
    >
      <div
        style={{
          width: knob,
          height: knob,
          borderRadius: "50%",
          background: "var(--color-primary-fg)",
          position: "absolute",
          top: 2,
          left: checked ? w - knob - 2 : 2,
          transition: "left 0.2s ease",
          boxShadow: "var(--shadow-control)",
        }}
      />
    </div>
  );
}