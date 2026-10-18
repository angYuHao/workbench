use tauri::{AppHandle, Manager};

use crate::win::screen;

/// 二维码在屏幕上通常只有两三百像素，压到 1000 像素足够 jsQR 识别，
/// 又能把单次 IPC 的体积控制在一两兆以内。
const MAX_EDGE: u32 = 1000;

/// 隐藏本窗口 → 抓屏 → 恢复窗口。异步命令跑在工作线程上，
/// 阻塞式睡眠才不会卡住主线程，让 hide()/show() 真的生效。
#[tauri::command]
pub async fn capture_screen(app: AppHandle) -> Result<String, String> {
    let window = app.get_webview_window("main");
    if let Some(w) = &window {
        let _ = w.hide();
    }
    // 等合成器把窗口从屏幕上移走，否则截到的是残留的旧帧
    std::thread::sleep(std::time::Duration::from_millis(300));
    let captured = screen::capture_png_base64(MAX_EDGE);
    if let Some(w) = window {
        let _ = w.show();
        let _ = w.set_focus();
    }
    captured.map_err(|e| e.to_string())
}
