//! 资源管理器「新建」那张单子的数据源：注册表里的 `ShellNew`。
//!
//! 这张表不写死在代码里 —— 系统上装了什么就能新建什么。抄一份固定名单的话，
//! 用户机器上装了 WPS 却只给他五个选项，那就不叫「和原生一样」。本机实测就是这样：
//! DOC/DOCX/PPT/PPTX/RTF/XLS/XLSX 全来自 WPS 登记的那几个键，换台机器就不一样。
//!
//! 名字也不自己编：取 `SHGetFileInfo` 的类型名，和资源管理器「类型」列、右键「新建」
//! 菜单同一个来源（"DOCX 文档"、"文本文档"），两边永远说得上话。
//!
//! 只收三种写法，因为只建得出这三种：
//! - `NullFile`（空文件，.txt/.md/.json 这类）
//! - `Data`（模板字节，Office 那几类靠它才有合法表头；REG_BINARY 或一串 hex）
//! - `FileName`（现成模板文件，`%V%` 指本类的 InstallLocation）
//!
//! 只有 `Command` 的那种（走 DDE）不收 —— 建不出来就别摆进菜单。
//! 「快捷方式」也不在表里：它建完会立刻弹「选取该快捷方式指向何处」向导，我们不做那个
//! 向导，留下一个谁也不指向的 .lnk 只是垃圾。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::win::registry::{parse_hex, Arch, Hive, Key};

/// 菜单上的一项：`ext` 带着点（".docx"），`label` 是那一行显示的类型名
#[derive(Debug, Clone, Serialize)]
pub struct NewType {
    pub ext: String,
    pub label: String,
}

/// 按 ShellNew 的记载，这个文件该怎么造出来
#[derive(Debug, Clone, PartialEq, Eq)]
enum Recipe {
    Empty,
    Bytes(Vec<u8>),
    Template(PathBuf),
}

/// 扩展名下面这些子键都不是「类名」，不用去它们底下找 ShellNew
const NOT_A_CLASS: &[&str] = &[
    "CLSID",
    "CurVer",
    "DefaultIcon",
    "DocObject",
    "DropTarget",
    "Explorer",
    "Insertable",
    "NoRemove",
    "OpenWithList",
    "OpenWithProgids",
    "PersistentHandler",
    "Protocol",
    "Shell",
    "ShellEx",
    "ShellNew",
    "TypeLib",
];

/// 一个扩展名才配拿去拼注册表路径。带点、不止一个字符、不含分隔符和 `%`
/// （HKCR 里确实有 `.\SystemRoot\...` 这种老式键名，拼进去就是在开别的键）
fn sane_ext(ext: &str) -> bool {
    ext.len() > 1
        && ext.starts_with('.')
        && !ext.contains('\\')
        && !ext.contains('/')
        && !ext.contains('%')
}

/// 这几种类型系统确实登记了 ShellNew，但资源管理器自己的新建菜单也不摆它们：
/// 库、联系人不是用户会去新建的东西，.lnk 建完指向空处（原生靠那个向导补上目标）
fn skip(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        ".lnk" | ".library-ms" | ".contact"
    )
}

/// 这个扩展名的 `ShellNew` 挂在哪个键上。同一个东西有三种放法，少认一种就少一整列菜单：
/// 1. 挂在扩展名自己身上（老登记法）
/// 2. 挂在它默认值指的那个类上（经典写法：`.txt` → `txtfile\ShellNew`）
/// 3. 挂在扩展名自己的一个子键上（本机 WPS 就这么干：`.docx\WPS.Docx.6\ShellNew`）
fn new_class(ext: &str) -> Option<Key> {
    let root = Key::open(Hive::Classes, ext, Arch::Native)?;
    if root.child("ShellNew").is_some() {
        return Some(root);
    }
    if let Some(progid) = root
        .string("")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != ext)
    {
        if let Some(top) = Key::open(Hive::Classes, &progid, Arch::Native) {
            if top.child("ShellNew").is_some() {
                return Some(top);
            }
        }
        if let Some(nested) = root.child(&progid) {
            if nested.child("ShellNew").is_some() {
                return Some(nested);
            }
        }
    }
    for name in root.child_names() {
        if NOT_A_CLASS.contains(&name.as_str()) {
            continue;
        }
        let Some(child) = root.child(&name) else { continue };
        if child.child("ShellNew").is_some() {
            return Some(child);
        }
    }
    None
}

