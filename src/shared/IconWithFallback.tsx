import { convertFileSrc } from "@tauri-apps/api/core";
import { useMemo, useState } from "react";

/** 读不到图标文件时的替补色块。色相从名字哈希，深浅是配好的一对：
    以前是固定中亮度底 + 白字，哈希到黄绿色那一档时白字几乎看不见 */
function stringToTile(str: string): { bg: string; fg: string } {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = str.charCodeAt(i) + ((hash << 5) - hash);
  }
  const hue = Math.abs(hash) % 360;
  return { bg: `hsl(${hue}, 62%, 90%)`, fg: `hsl(${hue}, 58%, 25%)` };
}

interface Props {
  name: string;
  iconPath: string | null;
  size?: number;
}

export default function IconWithFallback({ name, iconPath, size = 48 }: Props) {
  const [error, setError] = useState(false);
  const initial = useMemo(() => name.trim().charAt(0).toUpperCase() || "?", [name]);
  const tile = useMemo(() => stringToTile(name), [name]);

  if (!iconPath || error) {
    return (
      <div
        style={{
          width: size,
          height: size,
          borderRadius: "var(--radius-icon)",
          background: tile.bg,
          color: tile.fg,
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