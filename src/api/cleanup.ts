import { invoke } from "@tauri-apps/api/core";

/** 扫出来的一类垃圾，摆进表格让人勾。字段跟后端 CategoryScan 一一对应 */
export interface CleanupCategory {
  id: string;
  label: string;
  desc: string;
  /** 这类垃圾实际住哪，摆出来让人知道删的是什么 */
  hint: string;
  bytes: number;
  files: number;
  available: boolean;
  note: string | null;
}

/** 清理完一类的回执：清出多少、跳过多少 */
export interface CleanupResult {
  id: string;
  freed_bytes: number;
  removed_files: number;
  skipped_files: number;
  note: string | null;
}

/** 只统计不动磁盘 */
export function scanCleanup() {
  return invoke<CleanupCategory[]>("cleanup_scan");
}

/** 清掉勾上的那几类；清理不写库，别的窗口没有要重读的域，所以不走广播 */
export function runCleanup(ids: string[]) {
  return invoke<CleanupResult[]>("cleanup_run", { ids });
}