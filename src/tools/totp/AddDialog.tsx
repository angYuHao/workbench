import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { FileText, QrCode, ScanLine } from "lucide-react";
import { captureScreen } from "../../api/screen";
import * as api from "../../api/totp";
import { useTotpStore } from "../../stores/totp";
import { Button, Form, FormItem, Input, Modal, Select, Space, TextArea, message, useForm } from "../../ui";
import { errText } from "../../utils/error";
import type { TotpAccountInput, TotpAlgorithm } from "../../types";
import { isBase32, normalizeSecret, parseImportText } from "./import";
import type { ImportEntry } from "./import";
import { parseOtpauth } from "./otpauth";
import type { OtpauthFields } from "./otpauth";
import { base64ToPngBlob, decodeQr } from "../shared/qr";

const ALGORITHM_OPTIONS: { label: string; value: TotpAlgorithm }[] = [
  { label: "SHA1", value: "sha1" },
  { label: "SHA256", value: "sha256" },
  { label: "SHA512", value: "sha512" },
];

const DIGIT_OPTIONS = [6, 7, 8].map((d) => ({ label: `${d} 位`, value: d }));
const PERIOD_OPTIONS = [15, 30, 60].map((p) => ({ label: `${p} 秒`, value: p }));

export const emptyInput: TotpAccountInput = {
  id: 0,
  issuer: "",
  account: "",
  secret: "",
  algorithm: "sha1",
  digits: 6,
  period: 30,
};

type Mode = "screen" | "image" | "manual" | "import";

const MODES: { key: Mode; label: string }[] = [
  { key: "screen", label: "截图扫描" },
  { key: "image", label: "图片识别" },
  { key: "manual", label: "手动输入" },
  { key: "import", label: "文本导入" },
];

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div style={{ marginBottom: 12 }}>
      <div className="label" style={{ marginBottom: 4 }}>
        {label}
      </div>
      {children}
    </div>
  );
}

interface AddDialogProps {
  account: TotpAccountInput | null;
  onClose: () => void;
}

