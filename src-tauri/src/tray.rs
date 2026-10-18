use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub const TRAY_ID: &str = "workbench-tray";

/// 托盘图标：左键唤回主窗口，右键菜单里有「显示主窗口 / 退出工作台」。
/// 装了托盘，窗口关闭键才会变成「隐藏到托盘」，见 on_close_requested。
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    if app.tray_by_id(TRAY_ID).is_some() {
        return Ok(());
    }
    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出工作台", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let icon = match app.default_window_icon() {
        Some(i) => i.clone(),
        None => Image::from_bytes(include_bytes!("../icons/32x32.png"))?,
    };

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("个人工作台")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => reveal(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => reveal(tray.app_handle()),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

pub fn remove(app: &AppHandle) {
    app.remove_tray_by_id(TRAY_ID);
}

pub fn installed(app: &AppHandle) -> bool {
    app.tray_by_id(TRAY_ID).is_some()
}

fn reveal(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// 点关闭键时：装了托盘就只隐藏窗口（调用方再 prevent_close），否则照常退出
pub fn hide_to_tray(app: &AppHandle) -> bool {
    if !installed(app) {
        return false;
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    true
}
