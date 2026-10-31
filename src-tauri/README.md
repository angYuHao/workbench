# 后端（src-tauri）

`commands/` 是薄层：取连接锁 → 调 `services/` → 把 `AppError` 转成字符串。文件名按域走（`ai.rs`、`categorize.rs`、`bookmarks.rs`…）。业务逻辑在 `services/`：一个文件能一眼看完的就把自己尾部的 `#[cfg(test)]` 带着；分成子目录的那四个（`ai/categorize/`、`ai/models/`、`installed/`、`desktop/`）大测试集单独放 `tests.rs`，靠 `mod.rs` 的 `pub use` / `pub(crate) use` 摊平，所以外面写 `crate::services::desktop::scan_desktop(...)` 不用关心它落在哪个子文件（`categorize/` 新加的 `run.rs`、`apply.rs` 就照前一种，测试留自己尾巴上）。

拆目录时 `include_str!` 的相对深度会变（`../db/schema.sql` → `../../db/schema.sql`），编译期就报，不会静默。

**测试建库一律 `include_str!("…/db/schema.sql")`，别手抄建表语句**：抄来的那份会跟生产库悄悄走偏——原先 `apply` 那份手抄 fixture 里 `apps` 没有 `target NOT NULL`，同一份数据在真库里插不进去，测过了也不作数。

## 加一条命令要同步三个地方

1. `src/commands/<域>.rs` 写 `#[tauri::command] pub fn ...`
2. `src/lib.rs` 的 `tauri::generate_handler![...]` 登记一条
3. 前端 `src/api/<域>.ts` 里 `invoke("命令名", { ... })`

**漏第 2 步编译不报错**，运行时才「command not found」。当前基线：`#[tauri::command]` 定义数 == 注册数 == 70（2026-10-21 加顶边抽屉那条 `set_edge_drawer`；上一条是 2026-10-20 拆掉唤醒快捷键那三条命令后落在 69），前端 `invoke` 的字符串全部落在这个集合里。改完命令层就重新数一遍这两个数：

```bash
grep -rc '#\[tauri::command\]' src/commands/*.rs   # 求和
grep -c 'commands::' src/lib.rs
```

命令名和参数名是**协议**：前端用 camelCase（`modelId` ↔ `model_id`），Tauri 自动转换。所以重命名 Rust 参数会静默打断前端调用 —— 不要顺手改。

## 数据库

`src/db/schema.sql` 是唯一的建表来源，`db::init()` 每次启动整份跑一遍，全部 `CREATE TABLE IF NOT EXISTS`。

**因此永远不要给已有的表加列** —— 老库里加不出来，只会让线上读不到数据。要加数据就：新开一张表（比如 `usage_stats`、`bookmark_trash`），或者塞进 `settings` 那张 KV（比如模型列表缓存 `ai_model_cache:<id>`）。

事务用 `execute_batch("BEGIN")` / `db::commit()` / `ROLLBACK`：`Connection::transaction()` 要 `&mut`，而这里拿到的锁是 `&Connection`。**COMMIT 自己失败时必须补 ROLLBACK**，否则事务会留在共享连接上，下一次 `BEGIN` 直接报「已在事务里」，之后所有写操作一起坏 —— 所以统一走 `crate::db::commit()`，别手搓。

恢复备份（`restore_db`）换完库文件后要再跑一遍 `schema.sql`，否则旧备份里缺的表会让整个页面读不出东西。

## 连接锁

`state.conn()` 返回 `MutexGuard`。async 命令里**不要跨 `.await` 持有它**：先取出需要的配置、放锁、等网络、再重新取锁写库。`list_remote_ai_models` / `probe_ai_model` 是范例。

## 解析 .lnk 和抽图标要先有 COM

`win::lnk::resolve_lnk`（`IShellLinkW`）和 `win::icon::extract_icon_to_png` 都不自己 `CoInitializeEx`，**线程没初始化 COM 就整片失败**：前者失败会悄悄退回「快捷方式自己的路径」（看着像解析出了 .lnk），后者失败就是每张卡片都没图。

