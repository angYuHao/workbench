# 前端（src）

```
main.tsx / App.tsx    挂载与路由入口
app/                  外壳：AppLayout、Sidebar（底部是设置入口）、router.tsx 路由表
features/apps/        AppsView（「应用」页：开始菜单同步进来的程序）+ AppIconGrid
features/bookmarks/   BookmarksView + BookmarkAnalysisModal + useBookmarkAnalysis
features/files/       FilesView + PreviewPanel / OfficePreview / TextPreview / PreviewError
features/settings/    SettingsModal + 各个 Section + 编辑弹窗
shared/               两个以上功能共用的界面件与 hook（CategoryTree、useCategoryScaffold、SelectionBar、SmartCategorizeModal、useIdSelection…）
tools/                工具箱，一个工具一个目录 → 见 tools/README.md
office/               读磁盘格式的解析层（docx/xlsx/xls/doc/艺术字）→ 见 office/README.md
api/                  唯一允许调 invoke 的地方，一个后端域一个文件
stores/               zustand，每个功能一个 store
ui/                   自研组件库：Button Table Modal Select Input Form Tooltip…
types/                按功能域分文件，index.ts 只做汇总
styles/               global.css 只做 @import；剩下的按界面切成 8 个文件
utils/                真·通用：clipboard、error、fileKind、textFile
```

## 约定

- 路由页叫 `*View.tsx`，工具内组件叫 `*Tool.tsx`，其余组件 PascalCase、hook 以 `use` 开头。
- `features/<功能>/` 内部的文件互相用 `./`，**不要横向 import 别的 feature**；要共用就提到 `shared/`。`app/` 是唯一允许指名引各 feature 的地方（路由表在那儿）。
- `styles/` 里 `global.css` 的 `@import` 顺序就是层叠顺序，别重排；某一层的样式改对应那个文件，找不到归属才写进 `misc.css`。
- `api/<域>.ts` 的文件名对齐后端 `commands/<域>.rs`；**不要在组件里直接 `invoke`**，一律走 `api/`。
- `types/` 的对面是 `src-tauri/src/models/`：字段名和可空性两边必须一致（后端 `rename_all = "camelCase"`）。少一个字段就是运行时 `undefined`，所以不要用「标成可选」去迁就后端缺字段。
- 组件库只用 `ui/` 里自研的那套，不引第三方 UI 库；样式改 `styles/global.css`，颜色一律走 `var(--color-*)`，别写死色值（否则深色模式漏网）。
- **打开一律双击**，单击只留给「选择」模式里的勾选。以后新增任何列表或网格都按这个口径来。
- **键盘那一半不能省**：只要不是原生 `<button>` 又要能按，就 spread `ui/pressable(onActivate)`（Enter/空格 + `tabIndex` + role），焦点圈由 `styles/layout.css` 顶部的 `:focus-visible` 统一画。双击打开的卡片，Enter 就等于那一下双击。分类树的建树/新建/改名/拖拽归类走 `shared/useCategoryScaffold`，删除流程两边不一样，留在各自视图里。

## `@/` 别名现在不能用

`tsconfig.json` 里原有的 `paths: { "@/*": ["src/*"] }` 已经删掉，因为 `vite.config.ts` 里**没有**对应的 `resolve.alias` —— 谁写 `@/xxx` 谁在 dev/build 时报「Failed to resolve import」，而编辑器不报错。

要启用必须两处一起加：`tsconfig.json` 的 `paths` + `vite.config.ts` 的 `resolve.alias`。只加一处比不加更糟。

## 验证

改完跑 `npx tsc --noEmit`（类型）和 `npx vite build`（打包）。这两道**都不代表界面正确**：交互改动要在 `npm run tauri dev` 的窗口里点一遍，尤其是拖拽、双击、弹窗这三类 —— WebView2 和浏览器对 HTML5 拖放的行为不一样。
