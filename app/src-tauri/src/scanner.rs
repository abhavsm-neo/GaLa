//! Finds games already on disk. Games owned by a launcher are recorded with the link that
//! opens them there. Files are only ever read, and nothing joins the library until chosen.

use crate::{
    db::Db,
    installer::{scan, Source},
    library,
    metadata::clean_name,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DEFAULT_STEAM: &str = r"C:\Program Files (x86)\Steam";
const EPIC_MANIFESTS: &str = r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests";
const UBISOFT_INSTALLS: &str = r"HKLM\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs";
const STEAM_REDISTRIBUTABLES: &str = "228980";
const STEAM_INSTALLED_FLAG: u64 = 4;
const SKIP_FOLDERS: &[&str] = &[
    "windows", "program files", "program files (x86)", "programdata", "users", "system volume information",
    "steamworks shared", "steam controller configs", "gala", "node_modules",
];

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Found {
    pub name: String,
    pub folder: String,
    pub exe: Option<String>,
    pub launcher: String,
    pub launch_uri: Option<String>,
    pub installable: bool,
    pub sure: bool,
    pub added: bool,
}

#[derive(Default)]
struct Scan {
    games: Vec<Found>,
    seen: HashSet<String>,
    steam_seen: HashSet<PathBuf>,
}

fn tidy(name: &str) -> String {
    name.chars().filter(|c| !matches!(c, '®' | '™' | '©')).collect::<String>().trim().to_string()
}

impl Scan {
    fn push(&mut self, game: Found) {
        if self.seen.insert(game.folder.trim_end_matches('\\').to_lowercase()) {
            self.games.push(game);
        }
    }

    fn launcher_game(&mut self, name: &str, folder: &Path, launcher: &str, uri: String) {
        self.push(Found {
            name: tidy(name),
            folder: folder.to_string_lossy().into_owned(),
            launcher: launcher.into(),
            launch_uri: Some(uri),
            sure: true,
            ..Found::default()
        });
    }

    fn steam_library(&mut self, library: &Path) {
        let apps = library.join("steamapps");
        if !self.steam_seen.insert(apps.clone()) {
            return;
        }
        let common = apps.join("common");
        for entry in fs::read_dir(&apps).into_iter().flatten().flatten() {
            let file = entry.file_name().to_string_lossy().to_lowercase();
            if !(file.starts_with("appmanifest_") && file.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = fs::read_to_string(entry.path()) else { continue };
            if let Some((id, name, folder)) = parse_steam_app(&text) {
                self.launcher_game(&name, &common.join(folder), "Steam", format!("steam://rungameid/{id}"));
            }
        }
        // Folders Steam doesn't list are leftovers or games copied in by hand.
        self.subfolders(&common);
    }

    fn folder(&mut self, path: &Path) {
        if path.join("steamapps").is_dir() {
            return self.steam_library(path);
        }
        if path.join(".build.info").is_file() {
            let name = path.file_name().map_or("Game".into(), |n| n.to_string_lossy().into_owned());
            let product = fs::read_to_string(path.join(".build.info")).ok().and_then(|text| battlenet_product(&text));
            let uri = format!("battlenet://{}", product.unwrap_or_default());
            return self.launcher_game(&name, path, "Battle.net", uri);
        }
        // Epic games are listed by the launcher itself, and this folder has none of its details.
        if path.join(".egstore").is_dir() {
            return;
        }
        if is_collection(path) {
            return self.subfolders(path);
        }
        self.plain_game(path);
    }

    fn plain_game(&mut self, path: &Path) {
        let folder = path.to_string_lossy().into_owned();
        let name = clean_name(&path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned()));
        if name.is_empty() {
            return;
        }
        let confident = library::confident_exe(&folder, &name);
        // A lone tool buried in a subfolder isn't a game, so that one waits for a person to check.
        let sure = confident.as_deref().is_some_and(|exe| {
            let exe = Path::new(exe);
            let stem = exe.file_stem().map_or(String::new(), |s| library::letters(&s.to_string_lossy()));
            exe.parent() == Some(path) || stem.contains(&library::letters(&name))
        });
        let exe = confident.or_else(|| library::find_exes(&folder, &name).into_iter().next());
        let installable = exe.is_none() && !matches!(scan(path, &name, &HashSet::new()), Source::Unknown);
        if exe.is_some() || installable {
            self.push(Found { name, folder, exe, installable, sure, ..Found::default() });
        }
    }

    fn subfolders(&mut self, parent: &Path) {
        let Ok(entries) = fs::read_dir(parent) else { return };
        let mut folders: Vec<PathBuf> = entries.flatten().filter(|entry| entry.path().is_dir()).map(|entry| entry.path()).collect();
        folders.sort();
        for path in folders {
            let name = path.file_name().map_or(String::new(), |n| n.to_string_lossy().to_lowercase());
            if !SKIP_FOLDERS.contains(&name.as_str()) && !name.starts_with(['$', '.']) {
                self.folder(&path);
            }
        }
    }

    fn chosen_folder(&mut self, path: &Path) {
        let inside_steam = path.file_name().is_some_and(|name| name.eq_ignore_ascii_case("common"))
            && path.parent().is_some_and(|parent| parent.file_name().is_some_and(|name| name.eq_ignore_ascii_case("steamapps")));
        if inside_steam {
            if let Some(library) = path.parent().and_then(Path::parent) {
                return self.steam_library(library);
            }
        }
        let special = path.join("steamapps").is_dir() || path.join(".build.info").is_file();
        if special || holds_own_exe(path) {
            return self.folder(path);
        }
        // A loose trainer or tool doesn't make a folder of games into one game.
        let before = self.games.len();
        self.subfolders(path);
        if self.games.len() == before && !direct_exes(path).is_empty() {
            self.plain_game(path);
        }
    }

    fn installed_launchers(&mut self) {
        let root = registry_value(r"HKCU\Software\Valve\Steam", "SteamPath").unwrap_or_else(|| DEFAULT_STEAM.into());
        let root = PathBuf::from(root.replace('/', "\\"));
        let libraries = fs::read_to_string(root.join(r"steamapps\libraryfolders.vdf")).map_or(Vec::new(), |text| parse_steam_libraries(&text));
        for library in libraries.into_iter().map(PathBuf::from).chain([root]) {
            self.steam_library(&library);
        }

        for entry in fs::read_dir(EPIC_MANIFESTS).into_iter().flatten().flatten() {
            let Ok(text) = fs::read_to_string(entry.path()) else { continue };
            if let Some((name, folder, uri)) = parse_epic_item(&text) {
                if Path::new(&folder).is_dir() {
                    self.launcher_game(&name, Path::new(&folder), "Epic Games", uri);
                }
            }
        }

        for (id, folder) in ubisoft_installs() {
            let path = PathBuf::from(folder.replace('/', "\\"));
            let path = path.components().collect::<PathBuf>();
            if path.is_dir() {
                let name = path.file_name().map_or("Game".into(), |n| n.to_string_lossy().into_owned());
                self.launcher_game(&name, &path, "Ubisoft Connect", format!("uplay://launch/{id}/0"));
            }
        }
    }
}

fn direct_exes(folder: &Path) -> Vec<PathBuf> {
    fs::read_dir(folder).into_iter().flatten().flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")))
        .collect()
}

fn holds_own_exe(folder: &Path) -> bool {
    let name = folder.file_name().map_or(String::new(), |n| clean_name(&n.to_string_lossy()));
    let wanted = library::letters(&name);
    !wanted.is_empty() && direct_exes(folder).iter().any(|exe| {
        let stem = exe.file_stem().map_or(String::new(), |s| library::letters(&s.to_string_lossy()));
        !stem.is_empty() && (stem.contains(&wanted) || wanted.contains(&stem))
    })
}

/// A folder holding several games, as opposed to one game with some loose tools in it.
fn is_collection(folder: &Path) -> bool {
    !holds_own_exe(folder)
        && fs::read_dir(folder).into_iter().flatten().flatten().filter(|entry| entry.path().is_dir() && holds_own_exe(&entry.path())).count() >= 2
}

fn steam_value(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.trim().split('"').filter(|part| !part.trim().is_empty());
    Some((parts.next()?, parts.next()?))
}

fn parse_steam_app(text: &str) -> Option<(String, String, String)> {
    let find = |wanted: &str| text.lines().filter_map(steam_value).find(|(key, _)| key.eq_ignore_ascii_case(wanted)).map(|(_, value)| value.to_string());
    let (id, name, folder) = (find("appid")?, find("name")?, find("installdir")?);
    let flags: u64 = find("StateFlags").and_then(|flags| flags.parse().ok()).unwrap_or(0);
    (flags & STEAM_INSTALLED_FLAG != 0 && id != STEAM_REDISTRIBUTABLES).then_some((id, name, folder))
}

fn parse_steam_libraries(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(steam_value)
        .filter(|(key, _)| key.eq_ignore_ascii_case("path"))
        .map(|(_, value)| value.replace(r"\\", r"\"))
        .collect()
}

fn parse_epic_item(text: &str) -> Option<(String, String, String)> {
    let item: serde_json::Value = serde_json::from_str(text).ok()?;
    let games = item["AppCategories"].as_array()?.iter().any(|category| category == "games");
    if !games {
        return None;
    }
    let field = |key: &str| item[key].as_str().map(str::to_string);
    let uri = format!(
        "com.epicgames.launcher://apps/{}%3A{}%3A{}?action=launch&silent=true",
        field("CatalogNamespace")?,
        field("CatalogItemId")?,
        field("AppName")?,
    );
    Some((field("DisplayName")?, field("InstallLocation")?, uri))
}

/// The first line names the columns, and the first line without a `#` holds the values.
fn battlenet_product(text: &str) -> Option<String> {
    let mut lines = text.lines().filter(|line| !line.is_empty() && !line.starts_with('#'));
    let header = lines.next()?;
    let values = lines.next()?;
    let column = header.split('|').position(|name| name.starts_with("Product!"))?;
    values.split('|').nth(column).map(str::to_string).filter(|product| !product.is_empty())
}

fn registry(args: &[&str]) -> String {
    Command::new("reg")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_or(String::new(), |output| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn registry_value(key: &str, name: &str) -> Option<String> {
    registry(&["query", key, "/v", name]).lines().find_map(|line| Some(line.split_once("REG_SZ")?.1.trim().to_string()))
}

fn parse_ubisoft(text: &str) -> Vec<(String, String)> {
    let (mut id, mut installs) = (String::new(), Vec::new());
    for line in text.lines() {
        if line.starts_with("HKEY") {
            id = line.rsplit('\\').next().unwrap_or_default().trim().to_string();
        } else if let Some((_, folder)) = line.split_once("REG_SZ") {
            installs.push((id.clone(), folder.trim().to_string()));
        }
    }
    installs
}

fn ubisoft_installs() -> Vec<(String, String)> {
    parse_ubisoft(&registry(&["query", UBISOFT_INSTALLS, "/s", "/v", "InstallDir"]))
}

pub fn run(db: &Db, folders: &[String], launchers: bool) -> Result<Vec<Found>, String> {
    let mut found = Scan::default();
    if launchers {
        found.installed_launchers();
    }
    for folder in folders {
        found.chosen_folder(Path::new(folder));
    }
    let known = db.games().map_err(|e| e.to_string())?;
    for game in &mut found.games {
        game.added = known.iter().any(|other| {
            other.install_dir.trim_end_matches('\\').eq_ignore_ascii_case(game.folder.trim_end_matches('\\'))
                || (game.launch_uri.is_some() && other.launch_uri == game.launch_uri)
        });
    }
    found.games.sort_by_key(|game| (game.added, !game.sure, game.name.to_lowercase()));
    Ok(found.games)
}

pub fn add(db: &Db, games: &[Found]) -> Result<(), String> {
    for game in games {
        let id = db.add(None, &game.name, &game.folder, game.exe.as_deref()).map_err(|e| e.to_string())?;
        if let Some(uri) = &game.launch_uri {
            db.set_launcher(id, &game.launcher, uri).map_err(|e| e.to_string())?;
        } else if game.installable {
            db.mark_installable(id, &game.folder).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = "\"AppState\"\n{\n\t\"appid\"\t\t\"2073850\"\n\t\"name\"\t\t\"THE FINALS\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"The Finals\"\n}\n";

    #[test]
    fn reads_a_steam_game() {
        assert_eq!(parse_steam_app(APP), Some(("2073850".into(), "THE FINALS".into(), "The Finals".into())));
    }

    #[test]
    fn skips_steam_games_that_are_not_installed() {
        assert!(parse_steam_app(&APP.replace("\"4\"", "\"1026\"")).is_none());
        assert!(parse_steam_app(&APP.replace("\"4\"", "\"1542\"")).is_some());
        assert!(parse_steam_app(&APP.replace("2073850", STEAM_REDISTRIBUTABLES)).is_none());
    }

    #[test]
    fn reads_steam_libraries() {
        let text = "\"0\"\n{\n\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n}\n\"1\"\n{\n\t\"path\"\t\t\"E:\\\\SteamLibrary\"\n}\n";
        assert_eq!(parse_steam_libraries(text), [r"C:\Program Files (x86)\Steam", r"E:\SteamLibrary"]);
    }

    #[test]
    fn reads_an_epic_game_and_skips_other_apps() {
        let text = r#"{"AppCategories":["public","games"],"DisplayName":"Hades","InstallLocation":"C:\\Games\\Hades","CatalogNamespace":"ns","CatalogItemId":"item","AppName":"Min"}"#;
        let (name, folder, uri) = parse_epic_item(text).unwrap();
        assert_eq!((name.as_str(), folder.as_str()), ("Hades", r"C:\Games\Hades"));
        assert_eq!(uri, "com.epicgames.launcher://apps/ns%3Aitem%3AMin?action=launch&silent=true");
        assert!(parse_epic_item(&text.replace("games", "software")).is_none());
    }

    #[test]
    fn reads_a_battlenet_product() {
        let text = "Branch!STRING:0|Active!DEC:1|Product!STRING:0\n## seqn = 1\nus|1|fenris\n";
        assert_eq!(battlenet_product(text).as_deref(), Some("fenris"));
        assert!(battlenet_product("Branch!STRING:0\nus\n").is_none());
    }

    #[test]
    fn reads_ubisoft_installs() {
        let text = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Ubisoft\\Launcher\\Installs\\13504\r\n    InstallDir    REG_SZ    D:/Games/Valhalla/\r\n\r\n";
        assert_eq!(parse_ubisoft(text), [("13504".to_string(), "D:/Games/Valhalla/".to_string())]);
    }

    #[test]
    fn removes_trademark_signs() {
        assert_eq!(tidy("Titanfall® 2"), "Titanfall 2");
    }

    #[test]
    fn a_loose_tool_does_not_make_a_folder_one_game() {
        let root = std::env::temp_dir().join(format!("gala-loose-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let games = root.join("Games");
        fs::create_dir_all(games.join("PEAK")).unwrap();
        fs::write(games.join("PEAK/PEAK.exe"), b"MZ").unwrap();
        fs::create_dir_all(games.join("Risk of Rain 2")).unwrap();
        fs::write(games.join("Risk of Rain 2/Risk of Rain 2.exe"), b"MZ").unwrap();
        fs::write(games.join("Some Plus 46 Trainer.exe"), b"MZ").unwrap();

        let mut chosen = Scan::default();
        chosen.chosen_folder(&games);
        assert_eq!(chosen.games.len(), 2);

        let mut parent = Scan::default();
        parent.chosen_folder(&root);
        assert_eq!(parent.games.len(), 2, "games one level further down");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_lone_tool_in_a_subfolder_needs_checking() {
        let root = std::env::temp_dir().join(format!("gala-tool-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("SCHOOL/sidh")).unwrap();
        fs::write(root.join("SCHOOL/sidh/Admission Exam.exe"), b"MZ").unwrap();
        fs::create_dir_all(root.join("Broforce-AnkerGames/BroForce v3148")).unwrap();
        fs::write(root.join("Broforce-AnkerGames/BroForce v3148/Broforce.exe"), b"MZ").unwrap();
        let mut found = Scan::default();
        found.subfolders(&root);
        assert!(found.games.iter().find(|game| game.name == "SCHOOL").is_some_and(|game| !game.sure));
        assert!(found.games.iter().find(|game| game.name == "Broforce").is_some_and(|game| game.sure));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_chosen_game_folder_is_one_game() {
        let root = std::env::temp_dir().join(format!("gala-one-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let game = root.join("Big Walk");
        fs::create_dir_all(game.join("Big Walk_Data")).unwrap();
        fs::write(game.join("Big Walk.exe"), b"MZ").unwrap();
        fs::write(game.join("UnityCrashHandler64.exe"), b"MZ").unwrap();
        let mut found = Scan::default();
        found.chosen_folder(&game);
        assert_eq!(found.games.len(), 1);
        assert_eq!(found.games[0].name, "Big Walk");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn finds_games_in_a_mixed_folder() {
        let root = std::env::temp_dir().join(format!("gala-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Hollow Knight")).unwrap();
        fs::write(root.join("Hollow Knight/Hollow Knight.exe"), b"MZ").unwrap();
        fs::create_dir_all(root.join("Movies")).unwrap();
        fs::write(root.join("Movies/film.mkv"), b"x").unwrap();
        fs::create_dir_all(root.join("Oddball/bin")).unwrap();
        fs::write(root.join("Oddball/bin/run.exe"), b"MZ").unwrap();
        fs::write(root.join("Oddball/bin/editor.exe"), b"MZ").unwrap();
        fs::create_dir_all(root.join("Diablo IV")).unwrap();
        fs::write(root.join("Diablo IV/.build.info"), "Product!STRING:0\nfenris\n").unwrap();
        fs::create_dir_all(root.join("SteamLibrary/steamapps/common/Copied Game")).unwrap();
        fs::write(root.join("SteamLibrary/steamapps/appmanifest_1.acf"), APP).unwrap();
        fs::write(root.join("SteamLibrary/steamapps/common/Copied Game/Copied Game.exe"), b"MZ").unwrap();

        let mut found = Scan::default();
        found.chosen_folder(&root);
        let names = |wanted: &str| found.games.iter().find(|game| game.name == wanted);

        assert!(names("Hollow Knight").is_some_and(|game| game.sure && game.launcher.is_empty()));
        assert!(names("Movies").is_none());
        assert!(names("Oddball").is_some_and(|game| !game.sure && game.exe.is_some()));
        assert_eq!(names("Diablo IV").and_then(|game| game.launch_uri.as_deref()), Some("battlenet://fenris"));
        assert_eq!(names("THE FINALS").and_then(|game| game.launch_uri.as_deref()), Some("steam://rungameid/2073850"));
        assert!(names("Copied Game").is_some_and(|game| game.launcher.is_empty() && game.sure));
        fs::remove_dir_all(&root).unwrap();
    }
}
