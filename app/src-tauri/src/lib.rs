mod db;
mod engine;
mod hardware;
mod installer;
mod library;
mod metadata;
mod mover;
mod news;
mod player;
mod saves;
mod scanner;
mod seeding;
mod settings;
mod specs;

use db::{Article, DayPlaytime, Db, Game, GamePatch};
use engine::Engine;
use installer::{Installer, Progress};
use metadata::{Match, Metadata};
use mover::{MoveProgress, Mover};
use news::News;
use player::{Player, Running};
use saves::{SaveStatus, Saves};
use scanner::Found;
use seeding::Seeder;
use serde_json::json;
use settings::{NewsFeed, Settings, Store};
use std::{path::Path, process::Command, sync::Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};

/// A torrent or magnet link Windows handed over, waiting for the Add dialog.
#[derive(Default)]
struct Opened(Mutex<Option<String>>);

fn is_magnet(text: &str) -> bool {
    text.get(..8).is_some_and(|start| start.eq_ignore_ascii_case("magnet:?"))
}

fn opened_item<I: IntoIterator<Item = String>>(args: I, is_file: impl Fn(&str) -> bool) -> Option<String> {
    args.into_iter().skip(1).find(|arg| is_magnet(arg) || (arg.to_lowercase().ends_with(".torrent") && is_file(arg)))
}

fn receive_opened(app: &AppHandle, args: Vec<String>) {
    if let Some(item) = opened_item(args, |path| Path::new(path).is_file()) {
        *app.state::<Opened>().0.lock().unwrap() = Some(item);
        let _ = app.emit("torrent-opened", ());
    }
}

#[tauri::command]
fn take_opened(opened: State<Opened>) -> Option<String> {
    opened.0.lock().unwrap().take()
}

#[tauri::command]
fn engine_snapshot(engine: State<Engine>) -> bool {
    engine.send(json!({ "cmd": "snapshot" }));
    engine.connected()
}

#[tauri::command]
/// New torrents wait for their files to be chosen. The engine answers with the request.
fn add_magnet(engine: State<Engine>, magnet: String, save_path: String, request: String) -> Result<(), String> {
    if !magnet.to_ascii_lowercase().starts_with("magnet:?") {
        return Err("That isn't a magnet link".into());
    }
    engine.send(json!({ "cmd": "add", "magnet": magnet, "save_path": save_path, "hold": true, "request": request }));
    Ok(())
}

#[tauri::command]
fn add_torrent_file(engine: State<Engine>, path: String, save_path: String, request: String) {
    engine.send(json!({ "cmd": "add", "torrent_file": path, "save_path": save_path, "hold": true, "request": request }));
}

#[tauri::command]
fn set_files(engine: State<Engine>, id: String, wanted: Vec<bool>) {
    engine.send(json!({ "cmd": "set_files", "id": id, "wanted": wanted }));
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
    password: Option<String>,
) -> Result<(), String> {
    let game = db.game(id).map_err(text)?.ok_or("That game is no longer in your library")?;
    store.update(|s| s.install_dir = folder.clone())?;
    let dest = Path::new(&folder).join(installer::folder_name(&game.name));
    installer.enqueue(&app, id, dest, interactive, password.filter(|p| !p.is_empty()))
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
async fn remove_game(app: AppHandle, id: i64, delete_files: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if delete_files {
            mover::recycle_game(&app, id)?;
        }
        app.state::<Db>().hide(id).map_err(text)?;
        library::changed(&app);
        Ok(())
    })
    .await
    .map_err(text)?
}

#[tauri::command]
async fn scan_games(app: AppHandle, folders: Vec<String>, launchers: bool) -> Result<Vec<Found>, String> {
    tauri::async_runtime::spawn_blocking(move || scanner::run(&app.state::<Db>(), &folders, launchers))
        .await
        .map_err(text)?
}

#[tauri::command]
fn add_scanned(app: AppHandle, db: State<Db>, games: Vec<Found>) -> Result<(), String> {
    scanner::add(&db, &games)?;
    library::changed(&app);
    Ok(())
}

#[tauri::command]
fn move_game(app: AppHandle, mover: State<Mover>, id: i64, folder: String) -> Result<(), String> {
    mover.start(&app, id, &folder)
}

#[tauri::command]
fn cancel_move(mover: State<Mover>) {
    mover.cancel();
}

#[tauri::command]
fn move_status(mover: State<Mover>) -> Option<MoveProgress> {
    mover.current()
}

async fn with_game<T: Send + 'static>(
    app: AppHandle,
    id: i64,
    work: impl FnOnce(&AppHandle, &Game) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let game = app.state::<Db>().game(id).map_err(text)?.ok_or("That game is no longer in your library")?;
        work(&app, &game)
    })
    .await
    .map_err(text)?
}

#[tauri::command]
async fn save_status(app: AppHandle, id: i64) -> Result<SaveStatus, String> {
    with_game(app, id, |app, game| Ok(saves::status(app, game))).await
}

#[tauri::command]
async fn back_up_saves(app: AppHandle, id: i64) -> Result<(), String> {
    with_game(app, id, saves::back_up_now).await
}

#[tauri::command]
async fn restore_saves(app: AppHandle, id: i64, created: i64) -> Result<(), String> {
    with_game(app, id, move |app, game| saves::restore(app, game, created)).await
}