/// `%V%` 展开成本类的 InstallLocation，其余按字面路径处理。展不出来就回 None
fn resolve_template(raw: &str, install_location: Option<&str>) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.strip_prefix("%V%") {
        Some(rest) => {
            let root = install_location?;
            Some(Path::new(root).join(rest.trim_start_matches(['\\', '/'])))
        }
        None => Some(PathBuf::from(trimmed)),
    }
}

/// 三种写法的优先顺序照 shell 自己的来法：有 NullFile 就是空文件，其次模板字节，最后模板文件
fn recipe_of(
    has_null_file: bool,
    data: Option<Vec<u8>>,
    template: Option<PathBuf>,
) -> Option<Recipe> {
    if has_null_file {
        return Some(Recipe::Empty);
    }
    if let Some(bytes) = data {
        return Some(Recipe::Bytes(bytes));
    }
    template.map(Recipe::Template)
}

/// 从 `ShellNew` 键里读出配方。读不出来回 None（只有 `Command` 的那种落这儿）
fn read_recipe(shell_new: &Key, class: &Key) -> Option<Recipe> {
    let data = shell_new
        .binary("Data")
        .or_else(|| shell_new.string("Data").and_then(|s| parse_hex(&s)));
    let install = class.child("InstallLocation").and_then(|k| k.string(""));
    let template = shell_new
        .string("FileName")
        .and_then(|raw| resolve_template(&raw, install.as_deref()));
    recipe_of(shell_new.has_value("NullFile"), data, template)
}

/// 菜单项显示的那个类型名。探一个不存在的文件名即可（见 win::shell::type_name）
fn label_of(ext: &str) -> Option<String> {
    crate::win::shell::type_name(&format!("1{ext}"))
}

/// 这台机器现在能新建哪些文件类型，按类型名排（资源管理器也是按名字排）
pub fn list() -> Vec<NewType> {
    let Some(root) = Key::open(Hive::Classes, "", Arch::Native) else {
        return Vec::new();
    };
    let mut out: Vec<NewType> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for ext in root.child_names() {
        if !sane_ext(&ext) || skip(&ext) {
            continue;
        }
        let Some(class) = new_class(&ext) else { continue };
        let Some(shell_new) = class.child("ShellNew") else { continue };
        if read_recipe(&shell_new, &class).is_none() {
            continue;
        }
        if !seen.insert(ext.to_ascii_lowercase()) {
            continue;
        }
        let Some(label) = label_of(&ext) else { continue };
        out.push(NewType { ext, label });
    }
    out.sort_by(|a, b| a.label.cmp(&b.label).then_with(|| a.ext.cmp(&b.ext)));
    out
}

