//! Turns a download into something playable: extract archives and disc images, run
//! installers, then find the game's file. One game at a time, on a worker thread.

use crate::{db::Db, library};
use serde::Serialize;
use std::{
    collections::{HashSet, VecDeque},
    fs,
    io::Read,
    iter::once,
    mem::{size_of, zeroed},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    ptr::null_mut,
    sync::{Condvar, Mutex},
    thread,
};
use tauri::{AppHandle, Emitter, Manager};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED},
    Storage::FileSystem::GetDiskFreeSpaceExW,
    System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE},
    UI::{
        Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW},
        WindowsAndMessaging::SW_SHOW,
    },
};

const SEVEN_ZIP: &str = r"C:\Program Files\7-Zip\7z.exe";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ERROR_ELEVATION_REQUIRED: i32 = 740;
const SNIFF_BYTES: u64 = 16 << 20;
const SCAN_DEPTH: usize = 2;
const MAX_ROUNDS: usize = 4;
const ARCHIVES: &[&str] = &["iso", "zip", "7z", "rar"];
const SKIP_FOLDERS: &[&str] = &["redist", "_commonredist", "directx", "vcredist", "dotnet", "support", "crack", "fix"];
pub const NEEDS_PASSWORD: &str = "This archive needs a password";
const WRONG_PASSWORD: &str = "That password didn't work";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SetupKind {
    Inno,
    Nsis,
    Msi,
    Other,
}

#[derive(Debug, PartialEq)]
pub enum Source {
    Setup(PathBuf, SetupKind),
    Archive(PathBuf),
    Play(PathBuf),
    Unknown,
}

#[derive(Clone, Serialize)]
pub struct Progress {
    pub game_id: i64,
    pub stage: String,
    pub percent: Option<u8>,
}

struct Job {
    game_id: i64,
    dest: PathBuf,
    interactive: bool,
    password: Option<String>,
}

#[derive(Default)]
pub struct Installer {
    queue: Mutex<VecDeque<Job>>,
    wake: Condvar,
    current: Mutex<Option<Progress>>,
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

fn setup_kind(path: &Path) -> SetupKind {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("msi")) {
        return SetupKind::Msi;
    }
    let mut head = Vec::new();
    if let Ok(file) = fs::File::open(path) {
        let _ = file.take(SNIFF_BYTES).read_to_end(&mut head);
    }
    if contains(&head, b"Inno Setup") {
        SetupKind::Inno
    } else if contains(&head, b"Nullsoft") || contains(&head, b"NullsoftInst") {
        SetupKind::Nsis
    } else {
        SetupKind::Other
    }
}

fn extension(path: &Path) -> String {
    path.extension().map_or(String::new(), |e| e.to_string_lossy().to_lowercase())
}

/// Only the first volume of a split RAR is opened. 7-Zip finds the rest.
fn is_archive(path: &Path) -> bool {
    let name = path.file_name().map_or(String::new(), |n| n.to_string_lossy().to_lowercase());
    if let Some(start) = name.rfind(".part") {
        let digits = name[start + 5..].trim_end_matches(".rar");
        if name.ends_with(".rar") && digits.parse::<u32>().is_ok_and(|part| part != 1) {
            return false;
        }
    }
    ARCHIVES.contains(&extension(path).as_str()) || name.ends_with(".7z.001")
}

fn is_setup_name(path: &Path) -> bool {
    let stem = path.file_stem().map_or(String::new(), |s| s.to_string_lossy().to_lowercase());
    extension(path) == "msi" || (extension(path) == "exe" && (stem.starts_with("setup") || stem.starts_with("install")))
}

fn files_within(folder: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if depth < SCAN_DEPTH && !SKIP_FOLDERS.iter().any(|skip| name.contains(skip)) {
                files_within(&path, depth + 1, found);
            }
        } else {
            found.push(path);
        }
    }
}

