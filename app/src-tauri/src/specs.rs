//! "Can your PC run this?": this PC's parts, read from Windows, against a game's Steam
//! requirements. Requirements are kept in the database and refreshed monthly.

use crate::{
    db::Db,
    hardware::{cpu_score, gpu_score, parse_level, required_cpu, required_gpu, Level},
    installer::free_space,
    metadata::{clean_name, similarity, ACCEPTED},
    scanner::{registry, registry_value},
    settings::Store,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{mem::{size_of, zeroed}, path::Path, sync::OnceLock, time::Duration};
use tauri::{AppHandle, Emitter, Manager};
use tokio::time;
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

const DISPLAY_CLASS: &str = r"HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
const STEAM_DETAILS: &str = "https://store.steampowered.com/api/appdetails";
const STEAM_SEARCH: &str = "https://store.steampowered.com/api/storesearch/";
const STEAM_LAUNCH: &str = "steam://rungameid/";
const REFRESH_AFTER: i64 = 30 * 86400;
const BETWEEN_GAMES: Duration = Duration::from_millis(1500);
const IDLE: Duration = Duration::from_secs(10 * 60);
const GB: f64 = 1024.0 * 1024.0 * 1024.0;
const WINDOWS_11_BUILD: u32 = 22000;
// Cards from about the GTX 600 and HD 7000 on run DirectX 12.
const DIRECTX_12_SCORE: f64 = 1.8;

#[derive(Clone, Serialize)]
pub struct Pc {
    pub cpu: String,
    pub cpu_score: Option<f64>,
    pub gpu: String,
    pub gpu_score: Option<f64>,
    pub vram_gb: f64,
    pub ram_gb: f64,
    pub windows: u32,
}

#[derive(Serialize)]
pub struct Row {
    pub part: String,
    pub yours: String,
    pub minimum: Option<String>,
    pub recommended: Option<String>,
    pub meets_minimum: Option<bool>,
    pub meets_recommended: Option<bool>,
}

#[derive(Serialize)]
pub struct Check {
    pub steam_id: String,
    pub name: String,
    /// "well", "lower", "below" or "unknown".
    pub verdict: String,
    pub summary: String,
    pub rows: Vec<Row>,
    pub minimum: Vec<String>,
    pub recommended: Vec<String>,
}

/// What GaLa stores per game: the Steam app and its requirement HTML, or nothing found.
pub struct Stored {
    pub steam_id: Option<String>,
    pub name: String,
    pub minimum: String,
    pub recommended: String,
    pub fetched: i64,
}

#[derive(Deserialize)]
struct SearchReply {
    #[serde(default)]
    items: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    id: i64,
    name: String,
}

fn ram_gb() -> f64 {
    let mut status: MEMORYSTATUSEX = unsafe { zeroed() };
    status.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return 0.0;
    }
    status.ullTotalPhys as f64 / GB
}

fn hex_number(text: &str) -> Option<u64> {
    let text = text.trim();
    text.strip_prefix("0x").map_or_else(|| text.parse().ok(), |hex| u64::from_str_radix(hex, 16).ok())
}

