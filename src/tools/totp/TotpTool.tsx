import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, Copy, Download, Eye, EyeOff, Pencil, Plus, Trash2, Upload } from "lucide-react";
import { totpSecret } from "../../api/totp";
import { useTotpStore } from "../../stores/totp";
import { Button, Empty, ModalConfirm, SearchBox, Space, Spin, message, pressable } from "../../ui";
import { copyText } from "../../utils/clipboard";
import { errText } from "../../utils/error";
import AddDialog, { emptyInput } from "./AddDialog";
import BackupDialog from "./BackupDialog";
import type { BackupMode } from "./BackupDialog";
import { useTotpCodes } from "./useTotpCodes";
import type { TotpAccount, TotpAccountInput } from "../../types";

function formatCode(code: string) {
  if (code.length === 6) return `${code.slice(0, 3)} ${code.slice(3)}`;
  if (code.length === 8) return `${code.slice(0, 4)} ${code.slice(4)}`;
  return code;
}

function Countdown({ remaining, period }: { remaining: number; period: number }) {
  const radius = 9;
  const circumference = 2 * Math.PI * radius;
  const ratio = Math.min(1, Math.max(0, remaining / period));
  const color = remaining <= 5 ? "var(--color-danger)" : "var(--color-primary)";
  return (
    <svg width={26} height={26} viewBox="0 0 26 26" style={{ flexShrink: 0 }}>
      <circle cx={13} cy={13} r={radius} fill="none" stroke="var(--color-border)" strokeWidth={3} />
      <circle
        cx={13}
        cy={13}
        r={radius}
        fill="none"
        stroke={color}
        strokeWidth={3}
        strokeLinecap="round"
        strokeDasharray={circumference}
        strokeDashoffset={circumference * (1 - ratio)}
        transform="rotate(-90 13 13)"
      />
      <text x={13} y={13} textAnchor="middle" dominantBaseline="central" fontSize={9} fill={color}>
        {remaining}
      </text>
    </svg>
  );
}

export default function TotpTool({ onBack }: { onBack: () => void }) {
  const { accounts, loading, load, remove } = useTotpStore();
  const { entries, now } = useTotpCodes(accounts);
  const [keyword, setKeyword] = useState("");
  const [edit, setEdit] = useState<TotpAccountInput | null>(null);
  const [secrets, setSecrets] = useState<Record<number, string>>({});
  const [backup, setBackup] = useState<BackupMode | null>(null);

  useEffect(() => {
    load();
  }, []);

  const visible = useMemo(() => {
    const k = keyword.toLowerCase();
    return accounts.filter(
      (a) => !k || a.issuer.toLowerCase().includes(k) || a.account.toLowerCase().includes(k),
    );
  }, [accounts, keyword]);

  const handleCopyCode = async (account: TotpAccount) => {
    const code = entries[account.id]?.code;
    if (!code) return;
    const ok = await copyText(code);
    message[ok ? "success" : "error"](ok ? "验证码已复制" : "复制失败");
  };

  const handleToggleSecret = async (account: TotpAccount) => {
    if (secrets[account.id] !== undefined) {
      setSecrets((prev) => {
        const next = { ...prev };
        delete next[account.id];
        return next;
      });
      return;
    }
    try {
      const secret = await totpSecret(account.id);
      setSecrets((prev) => ({ ...prev, [account.id]: secret }));
    } catch (e) {
      message.error(errText(e));
    }
  };

  return (
    <div className="page">
      <div className="toolbar">
        <Button type="text" icon={<ArrowLeft size={16} />} onClick={onBack}>
          工具
        </Button>
        <h2 className="toolbar-title">2FA 认证管理</h2>
        <Space>
          <SearchBox placeholder="搜索服务方 / 账号" value={keyword} onChange={setKeyword} />
          <Button type="primary" icon={<Plus size={16} />} onClick={() => setEdit({ ...emptyInput })}>
            添加账户
          </Button>
          <Button icon={<Download size={15} />} onClick={() => setBackup("export")}>
            加密备份
          </Button>
          <Button icon={<Upload size={15} />} onClick={() => setBackup("import")}>
            恢复备份
          </Button>
        </Space>
      </div>

      <div className="content-pane">
        {loading && accounts.length === 0 ? (
          <div style={{ display: "flex", justifyContent: "center", padding: 40 }}>
            <Spin />
          </div>
        ) : visible.length === 0 ? (
          <Empty description="还没有 2FA 账户，点击「添加账户」录入 GitHub / Google 的密钥" />
        ) : (
          <div className="totp-grid">
            {visible.map((a) => {
              const entry = entries[a.id];
              const remaining = a.period - (now % a.period);
              const secret = secrets[a.id];
              const openEditor = () =>
                setEdit({
                  id: a.id,
                  issuer: a.issuer,
                  account: a.account,
                  secret: "",
                  algorithm: a.algorithm,
                  digits: a.digits,
                  period: a.period,
                });
              const confirmRemove = () =>
                ModalConfirm({
                  title: `确认删除 ${a.issuer} 的 2FA 账户？下次登录需要重新绑定密钥。`,
                  onOk: () => remove(a.id),
                });
              const toggleSecret = () => handleToggleSecret(a);
              return (
                <div key={a.id} className="card totp-card">
                  <div className="totp-head">
                    <div className="totp-avatar">{a.issuer.slice(0, 1).toUpperCase()}</div>
                    <div className="stack" style={{ flex: 1, minWidth: 0 }}>
                      <div className="ellipsis totp-title">{a.issuer}</div>
                      <div className="ellipsis label">{a.account || "未填账号"}</div>
                    </div>
                    <Space size={4}>
                      {secret ? (
                        <EyeOff
                          className="icon-button"
                          size={15}
                          aria-label="隐藏密钥"
                          {...pressable(toggleSecret)}
                          onClick={toggleSecret}
                        />
                      ) : (
                        <Eye
                          className="icon-button"
                          size={15}
                          aria-label="显示密钥"
                          {...pressable(toggleSecret)}
                          onClick={toggleSecret}
                        />
                      )}
                      <Pencil
                        className="icon-button"
                        size={15}
                        aria-label={`编辑 ${a.issuer}`}
                        {...pressable(openEditor)}
                        onClick={openEditor}
                      />
                      <Trash2
                        className="icon-button"
                        size={15}
                        aria-label={`删除 ${a.issuer}`}
                        {...pressable(confirmRemove)}
                        onClick={confirmRemove}
                      />
                    </Space>
                  </div>

                  {entry?.error ? (
                    <div className="totp-error">{entry.error}</div>
                  ) : (
                    <div className="totp-code-row">
                      <div className="totp-code">{entry ? formatCode(entry.code) : "···  ···"}</div>
                      <Copy
                        className="icon-button"
                        size={16}
                        aria-label={`复制 ${a.issuer} 的验证码`}
                        {...pressable(() => handleCopyCode(a))}
                        onClick={() => handleCopyCode(a)}
                      />
                      <Countdown remaining={remaining} period={a.period} />
                    </div>
                  )}

                  {secret && (
                    <div
                      className="totp-secret"
                      title="点击复制密钥"
                      onClick={async () => {
                        if (await copyText(secret)) message.success("密钥已复制");
                      }}
                      {...pressable(() => {
                        void copyText(secret).then((ok) => ok && message.success("密钥已复制"));
                      })}
                    >
                      {secret}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      <AddDialog account={edit} onClose={() => setEdit(null)} />
      <BackupDialog mode={backup} onClose={() => setBackup(null)} />
    </div>
  );
}