同步命令（非 async）跑在主线程上，tao/WebView2 已经把 COM 起好了，所以 `scan_desktop`、`sync_installed_apps` 在应用里是好的。**在测试线程里直接调这两个 API 会全灭**，要照着 `services::installed::tests::the_real_registry_lists_installed_programs` 那样先 `CoInitializeEx(None, COINIT_APARTMENTTHREADED)`；那条 `#[ignore]` 的测试就是拿真注册表核对这一步的（本机读出 352 条卸载项：挡掉 232、收进来 60、抽到 59 张图标、35 条认不出目标）。

## 「应用」页的数据源：注册表四视图 + 开始菜单补一遍

`services/installed/`（拆成 `scan` 读源头 / `target` 认目标 / `filter` 判收不收 / `store` 写库，共用类型在 `mod.rs`）读 `HKLM`/`HKCU` 的 `...\CurrentVersion\Uninstall` **四个视图**（32 位程序在 `WOW6432Node` 那份里，只读一份会漏一半），只读层在 `win/registry.rs`（`KEY_READ=131097`、`KEY_WOW64_32KEY=512`、`KEY_WOW64_64KEY=256`、`ERROR_NO_MORE_ITEMS=259` 这些常量是从本地 `windows-0.58.0` 源码里查的，改之前先 grep，别照记忆写）。

第二遍采集的是 `shortcut_index()` 里**没被第一遍认领**的 .lnk：商店应用和绿色版不写卸载项，光读注册表就缺这一类。两道闸门防止重复和不该长的卡片——`claimed`（某个注册表条目已经拿这个 .lnk 当过目标）和 `program_target()`（解出来的目标得是 exe / 脚本 / .lnk 这种能跑的东西，.chm 帮助、.htm 说明不算应用）；系统过滤和手动名单在第二遍一样走一遍，所以 `C:\WINDOWS\system32` 那堆自带工具不会跟着进来。

注册表知道「装了什么」，说不准「怎么打开」，所以目标按六级找：`DisplayIcon` 里那个真存在的 exe → `DisplayIcon` 指的是快捷方式的 → 开始菜单同名快捷方式（名字带厂商前缀也认，`Nsight Compute 2026.1.0` 对 `NVIDIA Nsight Compute 2026.1.0`）→ 安装目录自己就是 exe → 已知那几个目录（安装目录、**卸载器所在目录**、`DisplayIcon` 所在目录）里名字对得上的 exe → 卸载串里那个不是卸载器的 exe；全落空就用目录本身（点开是文件夹），一条路径都没有的算 `unresolved` 报数、不收卡片。`InstallLocation` 为空是常态，所以卸载器那个 `unins000.exe` 虽然不能当目标，**它所在的目录一定要拿去翻主程序**（Charles、Navicat 这类就是这么找回来的）。还有一条：他自己搬过目录的软件，注册表里留的是老路径，`relocated` 会退一层再认一次。两处踩过的坑：

- **`DisplayIcon` 不能拿去判系统应用**——MSI 的图标全在 `C:\Windows\Installer` 里，按它判会把正经软件成片挡掉；系统判定只看安装目录和真目标。
- **安装器不是程序本体**——`Docker Desktop Installer.exe`、`ProgramData\Package Cache\...\XxxSetup.exe` 点开来是重装/修复向导，`usable_exe` 一律不收这种路径。
- **0 字节的 exe 不是能跑的东西**——WPS 自更新会把安装根目录的 `ksolaunch.exe` 换成空壳（真身改名成 `ksolaunch.bak.exe`），而注册表的 `DisplayIcon` 和开始菜单快捷方式都还指着它，症状就是「文件在、点不动」。`runnable()` 见到空文件一律拒。
- **注册表给的目录常常不是主程序那一层**——`exe_near()` 从已知目录往上退一层、再往隔壁的子目录各看一层（WPS 的真身在 `<版本>\office6\wps.exe`，注册表只给到 `<版本>\utility\uninst.exe`）。松匹配几档之间取**名字最短**的那个候选，所以拿到的是 `wps.exe` 而不是首页启动器 `wpsoffice.exe`；一个条目最多翻 60 个目录（`MAX_SCAN_DIRS`），宁可找不到目标也不去扫整块盘。

