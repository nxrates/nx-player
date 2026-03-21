use tauri::State;

use crate::covers::CoversDir;
use crate::db::{self, DbState};
use crate::models::{ArtistSummary, Track};
use crate::scanner;

#[tauri::command]
pub fn scan_library(
    folders: Vec<String>,
    db_state: State<'_, DbState>,
    covers_dir: State<'_, CoversDir>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;

    // If no folders provided, use the stored folders
    let scan_folders = if folders.is_empty() {
        db::get_folders(&conn).map_err(|e| e.to_string())?
    } else {
        folders
    };
    drop(conn);

    if scan_folders.is_empty() {
        return Ok(());
    }

    // Get the DB path so we can open a new connection in the background thread
    let db_path = {
        let c = db_state.0.lock().map_err(|e| e.to_string())?;
        c.path().map(|p| std::path::PathBuf::from(p))
    };

    let covers_path = covers_dir.0.clone();

    std::thread::spawn(move || {
        if let Some(db_path) = db_path {
            match crate::db::initialize(&db_path) {
                Ok(conn) => {
                    let conn_mutex = std::sync::Mutex::new(conn);
                    if let Err(e) = scanner::scan_folders(
                        &scan_folders,
                        &conn_mutex,
                        &covers_path,
                        &app_handle,
                    ) {
                        eprintln!("Scan error: {}", e);
                    }
                }
                Err(e) => eprintln!("Failed to open DB for scan: {}", e),
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn get_tracks(
    sort_by: String,
    sort_order: String,
    search: Option<String>,
    db_state: State<'_, DbState>,
) -> Result<Vec<Track>, String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::get_all_tracks(&conn, &sort_by, &sort_order, search.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_track(id: String, db_state: State<'_, DbState>) -> Result<Track, String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::get_track_by_id(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_artists(db_state: State<'_, DbState>) -> Result<Vec<ArtistSummary>, String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::get_artists(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_tracks_by_artist(
    artist: String,
    db_state: State<'_, DbState>,
) -> Result<Vec<Track>, String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::get_tracks_by_artist(&conn, &artist).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_cover_path(
    track_id: String,
    covers_dir: State<'_, CoversDir>,
) -> Result<Option<String>, String> {
    Ok(crate::covers::get_cover_path(&track_id, &covers_dir.0))
}

#[tauri::command]
pub fn get_waveform(
    track_id: String,
    db_state: State<'_, DbState>,
) -> Result<Option<Vec<u8>>, String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    // Check DB first
    if let Ok(Some(wf)) = db::get_waveform(&conn, &track_id) {
        return Ok(Some(wf));
    }
    // Generate on demand if not cached
    let track = db::get_track_by_id(&conn, &track_id).map_err(|e| e.to_string())?;
    let wf = crate::waveform::extract_waveform(std::path::Path::new(&track.path));
    if let Some(ref data) = wf {
        let _ = conn.execute(
            "UPDATE tracks SET waveform = ?1 WHERE id = ?2",
            rusqlite::params![data, track_id],
        );
    }
    Ok(wf)
}

#[tauri::command]
pub fn add_folder(path: String, db_state: State<'_, DbState>) -> Result<(), String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::add_folder(&conn, &path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_folder(path: String, db_state: State<'_, DbState>) -> Result<(), String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::remove_folder(&conn, &path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_folders(db_state: State<'_, DbState>) -> Result<Vec<String>, String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::get_folders(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn fetch_cover_art(
    track_id: String,
    artist: String,
    title: String,
    covers_dir: State<'_, CoversDir>,
    db_state: State<'_, DbState>,
) -> Result<Option<String>, String> {
    let dest = covers_dir.0.join(format!("{}.jpg", track_id));

    // If already have a cover, return it
    if dest.exists() {
        return Ok(Some(dest.to_string_lossy().to_string()));
    }

    let covers_path = covers_dir.0.clone();

    // Fetch from iTunes
    let result =
        crate::covers_fetch::fetch_cover(&artist, &title, &track_id, &covers_path).await;

    if result.is_some() {
        // Update the track's has_cover in the database
        let conn = db_state.0.lock().map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "UPDATE tracks SET has_cover = 1 WHERE id = ?1",
            rusqlite::params![track_id],
        );
    }

    Ok(result.map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
pub fn reset_library(
    db_state: State<'_, DbState>,
    covers_dir: State<'_, CoversDir>,
) -> Result<(), String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::delete_all_tracks(&conn).map_err(|e| e.to_string())?;
    db::delete_all_folders(&conn).map_err(|e| e.to_string())?;

    // Delete all cover files
    if covers_dir.0.exists() {
        if let Ok(entries) = std::fs::read_dir(&covers_dir.0) {
            for entry in entries.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    Ok(())
}
