import { invoke } from "@tauri-apps/api/core";
import type { FileNode, FileRecord, ReadTextResult } from "../types";

export function getManagedFolders() {
  return invoke<string[]>("get_managed_folders");
}

export function addManagedFolder(path: string) {
  return invoke<string[]>("add_managed_folder", { path });
}

export function removeManagedFolder(path: string) {
  return invoke<string[]>("remove_managed_folder", { path });
}

export function listFiles(root: string) {
  return invoke<FileNode[]>("list_files", { root });
}

export function getFile(path: string) {
  return invoke<FileRecord>("get_file", { path });
}

export function saveFileMeta(path: string, alias: string | null, description: string | null) {
  return invoke<FileRecord>("save_file_meta", { path, alias, description });
}

export function readTextFile(path: string) {
  return invoke<ReadTextResult>("read_text_file", { path });
}