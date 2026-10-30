//! 系统剪贴板上的「文件」：`CF_HDROP`（一串路径）加 `Preferred DropEffect`（复制还是剪切）。
//! 只做这一种格式 —— 资源管理器、Outlook、浏览器互相认的就是它。
//! 写进去的是真内存块而不是延迟渲染，所以本程序退出后剪贴板照样能粘。
use std::thread::sleep;
use std::time::Duration;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{GlobalFree, BOOL, HGLOBAL, HANDLE, POINT};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, RegisterClipboardFormatW,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Shell::{DragQueryFileW, DROPFILES, HDROP};

use crate::error::{AppError, AppResult};
use crate::models::ClipboardFiles;

/// WinUser.h 里的 CF_HDROP。为一个常量去开 Win32_System_Ole 整块模块不划算
const CF_HDROP: u32 = 15;
const DROPEFFECT_COPY: u32 = 1;
const DROPEFFECT_MOVE: u32 = 2;
/// 剪贴板里那串文件的意图。`None` 是资源管理器搬完之后的做法：路径还留着，但标成「不要了」
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Copy,
    Move,
    None,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 剪贴板同一时刻只能有一个人开着。别的程序正用着（浏览器复制大图时会占一会儿）不是错误，
/// 按系统文档的意思让一下再来，最多等一百来毫秒
fn open() -> AppResult<()> {
    for _ in 0..8 {
        if unsafe { OpenClipboard(None) }.is_ok() {
            return Ok(());
        }
        sleep(Duration::from_millis(15));
    }
    Err(AppError::msg("系统剪贴板正被别的程序占着，稍后再试"))
}

unsafe fn free_block(block: &mut Option<HGLOBAL>) {
    if let Some(h) = block.take() {
        let _ = GlobalFree(h);
    }
}

/// 分配一块可移动的全局内存并锁住往里写。写完立刻解锁：
/// 留着锁的全局内存在 `SetClipboardData` 之后别的程序照样读得到，但没有必要一直攥着
unsafe fn global_with(len: usize) -> AppResult<(*mut u8, HGLOBAL)> {
    let h = GlobalAlloc(GMEM_MOVEABLE, len)
        .map_err(|e| AppError::msg(format!("分配剪贴板内存失败：{e}")))?;
    let ptr = GlobalLock(h) as *mut u8;
    if ptr.is_null() {
        let _ = GlobalFree(h);
        return Err(AppError::msg("锁不住刚分配的剪贴板内存"));
    }
    Ok((ptr, h))
}

/// `DROPFILES` 头 + 每个路径各以 NUL 结尾、整串再补一个 NUL 的 UTF-16 路径表。
/// 单独拎出来是为了不碰系统剪贴板也能把字节结构钉进测试
fn hdrop_bytes(paths: &[String]) -> Vec<u8> {
    let head = std::mem::size_of::<DROPFILES>();
    let mut bytes = vec![0u8; head];
    let header = DROPFILES {
        pFiles: head as u32,
        pt: POINT { x: 0, y: 0 },
        fNC: BOOL(0),
        fWide: BOOL(1),
    };
    unsafe {
        // DROPFILES 是 packed(1) 的，按字节整块拷过去最稳
        std::ptr::copy_nonoverlapping(&header as *const DROPFILES as *const u8, bytes.as_mut_ptr(), head);
    }
    for p in paths {
        bytes.extend(p.encode_utf16().flat_map(u16::to_le_bytes));
        bytes.extend([0, 0]);
    }
    bytes.extend([0, 0]);
    bytes
}

unsafe fn make_hdrop(bytes: &[u8]) -> AppResult<HGLOBAL> {
    let (ptr, h) = global_with(bytes.len())?;
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    // 锁计数归零时 GlobalUnlock 按约定返回 FALSE 且 GetLastError 是 NO_ERROR，不能拿返回值判失败
    let _ = GlobalUnlock(h);
    Ok(h)
}

unsafe fn make_dword(value: u32) -> AppResult<HGLOBAL> {
    let (ptr, h) = global_with(4)?;
    std::ptr::write_unaligned(ptr as *mut u32, value);
    let _ = GlobalUnlock(h);
    Ok(h)
}

