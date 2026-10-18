import type { TotpAlgorithm } from "../../types";

export interface OtpauthFields {
  issuer: string;
  account: string;
  secret: string;
  algorithm: TotpAlgorithm;
  digits: number;
  period: number;
}

const ALGORITHMS: Record<string, TotpAlgorithm> = {
  sha1: "sha1",
  sha256: "sha256",
  sha512: "sha512",
};

// 各服务给的链接形如
// otpauth://totp/GitHub:alice?secret=ABCD2345&issuer=GitHub&algorithm=SHA1&digits=6&period=30
export function parseOtpauth(raw: string): OtpauthFields | null {
  const match = /^otpauth:\/\/totp\/([^?]*)\??(.*)$/i.exec(raw.trim());
  if (!match) return null;

  const query = new URLSearchParams(match[2] ?? "");
  const secret = (query.get("secret") ?? "").replace(/[\s-_]/g, "").toUpperCase();
  if (!secret) return null;

  const label = decodeURIComponent(match[1] ?? "");
  const [labelIssuer, ...accountParts] = label.split(":");
  const account = accountParts.join(":").trim() || (labelIssuer === label ? label.trim() : "");

  return {
    issuer: (query.get("issuer") ?? labelIssuer ?? "").trim(),
    account,
    secret,
    algorithm: ALGORITHMS[(query.get("algorithm") ?? "sha1").toLowerCase()] ?? "sha1",
    digits: Number(query.get("digits")) || 6,
    period: Number(query.get("period")) || 30,
  };
}
