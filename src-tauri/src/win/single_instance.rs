//! 单实例：同时只允许一个工作台在跑。
//!
//! 不用 `tauri-plugin-single-instance` 是因为本机 crates.io 拿不到（`static.crates.io`
//! 超时、本地也没缓存），而这件事一个命名互斥体就够。第二个进程刚起来就退，
//! 顺手把在跑的那个窗口叫到前面——不然双击图标看着像「什么都没发生」。

use std::sync::atomic::{AtomicIsize, Ordering};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

/// 名字跟着应用标识走，别用 exe 路径：开发版和装机版因此共用一把锁，
/// 同时只会有一个窗口——两个进程一起写同一个 SQLite 库才是真麻烦
const MUTEX_NAME: &str = r"Local\com.workbench.app";

/// 拿到手的锁句柄。`HANDLE` 是 Copy、没有 Drop，不关它就活到进程退出，
/// 存这一笔纯粹是标明「故意不关」——别在这里写 `mem::forget`，那是空操作。
static LOCK: AtomicIsize = AtomicIsize::new(0);

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 返回 false = 已经有一个在跑（调用方该直接退出），true = 拿到了锁可以继续启动。
pub fn claim_or_wake_existing(window_title: &str) -> bool {
    claim(MUTEX_NAME, window_title)
}

fn claim(name: &str, window_title: &str) -> bool {
    let wide = to_wide(name);
    unsafe {
        let created = CreateMutexW(None, true, PCWSTR(wide.as_ptr()));
        // 紧跟着读，中间不能插别的调用：CreateMutexW 成功时不会清掉上一次的错误码
        let already_running = GetLastError() == ERROR_ALREADY_EXISTS;
        if already_running {
            // 锁是别人先建的那把，我们这份引用得关掉，不然白占着
            if let Ok(handle) = created {
                let _ = CloseHandle(handle);
            }
            wake(window_title);
            return false;
        }
        // 故意不关：进程活着锁才在，进程一退系统自己回收
        if let Ok(handle) = created {
            LOCK.store(handle.0 as isize, Ordering::Relaxed);
        }
        true
    }
}

/// 按窗口标题找现成的那个窗口：标题是从 `tauri.conf.json` 读来的，不在这儿抄第二份
fn wake(window_title: &str) {
    if window_title.is_empty() {
        return;
    }
    let wide = to_wide(window_title);
    unsafe {
        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(wide.as_ptr())) {
            // 收进托盘是 hide()，SW_RESTORE 对隐藏窗口就是「显示出来」
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::claim;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SEQ: AtomicUsize = AtomicUsize::new(0);

    fn unique_name() -> String {
        format!(
            "Local\\workbench-single-instance-test-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        )
    }

    /// 找不到的标题也要能走完：wake 里 FindWindowW 失败只是不做事，不该把流程打断
    #[test]
    fn the_second_claim_loses_and_still_tries_to_wake_the_first() {
        let name = unique_name();
        assert!(claim(&name, "这个标题不存在的工作台"), "第一次该拿到锁");
        assert!(!claim(&name, "这个标题不存在的工作台"), "第二次该认出已有实例");
        // 锁是进程级的，同一把名字再试一次仍然是「已存在」
        assert!(!claim(&name, "这个标题不存在的工作台"));
    }

    #[test]
    fn different_names_do_not_block_each_other() {
        assert!(claim(&unique_name(), "不存在"));
        assert!(claim(&unique_name(), "不存在"));
    }
}
