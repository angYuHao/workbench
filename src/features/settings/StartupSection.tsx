import { Minimize2, Rocket } from "lucide-react";
import { Switch, message } from "../../ui";
import { useSettingsStore } from "../../stores/settings";

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
      <div className="settings-line">
        <span className="settings-line-icon">
          <Rocket size={16} strokeWidth={1.6} />
        </span>
        <div className="settings-line-text">
          <div className="settings-line-title">开机自启动</div>
          <div className="settings-line-desc">登录 Windows 后自动运行，写在注册表启动项里</div>
        </div>
        <Switch checked={autostart} onChange={handleAutostart} />
      </div>
      <div className="settings-line">
        <span className="settings-line-icon">
          <Minimize2 size={16} strokeWidth={1.6} />
        </span>
        <div className="settings-line-text">
          <div className="settings-line-title">关闭时放入系统托盘</div>
          <div className="settings-line-desc">
            点窗口关闭键只是隐藏，托盘图标左键或右键菜单里的「显示主窗口」唤回；要真退出走托盘菜单的「退出工作台」
          </div>
        </div>
        <Switch checked={closeToTray} onChange={handleTray} />
      </div>
    </div>
  );
}
