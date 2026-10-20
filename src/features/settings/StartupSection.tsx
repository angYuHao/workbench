import { Minimize2, Rocket } from "lucide-react";
import { Switch, message } from "../../ui";
import { useSettingsStore } from "../../stores/settings";
import { SettingsLine } from "./shared";

/** 开关状态和落库都在 settings store 里，这里只补一句结果提示 */
export default function StartupSection() {
  const { autostart, closeToTray, toggleAutostart, toggleCloseToTray } = useSettingsStore();

  const handleAutostart = async (on: boolean) => {
    try {
      await toggleAutostart(on);
      message.success(on ? "已开启开机自启" : "已关闭开机自启");
    } catch (e) {
      message.error(String(e));
    }
  };

  const handleTray = async (on: boolean) => {
    try {
      await toggleCloseToTray(on);
      message.success(on ? "已开启：关闭窗口会收进托盘" : "已关闭：关闭窗口即退出");
    } catch (e) {
      message.error(String(e));
    }
  };

  return (
    <div className="settings-card">
      <SettingsLine
        icon={Rocket}
        title="开机自启动"
        desc="登录 Windows 后自动运行，写在注册表启动项里"
      >
        <Switch checked={autostart} onChange={handleAutostart} />
      </SettingsLine>
      <SettingsLine
        icon={Minimize2}
        title="关闭时放入系统托盘"
        desc="点窗口关闭键只是隐藏，托盘图标左键或右键菜单里的「显示主窗口」唤回；要真退出走托盘菜单的「退出工作台」"
      >
        <Switch checked={closeToTray} onChange={handleTray} />
      </SettingsLine>
    </div>
  );
}
