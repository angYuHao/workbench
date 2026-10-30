import { useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { AppWindow, FolderKanban, Globe, Moon, Puzzle, Settings, Sun } from "lucide-react";
import { useAppearanceStore } from "../stores/appearance";
import { resolveMode } from "../styles/apply";
import { Tooltip, pressable } from "../ui";
import SettingsModal from "../features/settings/SettingsModal";

const NAV_ITEMS = [
  { to: "/", icon: AppWindow, label: "应用" },
  { to: "/bookmarks", icon: Globe, label: "网页管理" },
  { to: "/files", icon: FolderKanban, label: "文件管理" },
  { to: "/tools", icon: Puzzle, label: "工具" },
];

export default function Sidebar() {
  // 侧栏这颗只切深浅，不动风格本身；挂着「跟随系统」时按一下就把那档定成具体一边
  const mode = useAppearanceStore((s) => s.overrides.mode ?? s.userStyles.find((u) => u.id === s.styleId)?.seed.mode ?? "light");
  const setMode = useAppearanceStore((s) => s.setMode);
  const dark = resolveMode(mode) === "dark";
  const flip = () => void setMode(dark ? "light" : "dark");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const { pathname } = useLocation();
  const navigate = useNavigate();

  return (
    <div className="sidebar">
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
        <Tooltip title={dark ? "切换到浅色" : "切换到深色"} placement="right">
          <div className="nav-button" aria-label={dark ? "切换到浅色" : "切换到深色"} onClick={flip} {...pressable(flip)}>
            {dark ? <Sun size={20} strokeWidth={1.5} /> : <Moon size={20} strokeWidth={1.5} />}
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
