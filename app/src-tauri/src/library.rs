//! Turns finished downloads into games and works out which file launches them.

use crate::{
    db::Db,
    installer::{scan, Source},
    metadata::Metadata,
};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Emitter, Manager};

const MAX_DEPTH: usize = 4;
const NOT_GAMES: &[&str] = &[
    "unins", "setup", "install", "redist", "vcredist", "vc_", "dxsetup", "directx", "dotnet", "crash",
    "report", "helper", "updater", "launcherpatcher", "config", "benchmark", "ue4prereq", "easyanticheat",
    "battleye", "cefprocess", "quicksfv",
];

const GENERIC_FILES: &[&str] = &["setup", "install", "installer", "game", "launcher", "autorun"];

#[derive(Debug)]
struct Candidate {
    path: PathBuf,
    score: u64,
}

fn walk(folder: &Path, depth: usize, found: &mut Vec<(PathBuf, usize, u64)>) {
    let Ok(entries) = fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            if depth < MAX_DEPTH {
                walk(&path, depth + 1, found);
            }
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
            found.push((path, depth, meta.len()));
        }
    }
}

fn letters(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Best guess first. Files named like the game win, then shallow ones, then big ones.
fn rank(found: Vec<(PathBuf, usize, u64)>, game_name: &str) -> Vec<Candidate> {
    let wanted = letters(game_name);
    let mut candidates: Vec<Candidate> = found
        .into_iter()
        .filter_map(|(path, depth, size)| {
            let stem = letters(&path.file_stem()?.to_string_lossy());
            let named = !stem.is_empty() && !wanted.is_empty() && (wanted.contains(&stem) || stem.contains(&wanted));
            if !named && NOT_GAMES.iter().any(|word| stem.contains(&letters(word))) {
                return None;
            }
            let score = u64::from(named) * 1_000_000 + (MAX_DEPTH - depth.min(MAX_DEPTH)) as u64 * 1_000 + (size >> 20).min(999);
            Some(Candidate { path, score })
        })
        .collect();
    candidates.sort_by(|a, b| b.score.cmp(&a.score));
    candidates
}

pub fn find_exes(folder: &str, game_name: &str) -> Vec<String> {
    let mut found = Vec::new();
    walk(Path::new(folder), 0, &mut found);
    rank(found, game_name).into_iter().map(|c| c.path.to_string_lossy().into_owned()).collect()
}

/// Only picks a file by itself when there's one candidate, or one is named after the game.
pub fn confident_exe(folder: &str, game_name: &str) -> Option<String> {
    let mut found = Vec::new();
    walk(Path::new(folder), 0, &mut found);
    let candidates = rank(found, game_name);
    let best = candidates.first()?;
    (candidates.len() == 1 || best.score >= 1_000_000).then(|| best.path.to_string_lossy().into_owned())
}

pub fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else { return 0 };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

pub fn changed(app: &AppHandle) {
    let _ = app.emit("library-changed", ());
    app.state::<Metadata>().wake();
}

/// A game file is ready to play. An installer, archive or disc image waits for Install.
pub fn add_file(db: &Db, file: &str) -> rusqlite::Result<i64> {
    let path = Path::new(file);
    let folder = path.parent().map_or(String::new(), |p| p.to_string_lossy().into_owned());
    let stem = path.file_stem().map_or("New game".into(), |s| s.to_string_lossy().into_owned());
    // "setup.exe" says nothing about the game, but its folder usually does.
    let generic = GENERIC_FILES.contains(&stem.to_lowercase().as_str());
    let name = path.parent().and_then(Path::file_name).filter(|_| generic).map_or(stem.clone(), |n| n.to_string_lossy().into_owned());
    let id = db.add(None, &name, &folder, None)?;
    match scan(path, &name, &HashSet::new()) {
        Source::Play(exe) => db.set_exe(id, Some(&exe.to_string_lossy()))?,
        Source::Unknown => {}
        _ => db.mark_installable(id, file)?,
    }
    Ok(id)
}

fn adopt(db: &Db, id: &str, name: &str, save_path: &str) -> rusqlite::Result<bool> {
    if db.has_torrent(id)? {
        return Ok(false);
    }
    // A single-file download has no folder of its own, and the shared one isn't this game's.
    let own = Path::new(save_path).join(name);
    let folder = if own.is_dir() { own.to_string_lossy().into_owned() } else { save_path.to_string() };
    let game = db.add(Some(id), name, &folder, None)?;
    match scan(&own, name, &HashSet::new()) {
        Source::Play(exe) => db.set_exe(game, Some(&exe.to_string_lossy()))?,
        Source::Unknown => {}
        _ => db.mark_installable(game, &own.to_string_lossy())?,
    }
    Ok(true)
}

fn restore(db: &Db, id: &str) -> rusqlite::Result<bool> {
    let Some(game_id) = db.restore_torrent(id)? else { return Ok(false) };
    if let Some(game) = db.game(game_id)? {
        db.set_exe(game_id, confident_exe(&game.install_dir, &game.name).as_deref())?;
    }
    Ok(true)
}

/// A game can finish while no one is listening, so seeding reports count as well.
pub fn on_engine_event(app: &AppHandle, event: &Value) {
    let db = app.state::<Db>();
    let id = event["id"].as_str().unwrap_or_default();
    let result = match event["event"].as_str() {
        Some("finished") => restore(&db, id),
        Some("progress") if matches!(event["state"].as_str(), Some("seeding" | "finished")) => adopt(
            &db,
            id,
            event["name"].as_str().unwrap_or_default(),
            event["save_path"].as_str().unwrap_or_default(),
        ),
        _ => Ok(false),
    };
    if matches!(result, Ok(true)) {
        changed(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(found: Vec<(&str, usize, u64)>, game: &str) -> Vec<String> {
        let found = found.into_iter().map(|(p, d, s)| (PathBuf::from(p), d, s)).collect();
        rank(found, game).into_iter().map(|c| c.path.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn skips_installers_and_helpers() {
        let found = vec![("unins000.exe", 0, 1 << 21), ("vcredist_x64.exe", 1, 1 << 24), ("CrashReporter.exe", 1, 1 << 22), ("Game.exe", 2, 1 << 26)];
        assert_eq!(names(found, "Something Else"), ["Game.exe"]);
    }

    #[test]
    fn prefers_the_game_name_over_size() {
        let found = vec![("Huge.exe", 0, 1 << 32), ("bin/HollowKnight.exe", 2, 1 << 20)];
        assert_eq!(names(found, "Hollow Knight")[0], "bin/HollowKnight.exe");
    }

    #[test]
    fn keeps_a_game_named_like_a_helper() {
        let found = vec![("TheInstallation.exe", 0, 1 << 20), ("unins000.exe", 0, 1 << 20)];
        assert_eq!(names(found, "The Installation"), ["TheInstallation.exe"]);
    }

    #[test]
    fn prefers_shallow_then_big() {
        let found = vec![("tools/editor.exe", 1, 1 << 30), ("run.exe", 0, 1 << 20)];
        assert_eq!(names(found, "Unrelated")[0], "run.exe");
    }
}
