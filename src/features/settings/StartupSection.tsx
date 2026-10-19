import { useEffect, useState } from "react";
import { Keyboard, Minimize2, Rocket } from "lucide-react";
import { Button, Space, Switch, message } from "../../ui";
import { useSettingsStore } from "../../stores/settings";

const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "ShiftLeft",
  "ShiftRight",
  "MetaLeft",
  "MetaRight",
]);

/** 没有单字母对应的键得换个名字，否则后端那套解析认不出来 */
const PLAIN_KEYS: Record<string, string> = {
  Space: "Space",
  Enter: "Enter",
  Tab: "Tab",
  Backspace: "Backspace",
  Insert: "Insert",
  Delete: "Delete",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
};

/**
 * 把一次按键拼成后端插件认的写法。两条实测约束：
 * 修饰键别名里没有 `WIN`，只能写 `Super`；只按修饰键不算一个组合键
 * （`Ctrl+Alt` 本身就是 AltGr，录进去会打断打字）。
 */
function comboOf(e: KeyboardEvent): string | null {
  const mods = [
    e.ctrlKey && "Ctrl",
    e.altKey && "Alt",
    e.shiftKey && "Shift",
    e.metaKey && "Super",
  ].filter(Boolean);
  if (!mods.length || MODIFIER_CODES.has(e.code)) return null;
  const main =
    /^Key([A-Z])$/.exec(e.code)?.[1] ??
    /^Digit(\d)$/.exec(e.code)?.[1] ??
    (/^F(\d{1,2})$/.test(e.code) ? e.code : null) ??
    PLAIN_KEYS[e.code] ??
    null;
  return main ? `${[...mods, main].join("+")}` : null;
}

/** 点一下变成「等按键」，按完就交给后端注册；Esc 只是取消录入，不是清空热键 */
function HotkeyRecorder({
  value,
  onPick,
  onClear,
}: {
  value: string;
  onPick: (key: string) => void;
  onClear: () => void;
}) {
  const [listening, setListening] = useState(false);

  useEffect(() => {
    if (!listening) return;
    const onKey = (e: KeyboardEvent) => {
      // 捕获阶段先拦住：别让弹窗把 Esc 当成「关闭设置」，也别让空格回头又点到这个按钮
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setListening(false);
        return;
      }
      if (e.repeat) return;
      const combo = comboOf(e);
      if (!combo) return;
      setListening(false);
      onPick(combo);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [listening, onPick]);

  return (
    <Space size={6}>
      <Button size="small" onClick={() => setListening((v) => !v)}>
        {listening ? "按下组合键…" : value || "点这里设置"}
      </Button>
      {!!value && !listening && (
        <Button type="link" size="small" danger onClick={onClear}>
          清除
        </Button>
      )}
    </Space>
  );
}

/** 开关状态和落库都在 settings store 里，这里只补一句结果提示 */
export default function StartupSection() {
  const { autostart, closeToTray, wakeHotkey, toggleAutostart, toggleCloseToTray, bindWakeHotkey, dropWakeHotkey } =
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

  const handleBind = async (key: string) => {
    try {
      await bindWakeHotkey(key);
      message.success(`唤醒快捷键已设为 ${key}`);
    } catch (e) {
      // 被别的程序占了就照实说，别留下「设了但没生效」
      message.error(String(e));
    }
  };

  const handleClear = async () => {
    try {
      await dropWakeHotkey();
      message.success("已清除唤醒快捷键");
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
      <div className="settings-line">
        <span className="settings-line-icon">
          <Keyboard size={16} strokeWidth={1.6} />
        </span>
        <div className="settings-line-text">
          <div className="settings-line-title">唤醒快捷键</div>
          <div className="settings-line-desc">
            窗口收进托盘之后按一下就回来。点右边的框、再按一个组合键就换好了；必须带一个修饰键，
            光按 Ctrl/Alt/Shift 不算。默认 Alt+Space，注册不上就是被别的程序占了，换一个就行
          </div>
        </div>
        <HotkeyRecorder value={wakeHotkey} onPick={(k) => void handleBind(k)} onClear={() => void handleClear()} />
      </div>
    </div>
  );
}
