# 前端（src）

```
main.tsx / App.tsx    挂载与路由入口。main.tsx 先看窗口 label：是 `drawer` 就换挂 DrawerApp，不套路由
app/                  外壳：AppLayout、Sidebar（底部是设置入口）、router.tsx 路由表、DrawerApp（抽屉窗口的壳）
features/apps/        AppsView（「应用」页：注册表里的已安装程序）+ AppIconGrid
features/bookmarks/   BookmarksView + BookmarkAnalysisModal + useBookmarkAnalysis
features/drawer/      DrawerStrip —— 顶边那块搜索面板（全部应用 / 全部网页两栏、各自滚、可打字可点开）
features/files/       FilesView（多栏 × 每栏多标签 × 每标签一串 Miller 列，浏览整台机器）+ FileHeader（页级表头：面包屑地址行在上、标签行在下，横跨整块内容区并跟着当前栏走；点空白或 Ctrl+L 可输入整条路径）+ FilePanel（一栏：列区/状态条，排序和批量动作都收进右键菜单）+ FileColumn（一列：目录内容，不带头行）+ listing（筛和排）+ useFileActions（删·拷·搬·刷新的共同入口，右键菜单和键盘都走它）+ PreviewPanel（选中具体文件才弹出的右栏）/ OfficePreview / TextPreview / PreviewError
features/settings/    SettingsModal + 各个 Section + 编辑弹窗
shared/               两个以上功能共用的界面件与 hook（CategoryTree、useCategoryScaffold、SelectionBar、SmartCategorizeModal、useIdSelection、useSmartScope、dragItem…）
tools/                工具箱，一个工具一个目录 → 见 tools/README.md
office/               读磁盘格式的解析层（docx/xlsx/xls/doc/艺术字）→ 见 office/README.md
api/                  唯一允许调 invoke 的地方，一个后端域一个文件
stores/               zustand，每个功能一个 store
ui/                   自研组件库：Button Table Modal Select Input SearchBox Form Tooltip…
types/                按功能域分文件，index.ts 只做汇总
styles/               global.css 只做 @import；剩下的按界面切成 9 个文件。颜色不在这里定 —— 见下面「颜色只有三个地方」
utils/                真·通用：clipboard、error、fileKind、textFile、paths（Windows 路径当字符串处理）、humanize、sort（排序档位和标签）
```

## 约定

- 路由页叫 `*View.tsx`，工具内组件叫 `*Tool.tsx`，其余组件 PascalCase、hook 以 `use` 开头。
- `features/<功能>/` 内部的文件互相用 `./`，**不要横向 import 别的 feature**；要共用就提到 `shared/`。`app/` 是唯一允许指名引各 feature 的地方（路由表在那儿）。
- `styles/` 里 `global.css` 的 `@import` 顺序就是层叠顺序，别重排；某一层的样式改对应那个文件，找不到归属才写进 `misc.css`。
- `api/<域>.ts` 的文件名对齐后端 `commands/<域>.rs`；**不要在组件里直接 `invoke`**，一律走 `api/`。
- 分两种调用：**只读用 `invoke`，会改库的用 `api/notify.ts` 的 `write(cmd, domain, args)`**。`write` 在命令成功后补一发 `data-changed` 广播，别的窗口按域重读（订阅在 `stores/sync.ts`）。新增写命令忘了给域名不会报错，只会让另一个窗口静悄悄显示旧数据 —— 所以域名就写在调用那一行上，别藏进公共封装里。
  两个例外是 `openUrl` / `launchApp`：只记一次使用数，惊动别的窗口没有意义。
- 跨窗口同步只传「哪块脏了」，不传数据：库是唯一真相，各窗口的 store 都是缓存视图。所以丢一次广播不会看到错的数据，只会晚一步。**loader 只许写数据片**，滚动位置、选中项、输入草稿这些视图片不在 load* 里，否则一次广播就把用户正在填的东西冲没了。
- `types/` 的对面是 `src-tauri/src/models/`：字段名和可空性两边必须一致（后端 `rename_all = "camelCase"`）。少一个字段就是运行时 `undefined`，所以不要用「标成可选」去迁就后端缺字段。
- 组件库只用 `ui/` 里自研的那套，不引第三方 UI 库；样式改归属那一层的文件，找不到归属才写进 `misc.css`。
- **颜色只有三个地方**：`styles/seeds.ts` 出种子（一个风格 = 十来个字面量），`styles/derive.ts` 由种子算出四十多个界面变量，界面只读变量名。所以别写死色值，也别在 CSS 里自己 `mix()` 一份 —— 那不是来源，是第二份抄本，换风格时必漏。加新风格只在 `seeds.ts` 加一条；`tokens.css` 是 JS 还没跑起来那几毫秒的兜底表，值必须等于默认风格的派生输出，不要手改。`npm run lint:colors` 拦两件事：token 层之外的裸色值、以及引用了派生层没发的变量名。
- 外观偏好走 `stores/appearance.ts`（存 settings 那张 KV），首帧之前由 `public/appearance.js` 从 localStorage 镜像重放一次，免得存了深色的用户每次开机白一下。**这条只能在打包出来的 exe 里验**：`tauri dev` 不吃 `tauri.conf.json` 里那份 CSP，而那份 CSP 的 `script-src 'self'` 正是这段不能写成内联脚本的原因。
- **`Tooltip` 别放进 `overflow:auto` 的容器里**：浮层是绝对定位 + `white-space:nowrap`，会把那一栏的横向滚动区撑大，一悬停就出滚动条（分类栏底下那个过滤落点就是这样，改用原生 `title` 了）。侧栏那种不滚动的容器里用它没事。
- **打开一律双击**，单击只留给「选择」模式里的勾选。以后新增任何列表或网格都按这个口径来。
- **键盘那一半不能省**：只要不是原生 `<button>` 又要能按，就 spread `ui/pressable(onActivate)`（Enter/空格 + `tabIndex` + role），焦点圈由 `styles/layout.css` 顶部的 `:focus-visible` 统一画。双击打开的卡片，Enter 就等于那一下双击。分类树的建树/新建/改名/拖拽归类走 `shared/useCategoryScaffold`，删除流程两边不一样，留在各自视图里。

## `@/` 别名现在不能用

`tsconfig.json` 里原有的 `paths: { "@/*": ["src/*"] }` 已经删掉，因为 `vite.config.ts` 里**没有**对应的 `resolve.alias` —— 谁写 `@/xxx` 谁在 dev/build 时报「Failed to resolve import」，而编辑器不报错。

要启用必须两处一起加：`tsconfig.json` 的 `paths` + `vite.config.ts` 的 `resolve.alias`。只加一处比不加更糟。

## 验证

改完跑 `npx tsc --noEmit`（类型）和 `npx vite build`（打包）。这两道**都不代表界面正确**：交互改动要在 `npm run tauri dev` 的窗口里点一遍，尤其是拖拽、双击、弹窗这三类 —— WebView2 和浏览器对 HTML5 拖放的行为不一样。
