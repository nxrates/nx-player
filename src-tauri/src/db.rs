use rusqlite::{params, Connection, Result as SqliteResult};
use std::path::Path;
use std::sync::Mutex;

use crate::models::{ArtistSummary, Playlist, Settings, Track};

/// Column list for track queries — single source of truth used by all SELECT track queries.
const TRACK_COLS: &str = "id, path, title, artist, album, album_artist, genre, year, track_no, disc_no, duration, has_cover, file_size, mtime, bpm, beat_grid, downbeats, key, lufs, analysis_version";

pub struct DbState(pub Mutex<Connection>);

/// Shared row mapper: converts a DB row (20-column track query) into a Track struct.
/// Used by get_all_tracks, get_track_by_id, get_tracks_by_artist, get_playlist_tracks.
fn row_to_track(row: &rusqlite::Row) -> SqliteResult<Track> {
    let beat_grid: Option<Vec<f64>> = row.get::<_, Option<Vec<u8>>>(15)?
        .and_then(|b| serde_json::from_slice(&b).ok());
    let downbeats: Option<Vec<f64>> = row.get::<_, Option<Vec<u8>>>(16)?
        .and_then(|b| serde_json::from_slice(&b).ok());
    Ok(Track {
        id: row.get(0)?,
        path: row.get(1)?,
        title: row.get(2)?,
        artist: row.get(3)?,
        album: row.get(4)?,
        album_artist: row.get(5)?,
        genre: row.get(6)?,
        year: row.get(7)?,
        track_no: row.get(8)?,
        disc_no: row.get(9)?,
        duration: row.get(10)?,
        has_cover: row.get::<_, i32>(11)? != 0,
        file_size: row.get(12)?,
        mtime: row.get(13)?,
        waveform: None,
        source: "local".to_string(),
        bpm: row.get(14)?,
        beat_grid,
        downbeats,
        key: row.get(17)?,
        lufs: row.get(18)?,
        analysis_version: row.get(19)?,
    })
}

/// Generic helper: query a single optional value, returning None on no rows.
fn query_optional<T, F>(conn: &Connection, sql: &str, params: &[&dyn rusqlite::types::ToSql], f: F) -> SqliteResult<Option<T>>
where
    F: FnOnce(&rusqlite::Row) -> SqliteResult<T>,
{
    match conn.query_row(sql, params, f) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn initialize(db_path: &Path) -> SqliteResult<Connection> {
    let conn = Connection::open(db_path)?;

    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS tracks (
            id          TEXT PRIMARY KEY,
            path        TEXT NOT NULL UNIQUE,
            title       TEXT NOT NULL DEFAULT '',
            artist      TEXT NOT NULL DEFAULT '',
            album       TEXT NOT NULL DEFAULT '',
            album_artist TEXT NOT NULL DEFAULT '',
            genre       TEXT NOT NULL DEFAULT '',
            year        INTEGER,
            track_no    INTEGER,
            disc_no     INTEGER,
            duration    REAL NOT NULL DEFAULT 0,
            has_cover   INTEGER NOT NULL DEFAULT 0,
            file_size   INTEGER NOT NULL DEFAULT 0,
            mtime       INTEGER NOT NULL DEFAULT 0,
            created_at  INTEGER NOT NULL DEFAULT (unixepoch())
        );

        CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
        CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album);
        CREATE INDEX IF NOT EXISTS idx_tracks_genre ON tracks(genre);
        CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title);
        CREATE INDEX IF NOT EXISTS idx_playlist_tracks_playlist ON playlist_tracks(playlist_id);
        CREATE INDEX IF NOT EXISTS idx_playlist_tracks_track ON playlist_tracks(track_id);

        CREATE TABLE IF NOT EXISTS folders (
            path TEXT PRIMARY KEY
        );

        CREATE TABLE IF NOT EXISTS playlists (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
            updated_at  INTEGER NOT NULL DEFAULT (unixepoch())
        );

        CREATE TABLE IF NOT EXISTS playlist_tracks (
            playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
            track_id    TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
            position    INTEGER NOT NULL,
            PRIMARY KEY (playlist_id, track_id)
        );

        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        ",
    )?;

    // Migration: add columns if they don't exist
    {
        let mut stmt = conn.prepare("PRAGMA table_info(tracks)")?;
        let cols: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .collect();
        let has = |name: &str| cols.iter().any(|c| c == name);

        if !has("waveform") {
            conn.execute_batch("ALTER TABLE tracks ADD COLUMN waveform BLOB;")?;
        }
        if !has("bpm") {
            conn.execute_batch("ALTER TABLE tracks ADD COLUMN bpm REAL;")?;
        }
        if !has("beat_grid") {
            conn.execute_batch("ALTER TABLE tracks ADD COLUMN beat_grid BLOB;")?;
        }
        if !has("downbeats") {
            conn.execute_batch("ALTER TABLE tracks ADD COLUMN downbeats BLOB;")?;
        }
        if !has("key") {
            conn.execute_batch("ALTER TABLE tracks ADD COLUMN key TEXT;")?;
        }
        if !has("analysis_version") {
            conn.execute_batch("ALTER TABLE tracks ADD COLUMN analysis_version INTEGER;")?;
        }
        if !has("lufs") {
            conn.execute_batch("ALTER TABLE tracks ADD COLUMN lufs REAL;")?;
        }
    }

    Ok(conn)
}