「过滤系统应用」那个开关（`SYSTEM_NAME_MARKS`）现在管三类：附属程序（update / runtime / redistributable / 组件…）、驱动（driver / 驱动 / chipset / ODBC）、运行环境（`.NET` / DirectX / WebView2）。**故意不收进去的词**：`python`、`node`、`java`、`sdk`、`audio`——那是他自己装来用的软件，一刀切会把 Python、Node.js、Audacity 一起带走（Java 那种名字里带 "Update" 的顺挡，属于巧合不是规则）。这条规则最怕切错，所以真机那条 `#[ignore]` 测试会把**挡掉的名单**和**收进来的名单**都整片打出来，改 marks 之前先跑一遍看这两张表。

`hits` 匹配过滤模式时名字要比两份：折小写的原样名字（带括号和版本号，「7-Zip 23.01 (x64)」这种从卡片名照抄的模式要能命中）和只留字母数字的那份。

去重只认名字（`same_program`）：名字相等、或者去掉标点后互为一段（短于 4 字的段不算）。目标文件相同**不能**当判据——Steam 全家共用 `steam.exe`、UWP 共用 `ApplicationFrameHost.exe`，按文件去重会把除第一张之外的全吞掉，当初连 Steam 本身都进不了页面。

## 智能分类只有三步

`services/ai/categorize`：本机记忆 → 大模型 → 联网搜索。**分类规则不参与**——那张表只决定桌面上文件挪去哪个文件夹（`services::desktop::match_category`），跟「这台机器装了什么程序」是两回事，所以 `reserved`（规则占用的分类名）那一整套禁令已经拆掉了。

## webview 能读到什么

`tauri.conf.json` 里两道：`security.csp` 和 `security.assetProtocol.scope`。

- **CSP 只作用在 Tauri 自己 serve 的那份 html 上**（`manager/mod.rs` 的 `is_html` 闸门，作为响应头下发）。`tauri dev` 的页面来自 vite（`devUrl`），**不吃这份 CSP**，所以改完必须用打包出来的 exe 验，别在 dev 窗口里以为没事了。Tauri 会自己往 `script-src`/`style-src` 里补它注入脚本的 hash，不用手加。
- 每条 directive 都有具体东西依赖：`convertFileSrc` 在 Windows 上出的是 `http://asset.localhost/<编码路径>`，所以 `img-src`/`frame-src`/`connect-src` 三个都要同时写 `asset:` 和 `http://asset.localhost`（前者给别的平台，后者给本机 WebView2）；`style-src 'unsafe-inline'` 是 React 那一大堆内联 `style=` 必需的，去掉就整体没样式；`connect-src ipc: http://ipc.localhost` 照 Tauri 配置文档的示例留着。书签页的 favicon 走 `https://www.google.com/s2/favicons`，是唯一一个远程图片源。
- **asset scope 静态是空的**，运行时两层授权：启动时 `services::files::preview_roots()` 只摆两个桌面根（`win::desktop::all_desktops()`）+ 图标缓存目录，在 `lib.rs` 的 setup 里逐条 `allow_directory(root, true)`；文件页浏览的是整台机器，其余目录**点到才给**——`commands::files::authorize_preview` 在前端真要预览某个文件时 `allow_file(那一个) + allow_directory(它所在目录, 不递归)`，同目录的相对路径图片因此也能读，没点开的盘面 webview 摸不到。授权**撤不回**：scope 只有 `allow_*` 和会永久压住后续允许的 `forbid_*`，所以只记这次会话里看过哪些目录，下次启动自然清零。列目录不受这层管（`list_files` 在后端直接 `read_dir`，任何路径都能列，含隐藏项由界面那个开关决定摆不摆）。
- 磁盘监视器盯的是「界面正在看的那几个目录」：`commands::files::watch_folders` 由文件页在切栏/切标签时报到后端（去重、攒一下），只报**每栏当前标签**的那串列——后台标签不读盘也不监视，切过去时前端自己补读；启动时先起一个空的。监视是增量的（只加/减变动的目录）、只看第一层，标签多时也不会因为一次重报就把整套监视拆了重建。不再有一张「受管文件夹」名单。
- 预览不受信文件靠三层叠加：`rehype-sanitize`（md 里的原始 HTML）、iframe `sandbox="allow-same-origin"`（.html，不放行脚本）、上面那份 CSP（内联脚本和远程脚本都执行不了）。