export default function AddDialog({ account, onClose }: AddDialogProps) {
  const { save, load } = useTotpStore();
  const [form] = useForm();
  const [mode, setMode] = useState<Mode>("screen");
  const [otpauth, setOtpauth] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [rows, setRows] = useState<ImportEntry[] | null>(null);
  const [unchecked, setUnchecked] = useState<Record<number, boolean>>({});

  useEffect(() => {
    if (!account) return;
    setMode(account.id ? "manual" : "screen");
    setOtpauth("");
    setMsg(null);
    setRows(null);
    setUnchecked({});
    form.setFieldsValue({
      issuer: account.issuer,
      account: account.account,
      // 密钥密文不回显，留空表示保持原密钥不变
      secret: "",
      algorithm: account.algorithm,
      digits: account.digits,
      period: account.period,
    });
  }, [account, form]);

  const checked = (entry: ImportEntry, index: number) => !entry.error && !unchecked[index];
  const picked = useMemo(
    () => (rows ?? []).filter((entry, i) => !entry.error && !unchecked[i]),
    [rows, unchecked],
  );

  const applyParsed = (parsed: OtpauthFields) => {
    form.setFieldsValue({
      issuer: parsed.issuer,
      account: parsed.account,
      secret: parsed.secret,
      algorithm: parsed.algorithm,
      digits: parsed.digits,
      period: parsed.period,
    });
  };

  // 识别结果可能是完整 otpauth 链接，也可能只是裸密钥；填好后回到手动输入让用户确认
  const absorb = (text: string, from: string) => {
    const parsed = parseOtpauth(text);
    if (parsed) {
      setOtpauth(text);
      applyParsed(parsed);
      setMode("manual");
      setMsg({
        ok: true,
        text: `${from}已识别：${parsed.issuer || "（无服务方）"}${parsed.account ? ` / ${parsed.account}` : ""}`,
      });
      return;
    }
    const bare = normalizeSecret(text);
    if (isBase32(bare)) {
      form.setFieldValue("secret", bare);
      setMode("manual");
      setMsg({ ok: true, text: `${from}识别到裸密钥，请补上服务方名称` });
      return;
    }
    setMsg({ ok: false, text: `${from}的内容不是 2FA 配置：${text.slice(0, 40)}` });
  };

  const handleOtpauth = (value: string) => {
    setOtpauth(value);
    const parsed = parseOtpauth(value);
    if (parsed) applyParsed(parsed);
  };

  // 后端会先隐藏本窗口再抓屏，扫到就直接填表，用户不用离开键盘
  const handleScreen = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const png = await captureScreen();
      const text = await decodeQr(base64ToPngBlob(png));
      if (!text) setMsg({ ok: false, text: "屏幕上没找到二维码，先把站点的二维码显示出来再扫" });
      else absorb(text, "屏幕");
    } catch (e) {
      setMsg({ ok: false, text: errText(e) });
    } finally {
      setBusy(false);
    }
  };

  const handleImage = async (file?: File | null) => {
    if (!file) return;
    setBusy(true);
    setMsg(null);
    try {
      const text = await decodeQr(file);
      if (!text) setMsg({ ok: false, text: "图里没识别出二维码，换一张清晰一些的图片" });
      else absorb(text, "图片");
    } catch (e) {
      setMsg({ ok: false, text: errText(e) });
    } finally {
      setBusy(false);
    }
  };

  // 停在「图片识别」时，截完屏直接 Ctrl+V 也能识别
  useEffect(() => {
    if (!account || account.id || mode !== "image") return;
    const onPaste = (e: ClipboardEvent) => {
      const file = Array.from(e.clipboardData?.files ?? []).find((f) => f.type.startsWith("image/"));
      if (file) handleImage(file);
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, [account, mode]);

  const handleImportFile = async (file?: File | null) => {
    if (!file) return;
    setBusy(true);
    setMsg(null);
    setRows(null);
    setUnchecked({});
    try {
      const entries = parseImportText(await file.text());
      if (!entries.length) {
        setMsg({ ok: false, text: "文件里没有可解析的内容" });
        return;
      }
      const bad = entries.filter((e) => e.error).length;
      setRows(entries);
      setMsg(
        bad
          ? { ok: false, text: `读到 ${entries.length} 行，其中 ${bad} 行有问题，已默认不勾选` }
          : { ok: true, text: `读到 ${entries.length} 条，确认无误后点右下角导入` },
      );
    } catch (e) {
      setMsg({ ok: false, text: errText(e) });
    } finally {
      setBusy(false);
    }
  };

  const importRows = async () => {
    if (!rows) return;
    if (!picked.length) {
      message.error("先勾选要导入的条目");
      return;
    }
    setBusy(true);
    let done = 0;
    const failed: string[] = [];
    for (const entry of picked) {
      try {
        await api.saveTotpAccount({
          id: 0,
          issuer: entry.issuer,
          account: entry.account,
          secret: entry.secret,
          algorithm: entry.algorithm,
          digits: entry.digits,
          period: entry.period,
        });
        done += 1;
      } catch (e) {
        failed.push(`第 ${entry.line} 行：${errText(e)}`);
      }
    }
    await load();
    setBusy(false);
    if (failed.length) {
      message.error(`已导入 ${done} 条，${failed.length} 条失败：${failed[0]}`);
    } else {
      message.success(`已导入 ${done} 条`);
      onClose();
    }
  };

  const saveManual = async () => {
    const values = await form.validateFields();
    const next: TotpAccountInput = { ...(account ?? emptyInput), ...values };
    if (!next.issuer.trim()) {
      message.error("请填写服务方，例如 GitHub 或 Google");
      return;
    }
    if (!next.id && !next.secret.trim()) {
      message.error("请粘贴 otpauth:// 链接，或手动填写密钥");
      return;
    }
    try {
      await save({ ...next, secret: next.secret.trim() });
      message.success("已保存");
      onClose();
    } catch (e) {
      message.error(errText(e));
    }
  };

  const isEdit = !!account?.id;

  return (
    <Modal
      title={isEdit ? "编辑 2FA 账户" : "添加 2FA 账户"}
      open={!!account}
      onCancel={onClose}
      onOk={() => (mode === "import" ? importRows() : saveManual())}
      okText={mode === "import" ? `导入 ${picked.length} 条` : "保存"}
    >
      {!isEdit && (
        <div className="mode-tabs">
          {MODES.map((m) => (
            <button
              key={m.key}
              type="button"
              className={m.key === mode ? "mode-tab active" : "mode-tab"}
              disabled={busy}
              onClick={() => {
                setMode(m.key);
                setMsg(null);
              }}
            >
              {m.label}
            </button>
          ))}
        </div>
      )}

      {msg && <div className={msg.ok ? "qr-msg ok" : "qr-msg err"}>{msg.text}</div>}

      <Form form={form}>
        {mode === "screen" && (
          <div className="mode-panel">
            <div className="label">
              先在浏览器或手机上把站点给的二维码显示出来，再点下面的按钮。本窗口会短暂隐藏让出屏幕，识别成功后自动填好表单。
            </div>
            <Button type="primary" icon={<ScanLine size={16} />} loading={busy} onClick={handleScreen}>
              扫描屏幕上的二维码
            </Button>
          </div>
        )}

        {mode === "image" && (
          <div className="mode-panel">
            <div className="qr-zone">
              <label className="qr-pick">
                <QrCode size={16} />
                选择二维码图片
                <input
                  type="file"
                  accept="image/*"
                  onChange={(e) => {
                    handleImage(e.target.files?.[0]);
                    e.target.value = "";
                  }}
                />
              </label>
              <span className="label">{busy ? "正在识别…" : "也可以把二维码截图 Ctrl+V 粘贴进来"}</span>
            </div>
          </div>
        )}

        {mode === "manual" && (
          <>
            {!isEdit && (
              <Field label="otpauth 链接（粘贴后自动填充下面的字段）">
                <TextArea
                  autoSize={{ minRows: 2 }}
                  placeholder="otpauth://totp/GitHub:alice?secret=XXXX&issuer=GitHub"
                  value={otpauth}
                  onChange={handleOtpauth}
                />
              </Field>
            )}
            <FormItem name="issuer" label="服务方">
              <Input placeholder="GitHub / Google" />
            </FormItem>
            <FormItem name="account" label="账号">
              <Input placeholder="alice@example.com" />
            </FormItem>
            <FormItem name="secret" label={isEdit ? "密钥（留空表示不修改）" : "密钥（base32）"}>
              <Input placeholder="站点给出的 16~32 位密钥" />
            </FormItem>
            <Space align="start">
              <FormItem name="algorithm" label="算法">
                <Select options={ALGORITHM_OPTIONS} style={{ width: 110 }} />
              </FormItem>
              <FormItem name="digits" label="位数">
                <Select options={DIGIT_OPTIONS} style={{ width: 100 }} />
              </FormItem>
              <FormItem name="period" label="周期">
                <Select options={PERIOD_OPTIONS} style={{ width: 100 }} />
              </FormItem>
            </Space>
          </>
        )}

        {mode === "import" && (
          <div className="mode-panel">
            <div className="qr-zone">
              <label className="qr-pick">
                <FileText size={16} />
                选择文本文件
                <input
                  type="file"
                  accept=".txt,.csv,text/plain"
                  onChange={(e) => {
                    handleImportFile(e.target.files?.[0]);
                    e.target.value = "";
                  }}
                />
              </label>
              <span className="label">{busy ? "正在解析…" : "一行一条：otpauth 链接，或「服务方,账号,密钥」"}</span>
            </div>
            {rows && (
              <div className="import-list">
                {rows.map((entry, i) => (
                  <label key={entry.line} className={entry.error ? "import-row bad" : "import-row"}>
                    <input
                      type="checkbox"
                      checked={checked(entry, i)}
                      disabled={!!entry.error}
                      onChange={() => setUnchecked((prev) => ({ ...prev, [i]: !prev[i] }))}
                    />
                    <span className="import-row-name">
                      {entry.issuer || "（无服务方）"}
                      {entry.account ? ` / ${entry.account}` : ""}
                    </span>
                    <span className="import-row-note">
                      {entry.error ?? `${entry.secret} · ${entry.digits} 位 / ${entry.period} 秒`}
                    </span>
                  </label>
                ))}
              </div>
            )}
          </div>
        )}
      </Form>

      <div className="label">密钥经当前 Windows 用户加密后存进本地数据库，不联网、不上传。</div>
    </Modal>
  );
}
