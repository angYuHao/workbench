import { useEffect } from "react";
import { RouterProvider } from "react-router-dom";
import { router } from "./app/router";
import { useSettingsStore } from "./stores/settings";
import { MessageContainer, ConfirmContainer, message } from "./ui";

export default function App() {
  const theme = useSettingsStore((s) => s.theme);
  const loadLocalPrefs = useSettingsStore((s) => s.loadLocalPrefs);
  const loadWakeHotkey = useSettingsStore((s) => s.loadWakeHotkey);

  useEffect(() => {
    loadLocalPrefs();
    // 热键装载失败要说出来：默认键被占的话，用户以为有热键其实没有，比没提示更糟
    loadWakeHotkey().catch((e) => message.error(String(e)));
  }, [loadLocalPrefs, loadWakeHotkey]);

  useEffect(() => {
    document.body.classList.toggle("dark", theme === "dark");
  }, [theme]);

  // 原生拖放拦截关掉之后（页面内的 HTML5 拖动才动得起来），从资源管理器拖文件进来
  // 就落回浏览器默认行为——把文件当链接打开、整个窗口跳走。这类拖放一律吞掉。
  // 只针对文件：自家卡片拖动要靠「不落就在别处出现禁止符」来提示有效落点。
  useEffect(() => {
    const swallowFiles = (e: DragEvent) => {
      if (Array.from(e.dataTransfer?.types ?? []).includes("Files")) e.preventDefault();
    };
    document.addEventListener("dragover", swallowFiles, true);
    document.addEventListener("drop", swallowFiles, true);
    return () => {
      document.removeEventListener("dragover", swallowFiles, true);
      document.removeEventListener("drop", swallowFiles, true);
    };
  }, []);

  return (
    <>
      <RouterProvider router={router} />
      <MessageContainer />
      <ConfirmContainer />
    </>
  );
}