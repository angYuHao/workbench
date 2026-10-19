//! 「应用」页的数据源：开始菜单里的快捷方式。
//!
//! 不用注册表的 `Uninstall` 键：那边 122 条里只有约 40 条有 `InstallLocation`，
//! 其余的 `DisplayIcon` 常常指着卸载器图标（`…\uninstallerIcon.ico`、`X.exe,0`），
//! 拿它当启动目标会有一大半点不动。开始菜单的快捷方式名字是给人看的、
//! `.lnk` 又能解析出真 exe，图标也照桌面上那套抽法。
//!
//! 同步之前先过一遍过滤名单（设置 → 应用过滤）：命中的一律不收。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::AppResult;
use crate::services::desktop::{extract_icon, extension_of, get_icon_dir, launch_target};

/// 开始菜单本来就有多层厂商目录，给三层的量
const MAX_DEPTH: usize = 3;
/// 试算里最多报几个被挡的名字，够看出规律就行
const SAMPLES: usize = 12;
/// 设置里那个「过滤系统应用」开关；没写过就当作开着
const SYSTEM_SWITCH: &str = "filter_system_apps";

struct Discovered {
    name: String,
    target: String,
    lnk: String,
}

/// 一次同步（或试算）的账单。界面照着它说话，不然「点了没反应」和「本来就没有新的」分不出来。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    /// 开始菜单里扫出多少个可启动的快捷方式
    pub scanned: usize,
    /// 本次收进来的（试算时是「会收进来的」）
    pub inserted: usize,
    /// 命中过滤名单没收的
    pub filtered: usize,
    /// 库里已经有的
    pub already: usize,
    /// 库里现在一共多少张卡片
    pub total: usize,
    /// 被挡掉的名字，最多 12 个
    pub samples: Vec<String>,
}

/// 过滤条件：手加的模式（名字和路径都做小写包含匹配）+「系统应用」那个总开关
pub struct Filters {
    pub patterns: Vec<String>,
    pub system: bool,
}

/// 扫一遍开始菜单，把没见过的应用收进启动器。只插不改：
/// 用户改过的名字、手动归好的分类、抽好的图标都不该被一次同步抹掉。
/// 删掉的卡片下次同步照样收回来——这一页是「本机装了什么」的镜子，不做拉黑名单。
pub fn sync_installed(conn: &Connection) -> AppResult<SyncReport> {
    run(conn, false)
}

/// 同一次扫描但不写库，用来看过滤名单挡掉了谁
pub fn preview_installed(conn: &Connection) -> AppResult<SyncReport> {
    run(conn, true)
}

fn run(conn: &Connection, dry: bool) -> AppResult<SyncReport> {
    let icon_dir = get_icon_dir()?;
    let roots = crate::win::desktop::all_program_menus()
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    scan(conn, &roots, &icon_dir, dry)
}

/// 真正干活的一层：目录和图标缓存都从外面进来，测试就能拿临时目录真跑一遍
pub(crate) fn scan(
    conn: &Connection,
    roots: &[PathBuf],
    icon_dir: &Path,
    dry: bool,
) -> AppResult<SyncReport> {
    let mut found = Vec::new();
    for root in roots {
        collect(root, 0, &mut found);
    }
    let filters = read_filters(conn)?;
    let system_root = system_root();

    // 「见过」按两种写法算：卡片存的 target，以及快捷方式自己的路径
    //（桌面登记那批存的就是 .lnk 路径，同一个程序两种写法都可能是它）
    let mut known: HashSet<String> = conn
        .prepare("SELECT target FROM apps")?
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .map(|t| path_key(&t))
        .collect();

    let _ = std::fs::create_dir_all(icon_dir);
    let mut report = SyncReport {
        scanned: found.len(),
        inserted: 0,
        filtered: 0,
        already: 0,
        total: 0,
        samples: Vec::new(),
    };
    for item in found {
        let lnk = path_key(&item.lnk);
        let target = path_key(&item.target);
        if known.contains(&lnk) || known.contains(&target) {
            report.already += 1;
            continue;
        }
        if hits(&item.name, &target, &lnk, &filters, &system_root) {
            report.filtered += 1;
            if report.samples.len() < SAMPLES {
                report.samples.push(item.name.clone());
            }
            continue;
        }
        if !dry {
            let icon = extract_icon(icon_dir, &[item.lnk.as_str(), item.target.as_str()]);
            conn.execute(
                "INSERT INTO apps (name, target, source, icon_path) VALUES (?1, ?2, 'startmenu', ?3)",
                params![item.name, item.target, icon],
            )?;
        }
        known.insert(target);
        known.insert(lnk);
        report.inserted += 1;
    }
    report.total = if dry {
        count_rows(conn)? + report.inserted
    } else {
        count_rows(conn)?
    };
    Ok(report)
}

