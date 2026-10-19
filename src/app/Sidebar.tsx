import { useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { AppWindow, FolderKanban, Globe, Moon, Puzzle, Settings, Sun } from "lucide-react";
import { useSettingsStore } from "../stores/settings";
import { Tooltip, pressable } from "../ui";
import SettingsModal from "../features/settings/SettingsModal";
// 侧栏顶部就用产品图标本身，不再另画一个字母标记：换图标时只换一处
import appIcon from "../../src-tauri/icons/128x128.png";

const NAV_ITEMS = [
  { to: "/", icon: AppWindow, label: "应用" },
  { to: "/bookmarks", icon: Globe, label: "网页管理" },
  { to: "/files", icon: FolderKanban, label: "文件管理" },
  { to: "/tools", icon: Puzzle, label: "工具" },
];

export default function Sidebar() {
  const theme = useSettingsStore((s) => s.theme);
  const toggleTheme = useSettingsStore((s) => s.toggleTheme);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const { pathname } = useLocation();
  const navigate = useNavigate();

  return (
    <div className="sidebar">
      <Tooltip title="个人工作台" placement="right">
        <img className="sidebar-logo" src={appIcon} alt="个人工作台" draggable={false} />
      </Tooltip>
      {NAV_ITEMS.map(({ to, icon: Icon, label }) => {
        const active = to === "/" ? pathname === "/" : pathname.startsWith(to);
        return (
          <Tooltip key={to} title={label} placement="right">
            <div
              className={active ? "nav-button active" : "nav-button"}
              aria-label={label}
              onClick={() => navigate(to)}
              {...pressable(() => navigate(to))}
            >
              <Icon size={20} strokeWidth={1.5} />
            </div>
          </Tooltip>
        );
      })}

      <div className="sidebar-footer">
        <Tooltip title={theme === "dark" ? "切换到浅色" : "切换到深色"} placement="right">
          <div
            className="nav-button"
            aria-label={theme === "dark" ? "切换到浅色" : "切换到深色"}
            onClick={toggleTheme}
            {...pressable(toggleTheme)}
          >
            {theme === "dark" ? <Sun size={20} strokeWidth={1.5} /> : <Moon size={20} strokeWidth={1.5} />}
          </div>
        </Tooltip>
        <Tooltip title="设置" placement="right">
          <div
            className={settingsOpen ? "nav-button active" : "nav-button"}
            aria-label="设置"
            onClick={() => setSettingsOpen(true)}
            {...pressable(() => setSettingsOpen(true))}
          >
            <Settings size={20} strokeWidth={1.5} />
          </div>
        </Tooltip>
      </div>

      <SettingsModal open={settingsOpen} onClose={() => setSettingsOpen(false)} />
    </div>
  );
}
