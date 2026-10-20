//! 交给 shell 的两个动作：用默认方式打开一个路径、在资源管理器里选中它。
//! 都是 `ShellExecute` 一层薄薄的包装，失败要说得出口（打开失败得让用户看见为什么）。
use windows::core::PCWSTR;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::error::{AppError, AppResult};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 用系统默认程序打开文件或 URL。
/// `ShellExecuteW` 的返回值按约定大于 32 才算成功，≤32 是错误码（文件已删除、没有关联程序都落在这儿）。
/// 以前这个返回值直接丢掉，打不开也报「成功」，启动器还会照样给它加一次使用计数——看着就像莫名其妙排序变了。
pub fn open(path: &str) -> AppResult<()> {
    let code = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(wide("open").as_ptr()),
            PCWSTR(wide(path).as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    let code = code.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(AppError::msg(format!("打不开「{path}」（系统返回码 {code}）")))
    }
}

/// 在资源管理器中定位文件。
/// 返回值同样按 `ShellExecute` 的约定判（>32 才算成功）：以前这里直接丢掉，
/// 弹不出窗口也报「成功」，和 `open()` 当初踩过的是同一个坑。
pub fn reveal(path: &str) -> AppResult<()> {
    let selector = format!("/select,{}", path);
    let code = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(wide("open").as_ptr()),
            PCWSTR(wide("explorer.exe").as_ptr()),
            PCWSTR(wide(&selector).as_ptr()),
            None,
            SW_SHOWNORMAL,
        )
    };
    let code = code.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(AppError::msg(format!("定位不到「{path}」（系统返回码 {code}）")))
    }
}