fn count_rows(conn: &Connection) -> AppResult<usize> {
    Ok(conn.query_row("SELECT COUNT(*) FROM apps", [], |r| r.get::<_, i64>(0))? as usize)
}

fn read_filters(conn: &Connection) -> AppResult<Filters> {
    let patterns = conn
        .prepare("SELECT pattern FROM app_filters")?
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    let system = crate::services::settings::get_setting(conn, SYSTEM_SWITCH)?
        .map(|v| v != "0")
        .unwrap_or(true);
    Ok(Filters { patterns, system })
}

/// 名字和两种路径写法里任意一个包含某个模式就算命中；模式空串不挡东西
fn hits(name: &str, target: &str, lnk: &str, filters: &Filters, system_root: &str) -> bool {
    if filters.system && is_system_path(target, lnk, system_root) {
        return true;
    }
    let n = name.to_lowercase();
    filters.patterns.iter().any(|p| {
        let p = p.trim().to_lowercase();
        !p.is_empty() && (n.contains(&p) || target.contains(&p) || lnk.contains(&p))
    })
}

/// 「系统应用」按路径认：Windows 目录自己，加上 UWP/商店应用住的 WindowsApps
/// 和那几个 Windows 自带的 Program Files 子目录。
/// 第三方装的 SDK（`Program Files (x86)\Windows Kits`）不在这儿，别一起挡掉。
fn is_system_path(target: &str, lnk: &str, system_root: &str) -> bool {
    let root = path_key(system_root).trim_end_matches('/').to_string();
    let t = path_key(target);
    let l = path_key(lnk);
    let under = |p: &str| !root.is_empty() && (p == root || p.starts_with(&format!("{root}/")));
    under(&t) || under(&l) || SYSTEM_MARKS.iter().any(|m| t.contains(m) || l.contains(m))
}

const SYSTEM_MARKS: [&str; 4] = [
    "/windowsapps/",
    "/program files/windows nt/",
    "/program files/windows defender/",
    "/program files/windows security/",
];

fn system_root() -> String {
    std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string())
}

/// 比对用的写法：老卡片存的是 `F:\\Program\\...`，手动填的可能是 `F:/Program/...`，
/// 同一个程序不该因为斜杠方向被收两遍
fn path_key(p: &str) -> String {
    p.trim().to_lowercase().replace('\\', "/")
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<Discovered>) {
    if depth > MAX_DEPTH {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, depth + 1, out);
            continue;
        }
        let file_name = match path.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };
        if !file_name.to_ascii_lowercase().ends_with(".lnk") || is_uninstaller(&file_name) {
            continue;
        }
        let target = launch_target(&path);
        // 解析不出可启动目标的（目标文件被删了、指向一个目录）不收：
        // 卡片点不动比少一张卡片更烦人
        if !launchable(&target) {
            continue;
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or(file_name);
        out.push(Discovered {
            name,
            lnk: path.to_string_lossy().to_string(),
            target,
        });
    }
}

fn launchable(target: &str) -> bool {
    if target.contains("://") {
        return true;
    }
    let path = Path::new(target);
    if !path.is_file() {
        return false;
    }
    // 目标是图标文件的那种快捷方式是安装程序顺手放的（Charles、Cpolar 都这样），
    // 点它只会开一个图片查看器，不算一个应用
    !matches!(extension_of(path).as_str(), "ico" | "cur")
}

/// 卸载器链接不是「打开应用」，收进来只会多出一堆点了没反应的卡片
fn is_uninstaller(file_name: &str) -> bool {
    let n = file_name.to_lowercase();
    n.starts_with("uninstall") || n.contains("卸载")
}

/// 过滤名单：一条模式就是个小写子串，比对名字和路径两种写法
pub fn get_app_filters(conn: &Connection) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT pattern FROM app_filters ORDER BY added_at DESC")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// 回 true 是真加上了；重复加（不分大小写）回 false，让界面说一句「已经在名单里」
pub fn add_app_filter(conn: &Connection, pattern: &str) -> AppResult<bool> {
    let p = pattern.trim();
    if p.is_empty() {
        return Err(crate::error::AppError::msg("先写点东西，空的模式不挡任何程序"));
    }
    Ok(conn.execute(
        "INSERT OR IGNORE INTO app_filters (pattern) VALUES (?1)",
        params![p],
    )? > 0)
}

