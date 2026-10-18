/** 后端 `TotpAccount.algorithm` 其实是 String，未知值在解码时会被拒；这里收窄是给界面用的 */
export type TotpAlgorithm = "sha1" | "sha256" | "sha512";

/** 2FA 账户（后端不回传密钥，需要时用 totp_secret 单独解密） */
export interface TotpAccount {
  id: number;
  issuer: string;
  account: string;
  algorithm: TotpAlgorithm;
  digits: number;
  period: number;
  sort: number;
  createdAt: string | null;
}

export interface TotpAccountInput {
  id: number;
  issuer: string;
  account: string;
  /** 编辑时留空表示沿用原密钥 */
  secret: string;
  algorithm?: TotpAlgorithm;
  digits?: number;
  period?: number;
}

/** 加密备份导入结果 */
export interface TotpImportReport {
  added: number;
  skipped: number;
}
