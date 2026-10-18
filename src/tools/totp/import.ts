import type { TotpAlgorithm } from "../../types";
import { parseOtpauth } from "./otpauth";

export interface ImportEntry {
  line: number;
  issuer: string;
  account: string;
  secret: string;
  algorithm: TotpAlgorithm;
  digits: number;
  period: number;
  error?: string;
}

const DEFAULTS: Omit<ImportEntry, "line"> = {
  issuer: "",
  account: "",
  secret: "",
  algorithm: "sha1",
  digits: 6,
  period: 30,
};

export function normalizeSecret(raw: string) {
  return raw.replace(/[\s-_]/g, "").toUpperCase();
}

// 真实密钥按字节编码，字符数总是 4 的倍数，用这条把「一段普通文字」挡在外面
export function isBase32(value: string) {
  return /^[A-Z2-7]{16,128}$/.test(value) && value.length % 4 === 0;
}

// 站点导出和手写表格都常见这两种写法，空行与 # 开头的说明行跳过：
//   otpauth://totp/GitHub:alice?secret=XXXX&issuer=GitHub
//   GitHub,alice@example.com,JBSWY3DPEHPK3PXP
export function parseImportText(text: string): ImportEntry[] {
  const rows: ImportEntry[] = [];
  text.split(/\r?\n/).forEach((raw, i) => {
    const line = raw.trim();
    if (!line || line.startsWith("#")) return;

    const parsed = parseOtpauth(line);
    if (parsed) {
      rows.push({ line: i + 1, ...parsed });
      return;
    }

    const entry: ImportEntry = { line: i + 1, ...DEFAULTS };
    if (line.toLowerCase().startsWith("otpauth://")) {
      entry.error = "这条 otpauth 链接缺少 secret";
      rows.push(entry);
      return;
    }

    const cells = line.split(/[,;\t]/).map((c) => c.trim()).filter(Boolean);
    const cell = (index: number) => cells[index] ?? "";
    if (cells.length >= 3) {
      entry.issuer = cell(0);
      entry.account = cell(1);
      entry.secret = normalizeSecret(cell(2));
    } else if (cells.length === 2) {
      entry.issuer = cell(0);
      entry.secret = normalizeSecret(cell(1));
    } else {
      entry.secret = normalizeSecret(cell(0));
    }

    if (!entry.secret) entry.error = "没找到密钥";
    else if (!isBase32(entry.secret)) entry.error = `密钥不是合法的 base32：${entry.secret.slice(0, 24)}`;
    else if (!entry.issuer) entry.error = "缺少服务方名称";
    rows.push(entry);
  });
  return rows;
}