/// Every display adapter Windows knows, with its video memory where the driver reports it.
fn graphics_cards() -> Vec<(String, f64)> {
    let output = registry(&["query", DISPLAY_CLASS, "/s"]);
    let mut cards = Vec::new();
    let mut name: Option<String> = None;
    let mut memory = 0u64;
    let mut finish = |name: &mut Option<String>, memory: &mut u64| {
        if let Some(found) = name.take() {
            cards.push((found, *memory as f64 / GB));
        }
        *memory = 0;
    };
    for line in output.lines() {
        if line.starts_with("HKEY") {
            finish(&mut name, &mut memory);
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let Some(&key) = fields.first() else { continue };
        let value = line.split_once("REG_").and_then(|(_, rest)| rest.split_once(' ')).map(|(_, v)| v.trim()).unwrap_or_default();
        match key {
            "DriverDesc" => name = Some(value.to_string()),
            "HardwareInformation.qwMemorySize" => memory = hex_number(value).unwrap_or(0),
            "HardwareInformation.MemorySize" if memory == 0 => {
                memory = if line.contains("REG_BINARY") {
                    // Little-endian bytes, written as one hex string.
                    let bytes: Vec<u8> = (0..value.len()).step_by(2).filter_map(|i| u8::from_str_radix(value.get(i..i + 2)?, 16).ok()).collect();
                    bytes.iter().rev().fold(0u64, |total, byte| total << 8 | u64::from(*byte))
                } else {
                    hex_number(value).unwrap_or(0)
                };
            }
            _ => {}
        }
    }
    finish(&mut name, &mut memory);
    cards.retain(|(name, _)| !name.to_lowercase().contains("basic display") && !name.to_lowercase().contains("remote"));
    cards
}

/// Read once: the parts don't change while GaLa runs.
pub fn pc() -> &'static Pc {
    static PC: OnceLock<Pc> = OnceLock::new();
    PC.get_or_init(|| {
        let cpu = registry_value(r"HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0", "ProcessorNameString")
            .map(|name| name.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_else(|| "Unknown processor".into());
        // A laptop has its built-in graphics and the real card; games use the stronger one.
        let (gpu, vram_gb) = graphics_cards()
            .into_iter()
            .max_by(|a, b| gpu_score(&a.0).unwrap_or(0.0).total_cmp(&gpu_score(&b.0).unwrap_or(0.0)).then(a.1.total_cmp(&b.1)))
            .unwrap_or_else(|| ("Unknown graphics card".into(), 0.0));
        let build: u32 = registry_value(r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuildNumber")
            .and_then(|build| build.parse().ok())
            .unwrap_or(0);
        Pc {
            cpu_score: cpu_score(&cpu),
            gpu_score: gpu_score(&gpu),
            cpu,
            gpu,
            vram_gb,
            ram_gb: ram_gb(),
            windows: if build >= WINDOWS_11_BUILD { 11 } else { 10 },
        }
    })
}

/// "12th Gen Intel(R) Core(TM) i5-12450H" reads as "Intel Core i5-12450H".
fn short_cpu(name: &str) -> String {
    let name = name.replace("(R)", "").replace("(TM)", "").replace("(tm)", "");
    let name = name.split(" @ ").next().unwrap_or(&name);
    let words: Vec<&str> = name
        .split_whitespace()
        .filter(|word| !matches!(*word, "CPU" | "Processor" | "Gen") && !word.ends_with("th") && !word.ends_with("-Core"))
        .collect();
    words.join(" ")
}

/// "NVIDIA GeForce RTX 3050 Laptop GPU" reads as "GeForce RTX 3050 Laptop".
fn short_gpu(name: &str) -> String {
    let name = name.replace("(R)", "").replace("(TM)", "");
    let words: Vec<&str> = name.split_whitespace().filter(|word| !matches!(*word, "NVIDIA" | "AMD" | "GPU")).collect();
    words.join(" ")
}

/// Windows keeps a little memory back, so a 16 GB PC reports about 15.7 GB.
fn installed_ram(reported: f64) -> f64 {
    (reported + 0.5).floor()
}

fn gb(value: f64) -> String {
    if value >= 10.0 || value.fract() < 0.05 { format!("{value:.0} GB") } else { format!("{value:.1} GB") }
}

fn meets_os(text: &str, windows: u32) -> Option<bool> {
    let lower = text.to_lowercase();
    if lower.contains("windows 11") && !lower.contains("windows 10") {
        return Some(windows >= 11);
    }
    lower.contains("windows").then_some(true)
}

/// Compares one part against both levels. `check` returns None when it can't read the requirement.
fn row(part: &str, yours: String, minimum: Option<&String>, recommended: Option<&String>, check: impl Fn(&str) -> Option<bool>) -> Option<Row> {
    if minimum.is_none() && recommended.is_none() {
        return None;
    }
    Some(Row {
        part: part.into(),
        yours,
        meets_minimum: minimum.and_then(|text| check(text)),
        meets_recommended: recommended.and_then(|text| check(text)),
        minimum: minimum.cloned(),
        recommended: recommended.cloned(),
    })
}

fn number_row(part: &str, yours: f64, label: String, minimum: Option<f64>, recommended: Option<f64>) -> Option<Row> {
    let text = |value: Option<f64>| value.map(gb);
    row(part, label, text(minimum).as_ref(), text(recommended).as_ref(), |required| {
        let needed = [minimum, recommended].into_iter().flatten().find(|value| gb(*value) == required)?;
        Some(yours + 1e-6 >= needed)
    })
}

/// `free` is the space on the drive a game would install to, when it isn't installed yet.
pub fn compare(pc: &Pc, minimum: &Level, recommended: &Level, free: Option<u64>) -> (String, String, Vec<Row>) {
    let mut rows = Vec::new();
    let score_check = |mine: Option<f64>, required: fn(&str) -> Option<f64>| move |text: &str| Some(mine? >= required(text)?);
    rows.extend(row("Processor", short_cpu(&pc.cpu), minimum.cpu.as_ref(), recommended.cpu.as_ref(), score_check(pc.cpu_score, required_cpu)));
    rows.extend(row("Graphics", short_gpu(&pc.gpu), minimum.gpu.as_ref(), recommended.gpu.as_ref(), |text| {
        match required_gpu(text) {
            Some(needed) => Some(pc.gpu_score? >= needed),
            // A requirement that only asks for video memory is checked on that instead.
            None => None,
        }
    }));
    if minimum.vram_gb.is_some() || recommended.vram_gb.is_some() {
        // When the requirement only asks for video memory, that row says it all.
        rows.retain(|row| row.part != "Graphics" || row.meets_minimum.is_some() || row.meets_recommended.is_some());
        rows.extend(number_row("Video memory", pc.vram_gb, gb(pc.vram_gb), minimum.vram_gb, recommended.vram_gb));
    }
    let ram = installed_ram(pc.ram_gb);
    rows.extend(number_row("Memory", ram, gb(ram), minimum.memory_gb, recommended.memory_gb));
    rows.extend(row("Windows", format!("Windows {}", pc.windows), minimum.os.as_ref(), recommended.os.as_ref(), |text| meets_os(text, pc.windows)));
    let directx = if pc.windows >= 10 && pc.gpu_score.unwrap_or(0.0) >= DIRECTX_12_SCORE { 12 } else { 11 };
    let dx = |level: &Level| level.directx.map(|version| format!("Version {version}"));
    rows.extend(row("DirectX", format!("Version {directx}"), dx(minimum).as_ref(), dx(recommended).as_ref(), |text| {
        Some(directx >= text.trim_start_matches("Version ").parse::<u32>().ok()?)
    }));
    if let Some(free) = free {
        let free = free as f64 / GB;
        rows.extend(number_row("Storage", free, format!("{} free", gb(free)), minimum.storage_gb, recommended.storage_gb));
    }

    let below: Vec<&str> = rows.iter().filter(|row| row.meets_minimum == Some(false)).map(|row| row.part.as_str()).collect();
    let short: Vec<&str> = rows.iter().filter(|row| row.meets_recommended == Some(false) && row.meets_minimum != Some(false)).map(|row| row.part.as_str()).collect();
    let unread = |part: &str| rows.iter().find(|row| row.part == part).is_none_or(|row| row.meets_minimum.is_none());
    let cant_check: Vec<&str> = ["Processor", "Graphics"].into_iter().filter(|part| unread(part)).collect();
    let graphics_known = !unread("Graphics") || rows.iter().any(|row| row.part == "Video memory" && row.meets_minimum.is_some());

    let parts = |list: &[&str]| list.join(", ").to_lowercase().replace("directx", "DirectX").replace("windows", "Windows");
    let (verdict, mut summary) = if !below.is_empty() {
        ("below", format!("Below minimum: {}", parts(&below)))
    } else if cant_check.len() == 2 && !graphics_known {
        ("unknown", "Can't tell".to_string())
    } else if !short.is_empty() {
        ("lower", format!("Runs it, at lower settings. Below recommended: {}", parts(&short)))
    } else {
        ("well", "Runs it well".to_string())
    };
    let unchecked: Vec<&str> = cant_check.into_iter().filter(|part| *part != "Graphics" || !graphics_known).collect();
    if verdict != "unknown" && !unchecked.is_empty() {
        summary.push_str(&format!(". Couldn't check the {}", parts(&unchecked)));
    }
    (verdict.into(), summary, rows)
}

pub fn build(stored: &Stored, free: Option<u64>) -> Option<Check> {
    let steam_id = stored.steam_id.clone()?;
    if stored.minimum.is_empty() && stored.recommended.is_empty() {
        return None;
    }
    let (minimum, recommended) = (parse_level(&stored.minimum), parse_level(&stored.recommended));
    let (verdict, summary, rows) = compare(pc(), &minimum, &recommended, free);
    Some(Check { steam_id, name: stored.name.clone(), verdict, summary, rows, minimum: minimum.lines, recommended: recommended.lines })
}

fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| reqwest::Client::builder().user_agent("GaLa/0.1").timeout(Duration::from_secs(20)).build().expect("the HTTP client failed to start"))
}