// --- Track queries ---

pub fn upsert_track(conn: &Connection, track: &Track) -> SqliteResult<()> {
    // Serialize beat_grid and downbeats as JSON blobs
    let beat_grid_blob: Option<Vec<u8>> = track
        .beat_grid
        .as_ref()
        .map(|bg| serde_json::to_vec(bg).unwrap_or_default());
    let downbeats_blob: Option<Vec<u8>> = track
        .downbeats
        .as_ref()
        .map(|db| serde_json::to_vec(db).unwrap_or_default());

    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, album_artist, genre, year, track_no, disc_no, duration, has_cover, file_size, mtime, waveform, bpm, beat_grid, downbeats, key, lufs, analysis_version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
         ON CONFLICT(id) DO UPDATE SET
            path=excluded.path, title=excluded.title, artist=excluded.artist,
            album=excluded.album, album_artist=excluded.album_artist, genre=excluded.genre,
            year=excluded.year, track_no=excluded.track_no, disc_no=excluded.disc_no,
            duration=excluded.duration, has_cover=excluded.has_cover,
            file_size=excluded.file_size, mtime=excluded.mtime,
            waveform=excluded.waveform, bpm=excluded.bpm,
            beat_grid=excluded.beat_grid, downbeats=excluded.downbeats,
            key=excluded.key, lufs=excluded.lufs, analysis_version=excluded.analysis_version",
        params![
            track.id,
            track.path,
            track.title,
            track.artist,
            track.album,
            track.album_artist,
            track.genre,
            track.year,
            track.track_no,
            track.disc_no,
            track.duration,
            track.has_cover as i32,
            track.file_size,
            track.mtime,
            track.waveform,
            track.bpm,
            beat_grid_blob,
            downbeats_blob,
            track.key,
            track.lufs,
            track.analysis_version,
        ],
    )?;
    Ok(())
}

pub fn get_all_tracks(
    conn: &Connection,
    sort_by: &str,
    sort_order: &str,
    search: Option<&str>,
) -> SqliteResult<Vec<Track>> {
    let sort_col = match sort_by {
        "title" => "title",
        "artist" => "artist",
        "album" => "album",
        "duration" => "duration",
        "genre" => "genre",
        "year" => "year",
        _ => "title",
    };
    let order = if sort_order == "desc" { "DESC" } else { "ASC" };

    let (where_clause, search_param) = if let Some(q) = search {
        if q.is_empty() {
            ("".to_string(), None)
        } else {
            let pattern = format!("%{}%", q);
            (
                " WHERE title LIKE ?1 OR artist LIKE ?1 OR album LIKE ?1 OR genre LIKE ?1".to_string(),
                Some(pattern),
            )
        }
    } else {
        ("".to_string(), None)
    };

    let sql = format!(
        "SELECT {} FROM tracks{} ORDER BY {} COLLATE NOCASE {}",
        TRACK_COLS, where_clause, sort_col, order
    );

    let mut stmt = conn.prepare(&sql)?;

    let tracks = if let Some(ref pattern) = search_param {
        stmt.query_map(params![pattern], row_to_track)?
            .filter_map(|r| r.ok())
            .collect()
    } else {
        stmt.query_map([], row_to_track)?
            .filter_map(|r| r.ok())
            .collect()
    };

    Ok(tracks)
}

