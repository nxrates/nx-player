use std::io::Write;
use tauri::State;

use crate::db::{self, DbState};
use crate::models::{Playlist, Track};

#[tauri::command]
pub fn create_playlist(name: String, db_state: State<'_, DbState>) -> Result<Playlist, String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    let id = uuid::Uuid::new_v4().to_string();
    db::create_playlist(&conn, &id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_playlist(
    id: String,
    name: String,
    db_state: State<'_, DbState>,
) -> Result<(), String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::rename_playlist(&conn, &id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_playlist(id: String, db_state: State<'_, DbState>) -> Result<(), String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::delete_playlist(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_playlists(db_state: State<'_, DbState>) -> Result<Vec<Playlist>, String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::get_playlists(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_playlist_tracks(
    id: String,
    db_state: State<'_, DbState>,
) -> Result<Vec<Track>, String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::get_playlist_tracks(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_to_playlist(
    playlist_id: String,
    track_ids: Vec<String>,
    db_state: State<'_, DbState>,
) -> Result<(), String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::add_tracks_to_playlist(&conn, &playlist_id, &track_ids).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_from_playlist(
    playlist_id: String,
    track_id: String,
    db_state: State<'_, DbState>,
) -> Result<(), String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::remove_track_from_playlist(&conn, &playlist_id, &track_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reorder_playlist(
    playlist_id: String,
    track_ids: Vec<String>,
    db_state: State<'_, DbState>,
) -> Result<(), String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    db::reorder_playlist_tracks(&conn, &playlist_id, &track_ids).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_playlist_m3u(
    id: String,
    path: String,
    db_state: State<'_, DbState>,
) -> Result<(), String> {
    let conn = db_state.0.lock().map_err(|e| e.to_string())?;
    let tracks = db::get_playlist_tracks(&conn, &id).map_err(|e| e.to_string())?;

    let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    writeln!(file, "#EXTM3U").map_err(|e| e.to_string())?;

    for track in &tracks {
        let duration = track.duration as i64;
        let display = if track.artist.is_empty() {
            track.title.clone()
        } else {
            format!("{} - {}", track.artist, track.title)
        };
        writeln!(file, "#EXTINF:{},{}", duration, display).map_err(|e| e.to_string())?;
        writeln!(file, "{}", track.path).map_err(|e| e.to_string())?;
    }

    Ok(())
}
