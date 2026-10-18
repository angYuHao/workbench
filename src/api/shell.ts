import { invoke } from "@tauri-apps/api/core";

export function openPath(path: string) {
  return invoke<void>("open_path", { path });
}

export function revealInExplorer(path: string) {
  return invoke<void>("reveal_in_explorer", { path });
}