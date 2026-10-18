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
        background: checked ? "var(--color-primary)" : "var(--color-border)",
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
          background: "#fff",
          position: "absolute",
          top: 2,
          left: checked ? w - knob - 2 : 2,
          transition: "left 0.2s ease",
          boxShadow: "0 1px 3px rgba(0,0,0,0.2)",
        }}
      />
    </div>
  );
}