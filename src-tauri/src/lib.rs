mod commands;
mod db;
mod drawer;
mod error;
mod filewatcher;
mod models;
mod paths;
mod services;
mod tray;
mod win;

use tauri::Manager;

pub fn run() {
    let context = tauri::generate_context!();

    // 只允许一个实例：第二个进程刚起来就把现有窗口叫到前面然后退出。
    // 窗口收进托盘后再双击图标，不然会开出第二个窗口、挂出第二个托盘图标
    let title = context
        .config()
        .app
        .windows
        .first()
        .map(|window| window.title.clone())
        .unwrap_or_default();
    if !win::single_instance::claim_or_wake_existing(&title) {
        return;
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if tray::hide_to_tray(window.app_handle()) {
                    api.prevent_close();
                }
            }
        })
        .setup(|app| {
            let state = db::init()?;
            // 先起一个空监视器：这页开的是整台机器，启动时并不知道用户要看哪几个目录，
            // 由文件页在切换目录时通过 `watch_folders` 说给它
            let fw = filewatcher::FileWatcher::new(app.handle().clone(), &[])?;
            *state.watcher.lock().unwrap() = Some(fw);
            app.manage(state);

            // 预览要读磁盘上的文件，但 webview 不该因此看见整块盘：静态 scope 是空的，
            // 这里按「桌面 + 图标缓存」逐条授权到目录级（桌面页两处都要用）。
            // 文件页其余的目录是用户点到哪一次，`authorize_preview` 当场授到哪一次。
            // 某条目录还没建好或读不到时不拦启动，用户真要预览它时会拿到明确的读取失败。
            let app_state = app.state::<db::AppState>();
            let conn = app_state.conn();
            let roots = services::files::preview_roots(&conn).unwrap_or_default();
            drop(conn);
            for root in roots {
                let _ = app.asset_protocol_scope().allow_directory(&root, true);
            }

            // AI 日志只按行数自己滚。启动时先清一次，之后每次写之前也会清，
            // 所以不需要定时器：一直开着不写的时候它本来也不会涨。
            services::ai::log::cleanup();

            // 托盘默认开：没存过这个键就算开，只有用户明确关掉才存 "0"。
            // 不然窗口一关就是真退出，收进托盘这个入口也就没有了
            let tray_off = services::settings::get_setting(
                &app.state::<db::AppState>().conn(),
                "close_to_tray",
            )
            .ok()
            .flatten()
            .as_deref()
                == Some("0");
            if !tray_off {
                tray::install(app.handle())?;
            }

            // 顶边抽屉：没写过这个键就算开。第一次跑（库里压根没这条）先把窗口显示一次，
            // 不然装完点开啥也没看见，只会以为程序没起来
            let conn = app_state.conn();
            let edge = services::settings::get_setting(&conn, "edge_drawer")
                .ok()
                .flatten();
            drop(conn);
            let drawer_on = edge.as_deref() != Some("0");
            drawer::apply(app.handle(), drawer_on);
            if edge.is_none() {
                drawer::reveal_once(app.handle());
            }
            drawer::start(app.handle());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::desktop::scan_desktop,
            commands::desktop::organize_desktop,
            commands::desktop::revert_organize,
            commands::desktop::launch_app,
            commands::desktop::get_apps,
            commands::desktop::save_app,
            commands::desktop::delete_app,
            commands::desktop::sync_installed_apps,
            commands::desktop::preview_installed_apps,
            commands::desktop::get_app_filters,
            commands::desktop::add_app_filter,
            commands::desktop::remove_app_filter,
            commands::bookmarks::import_bookmarks,
            commands::bookmarks::analyze_bookmarks,
            commands::bookmarks::apply_bookmark_analysis,
            commands::bookmarks::get_page_cache_count,
            commands::bookmarks::clear_page_cache,
            commands::bookmarks::get_bookmark_blacklist,
            commands::bookmarks::add_bookmark_blacklist,
            commands::bookmarks::remove_bookmark_blacklist,
            commands::bookmarks::block_bookmarks,
            commands::bookmarks::get_bookmark_trash_count,
            commands::bookmarks::restore_bookmark_trash,
            commands::bookmarks::empty_bookmark_trash,
            commands::bookmarks::get_bookmarks,
            commands::bookmarks::save_bookmark,
            commands::bookmarks::delete_bookmark,
            commands::bookmarks::open_url,
            commands::files::get_drives,
            commands::files::get_known_folders,
            commands::files::authorize_preview,
            commands::files::get_shell_icons,
            commands::files::watch_folders,
            commands::files::list_files,
            commands::files::get_file,
            commands::files::save_file_meta,
            commands::files::read_text_file,
            commands::files::read_file_head,
            commands::files::create_folder,
            commands::files::rename_path,
            commands::files::delete_paths,
            commands::files::copy_paths,
            commands::files::move_paths,
            commands::files::clipboard_write,
            commands::files::clipboard_read,
            commands::files::open_path,
            commands::files::reveal_in_explorer,
            commands::categories::get_categories,
            commands::categories::save_category,
            commands::categories::rename_category,
            commands::categories::delete_category,
            commands::categories::delete_categories,
            commands::categories::get_rules,
            commands::categories::save_rule,
            commands::categories::delete_rule,
            commands::ai::get_ai_models,
            commands::ai::save_ai_model,
            commands::ai::delete_ai_model,
            commands::ai::list_remote_ai_models,
            commands::ai::probe_ai_model,
            commands::ai::open_ai_log,
            commands::ai::read_ai_log,
            commands::settings::get_settings,
            commands::settings::set_setting,
            commands::settings::set_close_to_tray,
            commands::settings::set_edge_drawer,
            commands::settings::export_db,
            commands::settings::restore_db,
            commands::categorize::smart_categorize,
            commands::categorize::get_ai_memory,
            commands::categorize::clear_ai_memory,
            commands::categorize::apply_smart_categorize,
            commands::totp::get_totp_accounts,
            commands::totp::save_totp_account,
            commands::totp::delete_totp_account,
            commands::totp::totp_code,
            commands::totp::totp_secret,
            commands::totp::export_totp_backup,
            commands::totp::import_totp_backup,
            commands::notify::broadcast_change,
            commands::screen::capture_screen
        ])
        .run(context)
        .expect("error while running workbench");
}