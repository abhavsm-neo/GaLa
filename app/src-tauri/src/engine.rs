//! Keeps one connection to gala-engine alive and passes JSON lines between it and the UI.

use crate::settings::Store;
use serde_json::{json, Value};
use std::{
    io::ErrorKind,
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::windows::named_pipe::{ClientOptions, NamedPipeClient},
    sync::mpsc,
    time::{sleep, Instant},
};

const PIPE: &str = r"\\.\pipe\gala-engine";
const DEV_ENGINE: &str = r"E:\GaLa\dev\build\engine\Release\gala-engine.exe";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ERROR_PIPE_BUSY: i32 = 231;
const QUIT_TIMEOUT: Duration = Duration::from_secs(15);

pub struct Engine {
    commands: mpsc::UnboundedSender<String>,
    connected: Arc<AtomicBool>,
    quitting: Arc<AtomicBool>,
}

impl Engine {
    /// Commands sent while the engine is away wait here and go out on reconnect.
    pub fn send(&self, command: Value) {
        let _ = self.commands.send(command.to_string());
    }

    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Stops the engine, then the app. If the engine doesn't answer, leave anyway.
    pub fn quit(&self, app: &AppHandle) {
        self.quitting.store(true, Ordering::SeqCst);
        self.send(json!({ "cmd": "shutdown" }));
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            sleep(QUIT_TIMEOUT).await;
            app.exit(0);
        });
    }
}

pub fn start(app: &AppHandle) -> Engine {
    let (commands, receiver) = mpsc::unbounded_channel();
    let connected = Arc::new(AtomicBool::new(false));
    let quitting = Arc::new(AtomicBool::new(false));
    tauri::async_runtime::spawn(run(app.clone(), receiver, connected.clone(), quitting.clone()));
    Engine { commands, connected, quitting }
}

async fn run(
    app: AppHandle,
    mut commands: mpsc::UnboundedReceiver<String>,
    connected: Arc<AtomicBool>,
    quitting: Arc<AtomicBool>,
) {
    loop {
        let client = connect(&app).await;
        let (read, mut write) = tokio::io::split(client);
        let mut lines = BufReader::new(read).lines();

        connected.store(true, Ordering::SeqCst);
        let _ = app.emit("engine-status", true);
        if greet(&app, &mut write).await.is_err() {
            continue;
        }

        loop {
            tokio::select! {
                line = lines.next_line() => {
                    let Ok(Some(line)) = line else { break };
                    let Ok(event) = serde_json::from_str::<Value>(&line) else { continue };
                    if event["event"] == "stopped" && quitting.load(Ordering::SeqCst) {
                        app.exit(0);
                    }
                    crate::library::on_engine_event(&app, &event);
                    let _ = app.emit("engine-event", event);
                }
                Some(command) = commands.recv() => {
                    if write.write_all(format!("{command}\n").as_bytes()).await.is_err() {
                        break;
                    }
                }
            }
        }

        connected.store(false, Ordering::SeqCst);
        let _ = app.emit("engine-status", false);
    }
}

/// The engine remembers nothing about limits, so they go out on every connection.
async fn greet(
    app: &AppHandle,
    write: &mut (impl AsyncWriteExt + Unpin),
) -> std::io::Result<()> {
    let settings = app.state::<Store>().get();
    let limits = json!({ "cmd": "set_limits", "down": settings.down_limit, "up": settings.up_limit });
    write.write_all(format!("{limits}\n{}\n", json!({ "cmd": "snapshot" })).as_bytes()).await
}

async fn connect(app: &AppHandle) -> NamedPipeClient {
    let mut last_spawn: Option<Instant> = None;
    loop {
        match ClientOptions::new().open(PIPE) {
            Ok(client) => return client,
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if last_spawn.is_none_or(|at| at.elapsed() > Duration::from_secs(10)) {
                    last_spawn = Some(Instant::now());
                    if let Err(message) = spawn_engine(app) {
                        let _ = app.emit("engine-event", json!({ "event": "error", "id": "", "message": message }));
                    }
                }
            }
            Err(error) => {
                let _ = app.emit("engine-event", json!({ "event": "error", "id": "", "message": error.to_string() }));
            }
        }
        sleep(Duration::from_millis(200)).await;
    }
}

fn spawn_engine(app: &AppHandle) -> Result<(), String> {
    let state = app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("engine");
    Command::new(engine_path())
        .arg(state)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Couldn't start the engine: {e}"))
}

fn engine_path() -> PathBuf {
    let bundled = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("gala-engine.exe")));
    bundled.filter(|path| path.exists()).unwrap_or_else(|| DEV_ENGINE.into())
}
