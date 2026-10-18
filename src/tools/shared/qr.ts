// 图片尺寸差别很大：过大时 jsQR 逐像素遍历会很慢，
// 先等比压到 1200px 以内再解码（小图保持原尺寸，放大反而糊）。
const MAX_EDGE = 1200;

export async function decodeQr(file: Blob): Promise<string | null> {
  /** jsQR 一个人就 250 kB，只有真扫码时才值得拉，所以不写在文件头上 */
  const { default: jsQR } = await import("jsqr");
  const bitmap = await createImageBitmap(file);
  const scale = Math.min(1, MAX_EDGE / Math.max(bitmap.width, bitmap.height));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(bitmap.width * scale));
  canvas.height = Math.max(1, Math.round(bitmap.height * scale));

  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) throw new Error("浏览器不支持 canvas 取图");
  ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  bitmap.close();

  const image = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const result = jsQR(new Uint8ClampedArray(image.data), image.width, image.height, {
    inversionAttempts: "attemptBoth",
  });
  return result?.data ?? null;
}

// 把后端抓屏返回的 base64 PNG 还原成 Blob，复用上面这条解码链路
export function base64ToPngBlob(base64: string): Blob {
  const bin = atob(base64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return new Blob([bytes], { type: "image/png" });
}
