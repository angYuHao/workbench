import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { exportTotpBackup, importTotpBackup } from "../../api/totp";
import { useTotpStore } from "../../stores/totp";
import { Input, Modal, message } from "../../ui";
import { errText } from "../../utils/error";

export type BackupMode = "export" | "import";

const FILE_FILTERS = [{ name: "2FA 加密备份", extensions: ["json"] }];

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

function Hint({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        marginTop: 12,
        padding: "8px 10px",
        borderRadius: "var(--radius-control)",
        background: "var(--color-bg)",
        color: "var(--color-text-secondary)",
        fontSize: 12,
        lineHeight: 1.6,
      }}
    >
      {children}
    </div>
  );
}

export default function BackupDialog({ mode, onClose }: { mode: BackupMode | null; onClose: () => void }) {
  const { load } = useTotpStore();
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    setPassphrase("");
    setConfirm("");
    setBusy(false);
  }, [mode]);

  const handleOk = async () => {
    if (busy || !mode) return;
    if (!passphrase) {
      message.error(mode === "export" ? "请设置一个备份口令" : "请输入备份口令");
      return;
    }
    if (mode === "export" && passphrase !== confirm) {
      message.error("两次输入的口令不一致");
      return;
    }

    setBusy(true);
    try {
      if (mode === "export") {
        const dest = await saveDialog({
          title: "保存加密备份",
          defaultPath: "workbench-totp-backup.json",
          filters: FILE_FILTERS,
        });
        if (!dest) return;
        const count = await exportTotpBackup(dest, passphrase);
        message.success(`已加密导出 ${count} 个账户`);
      } else {
        const src = await openDialog({ title: "选择备份文件", filters: FILE_FILTERS });
        if (typeof src !== "string") return;
        const report = await importTotpBackup(src, passphrase);
        message.success(
          report.skipped > 0
            ? `已导入 ${report.added} 个账户，跳过 ${report.skipped} 个已存在的`
            : `已导入 ${report.added} 个账户`,
        );
        await load();
      }
      onClose();
    } catch (e) {
      message.error(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={mode === "export" ? "加密导出 2FA 账户" : "从加密备份恢复"}
      open={mode !== null}
      onCancel={onClose}
      onOk={handleOk}
      okText={mode === "export" ? "选择保存位置" : "选择备份文件"}
      width={440}
    >
      <Field label="备份口令">
        <Input
          type="password"
          value={passphrase}
          onChange={setPassphrase}
          onEnter={handleOk}
          placeholder={mode === "export" ? "至少 8 位，自己定一句记得住的话" : "导入时输入导出时设的口令"}
        />
      </Field>
      {mode === "export" && (
        <Field label="再输一遍">
          <Input
            type="password"
            value={confirm}
            onChange={setConfirm}
            onEnter={handleOk}
            placeholder="口令错了不会报错，只会让你再也打不开"
          />
        </Field>
      )}
      <Hint>
        {mode === "export" ? (
          <>
            备份文件里只有密文，能打开它的唯一凭据就是你输入的这个口令——<b>忘记口令无法找回</b>。
            文件不含任何本机信息，可以在别的电脑上用同一个口令导入。
          </>
        ) : (
          <>
            解密要靠口令，可能要等零点几秒。库里已有的「服务方 + 账号」相同账户会跳过而不是覆盖，
            重复导入是安全的。
          </>
        )}
      </Hint>
    </Modal>
  );
}
