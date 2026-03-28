mod analyzer;
mod audio;
mod commands;
mod covers;
mod covers_fetch;
mod db;
mod extensions;
mod models;
mod scanner;
mod waveform;

use commands::audio::{AudioEngineState, PlaybackReceiver};
use covers::CoversDir;
use db::DbState;
use std::sync::Mutex;
use tauri::Manager;

#[cfg(target_os = "macos")]
fn apply_macos_window_tweaks(app: &tauri::App) {
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

            // With titleBarStyle: Overlay and decorations: true in tauri.conf.json,
            // Tauri/tao already sets the correct style mask:
            //   Titled | Closable | Miniaturizable | Resizable | FullSizeContentView
            // This preserves native resize handling at window edges while giving us
            // a transparent overlay titlebar. No manual styleMask override needed.

            // Hide the traffic light buttons (close/minimize/zoom) since we have
            // a fully custom UI. They remain functional via keyboard shortcuts.
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

/// Handle a stream:// request — serves audio files with no-cache headers and Range support.
fn handle_stream_request(request: &http::Request<Vec<u8>>) -> http::Response<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    let uri_path = percent_encoding::percent_decode_str(request.uri().path())
        .decode_utf8_lossy()
        .to_string();
    let path = if uri_path.starts_with('/') { &uri_path[1..] } else { &uri_path };

    let Ok(mut file) = std::fs::File::open(path) else {
        return http::Response::builder()
            .status(404)
            .header("Cache-Control", "no-store")
            .body(b"Not found".to_vec())
            .unwrap();
    };
    let Ok(meta) = file.metadata() else {
        return http::Response::builder()
            .status(500)
            .header("Cache-Control", "no-store")
            .body(b"Cannot read file".to_vec())
            .unwrap();
    };
    let len = meta.len();

    // Guess MIME type from extension
    let mime = match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
    {
        "mp3" => "audio/mpeg",
        "m4a" | "aac" => "audio/mp4",
        "flac" => "audio/flac",
        "ogg" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        _ => "application/octet-stream",
    };

    // Check for Range header (HTMLAudioElement always sends these)
    if let Some(range_val) = request.headers().get("range").and_then(|v| v.to_str().ok()) {
        let range = range_val.trim_start_matches("bytes=");
        let parts: Vec<&str> = range.split('-').collect();
        let start: u64 = parts[0].parse().unwrap_or(0);
        let end: u64 = if parts.len() > 1 && !parts[1].is_empty() {
            parts[1].parse().unwrap_or(len - 1)
        } else {
            len - 1
        };
        // Cap chunk to 512KB to limit memory per response
        let end = end.min(start + 512 * 1024 - 1).min(len - 1);
        let chunk_len = (end - start + 1) as usize;

        if file.seek(SeekFrom::Start(start)).is_err() {
            return http::Response::builder()
                .status(500)
                .header("Cache-Control", "no-store")
                .body(b"Seek failed".to_vec())
                .unwrap();
        }
        let mut buf = vec![0u8; chunk_len];
        if file.read_exact(&mut buf).is_err() {
            buf.truncate(0); // partial read
        }

        http::Response::builder()
            .status(206)
            .header("Content-Type", mime)
            .header("Content-Length", chunk_len)
            .header("Content-Range", format!("bytes {start}-{end}/{len}"))
            .header("Accept-Ranges", "bytes")
            .header("Cache-Control", "no-store, no-cache")
            .body(buf)
            .unwrap()
    } else {
        // Full request (rare for audio, but handle it)
        let mut buf = Vec::with_capacity(len as usize);
        let _ = file.read_to_end(&mut buf);

        http::Response::builder()
            .status(200)
            .header("Content-Type", mime)
            .header("Content-Length", len)
            .header("Accept-Ranges", "bytes")
            .header("Cache-Control", "no-store, no-cache")
            .body(buf)
            .unwrap()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        // Custom streaming protocol: serves audio with Cache-Control: no-store
        // so WKWebView doesn't cache every played file in memory forever.
        .register_asynchronous_uri_scheme_protocol("stream", |_ctx, request, responder| {
            std::thread::spawn(move || {
                let resp = handle_stream_request(&request);
                responder.respond(resp);
            });
        })
        .setup(|app| {
            // Apply macOS window tweaks (hide traffic lights, ensure shadow)
            #[cfg(target_os = "macos")]
            apply_macos_window_tweaks(app);

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

            // Initialize audio engine
            let (audio_engine, playback_rx) = audio::engine::AudioEngine::new()
                .expect("Failed to initialize audio engine");
            app.manage(AudioEngineState(std::sync::Mutex::new(audio_engine)));
            app.manage(PlaybackReceiver(std::sync::Mutex::new(playback_rx)));

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
            commands::audio::audio_play,
            commands::audio::audio_pause,
            commands::audio::audio_stop,
            commands::audio::audio_load,
            commands::audio::audio_seek,
            commands::audio::audio_set_volume,
            commands::audio::audio_set_playback_rate,
            commands::audio::audio_start_crossfade,
            commands::audio::audio_cancel_crossfade,
            commands::audio::audio_set_visualization,
            commands::audio::audio_get_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
