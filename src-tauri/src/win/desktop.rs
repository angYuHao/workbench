//! 桌面根目录：一台机器上可能同时有本机桌面和 OneDrive 托管的那个，
//! 扫描和整理都要一起看，不然「桌面上一堆东西、页面里空的」就出现在用了云桌面的人身上。
use windows::core::GUID;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_CommonPrograms, FOLDERID_Desktop, FOLDERID_Programs, FOLDERID_PublicDesktop,
    KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};

pub fn user_desktop() -> Option<String> {
    known_folder(&FOLDERID_Desktop)
}

pub fn public_desktop() -> Option<String> {
    known_folder(&FOLDERID_PublicDesktop)
}

/// 开始菜单的「程序」目录：本机已安装应用的快捷方式都在这儿，
/// 名字是给人看的、目标能解析出真 exe，所以拿它当「本机装了哪些应用」的清单最实在
pub fn user_programs() -> Option<String> {
    known_folder(&FOLDERID_Programs)
}

pub fn public_programs() -> Option<String> {
    known_folder(&FOLDERID_CommonPrograms)
}

/// 两个开始菜单目录，去重后按「用户优先」排
pub fn all_program_menus() -> Vec<String> {
    let mut v = Vec::new();
    for dir in [user_programs(), public_programs()].into_iter().flatten() {
        if !v.iter().any(|p: &String| p.eq_ignore_ascii_case(&dir)) {
            v.push(dir);
        }
    }
    v
}

pub fn all_desktops() -> Vec<String> {
    let mut v = Vec::new();
    if let Some(p) = user_desktop() {
        v.push(p);
    }
    if let Some(p) = public_desktop() {
        if !v.contains(&p) {
            v.push(p);
        }
    }
    v
}

fn known_folder(folder_id: *const GUID) -> Option<String> {
    unsafe {
        let pw = SHGetKnownFolderPath(folder_id, KF_FLAG_DEFAULT, None).ok()?;
        let s = pw.to_string().ok();
        CoTaskMemFree(Some(pw.0 as *const _));
        s
    }
}