/// An installer beats a loose game file, which beats an archive that might only be extras.
pub fn scan(path: &Path, name: &str, done: &HashSet<PathBuf>) -> Source {
    if path.is_file() {
        return if is_archive(path) {
            Source::Archive(path.into())
        } else if is_setup_name(path) || setup_kind(path) != SetupKind::Other {
            Source::Setup(path.into(), setup_kind(path))
        } else if extension(path) == "exe" {
            Source::Play(path.into())
        } else {
            Source::Unknown
        };
    }

    let mut files = Vec::new();
    files_within(path, 0, &mut files);
    files.retain(|file| !done.contains(file));
    let shallowest = |a: &&PathBuf, b: &&PathBuf| a.components().count().cmp(&b.components().count());

    if let Some(setup) = files.iter().filter(|f| is_setup_name(f)).min_by(shallowest) {
        return Source::Setup(setup.clone(), setup_kind(setup));
    }
    if let Some(exe) = library::confident_exe(&path.to_string_lossy(), name) {
        return Source::Play(exe.into());
    }
    match files.iter().filter(|f| is_archive(f)).min_by(shallowest) {
        Some(archive) => Source::Archive(archive.clone()),
        None => Source::Unknown,
    }
}

pub fn free_space(path: &Path) -> u64 {
    let mut folder = path;
    while !folder.exists() {
        match folder.parent() {
            Some(parent) => folder = parent,
            None => return 0,
        }
    }
    let wide: Vec<u16> = folder.to_string_lossy().encode_utf16().chain(once(0)).collect();
    let mut free = 0u64;
    unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, null_mut(), null_mut()) };
    free
}

/// A folder name that's safe on Windows, from whatever the download was called.
pub fn folder_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { ' ' } else { c })
        .collect();
    let stem = ARCHIVES.iter().find_map(|ext| clean.strip_suffix(&format!(".{ext}"))).unwrap_or(&clean);
    let tidy = stem.trim().trim_end_matches('.').trim();
    if tidy.is_empty() { "Game".into() } else { tidy.into() }
}

/// `-p-` stands in when there's no password, so 7-Zip never waits for one to be typed.
fn password_arg(password: Option<&str>) -> String {
    format!("-p{}", password.unwrap_or("-"))
}

fn unpacked_size(archive: &Path, password: Option<&str>) -> Result<u64, String> {
    let output = Command::new(SEVEN_ZIP)
        .arg("l")
        .arg(password_arg(password))
        .arg(archive)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "GaLa needs 7-Zip, which wasn't found in Program Files".to_string())?;
    let text = String::from_utf8_lossy(&output.stdout);
    let totals = text.lines().rev().find(|line| line.contains(" files"));
    Ok(totals.and_then(|line| line.split_whitespace().rev().find_map(|word| {
        let digits = word.parse::<u64>().ok()?;
        (digits > 1_000).then_some(digits)
    })).unwrap_or(0))
}

fn extract(archive: &Path, dest: &Path, password: Option<&str>, report: impl Fn(u8)) -> Result<(), String> {
    let needed = unpacked_size(archive, password)?;
    let free = free_space(dest);
    if needed > free {
        return Err(format!("Needs {} GB free, but this drive has {} GB", needed >> 30, free >> 30));
    }
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;

    let mut child = Command::new(SEVEN_ZIP)
        .args(["x", "-y", "-aoa", "-bso0", "-bsp1"])
        .arg(password_arg(password))
        .arg(format!("-o{}", dest.display()))
        .arg(archive)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "GaLa needs 7-Zip, which wasn't found in Program Files".to_string())?;

    // 7-Zip redraws one line with backspaces, so progress never arrives as whole lines.
    let mut stdout = child.stdout.take().ok_or("7-Zip gave no output")?;
    let mut chunk = [0u8; 256];
    let mut recent = String::new();
    while let Ok(count) = stdout.read(&mut chunk) {
        if count == 0 {
            break;
        }
        recent.push_str(&String::from_utf8_lossy(&chunk[..count]));
        if let Some(mark) = recent.rfind('%') {
            let digits: String = recent[..mark].chars().rev().skip_while(|c| *c == ' ').take_while(char::is_ascii_digit).collect();
            if let Ok(percent) = digits.chars().rev().collect::<String>().parse::<u8>() {
                report(percent);
            }
            recent.clear();
        }
    }

    let mut errors = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut errors);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        return Ok(());
    }
    Err(if errors.to_lowercase().contains("password") {
        if password.is_some() { WRONG_PASSWORD } else { NEEDS_PASSWORD }.into()
    } else {
        errors
            .lines()
            .find(|line| !line.trim().is_empty() && !line.contains("Codec Load Error"))
            .unwrap_or("7-Zip couldn't extract this archive")
            .to_string()
    })
}

