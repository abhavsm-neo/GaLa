//! Backs up game saves. The community Ludusavi list says where each game keeps them, and
//! cracked copies use a few fixed folders named after the Steam id. Backups are plain
//! folders, so they outlive GaLa's database and can be opened by hand.

use crate::{
    db::{now, Db, Game},
    installer::{folder_name, free_space},
    metadata::words,
    mover::copy_tree,
    player::Player,
    settings::Store,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};

const LIST_URL: &str = "https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml";
const LIST_FILE: &str = "save-locations.json";
const LIST_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const BACKUP_FILE: &str = "backup.json";
const KEEP: usize = 10;
const MAX_BYTES: u64 = 5 << 30;

const CRACK_FOLDERS: &[&str] = &[
    "<winAppData>/Goldberg SteamEmu Saves/{id}",
    "<winAppData>/GSE Saves/{id}",
    "<winAppData>/Steam/CODEX/{id}",
    "<winAppData>/EMPRESS/{id}",
    "<winPublic>/Documents/Steam/CODEX/{id}",
    "<winPublic>/Documents/Steam/RUNE/{id}",
    "<winPublic>/Documents/EMPRESS/{id}",
    "<winLocalAppData>/SKIDROW/{id}",
    "<winDocuments>/SKIDROW/{id}",
];

