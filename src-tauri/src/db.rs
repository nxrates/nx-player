use rusqlite::{params, Connection, Result as SqliteResult};
use std::path::Path;
use std::sync::Mutex;

use crate::models::{ArtistSummary, Playlist, Settings, Track};

pub struct DbState(pub Mutex<Connection>);

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

    // Migration: add waveform and bpm columns if they don't exist
    {
        let mut has_waveform = false;
        let mut has_bpm = false;
        let mut stmt = conn.prepare("PRAGMA table_info(tracks)")?;
        let cols: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .collect();
        for col in &cols {
            if col == "waveform" { has_waveform = true; }
            if col == "bpm" { has_bpm = true; }
        }
        if !has_waveform {
            conn.execute_batch("ALTER TABLE tracks ADD COLUMN waveform BLOB;")?;
        }
        if !has_bpm {
            conn.execute_batch("ALTER TABLE tracks ADD COLUMN bpm REAL;")?;
        }
    }

    Ok(conn)
}

// --- Track queries ---

pub fn upsert_track(conn: &Connection, track: &Track) -> SqliteResult<()> {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, album_artist, genre, year, track_no, disc_no, duration, has_cover, file_size, mtime, waveform, bpm)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
         ON CONFLICT(id) DO UPDATE SET
            path=excluded.path, title=excluded.title, artist=excluded.artist,
            album=excluded.album, album_artist=excluded.album_artist, genre=excluded.genre,
            year=excluded.year, track_no=excluded.track_no, disc_no=excluded.disc_no,
            duration=excluded.duration, has_cover=excluded.has_cover,
            file_size=excluded.file_size, mtime=excluded.mtime,
            waveform=excluded.waveform, bpm=excluded.bpm",
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
        "SELECT id, path, title, artist, album, album_artist, genre, year, track_no, disc_no, duration, has_cover, file_size, mtime, bpm FROM tracks{} ORDER BY {} COLLATE NOCASE {}",
        where_clause, sort_col, order
    );

    let mut stmt = conn.prepare(&sql)?;

    let row_mapper = |row: &rusqlite::Row| -> SqliteResult<Track> {
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
            bpm: row.get(14)?,
        })
    };

    let tracks = if let Some(ref pattern) = search_param {
        stmt.query_map(params![pattern], row_mapper)?
            .filter_map(|r| r.ok())
            .collect()
    } else {
        stmt.query_map([], row_mapper)?
            .filter_map(|r| r.ok())
            .collect()
    };

    Ok(tracks)
}

pub fn get_track_by_id(conn: &Connection, id: &str) -> SqliteResult<Track> {
    conn.query_row(
        "SELECT id, path, title, artist, album, album_artist, genre, year, track_no, disc_no, duration, has_cover, file_size, mtime, bpm FROM tracks WHERE id = ?1",
        params![id],
        |row| {
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
                bpm: row.get(14)?,
            })
        },
    )
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
    let mut stmt = conn.prepare(
        "SELECT id, path, title, artist, album, album_artist, genre, year, track_no, disc_no, duration, has_cover, file_size, mtime, bpm
         FROM tracks WHERE artist = ?1 ORDER BY album COLLATE NOCASE ASC, disc_no ASC, track_no ASC",
    )?;
    let tracks = stmt
        .query_map(params![artist], |row| {
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
                bpm: row.get(14)?,
            })
        })?
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
    // Get all track paths from DB and delete ones not in the existing set
    let mut stmt = conn.prepare("SELECT id, path FROM tracks")?;
    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut removed = 0;
    for (id, path) in &rows {
        if !existing_paths.contains(path) {
            conn.execute("DELETE FROM tracks WHERE id = ?1", params![id])?;
            removed += 1;
        }
    }
    Ok(removed)
}

pub fn get_track_mtime(conn: &Connection, id: &str) -> SqliteResult<Option<i64>> {
    let result = conn.query_row(
        "SELECT mtime FROM tracks WHERE id = ?1",
        params![id],
        |row| row.get(0),
    );
    match result {
        Ok(mtime) => Ok(Some(mtime)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn get_track_bpm(conn: &Connection, id: &str) -> SqliteResult<Option<f64>> {
    let result = conn.query_row(
        "SELECT bpm FROM tracks WHERE id = ?1",
        params![id],
        |row| row.get::<_, Option<f64>>(0),
    );
    match result {
        Ok(bpm) => Ok(bpm),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn get_waveform(conn: &Connection, id: &str) -> SqliteResult<Option<Vec<u8>>> {
    let result = conn.query_row(
        "SELECT waveform FROM tracks WHERE id = ?1",
        params![id],
        |row| row.get::<_, Option<Vec<u8>>>(0),
    );
    match result {
        Ok(wf) => Ok(wf),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
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
        "SELECT t.id, t.path, t.title, t.artist, t.album, t.album_artist, t.genre, t.year, t.track_no, t.disc_no, t.duration, t.has_cover, t.file_size, t.mtime, t.bpm
         FROM tracks t
         INNER JOIN playlist_tracks pt ON pt.track_id = t.id
         WHERE pt.playlist_id = ?1
         ORDER BY pt.position ASC",
    )?;
    let tracks = stmt
        .query_map(params![playlist_id], |row| {
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
                bpm: row.get(14)?,
            })
        })?
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
    let pairs = [
        ("theme", settings.theme.as_str()),
        ("default_view", settings.default_view.as_str()),
    ];
    for (key, value) in &pairs {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
    }
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params!["playback_speed", settings.playback_speed.to_string()],
    )?;
    Ok(())
}
