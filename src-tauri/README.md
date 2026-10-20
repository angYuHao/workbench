# 后端（src-tauri）

`commands/` 是薄层：取连接锁 → 调 `services/` → 把 `AppError` 转成字符串。文件名按域走（`ai.rs`、`categorize.rs`、`bookmarks.rs`…）。业务逻辑在 `services/`：一个文件能一眼看完的就把自己尾部的 `#[cfg(test)]` 带着；`ai/` 里已经分成子目录的那两个（`categorize/`、`models/`）大测试集单独放 `tests.rs`，靠 `mod.rs` 的 `pub(crate) use` 摊平，所以外面写 `crate::services::ai::models::probe(...)` 不用关心它落在哪个子文件（`categorize/` 新加的 `run.rs`、`apply.rs` 就照前一种，测试留自己尾巴上）。

**测试建库一律 `include_str!("…/db/schema.sql")`，别手抄建表语句**：抄来的那份会跟生产库悄悄走偏——原先 `apply` 那份手抄 fixture 里 `apps` 没有 `target NOT NULL`，同一份数据在真库里插不进去，测过了也不作数。

## 加一条命令要同步三个地方

1. `src/commands/<域>.rs` 写 `#[tauri::command] pub fn ...`
2. `src/lib.rs` 的 `tauri::generate_handler![...]` 登记一条
3. 前端 `src/api/<域>.ts` 里 `invoke("命令名", { ... })`

**漏第 2 步编译不报错**，运行时才「command not found」。当前基线：`#[tauri::command]` 定义数 == 注册数 == 72（2026-10-19 加开始菜单同步和应用过滤名单那几条之后），前端 `invoke` 的字符串全部落在这个集合里。改完命令层就重新数一遍这两个数：

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

## 「应用」页的数据源是注册表，不是目录扫描

`services/installed.rs` 读 `HKLM`/`HKCU` 的 `...\CurrentVersion\Uninstall` **四个视图**（32 位程序在 `WOW6432Node` 那份里，只读一份会漏一半），只读层在 `win/registry.rs`（`KEY_READ=131097`、`KEY_WOW64_32KEY=512`、`KEY_WOW64_64KEY=256`、`ERROR_NO_MORE_ITEMS=259` 这些常量是从本地 `windows-0.58.0` 源码里查的，改之前先 grep，别照记忆写）。

注册表知道「装了什么」，说不准「怎么打开」，所以目标按四级找：`DisplayIcon` 里那个真存在的 exe → 开始菜单同名快捷方式 → 安装目录里同名的 exe → 卸载串里那个不是卸载器的 exe，全落空就用安装目录（点开是文件夹），一条路径都没有的算 `unresolved` 报数、不收卡片。两条踩过的坑：

- **`DisplayIcon` 不能拿去判系统应用**——MSI 的图标全在 `C:\Windows\Installer` 里，按它判会把正经软件成片挡掉；系统判定只看安装目录和真目标。
- **安装器不是程序本体**——`Docker Desktop Installer.exe`、`ProgramData\Package Cache\...\XxxSetup.exe` 点开来是重装/修复向导，`usable_exe` 一律不收这种路径。

「过滤系统应用」那个开关（`SYSTEM_NAME_MARKS`）现在管三类：附属程序（update / runtime / redistributable / 组件…）、驱动（driver / 驱动 / chipset / ODBC）、运行环境（`.NET` / DirectX / WebView2）。**故意不收进去的词**：`python`、`node`、`java`、`sdk`、`audio`——那是他自己装来用的软件，一刀切会把 Python、Node.js、Audacity 一起带走（Java 那种名字里带 "Update" 的顺挡，属于巧合不是规则）。这条规则最怕切错，所以真机那条 `#[ignore]` 测试会把**挡掉的名单**和**收进来的名单**都整片打出来，改 marks 之前先跑一遍看这两张表。

`hits` 匹配过滤模式时名字要比两份：折小写的原样名字（带括号和版本号，「7-Zip 23.01 (x64)」这种从卡片名照抄的模式要能命中）和只留字母数字的那份。

## 智能分类只有三步

`services/ai/categorize`：本机记忆 → 大模型 → 联网搜索。**分类规则不参与**——那张表只决定桌面上文件挪去哪个文件夹（`services::desktop::match_category`），跟「这台机器装了什么程序」是两回事，所以 `reserved`（规则占用的分类名）那一整套禁令已经拆掉了。

## webview 能读到什么

`tauri.conf.json` 里两道：`security.csp` 和 `security.assetProtocol.scope`。

- **CSP 只作用在 Tauri 自己 serve 的那份 html 上**（`manager/mod.rs` 的 `is_html` 闸门，作为响应头下发）。`tauri dev` 的页面来自 vite（`devUrl`），**不吃这份 CSP**，所以改完必须用打包出来的 exe 验，别在 dev 窗口里以为没事了。Tauri 会自己往 `script-src`/`style-src` 里补它注入脚本的 hash，不用手加。
- 每条 directive 都有具体东西依赖：`convertFileSrc` 在 Windows 上出的是 `http://asset.localhost/<编码路径>`，所以 `img-src`/`frame-src`/`connect-src` 三个都要同时写 `asset:` 和 `http://asset.localhost`（前者给别的平台，后者给本机 WebView2）；`style-src 'unsafe-inline'` 是 React 那一大堆内联 `style=` 必需的，去掉就整体没样式；`connect-src ipc: http://ipc.localhost` 照 Tauri 配置文档的示例留着。书签页的 favicon 走 `https://www.google.com/s2/favicons`，是唯一一个远程图片源。
- **asset scope 静态是空的**，运行时按目录授权：`services::files::preview_roots()` = 两个桌面根（`win::desktop::all_desktops()`）+ 图标缓存目录 + 每个受管文件夹，启动时在 `lib.rs` 的 setup 里逐条 `allow_directory(root, true)`，`add_managed_folder` 当场补一条。移除受管文件夹**撤不回**：scope 只有 `allow_*` 和会永久压住后续允许的 `forbid_*`，所以只摘监视器，预览权限等下次启动自然消失。
- 预览不受信文件靠三层叠加：`rehype-sanitize`（md 里的原始 HTML）、iframe `sandbox="allow-same-origin"`（.html，不放行脚本）、上面那份 CSP（内联脚本和远程脚本都执行不了）。

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
