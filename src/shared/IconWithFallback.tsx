import { convertFileSrc } from "@tauri-apps/api/core";
import { useMemo, useState } from "react";

function stringToColor(str: string): string {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = str.charCodeAt(i) + ((hash << 5) - hash);
  }
  const hue = Math.abs(hash) % 360;
  return `hsl(${hue}, 55%, 52%)`;
}

interface Props {
  name: string;
  iconPath: string | null;
  size?: number;
}

export default function IconWithFallback({ name, iconPath, size = 48 }: Props) {
  const [error, setError] = useState(false);
  const initial = useMemo(() => name.trim().charAt(0).toUpperCase() || "?", [name]);
  const bg = useMemo(() => stringToColor(name), [name]);

  if (!iconPath || error) {
    return (
      <div
        style={{
          width: size,
          height: size,
          borderRadius: "var(--radius-icon)",
          background: bg,
          color: "#fff",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontSize: size * 0.4,
          fontWeight: 600,
          flexShrink: 0,
        }}
      >
        {initial}
      </div>
    );
  }
  return (
    <img
      src={convertFileSrc(iconPath)}
      width={size}
      height={size}
      style={{ borderRadius: "var(--radius-icon)", objectFit: "contain", flexShrink: 0 }}
      onError={() => setError(true)}
      alt={name}
      draggable={false}
    />
  );
}