## 顶边抽屉（平时藏着，鼠标顶到屏幕上沿出来）

设置里那条「顶边唤出抽屉」存的是 `settings` 里的 `edge_drawer`，**没写过就算开**。开关两边都由 `commands::settings::set_edge_drawer` 同时落库和当场改窗口形状，所以界面不会和窗口状态不同步（和 `set_close_to_tray` 同一个路子）。

`tauri.conf.json` 里 `windows[0]` 存的是**开着的形状**：`visible: false` + `decorations: false` + `alwaysOnTop: true`——因为默认是开，这条路一次都不用改窗口样式；只有关掉时才在运行时 `set_decorations(true)` + 取消置顶 + `center()` 回到带标题栏的居中窗口。第一次跑（库里压根没 `edge_drawer` 这条）会先显示一次，不然装完点开啥也看不见。

`drawer.rs` 里那 150ms 一次的轮询用 `GetCursorPos`（`Win32_UI_WindowsAndMessaging`，为「鼠标键还按着」那条另开了 `Win32_UI_Input_KeyboardAndMouse`），三条保护都写在文件头的注释里：光标没进过窗口不算「离开」（托盘唤回不会一闪就没）、按住任何鼠标键期间不收（拖卡片到分类、拖桌面文件进来不能被废掉）、有全屏程序盖住整块显示器在前台时不弹出（视频、游戏、放映）。抽屉高度取 640 逻辑像素、屏幕不够高让到工作区一半，宽=那块屏工作区的宽，位置贴工作区上沿。

窗口 hide/show 全在 Rust 侧：capabilities 里只有 `core:window:default`，JS 侧根本调不动（`window.hide()` 会被拒）。托盘那条路留着当兜底——抽屉模式下窗口没有标题栏，「退出」只剩托盘菜单。

## 密钥与隐私边界

- `ai_models.key_enc`、`totp_accounts.secret_enc` 是当前 Windows 用户的 DPAPI 密文：换机换用户解不开，任何接口都不回传密钥，只在内存里短暂出现。
- 加密备份口令走 `win/cng.rs`（bcrypt.dll 的 PBKDF2 + AES-GCM），迭代数范围见 `services/totp_backup.rs`，过高会在派生时卡住整个库锁。
- `ai.log` 只记地址、模型名、状态码、耗时和请求/响应开头；**密钥永不入日志**，书签分析的 prompt 连网址都不落盘（`smart::chat` 的 `redact` 参数）。
- 只有 2FA 口令备份是可跨机迁移的东西，其余一切本机。

## 编译期路径

`include_str!` / `include_bytes!` 按**当前文件**相对解析，移动文件必须同步改（编译期就会报，不会静默）：

- `schema.sql`：生产读它的是 `db/mod.rs` 和 `commands/settings.rs`（恢复备份后补建表），其余都在测试里各建一份库。站点会变，别照这份清单数，现查：`grep -rn "schema.sql" src`
- `../icons/32x32.png`：`tray.rs` 的托盘兜底图

`tauri.conf.json` 里的 `bundle.icon` 和 `frontendDist` 也是写死的路径，动 `icons/` 目录要一起改。
