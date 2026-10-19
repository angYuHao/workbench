# 开发说明

用法看 [README.md](README.md)。这里是从源码改起来要知道的东西；再深一层的约定在 [src/README.md](src/README.md)、[src-tauri/README.md](src-tauri/README.md)、[src/office/README.md](src/office/README.md)、[src/tools/README.md](src/tools/README.md)。

## 跑起来

```bash
npm ci                 # 依赖一律离线复用，别随手 npm install / cargo add
npm run tauri dev      # 开发：vite 在 :1420，改前端即时热更，改 Rust 自动重编重启
npm run tauri build    # 打包（本机能不能出安装包看下一节）
```

新依赖优先用 `Cargo.lock` 里已有的版本（比如 `reqwest` 是 rig 自带的），装包由维护者本机手动做。

## 本机打安装包

打包这一步会去 GitHub 下 WiX 和 NSIS 两个工具，网不通就卡在那儿。**离线只出 exe、不出安装包**：

```bash
npx tauri build -d --no-bundle     # 产物在 src-tauri\target\debug\ 下
```

要在本机出安装包，得把三个工具文件预先放进 `%LOCALAPPDATA%\tauri\`（CLI 2.12.1 里没有环境变量开关，只认这个目录）：

| 放哪儿 | 内容 |
|---|---|
| `tauri\WixTools314\` | `wix314-binaries.zip` 解出来的内容，根目录直接是 `candle.exe`、`light.exe`、`Wix*Extension.dll` |
| `tauri\NSIS\` | `nsis-3.11.zip` 解出来，**去掉 zip 里那层 `nsis-3.11\` 目录**，留下 `Bin\`、`Stubs\`、`Plugins\`、`Include\` |
| `tauri\NSIS\Plugins\x86-unicode\additional\` | 单独放 `nsis_tauri_utils.dll`（这个要单独下载） |

三个文件的地址和校验和（校验和是从 `cli.win32-x64-msvc.node` 里切出来的常量，下错了不会报错、只会又去联网下）：

- `https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip` — SHA-256 `6ac824e1642d6f7277d0ed7ea09411a508f6116ba6fae0aa5f2c7daa2ff43d31`
- `https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.11/nsis-3.11.zip` — SHA-1 `EF7FF767E5CBD9EDD22ADD3A32C9B8F4500BB10D`
- `https://github.com/tauri-apps/nsis-tauri-utils/releases/download/nsis_tauri_utils-v0.5.3/nsis_tauri_utils.dll` — SHA-1 `75197FEE3C6A814FE035788D1C34EAD39349B860`

**`bundle > windows > wix > language` 必须是 `zh-CN`**：WiX 3 生成的 MSI 数据库默认代码页是 1252（西欧），产品名「个人工作台」进去就是 `error LGHT0311: characters that are not available in the database code page '1252'`，`light.exe` 直接失败，跟机器无关。NSIS 是 Unicode 的，没这个坑。

排查打包失败时记一下退出码：**1 是 bundler 挂了，101 是 cargo 编译挂了**。

## 发布

推一个 `v*` 的 tag 就触发 [.github/workflows/release.yml](.github/workflows/release.yml)：runner 上装依赖、`tauri build`、然后建 Release 并带上现场算的 SHA-256。tag 名字里带横杠的（`v1.0.0-rc1` 这种）只跑不发布，用来试。

上传前工作流会把安装包名开头的中文产品名换成 `workbench`：**GitHub 会抹掉资产名里的非 ASCII 字符**（`个人工作台_1.0.1_x64-setup.exe` 传上去只剩 `_1.0.1_x64-setup.exe`），不改名的话 Release 正文里的文件名和实际下载到的对不上。`bundle.artifactName` 这个配置键在 CLI 2.12.1 里还没有，所以只能在上传前动手。

## 改完必须跑的门

```bash
cd src-tauri && cargo test --offline && cargo clippy --offline --all-targets
npx tsc --noEmit --noUnusedLocals --noUnusedParameters && npx vite build
```

`vite build` **不做类型检查**，别把它的成功当类型过关。这四道门只保证能编译、单测过，**不代表界面行为对**：拖拽、双击、弹窗这三类要在 `npm run tauri dev` 的窗口里点一遍，WebView2 和浏览器对 HTML5 拖放的行为不一样。

动过命令层还要重新数那个不变量——`#[tauri::command]` 定义数 == `generate_handler!` 注册数，漏登记编译不报错，运行时才「command not found」：

```bash
grep -rho '#\[tauri::command\]' src-tauri/src/commands/*.rs | wc -l
grep -c 'commands::' src-tauri/src/lib.rs
```

CSP 相关的改动**必须在打包出来的 exe 里验**：`tauri dev` 的页面来自 vite（`devUrl`），不吃 `tauri.conf.json` 里那份 CSP。

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

## 没有版本控制时代留下的规矩

这个仓库现在是 git 仓库了，但早期几条习惯还有效：

- 删掉的文件不真删，先挪到 `.removed-<主题>/`；整套被替换的资源挪到 `.icons-backup-<日期>/`。这些目录已经在 `.gitignore` 里，**磁盘上是唯一的一份**，别顺手清。
- `tauri-dev.log` 之类的本机日志不进仓库，可以直接截断。
- 大改之前先整目录备份（`cp -r src .archive/bak-<日期>/src`）。