#[tauri::command]
fn set_save_folder(app: AppHandle, db: State<Db>, id: i64, folder: Option<String>) -> Result<(), String> {
    db.set_save_folder(id, folder.as_deref()).map_err(text)?;
    library::changed(&app);
    Ok(())
}

#[tauri::command]
fn set_seed_rule(store: State<Store>, rule: String) -> Result<Settings, String> {
    store.update(|s| s.seed_rule = rule)
}

#[tauri::command]
fn pc_specs() -> specs::Pc {
    specs::pc().clone()
}

#[tauri::command]
async fn pc_check(app: AppHandle, id: i64) -> Result<Option<specs::Check>, String> {
    specs::check_game(&app, id).await
}

#[tauri::command]
async fn pc_check_name(app: AppHandle, name: String) -> Result<Option<specs::Check>, String> {
    specs::check_name(&app, &name).await
}

#[tauri::command]
async fn pc_verdicts(app: AppHandle) -> Vec<(i64, String, String)> {
    tauri::async_runtime::spawn_blocking(move || specs::verdicts(&app)).await.unwrap_or_default()
}

#[tauri::command]
fn news_list(app: AppHandle) -> Result<Vec<Article>, String> {
    news::list(&app)
}

#[tauri::command]
async fn news_image(app: AppHandle, id: i64) -> Result<Option<String>, String> {
    app.state::<News>().image(&app, id).await
}

#[tauri::command]
fn news_mark_read(db: State<Db>, ids: Vec<i64>) -> Result<(), String> {
    db.mark_read(&ids).map_err(text)
}

#[tauri::command]
fn news_mark_all_read(db: State<Db>) -> Result<(), String> {
    db.mark_all_read().map_err(text)
}

#[tauri::command]
fn news_refresh(news: State<News>) {
    news.wake();
}

#[tauri::command]
async fn check_news_feed(app: AppHandle, url: String) -> Result<String, String> {
    app.state::<News>().check(url.trim()).await
}

/// Articles from a feed that's no longer followed go with it.
#[tauri::command]
fn set_news_feeds(app: AppHandle, store: State<Store>, db: State<Db>, feeds: Vec<NewsFeed>) -> Result<Settings, String> {
    let before = store.get().news_feeds;
    let settings = store.update(|s| s.news_feeds = feeds)?;
    for gone in before.iter().filter(|old| !settings.news_feeds.iter().any(|feed| feed.name == old.name)) {
        for image in db.drop_source(&gone.name).map_err(text)? {
            let _ = std::fs::remove_file(image);
        }
    }
    app.state::<News>().wake();
    Ok(settings)
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    news::open_url(&url)
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
    let stop_seeding = MenuItem::with_id(app, "stop_seeding", "Stop all seeding", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit GaLa", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &stop_seeding, &quit])?;

    let mut tray = TrayIconBuilder::with_id(seeding::TRAY_ID)
        .tooltip("GaLa")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "stop_seeding" => seeding::stop_all(app),
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
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            show_main(app);
            receive_opened(app, args);
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle();
            app.manage(Opened::default());
            receive_opened(handle, std::env::args().collect());
            app.manage(Store::load(handle)?);
            let data = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data)?;
            app.manage(Db::open(&data.join("library.db"))?);
            app.manage(Player::default());
            app.manage(Installer::default());
            Installer::start_worker(handle);
            app.manage(Metadata::default());
            metadata::start_worker(handle);
            app.manage(News::default());
            news::start_worker(handle);
            specs::start_worker(handle);
            // Reading the parts takes a moment, so it happens before anyone asks.
            std::thread::spawn(|| {
                specs::pc();
            });
            app.manage(Mover::default());
            app.manage(Saves::default());
            app.manage(Seeder::default());
            tauri::async_runtime::spawn(saves::refresh_list(handle.clone()));
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
            news_list,
            pc_specs,
            pc_check,
            pc_check_name,
            pc_verdicts,
            news_image,
            news_mark_read,
            news_mark_all_read,
            news_refresh,
            check_news_feed,
            set_news_feeds,
            open_url,
            add_magnet,
            add_torrent_file,
            pause,
            resume,
            set_files,
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
            scan_games,
            add_scanned,
            move_game,
            cancel_move,
            move_status,
            save_status,
            back_up_saves,
            restore_saves,
            set_save_folder,
            set_seed_rule,
            take_opened,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn opened(args: &[&str]) -> Option<String> {
        opened_item(args.iter().map(|arg| arg.to_string()), |path| path.contains("real"))
    }

    #[test]
    fn accepts_magnets_and_real_torrent_files() {
        assert_eq!(opened(&["gala.exe", "magnet:?xt=urn:btih:abc&dn=x"]).as_deref(), Some("magnet:?xt=urn:btih:abc&dn=x"));
        assert_eq!(opened(&["gala.exe", "MAGNET:?xt=abc"]).as_deref(), Some("MAGNET:?xt=abc"));
        assert_eq!(opened(&["gala.exe", r"E:\real game.TORRENT"]).as_deref(), Some(r"E:\real game.TORRENT"));
    }

    #[test]
    fn ignores_everything_else() {
        assert_eq!(opened(&["gala.exe"]), None);
        assert_eq!(opened(&["gala.exe", r"E:\missing.torrent"]), None);
        assert_eq!(opened(&["gala.exe", "magnet:nope"]), None);
        assert_eq!(opened(&["magnet:?xt=abc"]), None);
        assert_eq!(opened(&["gala.exe", r"E:\real.exe"]), None);
    }
}
