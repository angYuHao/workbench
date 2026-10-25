//! 测试基本全程离线：条目手搭、快捷方式索引手搭、图标目录指到临时目录。
//! 只有那条 `#[ignore]` 的会去读真注册表——改过滤词表之前先在真机跑它，
//! 挡掉和收进来两张名单都会整片打出来。

use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::services::desktop::get_icon_dir;

use super::filter::{find_program, hits, is_system, read_filters, system_root, SYSTEM_SWITCH};
use super::scan::{read_entries, shortcut_index};
use super::store::plan;
use super::target::{
    clean_path, command_exe, find_shortcut, is_uninstaller, program_target, resolve, usable_exe,
};
use super::*;

fn conn() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch(include_str!("../../db/schema.sql")).unwrap();
    c
}

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("workbench-registry-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn touch(dir: &Path, name: &str) -> String {
    let p = dir.join(name);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&p, b"MZ\x90\x00not a real binary").unwrap();
    p.to_string_lossy().to_string()
}

fn entry(name: &str) -> Entry {
    Entry {
        name: name.to_string(),
        reg_key: format!(r"HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\{name}"),
        ..Default::default()
    }
}

fn rows(c: &Connection) -> Vec<String> {
    c.prepare("SELECT name || '|' || source || '|' || COALESCE(description,'') FROM apps ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn plan_with(conn: &Connection, entries: &[Entry], shortcuts: &[Shortcut], icons: &Path, dry: bool) -> SyncReport {
    plan(conn, entries, shortcuts, icons, dry).unwrap()
}

fn inserted(report: SyncReport) -> usize {
    report.inserted
}

/// DisplayIcon 里那个真存在的 exe 是最准的目标，`,0` 那种序号得剥掉
#[test]
fn a_display_icon_exe_becomes_the_target() {
    let dir = sandbox("display-icon");
    let exe = touch(&dir, r"Foo\foo.exe");
    let e = Entry { display_icon: format!("{exe},0"), ..entry("Foo") };

    let r = resolve(&e, &[]);
    assert_eq!(r.found, Found::Exe);
    assert_eq!(r.target, exe);
    assert_eq!(r.icons, vec![exe.clone()]);

    let c = conn();
    let report = plan_with(&c, std::slice::from_ref(&e), &[], &dir.join("icons"), false);
    assert_eq!(report.inserted, 1, "{report:?}");
    assert_eq!(rows(&c), vec!["Foo|registry|".to_string()]);
}

/// 卸载器图标不能当目标：MSI 那一堆 `C:\Windows\Installer\{GUID}\ARPIcon.ico`    /// 和 `uninstallerIcon.ico` 都只是图标，卡片点开该是安装目录
#[test]
fn an_uninstaller_icon_is_an_icon_not_a_target() {
    let dir = sandbox("uninstaller-icon");
    let ico = touch(&dir, r"Foo\uninstallerIcon.ico");
    let unins = touch(&dir, r"Foo\unins000.exe");
    let install = dir.join("Foo");
    let e = Entry {
        display_icon: ico.clone(),
        uninstall: format!(r#""{unins}" /SILENT"#),
        install_location: install.to_string_lossy().to_string(),
        ..entry("Foo")
    };

    let r = resolve(&e, &[]);
    assert_eq!(r.found, Found::Folder, "卸载器不该被当成打开应用");
    assert_eq!(r.target, install.to_string_lossy().to_string());
    // 图标还是从 DisplayIcon 抽：那张就是桌面上看到的
    assert_eq!(r.icons, vec![ico]);
}

/// 名单里的模式常是从卡片名字照抄的，带着括号和版本号：这种写法也得命中名字
/// 假 .lnk：resolve_lnk 解不开，于是目标就是它自己——正好当「只有快捷方式的程序」用
fn fake_lnk(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, b"not a real shell link").unwrap();
    p
}

/// 第二遍采集：注册表没写卸载项、只有快捷方式的东西（商店应用和绿色版）也进卡片
/// 帮助文档不是应用：`.chm` / `.htm` 那种附属快捷方式不该长成卡片
#[test]
fn a_doc_shortcut_is_not_an_app() {
    let dir = sandbox("doc-lnk");
    let exe = touch(&dir, "app.exe");
    let chm = touch(&dir, "Guide.chm");
    let htm = touch(&dir, "notes.htm");
    let lnk = fake_lnk(&dir, "Store App.lnk");

    assert!(program_target(&exe));
    assert!(
        program_target(&lnk.to_string_lossy()),
        "解不出真目标的 .lnk 自己就是入口（商店应用全靠它）"
    );
    assert!(program_target("https://example.com/"));
    assert!(!program_target(&chm), "帮助文档不该收成应用");
    assert!(!program_target(&htm));
    assert!(!program_target(""));
}

/// 一个启动器养一片程序：Steam 每个游戏的 DisplayIcon 都写 `steam.exe`，按目标文件去重
/// 会只留第一张——Steam 自己就是这么从页面上消失的
#[test]
fn a_shared_launcher_exe_does_not_swallow_the_programs_behind_it() {
    let dir = sandbox("steam-family");
    let launcher = touch(&dir, r"Steam\\steam.exe");
    let entries = vec![
        Entry {
            display_icon: launcher.clone(),
            ..entry("Steam")
        },
        Entry {
            display_icon: launcher.clone(),
            ..entry("黑神话：悟空")
        },
    ];
    let c = conn();

    let report = plan(&c, &entries, &[], &dir.join("icons"), false).unwrap();
    assert_eq!(report.inserted, 2, "{report:?}");
    assert_eq!(report.already, 0, "共用一个 steam.exe 不算重复");
    assert_eq!(rows(&c).len(), 2);
}

/// 名字太短的不当「一段」看：Git 不该把 GitHub CLI 吞掉
#[test]
fn a_three_letter_name_is_not_treated_as_a_prefix_of_another() {
    let dir = sandbox("git-vs-github");
    let git = touch(&dir, r"Git\\git.exe");
    let gh = touch(&dir, r"GitHub\\gh.exe");
    let entries = vec![
        Entry {
            display_icon: git,
            ..entry("Git")
        },
        Entry {
            display_icon: gh,
            ..entry("GitHub CLI")
        },
    ];
    let c = conn();

    let report = plan(&c, &entries, &[], &dir.join("icons"), false).unwrap();
    assert_eq!(report.inserted, 2, "{report:?}");
}

#[test]
fn start_menu_shortcuts_the_registry_never_mentioned_also_become_cards() {
    let dir = sandbox("orphan-lnk");
    let programs = dir.join("Programs");
    std::fs::create_dir_all(&programs).unwrap();
    let green = fake_lnk(&programs, "Green App.lnk");
    let scs = shortcut_index(std::slice::from_ref(&programs));
    let c = conn();

    let report = plan(&c, &[], &scs, &dir.join("icons"), false).unwrap();
    assert_eq!(report.inserted, 1, "{report:?}");
    assert_eq!(rows(&c), vec!["Green App|startmenu|"]);
    let again = plan(&c, &[], &scs, &dir.join("icons"), false).unwrap();
    assert_eq!(again.inserted, 0, "再来一次不该长出第二张：{again:?}");
    assert_eq!(again.already, 1);
    assert!(green.is_file());
}

/// 注册表那条已经靠这个 .lnk 认出目标了，第二遍就不能再收一张
#[test]
fn a_shortcut_the_registry_row_already_uses_is_not_collected_again() {
    let dir = sandbox("claimed-lnk");
    let programs = dir.join("Programs");
    std::fs::create_dir_all(&programs).unwrap();
    fake_lnk(&programs, "Loomy.lnk");
    let scs = shortcut_index(std::slice::from_ref(&programs));
    let c = conn();

    let report = plan(&c, &[entry("Loomy")], &scs, &dir.join("icons"), false).unwrap();
    assert_eq!(report.inserted, 1, "{report:?}");
    assert_eq!(rows(&c), vec!["Loomy|registry|"], "同一个程序不该收两张");
}

/// 同一家厂商的兄弟产品：`Qoder CN 0.4.11` 不许因为「qoder 是 qodercn0411 的一段」而
/// 匹配到国际版的快捷方式上——那张卡点开会打开另一个产品。索引里故意把短名字排在前面，
/// 一旦匹配掉到「互相包含取最短」那一档就会红。
#[test]
fn a_regional_sibling_finds_its_own_shortcut_not_the_shorter_namesake() {
    let dir = sandbox("sibling-lnk");
    let international = touch(&dir, "Qoder.exe");
    let cn = touch(&dir, "Qoder CN.exe");
    let scs = [
        Shortcut {
            name: "Qoder".to_string(),
            key: name_key("Qoder"),
            lnk: international.clone(),
            target: international.clone(),
        },
        Shortcut {
            name: "Qoder CN".to_string(),
            key: name_key("Qoder CN"),
            lnk: cn.clone(),
            target: cn.clone(),
        },
    ];
    let e = Entry {
        display_icon: r"F:\Program\Qoder CN\uninstallerIcon.ico".to_string(),
        uninstall: r#""F:\Program\Qoder CN\Uninstall Qoder CN.exe" /currentuser"#.to_string(),
        ..entry("Qoder CN 0.4.11")
    };

    let r = resolve(&e, &scs);
    assert_eq!(path_key(&r.target), path_key(&cn), "该认 CN 那个快捷方式：{r:?}");
}

/// 来源体检：卸载了的、以及被新版本顶掉旧卸载项而留下的孤儿卡片，同步时要清掉；
/// 用户自己加的（manual）永远不许动 —— 它没有「来源」可以对。
#[test]
fn sweep_clears_orphans_but_never_manual_cards() {
    let dir = sandbox("sweep");
    let exe = touch(&dir, r"Still\still.exe");
    let alive = Entry { display_icon: format!("{exe},0"), ..entry("Still Here") };
    let c = conn();
    c.execute("INSERT INTO apps (name, target, source) VALUES ('Still Here', ?1, 'registry')", params![exe]).unwrap();
    c.execute("INSERT INTO apps (name, target, source) VALUES ('Gone App', 'x', 'registry')", []).unwrap();
    c.execute("INSERT INTO apps (name, target, source) VALUES ('My Own', 'x', 'manual')", []).unwrap();

    let report = plan(&c, &[alive], &[], &dir.join("icons"), false).unwrap();

    assert_eq!(report.removed, 1, "{report:?}");
    assert_eq!(report.removed_names, vec!["Gone App".to_string()]);
    assert_eq!(
        rows(&c),
        vec!["My Own|manual|".to_string(), "Still Here|registry|".to_string()],
        "还在的和手加的都得留着"
    );
}

/// 读不到注册表（权限、异常机器）不等于本机什么都没装。这时候最不该做的就是清空整页。
#[test]
fn an_empty_registry_read_removes_nothing() {
    let c = conn();
    c.execute("INSERT INTO apps (name, target, source) VALUES ('Gone App', 'x', 'registry')", []).unwrap();

    let report = plan(&c, &[], &[], &sandbox("sweep-empty").join("icons"), false).unwrap();

    assert_eq!(report.removed, 0, "一条注册表都没读到，不许当成全卸载了");
    assert_eq!(rows(&c).len(), 1, "卡片得原样留着");
}

/// 试算（设置页那个「试算一下」）只报数不动手。
#[test]
fn a_dry_run_reports_the_sweep_without_doing_it() {
    let dir = sandbox("sweep-dry");
    let c = conn();
    c.execute("INSERT INTO apps (name, target, source) VALUES ('Gone App', 'x', 'registry')", []).unwrap();

    let report = plan(&c, &[entry("Something Else")], &[], &dir.join("icons"), true).unwrap();

    assert_eq!(report.removed, 1, "试算也要说清会清掉几条：{report:?}");
    assert_eq!(rows(&c).len(), 1, "但真一条都不能删");
}

/// 用户改过名字的卡片：名字对不上注册表了，但路径还是本机在用的那一条 —— 不算孤儿。
#[test]
fn a_renamed_card_is_kept_because_its_target_is_still_live() {
    let dir = sandbox("sweep-renamed");
    let exe = touch(&dir, r"Tool\tool.exe");
    let alive = Entry { display_icon: format!("{exe},0"), ..entry("Original Tool") };
    let c = conn();
    c.execute("INSERT INTO apps (name, target, source) VALUES ('我的工具', ?1, 'registry')", params![exe]).unwrap();

    let report = plan(&c, &[alive], &[], &dir.join("icons"), false).unwrap();

    assert_eq!(report.removed, 0, "只是改了名字，不该被当成卸载：{report:?}");
    assert_eq!(rows(&c).len(), 2, "原名那条作为新卡片收进来，改名的这张留着");
}
#[test]
fn a_startmenu_card_is_judged_by_whether_its_target_still_exists() {
    let dir = sandbox("sweep-sm");
    let lnk = touch(&dir, "Keep.lnk");
    let c = conn();
    c.execute("INSERT INTO apps (name, target, source) VALUES ('Keep', ?1, 'startmenu')", params![lnk]).unwrap();
    c.execute("INSERT INTO apps (name, target, source) VALUES ('Stale', 'D:\\没有这种盘\\x.lnk', 'startmenu')", []).unwrap();

    let report = plan(&c, &[entry("Unrelated")], &[], &dir.join("icons"), false).unwrap();

    assert_eq!(report.removed, 1, "{report:?}");
    assert_eq!(report.removed_names, vec!["Stale".to_string()]);
    assert_eq!(rows(&c), vec!["Keep|startmenu|".to_string()]);
}
/// 「卡片指到兄弟产品」这类事第一个该跑它——匹配掉到「互相包含取最短」那一档，
/// 十有八九是因为本该精确命中的那条压根没进索引。
#[test]
#[ignore]
fn which_shortcuts_make_it_into_the_index() {
    let scs = shortcut_index(
        &crate::win::desktop::all_program_menus()
            .into_iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>(),
    );
    let needle = std::env::var("WB_PROBE").unwrap_or_else(|_| "qoder".to_string());
    println!("索引 {} 条，筛「{needle}」：", scs.len());
    for s in scs.iter().filter(|s| s.key.contains(&needle)) {
        println!("  key={}\n    lnk={}\n    ->  {}", s.key, s.lnk, s.target);
    }
    // 同一把筛子过一遍真机解析：卡片指错东西时，这两段对着看就知道是索引缺了还是匹配挑错了
    println!("\n注册表里带「{needle}」的条目解析成：");
    for e in read_entries().into_iter().filter(|e| e.name.to_lowercase().contains(&needle)) {
        let r = resolve(&e, &scs);
        println!("  {}\n    [{:?}] -> {}\n    图标候选 {:?}", e.name, r.found, r.target, r.icons);
    }
}
/// 这条是给上一条划边界的——版本号后缀可以削，单独一个标题数字不行。
#[test]
fn two_numbered_titles_are_two_programs() {
    let c = conn();
    c.execute("INSERT INTO apps (name, target, source) VALUES ('Halo 3', 'x', 'registry')", [])
        .unwrap();
    let known: HashMap<String, ()> = [(name_key("Halo 3"), ())].into_iter().collect();

    assert!(
        find_program(&known, &name_key("Halo 4")).is_none(),
        "Halo 3 和 Halo 4 是两个程序，不能判成同一个"
    );
    assert!(
        find_program(&known, &name_key("Halo 3")).is_some(),
        "同名同版本才是同一个"
    );
}
/// 这条测的是「已有卡片直接 continue」那个洞——图标不更新就是这么来的。
/// 这条测的是「已有卡片直接 continue」那个洞——图标不更新就是这么来的。
#[test]
fn an_updated_app_follows_the_new_path_without_losing_user_edits() {
    let dir = sandbox("refreshed-app");
    let old = touch(&dir, r"Old\app.exe");
    let new = touch(&dir, r"New\app.exe");
    let c = conn();
    c.execute(
        "INSERT INTO apps (name, alias, target, source, icon_path) VALUES ('Foo', '我的Foo', ?1, 'registry', '旧图标.png')",
        params![old],
    )
    .unwrap();

    let mut e = entry("Foo");
    e.display_icon = new.clone();
    let report = plan(&c, &[e], &[], &dir.join("icons"), false).unwrap();

    assert_eq!(report.inserted, 0, "不该再插一张卡片");
    assert_eq!(report.already, 1, "名字判成同一个程序");
    assert_eq!(report.updated, 1, "启动路径变了要顺手追回来：{report:?}");

    let (target, alias, icon): (String, String, Option<String>) = c
        .query_row("SELECT target, alias, icon_path FROM apps", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!(target, new, "路径该跟着更新");
    assert_eq!(alias, "我的Foo", "用户改的别名不能动");
    assert_eq!(icon.as_deref(), Some("旧图标.png"), "这张假 exe 抽不出图标，那就留着旧的而不是清空");
}

/// 开始菜单里那堆系统自带工具照样归「系统应用」那条管；开关关掉才收
#[test]
fn start_menu_system_tools_stay_filtered_by_the_same_switch() {
    let dir = sandbox("sys-lnk");
    let programs = dir.join("Programs");
    std::fs::create_dir_all(&programs).unwrap();
    fake_lnk(&programs, "讲述人.lnk");
    let mut scs = shortcut_index(std::slice::from_ref(&programs));
    scs[0].target = Path::new(&system_root()).join("system32").join("narrator.exe").to_string_lossy().to_string();
    let icons = dir.join("icons");

    let c = conn();
    let report = plan(&c, &[], &scs, &icons, false).unwrap();
    assert_eq!(report.filtered, 1, "{report:?}");
    assert_eq!(report.samples, vec!["讲述人".to_string()]);
    assert_eq!(rows(&c), Vec::<String>::new());

    crate::services::settings::set_setting(&c, SYSTEM_SWITCH, "0").unwrap();
    let off = plan(&c, &[], &scs, &icons, false).unwrap();
    assert_eq!(off.inserted, 1, "关掉开关就该把它收回来：{off:?}");
}
#[test]
fn a_pattern_copied_from_the_card_name_still_blocks_it() {
    let dir = sandbox("punct-pattern");
    let exe = touch(&dir, r"Zip\7zFM.exe");
    let c = conn();
    add_app_filter(&c, "7-Zip 23.01 (x64)").unwrap();
    let e = Entry { display_icon: exe, ..entry("7-Zip 23.01 (x64)") };

    let report = plan_with(&c, &[e], &[], &dir.join("icons"), false);
    assert_eq!(report.filtered, 1, "{report:?}");
    assert_eq!(report.inserted, 0);
    assert_eq!(report.samples, vec!["7-Zip 23.01 (x64)".to_string()]);
}

/// 卸载器、`Docker Desktop Installer.exe` 这种安装器、还有 Package Cache 里那个
/// setup 外壳，点开都是安装/修复向导，不是程序本体：一律不算目标
#[test]
fn an_installer_exe_is_never_the_target() {
    let dir = sandbox("installer");
    let installer = touch(&dir, r"Docker\Docker Desktop Installer.exe");
    let cached = touch(&dir, r"Package Cache\{62b10058}\AuraServiceSetup.exe");
    std::fs::create_dir_all(dir.join("Aura")).unwrap();

    let by_name = resolve(&Entry { display_icon: installer, ..entry("Docker Desktop") }, &[]);
    assert_eq!(by_name.found, Found::Folder, "安装器 exe 不该被当成目标");
    assert_eq!(by_name.target, dir.join("Docker").to_string_lossy().to_string());

    let by_path = resolve(
        &Entry {
            display_icon: cached,
            install_location: dir.join("Aura").to_string_lossy().to_string(),
            ..entry("AURA Service")
        },
        &[],
    );
    assert_eq!(by_path.found, Found::Folder, "Package Cache 里那个是重装程序");
    assert_eq!(by_path.target, dir.join("Aura").to_string_lossy().to_string());
}

#[test]
fn a_moved_install_folder_still_resolves() {
    let dir = sandbox("moved");
    let real = touch(&dir, r"Navicat Premium 15\navicat.exe");
    let stale = dir.join(r"Navicat Premium 15\Navicat Premium 15\navicat.exe");
    let e = Entry {
        display_icon: stale.to_string_lossy().to_string(),
        install_location: stale.parent().unwrap().to_string_lossy().to_string(),
        ..entry("PremiumSoft Navicat Premium 15.0")
    };

    let r = resolve(&e, &[]);
    assert_eq!(r.found, Found::Exe, "搬过目录留下的旧路径要能退一层认出来");
    assert_eq!(path_key(&r.target), path_key(&real));
}

/// WPS 那种布局：`DisplayIcon` 是自更新留下的 0 字节空壳，注册表给的卸载器目录
/// `<版本>\utility\uninst.exe` 旁边也没主程序，真身在隔壁的 `<版本>\office6\wps.exe`
#[test]
fn wps_layout_skips_the_empty_stub_and_finds_the_real_program() {
    let dir = sandbox("wps");
    let stub = dir.join("ksolaunch.exe");
    std::fs::write(&stub, b"").unwrap();
    let real = touch(&dir, r"WPS Office\12.1.0.25865\office6\wps.exe");
    touch(&dir, r"WPS Office\12.1.0.25865\office6\wpsoffice.exe");
    touch(&dir, r"WPS Office\12.1.0.25865\office6\et.exe");
    let uninst = touch(&dir, r"WPS Office\12.1.0.25865\utility\uninst.exe");
    assert!(
        !usable_exe(&stub.to_string_lossy()),
        "0 字节的 exe 不该当成能跑的东西"
    );

    let e = Entry {
        display_icon: stub.to_string_lossy().to_string(),
        uninstall: format!(r#""{}""#, uninst),
        ..entry("WPS Office (12.1.0.25865)")
    };
    let r = resolve(&e, &[]);
    assert_eq!(r.found, Found::Exe, "空壳不能当目标，得往隔壁的 office6 里找");
    assert_eq!(path_key(&r.target), path_key(&real));
}

#[test]
fn the_uninstaller_folder_holds_the_real_program() {
    let dir = sandbox("unins-folder");
    let exe = touch(&dir, r"Navicat\navicat.exe");
    let unins = touch(&dir, r"Navicat\unins000.exe");
    // Inno Setup 那一类：InstallLocation 是空的，只有卸载串说到这个目录
    let e = Entry {
        uninstall: format!(r#""{unins}""#),
        ..entry("PremiumSoft Navicat Premium 15.0")
    };

    let r = resolve(&e, &[]);
    assert_eq!(r.found, Found::Exe, "卸载器旁边就是主程序，得进去找");
    assert_eq!(path_key(&r.target), path_key(&exe));
}

#[test]
fn the_package_cache_folder_is_not_scanned_for_a_program() {
    let dir = sandbox("cache-scan");
    touch(&dir, r"Package Cache\{62b10058}\tool.exe");
    let unins = touch(&dir, r"Package Cache\{62b10058}\unins000.exe");
    let e = Entry { uninstall: format!(r#""{unins}""#), ..entry("Some MSI Thing") };

    let r = resolve(&e, &[]);
    assert_eq!(r.found, Found::Nothing, "MSI 缓存目录里不该被翻出一个「主程序」");
}

/// 开始菜单的同名快捷方式排在安装目录前面：目标准、图标也是 shell 算好的那张
#[test]
fn a_same_name_shortcut_beats_the_folder_fallback() {
    let dir = sandbox("shortcut");
    let real = touch(&dir, r"Loomy\loomy.exe");
    let install = dir.join("Loomy");
    let sc = Shortcut {
        name: "Loomy".into(),
        key: name_key("Loomy"),
        lnk: dir.join("Loomy.lnk").to_string_lossy().to_string(),
        target: real.clone(),
    };
    let e = Entry {
        install_location: install.to_string_lossy().to_string(),
        ..entry("Loomy")
    };

    let r = resolve(&e, std::slice::from_ref(&sc));
    assert_eq!(r.found, Found::Shortcut);
    assert_eq!(r.target, real);
    assert_eq!(r.icons[0], sc.lnk, "抽图标要从快捷方式来");
}

/// 安装目录顶层那个和程序同名的 exe 也算找到了
#[test]
fn an_exe_named_like_the_program_inside_the_install_dir_is_used() {
    let dir = sandbox("dir-exe");
    let exe = touch(&dir, r"Gimp\gimp.exe");
    touch(&dir, r"Gimp\unins000.exe");
    let e = Entry { install_location: dir.join("Gimp").to_string_lossy().to_string(), ..entry("GIMP") };

    let r = resolve(&e, &[]);
    assert_eq!(r.found, Found::Exe);
    assert_eq!(path_key(&r.target), path_key(&exe));
}

/// 什么路径都没有的：报数，不收一张点不动的卡片
#[test]
fn an_entry_without_any_path_is_counted_not_collected() {
    let dir = sandbox("nothing");
    let c = conn();
    let report = plan_with(&c, &[entry("幽灵程序")], &[], &dir.join("icons"), false);
    assert_eq!(report.unresolved, 1, "{report:?}");
    assert_eq!(report.inserted, 0);
    assert_eq!(rows(&c), Vec::<String>::new());
}

/// 同一个程序在 64 位和 32 位视图里各登记一次，只该收一张
#[test]
fn the_same_product_in_two_views_is_collected_once() {
    let dir = sandbox("two-views");
    let exe = touch(&dir, r"Two\two.exe");
    let mut first = Entry { display_icon: exe.clone(), ..entry("Two Tool") };
    first.version = "1.2.3".into();
    first.publisher = "某某科技".into();
    let second = Entry { display_icon: exe.clone(), ..entry("Two Tool") };

    let c = conn();
    let report = plan_with(&c, &[first, second], &[], &dir.join("icons"), false);
    assert_eq!(report.inserted, 1, "{report:?}");
    assert_eq!(report.already, 1, "第二条得认出是同一个");
    assert_eq!(rows(&c), vec!["Two Tool|registry|某某科技 · 1.2.3"]);
}

/// 「应用」页那批开始菜单卡片存的是解析出的 exe：名字或路径任一对上就不重复收
#[test]
fn a_card_from_the_start_menu_page_blocks_the_registry_row() {
    let dir = sandbox("startmenu-block");
    let exe = touch(&dir, r"Docker\docker-desktop.exe");
    let c = conn();
    c.execute(
        "INSERT INTO apps (name, target, source) VALUES ('Docker Desktop', ?1, 'startmenu')",
        params![path_key(&exe).replace('/', r"\")],
    )
    .unwrap();
    let e = Entry { display_icon: exe, ..entry("Docker Desktop") };

    let report = plan_with(&c, &[e], &[], &dir.join("icons"), false);
    assert_eq!(report.inserted, 0, "{report:?}");
    assert_eq!(report.already, 1);
    assert_eq!(rows(&c), vec!["Docker Desktop|startmenu|"]);
}

/// 老卡片只改过名字、路径认不出时，新条目照常收，但那一行一个字都不动
#[test]
fn sync_never_touches_an_existing_row() {
    let dir = sandbox("no-clobber");
    let exe = touch(&dir, r"Loomy\loomy.exe");
    let c = conn();
    c.execute(
        "INSERT INTO apps (name, target, source, category_id, alias) VALUES ('我的洛米', 'F:/other/Loomy.exe', 'manual', NULL, 'l')",
        [],
    )
    .unwrap();
    let e = Entry { display_icon: exe, ..entry("Loomy") };

    plan_with(&c, &[e], &[], &dir.join("icons"), false);
    let (name, target): (String, String) = c
        .query_row("SELECT name, target FROM apps WHERE alias = 'l'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((name.as_str(), target.as_str()), ("我的洛米", "F:/other/Loomy.exe"));
    assert!(rows(&c).contains(&"Loomy|registry|".to_string()));
}

/// 删掉的卡片下次同步照样收回来：过滤名单在 app_filters，不是删一次的副产品
#[test]
fn a_deleted_card_comes_back_on_the_next_sync() {
    let dir = sandbox("resync");
    let exe = touch(&dir, r"Game\launcher.exe");
    let e = Entry { display_icon: exe.clone(), ..entry("Game Launcher") };
    let icons = dir.join("icons");
    let c = conn();

    assert_eq!(inserted(plan_with(&c, std::slice::from_ref(&e), &[], &icons, false)), 1);
    let id: i64 = c.query_row("SELECT id FROM apps", [], |r| r.get(0)).unwrap();
    crate::services::desktop::delete_app(&c, id).unwrap();
    assert_eq!(rows(&c), Vec::<String>::new());
    assert_eq!(inserted(plan_with(&c, &[e], &[], &icons, false)), 1);
}

/// 名单能挡名字，也能挡安装目录、注册表键和 DisplayIcon 路径
#[test]
fn a_filtered_entry_is_counted_not_collected() {
    let dir = sandbox("filtered");
    let exe = touch(&dir, r"助手\x.exe");
    let icons = dir.join("icons");
    let c = conn();
    c.execute("INSERT INTO app_filters (pattern) VALUES ('助手')", []).unwrap();

    let by_name = Entry { display_icon: exe.clone(), ..entry("某某助手") };
    let report = plan_with(&c, &[by_name, entry("Loomy")], &[], &icons, false);
    assert_eq!(report.inserted, 0, "{report:?}");
    assert_eq!(report.filtered, 1, "名字里带模式的这条要挡住");
    assert_eq!(report.unresolved, 1, "Loomy 什么路径都没有");
    assert_eq!(report.samples, vec!["某某助手".to_string()]);
    assert_eq!(rows(&c), Vec::<String>::new());

    // 路径写法也能挡：DisplayIcon 里那段目录就在名单里
    let mut second = Entry { display_icon: exe.clone(), ..entry("另一个") };
    second.install_location = dir.join("助手").to_string_lossy().to_string();
    let c2 = conn();
    c2.execute("INSERT INTO app_filters (pattern) VALUES ('助手')", []).unwrap();
    let report = plan_with(&c2, &[second], &[], &icons, false);
    assert_eq!(report.filtered, 1, "{report:?}");
}

/// 「系统应用」在注册表里的三种长相都要挡；但 MSI 的图标住在 Windows 目录里，
/// 那不能算系统应用——否则正经软件会被成片挡掉
#[test]
fn system_entries_are_filtered_and_msi_icons_are_not() {
    let dir = sandbox("system");
    let exe = touch(&dir, r"Real\real.exe");
    let root = r"C:\Windows";

    let with_system = Filters { patterns: vec![], system: true };
    let no_system = Filters { patterns: vec![], system: false };

    let mut component = entry("某系统组件");
    component.system_component = true;
    assert!(hits(&component, &exe, &with_system, root));

    let mut patch = entry("某程序 1.2 补丁");
    patch.is_patch = true;
    assert!(hits(&patch, &exe, &with_system, root));

    assert!(hits(&entry("Microsoft Edge Update"), &exe, &with_system, root), "更新程序是附属程序");
    assert!(hits(&entry("Microsoft Visual C++ 2015  redistributable - x64"), &exe, &with_system, root));
    // 驱动
    assert!(hits(&entry("Realtek High Definition Audio Driver"), &exe, &with_system, root));
    assert!(hits(&entry("英特尔(R) 无线显示器驱动"), &exe, &with_system, root));
    assert!(hits(&entry("Microsoft ODBC Driver 17 for SQL Server"), &exe, &with_system, root));
    assert!(hits(&entry("AMD Chipset Software"), &exe, &with_system, root));
    // 运行环境
    assert!(hits(&entry("Microsoft .NET Runtime 8.0.5 (x64)"), &exe, &with_system, root));
    assert!(hits(&entry("DirectX 运行库"), &exe, &with_system, root));
    assert!(hits(&entry("Microsoft Edge WebView2 Runtime"), &exe, &with_system, root));
    assert!(hits(&entry("某某常用组件"), &exe, &with_system, root));
    // 他自己装来用的东西不算环境：这些要是被挡了，就是词根切得太狠
    assert!(!hits(&entry("Python 3.11.7 (64-bit)"), &exe, &with_system, root));
    assert!(!hits(&entry("Node.js 20.11.0"), &exe, &with_system, root));
    assert!(!hits(&entry("Audacity 3.5.1"), &exe, &with_system, root));
    assert!(!hits(&entry("Android Studio"), &exe, &with_system, root));
    let mut under_windows = entry("资源管理器附带");
    under_windows.install_location = format!(r"{root}\System32");
    assert!(hits(&under_windows, &exe, &with_system, root));

    // 正经软件：图标在 C:\Windows\Installer 里，安装在别处
    let msi = Entry {
        display_icon: format!(r"{root}\Installer\{{GUID}}\ARPIcon.ico,0"),
        install_location: r"F:\Program\Real".to_string(),
        uninstall: format!(r#""{exe}" /uninstall"#),
        ..entry("Real Thing")
    };
    assert!(!hits(&msi, &exe, &with_system, root), "MSI 图标在 Windows 目录里不该被当成系统应用");
    // 关掉开关，系统组件也不挡了
    assert!(!hits(&component, &exe, &no_system, root));
}

/// 试算只看不动库：数字要和真同步一致，但一行都不写
#[test]
fn a_preview_counts_without_writing_anything() {
    let dir = sandbox("preview");
    let a = touch(&dir, "A\\a.exe");
    let b = touch(&dir, "B\\b.exe");
    let entries = vec![
        Entry { display_icon: a, ..entry("Aaaa") },
        Entry { display_icon: b, ..entry("Bbbb") },
    ];
    let icons = dir.join("icons");
    let c = conn();

    let preview = plan_with(&c, &entries, &[], &icons, true);
    assert_eq!(preview.inserted, 2, "{preview:?}");
    assert_eq!(preview.total, 2, "试算要把「同步之后会长成多少张」说出来");
    assert_eq!(rows(&c), Vec::<String>::new(), "试算不该写库");

    let real = plan_with(&c, &entries, &[], &icons, false);
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

/// 注册表里的路径写法五花八门：图标序号、引号包着带空格的目录、后面还跟着参数、
/// `%变量%` 没展开
#[test]
fn messy_registry_paths_come_out_as_one_usable_path() {
    std::env::set_var("WORKBENCH_TEST_DIR", r"F:\Program\Two Words");
    assert_eq!(
        clean_path(r#"%WORKBENCH_TEST_DIR%\app.exe, 0"#),
        r"F:\Program\Two Words\app.exe"
    );
    assert_eq!(
        clean_path(r#""F:\Program\Two Words\app.exe" --silent"#),
        r"F:\Program\Two Words\app.exe"
    );
    assert_eq!(clean_path(r"F:\a,b\c.exe"), r"F:\a,b\c.exe", "路径里真有逗号时别乱剥");
    assert_eq!(clean_path(r"%这个变量不存在%\x.exe"), r"%这个变量不存在%\x.exe", "展开不了的原样留着");
    assert_eq!(clean_path(""), "");
    assert_eq!(command_exe(r#""F:\P\app.exe" /uninstall"#).as_deref(), Some(r"F:\P\app.exe"));
    assert_eq!(command_exe("MsiExec.exe /X{GUID}").as_deref(), Some("MsiExec.exe"));
    assert_eq!(command_exe(""), None);
    assert!(is_uninstaller(r"F:\P\unins000.exe"));
    assert!(is_uninstaller(r"F:\P\某某卸载.exe"));
    assert!(!is_uninstaller(r"F:\P\app.exe"));
}

/// 名字比对：版本号和括号那些尾巴不该把同一个程序拆成两个
#[test]
fn names_match_across_version_tails_and_slashes() {
    assert_eq!(name_key(" 某某 助手 2.1 (x64) "), name_key("某某助手21x64"));
    assert_eq!(short_key("Python 3.11.7"), short_key("Python"));
    assert_eq!(path_key(r"F:\Program\App.exe"), path_key("F:/Program/App.exe"));

    let sc = [Shortcut {
        name: "Loomy".into(),
        key: name_key("Loomy"),
        lnk: "L:\\Loomy.lnk".into(),
        target: "L:\\Loomy\\loomy.exe".into(),
    }];
    // 注册表那条多了版本号：严格相等对不上，去版本号才对上
    assert!(find_shortcut("Loomy 1.2.3", &sc).is_some());
    assert!(find_shortcut("NVIDIA Loomy 1.2", &sc).is_some());
    assert!(find_shortcut("完全不同的东西", &sc).is_none());
}

/// 快捷方式索引只收真能启动的：卸载器、目录、解析不出目标的都不进
#[test]
fn the_shortcut_index_keeps_only_launchable_entries() {
    let dir = sandbox("index");
    let programs = dir.join("Programs");
    std::fs::create_dir_all(programs.join("厂商")).unwrap();
    touch(&programs, "Loomy.lnk");
    touch(&programs.join("厂商"), "Nested.lnk");
    touch(&programs, "uninstall.lnk");
    touch(&programs, "某某 卸载.lnk");
    touch(&programs, "readme.txt");
    std::fs::create_dir_all(programs.join("Gone.lnk")).unwrap();

    let index = shortcut_index(std::slice::from_ref(&programs));
    let mut keys: Vec<String> = index.iter().map(|s| s.key.clone()).collect();
    keys.sort();
    assert_eq!(keys, vec!["loomy".to_string(), "nested".to_string()], "{index:?}");
}

/// 真机核对一：拿这台机器的注册表跑一遍，看四个视图一共读出多少条、
/// 各级目标找到多少、图标抽到几张。库是内存库，不动他的数据。
/// 只有解析快捷方式那步要 COM，测试线程得自己初始化。
#[test]
#[ignore]
fn the_real_registry_lists_installed_programs() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let entries = read_entries();
    assert!(entries.len() > 20, "注册表只读出 {} 条，多半是视图没开对", entries.len());
    let shortcuts = shortcut_index(
        &crate::win::desktop::all_program_menus()
            .into_iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>(),
    );
    let mut tiers = [0usize; 4];
    for e in &entries {
        tiers[match resolve(e, &shortcuts).found {
            Found::Exe => 0,
            Found::Shortcut => 1,
            Found::Folder => 2,
            Found::Nothing => 3,
        }] += 1;
    }
    println!(
        "注册表读出 {} 条（快捷方式索引 {} 条）：exe {} / 快捷方式 {} / 只剩目录 {} / 什么都没找到 {}",
        entries.len(),
        shortcuts.len(),
        tiers[0],
        tiers[1],
        tiers[2],
        tiers[3]
    );
    // 「哪些条目连目录都没给出来」是最容易挨问的一类：名字和卸载键末段一起打出来，
    // 下次他说某个软件没进卡片，先在这里看它是不是压根没登记卸载项
    println!("  什么都没找到的那些：");
    for e in entries.iter().filter(|e| resolve(e, &shortcuts).found == Found::Nothing).take(30) {
        println!(
            "    - {} | {}",
            e.name,
            e.reg_key.rsplit('\\').next().unwrap_or("")
        );
    }
    for e in &entries[..entries.len().min(15)] {
        let r = resolve(e, &shortcuts);
        println!("  {} [{:?}] -> {}", e.name, r.found, r.target);
    }

    // 把「按系统/驱动/环境挡掉了谁」整片打出来：这条规则最怕的就是切太狠，
    // 光看个数看不出来，得逐个名字过一遍
    let root = system_root();
    let blocked: Vec<&str> = entries
        .iter()
        .filter(|e| {
            let target = resolve(e, &shortcuts).target;
            e.system_component || e.is_patch || is_system(e, &target, &root)
        })
        .map(|e| e.name.as_str())
        .collect();
    println!("被系统/驱动/环境这条挡掉的 {} 个：", blocked.len());
    for name in &blocked {
        println!("  - {name}");
    }

    let c = conn();
    let report = plan(&c, &entries, &shortcuts, &get_icon_dir().unwrap(), false).unwrap();
    let with_icon: i64 = c
        .query_row("SELECT COUNT(*) FROM apps WHERE icon_path IS NOT NULL", [], |r| r.get(0))
        .unwrap();
    let with_meta: i64 = c
        .query_row("SELECT COUNT(*) FROM apps WHERE description IS NOT NULL", [], |r| r.get(0))
        .unwrap();

    // 两个来源各收了多少：第二遍（快捷方式）补进来的商店应用和绿色版要看得到
    let by_source: Vec<(String, i64)> = c
        .prepare("SELECT source, COUNT(*) FROM apps GROUP BY source ORDER BY source")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    println!("  来源分布：{:?}", by_source);
    println!(
        "收进来 {} 个、库里原有 {} 个、被挡 {} 个、找不到目标的 {} 个；有图标 {with_icon} 张、有版本/发行商 {with_meta} 条",
        report.inserted, report.already, report.filtered, report.unresolved
    );
    assert!(report.inserted > 10, "只收进 {} 个", report.inserted);
    assert!(with_icon * 2 > report.inserted as i64, "只抽到 {with_icon} 张图标，抽图标这步多半整片失败了");
    assert!(
        report.unresolved * 2 <= report.scanned,
        "{} / {} 条连一条路径都认不出来，比例太高",
        report.unresolved,
        report.scanned
    );

    // 收进来的名字整片打出来：过滤这条最怕的是切错东西，光看个数看不出来
    let examples: Vec<(String, String, Option<String>)> = c
        .prepare("SELECT name, target, description FROM apps ORDER BY name LIMIT 130")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for (name, target, description) in examples {
        println!("  {name} | {description:?} | {target}");
    }
}

/// 真机核对二：过滤名单在这台机器上到底挡不挡东西。
/// 关开关 → 系统组件回来了；拿其中一个名字进名单、照拖动那样删掉卡片 → 再同步不该回来。
#[test]
#[ignore]
fn the_filter_list_actually_blocks_on_this_machine() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let entries = read_entries();
    let shortcuts = shortcut_index(
        &crate::win::desktop::all_program_menus()
            .into_iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>(),
    );
    let icons = get_icon_dir().unwrap();
    let c = conn();

    let on_first = plan(&c, &entries, &shortcuts, &icons, false).unwrap();
    assert!(on_first.filtered > 0, "默认开着系统应用过滤却一个都没挡：{on_first:?}");
    crate::services::settings::set_setting(&c, SYSTEM_SWITCH, "0").unwrap();
    let system_off = plan(&c, &entries, &shortcuts, &icons, false).unwrap();
    assert_eq!(system_off.scanned, on_first.scanned, "开关不该改变读到的总数");
    println!(
        "默认开：收 {} / 挡 {}；关掉开关后又收回 {} 个系统应用",
        on_first.inserted, on_first.filtered, system_off.inserted
    );
    assert!(system_off.inserted > 0, "关掉开关一个都没多出来，说明那条过滤没生效");

    let (vid, victim): (i64, String) = c
        .query_row("SELECT id, name FROM apps LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    add_app_filter(&c, &victim).unwrap();
    crate::services::desktop::delete_app(&c, vid).unwrap();
    let after = plan(&c, &entries, &shortcuts, &icons, false).unwrap();
    let still: i64 = c
        .query_row("SELECT COUNT(*) FROM apps WHERE name = ?1", params![victim], |r| r.get(0))
        .unwrap();
    println!("把「{victim}」加进名单再同步：库里现在 {} 个，本次收 {}、挡 {}", after.total, after.inserted, after.filtered);
    assert_eq!(still, 0, "名单里那个又被收回来了，过滤没生效");
    assert!(after.filtered >= 1, "{after:?}");
}