async fn get_json(url: &str) -> Result<Value, String> {
    let response = http().get(url).send().await.map_err(|e| e.without_url().to_string())?;
    response.error_for_status().map_err(|e| e.without_url().to_string())?.json().await.map_err(|e| e.to_string())
}

/// The Steam app for a game name, when Steam sells one by close to that name.
async fn find_on_steam(name: &str) -> Result<Option<String>, String> {
    let query = clean_name(name);
    let query = if query.is_empty() { name.to_string() } else { query };
    let url = format!("{STEAM_SEARCH}?term={}&cc=us&l=en", query.replace(' ', "%20").replace('&', "%26"));
    let reply: SearchReply = serde_json::from_value(get_json(&url).await?).map_err(|e| e.to_string())?;
    Ok(reply
        .items
        .into_iter()
        .map(|item| (similarity(&query, &item.name), item))
        .filter(|(score, _)| *score >= ACCEPTED)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, item)| item.id.to_string()))
}

async fn fetch_requirements(steam_id: &str) -> Result<Stored, String> {
    let reply = get_json(&format!("{STEAM_DETAILS}?appids={steam_id}&cc=us&l=en")).await?;
    let data = &reply[steam_id]["data"];
    // Games without requirements give an empty list instead of an object.
    let level = |key: &str| data["pc_requirements"][key].as_str().unwrap_or_default().to_string();
    Ok(Stored {
        steam_id: Some(steam_id.to_string()),
        name: data["name"].as_str().unwrap_or_default().to_string(),
        minimum: level("minimum"),
        recommended: level("recommended"),
        fetched: crate::db::now(),
    })
}

