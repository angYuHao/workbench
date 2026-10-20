//! 测试靠临时目录跑：真桌面的东西一概不碰，整理与回退都在 sandbox 里走一遍。

use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

use crate::services::usage::{note_use, APP};

use super::*;
use super::organize::{free_dest, organize_into, prune_empty_dirs};
use super::rules::{category_ext, match_category};
use super::scan::{norm_path, scan_directory, OwnExe};

use crate::models::{AppEntry, DesktopRule, OrganizePlan};

/// 用真 schema.sql 建库：手抄一遍建表语句迟早和生产库走偏，那样的绿色没意义
fn conn() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch(include_str!("../../db/schema.sql")).unwrap();
    c
}

/// 每个用例一个临时目录，进去先清干净；真桌面只有 win::desktop 知道，测试碰不到
fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("workbench-desktop-test-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn touch(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, b"x").unwrap();
    p
}

fn s(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

fn rule(category: &str, keywords: Option<&str>, extensions: Option<&str>, enabled: bool) -> DesktopRule {
    DesktopRule {
        id: 0,
        category: category.to_string(),
        keywords: keywords.map(str::to_string),
        extensions: extensions.map(str::to_string),
        enabled,
        sort: 0,
    }
}

fn plan(path: &Path, category: &str) -> OrganizePlan {
    OrganizePlan {
        path: s(path),
        category: category.to_string(),
    }
}

fn entry(name: &str, target: &str) -> AppEntry {
    AppEntry {
        id: 0,
        name: name.to_string(),
        alias: None,
        description: None,
        target: target.to_string(),
        args: None,
        icon_path: None,
        category_id: None,
        source: "manual".to_string(),
        created_at: None,
        use_count: 0,
    }
}

/// 按插入顺序取 target，断言启动器指向哪儿
fn targets(c: &Connection) -> Vec<String> {
    let mut stmt = c.prepare("SELECT target FROM apps ORDER BY id").unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn keyword_rule_wins_over_extension_and_disabled_rules_are_skipped() {
    let rules = vec![
        rule("文档", None, Some("pdf, .docx"), true),
        rule("报销", Some(" 发票 , 报销单 ,"), None, true),
        rule("废弃", Some("pdf"), None, false),
    ];
    // 名字里带「发票」就先按关键词归报销，哪怕扩展名也命中文档
    assert_eq!(match_category("12月发票.pdf", "pdf", &rules).as_deref(), Some("报销"));
    assert_eq!(match_category("手册.pdf", "pdf", &rules).as_deref(), Some("文档"));
    // 规则里的扩展名带不带点、大小写都算同一个
    assert_eq!(match_category("手册.DOCX", "docx", &rules).as_deref(), Some("文档"));
    // 谁都没撞上就是没归属，不兜底造一个「其他」
    assert_eq!(match_category("看不懂的", "bin", &rules), None);
    // 关键词只有空格时不该把什么都吃掉
    let blank = vec![rule("空规则", Some(" , "), None, true)];
    assert_eq!(match_category("任意", "txt", &blank), None);
}

#[test]
fn a_shortcut_pointing_at_an_icon_is_not_a_picture() {
    let rules = vec![
        rule("图片", None, Some(".jpg,.png,.gif,.svg,.ico,.cur"), true),
        rule("文档", None, Some(".doc,.pdf"), true),
        rule("快捷方式", None, Some(".lnk"), true),
    ];
    // Cpolar Web UI.lnk 就是这样：Windows 自己报的 TargetPath 是安装器缓存里的 app.ico。
    // 按 .lnk 自己算，所以它归「快捷方式」而不是图片
    assert_eq!(category_ext(true, "lnk", "ico"), "lnk");
    assert_eq!(
        match_category("Cpolar Web UI", category_ext(true, "lnk", "ico"), &rules).as_deref(),
        Some("快捷方式")
    );
    // 指向文档的快捷方式照旧跟着目标走
    assert_eq!(category_ext(true, "lnk", "pdf"), "pdf");
    assert_eq!(
        match_category("报告", category_ext(true, "lnk", "pdf"), &rules).as_deref(),
        Some("文档")
    );
    // 解析不出扩展名、或者快捷方式套快捷方式，都按 .lnk 自己算
    assert_eq!(category_ext(true, "lnk", ""), "lnk");
    assert_eq!(category_ext(true, "lnk", "lnk"), "lnk");
    // 真正的图片文件不受这条影响
    assert_eq!(category_ext(false, "ico", ""), "ico");
    assert_eq!(
        match_category("logo", category_ext(false, "ico", ""), &rules).as_deref(),
        Some("图片")
    );
}

#[test]
fn the_scanner_skips_its_own_executable_and_any_shortcut_pointing_at_it() {
    let dir = sandbox("self-exe");
    let own_file = touch(&dir, "workbench.exe");
    touch(&dir, "别的.exe");
    let rules = vec![rule("程序", None, Some(".exe"), true)];
    let icon_dir = dir.join("icons");
    std::fs::create_dir_all(&icon_dir).unwrap();
    let own = OwnExe {
        file: "workbench.exe".to_string(),
        full: norm_path(&own_file),
    };

    let mut items = Vec::new();
    scan_directory(&s(&dir), 0, &rules, &icon_dir, Some(&own), &mut items);
    assert_eq!(
        items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        vec!["别的.exe"],
        "自己那份不该进扫描结果"
    );

    // 开发跑 target\\debug 那份、快捷方式指装机那份：路径不同但同名，只比路径会漏
    let installed = sandbox("self-installed");
    let same_name = touch(&installed, "WorkBench.EXE");
    assert!(own.is_this(&same_name, &s(&same_name)), "换个目录的同名 exe 也该认出来");
    let mut second = Vec::new();
    scan_directory(&s(&installed), 0, &rules, &icon_dir, Some(&own), &mut second);
    assert!(second.is_empty(), "{:?}", second);

    // 拿不到自身路径（current_exe 失败）时宁可多列，不能凭空跳过东西
    let mut blind = Vec::new();
    scan_directory(&s(&dir), 0, &rules, &icon_dir, None, &mut blind);
    assert_eq!(blind.len(), 2, "认不出自己时一项都不该被跳掉");
}

/// 没有规则命中就不成项：整理清单和页面上都不该冒出一张「没归属」的卡片，
/// 也不该为此凭空造一个「其他」分类
#[test]
fn the_scanner_drops_anything_no_rule_covers() {
    let dir = sandbox("no-rule");
    touch(&dir, "报告.pdf");
    touch(&dir, "看不懂的.bin");
    let rules = vec![rule("文档", None, Some(".pdf"), true)];
    let icon_dir = dir.join("icons");
    std::fs::create_dir_all(&icon_dir).unwrap();

    let mut items = Vec::new();
    scan_directory(&s(&dir), 0, &rules, &icon_dir, None, &mut items);

    assert_eq!(
        items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        vec!["报告.pdf"],
        "扩展名没撞上规则的那份整个不该出现"
    );
    assert_eq!(items[0].category, "文档");
    let _ = std::fs::remove_dir_all(&dir);
}

/// 造一套「已经整理过」的样子：`桌面整理\文档\报告.pdf` 摆在那儿，
/// 卡片、恢复日志、文件备注、规则四边都记着这个名字
fn organized_as_docs(desktop: &Path) -> (Connection, PathBuf) {
    let c = conn();
    let cat = desktop.join(ORGANIZE_DIR).join("文档");
    std::fs::create_dir_all(&cat).unwrap();
    let file = cat.join("报告.pdf");
    std::fs::write(&file, b"x").unwrap();
    let (to, root) = (s(&file), s(&cat));
    c.execute("INSERT INTO categories (id, name, type) VALUES (1, '文档', 'app')", [])
        .unwrap();
    c.execute(
        "INSERT INTO desktop_rules (id, category, extensions) VALUES (30, '文档', '.pdf')",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO apps (id, name, target, category_id) VALUES (10, '报告.pdf', ?1, 1)",
        params![to],
    )
    .unwrap();
    c.execute(
        "INSERT INTO organize_logs (id, file_name, from_path, to_path) VALUES (1, '报告.pdf', 'C:/Users/me/Desktop/报告.pdf', ?1)",
        params![to],
    )
    .unwrap();
    c.execute(
        "INSERT INTO files (id, path, name, managed_root) VALUES (20, ?1, '报告.pdf', ?2)",
        params![to, root],
    )
    .unwrap();
    (c, cat)
}

fn scalar(c: &Connection, sql: &str) -> String {
    c.query_row(sql, [], |r| r.get::<_, String>(0)).unwrap()
}

#[test]
fn renaming_a_rule_category_moves_the_folder_and_every_row_that_remembers_it() {
    let desktop = sandbox("rule-rename");
    let (c, cat) = organized_as_docs(&desktop);
    let mut renamed = rule("资料", None, Some(".pdf"), true);
    renamed.id = 30;

    save_rule_with_rename(&c, &renamed, &desktop).unwrap();

    let new_file = desktop.join(ORGANIZE_DIR).join("资料").join("报告.pdf");
    assert!(!cat.exists(), "旧目录该跟着改名，不该留个空壳");
    assert!(new_file.exists(), "目录里的文件没跟着走");
    let (to, root) = (s(&new_file), s(new_file.parent().unwrap()));
    assert_eq!(scalar(&c, "SELECT name FROM categories WHERE id = 1"), "资料");
    assert_eq!(scalar(&c, "SELECT category FROM desktop_rules WHERE id = 30"), "资料");
    // 卡片双击要靠 target、恢复整理要靠 to_path、文件备注要靠 path：三条都得换到新路径
    assert_eq!(scalar(&c, "SELECT target FROM apps WHERE id = 10"), to);
    assert_eq!(scalar(&c, "SELECT to_path FROM organize_logs WHERE id = 1"), to);
    assert_eq!(scalar(&c, "SELECT path FROM files WHERE id = 20"), to);
    assert_eq!(scalar(&c, "SELECT managed_root FROM files WHERE id = 20"), root);
}

#[test]
fn a_rule_name_still_used_by_another_rule_leaves_the_folder_alone() {
    let desktop = sandbox("rule-rename-shared");
    let (c, cat) = organized_as_docs(&desktop);
    c.execute(
        "INSERT INTO desktop_rules (id, category, keywords) VALUES (31, '文档', '合同')",
        [],
    )
    .unwrap();
    let mut renamed = rule("资料", None, Some(".pdf"), true);
    renamed.id = 30;

    save_rule_with_rename(&c, &renamed, &desktop).unwrap();

    // 31 号规则还叫「文档」，动分类名和目录等于替它改名
    assert!(cat.exists());
    assert_eq!(scalar(&c, "SELECT name FROM categories WHERE id = 1"), "文档");
    assert_eq!(scalar(&c, "SELECT category FROM desktop_rules WHERE id = 30"), "资料");
}

#[test]
fn a_rule_rename_onto_an_existing_category_is_refused_before_anything_moves() {
    let desktop = sandbox("rule-rename-clash");
    let (c, cat) = organized_as_docs(&desktop);
    c.execute("INSERT INTO categories (id, name, type) VALUES (40, '图片', 'app')", [])
        .unwrap();
    let mut renamed = rule("图片", None, Some(".pdf"), true);
    renamed.id = 30;

    let err = save_rule_with_rename(&c, &renamed, &desktop).unwrap_err().to_string();
    assert!(err.contains("图片"), "{err}");
    // 拒绝要拒在写之前：规则、分类、目录三者都还是原样
    assert!(cat.exists());
    assert_eq!(scalar(&c, "SELECT name FROM categories WHERE id = 1"), "文档");
    assert_eq!(scalar(&c, "SELECT category FROM desktop_rules WHERE id = 30"), "文档");
}

#[test]
fn a_rule_rename_onto_an_existing_folder_is_refused() {
    let desktop = sandbox("rule-rename-folder-taken");
    let (c, cat) = organized_as_docs(&desktop);
    std::fs::create_dir_all(desktop.join(ORGANIZE_DIR).join("资料")).unwrap();
    let mut renamed = rule("资料", None, Some(".pdf"), true);
    renamed.id = 30;

    let err = save_rule_with_rename(&c, &renamed, &desktop).unwrap_err().to_string();
    assert!(err.contains("已经存在"), "{err}");
    assert!(cat.exists(), "目录被占了就不该动旧目录");
    assert_eq!(scalar(&c, "SELECT category FROM desktop_rules WHERE id = 30"), "文档");
}

#[test]
fn extension_is_lowercased_and_only_the_last_dot_counts() {
    assert_eq!(extension_of(Path::new("a/b/PDF.PDF")), "pdf");
    assert_eq!(extension_of(Path::new("a/archive.tar.GZ")), "gz");
    assert_eq!(extension_of(Path::new("Makefile")), "");
    // 开头那个点算隐藏位，不是扩展名分隔符
    assert_eq!(extension_of(Path::new(".gitignore")), "");
}

#[test]
fn an_occupied_archive_name_gets_a_numbered_copy() {
    let dir = sandbox("free-dest");
    let cat = dir.join("文档");
    std::fs::create_dir_all(&cat).unwrap();
    let src = Path::new("/desktop/报告.pdf");
    assert_eq!(free_dest(&cat, src, "报告.pdf"), cat.join("报告.pdf"));
    touch(&cat, "报告.pdf");
    assert_eq!(free_dest(&cat, src, "报告.pdf"), cat.join("报告 (1).pdf"));
    touch(&cat, "报告 (1).pdf");
    assert_eq!(free_dest(&cat, src, "报告.pdf"), cat.join("报告 (2).pdf"));
    // 没扩展名也一样递增，别把后缀丢了
    let bare = Path::new("/desktop/Makefile");
    touch(&cat, "Makefile");
    assert_eq!(free_dest(&cat, bare, "Makefile"), cat.join("Makefile (1)"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// 整理只动文件和自己的流水：`apps` 是「应用」页的表，桌面那种大杂烩不该进去
#[test]
fn organize_moves_files_into_category_dirs_and_leaves_the_launcher_alone() {
    let desktop = sandbox("organize");
    let pdf = touch(&desktop, "报告.pdf");
    let txt = touch(&desktop, "笔记.txt");
    let c = conn();
    c.execute("INSERT INTO categories (name, type) VALUES ('文档','app')", []).unwrap();

    let logs = organize_into(&c, &[plan(&pdf, "文档"), plan(&txt, "其他")], &desktop).unwrap();

    assert_eq!(logs.len(), 2);
    let moved_pdf = desktop.join("桌面整理").join("文档").join("报告.pdf");
    assert!(moved_pdf.exists(), "文件要落到 桌面整理/<分类>/ 下");
    assert!(!pdf.exists(), "原位置该空出来");
    assert!(desktop.join("桌面整理").join("其他").join("笔记.txt").exists());
    assert_eq!(targets(&c), Vec::<String>::new(), "整理不该往「应用」里登记任何东西");
    let logged: i64 = c
        .query_row("SELECT COUNT(*) FROM organize_logs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(logged, 2, "搬了的都得留下可撤销的流水");
    assert!(!logs[0].reverted);
    let _ = std::fs::remove_dir_all(&desktop);
}

#[test]
fn organize_never_overwrites_a_file_already_sitting_in_the_category_dir() {
    let desktop = sandbox("organize-clash");
    let cat = desktop.join("桌面整理").join("文档");
    std::fs::create_dir_all(&cat).unwrap();
    let keeper = touch(&cat, "报告.pdf");
    let pdf = touch(&desktop, "报告.pdf");
    let c = conn();

    let logs = organize_into(&c, &[plan(&pdf, "文档")], &desktop).unwrap();

    assert_eq!(logs[0].to_path.as_deref(), Some(s(&cat.join("报告 (1).pdf")).as_str()));
    assert!(keeper.exists(), "归档里原来那份不能被动掉");
    assert!(cat.join("报告 (1).pdf").exists());
    let _ = std::fs::remove_dir_all(&desktop);
}

#[test]
fn revert_puts_everything_back_and_prunes_the_empty_dirs() {
    let desktop = sandbox("revert");
    let pdf = touch(&desktop, "报告.pdf");
    let c = conn();
    organize_into(&c, &[plan(&pdf, "文档")], &desktop).unwrap();
    let moved = desktop.join("桌面整理").join("文档").join("报告.pdf");
    assert!(moved.exists());

    assert_eq!(revert_organize(&c, None).unwrap(), 1);
    assert!(pdf.exists(), "撤销要搬回原位置");
    assert!(!moved.exists());
    assert!(
        !desktop.join("桌面整理").exists(),
        "被搬空的分类目录和只剩空目录的「桌面整理」都该收掉"
    );
    assert_eq!(revert_organize(&c, None).unwrap(), 0, "撤销过的流水不该再算一遍");
    let _ = std::fs::remove_dir_all(&desktop);
}

#[test]
fn a_copy_that_already_vanished_is_marked_but_not_counted() {
    let desktop = sandbox("revert-gone");
    let from = desktop.join("报告.pdf");
    // 归档那份压根不存在：用户自己删了或移走了
    let to = desktop.join("桌面整理").join("文档").join("报告.pdf");
    let c = conn();
    c.execute(
        "INSERT INTO organize_logs (file_name, from_path, to_path) VALUES ('报告.pdf', ?1, ?2)",
        params![s(&from), s(&to)],
    )
    .unwrap();
    let id = c.last_insert_rowid();

    assert_eq!(revert_organize(&c, Some(id)).unwrap(), 0, "副本都没了，不该谎报搬回一个");
    let reverted: i64 = c
        .query_row("SELECT reverted FROM organize_logs WHERE id = ?1", params![id], |r| r.get(0))
        .unwrap();
    assert_eq!(reverted, 1, "状态还是得补记，否则每次恢复都白跑一遍");
    let _ = std::fs::remove_dir_all(&desktop);
}

#[test]
fn prune_only_removes_dirs_that_emptied_out() {
    let root = sandbox("prune");
    let base = root.join("桌面整理");
    let empty_cat = base.join("文档");
    let full_cat = base.join("其他");
    std::fs::create_dir_all(&empty_cat).unwrap();
    std::fs::create_dir_all(&full_cat).unwrap();
    touch(&full_cat, "a.txt");

    prune_empty_dirs(&[empty_cat.clone(), full_cat.clone()]);

    assert!(!empty_cat.exists(), "搬空的分类目录该删掉");
    assert!(full_cat.exists(), "非空目录一律留着");
    assert!(base.exists(), "归档根目录里还有东西就不该动");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn save_app_inserts_on_zero_id_and_updates_otherwise() {
    let c = conn();
    let saved = save_app(&c, &entry("新程序", "C:/a.exe")).unwrap();
    assert_eq!(saved.id, 1, "新增要带回真的 id，前端下一轮要靠它计数");
    let edited = AppEntry {
        id: saved.id,
        name: "改了名".to_string(),
        target: "C:/b.exe".to_string(),
        ..saved.clone()
    };
    save_app(&c, &edited).unwrap();
    let rows = get_apps(&c).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "改了名");
    assert_eq!(rows[0].target, "C:/b.exe");
}

#[test]
fn clicked_apps_lead_the_list_and_deleting_forgets_the_count() {
    let c = conn();
    let a = save_app(&c, &entry("WPS", "C:/wps.exe")).unwrap();
    let b = save_app(&c, &entry("Terminal", "C:/wt.exe")).unwrap();
    note_use(&c, APP, b.id).unwrap();
    note_use(&c, APP, b.id).unwrap();
    note_use(&c, APP, a.id).unwrap();

    let rows = get_apps(&c).unwrap();
    assert_eq!(
        rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        vec!["Terminal", "WPS"],
        "点得多的排前面"
    );
    assert_eq!(rows[0].use_count, 2);

    delete_app(&c, b.id).unwrap();
    let left: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM usage_stats WHERE kind = 'app' AND id = ?1",
            params![b.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(left, 0, "删了程序要把计数一起带走，不然 id 复用会继承旧账");
}

#[test]
fn launch_by_id_reports_the_missing_row_with_its_id() {
    let c = conn();
    let err = launch_by_id(&c, 4242).unwrap_err().to_string();
    assert!(err.contains("4242"), "报错要带上那个 id，got {err}");
}
