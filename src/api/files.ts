import { invoke } from "@tauri-apps/api/core";
import { write } from "./notify";
import type { FileNode, FileRecord, ReadTextResult } from "../types";

export function getManagedFolders() {
  return invoke<string[]>("get_managed_folders");
}

/** 受管目录的增减要重读根列表，所以归 files 这一个域 */
export function addManagedFolder(path: string) {
  return write<string[]>("add_managed_folder", "files", { path });
}

export function removeManagedFolder(path: string) {
  return write<string[]>("remove_managed_folder", "files", { path });
}

export function listFiles(root: string) {
  return invoke<FileNode[]>("list_files", { root });
}

export function getFile(path: string) {
  return invoke<FileRecord>("get_file", { path });
}

export function saveFileMeta(path: string, alias: string | null, description: string | null) {
  return write<FileRecord>("save_file_meta", "files", { path, alias, description });
}

export function readTextFile(path: string) {
  return invoke<ReadTextResult>("read_text_file", { path });
}
