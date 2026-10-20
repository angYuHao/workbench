import { useEffect, useMemo, useState } from "react";
import * as api from "../../api/totp";
import type { TotpAccount } from "../../types";

interface TotpCodeEntry {
  code: string;
  window: number;
  error?: string;
}

// 秒级对时 + 只在时间窗切换时向后端取码：一次 invoke 代价很低，
// 但每秒为每个账户解密密钥没有必要。
export function useTotpCodes(accounts: TotpAccount[]) {
  const [entries, setEntries] = useState<Record<number, TotpCodeEntry>>({});
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));

  useEffect(() => {
    const timer = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000);
    return () => clearInterval(timer);
  }, []);

  const stale = useMemo(
    () =>
      accounts
        .map((account) => ({ account, window: Math.floor(now / account.period) }))
        .filter(({ account, window }) => entries[account.id]?.window !== window),
    [accounts, entries, now],
  );

  useEffect(() => {
    if (stale.length === 0) return;
    let alive = true;
    for (const { account, window } of stale) {
      const put = (entry: TotpCodeEntry) => {
        if (alive) setEntries((prev) => ({ ...prev, [account.id]: entry }));
      };
      api
        .totpCode(account.id)
        .then((code) => put({ code, window }))
        .catch((e: unknown) => put({ code: "", window, error: String(e) }));
    }
    return () => {
      alive = false;
    };
  }, [stale]);

  return { entries, now };
}
