import { Download, Upload } from "lucide-react";
import { Button, message } from "../../ui";
import { exportDb, restoreDb } from "../../api/settings";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { SettingsLine } from "./shared";

/** 导出/恢复都是「先弹系统对话框再落库」，不碰设置窗口里的任何状态 */
export default function DataSection() {
  const handleExport = async () => {
    const path = await saveDialog({
      title: "导出数据",
      defaultPath: "workbench-backup.db",
      filters: [{ name: "SQLite 数据库", extensions: ["db"] }],
    });
    if (path) {
      await exportDb(path);
      message.success("已导出数据");
    }
  };

  const handleRestore = async () => {
    const path = await openDialog({
      title: "选择备份文件",
      filters: [{ name: "SQLite 数据库", extensions: ["db"] }],
    });
    if (typeof path === "string") {
      await restoreDb(path);
      message.success("已恢复，正在刷新...");
      setTimeout(() => window.location.reload(), 800);
    }
  };

  return (
    <div className="settings-card">
      <SettingsLine
        icon={Download}
        title="导出备份"
        desc="把整个数据库另存为一个 .db 文件，含账户、书签与整理记录"
      >
        <Button icon={<Download size={15} />} onClick={handleExport}>
          导出
        </Button>
      </SettingsLine>
      <SettingsLine
        icon={Upload}
        title="恢复备份"
        desc="用选中的 .db 覆盖当前数据，完成后界面自动刷新"
      >
        <Button icon={<Upload size={15} />} onClick={handleRestore}>
          恢复
        </Button>
      </SettingsLine>
    </div>
  );
}
