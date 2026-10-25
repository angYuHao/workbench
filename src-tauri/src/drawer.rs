//! 顶边抽屉：鼠标顶到屏幕最上沿**中间那一段**，从顶边滑出一块搜索面板（全部应用 / 全部网址，可搜可点开）。
//! 左右两端的顶边划过什么都不出 —— 那两段下面是别人窗口的标题栏和最小化按钮，顶到就弹一条会把它们挡掉。
//!
//! 它是独立的一个窗口（label = `drawer`），跟主窗口互不相干：主窗口该开着就开着、该藏着就藏着，
//! 顶边划过不会动它一下。这也是把抽屉从主窗口里拆出来的原因 —— 以前顶边一划是把主窗口
//! 拉来贴顶，等于每次 glance 一下都要劫持一次用户正在用的那个页面。
//!
//! 尺寸是工作区宽度的三分之一、横向居中、高度五分之二，里面两栏各自滚动。
//!
//! **它是可交互的**：显示即拿焦点（要能直接打字搜索），所以点它会把你正在输入的窗口的焦点抢走 ——
//! 这是「能搜」换来的代价，不划出来的时候它压根不在屏幕上。置顶是无条件的：
//! 全屏播放、打游戏、放映 PPT 时划过顶边照样出（以前那条「全屏不弹出」的闸按需求去掉了）。
//!
//! 为什么是轮询而不是全局鼠标钩子：`SetWindowsHookEx(WH_MOUSE_LL)` 要一条一直在 pump 消息的
//! 线程，而且这台机器上「全局快捷键唤出」那套已经因为撞键被拆掉过（见 README）。每 150ms 读一次
//! `GetCursorPos` 只要一个 sleep 循环，代价是最多晚 150ms 才弹出，手感上看不出来。
//!
//! 收起是不计时的：光标划出矩形、或者焦点被别的窗口抢走，下一轮轮询（最多 150ms）就收。
//! 几条不打扰的例外：
//! - **刚弹出那 700ms**：`set_focus` 未必已经落定，这时候判「没焦点」会自己把自己关掉。
//! - **焦点压根没落上来过**：有的全屏程序死占焦点，这时「没焦点」是常态不是「人走了」，
//!   照它收会 弹出→收→弹出 地闪（光标一直待在顶边那三个像素里，收了马上又触发）。
//! - **Engaged 那一下**：光标真的进过面板一次，才允许自动收起。所以点托盘的「显示主窗口」
//!   （光标还在托盘上）不会一闪就没。
//! - **按住不放就不收**：正在拖东西、正在面板里选中文字时，光标迟早会越出矩形，
//!   中途收掉等于把这次操作废了，所以左右中键有一个还按着就暂缓。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITOR_DEFAULTTONEAREST, MONITORINFO,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// 轮询间隔：足够快到手感看不出来，又不至于白烧 CPU
const POLL: Duration = Duration::from_millis(150);
/// 光标顶到工作区上沿几个像素以内算「顶边」
const EDGE_PX: i32 = 3;
/// 刚弹出来这一小会儿不收：焦点可能还没落到面板上，抢着判会自己把自己关掉
const SETTLE: u64 = 700;
/// 抽屉那条的 label。它和主窗口是两个窗口，谁也不碰谁。
const DRAWER_LABEL: &str = "drawer";
/// 宽度取工作区的三分之一、横向居中：顶边中间那块是「抬头看一眼」的位置，
/// 铺满整条会压住别人正在用的窗口标题栏
const WIDTH_SHARE: i32 = 3;
/// 高度取工作区的五分之二，里面自己滚。装不下 52 个应用加几百条网址，所以必须有滚动
const HEIGHT_SHARE: i32 = 5;
/// 圆整用的最小高度，屏幕被压得很扁时也不至于塌成一条缝
const MIN_HEIGHT: i32 = 240;