/// 把这几项放进剪贴板。`mode` 是 `None` 时路径照样留着，只是标成「已经搬过、别再粘」
pub fn write(paths: &[String], mode: Mode) -> AppResult<()> {
    // 两块内存都先备好再动系统剪贴板：分配失败时别把用户原来那份东西清了
    let mut hdrop = Some(unsafe { make_hdrop(&hdrop_bytes(paths)) }?);
    let mut effect = Some(unsafe { make_dword(mode.effect()) }?);

    unsafe {
        if let Err(e) = open() {
            free_block(&mut hdrop);
            free_block(&mut effect);
            return Err(e);
        }
        let result = (|| -> AppResult<()> {
            EmptyClipboard().map_err(|e| AppError::msg(format!("清空剪贴板失败：{e}")))?;
            let fmt = RegisterClipboardFormatW(PCWSTR(wide("Preferred DropEffect").as_ptr()));
            if fmt == 0 {
                return Err(AppError::msg("系统不认 Preferred DropEffect 格式"));
            }
            let block = effect.take().expect("刚分配过");
            if let Err(e) = SetClipboardData(fmt, HANDLE(block.0)) {
                effect = Some(block);
                return Err(AppError::msg(format!("写入剪贴板失败：{e}")));
            }
            let block = hdrop.take().expect("刚分配过");
            if let Err(e) = SetClipboardData(CF_HDROP, HANDLE(block.0)) {
                hdrop = Some(block);
                return Err(AppError::msg(format!("写入剪贴板失败：{e}")));
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        // 交出去的那块归系统管，没交出去的自己收
        free_block(&mut hdrop);
        free_block(&mut effect);
        result
    }
}

/// 剪贴板里有文件才回清单；拷的是文字、图片一律当没有 —— 这一页只认路径
pub fn read() -> AppResult<Option<ClipboardFiles>> {
    unsafe {
        open()?;
        let result = read_locked();
        let _ = CloseClipboard();
        result
    }
}

unsafe fn read_locked() -> AppResult<Option<ClipboardFiles>> {
    let Ok(handle) = GetClipboardData(CF_HDROP) else {
        return Ok(None);
    };
    if handle.is_invalid() {
        return Ok(None);
    }
    let hdrop = HDROP(handle.0);
    let count = DragQueryFileW(hdrop, u32::MAX, None);
    let mut paths = Vec::with_capacity(count as usize);
    for i in 0..count {
        let len = DragQueryFileW(hdrop, i, None) as usize;
        if len == 0 {
            continue;
        }
        let mut buf = vec![0u16; len + 1];
        let got = DragQueryFileW(hdrop, i, Some(&mut buf));
        if got == 0 {
            continue;
        }
        buf.truncate(got as usize);
        paths.push(String::from_utf16_lossy(&buf));
    }
    // 剪贴板给的 HDROP 是它自己的内存，不能 DragFinish —— 那个是拖放会话结束时才用的
    if paths.is_empty() {
        return Ok(None);
    }
    let effect = preferred_effect();
    // 效果被标成 0 就是「这串已经作废」（资源管理器搬完文件之后这么干），别拿它去粘
    if effect == Some(0) {
        return Ok(None);
    }
    let cut = effect.is_some_and(|e| e & DROPEFFECT_MOVE != 0);
    Ok(Some(ClipboardFiles { paths, cut }))
}

unsafe fn preferred_effect() -> Option<u32> {
    let fmt = RegisterClipboardFormatW(PCWSTR(wide("Preferred DropEffect").as_ptr()));
    if fmt == 0 {
        return None;
    }
    let handle = GetClipboardData(fmt).ok()?;
    if handle.is_invalid() {
        return None;
    }
    let h = HGLOBAL(handle.0);
    let ptr = GlobalLock(h) as *const u32;
    if ptr.is_null() {
        return None;
    }
    let value = std::ptr::read_unaligned(ptr);
    let _ = GlobalUnlock(h);
    Some(value)
}

impl Mode {
    fn effect(self) -> u32 {
        match self {
            Mode::Copy => DROPEFFECT_COPY,
            Mode::Move => DROPEFFECT_MOVE,
            Mode::None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_file_list_is_a_header_then_double_nul_utf16_paths() {
        let bytes = hdrop_bytes(&["C:\\a.txt".to_string(), "D:\\目录".to_string()]);
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 20, "pFiles 是头的大小");
        // 跳过 pt 和 fNC，第 17 到 20 字节是 fWide：写成 0 系统会按 ANSI 去读这串路径
        assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 1);
        let text: Vec<u16> = bytes[20..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        assert_eq!(text, "C:\\a.txt\0D:\\目录\0\0".encode_utf16().collect::<Vec<_>>());
    }

    #[test]
    fn the_three_modes_write_the_effects_the_shell_expects() {
        assert_eq!(Mode::Copy.effect(), 1);
        assert_eq!(Mode::Move.effect(), 2);
        assert_eq!(Mode::None.effect(), 0);
    }
}
