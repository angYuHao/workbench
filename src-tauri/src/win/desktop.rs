use windows::core::GUID;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{FOLDERID_Desktop, FOLDERID_PublicDesktop, KF_FLAG_DEFAULT, SHGetKnownFolderPath};

pub fn user_desktop() -> Option<String> {
    known_folder(&FOLDERID_Desktop)
}

pub fn public_desktop() -> Option<String> {
    known_folder(&FOLDERID_PublicDesktop)
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