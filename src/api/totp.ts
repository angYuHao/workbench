import { invoke } from "@tauri-apps/api/core";
import type { TotpAccount, TotpAccountInput, TotpImportReport } from "../types";

export function getTotpAccounts() {
  return invoke<TotpAccount[]>("get_totp_accounts");
}

export function saveTotpAccount(account: TotpAccountInput) {
  return invoke<TotpAccount>("save_totp_account", { account });
}

export function deleteTotpAccount(id: number) {
  return invoke<void>("delete_totp_account", { id });
}

export function totpCode(id: number) {
  return invoke<string>("totp_code", { id });
}

export function totpSecret(id: number) {
  return invoke<string>("totp_secret", { id });
}

/// 口令加密导出，返回写入文件的账户条数
export function exportTotpBackup(dest: string, passphrase: string) {
  return invoke<number>("export_totp_backup", { dest, passphrase });
}

export function importTotpBackup(src: string, passphrase: string) {
  return invoke<TotpImportReport>("import_totp_backup", { src, passphrase });
}
