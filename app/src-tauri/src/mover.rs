//! Moves an installed game to another folder or drive. The old copy stays until the new
//! one is complete, so a failure or a closed app never leaves a game half gone.

use crate::{
    db::{Db, Game},
    engine::Engine,
    installer::{folder_name, free_space},
    library,
    player::Player,
    settings::Store,
};
use serde::Serialize;
use serde_json::json;
use std::{
    fs,
    iter::once,
    os::windows::ffi::OsStrExt,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
use windows_sys::Win32::UI::Shell::{
    SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_SILENT, FOF_WANTNUKEWARNING, FO_DELETE, SHFILEOPSTRUCTW,
};

const CHUNK: usize = 4 << 20;
const REPORT_EVERY: Duration = Duration::from_millis(250);
const TORRENT_RELEASE: Duration = Duration::from_secs(2);
const SPARE_BYTES: u64 = 1 << 30;
const CANCELLED: &str = "Move cancelled. The game is still in its old folder";

#[derive(Clone, Serialize)]
pub struct MoveProgress {
    pub game_id: i64,
    pub done: u64,
    pub total: u64,
}

#[derive(Default)]
pub struct Mover {
    current: Mutex<Option<MoveProgress>>,
    cancelled: AtomicBool,
}

fn same(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().trim_end_matches('\\').eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches('\\'))
}

fn inside(path: &Path, folder: &Path) -> bool {
    path.ancestors().skip(1).any(|parent| same(parent, folder))
}

fn drive(path: &Path) -> String {
    match path.components().next() {
        Some(Component::Prefix(prefix)) => prefix.as_os_str().to_string_lossy().to_lowercase(),
        _ => String::new(),
    }
}

/// Guards against moving a folder the game merely sits in, such as Downloads.
fn refusal(folder: &Path, exe: Option<&Path>, shared: &[PathBuf], other_games: &[PathBuf], home: &Path) -> Option<&'static str> {
    if !exe.is_some_and(|exe| inside(exe, folder)) {
        return Some("The game's file isn't inside its folder, so GaLa can't tell what to move");
    }
    if folder.parent().is_none() || same(folder, home) || folder.parent().is_some_and(|parent| same(parent, home)) {
        return Some("That folder holds more than this game, so GaLa won't move it");
    }
    if shared.iter().any(|path| same(path, folder) || inside(path, folder)) {
        return Some("That folder holds more than this game, so GaLa won't move it");
    }
    if other_games.iter().any(|path| same(path, folder) || inside(path, folder) || inside(folder, path)) {
        return Some("Another game shares this folder, so GaLa won't move it");
    }
    None
}

