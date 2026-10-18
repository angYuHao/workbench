# 个人工作台

Windows 单机自用工作台：桌面整理 + 网页管理 + 文件管理 + 工具箱，配置集中在一个设置窗口里。Tauri 2 + React 18 + rusqlite/SQLite，产品名「个人工作台」，窗口 1200×800（最小 960×640）。**没有服务端，数据全在本机 `%APPDATA%\workbench\`**，除你主动点的抓取和 AI 请求以外不联网。

侧栏四个页面，底部两个入口：深浅色切换、设置（弹窗，不占路由）。

## 桌面整理（打开就是这页）

扫**当前用户桌面 + 公共桌面**两个根，连子目录一起往里看两层（`桌面\A\B\` 里的文件算，`桌面\A\B\C\` 不再进）；`desktop.ini`、`.` 开头的跳过；上一次整理建出来的 `桌面\桌面整理\` 不再往里翻。

- 卡片**双击**才启动（Enter 等价于那一下双击），卡片和分类树都按打开次数排。卡片上没有按钮也没有右键菜单，勾选和批量删除走「选择」模式。
- 卡片拖到左边分类树上就地归类；分类树能建树、改名、删除，**改名会连带改同名分类规则**。
- **一键整理**先给一份预览，逐条列出会进哪个分类，底下两个勾，默认都开着：
  - 「只处理新增：已经在启动器里的项这次不动」——去掉勾才会重抽图标、重新归类登记过的项；
  - 「只把程序登记进启动器，桌面上的文件保持原地不动」——取消它才把文件搬进 `桌面\桌面整理\<分类>`（同名加 `(1)(2)`），搬完能点**恢复整理**原路退回，回滚凭据在 `organize_logs` 表里。
- **智能分类**四级顺序：分类规则 → 本机记忆 → 大模型 → 联网搜索。规则判掉的和记忆里有的都不出本机，**发给模型的只有名字，不带本地路径**；只有软件类的判断会写进记忆。默认只判「未分类」和落在「其他」里的。
- 图标抽的是「桌面上看到的那张」：先解析快捷方式自己（`IconLocation` 由 shell 算好），拿不到再退回它指向的程序；PNG 按写法哈希缓存在 `%APPDATA%\workbench\icons\`。指向 `.ico` 的快捷方式不会被当成图片归档——那是安装器把入口指到了自己的图标文件上。

## 网页管理

书签库 + 同一套分类树，「智能分类」和桌面那套共用后端，也只判未分类和其他。

- 加网址有三条路：从浏览器导入收藏夹、手填、编辑现有条目。favicon 走 Google s2，是**唯一一个远程图片源**。
- **智能分析**：开窗只告诉你「这批 N 条」，按「开始分析」才发请求。顺序是本机网页缓存 → 抓页面 title/description（GitHub 走它的 API：这台机器上 github.com 网页不通但 API 通）→ 都拿不到才交给模型。一轮最多抓 40 个网页、GitHub 60 次，撞额度就本轮收手，抓到的存本机，再点一次只补没排到的。每行的来源都标着（网页自带 / GitHub API / 模型判断 / 走了缓存），模型猜的、网页还没抓到的会写「模型猜·未抓」。
- 判出来的重复条目默认勾上要删的那些，删掉的不真删，进回收站能捞回来。
- 换分类或改搜索词再点，分析的就只是筛出来的这一批。

## 文件管理

「受管文件夹」列表 + 目录浏览 + 预览面板。**添加文件夹时当场给它开预览权限**；移除只摘监视器——asset scope 没有撤回 API，权限等下次启动自然消失。

预览覆盖：markdown（GFM + 原始 HTML 走 sanitize 白名单、代码高亮、长代码横向滚动不折行、相对路径图片按 md 所在目录解析）、纯文本和日志、PDF 与 HTML（iframe，HTML 沙箱不放行脚本）、xlsx / xls / docx / doc（自研字节解析 + docx-preview，列宽、合并单元格、日期数字格式、艺术字文本框都处理）。**pptx 明确不做**。

## 工具

一个工具一个目录，`src/tools/registry.ts` 是唯一清单。目前只有一个：

**2FA 认证管理** —— 本机生成 TOTP 动态码，加账户四个入口：截图扫描（抓屏 → 识二维码）、图片识别（Ctrl+V 粘贴）、手动输入、文本导入（批量）。密钥是当前 Windows 用户的 DPAPI 密文，任何接口都不回传。配套的**加密备份**走口令 → PBKDF2 + AES-GCM，是整套东西里唯一能跨机迁移的文件；口令错了不报错，只会让你再也解不开。

## 设置

侧栏底部弹出窗口，左边分组导航 + 顶部搜索，右边分组标题 + 说明 + 一行一个设置项。五组：

| 分组 | 里面有什么 |
|---|---|
| 分类规则 | 桌面整理的归档依据：目标分类名、关键词、扩展名、排序、开关。**关键词优先于扩展名**，都不命中落「其他」 |
| 大模型 | OpenAI 兼容接口：显示名、地址、模型名、key。能拉远端模型列表点选、能测连通。外加三样本机 AI 数据——调用日志（文件只留最近 600 行，界面看最近 100 行）、本机记忆、网页缓存，都能清 |
| 网址黑名单 | 精确地址（多选拉黑）或域名及其所有子域（手输） |
| 启动 | 开机自启、关闭时放入系统托盘，**两个都默认关**：关着就是关窗即退出 |
| 数据 | 导出整个库 / 从备份文件恢复（换完库文件会再跑一遍建表脚本，旧备份缺的表能补齐） |

## 数据都在哪

```
%APPDATA%\workbench\workbench.db      唯一的库：分类、程序、书签、文件、整理日志、规则、模型、2FA、记忆、使用计数…
%APPDATA%\workbench\workbench.db-wal  SQLite 的日志文件，和主库同目录，别单独拷主库
%APPDATA%\workbench\icons\            图标 PNG 缓存，删了会重抽
%APPDATA%\workbench\ai.log            AI 调用日志，不含密钥，书签分析的 prompt 连网址都不落盘
```

- 建表只有 `src-tauri/src/db/schema.sql` 一份来源，全部 `CREATE TABLE IF NOT EXISTS`，**没有迁移机制**，所以永远不给已有的表加列（详见 `src-tauri/README.md`）。
- 密钥类一律 DPAPI 绑当前 Windows 用户：模型 key、2FA 密钥。换机换用户解不开，只能重加。
- 开机自启写注册表，主题、托盘、上次用的模型这些是 `settings` 那张 KV。

## 跑起来

```bash
npm run tauri dev      # 开发：vite 在 :1420，改前端即时热更，改 Rust 自动重编重启
npm run tauri build    # 打包（要看下面那条）
```

打包这一步会去 GitHub 下 WiX 和 NSIS 两个安装包工具，网不通就卡在那儿。**离线只出 exe、不出安装包**：

```bash
npx tauri build -d --no-bundle     # 产物在 src-tauri\target\debug\个人工作台.exe
```

要在本机出安装包，得把三个工具文件预先放进 `%LOCALAPPDATA%\tauri\`（CLI 里没有环境变量开关，只认这个目录）：

| 放哪儿 | 内容 |
|---|---|
| `tauri\WixTools314\` | `wix314-binaries.zip` 解出来的内容，根目录直接是 `candle.exe`、`light.exe`、`Wix*Extension.dll` |
| `tauri\NSIS\` | `nsis-3.11.zip` 解出来，**去掉 zip 里那层 `nsis-3.11\` 目录**，留下 `Bin\`、`Stubs\`、`Plugins\`、`Include\` |
| `tauri\NSIS\Plugins\x86-unicode\additional\` | 单独放 `nsis_tauri_utils.dll`（这个要单独下载，SHA-1 校验得过） |

三个文件从这些地方下（校验和是从本机那份 `cli.win32-x64-msvc.node` 里切出来的常量，抄一份钉在这儿，下错了不会报错、只会又去联网下）：

- `https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip` — SHA-256 `6ac824e1642d6f7277d0ed7ea09411a508f6116ba6fae0aa5f2c7daa2ff43d31`
- `https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.11/nsis-3.11.zip` — SHA-1 `EF7FF767E5CBD9EDD22ADD3A32C9B8F4500BB10D`
- `https://github.com/tauri-apps/nsis-tauri-utils/releases/download/nsis_tauri_utils-v0.5.3/nsis_tauri_utils.dll` — SHA-1 `75197FEE3C6A814FE035788D1C34EAD39349B860`

