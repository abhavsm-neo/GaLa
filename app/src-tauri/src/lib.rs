mod db;
mod engine;
mod installer;
mod library;
mod metadata;
mod player;
mod settings;

use db::{DayPlaytime, Db, Game, GamePatch};
use engine::Engine;
use installer::{Installer, Progress};
use metadata::{Match, Metadata};
use player::{Player, Running};
use serde_json::json;
use settings::{Settings, Store};
use std::{path::Path, process::Command};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, State, WindowEvent,
};

#[tauri::command]
fn engine_snapshot(engine: State<Engine>) -> bool {
    engine.send(json!({ "cmd": "snapshot" }));
    engine.connected()
}

#[tauri::command]
fn add_magnet(engine: State<Engine>, magnet: String, save_path: String) -> Result<(), String> {
    if !magnet.to_ascii_lowercase().starts_with("magnet:?") {
        return Err("That isn't a magnet link".into());
    }
    engine.send(json!({ "cmd": "add", "magnet": magnet, "save_path": save_path }));
    Ok(())
}

#[tauri::command]
fn add_torrent_file(engine: State<Engine>, path: String, save_path: String) {
    engine.send(json!({ "cmd": "add", "torrent_file": path, "save_path": save_path }));
}

#[tauri::command]
fn pause(engine: State<Engine>, id: String) {
    engine.send(json!({ "cmd": "pause", "id": id }));
}

#[tauri::command]
fn resume(engine: State<Engine>, id: String) {
    engine.send(json!({ "cmd": "resume", "id": id }));
}

#[tauri::command]
fn start_next(engine: State<Engine>, id: String) {
    engine.send(json!({ "cmd": "start_next", "id": id }));
}

#[tauri::command]
fn remove(app: AppHandle, engine: State<Engine>, db: State<Db>, id: String, delete_files: bool) -> Result<(), String> {
    engine.send(json!({ "cmd": "remove", "id": &id, "delete_files": delete_files }));
    if delete_files && db.hide_torrent(&id).map_err(text)? > 0 {
        library::changed(&app);
    }
    Ok(())
}

fn text(error: impl ToString) -> String {
    error.to_string()
}

#[tauri::command]
fn list_games(db: State<Db>) -> Result<Vec<Game>, String> {
    db.games().map_err(text)
}

#[tauri::command]
fn add_game(app: AppHandle, db: State<Db>, file: String) -> Result<i64, String> {
    let id = library::add_file(&db, &file).map_err(text)?;
    library::changed(&app);
    Ok(id)
}

#[tauri::command]
fn install_game(
    app: AppHandle,
    installer: State<Installer>,
    db: State<Db>,
    store: State<Store>,
    id: i64,
    folder: String,
    interactive: bool,
) -> Result<(), String> {
    let game = db.game(id).map_err(text)?.ok_or("That game is no longer in your library")?;
    store.update(|s| s.install_dir = folder.clone())?;
    let dest = Path::new(&folder).join(installer::folder_name(&game.name));
    installer.enqueue(&app, id, dest, interactive)
}

#[tauri::command]
fn install_status(installer: State<Installer>) -> Option<Progress> {
    installer.current()
}

#[tauri::command]
fn free_space(path: String) -> u64 {
    installer::free_space(Path::new(&path))
}

#[tauri::command]
fn update_game(app: AppHandle, db: State<Db>, id: i64, patch: GamePatch) -> Result<(), String> {
    db.patch(id, &patch).map_err(text)?;
    library::changed(&app);
    Ok(())
}

#[tauri::command]
fn set_game_exe(app: AppHandle, db: State<Db>, id: i64, exe: String) -> Result<(), String> {
    db.set_exe(id, Some(&exe)).map_err(text)?;
    library::changed(&app);
    Ok(())
}

#[tauri::command]
fn remove_game(app: AppHandle, db: State<Db>, id: i64) -> Result<(), String> {
    db.hide(id).map_err(text)?;
    library::changed(&app);
    Ok(())
}

#[tauri::command]
async fn search_metadata(app: AppHandle, query: String) -> Result<Vec<Match>, String> {
    app.state::<Metadata>().search(&app, &query).await
}

#[tauri::command]
async fn choose_metadata(app: AppHandle, id: i64, igdb_id: i64) -> Result<(), String> {
    app.state::<Metadata>().choose(&app, id, igdb_id).await?;
    library::changed(&app);
    Ok(())
}

#[tauri::command]
fn find_exes(db: State<Db>, id: i64) -> Result<Vec<String>, String> {
    let game = db.game(id).map_err(text)?.ok_or("That game is no longer in your library")?;
    Ok(library::find_exes(&game.install_dir, &game.name))
}

#[tauri::command]
async fn folder_size(path: String) -> u64 {
    tauri::async_runtime::spawn_blocking(move || library::dir_size(Path::new(&path))).await.unwrap_or(0)
}

#[tauri::command]
fn play_game(app: AppHandle, player: State<Player>, id: i64) -> Result<(), String> {
    player.play(&app, id)
}

#[tauri::command]
fn stop_game(player: State<Player>) {
    player.stop();
}

#[tauri::command]
fn running_game(player: State<Player>) -> Option<Running> {
    player.running()
}

#[tauri::command]
fn playtime_by_day(db: State<Db>, game_id: Option<i64>, days: i64) -> Result<Vec<DayPlaytime>, String> {
    db.playtime_by_day(game_id, days).map_err(text)
}

#[tauri::command]
fn detail(engine: State<Engine>, id: String) {
    engine.send(json!({ "cmd": "detail", "id": id }));
}

#[tauri::command]
fn get_settings(store: State<Store>) -> Settings {
    store.get()
}

#[tauri::command]
fn set_limits(engine: State<Engine>, store: State<Store>, down: u64, up: u64) -> Result<Settings, String> {
    let settings = store.update(|s| {
        s.down_limit = down;
        s.up_limit = up;
    })?;
    engine.send(json!({ "cmd": "set_limits", "down": down, "up": up }));
    Ok(settings)
}

#[tauri::command]
fn set_download_dir(store: State<Store>, path: String) -> Result<Settings, String> {
    store.update(|s| s.download_dir = path)
}

#[tauri::command]
fn quit(app: AppHandle, engine: State<Engine>) {
    engine.quit(&app);
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    Command::new("explorer").arg(path).spawn().map(|_| ()).map_err(|e| e.to_string())
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open GaLa", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit GaLa", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;

    let mut tray = TrayIconBuilder::new()
        .tooltip("GaLa")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "quit" => app.state::<Engine>().quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_main(app)))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle();
            app.manage(Store::load(handle)?);
            let data = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data)?;
            app.manage(Db::open(&data.join("library.db"))?);
            app.manage(Player::default());
            app.manage(Installer::default());
            Installer::start_worker(handle);
            app.manage(Metadata::default());
            metadata::start_worker(handle);
            app.manage(engine::start(handle));
            build_tray(handle)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window leaves GaLa in the tray so downloads carry on.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            engine_snapshot,
            add_magnet,
            add_torrent_file,
            pause,
            resume,
            start_next,
            remove,
            detail,
            get_settings,
            set_limits,
            set_download_dir,
            quit,
            open_folder,
            list_games,
            add_game,
            install_game,
            install_status,
            free_space,
            update_game,
            set_game_exe,
            remove_game,
            search_metadata,
            choose_metadata,
            find_exes,
            folder_size,
            play_game,
            stop_game,
            running_game,
            playtime_by_day,
        ])
        .run(tauri::generate_context!())
        .expect("GaLa failed to start");
}
