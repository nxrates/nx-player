use lofty::prelude::*;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;
use walkdir::WalkDir;

use crate::covers;
use crate::db;
use crate::models::{AppError, ScanProgress, Track};

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "aac", "flac", "ogg", "opus", "wav"];

fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn scan_folders(
    folders: &[String],
    conn: &Mutex<rusqlite::Connection>,
    covers_dir: &Path,
    app_handle: &tauri::AppHandle,
) -> Result<(), AppError> {
    use tauri::Emitter;

    // Phase 1: Discover audio files
    let _ = app_handle.emit(
        "scan-progress",
        ScanProgress {
            current: 0,
            total: 0,
            phase: "discovering".to_string(),
        },
    );

    let mut audio_files: Vec<PathBuf> = Vec::new();
    for folder in folders {
        for entry in WalkDir::new(folder)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path().to_path_buf();
            if path.is_file() && is_audio_file(&path) {
                audio_files.push(path);
            }
        }
    }

    let total = audio_files.len();
    let _ = app_handle.emit(
        "scan-progress",
        ScanProgress {
            current: 0,
            total,
            phase: "scanning".to_string(),
        },
    );

    // Phase 2: Check which files need updating (by mtime or missing BPM/waveform)
    let files_to_scan: Vec<PathBuf> = {
        let db = conn.lock().map_err(|e| AppError::from(e.to_string()))?;
        audio_files
            .iter()
            .filter(|path| {
                let path_str = path.to_string_lossy().to_string();
                let track_id = generate_track_id(&path_str);
                let file_mtime = get_file_mtime(path).unwrap_or(0);
                match db::get_track_mtime(&db, &track_id) {
                    Ok(Some(db_mtime)) => {
                        if file_mtime != db_mtime {
                            return true;
                        }
                        // Also rescan if BPM is missing
                        match db::get_track_bpm(&db, &track_id) {
                            Ok(Some(_)) => false,
                            _ => true,
                        }
                    }
                    _ => true,
                }
            })
            .cloned()
            .collect()
    };

    // Phase 3: Extract metadata in parallel
    let processed = std::sync::atomic::AtomicUsize::new(0);
    let app_handle_ref = app_handle.clone();

    let tracks: Vec<Track> = files_to_scan
        .par_iter()
        .filter_map(|path| {
            let result = extract_metadata(path, covers_dir);
            let count = processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if count % 50 == 0 || count == files_to_scan.len() {
                let _ = app_handle_ref.emit(
                    "scan-progress",
                    ScanProgress {
                        current: count,
                        total: files_to_scan.len(),
                        phase: "scanning".to_string(),
                    },
                );
            }
            result
        })
        .collect();

    // Phase 4: Batch insert into DB
    {
        let db = conn.lock().map_err(|e| AppError::from(e.to_string()))?;
        let tx = db.unchecked_transaction().map_err(|e| AppError::from(e.to_string()))?;
        for track in &tracks {
            if let Err(e) = db::upsert_track(&tx, track) {
                eprintln!("Failed to upsert track {}: {}", track.path, e);
            }
        }
        tx.commit().map_err(|e| AppError::from(e.to_string()))?;
    }

    // Phase 5: Remove tracks whose files no longer exist
    {
        let existing_paths: Vec<String> = audio_files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        let db = conn.lock().map_err(|e| AppError::from(e.to_string()))?;
        let _ = db::delete_tracks_not_in_paths(&db, &existing_paths);
    }

    let _ = app_handle.emit(
        "scan-progress",
        ScanProgress {
            current: total,
            total,
            phase: "complete".to_string(),
        },
    );

    Ok(())
}

fn generate_track_id(path: &str) -> String {
    let hash = blake3::hash(path.as_bytes());
    hash.to_hex().to_string()
}

fn get_file_mtime(path: &Path) -> Option<i64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

fn extract_metadata(path: &Path, covers_dir: &Path) -> Option<Track> {
    let path_str = path.to_string_lossy().to_string();
    let track_id = generate_track_id(&path_str);

    let file_meta = std::fs::metadata(path).ok()?;
    let file_size = file_meta.len() as i64;
    let mtime = file_meta
        .modified()
        .ok()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;

    let tagged_file = lofty::read_from_path(path).ok()?;

    let duration = tagged_file.properties().duration().as_secs_f64();

    let tag = tagged_file.primary_tag().or_else(|| tagged_file.first_tag());

    let (title, artist, album, album_artist, genre, year, track_no, disc_no) = if let Some(tag) = tag {
        (
            tag.title()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            tag.artist()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            tag.album()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            tag.get_string(&lofty::tag::ItemKey::AlbumArtist)
                .unwrap_or_default()
                .to_string(),
            tag.genre()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            tag.year().map(|y| y as i32),
            tag.track().map(|t| t as i32),
            tag.disk().map(|d| d as i32),
        )
    } else {
        (String::new(), String::new(), String::new(), String::new(), String::new(), None, None, None)
    };

    // Use filename as title if empty
    let title = if title.is_empty() {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string()
    } else {
        title
    };

    let has_cover = covers::extract_cover(path, &track_id, covers_dir);

    let waveform = crate::waveform::extract_waveform(path);
    let bpm = crate::analyzer::detect_bpm(path);

    Some(Track {
        id: track_id,
        path: path_str,
        title,
        artist,
        album,
        album_artist,
        genre,
        year,
        track_no,
        disc_no,
        duration,
        has_cover,
        file_size,
        mtime,
        waveform,
        bpm,
    })
}
