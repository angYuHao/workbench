import { useState } from "react";
import { Globe } from "lucide-react";

/** 网址自己的 favicon：拿不到（没网址、解不出域名、图挂了）就退成地球图标。
    favicon 走 Google 的 s2 接口，是这一页唯一允许的远程图片源。 */
export default function Favicon({ url }: { url: string | null }) {
  const [error, setError] = useState(false);
  if (!url || error) {
    return <Globe size={20} color="var(--color-text-secondary)" />;
  }

  let domain = "";
  try {
    domain = new URL(url).hostname;
  } catch {
    // 非法网址时退化为图标占位
  }

  return (
    <img
      src={domain ? `https://www.google.com/s2/favicons?domain=${domain}&sz=64` : ""}
      width={20}
      height={20}
      alt=""
      draggable={false}
      style={{ borderRadius: "var(--radius-icon)", flexShrink: 0 }}
      onError={() => setError(true)}
    />
  );
}