CI 在 runner 上自己下这三个，不受本机网络限制；本机这份缓存只存在本机，`.gitignore` 不收它（它在仓库外）。

**`bundle > windows > wix > language` 必须是 `zh-CN`**：WiX 3 生成的 MSI 数据库默认代码页是 1252（西欧），产品名「个人工作台」进去就是 `error LGHT0311: characters that are not available in the database code page '1252'`，`light.exe` 直接失败，跟机器无关。NSIS 是 Unicode 的，没这个坑。

debug 产物照样嵌了前端资源、照样吃 `tauri.conf.json` 里的 CSP，只是额外带 devtools。反过来，`tauri dev` 的页面来自 vite（`devUrl`），**不吃那份 CSP**，安全相关的改动必须用打包出来的 exe 验。

依赖一律离线复用，不要随手 `npm install` / `cargo add`：新依赖优先用 `Cargo.lock` 里已有的版本（比如 `reqwest` 是 rig 自带的），装包由维护者本机手动做。

## 改完必须跑的门

```bash
cd src-tauri && cargo test --offline && cargo clippy --offline --all-targets
npx tsc --noEmit --noUnusedLocals --noUnusedParameters && npx vite build
```

`vite build` **不做类型检查**，别把它的成功当类型过关。这四道门只保证能编译、单测过，**不代表界面行为对**：拖拽、双击、弹窗这三类要在 `npm run tauri dev` 的窗口里点一遍，WebView2 和浏览器对 HTML5 拖放的行为不一样。

