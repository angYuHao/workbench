import { invoke } from "@tauri-apps/api/core";

// 后端会先隐藏本窗口再抓屏（否则截到的是自己），返回压缩后的 PNG base64
export function captureScreen() {
  return invoke<string>("capture_screen");
}
