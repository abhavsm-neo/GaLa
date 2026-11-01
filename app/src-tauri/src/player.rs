//! Launches games inside a Job Object. A launcher that starts the real game and quits
//! stays in the job, so the session only ends when every process in it has gone.

use crate::{db::Db, library};
use serde::Serialize;
use std::{
    iter::once,
    mem::{size_of, zeroed},
    path::Path,
    ptr::{null, null_mut},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ELEVATION_REQUIRED},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
            QueryInformationJobObject, TerminateJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        },
        Threading::{CreateProcessW, ResumeThread, TerminateProcess, CREATE_SUSPENDED, PROCESS_INFORMATION, STARTUPINFOW},
    },
};

const POLL: Duration = Duration::from_secs(1);
const SAVE_EVERY: Duration = Duration::from_secs(15);

#[derive(Clone, Serialize)]
pub struct Running {
    pub game_id: i64,
    pub started_at: i64,
    #[serde(skip)]
    job: usize,
}

#[derive(Default)]
pub struct Player(Mutex<Option<Running>>);

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(once(0)).collect()
}

fn launch(exe: &str, args: &str) -> Result<usize, String> {
    let folder = Path::new(exe).parent().ok_or("That file has no folder")?.to_string_lossy().into_owned();
    let mut command_line = wide(&format!("\"{exe}\" {args}"));

    // The process starts frozen so it can't spawn anything before it's inside the job.
    unsafe {
        let job = CreateJobObjectW(null(), null());
        if job.is_null() {
            return Err("Windows wouldn't create a job for this game".into());
        }
        let mut startup: STARTUPINFOW = zeroed();
        startup.cb = size_of::<STARTUPINFOW>() as u32;
        let mut process: PROCESS_INFORMATION = zeroed();
        let created = CreateProcessW(
            wide(exe).as_ptr(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            0,
            CREATE_SUSPENDED,
            null(),
            wide(&folder).as_ptr(),
            &startup,
            &mut process,
        );
        if created == 0 {
            let error = GetLastError();
            CloseHandle(job);
            return Err(if error == ERROR_ELEVATION_REQUIRED {
                "This game asks to run as administrator, which GaLa can't track yet".into()
            } else {
                format!("Windows couldn't start the game (error {error})")
            });
        }
        if AssignProcessToJobObject(job, process.hProcess) == 0 {
            TerminateProcess(process.hProcess, 1);
            CloseHandle(process.hThread);
            CloseHandle(process.hProcess);
            CloseHandle(job);
            return Err("Windows wouldn't let GaLa track this game".into());
        }
        ResumeThread(process.hThread);
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
        Ok(job as usize)
    }
}

fn still_running(job: usize) -> bool {
    unsafe {
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = zeroed();
        let ok = QueryInformationJobObject(
            job as _,
            JobObjectBasicAccountingInformation,
            &mut info as *mut _ as *mut _,
            size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
            null_mut(),
        );
        ok != 0 && info.ActiveProcesses > 0
    }
}

fn watch(app: AppHandle, job: usize, session: i64) {
    let started = Instant::now();
    let mut saved = Instant::now();
    let db = app.state::<Db>();
    while still_running(job) {
        thread::sleep(POLL);
        if saved.elapsed() >= SAVE_EVERY {
            saved = Instant::now();
            let _ = db.record_session(session, started.elapsed().as_secs() as i64);
        }
    }
    let _ = db.record_session(session, started.elapsed().as_secs() as i64);
    unsafe { CloseHandle(job as _) };
    *app.state::<Player>().0.lock().unwrap() = None;
    let _ = app.emit("game-status", Option::<Running>::None);
    library::changed(&app);
}

impl Player {
    pub fn running(&self) -> Option<Running> {
        self.0.lock().unwrap().clone()
    }

    pub fn play(&self, app: &AppHandle, game_id: i64) -> Result<(), String> {
        let mut running = self.0.lock().unwrap();
        if running.is_some() {
            return Err("Close the game that's running first".into());
        }
        let db = app.state::<Db>();
        let game = db.game(game_id).map_err(|e| e.to_string())?.ok_or("That game is no longer in your library")?;
        let exe = game.exe_path.ok_or("Choose the game's file first")?;
        if !Path::new(&exe).exists() {
            return Err(format!("{exe} is missing"));
        }

        let job = launch(&exe, &game.args)?;
        let session = db.start_session(game_id).map_err(|e| e.to_string())?;
        let now = Running { game_id, started_at: crate::db::now(), job };
        *running = Some(now.clone());
        let _ = app.emit("game-status", Some(now));
        library::changed(app);

        let app = app.clone();
        thread::spawn(move || watch(app, job, session));
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(running) = &*self.0.lock().unwrap() {
            unsafe { TerminateJobObject(running.job as _, 0) };
        }
    }
}