fn silent_args(kind: SetupKind, dest: &Path) -> String {
    let dest = dest.display();
    match kind {
        SetupKind::Inno => format!("/SILENT /SP- /SUPPRESSMSGBOXES /NORESTART /DIR=\"{dest}\""),
        // NSIS reads everything after /D= as the path, so it must come last and unquoted.
        SetupKind::Nsis => format!("/S /D={dest}"),
        SetupKind::Other | SetupKind::Msi => String::new(),
    }
}

fn run_elevated(file: &str, params: &str, folder: &Path) -> Result<u32, String> {
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(once(0)).collect() };
    let (verb, file, params, folder) = (wide("runas"), wide(file), wide(params), wide(&folder.to_string_lossy()));
    unsafe {
        let mut info: SHELLEXECUTEINFOW = zeroed();
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.lpDirectory = folder.as_ptr();
        info.nShow = SW_SHOW as i32;
        if ShellExecuteExW(&mut info) == 0 {
            let error = GetLastError();
            return Err(if error == ERROR_CANCELLED {
                "The Windows permission prompt was declined".into()
            } else {
                format!("Windows couldn't start the installer (error {error})")
            });
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1;
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        Ok(code)
    }
}

fn run_setup(setup: &Path, kind: SetupKind, dest: &Path, interactive: bool) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let folder = setup.parent().unwrap_or(Path::new("."));
    let (program, params) = if kind == SetupKind::Msi {
        let mode = if interactive { "/qf" } else { "/qb" };
        ("msiexec".to_string(), format!("/i \"{}\" {mode} INSTALLDIR=\"{1}\" TARGETDIR=\"{1}\"", setup.display(), dest.display()))
    } else {
        let params = if interactive { String::new() } else { silent_args(kind, dest) };
        (setup.to_string_lossy().into_owned(), params)
    };

    // Most installers ask for administrator rights, but some don't need the prompt.
    let code = match Command::new(&program).raw_arg(&params).current_dir(folder).status() {
        Ok(status) => status.code().unwrap_or(1) as u32,
        Err(error) if error.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED) => run_elevated(&program, &params, folder)?,
        Err(error) => return Err(error.to_string()),
    };
    if code == 0 {
        Ok(())
    } else {
        Err(format!("The installer stopped with code {code}"))
    }
}

impl Installer {
    pub fn start_worker(app: &AppHandle) {
        let app = app.clone();
        thread::spawn(move || loop {
            let job = {
                let installer = app.state::<Installer>();
                let mut queue = installer.queue.lock().unwrap();
                loop {
                    match queue.pop_front() {
                        Some(job) => break job,
                        None => queue = installer.wake.wait(queue).unwrap(),
                    }
                }
            };
            let result = process(&app, &job);
            finish(&app, &job, result);
        });
    }

    pub fn enqueue(&self, app: &AppHandle, game_id: i64, dest: PathBuf, interactive: bool, password: Option<String>) -> Result<(), String> {
        app.state::<Db>().set_install(game_id, "queued", "").map_err(|e| e.to_string())?;
        self.queue.lock().unwrap().push_back(Job { game_id, dest, interactive, password });
        self.wake.notify_one();
        library::changed(app);
        Ok(())
    }