fn list_files(folder: &Path, files: &mut Vec<(PathBuf, u64)>, dirs: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(folder).map_err(|e| format!("{}: {e}", folder.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        if meta.is_dir() {
            dirs.push(entry.path());
            list_files(&entry.path(), files, dirs)?;
        } else {
            files.push((entry.path(), meta.len()));
        }
    }
    Ok(())
}

/// Copies a file or folder. `progress` gets each batch of bytes and returns false to stop.
pub fn copy_tree(from: &Path, to: &Path, progress: &mut dyn FnMut(u64) -> bool) -> Result<(), String> {
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    if from.is_dir() {
        list_files(from, &mut files, &mut dirs)?;
        fs::create_dir_all(to).map_err(|e| e.to_string())?;
    } else {
        files.push((from.to_path_buf(), 0));
    }
    for dir in dirs {
        let relative = dir.strip_prefix(from).map_err(|e| e.to_string())?;
        fs::create_dir_all(to.join(relative)).map_err(|e| e.to_string())?;
    }

    let mut buffer = vec![0u8; CHUNK];
    for (file, _) in files {
        let target = match file.strip_prefix(from) {
            Ok(relative) if !relative.as_os_str().is_empty() => to.join(relative),
            _ => to.to_path_buf(),
        };
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut input = fs::File::open(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let mut output = fs::File::create(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        loop {
            let count = input.read(&mut buffer).map_err(|e| format!("{}: {e}", file.display()))?;
            if count == 0 {
                break;
            }
            output.write_all(&buffer[..count]).map_err(|e| format!("{}: {e}", target.display()))?;
            if !progress(count as u64) {
                return Err(CANCELLED.into());
            }
        }
    }
    Ok(())
}

fn emit(app: &AppHandle, progress: Option<&MoveProgress>) {
    *app.state::<Mover>().current.lock().unwrap() = progress.cloned();
    let _ = app.emit("move-progress", progress);
}

fn check(app: &AppHandle, game: &Game, dest: &Path) -> Result<(), String> {
    if !game.install_state.is_empty() {
        return Err("Finish installing this game first".into());
    }
    if app.state::<Player>().running().is_some_and(|running| running.game_id == game.id) {
        return Err("Close the game before moving it".into());
    }
    let folder = Path::new(&game.install_dir);
    if same(folder, dest) {
        return Err("The game is already there".into());
    }
    if inside(dest, folder) {
        return Err("A game can't move into its own folder".into());
    }
    if dest.exists() {
        return Err(format!("{} already exists", dest.display()));
    }
    own_folder(app, game)
}

fn own_folder(app: &AppHandle, game: &Game) -> Result<(), String> {
    let folder = Path::new(&game.install_dir);
    let settings = app.state::<Store>().get();
    let shared = [PathBuf::from(settings.download_dir), PathBuf::from(settings.install_dir)];
    let others: Vec<PathBuf> = app.state::<Db>().games().map_err(|e| e.to_string())?
        .into_iter()
        .filter(|other| other.id != game.id)
        .map(|other| other.install_dir.into())
        .collect();
    let home = PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default());
    match refusal(folder, game.exe_path.as_deref().map(Path::new), &shared, &others, &home) {
        Some(reason) => Err(reason.into()),
        None => Ok(()),
    }
}

/// Everything is checked again here: the dialog may have been open for a while.
fn run(app: &AppHandle, game: &Game, dest: &Path) -> Result<(), String> {
    check(app, game, dest)?;
    let folder = Path::new(&game.install_dir);
    let mover = app.state::<Mover>();

    if drive(folder) == drive(dest) {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        release_torrent(app, game);
        fs::rename(folder, dest).map_err(|e| e.to_string())?;
    } else {
        let (mut files, mut dirs) = (Vec::new(), Vec::new());
        list_files(folder, &mut files, &mut dirs)?;
        let total: u64 = files.iter().map(|(_, size)| size).sum();
        let free = free_space(dest);
        if total + SPARE_BYTES > free {
            return Err(format!("Needs {} GB free, but this drive has {} GB", (total + SPARE_BYTES) >> 30, free >> 30));
        }
        release_torrent(app, game);

        let partial = dest.with_extension("moving");
        let _ = fs::remove_dir_all(&partial);
        let mut progress = MoveProgress { game_id: game.id, done: 0, total };
        let mut reported = Instant::now();
        let copied = copy_tree(folder, &partial, &mut |bytes| {
            progress.done += bytes;
            if reported.elapsed() >= REPORT_EVERY {
                reported = Instant::now();
                emit(app, Some(&progress));
            }
            !mover.cancelled.load(Ordering::SeqCst)
        })
        .and_then(|()| fs::rename(&partial, dest).map_err(|e| e.to_string()));
        if let Err(error) = copied {
            let _ = fs::remove_dir_all(&partial);
            return Err(error);
        }
    }

    let exe = game.exe_path.as_deref().map(|exe| match Path::new(exe).strip_prefix(folder) {
        Ok(relative) => dest.join(relative).to_string_lossy().into_owned(),
        Err(_) => exe.to_string(),
    });
    app.state::<Db>().finish_move(game.id, &dest.to_string_lossy(), exe.as_deref()).map_err(|e| e.to_string())?;
    if folder.exists() && fs::remove_dir_all(folder).is_err() {
        return Err(format!("{} moved, but its old folder couldn't be deleted: {}", game.name, folder.display()));
    }
    Ok(())
}

/// A game played straight from its download is what the torrent seeds, so seeding ends here.
fn release_torrent(app: &AppHandle, game: &Game) {
    if let (Some(id), None) = (&game.torrent_id, &game.source_path) {
        app.state::<Engine>().send(json!({ "cmd": "remove", "id": id, "delete_files": false }));
        thread::sleep(TORRENT_RELEASE);
    }
}

impl Mover {
    pub fn current(&self) -> Option<MoveProgress> {
        self.current.lock().unwrap().clone()
    }

    pub fn is_moving(&self, game_id: i64) -> bool {
        self.current().is_some_and(|progress| progress.game_id == game_id)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn start(&self, app: &AppHandle, game_id: i64, folder: &str) -> Result<(), String> {
        let game = app.state::<Db>().game(game_id).map_err(|e| e.to_string())?.ok_or("That game is no longer in your library")?;
        let dest = Path::new(folder).join(folder_name(&game.name));
        {
            let mut current = self.current.lock().unwrap();
            if current.is_some() {
                return Err("Another game is moving. Wait for it to finish".into());
            }
            check(app, &game, &dest)?;
            *current = Some(MoveProgress { game_id, done: 0, total: 0 });
        }
        self.cancelled.store(false, Ordering::SeqCst);
        emit(app, self.current().as_ref());

        let app = app.clone();
        thread::spawn(move || {
            let result = run(&app, &game, &dest);
            emit(&app, None);
            if let Err(message) = result {
                let _ = app.emit("engine-event", json!({ "event": "error", "id": "", "message": message }));
            }
            library::changed(&app);
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refuse(folder: &str, exe: &str, shared: &[&str], others: &[&str]) -> bool {
        let shared: Vec<PathBuf> = shared.iter().map(PathBuf::from).collect();
        let others: Vec<PathBuf> = others.iter().map(PathBuf::from).collect();
        refusal(Path::new(folder), Some(Path::new(exe)), &shared, &others, Path::new(r"C:\Users\me")).is_some()
    }

    #[test]
    fn allows_a_game_in_its_own_folder() {
        assert!(!refuse(r"E:\GaLa\Games\Hades II", r"E:\GaLa\Games\Hades II\bin\Hades.exe", &[r"E:\GaLa\Games"], &[r"E:\GaLa\Games\Other"]));
    }

    #[test]
    fn refuses_shared_folders() {
        assert!(refuse(r"E:\GaLa\Downloads", r"E:\GaLa\Downloads\a.exe", &[r"E:\GaLa\Downloads"], &[]));
        assert!(refuse(r"E:\GaLa", r"E:\GaLa\Games\a.exe", &[r"E:\GaLa\Games"], &[]));
        assert!(refuse(r"E:\", r"E:\a.exe", &[], &[]));
        assert!(refuse(r"C:\Users\me", r"C:\Users\me\a.exe", &[], &[]));
        assert!(refuse(r"C:\Users\me\Desktop", r"C:\Users\me\Desktop\a.exe", &[], &[]));
    }

    #[test]
    fn refuses_a_folder_shared_with_another_game() {
        assert!(refuse(r"E:\Games", r"E:\Games\a.exe", &[], &[r"E:\Games"]));
        assert!(refuse(r"E:\Games", r"E:\Games\a.exe", &[], &[r"E:\Games\Other"]));
    }

    #[test]
    fn refuses_a_game_file_outside_the_folder() {
        assert!(refuse(r"E:\Games\A", r"E:\Other\a.exe", &[], &[]));
    }

    #[test]
    fn compares_paths_without_case() {
        assert!(same(Path::new(r"E:\Games\A"), Path::new(r"e:\games\a\")));
        assert_eq!(drive(Path::new(r"E:\Games")), drive(Path::new(r"e:\Other")));
    }

    #[test]
    fn copies_a_tree() {
        let root = std::env::temp_dir().join(format!("gala-copy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("from/sub/empty")).unwrap();
        fs::write(root.join("from/sub/save.dat"), b"hello").unwrap();
        let mut copied = 0;
        copy_tree(&root.join("from"), &root.join("to"), &mut |bytes| { copied += bytes; true }).unwrap();
        assert_eq!(fs::read(root.join("to/sub/save.dat")).unwrap(), b"hello");
        assert!(root.join("to/sub/empty").is_dir());
        assert_eq!(copied, 5);
        copy_tree(&root.join("from/sub/save.dat"), &root.join("one/save.dat"), &mut |_| true).unwrap();
        assert!(root.join("one/save.dat").is_file());
        fs::remove_dir_all(&root).unwrap();
    }
}

fn recycle(folder: &Path) -> Result<(), String> {
    // The list of paths ends with an extra null.
    let from: Vec<u16> = folder.as_os_str().encode_wide().chain(once(0)).chain(once(0)).collect();
    let mut operation = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_WANTNUKEWARNING) as _,
        ..unsafe { std::mem::zeroed() }
    };
    let code = unsafe { SHFileOperationW(&mut operation) };
    if code != 0 {
        return Err(format!("Windows couldn't delete {} (error {code})", folder.display()));
    }
    if operation.fAnyOperationsAborted != 0 {
        return Err("Deleting was cancelled, so the game's files are still there".into());
    }
    Ok(())
}

/// Sends a game's folder to the Recycle Bin, with the same guards as a move.
pub fn recycle_game(app: &AppHandle, game_id: i64) -> Result<(), String> {
    let game = app.state::<Db>().game(game_id).map_err(|e| e.to_string())?.ok_or("That game is no longer in your library")?;
    if !game.launcher.is_empty() {
        return Err(format!("Uninstall {} from {} to delete its files", game.name, game.launcher));
    }
    if !game.install_state.is_empty() {
        return Err("This game isn't installed yet, so there are no game files to delete".into());
    }
    if app.state::<Player>().running().is_some_and(|running| running.game_id == game.id) {
        return Err("Close the game before deleting it".into());
    }
    if app.state::<Mover>().current().is_some_and(|moving| moving.game_id == game.id) {
        return Err("Wait for the move to finish".into());
    }
    own_folder(app, &game)?;
    release_torrent(app, &game);
    recycle(Path::new(&game.install_dir))
}
