import { Tag } from "../../ui";

export function PreviewError({ label, message }: { label: string; message: string }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <Tag color="warning">{label}</Tag>
      <div style={{ fontSize: 13, lineHeight: 1.6, wordBreak: "break-word", color: "var(--color-text-secondary)" }}>
        {message}
      </div>
    </div>
  );
}
