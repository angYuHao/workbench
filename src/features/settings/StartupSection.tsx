import { ChevronUp, Minimize2, Rocket } from "lucide-react";
import { Switch, message } from "../../ui";
import { useSettingsStore } from "../../stores/settings";
import { SettingsLine } from "./shared";

/** 开关状态和落库都在 settings store 里，这里只补一句结果提示 */
export default function StartupSection() {
  const { autostart, closeToTray, edgeDrawer, toggleAutostart, toggleCloseToTray, toggleEdgeDrawer } =
    useSettingsStore();

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

  const handleEdgeDrawer = async (on: boolean) => {
    try {
      await toggleEdgeDrawer(on);
      message.success(on ? "已开启：鼠标顶到屏幕上方就叫出工作台" : "已关闭：窗口回到带标题栏的居中模式");
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
      <SettingsLine
        icon={ChevronUp}
        title="顶边唤出抽屉"
        desc="鼠标顶到屏幕最上沿，从顶边中间滑出一块搜索面板：左边全部应用、右边全部网页，各自滚动，直接打字就能搜。占屏幕宽度的三分之一、高度五分之二，横向居中，所以不会压住别人窗口的标题栏。它是可交互的：划出来就拿到焦点（否则没法打字），光标离开一会儿自己收回去。全屏播放、打游戏、放映 PPT 时划过顶边照样出"
      >
        <Switch checked={edgeDrawer} onChange={handleEdgeDrawer} />
      </SettingsLine>
    </div>
  );
}