/// Looks up a game's requirements by its Steam id, or else by name.
pub async fn lookup(launch_uri: Option<&str>, name: &str) -> Result<Stored, String> {
    let known = launch_uri.and_then(|uri| uri.strip_prefix(STEAM_LAUNCH)).filter(|id| id.chars().all(|c| c.is_ascii_digit())).map(str::to_string);
    let steam_id = match known {
        Some(id) => Some(id),
        None => find_on_steam(name).await?,
    };
    match steam_id {
        Some(id) => fetch_requirements(&id).await,
        None => Ok(Stored { steam_id: None, name: String::new(), minimum: String::new(), recommended: String::new(), fetched: crate::db::now() }),
    }
}

/// Storage only matters before a game is installed.
fn free_for(app: &AppHandle, installed: bool) -> Option<u64> {
    (!installed).then(|| free_space(Path::new(&app.state::<Store>().get().install_dir)))
}

pub async fn check_game(app: &AppHandle, game_id: i64) -> Result<Option<Check>, String> {
    let db = app.state::<Db>();
    let game = db.game(game_id).map_err(|e| e.to_string())?.ok_or("That game is no longer in your library")?;
    let stored = match db.requirements(game_id).map_err(|e| e.to_string())? {
        Some(stored) if crate::db::now() - stored.fetched < REFRESH_AFTER => stored,
        _ => {
            let stored = lookup(game.launch_uri.as_deref(), &game.name).await?;
            db.set_requirements(game_id, &stored).map_err(|e| e.to_string())?;
            stored
        }
    };
    let installed = game.exe_path.is_some() || game.launch_uri.is_some();
    Ok(build(&stored, free_for(app, installed)))
}

pub async fn check_name(app: &AppHandle, name: &str) -> Result<Option<Check>, String> {
    let stored = lookup(None, name).await?;
    Ok(build(&stored, free_for(app, false)))
}

/// Verdicts for every game with stored requirements, for the library badges.
pub fn verdicts(app: &AppHandle) -> Vec<(i64, String, String)> {
    let db = app.state::<Db>();
    let games = db.games().unwrap_or_default();
    db.all_requirements()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(game_id, stored)| {
            let game = games.iter().find(|game| game.id == game_id)?;
            let installed = game.exe_path.is_some() || game.launch_uri.is_some();
            let check = build(&stored, free_for(app, installed))?;
            Some((game_id, check.verdict, check.summary))
        })
        .collect()
}

