//! 「应用」页的数据源：开始菜单里的快捷方式。
//!
//! 不用注册表的 `Uninstall` 键：那边 122 条里只有约 40 条有 `InstallLocation`，
//! 其余的 `DisplayIcon` 常常指着卸载器图标（`…\uninstallerIcon.ico`、`X.exe,0`），
//! 拿它当启动目标会有一大半点不动。开始菜单的快捷方式名字是给人看的、
//! `.lnk` 又能解析出真 exe，图标也照桌面上那套抽法。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::error::AppResult;
use crate::services::desktop::{extract_icon, extension_of, get_icon_dir, launch_target};

/// 开始菜单本来就有多层厂商目录，给三层的量
const MAX_DEPTH: usize = 3;

struct Discovered {
    name: String,
    target: String,
    lnk: String,
}

/// 扫一遍开始菜单，把没见过的应用收进启动器。只插不改：
/// 用户改过的名字、手动归好的分类、抽好的图标都不该被一次同步抹掉。
/// 删掉的卡片下次同步会再收回来——这一页就是「本机装了什么」的镜子，不做拉黑。
pub fn sync_installed(conn: &Connection) -> AppResult<usize> {
    let icon_dir = get_icon_dir()?;
    let roots = crate::win::desktop::all_program_menus()
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    sync_from_roots(conn, &roots, &icon_dir)
}

/// 真正干活的一层：目录和图标缓存都从外面进来，测试就能拿临时目录真跑一遍
pub(crate) fn sync_from_roots(
    conn: &Connection,
    roots: &[PathBuf],
    icon_dir: &Path,
) -> AppResult<usize> {
    let mut found = Vec::new();
    for root in roots {
        collect(root, 0, &mut found);
    }

    // 「见过」按两种写法算：卡片存的 target，以及快捷方式自己的路径
    //（桌面登记那批存的就是 .lnk 路径，同一个程序两种写法都可能是它）
    let mut known: HashSet<String> = conn
        .prepare("SELECT target FROM apps")?
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .map(|t| path_key(&t))
        .collect();

    let _ = std::fs::create_dir_all(icon_dir);
    let mut inserted = 0;
    for item in found {
        let lnk = path_key(&item.lnk);
        let target = path_key(&item.target);
        if known.contains(&lnk) || known.contains(&target) {
            continue;
        }
        let icon = extract_icon(icon_dir, &[item.lnk.as_str(), item.target.as_str()]);
        conn.execute(
            "INSERT INTO apps (name, target, source, icon_path) VALUES (?1, ?2, 'startmenu', ?3)",
            params![item.name, item.target, icon],
        )?;
        known.insert(target);
        known.insert(lnk);
        inserted += 1;
    }
    Ok(inserted)
}

/// 比对用的写法：老卡片那批存的是 `F:\\Program\\...`，手动填的可能打成 `F:/Program/...`，
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
    // 目标是图标文件的那种快捷方式是安装程序顺手放进去的（Charles、Cpolar 都这样），
    // 点它只会开一个图片查看器，不算一个应用
    !matches!(extension_of(path).as_str(), "ico" | "cur")
}

/// 卸载器链接不是「打开应用」，收进来只会多出一堆点了没反应的卡片
fn is_uninstaller(file_name: &str) -> bool {
    let n = file_name.to_lowercase();
    n.starts_with("uninstall") || n.contains("卸载")
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
        let n = sync_from_roots(&c, std::slice::from_ref(&programs), &icon_dir).unwrap();
        // 厂商子目录里的收了，卸载器和非快捷方式没收
        assert_eq!(n, 2, "{:?}", targets(&c));
        assert_eq!(
            targets(&c),
            vec!["Loomy|startmenu", "Python 3.11 (64-bit)|startmenu"]
        );

        // 再同步一次一个都不该新增：target 和 .lnk 两种写法都记下来了
        assert_eq!(sync_from_roots(&c, &[programs], &icon_dir).unwrap(), 0);
    }

    fn add_category(c: &Connection, name: &str) -> i64 {
        c.execute(
            "INSERT INTO categories (name, type) VALUES (?1, 'app')",
            params![name],
        )
        .unwrap();
        c.last_insert_rowid()
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
            sync_from_roots(&c, &[programs], &root.join("icons")).unwrap(),
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
            sync_from_roots(&c, std::slice::from_ref(&programs), &root.join("icons")).unwrap(),
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

        sync_from_roots(&c, &[programs], &root.join("icons")).unwrap();
        let (name, cat): (String, Option<i64>) = c
            .query_row("SELECT name, category_id FROM apps WHERE alias = 'l'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((name.as_str(), cat), ("我的洛米", Some(mine)));
        assert!(targets(&c).contains(&"Loomy|startmenu".to_string()));
    }

    /// 删掉的卡片下次同步照样收回来：这一页是「本机装了什么」的镜子，不做拉黑名单
    #[test]
    fn a_deleted_card_comes_back_on_the_next_sync() {
        let root = sandbox("resync");
        let programs = root.join("Programs");
        fake_lnk(&programs, "Game Launcher.lnk");
        let roots = [programs];
        let icons = root.join("icons");
        let c = conn();

        assert_eq!(sync_from_roots(&c, &roots, &icons).unwrap(), 1);
        let id: i64 = c.query_row("SELECT id FROM apps", [], |r| r.get(0)).unwrap();
        crate::services::desktop::delete_app(&c, id).unwrap();
        assert_eq!(targets(&c), Vec::<String>::new(), "卡片本身要删掉");

        assert_eq!(
            sync_from_roots(&c, &roots, &icons).unwrap(),
            1,
            "重新同步该把它收回来"
        );
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
        let n = sync_installed(&c).unwrap();
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
        println!("新收录 {n} 个程序，其中 {with_icon} 个抽到了图标");
        for (name, target) in rows {
            println!("  {name} -> {target}");
        }
        assert!(n > 10, "开始菜单只收出 {n} 个，多半是已知文件夹路径没拿到");
        // 解析不出目标的那些会退回 .lnk 自身：少量正常（指向商店应用、指向文件夹），大面积失败就是 COM 没起
        let unresolved: i64 = c
            .query_row("SELECT COUNT(*) FROM apps WHERE lower(target) LIKE '%.lnk'", [], |r| r.get(0))
            .unwrap();
        assert!(
            unresolved * 10 <= n as i64,
            "{unresolved}/{n} 个快捷方式没解析出真实目标，多半是解析这步整片失败了"
        );
        assert!(
            with_icon * 2 > n as i64,
            "只抽到 {with_icon}/{n} 张图标，抽图标这步多半整片失败了"
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
            sync_from_roots(&c, &[programs], &root.join("icons")).unwrap(),
            0
        );
    }
}
