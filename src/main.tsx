import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import DrawerApp from "./app/DrawerApp";
import "./styles/global.css";

/** 顶边抽屉不套路由：它没有侧栏也没有页面可跳，走 router 会把侧栏和四个 view 的 chunk 一起拖起来。
 *  浏览器里（直接开 1420）取不到窗口 label，所以留了 `#drawer-preview` 这个手动开关 ——
 *  不然抽屉长什么样只能在打包出来的窗口里看。 */
function isDrawerWindow(): boolean {
  if (location.hash === "#drawer-preview") return true;
  try {
    return getCurrentWindow().label === "drawer";
  } catch {
    return false;
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isDrawerWindow() ? <DrawerApp /> : <App />}</React.StrictMode>,
);
