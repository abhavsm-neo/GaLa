//! The library: games and play sessions, in one SQLite file.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS games (
    id INTEGER PRIMARY KEY,
    torrent_id TEXT UNIQUE,
    name TEXT NOT NULL,
    install_dir TEXT NOT NULL,
    exe_path TEXT,
    args TEXT NOT NULL DEFAULT '',
    favorite INTEGER NOT NULL DEFAULT 0,
    progress TEXT NOT NULL DEFAULT 'Backlog',
    rating INTEGER NOT NULL DEFAULT 0,
    notes TEXT NOT NULL DEFAULT '',
    added_at INTEGER NOT NULL,
    hidden INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY,
    game_id INTEGER NOT NULL REFERENCES games(id),
    started_at INTEGER NOT NULL,
    seconds INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS sessions_game ON sessions(game_id);
";

const MIGRATIONS: &[&str] = &["
ALTER TABLE games ADD COLUMN source_path TEXT;
ALTER TABLE games ADD COLUMN install_state TEXT NOT NULL DEFAULT '';
ALTER TABLE games ADD COLUMN install_error TEXT NOT NULL DEFAULT '';
", "
ALTER TABLE games ADD COLUMN meta_state TEXT NOT NULL DEFAULT '';
ALTER TABLE games ADD COLUMN igdb_id INTEGER;
ALTER TABLE games ADD COLUMN summary TEXT NOT NULL DEFAULT '';
ALTER TABLE games ADD COLUMN developer TEXT NOT NULL DEFAULT '';
ALTER TABLE games ADD COLUMN publisher TEXT NOT NULL DEFAULT '';
ALTER TABLE games ADD COLUMN release_date INTEGER;
ALTER TABLE games ADD COLUMN igdb_rating REAL;
ALTER TABLE games ADD COLUMN genres TEXT NOT NULL DEFAULT '[]';
ALTER TABLE games ADD COLUMN modes TEXT NOT NULL DEFAULT '[]';
ALTER TABLE games ADD COLUMN categories TEXT NOT NULL DEFAULT '[]';
ALTER TABLE games ADD COLUMN cover TEXT;
ALTER TABLE games ADD COLUMN hero TEXT;
ALTER TABLE games ADD COLUMN logo TEXT;
ALTER TABLE games ADD COLUMN screenshots TEXT NOT NULL DEFAULT '[]';
", "
ALTER TABLE games ADD COLUMN save_folder TEXT;
", "
ALTER TABLE games ADD COLUMN launcher TEXT NOT NULL DEFAULT '';
ALTER TABLE games ADD COLUMN launch_uri TEXT;
"];

const TWO_WEEKS: i64 = 14 * 24 * 60 * 60;

const SELECT_GAMES: &str = "
SELECT g.id, g.torrent_id, g.name, g.install_dir, g.exe_path, g.args, g.favorite, g.progress,
       g.rating, g.notes, g.added_at, COALESCE(SUM(s.seconds), 0),
       COALESCE(SUM(CASE WHEN s.started_at >= ?1 THEN s.seconds END), 0), MAX(s.started_at),
       g.source_path, g.install_state, g.install_error, g.meta_state, g.igdb_id, g.summary, g.developer,
       g.publisher, g.release_date, g.igdb_rating, g.genres, g.modes, g.categories, g.cover, g.hero, g.logo,
       g.screenshots, g.save_folder, g.launcher, g.launch_uri
FROM games g LEFT JOIN sessions s ON s.game_id = g.id
WHERE g.hidden = 0";

#[derive(Clone, Serialize)]
pub struct Game {
    pub id: i64,
    pub torrent_id: Option<String>,
    pub name: String,
    pub install_dir: String,
    pub exe_path: Option<String>,
    pub args: String,
    pub favorite: bool,
    pub progress: String,
    pub rating: i64,
    pub notes: String,
    pub added_at: i64,
    pub total_seconds: i64,
    pub recent_seconds: i64,
    pub last_played: Option<i64>,
    pub source_path: Option<String>,
    pub install_state: String,
    pub install_error: String,
    pub meta_state: String,
    pub igdb_id: Option<i64>,
    pub summary: String,
    pub developer: String,
    pub publisher: String,
    pub release_date: Option<i64>,
    pub igdb_rating: Option<f64>,
    pub genres: Vec<String>,
    pub modes: Vec<String>,
    pub categories: Vec<String>,
    pub cover: Option<String>,
    pub hero: Option<String>,
    pub logo: Option<String>,
    pub screenshots: Vec<String>,
    pub save_folder: Option<String>,
    pub launcher: String,
    pub launch_uri: Option<String>,
}

/// What IGDB and SteamGridDB say about a game.
pub struct Details {
    pub igdb_id: i64,
    pub name: String,
    pub summary: String,
    pub developer: String,
    pub publisher: String,
    pub release_date: Option<i64>,
    pub igdb_rating: Option<f64>,
    pub genres: Vec<String>,
    pub modes: Vec<String>,
    pub cover: Option<String>,
    pub hero: Option<String>,
    pub logo: Option<String>,
    pub screenshots: Vec<String>,
}

#[derive(Deserialize)]
pub struct GamePatch {
    pub name: Option<String>,
    pub args: Option<String>,
    pub favorite: Option<bool>,
    pub progress: Option<String>,
    pub rating: Option<i64>,
    pub notes: Option<String>,
    pub categories: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct DayPlaytime {
    pub day: String,
    pub seconds: i64,
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

fn list(row: &Row, index: usize) -> rusqlite::Result<Vec<String>> {
    let text: String = row.get(index)?;
    Ok(serde_json::from_str(&text).unwrap_or_default())
}

fn to_json(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".into())
}

fn to_game(row: &Row) -> rusqlite::Result<Game> {
    Ok(Game {
        id: row.get(0)?,
        torrent_id: row.get(1)?,
        name: row.get(2)?,
        install_dir: row.get(3)?,
        exe_path: row.get(4)?,
        args: row.get(5)?,
        favorite: row.get(6)?,
        progress: row.get(7)?,
        rating: row.get(8)?,
        notes: row.get(9)?,
        added_at: row.get(10)?,
        total_seconds: row.get(11)?,
        recent_seconds: row.get(12)?,
        last_played: row.get(13)?,
        source_path: row.get(14)?,
        install_state: row.get(15)?,
        install_error: row.get(16)?,
        meta_state: row.get(17)?,
        igdb_id: row.get(18)?,
        summary: row.get(19)?,
        developer: row.get(20)?,
        publisher: row.get(21)?,
        release_date: row.get(22)?,
        igdb_rating: row.get(23)?,
        genres: list(row, 24)?,
        modes: list(row, 25)?,
        categories: list(row, 26)?,
        cover: row.get(27)?,
        hero: row.get(28)?,
        logo: row.get(29)?,
        screenshots: list(row, 30)?,
        save_folder: row.get(31)?,
        launcher: row.get(32)?,
        launch_uri: row.get(33)?,
    })
}

pub struct Db(Mutex<Connection>);

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.execute_batch(SCHEMA)?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        for (index, migration) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            connection.execute_batch(migration)?;
            connection.pragma_update(None, "user_version", index as i64 + 1)?;
        }
        // An install that was running when GaLa closed can't still be running now.
        connection.execute(
            "UPDATE games SET install_state = 'failed', install_error = 'GaLa closed during the install'
             WHERE install_state IN ('queued', 'extracting', 'installing', 'detecting')",
            [],
        )?;
        Ok(Self(Mutex::new(connection)))
    }

    pub fn mark_installable(&self, id: i64, source: &str) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute(
            "UPDATE games SET source_path = ?2, install_state = 'pending' WHERE id = ?1",
            params![id, source],
        ).map(|_| ())
    }

    pub fn set_install(&self, id: i64, state: &str, error: &str) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute(
            "UPDATE games SET install_state = ?2, install_error = ?3 WHERE id = ?1",
            params![id, state, error],
        ).map(|_| ())
    }

    pub fn finish_install(&self, id: i64, dir: &str, exe: Option<&str>) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute(
            "UPDATE games SET install_dir = ?2, exe_path = ?3, install_state = '', install_error = '' WHERE id = ?1",
            params![id, dir, exe],
        ).map(|_| ())
    }

    pub fn set_launcher(&self, id: i64, launcher: &str, uri: &str) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute(
            "UPDATE games SET launcher = ?2, launch_uri = ?3 WHERE id = ?1",
            params![id, launcher, uri],
        ).map(|_| ())
    }

    pub fn finish_move(&self, id: i64, dir: &str, exe: Option<&str>) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute(
            "UPDATE games SET install_dir = ?2, exe_path = ?3 WHERE id = ?1",
            params![id, dir, exe],
        ).map(|_| ())
    }

    pub fn set_save_folder(&self, id: i64, folder: Option<&str>) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute("UPDATE games SET save_folder = ?2 WHERE id = ?1", params![id, folder]).map(|_| ())
    }

    pub fn next_unidentified(&self) -> rusqlite::Result<Option<(i64, String)>> {
        self.0.lock().unwrap()
            .query_row(
                "SELECT id, name FROM games WHERE meta_state = '' AND hidden = 0 ORDER BY id LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
    }

    pub fn set_meta_state(&self, id: i64, state: &str) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute("UPDATE games SET meta_state = ?2 WHERE id = ?1", params![id, state]).map(|_| ())
    }

    pub fn set_details(&self, id: i64, details: &Details) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute(
            "UPDATE games SET meta_state = 'matched', igdb_id = ?2, name = ?3, summary = ?4, developer = ?5,
             publisher = ?6, release_date = ?7, igdb_rating = ?8, genres = ?9, modes = ?10, cover = ?11,
             hero = ?12, logo = ?13, screenshots = ?14 WHERE id = ?1",
            params![
                id,
                details.igdb_id,
                details.name,
                details.summary,
                details.developer,
                details.publisher,
                details.release_date,
                details.igdb_rating,
                to_json(&details.genres),
                to_json(&details.modes),
                details.cover,
                details.hero,
                details.logo,
                to_json(&details.screenshots),
            ],
        ).map(|_| ())
    }

    pub fn games(&self) -> rusqlite::Result<Vec<Game>> {
        let connection = self.0.lock().unwrap();
        let mut query = connection.prepare(&format!("{SELECT_GAMES} GROUP BY g.id ORDER BY g.name COLLATE NOCASE"))?;
        let games = query.query_map([now() - TWO_WEEKS], to_game)?.collect();
        games
    }

    pub fn game(&self, id: i64) -> rusqlite::Result<Option<Game>> {
        let connection = self.0.lock().unwrap();
        connection
            .query_row(&format!("{SELECT_GAMES} AND g.id = ?2 GROUP BY g.id"), params![now() - TWO_WEEKS, id], to_game)
            .optional()
    }

    pub fn add(&self, torrent_id: Option<&str>, name: &str, install_dir: &str, exe: Option<&str>) -> rusqlite::Result<i64> {
        let connection = self.0.lock().unwrap();
        connection.execute(
            "INSERT INTO games (torrent_id, name, install_dir, exe_path, added_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![torrent_id, name, install_dir, exe, now()],
        )?;
        Ok(connection.last_insert_rowid())
    }

    pub fn has_torrent(&self, torrent_id: &str) -> rusqlite::Result<bool> {
        let connection = self.0.lock().unwrap();
        let found = connection
            .query_row("SELECT 1 FROM games WHERE torrent_id = ?1", [torrent_id], |_| Ok(()))
            .optional()?;
        Ok(found.is_some())
    }

    /// Brings back a game whose files were deleted and have now been downloaded again.
    pub fn restore_torrent(&self, torrent_id: &str) -> rusqlite::Result<Option<i64>> {
        let connection = self.0.lock().unwrap();
        connection
            .query_row(
                "UPDATE games SET hidden = 0 WHERE torrent_id = ?1 AND hidden = 1 RETURNING id",
                [torrent_id],
                |row| row.get(0),
            )
            .optional()
    }

    /// Hiding keeps the playtime in case the game comes back.
    pub fn retry_unmatched(&self) -> rusqlite::Result<usize> {
        self.0.lock().unwrap().execute("UPDATE games SET meta_state = '' WHERE meta_state = 'unmatched'", [])
    }

    pub fn hide(&self, id: i64) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute("UPDATE games SET hidden = 1 WHERE id = ?1", [id]).map(|_| ())
    }

    pub fn hide_torrent(&self, torrent_id: &str) -> rusqlite::Result<usize> {
        self.0.lock().unwrap().execute(
            "UPDATE games SET hidden = 1, exe_path = NULL WHERE torrent_id = ?1",
            [torrent_id],
        )
    }

    pub fn set_exe(&self, id: i64, exe: Option<&str>) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute("UPDATE games SET exe_path = ?2 WHERE id = ?1", params![id, exe]).map(|_| ())
    }

    pub fn patch(&self, id: i64, patch: &GamePatch) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute(
            "UPDATE games SET name = COALESCE(?2, name), args = COALESCE(?3, args),
             favorite = COALESCE(?4, favorite), progress = COALESCE(?5, progress),
             rating = COALESCE(?6, rating), notes = COALESCE(?7, notes),
             categories = COALESCE(?8, categories) WHERE id = ?1",
            params![
                id,
                patch.name,
                patch.args,
                patch.favorite,
                patch.progress,
                patch.rating,
                patch.notes,
                patch.categories.as_deref().map(to_json),
            ],
        ).map(|_| ())
    }

    pub fn start_session(&self, game_id: i64) -> rusqlite::Result<i64> {
        let connection = self.0.lock().unwrap();
        connection.execute("INSERT INTO sessions (game_id, started_at) VALUES (?1, ?2)", params![game_id, now()])?;
        Ok(connection.last_insert_rowid())
    }

    pub fn record_session(&self, session: i64, seconds: i64) -> rusqlite::Result<()> {
        self.0.lock().unwrap().execute("UPDATE sessions SET seconds = ?2 WHERE id = ?1", params![session, seconds]).map(|_| ())
    }

    pub fn playtime_by_day(&self, game_id: Option<i64>, days: i64) -> rusqlite::Result<Vec<DayPlaytime>> {
        let connection = self.0.lock().unwrap();
        let mut query = connection.prepare(
            "SELECT date(s.started_at, 'unixepoch', 'localtime'), SUM(s.seconds) FROM sessions s
             WHERE s.started_at >= ?1 AND (?2 IS NULL OR s.game_id = ?2) GROUP BY 1",
        )?;
        let days = query
            .query_map(params![now() - days * 24 * 60 * 60, game_id], |row| {
                Ok(DayPlaytime { day: row.get(0)?, seconds: row.get(1)? })
            })?
            .collect();
        days
    }
}