/// 抽屉模式开否（设置里那一行改的就是它）
static ENABLED: AtomicBool = AtomicBool::new(false);
/// 轮询线程只起一次，来回开关不能重复起
static RUNNING: AtomicBool = AtomicBool::new(false);
/// 光标这轮是否真的进过抽屉——没进过就不自动收（托盘唤回的场景靠它）
static ENGAGED: AtomicBool = AtomicBool::new(false);
/// 这轮抽屉有没有拿到过焦点。丢焦点算不算「人走了」只在拿到过之后才成立，见轮询里那段
static HAD_FOCUS: AtomicBool = AtomicBool::new(false);
/// 上一次「显示 + 让出焦点」的时刻：刚叫出来的这一小会儿不收
static SHOWN_AT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 工作区：显示器去掉任务栏那块，顶边和贴顶尺寸都以它为准
struct Work {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

fn monitor_of(point: POINT) -> Option<Work> {
    unsafe {
        let hmon = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // GetMonitorInfoW 回的是 BOOL（不是 windows-rs 那种 Result），判真值就行
        if !GetMonitorInfoW(hmon, &mut info).as_bool() {
            return None;
        }
        Some(rect_to_work(info.rcWork))
    }
}

fn rect_to_work(rc: RECT) -> Work {
    Work {
        left: rc.left,
        top: rc.top,
        width: rc.right - rc.left,
        height: rc.bottom - rc.top,
    }
}

fn cursor() -> Option<(POINT, Work)> {
    unsafe {
        let mut p = POINT::default();
        GetCursorPos(&mut p).ok()?;
        Some((p, monitor_of(p)?))
    }
}

/// 任何一个鼠标键还按着：正拖东西的时候绝不收窗
fn button_held() -> bool {
    unsafe {
        [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON]
            .iter()
            .any(|vk| GetAsyncKeyState(vk.0 as i32) < 0)
    }
}

/// 存的是毫秒时间戳，用来算「离开多久了」「刚弹出多久」这两段时间差。
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 面板横向占的那一段：左边界和宽度。触发区和面板**必须用同一个算法**，
/// 分成两处写迟早对不上——变成「弹出来的位置和我顶的那条边不是一回事」
fn panel_span(work: &Work) -> (i32, i32) {
    let width = work.width / WIDTH_SHARE;
    (work.left + (work.width - width) / 2, width)
}

/// 贴顶居中显示：宽取工作区三分之一、高取五分之二，里面自己滚
fn show_drawer(app: &AppHandle, work: &Work) {
    let Some(w) = app.get_webview_window(DRAWER_LABEL) else {
        return;
    };
    // 工作区的尺寸本来就是物理像素，直接按份额切，不用再折一次 DPI
    let (left, width) = panel_span(work);
    let height = (work.height * 2 / HEIGHT_SHARE).max(MIN_HEIGHT);
    let _ = w.set_size(PhysicalSize::new(width as u32, height as u32));
    let _ = w.set_position(PhysicalPosition::new(left, work.top));
    let _ = w.show();
    // 面板要能直接打字，所以显示即拿焦点；这是「可交互」换来的代价，
    // 不划出来的时候它压根不在屏幕上
    let _ = w.set_focus();
    SHOWN_AT.store(now_ms(), Ordering::Relaxed);
    ENGAGED.store(false, Ordering::Relaxed);
    HAD_FOCUS.store(false, Ordering::Relaxed);
}

fn hide_drawer(app: &AppHandle) {
    let Some(w) = app.get_webview_window(DRAWER_LABEL) else {
        return;
    };
    let _ = w.hide();
    ENGAGED.store(false, Ordering::Relaxed);
    HAD_FOCUS.store(false, Ordering::Relaxed);
}

/// 光标在不在抽屉那条矩形里
fn inside_window(app: &AppHandle, p: POINT) -> bool {
    let Some(w) = app.get_webview_window(DRAWER_LABEL) else {
        return false;
    };
    let Ok(pos) = w.outer_position() else {
        return false;
    };
    let Ok(size) = w.outer_size() else {
        return false;
    };
    p.x >= pos.x
        && p.x < pos.x + size.width as i32
        && p.y >= pos.y
        && p.y < pos.y + size.height as i32
}

/// 起轮询线程（整个进程只起一次）
pub fn start(app: &AppHandle) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(POLL);
        if !ENABLED.load(Ordering::Relaxed) {
            continue;
        }
        let Some((p, work)) = cursor() else { continue };
        let Some(w) = app.get_webview_window(DRAWER_LABEL) else { continue };
        let visible = w.is_visible().unwrap_or(false);

        if !visible {
            // 触发区和面板同一套算法：左右两端那两段顶边划过什么都不出
            let (left, width) = panel_span(&work);
            if p.y <= work.top + EDGE_PX && p.x >= left && p.x < left + width {
                show_drawer(&app, &work);
            }
            continue;
        }

        let inside = inside_window(&app, p);
        if inside {
            ENGAGED.store(true, Ordering::Relaxed);
        }
        // 焦点只要落到面板上**过一次**，之后「丢焦点」才是真信号。从没拿到过焦点的时候
        // （某个全屏程序死死占着不放）不靠它判断 —— 那样会陷入 弹出→收→弹出 的闪，
        // 因为光标一直待在顶边那三个像素里。
        let focused = w.is_focused().unwrap_or(false);
        if focused {
            HAD_FOCUS.store(true, Ordering::Relaxed);
        }
        // 刚叫出来的这一小会儿（焦点未必已经落定）、正拖着东西、光标还没进过面板：
        // 这三样都先不收。除此之外不再有任何计时——离开即收。
        if now_ms() - SHOWN_AT.load(Ordering::Relaxed) < SETTLE
            || button_held()
            || !ENGAGED.load(Ordering::Relaxed)
        {
            continue;
        }
        // 「离开了」有两种：光标划出矩形，或者鼠标还停在面板里但焦点已经被点走的别的窗口抢走
        //（那种情况面板已经打字不了了，留着只是挡屏幕）
        if !inside || (HAD_FOCUS.load(Ordering::Relaxed) && !focused) {
            hide_drawer(&app);
        }
    });
}

/// 开关这个模式。抽屉拆成独立窗口之后，这里就只管那条：关掉时顺手收起来，
/// 别再留一条挂在屏幕上。**主窗口的边框、置顶、大小一概不碰** —— 它是个正常窗口，
/// 该被用户自己开着用，不再是抽屉的替身。
pub fn apply(app: &AppHandle, on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
    ENGAGED.store(false, Ordering::Relaxed);
    if !on {
        hide_drawer(app);
    }
}

/// 装完第一次跑：总得让人看见窗口一次，不然以为没启动起来
pub fn reveal_once(app: &AppHandle) {
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let _ = w.show();
    let _ = w.set_focus();
    SHOWN_AT.store(now_ms(), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 触发区 = 面板本身，而且只占中间那一段：两端各留三分之一，划过不弹
    #[test]
    fn trigger_is_the_middle_third_only() {
        let work = Work { left: 0, top: 0, width: 1920, height: 1080 };
        let (left, width) = panel_span(&work);
        assert_eq!(width, 640);
        assert_eq!(left, 640);
        assert!(left > work.left, "左端那一段不该触发");
        assert!(left + width < work.left + work.width, "右端那一段不该触发");
    }

    /// 副屏挂在左边时工作区原点是个负数，居中不能按 0 算
    #[test]
    fn panel_span_follows_a_shifted_work_area() {
        let work = Work { left: -1920, top: 0, width: 1920, height: 1080 };
        let (left, width) = panel_span(&work);
        assert_eq!((left + width / 2, width), (-960, 640));
    }
}