/// Fills in requirements for games that don't have them yet, one every 1.5 seconds.
pub fn start_worker(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        time::sleep(Duration::from_secs(20)).await;
        loop {
            let db = app.state::<Db>();
            let stale = db.games_needing_requirements(crate::db::now() - REFRESH_AFTER).unwrap_or_default();
            let mut changed = false;
            for (game_id, launch_uri, name) in stale {
                match lookup(launch_uri.as_deref(), &name).await {
                    Ok(stored) => changed |= db.set_requirements(game_id, &stored).is_ok(),
                    Err(error) => {
                        eprintln!("requirements for {name}: {error}");
                        break;
                    }
                }
                time::sleep(BETWEEN_GAMES).await;
            }
            if changed {
                let _ = app.emit("requirements-changed", ());
            }
            time::sleep(IDLE).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn laptop() -> Pc {
        Pc {
            cpu: "12th Gen Intel(R) Core(TM) i5-12450H".into(),
            cpu_score: cpu_score("12th Gen Intel(R) Core(TM) i5-12450H"),
            gpu: "NVIDIA GeForce RTX 3050 Laptop GPU".into(),
            gpu_score: gpu_score("NVIDIA GeForce RTX 3050 Laptop GPU"),
            vram_gb: 4.0,
            ram_gb: 15.7,
            windows: 11,
        }
    }

    fn level(cpu: &str, ram: f64, gpu: &str) -> Level {
        Level {
            os: Some("Windows 10 64-bit".into()),
            cpu: Some(cpu.into()),
            memory_gb: Some(ram),
            gpu: Some(gpu.into()),
            directx: Some(12),
            ..Level::default()
        }
    }

    #[test]
    fn shortens_part_names() {
        assert_eq!(short_cpu("12th Gen Intel(R) Core(TM) i5-12450H"), "Intel Core i5-12450H");
        assert_eq!(short_cpu("AMD Ryzen 7 5800X 8-Core Processor"), "AMD Ryzen 7 5800X");
        assert_eq!(short_cpu("Intel(R) Core(TM) i7-8700 CPU @ 3.20GHz"), "Intel Core i7-8700");
        assert_eq!(short_gpu("NVIDIA GeForce RTX 3050 Laptop GPU"), "GeForce RTX 3050 Laptop");
        assert_eq!(short_gpu("AMD Radeon RX 6700 XT"), "Radeon RX 6700 XT");
    }

    #[test]
    fn the_finals_runs_at_lower_settings() {
        let minimum = level("Intel Core i5-6600K or AMD Ryzen R5 1600 processor", 12.0, "NVIDIA GeForce GTX 1050 Ti or AMD Radeon RX 580");
        let recommended = level("Intel Core i5-9600K or AMD Ryzen 5 3600 processor", 16.0, "NVIDIA GeForce RTX 2070 or AMD Radeon RX 5700 XT");
        let (verdict, summary, rows) = compare(&laptop(), &minimum, &recommended, None);
        assert_eq!(verdict, "lower", "{summary}");
        assert_eq!(summary, "Runs it, at lower settings. Below recommended: graphics");
        let memory = rows.iter().find(|row| row.part == "Memory").unwrap();
        assert_eq!((memory.meets_minimum, memory.meets_recommended), (Some(true), Some(true)), "15.7 GB counts as 16");
    }

    #[test]
    fn hades_runs_well() {
        let mut minimum = level("Dual Core 2.4 GHz", 4.0, "1GB VRAM / DirectX 10+ support");
        minimum.vram_gb = Some(1.0);
        minimum.directx = Some(10);
        let mut recommended = level("Dual Core 3.0 GHz+", 8.0, "2GB VRAM / DirectX 10+ support");
        recommended.vram_gb = Some(2.0);
        recommended.directx = Some(10);
        let (verdict, summary, _) = compare(&laptop(), &minimum, &recommended, Some(200 * 1024 * 1024 * 1024));
        assert_eq!(verdict, "well", "{summary}");
    }

    #[test]
    fn too_little_space_or_memory_is_below_minimum() {
        let minimum = Level { memory_gb: Some(32.0), storage_gb: Some(150.0), ..level("Core i5-6600K", 32.0, "GTX 1050 Ti") };
        let (verdict, summary, _) = compare(&laptop(), &minimum, &Level::default(), Some(100 * 1024 * 1024 * 1024));
        assert_eq!(verdict, "below");
        assert_eq!(summary, "Below minimum: memory, storage");
    }

    #[test]
    fn unreadable_parts_say_so() {
        let minimum = level("Something fast", 8.0, "A good graphics card");
        let (verdict, _, _) = compare(&laptop(), &minimum, &Level::default(), None);
        assert_eq!(verdict, "unknown");
    }
}
