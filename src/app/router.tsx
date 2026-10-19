import { lazy } from "react";
import { createHashRouter } from "react-router-dom";
import AppLayout from "./AppLayout";

/** 一个界面一个 chunk：文件页那套 markdown + 高亮 + 磁盘格式解析有小几百 kB，不该跟着启动一起加载 */
const AppsView = lazy(() => import("../features/apps/AppsView"));
const BookmarksView = lazy(() => import("../features/bookmarks/BookmarksView"));
const FilesView = lazy(() => import("../features/files/FilesView"));
const ToolsView = lazy(() => import("../tools/ToolsView"));

export const router = createHashRouter([
  {
    path: "/",
    element: <AppLayout />,
    children: [
      { index: true, element: <AppsView /> },
      { path: "bookmarks", element: <BookmarksView /> },
      { path: "files", element: <FilesView /> },
      { path: "tools", element: <ToolsView /> },
      { path: "tools/:toolId", element: <ToolsView /> },
    ],
  },
]);