动过命令层还要重新数那个不变量——`#[tauri::command]` 定义数 == `generate_handler!` 注册数 == 64，漏登记编译不报错，运行时才「command not found」：

```bash
grep -rho '#\[tauri::command\]' src-tauri/src/commands/*.rs | wc -l
grep -c 'commands::' src-tauri/src/lib.rs
```

## 目录

```
src/                 前端（见 src/README.md）
  app/               外壳：布局、侧栏、路由表
  features/          四个主界面各自一个目录：desktop bookmarks files settings
  shared/            两个以上功能共用的界面件与 hook
  types/             按功能域拆开的类型，index.ts 只做汇总（对面是 src-tauri/src/models/）
  ui/                自研组件库，不引 antd 之类的第三方 UI
  office/            读磁盘格式的解析层（见 src/office/README.md）
  tools/             工具箱：一个工具一个目录，registry.ts 是唯一清单（见 src/tools/README.md）
src-tauri/           后端（见 src-tauri/README.md）
  src/commands/      薄层：取锁 → 调 service → 把错误转字符串
  src/services/      业务逻辑，测试就写在文件尾
  src/models/        跨层数据结构，按域分文件
  src/db/schema.sql  唯一的建表来源
  src/win/           手写 Win32：DPAPI、CNG、shell、图标、快捷方式、抓屏
```

## 应用图标

源图是 `src-tauri/icons/app-icon.png`（1024、RGBA）。改完图标要三步：

```bash
npx tauri icon src-tauri/icons/app-icon.png   # 生成全套 png / ico / icns / 移动端
touch src-tauri/tauri.conf.json               # 必须：构建脚本只监听配置和 capabilities
# 然后重启 tauri dev，否则 exe 里还是旧图标
```

侧栏顶部的 logo 直接引用 `src-tauri/icons/128x128.png`，所以图标只有一处真身。任务栏上看着还是旧图通常是 Windows 图标缓存：取消固定再钉一次，或 `ie4uinit.exe -show`。

## 没有版本控制，所以

- 删掉的文件不真删，挪到 `.removed-<主题>/`；被替换的整套资源挪到 `.icons-backup-<日期>/`。这两个目录是回退手段，别顺手清。
- `tauri-dev.log` 是开发时重定向的日志，可以直接截断。
- 大改之前先整目录备份（`cp -r src .archive/bak-<日期>/src`），出问题整目录换回来，不要逐文件回退。
