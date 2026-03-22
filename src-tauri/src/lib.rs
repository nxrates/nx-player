mod analyzer;
mod commands;
mod covers;
mod covers_fetch;
mod db;
mod extensions;
mod models;
mod scanner;
mod waveform;

use covers::CoversDir;
use db::DbState;
use std::sync::Mutex;
use tauri::Manager;

#[cfg(target_os = "macos")]
fn apply_macos_window_rounding(app: &tauri::App) {
    use raw_window_handle::HasWindowHandle;

    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let raw = handle.as_raw();
    if let raw_window_handle::RawWindowHandle::AppKit(appkit) = raw {
        let ns_view = appkit.ns_view.as_ptr() as *mut objc2::runtime::AnyObject;
        unsafe {
            let ns_window: *mut objc2::runtime::AnyObject =
                objc2::msg_send![ns_view, window];

            let current_mask: u64 = objc2::msg_send![ns_window, styleMask];
            let new_mask: u64 = current_mask
                | (1 << 0)   // NSTitledWindowMask
                | (1 << 1)   // NSClosableWindowMask
                | (1 << 2)   // NSMiniaturizableWindowMask
                | (1 << 3)   // NSResizableWindowMask
                | (1 << 15); // NSFullSizeContentViewWindowMask
            let _: () = objc2::msg_send![ns_window, setStyleMask: new_mask];

            let _: () = objc2::msg_send![ns_window, setTitlebarAppearsTransparent: true];
            let _: () = objc2::msg_send![ns_window, setTitleVisibility: 1_i64];
            // Note: setMovableByWindowBackground removed — it conflicts with resize edges.
            // Dragging is handled via data-tauri-drag-region on the topbar instead.

            let toolbar: *mut objc2::runtime::AnyObject =
                objc2::msg_send![ns_window, toolbar];
            if !toolbar.is_null() {
                let _: () = objc2::msg_send![toolbar, setVisible: false];
            }

            for i in 0_i64..3 {
                let btn: *mut objc2::runtime::AnyObject =
                    objc2::msg_send![ns_window, standardWindowButton: i];
                if !btn.is_null() {
                    let _: () = objc2::msg_send![btn, setHidden: true];
                }
            }

            let _: () = objc2::msg_send![ns_window, setHasShadow: true];

            let content_view: *mut objc2::runtime::AnyObject =
                objc2::msg_send![ns_window, contentView];
            let _: () = objc2::msg_send![content_view, setWantsLayer: true];
        }
    }
}

/// Returns the default music directories for the current OS,
/// filtered to only those that actually exist on disk.
fn default_music_dirs() -> Vec<String> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    if let Some(home) = dirs::home_dir() {
        // ~/Music is conventional on all platforms
        candidates.push(home.join("Music"));

        #[cfg(target_os = "linux")]
        {
            candidates.push(home.join(".local/share/music"));
            candidates.push(home.join("music"));
        }
    }

    // On Windows, also try %USERPROFILE%\Music via the audio dir helper
    #[cfg(target_os = "windows")]
    {
        if let Some(audio) = dirs::audio_dir() {
            candidates.push(audio);
        }
    }

    // Deduplicate (canonicalize where possible) and filter to existing dirs
    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|p| p.is_dir())
        .filter(|p| {
            let canonical = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
            seen.insert(canonical)
        })
        .map(|p| p.to_string_lossy().to_string())
        .collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // Apply rounded corners on macOS
            #[cfg(target_os = "macos")]
            apply_macos_window_rounding(app);

            // Create app data directories
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");
            std::fs::create_dir_all(&app_data_dir).expect("Failed to create app data directory");

            let covers_dir = app_data_dir.join("covers");
            std::fs::create_dir_all(&covers_dir).expect("Failed to create covers directory");

            // Initialize database
            let db_path = app_data_dir.join("lightseek.db");
            let conn =
                db::initialize(&db_path).expect("Failed to initialize database");

            // Get folders for auto-scan before moving conn into state
            let mut folders = db::get_folders(&conn).unwrap_or_default();

            // First launch: no folders configured yet — seed with OS defaults
            if folders.is_empty() {
                let defaults = default_music_dirs();
                for dir in &defaults {
                    if let Err(e) = db::add_folder(&conn, dir) {
                        eprintln!("Failed to add default folder {}: {}", dir, e);
                    }
                }
                folders = defaults;
            }

            // Initialize extension host
            let ext_dir = app_data_dir.join("extensions");
            app.manage(extensions::ExtensionHostState(Mutex::new(
                extensions::ExtensionHost::new(ext_dir),
            )));

            // Manage state
            app.manage(DbState(Mutex::new(conn)));
            app.manage(CoversDir(covers_dir.clone()));

            // Auto-scan on startup if there are any folders to scan
            if !folders.is_empty() {
                let app_handle = app.handle().clone();
                let covers_path = covers_dir.clone();
                std::thread::spawn(move || {
                    let db_path_clone = app_handle
                        .path()
                        .app_data_dir()
                        .expect("Failed to get app data dir")
                        .join("lightseek.db");
                    match db::initialize(&db_path_clone) {
                        Ok(conn) => {
                            let conn_mutex = Mutex::new(conn);
                            if let Err(e) = scanner::scan_folders(
                                &folders,
                                &conn_mutex,
                                &covers_path,
                                &app_handle,
                            ) {
                                eprintln!("Auto-scan error: {}", e);
                            }
                        }
                        Err(e) => eprintln!("Failed to open DB for auto-scan: {}", e),
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::library::scan_library,
            commands::library::get_tracks,
            commands::library::get_track,
            commands::library::get_artists,
            commands::library::get_tracks_by_artist,
            commands::library::get_cover_path,
            commands::library::get_waveform,
            commands::library::add_folder,
            commands::library::remove_folder,
            commands::library::get_folders,
            commands::library::fetch_cover_art,
            commands::library::reset_library,
            commands::playlists::create_playlist,
            commands::playlists::rename_playlist,
            commands::playlists::delete_playlist,
            commands::playlists::get_playlists,
            commands::playlists::get_playlist_tracks,
            commands::playlists::add_to_playlist,
            commands::playlists::remove_from_playlist,
            commands::playlists::reorder_playlist,
            commands::playlists::export_playlist_m3u,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::extensions::list_extensions,
            commands::extensions::install_extension,
            commands::extensions::uninstall_extension,
            commands::extensions::start_extension,
            commands::extensions::stop_extension,
            commands::extensions::extension_search,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