pub fn get_track_by_id(conn: &Connection, id: &str) -> SqliteResult<Track> {
    let sql = format!("SELECT {} FROM tracks WHERE id = ?1", TRACK_COLS);
    conn.query_row(&sql, params![id], row_to_track)
}

pub fn get_artists(conn: &Connection) -> SqliteResult<Vec<ArtistSummary>> {
    let mut stmt = conn.prepare(
        "SELECT artist, COUNT(*) as track_count, COUNT(DISTINCT album) as album_count
         FROM tracks WHERE artist != '' GROUP BY artist ORDER BY artist COLLATE NOCASE ASC",
    )?;
    let artists = stmt
        .query_map([], |row| {
            Ok(ArtistSummary {
                name: row.get(0)?,
                track_count: row.get(1)?,
                album_count: row.get(2)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(artists)
}

pub fn get_tracks_by_artist(conn: &Connection, artist: &str) -> SqliteResult<Vec<Track>> {
    let sql = format!(
        "SELECT {} FROM tracks WHERE artist = ?1 ORDER BY album COLLATE NOCASE ASC, disc_no ASC, track_no ASC",
        TRACK_COLS
    );
    let mut stmt = conn.prepare(&sql)?;
    let tracks = stmt
        .query_map(params![artist], row_to_track)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(tracks)
}

pub fn delete_all_tracks(conn: &Connection) -> SqliteResult<()> {
    conn.execute("DELETE FROM tracks", [])?;
    Ok(())
}

pub fn delete_tracks_not_in_paths(conn: &Connection, existing_paths: &[String]) -> SqliteResult<usize> {
    if existing_paths.is_empty() {
        let count = conn.execute("DELETE FROM tracks", [])?;
        return Ok(count);
    }
    // Use HashSet for O(1) lookup instead of Vec::contains O(n)
    let path_set: std::collections::HashSet<&str> = existing_paths.iter().map(|s| s.as_str()).collect();

    let mut stmt = conn.prepare("SELECT id, path FROM tracks")?;
    let to_delete: Vec<String> = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .filter_map(|r| r.ok())
        .filter(|(_, path)| !path_set.contains(path.as_str()))
        .map(|(id, _)| id)
        .collect();

    let mut del_stmt = conn.prepare("DELETE FROM tracks WHERE id = ?1")?;
    for id in &to_delete {
        del_stmt.execute(params![id])?;
    }
    Ok(to_delete.len())
}

pub fn get_track_mtime(conn: &Connection, id: &str) -> SqliteResult<Option<i64>> {
    query_optional(conn, "SELECT mtime FROM tracks WHERE id = ?1", &[&id], |row| row.get(0))
}

pub fn get_track_analysis_version(conn: &Connection, id: &str) -> SqliteResult<Option<i32>> {
    query_optional(conn, "SELECT analysis_version FROM tracks WHERE id = ?1", &[&id], |row| row.get::<_, Option<i32>>(0))
        .map(|opt| opt.flatten())
}

pub fn get_waveform(conn: &Connection, id: &str) -> SqliteResult<Option<Vec<u8>>> {
    query_optional(conn, "SELECT waveform FROM tracks WHERE id = ?1", &[&id], |row| row.get::<_, Option<Vec<u8>>>(0))
        .map(|opt| opt.flatten())
}

// --- Folder queries ---

pub fn add_folder(conn: &Connection, path: &str) -> SqliteResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO folders (path) VALUES (?1)",
        params![path],
    )?;
    Ok(())
}

pub fn remove_folder(conn: &Connection, path: &str) -> SqliteResult<()> {
    conn.execute("DELETE FROM folders WHERE path = ?1", params![path])?;
    Ok(())
}

pub fn get_folders(conn: &Connection) -> SqliteResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT path FROM folders ORDER BY path")?;
    let folders = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(folders)
}

pub fn delete_all_folders(conn: &Connection) -> SqliteResult<()> {
    conn.execute("DELETE FROM folders", [])?;
    Ok(())
}

// --- Playlist queries ---

pub fn create_playlist(conn: &Connection, id: &str, name: &str) -> SqliteResult<Playlist> {
    conn.execute(
        "INSERT INTO playlists (id, name) VALUES (?1, ?2)",
        params![id, name],
    )?;
    conn.query_row(
        "SELECT id, name, created_at, updated_at FROM playlists WHERE id = ?1",
        params![id],
        |row| {
            Ok(Playlist {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        },
    )
}

pub fn rename_playlist(conn: &Connection, id: &str, name: &str) -> SqliteResult<()> {
    conn.execute(
        "UPDATE playlists SET name = ?2, updated_at = unixepoch() WHERE id = ?1",
        params![id, name],
    )?;
    Ok(())
}

pub fn delete_playlist(conn: &Connection, id: &str) -> SqliteResult<()> {
    conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn get_playlists(conn: &Connection) -> SqliteResult<Vec<Playlist>> {
    let mut stmt =
        conn.prepare("SELECT id, name, created_at, updated_at FROM playlists ORDER BY name COLLATE NOCASE ASC")?;
    let playlists = stmt
        .query_map([], |row| {
            Ok(Playlist {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(playlists)
}

pub fn get_playlist_tracks(conn: &Connection, playlist_id: &str) -> SqliteResult<Vec<Track>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.path, t.title, t.artist, t.album, t.album_artist, t.genre, t.year, \
         t.track_no, t.disc_no, t.duration, t.has_cover, t.file_size, t.mtime, t.bpm, \
         t.beat_grid, t.downbeats, t.key, t.lufs, t.analysis_version \
         FROM tracks t INNER JOIN playlist_tracks pt ON pt.track_id = t.id \
         WHERE pt.playlist_id = ?1 ORDER BY pt.position ASC",
    )?;
    let tracks = stmt
        .query_map(params![playlist_id], row_to_track)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(tracks)
}

pub fn add_tracks_to_playlist(
    conn: &Connection,
    playlist_id: &str,
    track_ids: &[String],
) -> SqliteResult<()> {
    let max_pos: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), -1) FROM playlist_tracks WHERE playlist_id = ?1",
            params![playlist_id],
            |row| row.get(0),
        )
        .unwrap_or(-1);

    let mut pos = max_pos + 1;
    for track_id in track_ids {
        conn.execute(
            "INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
            params![playlist_id, track_id, pos],
        )?;
        pos += 1;
    }
    conn.execute(
        "UPDATE playlists SET updated_at = unixepoch() WHERE id = ?1",
        params![playlist_id],
    )?;
    Ok(())
}

pub fn remove_track_from_playlist(
    conn: &Connection,
    playlist_id: &str,
    track_id: &str,
) -> SqliteResult<()> {
    conn.execute(
        "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2",
        params![playlist_id, track_id],
    )?;
    conn.execute(
        "UPDATE playlists SET updated_at = unixepoch() WHERE id = ?1",
        params![playlist_id],
    )?;
    Ok(())
}

pub fn reorder_playlist_tracks(
    conn: &Connection,
    playlist_id: &str,
    track_ids: &[String],
) -> SqliteResult<()> {
    conn.execute(
        "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
        params![playlist_id],
    )?;
    for (i, track_id) in track_ids.iter().enumerate() {
        conn.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
            params![playlist_id, track_id, i as i64],
        )?;
    }
    conn.execute(
        "UPDATE playlists SET updated_at = unixepoch() WHERE id = ?1",
        params![playlist_id],
    )?;
    Ok(())
}

// --- Settings queries ---

pub fn get_settings(conn: &Connection) -> SqliteResult<Settings> {
    let mut settings = Settings::default();

    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    for (key, value) in rows {
        match key.as_str() {
            "theme" => settings.theme = value,
            "playback_speed" => {
                if let Ok(v) = value.parse::<f64>() {
                    settings.playback_speed = v;
                }
            }
            "default_view" => settings.default_view = value,
            _ => {}
        }
    }

    Ok(settings)
}

pub fn update_settings(conn: &Connection, settings: &Settings) -> SqliteResult<()> {
    let speed_str = settings.playback_speed.to_string();
    let pairs: [(&str, &str); 3] = [
        ("theme", settings.theme.as_str()),
        ("default_view", settings.default_view.as_str()),
        ("playback_speed", &speed_str),
    ];
    for (key, value) in &pairs {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
    }
    Ok(())
}
