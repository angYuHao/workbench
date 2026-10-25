import { useEffect } from "react";
import { useAppearanceStore } from "../stores/appearance";
import { startSync } from "../stores/sync";
import DrawerStrip from "../features/drawer/DrawerStrip";
import { MessageContainer } from "../ui";

/** 抽屉窗口的入口壳：只要主题、数据订阅和一句出错提示，不要侧栏、路由和那四个 view。
 *
 *  `data-window` 这个标记是给 drawer.css 用的：全局那条 `body { background: var(--color-bg) }`
 *  落在透明窗口上，就是一个压在屏幕顶部的不透明长方形。 */
export default function DrawerApp() {
  useEffect(() => {
    document.documentElement.dataset.window = "drawer";
    void useAppearanceStore.getState().load();
    return startSync();
  }, []);

  return (
    <>
      <DrawerStrip />
      {/* 抽屉是可交互的，点开失败总得有个地方说话 */}
      <MessageContainer />
    </>
  );
}