    pub fn current(&self) -> Option<Progress> {
        self.current.lock().unwrap().clone()
    }
}

fn report(app: &AppHandle, game_id: i64, stage: &str, percent: Option<u8>) {
    let progress = Progress { game_id, stage: stage.into(), percent };
    *app.state::<Installer>().current.lock().unwrap() = Some(progress.clone());
    let _ = app.emit("install-progress", Some(progress));
}

fn enter_stage(app: &AppHandle, game_id: i64, stage: &str) -> Result<(), String> {
    app.state::<Db>().set_install(game_id, stage, "").map_err(|e| e.to_string())?;
    report(app, game_id, stage, None);
    library::changed(app);
    Ok(())
}

fn process(app: &AppHandle, job: &Job) -> Result<Option<String>, String> {
    let db = app.state::<Db>();
    let game = db.game(job.game_id).map_err(|e| e.to_string())?.ok_or("That game is no longer in your library")?;
    let source = PathBuf::from(game.source_path.as_deref().unwrap_or(&game.install_dir));
    if !source.exists() {
        return Err(format!("{} is missing", source.display()));
    }

    let mut done = HashSet::new();
    let mut current = source;
    for _ in 0..MAX_ROUNDS {
        match scan(&current, &game.name, &done) {
            Source::Archive(archive) => {
                enter_stage(app, job.game_id, "extracting")?;
                let game_id = job.game_id;
                extract(&archive, &job.dest, job.password.as_deref(), |percent| report(app, game_id, "extracting", Some(percent)))?;
                done.insert(archive);
                current = job.dest.clone();
            }
            Source::Setup(setup, kind) => {
                enter_stage(app, job.game_id, "installing")?;
                run_setup(&setup, kind, &job.dest, job.interactive)?;
                break;
            }
            Source::Play(exe) => return Ok(Some(exe.to_string_lossy().into_owned())),
            Source::Unknown => break,
        }
    }

    enter_stage(app, job.game_id, "detecting")?;
    Ok(library::confident_exe(&job.dest.to_string_lossy(), &game.name))
}

fn finish(app: &AppHandle, job: &Job, result: Result<Option<String>, String>) {
    let db = app.state::<Db>();
    *app.state::<Installer>().current.lock().unwrap() = None;
    let _ = app.emit("install-progress", Option::<Progress>::None);
    let dir = job.dest.to_string_lossy();
    let saved = match result {
        Ok(exe) => db.finish_install(job.game_id, &dir, exe.as_deref()),
        Err(error) => db.set_install(job.game_id, "failed", &error),
    };
    if let Err(error) = saved {
        let _ = app.emit("engine-event", serde_json::json!({ "event": "error", "id": "", "message": error.to_string() }));
    }
    library::changed(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_only_the_first_rar_volume() {
        assert!(is_archive(Path::new("game.part1.rar")));
        assert!(is_archive(Path::new("game.part01.rar")));
        assert!(!is_archive(Path::new("game.part02.rar")));
        assert!(is_archive(Path::new("game.rar")));
        assert!(is_archive(Path::new("game.7z.001")));
        assert!(!is_archive(Path::new("game.bin")));
    }

    #[test]
    fn asks_7zip_never_to_prompt() {
        assert_eq!(password_arg(None), "-p-");
        assert_eq!(password_arg(Some("cs.rin.ru")), "-pcs.rin.ru");
    }

    #[test]
    fn makes_safe_folder_names() {
        assert_eq!(folder_name("Game: The Sequel?.iso"), "Game  The Sequel");
        assert_eq!(folder_name("..."), "Game");
    }

    #[test]
    fn nsis_folder_comes_last_and_unquoted() {
        assert_eq!(silent_args(SetupKind::Nsis, Path::new(r"E:\My Games\X")), r"/S /D=E:\My Games\X");
    }
}