#[derive(Default, Deserialize)]
#[serde(default)]
struct ListedGame {
    alias: Option<String>,
    files: HashMap<String, FileRule>,
    steam: Option<SteamId>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct FileRule {
    tags: Vec<String>,
    when: Vec<When>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct When {
    os: Option<String>,
    store: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct SteamId {
    id: Option<u64>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct Locations {
    paths: Vec<String>,
    steam: Option<u64>,
}

struct Index {
    exact: HashMap<String, Locations>,
    loose: HashMap<String, String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Root {
    index: usize,
    original: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Backup {
    pub created: i64,
    pub label: String,
    pub bytes: u64,
    roots: Vec<Root>,
}

#[derive(Serialize)]
pub struct SaveStatus {
    pub listed: bool,
    pub found: Vec<String>,
    pub backups: Vec<Backup>,
    pub folder: String,
}

#[derive(Default)]
pub struct Saves(Mutex<Option<Arc<Index>>>);

fn loose_key(name: &str) -> String {
    words(name).join(" ")
}

fn applies_to_windows(rule: &FileRule) -> bool {
    rule.tags.iter().any(|tag| tag == "save")
        && (rule.when.is_empty()
            || rule.when.iter().any(|when| {
                when.os.as_deref().is_none_or(|os| os == "windows") && when.store.as_deref() != Some("microsoft")
            }))
}

fn reduce(yaml: &str) -> Result<HashMap<String, Locations>, String> {
    let listed: HashMap<String, ListedGame> = serde_yaml::from_str(yaml).map_err(|e| e.to_string())?;
    let mut games = HashMap::new();
    for (name, game) in &listed {
        let source = game.alias.as_ref().and_then(|target| listed.get(target)).unwrap_or(game);
        let mut paths: Vec<String> = source.files.iter().filter(|(_, rule)| applies_to_windows(rule)).map(|(path, _)| path.clone()).collect();
        paths.sort();
        let steam = source.steam.as_ref().and_then(|steam| steam.id);
        if !paths.is_empty() || steam.is_some() {
            games.insert(name.to_lowercase(), Locations { paths, steam });
        }
    }
    Ok(games)
}

fn list_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_local_data_dir().map(|dir| dir.join(LIST_FILE)).map_err(|e| e.to_string())
}

/// Runs in the background at launch. Without the list, games with a chosen folder still work.
pub async fn refresh_list(app: AppHandle) {
    let Ok(path) = list_path(&app) else { return };
    let fresh = fs::metadata(&path).and_then(|meta| meta.modified()).ok().and_then(|at| at.elapsed().ok()).is_some_and(|age| age < LIST_MAX_AGE);
    if fresh {
        return;
    }
    let client = reqwest::Client::builder().user_agent("GaLa/0.1").timeout(Duration::from_secs(180)).build();
    let Ok(client) = client else { return };
    let yaml = match client.get(LIST_URL).send().await.and_then(reqwest::Response::error_for_status) {
        Ok(response) => response.text().await,
        Err(error) => Err(error),
    };
    let Ok(yaml) = yaml else { return };
    let reduced = tauri::async_runtime::spawn_blocking(move || {
        let games = reduce(&yaml)?;
        let temp = path.with_extension("tmp");
        fs::write(&temp, serde_json::to_vec(&games).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        fs::rename(&temp, &path).map_err(|e| e.to_string())
    })
    .await;
    match reduced {
        Ok(Ok(())) => *app.state::<Saves>().0.lock().unwrap() = None,
        Ok(Err(error)) => eprintln!("save list: {error}"),
        Err(error) => eprintln!("save list: {error}"),
    }
}

fn wild_match(pattern: &str, text: &str) -> bool {
    let (pattern, text): (Vec<char>, Vec<char>) = (pattern.to_lowercase().chars().collect(), text.to_lowercase().chars().collect());
    let (mut p, mut t, mut star, mut mark) = (0, 0, None, 0);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            mark = t;
            p += 1;
        } else if let Some(at) = star {
            p = at + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

fn expand_glob(pattern: &str) -> Vec<PathBuf> {
    let mut parts = pattern.split(['/', '\\']).filter(|part| !part.is_empty());
    let Some(first) = parts.next() else { return Vec::new() };
    let mut current = vec![PathBuf::from(format!("{first}\\"))];
    for part in parts {
        let mut next = Vec::new();
        for base in &current {
            if part.contains(['*', '?']) {
                let Ok(entries) = fs::read_dir(base) else { continue };
                next.extend(entries.flatten().filter(|entry| wild_match(part, &entry.file_name().to_string_lossy())).map(|entry| entry.path()));
            } else {
                next.push(base.join(part));
            }
        }
        current = next;
    }
    current.retain(|path| path.exists());
    current
}

fn places(app: &AppHandle) -> Vec<(&'static str, String)> {
    let paths = app.path();
    let text = |path: Option<PathBuf>| path.map_or(String::new(), |path| path.to_string_lossy().into_owned());
    let local = paths.local_data_dir().ok();
    let env = |name: &str| std::env::var(name).unwrap_or_default();
    vec![
        ("<home>", text(paths.home_dir().ok())),
        ("<winAppData>", text(paths.data_dir().ok())),
        ("<winLocalAppData>", text(local.clone())),
        ("<winLocalAppDataLow>", text(local.and_then(|dir| Some(dir.parent()?.join("LocalLow"))))),
        ("<winDocuments>", text(paths.document_dir().ok())),
        ("<winPublic>", text(paths.public_dir().ok())),
        ("<winProgramData>", env("ProgramData")),
        ("<winDir>", env("windir")),
        ("<osUserName>", env("USERNAME")),
        ("<storeUserId>", "*".into()),
    ]
}

fn fill(path: &str, install_dir: &str, places: &[(&str, String)]) -> Option<String> {
    let mut filled = path.replace("<base>", install_dir);
    for (name, value) in places {
        filled = filled.replace(name, value);
    }
    (!filled.contains('<')).then_some(filled)
}

impl Saves {
    fn index(&self, app: &AppHandle) -> Option<Arc<Index>> {
        let mut loaded = self.0.lock().unwrap();
        if loaded.is_none() {
            let bytes = fs::read(list_path(app).ok()?).ok()?;
            let exact: HashMap<String, Locations> = serde_json::from_slice(&bytes).ok()?;
            let loose = exact.keys().map(|name| (loose_key(name), name.clone())).collect();
            *loaded = Some(Arc::new(Index { exact, loose }));
        }
        loaded.clone()
    }

    fn lookup(&self, app: &AppHandle, name: &str) -> Option<Locations> {
        let index = self.index(app)?;
        let key = index.exact.get(&name.to_lowercase()).or_else(|| index.exact.get(index.loose.get(&loose_key(name))?));
        key.cloned()
    }
}

fn locate(app: &AppHandle, game: &Game) -> (bool, Vec<PathBuf>) {
    if let Some(folder) = &game.save_folder {
        let path = PathBuf::from(folder);
        return (true, if path.exists() { vec![path] } else { Vec::new() });
    }
    let Some(listed) = app.state::<Saves>().lookup(app, &game.name) else { return (false, Vec::new()) };
    let places = places(app);
    let cracked = listed.steam.iter().flat_map(|id| CRACK_FOLDERS.iter().map(move |path| path.replace("{id}", &id.to_string())));
    let mut found: Vec<PathBuf> = listed.paths.iter().cloned().chain(cracked)
        .filter_map(|path| fill(&path, &game.install_dir, &places))
        .flat_map(|path| expand_glob(&path))
        .collect();
    found.sort();
    found.dedup();
    // A folder already covers anything inside it.
    let folders: Vec<PathBuf> = found.iter().filter(|path| path.is_dir()).cloned().collect();
    found.retain(|path| !path.ancestors().skip(1).any(|parent| folders.iter().any(|folder| folder == parent)));
    (true, found)
}

fn measure(path: &Path, bytes: &mut u64, newest: &mut u64) {
    let Ok(meta) = fs::metadata(path) else { return };
    if meta.is_dir() {
        for entry in fs::read_dir(path).into_iter().flatten().flatten() {
            measure(&entry.path(), bytes, newest);
        }
    } else {
        *bytes += meta.len();
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        *newest = (*newest).max(modified.duration_since(UNIX_EPOCH).map_or(0, |age| age.as_secs()));
    }
}

fn game_backups(app: &AppHandle, game: &Game) -> PathBuf {
    Path::new(&app.state::<Store>().get().backup_dir).join(folder_name(&game.name))
}

fn list_backups(folder: &Path) -> Vec<Backup> {
    let mut backups: Vec<Backup> = fs::read_dir(folder).into_iter().flatten().flatten()
        .filter_map(|entry| serde_json::from_slice(&fs::read(entry.path().join(BACKUP_FILE)).ok()?).ok())
        .collect();
    backups.sort_by(|a, b| b.created.cmp(&a.created));
    backups
}

fn prune(folder: &Path) {
    for old in list_backups(folder).into_iter().skip(KEEP) {
        let _ = fs::remove_dir_all(folder.join(old.created.to_string()));
    }
}

fn back_up(app: &AppHandle, game: &Game, label: &str) -> Result<Option<Backup>, String> {
    let (_, found) = locate(app, game);
    if found.is_empty() {
        return Ok(None);
    }
    let (mut bytes, mut newest) = (0, 0);
    found.iter().for_each(|path| measure(path, &mut bytes, &mut newest));
    if bytes > MAX_BYTES {
        return Err(format!("{}'s saves are over {} GB, so they weren't backed up", game.name, MAX_BYTES >> 30));
    }
    let folder = game_backups(app, game);
    if bytes > free_space(&folder) {
        return Err(format!("There isn't room to back up {}'s saves", game.name));
    }

    let created = now();
    let target = folder.join(created.to_string());
    fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    let mut roots = Vec::new();
    for (index, path) in found.iter().enumerate() {
        let name = path.file_name().map_or("save".into(), |name| name.to_os_string());
        let to = if path.is_dir() { target.join(index.to_string()) } else { target.join(index.to_string()).join(name) };
        if let Err(error) = copy_tree(path, &to, &mut |_| true) {
            let _ = fs::remove_dir_all(&target);
            return Err(error);
        }
        roots.push(Root { index, original: path.to_string_lossy().into_owned() });
    }
    // Written last, so a backup that was cut short never appears in the list.
    let backup = Backup { created, label: label.into(), bytes, roots };
    let record = serde_json::to_vec_pretty(&backup).map_err(|e| e.to_string())?;
    fs::write(target.join(BACKUP_FILE), record).map_err(|e| e.to_string())?;
    Ok(Some(backup))
}

pub fn status(app: &AppHandle, game: &Game) -> SaveStatus {
    let (listed, found) = locate(app, game);
    let folder = game_backups(app, game);
    SaveStatus {
        listed,
        found: found.iter().map(|path| path.to_string_lossy().into_owned()).collect(),
        backups: list_backups(&folder),
        folder: folder.to_string_lossy().into_owned(),
    }
}

pub fn back_up_now(app: &AppHandle, game: &Game) -> Result<(), String> {
    match back_up(app, game, "Backed up by you")? {
        Some(_) => {
            prune(&game_backups(app, game));
            Ok(())
        }
        None => Err("GaLa couldn't find any saves for this game".into()),
    }
}

/// Current saves are backed up first, so a restore can always be undone.
pub fn restore(app: &AppHandle, game: &Game, created: i64) -> Result<(), String> {
    if app.state::<Player>().running().is_some_and(|running| running.game_id == game.id) {
        return Err("Close the game before restoring its saves".into());
    }
    let folder = game_backups(app, game);
    let chosen = list_backups(&folder).into_iter().find(|backup| backup.created == created).ok_or("That backup is no longer there")?;
    back_up(app, game, "Before restoring")?;
    let source = folder.join(created.to_string());
    for root in &chosen.roots {
        let stored = source.join(root.index.to_string());
        let original = Path::new(&root.original);
        if let Some(parent) = original.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let from = match original.file_name() {
            Some(name) if stored.join(name).is_file() => stored.join(name),
            _ => stored,
        };
        copy_tree(&from, original, &mut |_| true)?;
    }
    prune(&folder);
    Ok(())
}

/// Called when a game closes. Saves that haven't changed since the last backup are left alone.
pub fn after_playing(app: &AppHandle, game_id: i64) {
    let Ok(Some(game)) = app.state::<Db>().game(game_id) else { return };
    let (_, found) = locate(app, &game);
    let (mut bytes, mut newest) = (0, 0);
    found.iter().for_each(|path| measure(path, &mut bytes, &mut newest));
    let folder = game_backups(app, &game);
    if list_backups(&folder).first().is_some_and(|last| last.created >= newest as i64) {
        return;
    }
    match back_up(app, &game, "After playing") {
        Ok(_) => prune(&folder),
        Err(message) => {
            let _ = app.emit("engine-event", json!({ "event": "error", "id": "", "message": message }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"
Some Game:
  files:
    "<winAppData>/Some/<storeUserId>":
      tags:
        - save
      when:
        - os: windows
    "<winAppData>/Some/settings.ini":
      tags:
        - config
    "<home>/Library/Some":
      tags:
        - save
      when:
        - os: mac
    "<winLocalAppData>/Packages/Some":
      tags:
        - save
      when:
        - os: windows
          store: microsoft
  steam:
    id: 42
Other Name:
  alias: Some Game
Config Only:
  files:
    "<base>/settings.ini":
      tags:
        - config
"#;

    #[test]
    fn keeps_only_windows_saves() {
        let games = reduce(LIST).unwrap();
        assert_eq!(games["some game"].paths, ["<winAppData>/Some/<storeUserId>"]);
        assert_eq!(games["some game"].steam, Some(42));
        assert_eq!(games["other name"].paths, ["<winAppData>/Some/<storeUserId>"]);
        assert!(!games.contains_key("config only"));
    }

    #[test]
    fn matches_wildcards() {
        assert!(wild_match("Profile*.sav", "profile1.SAV"));
        assert!(wild_match("*", "anything"));
        assert!(wild_match("a?c*", "abcd"));
        assert!(!wild_match("Profile*.sav", "Global.sjson"));
        assert!(!wild_match("abc", "abcd"));
    }

    #[test]
    fn fills_in_folders() {
        let places = [("<winAppData>", r"C:\Users\me\AppData\Roaming".to_string()), ("<storeUserId>", "*".to_string())];
        assert_eq!(fill("<base>/saves", r"E:\Game", &places).unwrap(), r"E:\Game/saves");
        assert_eq!(fill("<winAppData>/Ring/<storeUserId>", "", &places).unwrap(), r"C:\Users\me\AppData\Roaming/Ring/*");
        assert!(fill("<root>/saves", "", &places).is_none());
    }

    #[test]
    fn finds_saves_on_disk() {
        let root = std::env::temp_dir().join(format!("gala-glob-{}", std::process::id()));
        fs::create_dir_all(root.join("Ring/7656")).unwrap();
        fs::write(root.join("Ring/7656/ER0000.sl2"), b"x").unwrap();
        let found = expand_glob(&format!("{}/Ring/*/ER*.sl2", root.display()));
        assert_eq!(found.len(), 1);
        assert!(expand_glob(&format!("{}/Missing/*", root.display())).is_empty());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn matches_names_loosely() {
        assert_eq!(loose_key("Hades II"), loose_key("HADES 2"));
    }

    #[test]
    fn a_backup_restores_to_where_it_came_from() {
        let root = std::env::temp_dir().join(format!("gala-saves-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let original = root.join("live/slot.sav");
        fs::create_dir_all(original.parent().unwrap()).unwrap();
        fs::write(&original, b"one").unwrap();
        let stored = root.join("backup/0/slot.sav");
        copy_tree(&original, &stored, &mut |_| true).unwrap();
        fs::write(&original, b"two").unwrap();
        copy_tree(&stored, &original, &mut |_| true).unwrap();
        assert_eq!(fs::read(&original).unwrap(), b"one");
        fs::remove_dir_all(&root).unwrap();
    }
}