/// 在当前目录建一个该类型的文件。返回真正建出来的路径（同名会让位，名字里可能多了「 (2)」）
pub fn create_file(parent: &str, ext: &str) -> AppResult<String> {
    if !sane_ext(ext) || skip(ext) {
        return Err(AppError::msg("这个类型不能用来新建文件"));
    }
    let class =
        new_class(ext).ok_or_else(|| AppError::msg(format!("系统没登记「{ext}」的新建模板")))?;
    let shell_new = class
        .child("ShellNew")
        .ok_or_else(|| AppError::msg(format!("系统没登记「{ext}」的新建模板")))?;
    let recipe = read_recipe(&shell_new, &class)
        .ok_or_else(|| AppError::msg(format!("「{ext}」的新建方式系统没给出可用的模板")))?;
    let label = label_of(ext).unwrap_or_else(|| ext.trim_start_matches('.').to_string());
    let name = crate::services::files::free_name(Path::new(parent), &format!("新建 {label}{ext}"));
    let dest = Path::new(parent).join(&name);
    match recipe {
        Recipe::Empty => {
            std::fs::File::create(&dest)?;
        }
        Recipe::Bytes(bytes) => {
            std::fs::write(&dest, bytes)?;
        }
        Recipe::Template(template) => {
            std::fs::copy(&template, &dest).map_err(|_| {
                AppError::msg(format!("模板文件找不到：{}（{ext}）", template.display()))
            })?;
        }
    }
    Ok(dest.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 只有扩展名这一种形状能拿去拼键路径：带分隔符或 `%` 的都是在指别的键
    #[test]
    fn only_a_plain_extension_is_a_candidate() {
        assert!(sane_ext(".txt"));
        assert!(sane_ext(".docx"));
        assert!(!sane_ext(""));
        assert!(!sane_ext("."));
        assert!(!sane_ext("txt"));
        assert!(!sane_ext(r".\SystemRoot\x"));
        assert!(!sane_ext(".txt/../../evil"));
        assert!(!sane_ext(".%V%"));
    }

    /// 那三种系统登记了、资源管理器却不摆的类型，这里也不摆
    #[test]
    fn the_types_explorer_hides_stay_hidden() {
        assert!(skip(".lnk"));
        assert!(skip(".LNK"));
        assert!(skip(".library-ms"));
        assert!(skip(".contact"));
        assert!(!skip(".docx"));
    }

    /// 三种写法的取舍：谁在前面、谁能用
    #[test]
    fn the_recipe_prefers_whichever_the_shell_would_do() {
        assert_eq!(recipe_of(true, None, None), Some(Recipe::Empty));
        // NullFile 在就不看 Data —— 两个都写时 shell 也按空文件办
        assert_eq!(recipe_of(true, Some(vec![1, 2]), None), Some(Recipe::Empty));
        assert_eq!(
            recipe_of(false, Some(vec![0, 1]), None),
            Some(Recipe::Bytes(vec![0, 1]))
        );
        assert_eq!(
            recipe_of(false, None, Some("a".into())),
            Some(Recipe::Template("a".into()))
        );
        // 只有 Command：建不出来，别摆进菜单
        assert_eq!(recipe_of(false, None, None), None);
    }

    /// `%V%` 要跟着 InstallLocation 走；没有落点就是展不出来，不能拿字面路径去拷
    #[test]
    fn the_template_placeholder_follows_the_install_location() {
        assert_eq!(
            resolve_template(r"%V%\New\doc.docx", Some(r"C:\Office")).unwrap(),
            PathBuf::from(r"C:\Office\New\doc.docx")
        );
        assert_eq!(
            resolve_template(r"D:\tpl.dotx", None).unwrap(),
            PathBuf::from(r"D:\tpl.dotx")
        );
        assert!(resolve_template(r"%V%\New\doc.docx", None).is_none());
        assert!(resolve_template("   ", Some(r"C:\Office")).is_none());
    }

    /// 真机冒烟：这张表是读出来的。本机装了 WPS，就该有 DOCX 那一类 ——
    /// 一条都没有说明那三种放法有一种没认出来（本机是 `.docx\WPS.Docx.6\ShellNew` 那种）
    #[test]
    fn the_machine_owns_a_non_empty_new_menu() {
        let items = list();
        assert!(
            !items.is_empty(),
            "一个能新建的类型都没读到，枚举那条路就是错的"
        );
        assert!(
            items.windows(2).all(|w| w[0].label <= w[1].label),
            "没按类型名排，菜单读起来就是乱的"
        );
        for item in &items {
            assert!(!skip(&item.ext), "{} 不该出现在表里", item.ext);
            assert!(!item.label.trim().is_empty(), "{} 没有类型名", item.ext);
        }
        let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        println!("这台机器能新建：{names:?}");
    }

    /// 表里排第一的那条要真建得出来 —— 光列出来建不了比不列更气人
    #[test]
    fn the_first_listed_type_actually_creates_a_file() {
        let Some(first) = list().into_iter().next() else {
            eprintln!("这台机器一个都没有，跳过");
            return;
        };
        let dir = std::env::temp_dir().join(format!("wb-shellnew-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let made = create_file(&dir.to_string_lossy(), &first.ext).unwrap();
        let path = Path::new(&made);
        assert!(path.exists());
        assert!(path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("新建 "));
        // 再建一次要自动让位，别覆盖前一个
        let again = create_file(&dir.to_string_lossy(), &first.ext).unwrap();
        assert_ne!(again, made);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