pub fn remove_app_filter(conn: &Connection, pattern: &str) -> AppResult<bool> {
    Ok(conn.execute("DELETE FROM app_filters WHERE pattern = ?1", params![pattern.trim()])? > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(include_str!("../db/schema.sql")).unwrap();
        c
    }

    fn sandbox(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("workbench-installed-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Programs")).unwrap();
        dir
    }

    /// 假 .lnk：resolve_lnk 解不开，于是目标就是它自己——正好用来测「收进来 / 去过重」这套逻辑
    fn fake_lnk(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&p, b"not a real shell link").unwrap();
        p
    }

    fn targets(c: &Connection) -> Vec<String> {
        c.prepare("SELECT name || '|' || source FROM apps ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn add_category(c: &Connection, name: &str) -> i64 {
        c.execute(
            "INSERT INTO categories (name, type) VALUES (?1, 'app')",
            params![name],
        )
        .unwrap();
        c.last_insert_rowid()
    }

    fn inserted(report: SyncReport) -> usize {
        report.inserted
    }

    #[test]
    fn it_collects_shortcuts_in_vendor_subfolders_and_skips_the_rest() {
        let root = sandbox("collect");
        let programs = root.join("Programs");
        let icon_dir = root.join("icons");
        fake_lnk(&programs, "Loomy.lnk");
        fake_lnk(&programs.join("Python"), "Python 3.11 (64-bit).lnk");
        fake_lnk(&programs, "uninstall.lnk");
        fake_lnk(&programs, "某某 卸载.lnk");
        std::fs::write(programs.join("readme.txt"), b"x").unwrap();

        let c = conn();
        let report = scan(&c, std::slice::from_ref(&programs), &icon_dir, false).unwrap();
        // 厂商子目录里的收了，卸载器和非快捷方式没收
        assert_eq!(report.scanned, 2, "{report:?}");
        assert_eq!(inserted(report), 2);
        assert_eq!(
            targets(&c),
            vec!["Loomy|startmenu", "Python 3.11 (64-bit)|startmenu"]
        );

        // 再同步一次一个都不该新增：target 和 .lnk 两种写法都记下来了
        let again = scan(&c, &[programs], &icon_dir, false).unwrap();
        assert_eq!(again.inserted, 0);
        assert_eq!(again.already, 2, "{again:?}");
        assert_eq!(again.total, 2);
    }

    #[test]
    fn an_app_already_registered_by_another_source_is_not_duplicated() {
        let root = sandbox("dedupe");
        let programs = root.join("Programs");
        let lnk = fake_lnk(&programs, "Docker Desktop.lnk");
        let c = conn();
        let net = add_category(&c, "网络工具");
        // 桌面登记那批存的正是 .lnk 自身路径：按路径认出是同一个
        c.execute(
            "INSERT INTO apps (name, target, source, category_id) VALUES ('Docker Desktop.lnk', ?1, 'desktop', ?2)",
            params![lnk.to_string_lossy().to_string(), net],
        )
        .unwrap();

        assert_eq!(
            inserted(scan(&c, &[programs], &root.join("icons"), false).unwrap()),
            0
        );
        assert_eq!(targets(&c), vec!["Docker Desktop.lnk|desktop"]);
    }

    /// 同一个程序换了斜杠方向也算认得：老卡片存反斜杠，手动填的可能是正斜杠
    #[test]
    fn the_same_program_under_a_different_slash_is_not_collected_twice() {
        let root = sandbox("slashes");
        let programs = root.join("Programs");
        let lnk = fake_lnk(&programs, "Loomy.lnk").to_string_lossy().to_string();
        let c = conn();
        c.execute(
            "INSERT INTO apps (name, target, source) VALUES ('Loomy', ?1, 'manual')",
            params![lnk.replace('\\', "/")],
        )
        .unwrap();

        assert_eq!(
            inserted(scan(&c, std::slice::from_ref(&programs), &root.join("icons"), false).unwrap()),
            0
        );
    }

    #[test]
    fn sync_never_touches_an_existing_row() {
        let root = sandbox("no-clobber");
        let programs = root.join("Programs");
        fake_lnk(&programs, "Loomy.lnk");
        let c = conn();
        let mine = add_category(&c, "我的分类");
        // 老行按目标认不出（存的是别处的副本路径），所以会新增一条，
        // 但老行被人改过的名字和分类必须一个字都不动
        c.execute(
            "INSERT INTO apps (name, target, source, category_id, alias) VALUES ('我的洛米', 'F:/other/Loomy.exe', 'manual', ?1, 'l')",
            params![mine],
        )
        .unwrap();

        scan(&c, &[programs], &root.join("icons"), false).unwrap();
        let (name, cat): (String, Option<i64>) = c
            .query_row("SELECT name, category_id FROM apps WHERE alias = 'l'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((name.as_str(), cat), ("我的洛米", Some(mine)));
        assert!(targets(&c).contains(&"Loomy|startmenu".to_string()));
    }

    /// 删掉的卡片下次同步照样收回来：过滤名单在 app_filters，不是删一次的副产品
    #[test]
    fn a_deleted_card_comes_back_on_the_next_sync() {
        let root = sandbox("resync");
        let programs = root.join("Programs");
        fake_lnk(&programs, "Game Launcher.lnk");
        let roots = [programs];
        let icons = root.join("icons");
        let c = conn();

        assert_eq!(inserted(scan(&c, &roots, &icons, false).unwrap()), 1);
        let id: i64 = c.query_row("SELECT id FROM apps", [], |r| r.get(0)).unwrap();
        crate::services::desktop::delete_app(&c, id).unwrap();
        assert_eq!(targets(&c), Vec::<String>::new(), "卡片本身要删掉");

        assert_eq!(inserted(scan(&c, &roots, &icons, false).unwrap()), 1);
    }

    /// 过滤名单挡掉的不写库，但要在账单里看得见，试算还得报上名字
    #[test]
    fn a_filtered_entry_is_counted_not_collected() {
        let root = sandbox("filtered");
        let programs = root.join("Programs");
        fake_lnk(&programs, "Loomy.lnk");
        fake_lnk(&programs, "某某助手.lnk");
        let roots = [programs];
        let icons = root.join("icons");
        let c = conn();
        c.execute("INSERT INTO app_filters (pattern) VALUES ('助手')", []).unwrap();

        let report = scan(&c, &roots, &icons, false).unwrap();
        assert_eq!(report.inserted, 1, "{report:?}");
        assert_eq!(report.filtered, 1);
        assert_eq!(report.samples, vec!["某某助手".to_string()]);
        assert_eq!(targets(&c), vec!["Loomy|startmenu"]);

        // 路径写法也能挡：加一条指到沙箱目录的模式，剩下的那个也不收了
        c.execute(
            "INSERT INTO app_filters (pattern) VALUES (?1)",
            params![root.to_string_lossy().to_string()],
        )
        .unwrap();
        let report = scan(&c, &roots, &icons, false).unwrap();
        assert_eq!(report.inserted, 0, "{report:?}");
        assert_eq!(report.filtered, 1);
    }

    #[test]
    fn the_system_switch_filters_windows_itself_not_third_party_sdks() {
        let root = r"C:\Windows";
        let no_system = Filters { patterns: vec![], system: false };
        let with_system = Filters { patterns: vec![], system: true };

        assert!(hits("资源管理器", &path_key(r"C:\Windows\explorer.exe"), "", &with_system, root));
        assert!(hits(
            "记事本",
            &path_key(r"C:\Users\me\AppData\Local\Microsoft\WindowsApps\Notepad.exe"),
            "",
            &with_system,
            root
        ));
        // 第三方装的 Windows Kits、装在 Program Files 的普通软件都不该被当成系统应用
        assert!(!hits(
            "Application Verifier",
            &path_key(r"C:\Program Files (x86)\Windows Kits\appverif.exe"),
            "",
            &with_system,
            root
        ));
        assert!(!hits("Loomy", &path_key(r"F:\Program\Loomy\loomy.exe"), "", &with_system, root));
        // 关掉开关就一个都不挡
        assert!(!hits("资源管理器", &path_key(r"C:\Windows\explorer.exe"), "", &no_system, root));
    }

    /// 试算只看不动库：数字要和真同步一致，但一行都不写
    #[test]
    fn a_preview_counts_without_writing_anything() {
        let root = sandbox("preview");
        let programs = root.join("Programs");
        fake_lnk(&programs, "Loomy.lnk");
        fake_lnk(&programs, "GIMP.lnk");
        let roots = [programs];
        let icons = root.join("icons");
        let c = conn();

        let preview = scan(&c, &roots, &icons, true).unwrap();
        assert_eq!(preview.inserted, 2, "{preview:?}");
        assert_eq!(preview.total, 2, "试算要把「同步之后会长成多少张」说出来");
        assert_eq!(targets(&c), Vec::<String>::new(), "试算不该写库");

        let real = scan(&c, &roots, &icons, false).unwrap();
        assert_eq!(real.inserted, preview.inserted);
        assert_eq!(real.total, 2);
    }

    /// 名单本身要能加、能去重、能删；空模式挡不住任何东西，直接拒绝
    #[test]
    fn the_filter_list_adds_once_and_deletes() {
        let c = conn();
        assert!(add_app_filter(&c, " 助手 ").unwrap());
        assert!(!add_app_filter(&c, "助手").unwrap(), "同一条不该记两遍");
        assert!(add_app_filter(&c, "   ").is_err(), "空模式不挡任何东西");
        assert_eq!(get_app_filters(&c).unwrap(), vec!["助手".to_string()]);
        assert!(remove_app_filter(&c, "助手").unwrap());
        assert!(get_app_filters(&c).unwrap().is_empty());
    }

    /// 「过滤系统应用」这个开关默认是开的，只有明写成 "0" 才算关
    #[test]
    fn the_system_switch_is_on_unless_explicitly_cleared() {
        let c = conn();
        assert!(read_filters(&c).unwrap().system, "没写过就该当开着");
        crate::services::settings::set_setting(&c, SYSTEM_SWITCH, "0").unwrap();
        assert!(!read_filters(&c).unwrap().system);
        crate::services::settings::set_setting(&c, SYSTEM_SWITCH, "1").unwrap();
        assert!(read_filters(&c).unwrap().system);
    }

    /// 真机核对：拿真的开始菜单跑一遍，看收得出多少条、图标抽到几张。
    /// 库是内存库所以不动他的数据，只有图标 PNG 会照常写进图标缓存。
    /// 要 `CoInitializeEx`：解析 .lnk 和抽图标都走 shell，COM 没初始化就全都解不出——
    /// 应用里那两个 API 是在主线程的命令里跑的，线程已经初始化过了，测试线程得自己补。
    #[test]
    #[ignore]
    fn the_real_start_menu_yields_launchable_apps() {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let c = conn();
        let report = sync_installed(&c).unwrap();
        let with_icon: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM apps WHERE icon_path IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let rows: Vec<(String, String)> = c
            .prepare("SELECT name, target FROM apps ORDER BY name LIMIT 12")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        println!(
            "扫到 {} 个，收进来 {} 个，被过滤 {} 个，抽到图标 {with_icon} 张",
            report.scanned, report.inserted, report.filtered
        );
        for (name, target) in rows {
            println!("  {name} -> {target}");
        }
        assert!(report.inserted > 10, "只收出 {} 个", report.inserted);
        // 解析不出目标的那些会退回 .lnk 自身：少量正常（指向商店应用、指向文件夹），大面积失败就是 COM 没起
        let unresolved: i64 = c
            .query_row("SELECT COUNT(*) FROM apps WHERE lower(target) LIKE '%.lnk'", [], |r| r.get(0))
            .unwrap();
        assert!(
            unresolved * 10 <= report.inserted as i64,
            "{unresolved} 个快捷方式没解析出真实目标，多半是解析这步整片失败了"
        );
        assert!(
            with_icon * 2 > report.inserted as i64,
            "只抽到 {with_icon} 张图标，抽图标这步多半整片失败了"
        );
    }

    #[test]
    fn only_a_launchable_target_counts() {
        assert!(launchable("http://localhost:9200/"), "网址目标算可启动");
        assert!(!launchable(""), "解不出目标的不要");
        assert!(
            !launchable(r"C:\Users\me\Desktop\这个文件不存在.exe"),
            "目标文件被删了的不要"
        );
        let here = std::env::current_exe().unwrap();
        assert!(
            launchable(&here.to_string_lossy()),
            "指向真实文件的算可启动：{}",
            here.display()
        );

        let root = sandbox("dir-lnk");
        // 目标是 .ico 的那种快捷方式是安装程序顺手放的，点了只会开一个图片查看器
        let icon_file = root.join("Charles.ico");
        std::fs::write(&icon_file, b"not really an icon").unwrap();
        assert!(
            !launchable(&icon_file.to_string_lossy()),
            "指向 .ico 的不算应用"
        );

        // 名字叫 .lnk 的目录不该被当成快捷方式收进来
        let programs = root.join("Programs");
        std::fs::create_dir_all(programs.join("Gone.lnk")).unwrap();
        let c = conn();
        assert_eq!(
            inserted(scan(&c, &[programs], &root.join("icons"), false).unwrap()),
            0
        );
    }
}
