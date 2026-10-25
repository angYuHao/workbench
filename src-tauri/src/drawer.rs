//! 顶边抽屉：鼠标顶到屏幕最上沿，从顶边中间滑出一块搜索面板（全部应用 / 全部网址，可搜可点开）。
//!
//! 它是独立的一个窗口（label = `drawer`），跟主窗口互不相干：主窗口该开着就开着、该藏着就藏着，
//! 顶边划过不会动它一下。这也是把抽屉从主窗口里拆出来的原因 —— 以前顶边一划是把主窗口
//! 拉来贴顶，等于每次 glance 一下都要劫持一次用户正在用的那个页面。
//!
//! 尺寸是工作区宽度的三分之一、横向居中、高度五分之二，里面两栏各自滚动。
//! 铺满整条顶边会压住别人窗口的标题栏，所以只占中间那一段。
//!
//! **它是可交互的**：显示即拿焦点（要能直接打字搜索），所以点它会把你正在输入的窗口的焦点抢走 ——
//! 这是「能搜」换来的代价，不划出来的时候它压根不在屏幕上。置顶是无条件的：
//! 全屏播放、打游戏、放映 PPT 时划过顶边照样出（以前那条「全屏不弹出」的闸按需求去掉了）。
//!
//! 为什么是轮询而不是全局鼠标钩子：`SetWindowsHookEx(WH_MOUSE_LL)` 要一条一直在 pump 消息的
//! 线程，而且这台机器上「全局快捷键唤出」那套已经因为撞键被拆掉过（见 README）。每 150ms 读一次
//! `GetCursorPos` 只要一个 sleep 循环，代价是最多晚 150ms 才弹出，手感上看不出来。
//!
//! 两条不打扰的规则：
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
/// 光标离开窗口矩形多久才真的收起：防路过边缘的一下抖动
const LEAVE: Duration = Duration::from_millis(450);
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
/// 上一次「显示 + 让出焦点」的时刻：刚叫出来的这一小会儿不收
static SHOWN_AT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// 光标离开窗口的起点，0 表示现在在里面
static LEFT_AT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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

/// 贴顶居中显示：宽取工作区三分之一、高取五分之二，里面自己滚
fn show_drawer(app: &AppHandle, work: &Work) {
    let Some(w) = app.get_webview_window(DRAWER_LABEL) else {
        return;
    };
    // 工作区的尺寸本来就是物理像素，直接按份额切，不用再折一次 DPI
    let width = work.width / WIDTH_SHARE;
    let height = (work.height * 2 / HEIGHT_SHARE).max(MIN_HEIGHT);
    let left = work.left + (work.width - width) / 2;
    let _ = w.set_size(PhysicalSize::new(width as u32, height as u32));
    let _ = w.set_position(PhysicalPosition::new(left, work.top));
    let _ = w.show();
    // 面板要能直接打字，所以显示即拿焦点；这是「可交互」换来的代价，
    // 不划出来的时候它压根不在屏幕上
    let _ = w.set_focus();
    SHOWN_AT.store(now_ms(), Ordering::Relaxed);
    LEFT_AT.store(0, Ordering::Relaxed);
    ENGAGED.store(false, Ordering::Relaxed);
}

fn hide_drawer(app: &AppHandle) {
    let Some(w) = app.get_webview_window(DRAWER_LABEL) else {
        return;
    };
    let _ = w.hide();
    ENGAGED.store(false, Ordering::Relaxed);
    LEFT_AT.store(0, Ordering::Relaxed);
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
            let at_top = p.y <= work.top + EDGE_PX && p.x >= work.left && p.x < work.left + work.width;
            if at_top {
                show_drawer(&app, &work);
            }
            continue;
        }

        if inside_window(&app, p) {
            ENGAGED.store(true, Ordering::Relaxed);
            LEFT_AT.store(0, Ordering::Relaxed);
            continue;
        }
        // 刚叫出来的这一小会儿、正拖着东西、或者光标还没进过窗口：都先不收
        if now_ms() - SHOWN_AT.load(Ordering::Relaxed) < 700 || button_held() {
            continue;
        }
        if !ENGAGED.load(Ordering::Relaxed) {
            continue;
        }
        let left_at = LEFT_AT.load(Ordering::Relaxed);
        if left_at == 0 {
            LEFT_AT.store(now_ms(), Ordering::Relaxed);
        } else if now_ms() - left_at >= LEAVE.as_millis() as u64 {
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
    LEFT_AT.store(0, Ordering::Relaxed);
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
