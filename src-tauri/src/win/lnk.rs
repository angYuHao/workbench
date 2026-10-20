//! 解析 .lnk：取它真正指向的目标。走 IShellLinkW，所以要调用方先把 COM 初始化好，
//! 没初始化时整片失败，而失败看着像「快捷方式指向自己」——不会报错，只会给出错的答案。
use windows::core::{Interface, PCWSTR};
use windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ};
use windows::Win32::UI::Shell::{IShellLinkW, SLGP_UNCPRIORITY, ShellLink};

/// 解析 .lnk 快捷方式，返回真实目标绝对路径。
pub fn resolve_lnk(path: &str) -> Option<String> {
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let persist: IPersistFile = link.cast().ok()?;
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        persist.Load(PCWSTR(wide.as_ptr()), STGM_READ).ok()?;

        let mut buf = [0u16; 2048];
        let mut wfd = WIN32_FIND_DATAW::default();
        link.GetPath(&mut buf, &mut wfd, SLGP_UNCPRIORITY.0 as u32)
            .ok()?;

        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let resolved = String::from_utf16_lossy(&buf[..end]);
        if resolved.is_empty() {
            None
        } else {
            Some(resolved)
        }
    }
}