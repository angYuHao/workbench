import { useEffect } from "react";
import { RouterProvider } from "react-router-dom";
import { router } from "./app/router";
import { useAppearanceStore } from "./stores/appearance";
import { useSettingsStore } from "./stores/settings";
import { startSync } from "./stores/sync";
import { MessageContainer, ConfirmContainer } from "./ui";

export default function App() {
  const loadLocalPrefs = useSettingsStore((s) => s.loadLocalPrefs);
  const loadAppearance = useAppearanceStore((s) => s.load);

  useEffect(() => {
    loadLocalPrefs();
    loadAppearance();
  }, [loadLocalPrefs, loadAppearance]);

  // 挂着「跟随系统」时，系统换深浅要立刻重算一遍，不能等下次进设置才动
  useEffect(() => useAppearanceStore.getState().watchSystem(), []);

  // 别的窗口写了库要跟着重读。窗口各有一份 store，所以这里只订阅「哪块脏了」这一件事
  useEffect(() => startSync(), []);

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