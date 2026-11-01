//! Finds out what a game is: details from IGDB, art from SteamGridDB, all kept on disk.

use crate::{
    db::{Db, Details},
    library,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use tauri::{AppHandle, Manager};
use tokio::{sync::Notify, time};

const TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";
const GAMES_URL: &str = "https://api.igdb.com/v4/games";
const STEAMGRID_URL: &str = "https://www.steamgriddb.com/api/v2";
const IGDB_IMAGE_URL: &str = "https://images.igdb.com/igdb/image/upload";
const FIELDS: &str = "name,summary,first_release_date,total_rating,total_rating_count,game_type,\
    cover.image_id,artworks.image_id,screenshots.image_id,genres.name,game_modes.name,\
    involved_companies.developer,involved_companies.publisher,involved_companies.company.name";
const WINDOWS: i64 = 6;
const MAIN_GAME_TYPES: &[i64] = &[0, 4, 8, 9, 10, 11];
const ACCEPTED: f64 = 0.75;
const MAX_YEARS_APART: i64 = 2;
const SCREENSHOTS: usize = 6;
const RETRY: Duration = Duration::from_secs(5 * 60);
const BETWEEN_GAMES: Duration = Duration::from_millis(300);
const NO_KEYS: &str = "IGDB and SteamGridDB keys are missing from .env";

const GROUPS: &[&str] = &[
    "fitgirl", "dodi", "elamigos", "kaos", "xatab", "gog", "codex", "plaza", "cpy", "empress", "rune", "skidrow",
    "reloaded", "razor1911", "tenoke", "flt", "p2p", "tinyiso", "darksiders", "goldberg", "rg", "mechanics",
];
const NOISE: &[&str] = &[
    "repack", "repacks", "cracked", "crack", "incl", "including", "dlc", "dlcs", "multi", "x64", "x86", "win64",
    "win32", "windows", "pc", "rip", "steamrip", "portable", "update", "build", "selective", "download", "torrent",
];
const EDITIONS: &[&str] = &[
    "deluxe", "ultimate", "gold", "premium", "complete", "goty", "digital", "collectors", "standard", "special",
    "platinum", "anniversary", "game",
];
const SHORT_GENRES: &[(&str, &str)] = &[
    ("Role-playing (RPG)", "RPG"),
    ("Real Time Strategy (RTS)", "Strategy"),
    ("Turn-based strategy (TBS)", "Strategy"),
    ("Hack and slash/Beat 'em up", "Hack and slash"),
    ("Card & Board Game", "Card and board"),
    ("Quiz/Trivia", "Quiz"),
];

struct Keys {
    igdb_id: String,
    igdb_secret: String,
    steamgrid: String,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
struct Image {
    image_id: String,
}

#[derive(Deserialize)]
struct Involved {
    company: Named,
    #[serde(default)]
    developer: bool,
    #[serde(default)]
    publisher: bool,
}

#[derive(Deserialize)]
struct IgdbGame {
    id: i64,
    name: String,
    summary: Option<String>,
    first_release_date: Option<i64>,
    total_rating: Option<f64>,
    total_rating_count: Option<i64>,
    game_type: Option<i64>,
    cover: Option<Image>,
    #[serde(default)]
    artworks: Vec<Image>,
    #[serde(default)]
    screenshots: Vec<Image>,
    #[serde(default)]
    genres: Vec<Named>,
    #[serde(default)]
    game_modes: Vec<Named>,
    #[serde(default)]
    involved_companies: Vec<Involved>,
}

#[derive(Serialize)]
pub struct Match {
    pub igdb_id: i64,
    pub name: String,
    pub year: Option<i64>,
    pub developer: String,
    pub cover: Option<String>,
}

#[derive(Deserialize)]
struct Token {
    access_token: String,
    expires_in: i64,
}

#[derive(Deserialize)]
struct SteamGridGame {
    id: i64,
    name: String,
    release_date: Option<i64>,
}

#[derive(Deserialize)]
struct SteamGridImage {
    url: String,
}

#[derive(Deserialize)]
struct SteamGridList<T> {
    data: Vec<T>,
}

#[derive(Default)]
struct SteamGridArt {
    cover: Option<String>,
    hero: Option<String>,
    logo: Option<String>,
}

pub struct Metadata {
    http: reqwest::Client,
    token: Mutex<Option<(String, i64)>>,
    wake: Notify,
}

impl Default for Metadata {
    fn default() -> Self {
        let http = reqwest::Client::builder()
            .user_agent("GaLa/0.1")
            .timeout(Duration::from_secs(30))
            .build()
            .expect("the HTTP client failed to start");
        Self { http, token: Mutex::new(None), wake: Notify::new() }
    }
}

// The token request carries the secret in its address, so errors never show the address.
fn net(error: reqwest::Error) -> String {
    error.without_url().to_string()
}

fn parse_keys(text: &str) -> Option<Keys> {
    let find = |wanted: &str| {
        text.lines()
            .filter_map(|line| line.split_once('='))
            .find(|(key, _)| key.trim() == wanted)
            .map(|(_, value)| value.trim().trim_matches('"').to_string())
            .filter(|value| !value.is_empty())
    };
    Some(Keys {
        igdb_id: find("IGDB_CLIENT_ID")?,
        igdb_secret: find("IGDB_CLIENT_SECRET")?,
        steamgrid: find("STEAMGRIDDB_API_KEY")?,
    })
}

fn load_keys(app: &AppHandle) -> Option<Keys> {
    let installed = app.path().app_config_dir().ok().map(|dir| dir.join(".env"));
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.env");
    installed
        .into_iter()
        .chain([project])
        .find_map(|path| parse_keys(&fs::read_to_string(path).ok()?))
}

fn numeral(word: &str) -> &str {
    match word {
        "ii" => "2",
        "iii" => "3",
        "iv" => "4",
        "vi" => "6",
        "vii" => "7",
        "viii" => "8",
        "ix" => "9",
        other => other,
    }
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace('&', " and ")
        .replace('\'', "")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| numeral(word).to_string())
        .collect()
}

fn similarity(query: &str, name: &str) -> f64 {
    let (wanted, found) = (words(query), words(name));
    if wanted.is_empty() || found.is_empty() {
        return 0.0;
    }
    if wanted == found {
        return 1.0;
    }
    let common = wanted.iter().filter(|word| found.contains(word)).count();
    let overlap = 2.0 * common as f64 / (wanted.len() + found.len()) as f64;
    // "Witcher 3" should find "The Witcher 3: Wild Hunt".
    if common == wanted.len() { overlap.max(ACCEPTED) } else { overlap }
}

fn score(query: &str, game: &IgdbGame) -> f64 {
    let popularity = game.total_rating_count.unwrap_or(0).min(1000) as f64 / 1000.0;
    similarity(query, &game.name) + popularity * 0.05
}

fn is_main_game(game: &IgdbGame) -> bool {
    game.game_type.is_none_or(|kind| MAIN_GAME_TYPES.contains(&kind))
}

fn best(query: &str, games: Vec<IgdbGame>) -> Option<IgdbGame> {
    games
        .into_iter()
        .filter(|game| is_main_game(game) || similarity(query, &game.name) == 1.0)
        .filter(|game| similarity(query, &game.name) >= ACCEPTED)
        .max_by(|a, b| score(query, a).total_cmp(&score(query, b)))
}

fn strip_brackets(text: &str) -> String {
    let mut depth = 0usize;
    text.chars()
        .filter(|c| match c {
            '[' | '(' | '{' => {
                depth += 1;
                false
            }
            ']' | ')' | '}' => {
                depth = depth.saturating_sub(1);
                false
            }
            _ => depth == 0,
        })
        .collect()
}

fn is_version(word: &str) -> bool {
    let digits = word.strip_prefix(['v', 'V']).unwrap_or(word);
    let marked = digits.len() < word.len() || digits.contains('.');
    marked && digits.starts_with(|c: char| c.is_ascii_digit()) && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Takes a release name such as "Hades.II.v1.0-FitGirl Repack" back to "Hades II".
pub fn clean_name(raw: &str) -> String {
    let plain = strip_brackets(raw).replace(['_', '+'], " ");
    // A dot is a separator unless it sits inside a version number.
    let chars: Vec<char> = plain.chars().collect();
    let spaced: String = chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let inside_number = i > 0
                && chars[i - 1].is_ascii_digit()
                && chars.get(i + 1).is_some_and(char::is_ascii_digit);
            if c == '.' && !inside_number { ' ' } else { c }
        })
        .collect();

    let mut kept: Vec<String> = Vec::new();
    for word in spaced.split_whitespace() {
        // "Ring-RUNE" ends in a release group, "Spider-Man" doesn't.
        let (word, tail) = word.rsplit_once('-').map_or((word, ""), |(head, tail)| (head, tail));
        let tail_lower = tail.to_lowercase();
        let tail_noise = !tail.is_empty() && (GROUPS.contains(&tail_lower.as_str()) || NOISE.contains(&tail_lower.as_str()));
        let whole = if tail.is_empty() || tail_noise { word.to_string() } else { format!("{word}-{tail}") };
        let lower = whole.to_lowercase();
        let bare = lower.trim_matches(|c: char| !c.is_alphanumeric());
        let noise = bare.is_empty()
            || is_version(bare)
            || GROUPS.contains(&bare)
            || NOISE.contains(&bare)
            || bare.starts_with("multi") && bare[5..].chars().all(|c| c.is_ascii_digit());
        if bare == "edition" {
            if kept.last().is_some_and(|last| EDITIONS.contains(&last.to_lowercase().as_str())) {
                kept.pop();
            }
            break;
        }
        if noise && !kept.is_empty() {
            break;
        }
        if !noise {
            kept.push(whole);
        }
        if tail_noise {
            break;
        }
    }
    kept.join(" ").trim_end_matches([':', '-', ' ']).to_string()
}

fn year(timestamp: Option<i64>) -> Option<i64> {
    // Good to within a day, which is all a year label needs.
    timestamp.map(|seconds| 1970 + (seconds + 43_200) / 31_556_952)
}

fn igdb_image(size: &str, image: &Image) -> String {
    format!("{IGDB_IMAGE_URL}/{size}/{}.jpg", image.image_id)
}

fn companies(game: &IgdbGame, wanted: fn(&Involved) -> bool) -> String {
    let names: Vec<&str> = game.involved_companies.iter().filter(|c| wanted(c)).map(|c| c.company.name.as_str()).take(2).collect();
    names.join(", ")
}

fn genres(game: &IgdbGame) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for genre in &game.genres {
        let short = SHORT_GENRES.iter().find(|(long, _)| *long == genre.name).map_or(genre.name.as_str(), |(_, short)| short);
        if !names.iter().any(|name| name == short) {
            names.push(short.to_string());
        }
    }
    names
}

/// Sites list early access differently, so a couple of years apart still counts as one game.
fn pick_steamgrid<'a>(found: &'a [SteamGridGame], name: &str, released: Option<i64>) -> Option<&'a SteamGridGame> {
    let same_era = |game: &&SteamGridGame| match (year(released), year(game.release_date)) {
        (Some(a), Some(b)) => (a - b).abs() <= MAX_YEARS_APART,
        _ => true,
    };
    let exact = found.iter().filter(same_era).find(|game| similarity(name, &game.name) == 1.0);
    exact.or(found.first().filter(same_era).filter(|game| similarity(name, &game.name) >= ACCEPTED))
}

fn extension(url: &str) -> &str {
    match url.rsplit_once('.') {
        Some((_, ext)) if (3..=4).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric()) => ext,
        _ => "jpg",
    }
}

impl Metadata {
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    async fn access_token(&self, keys: &Keys) -> Result<String, String> {
        if let Some((token, expires)) = self.token.lock().unwrap().as_ref() {
            if *expires > crate::db::now() + 60 {
                return Ok(token.clone());
            }
        }
        let token: Token = self
            .http
            .post(TOKEN_URL)
            .query(&[
                ("client_id", keys.igdb_id.as_str()),
                ("client_secret", keys.igdb_secret.as_str()),
                ("grant_type", "client_credentials"),
            ])
            .send()
            .await
            .map_err(net)?
            .error_for_status()
            .map_err(net)?
            .json()
            .await
            .map_err(net)?;
        *self.token.lock().unwrap() = Some((token.access_token.clone(), crate::db::now() + token.expires_in));
        Ok(token.access_token)
    }

    async fn igdb(&self, keys: &Keys, body: String) -> Result<Vec<IgdbGame>, String> {
        let token = self.access_token(keys).await?;
        let response = self
            .http
            .post(GAMES_URL)
            .header("Client-ID", &keys.igdb_id)
            .bearer_auth(token)
            .body(body)
            .send()
            .await
            .map_err(net)?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            *self.token.lock().unwrap() = None;
        }
        response.error_for_status().map_err(net)?.json().await.map_err(net)
    }

    async fn search_igdb(&self, keys: &Keys, query: &str) -> Result<Vec<IgdbGame>, String> {
        let query: String = query.chars().filter(|c| !matches!(c, '"' | '\\')).collect();
        self.igdb(keys, format!("search \"{query}\"; fields {FIELDS}; where platforms = ({WINDOWS}); limit 20;")).await
    }

    async fn steamgrid<T: serde::de::DeserializeOwned>(&self, keys: &Keys, path: &str) -> Result<Vec<T>, String> {
        let list: SteamGridList<T> = self
            .http
            .get(format!("{STEAMGRID_URL}{path}"))
            .bearer_auth(&keys.steamgrid)
            .send()
            .await
            .map_err(net)?
            .error_for_status()
            .map_err(net)?
            .json()
            .await
            .map_err(net)?;
        Ok(list.data)
    }

    async fn first_image(&self, keys: &Keys, path: &str) -> Option<String> {
        let images: Vec<SteamGridImage> = self.steamgrid(keys, path).await.ok()?;
        images.into_iter().next().map(|image| image.url)
    }

    async fn steamgrid_art(&self, keys: &Keys, name: &str, released: Option<i64>) -> SteamGridArt {
        let term: String = name.chars().filter(char::is_ascii).map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
        let term = term.split_whitespace().collect::<Vec<_>>().join("%20");
        let Ok(found) = self.steamgrid::<SteamGridGame>(keys, &format!("/search/autocomplete/{term}")).await else {
            return SteamGridArt::default();
        };
        let Some(game) = pick_steamgrid(&found, name, released) else {
            return SteamGridArt::default();
        };
        let filters = "types=static&nsfw=false&humor=false";
        SteamGridArt {
            cover: self.first_image(keys, &format!("/grids/game/{}?dimensions=600x900,660x930&{filters}", game.id)).await,
            hero: self.first_image(keys, &format!("/heroes/game/{}?{filters}", game.id)).await,
            logo: self.first_image(keys, &format!("/logos/game/{}?{filters}", game.id)).await,
        }
    }

    async fn save(&self, folder: &Path, name: &str, url: Option<String>) -> Option<String> {
        let url = url?;
        let path = folder.join(format!("{name}.{}", extension(&url)));
        if path.is_file() {
            return Some(path.to_string_lossy().into_owned());
        }
        let bytes = self.http.get(&url).send().await.ok()?.error_for_status().ok()?.bytes().await.ok()?;
        let partial = path.with_extension("part");
        fs::write(&partial, &bytes).ok()?;
        fs::rename(&partial, &path).ok()?;
        Some(path.to_string_lossy().into_owned())
    }

    async fn apply(&self, app: &AppHandle, keys: &Keys, id: i64, game: IgdbGame) -> Result<(), String> {
        let folder = art_folder(app)?.join(game.id.to_string());
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;

        let art = self.steamgrid_art(keys, &game.name, game.first_release_date).await;
        // The official cover goes first, since community covers vary a lot.
        let cover = game.cover.as_ref().map(|image| igdb_image("t_cover_big_2x", image)).or(art.cover);
        let hero = art.hero.or(game.artworks.first().or(game.screenshots.first()).map(|image| igdb_image("t_1080p", image)));

        let mut screenshots = Vec::new();
        for (index, image) in game.screenshots.iter().take(SCREENSHOTS).enumerate() {
            let url = igdb_image("t_screenshot_huge", image);
            screenshots.extend(self.save(&folder, &format!("shot{index}"), Some(url)).await);
        }

        let details = Details {
            igdb_id: game.id,
            developer: companies(&game, |c| c.developer),
            publisher: companies(&game, |c| c.publisher),
            genres: genres(&game),
            modes: game.game_modes.iter().map(|mode| mode.name.clone()).collect(),
            summary: game.summary.unwrap_or_default(),
            release_date: game.first_release_date,
            igdb_rating: game.total_rating,
            cover: self.save(&folder, "cover", cover).await,
            hero: self.save(&folder, "hero", hero).await,
            logo: self.save(&folder, "logo", art.logo).await,
            screenshots,
            name: game.name,
        };
        app.state::<Db>().set_details(id, &details).map_err(|e| e.to_string())
    }

    async fn identify(&self, app: &AppHandle, id: i64, name: &str) -> Result<(), String> {
        let keys = load_keys(app).ok_or(NO_KEYS)?;
        let query = clean_name(name);
        let found = if query.is_empty() { Vec::new() } else { self.search_igdb(&keys, &query).await? };
        match best(&query, found) {
            Some(game) => self.apply(app, &keys, id, game).await,
            None => app.state::<Db>().set_meta_state(id, "unmatched").map_err(|e| e.to_string()),
        }
    }

    pub async fn search(&self, app: &AppHandle, query: &str) -> Result<Vec<Match>, String> {
        let keys = load_keys(app).ok_or(NO_KEYS)?;
        let cleaned = clean_name(query);
        let term = if cleaned.is_empty() { query } else { &cleaned };
        let mut found = self.search_igdb(&keys, term).await?;
        found.sort_by(|a, b| score(term, b).total_cmp(&score(term, a)));
        Ok(found
            .iter()
            .map(|game| Match {
                igdb_id: game.id,
                name: game.name.clone(),
                year: year(game.first_release_date),
                developer: companies(game, |c| c.developer),
                cover: game.cover.as_ref().map(|image| igdb_image("t_cover_big", image)),
            })
            .collect())
    }

    pub async fn choose(&self, app: &AppHandle, id: i64, igdb_id: i64) -> Result<(), String> {
        let keys = load_keys(app).ok_or(NO_KEYS)?;
        let game = self
            .igdb(&keys, format!("fields {FIELDS}; where id = {igdb_id};"))
            .await?
            .into_iter()
            .next()
            .ok_or("IGDB no longer has that game")?;
        self.apply(app, &keys, id, game).await
    }
}

fn art_folder(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_local_data_dir().map(|dir| dir.join("art")).map_err(|e| e.to_string())
}

/// Works through new games one at a time. A failure usually means no internet, so it waits and tries again.
pub fn start_worker(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let metadata = app.state::<Metadata>();
            while let Ok(Some((id, name))) = app.state::<Db>().next_unidentified() {
                if let Err(error) = metadata.identify(&app, id, &name).await {
                    eprintln!("metadata for {name}: {error}");
                    break;
                }
                library::changed(&app);
                time::sleep(BETWEEN_GAMES).await;
            }
            let _ = time::timeout(RETRY, metadata.wake.notified()).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(id: i64, name: &str, kind: i64, ratings: i64) -> IgdbGame {
        IgdbGame {
            id,
            name: name.into(),
            summary: None,
            first_release_date: None,
            total_rating: None,
            total_rating_count: Some(ratings),
            game_type: Some(kind),
            cover: None,
            artworks: Vec::new(),
            screenshots: Vec::new(),
            genres: Vec::new(),
            game_modes: Vec::new(),
            involved_companies: Vec::new(),
        }
    }

    #[test]
    fn cleans_release_names() {
        for (raw, clean) in [
            ("Hades.II.v1.0-FitGirl Repack", "Hades II"),
            ("[DODI Repack] Cyberpunk 2077 - Ultimate Edition", "Cyberpunk 2077"),
            ("Elden.Ring-RUNE", "Elden Ring"),
            ("Spider-Man Remastered", "Spider-Man Remastered"),
            ("The_Witcher_3_Wild_Hunt_GOTY_Edition_GOG", "The Witcher 3 Wild Hunt"),
            ("Portal 2 (2011) MULTI12", "Portal 2"),
            ("Baldurs Gate 3 v4.1.1.3686210 + 5 DLCs", "Baldurs Gate 3"),
            ("Stardew Valley", "Stardew Valley"),
            ("Red Dead Redemption 2 Build 1491.50", "Red Dead Redemption 2"),
        ] {
            assert_eq!(clean_name(raw), clean, "{raw}");
        }
    }

    #[test]
    fn popularity_settles_a_name_tie() {
        let games = vec![game(1, "Hades 2", 0, 0), game(2, "Hades II", 0, 179)];
        assert_eq!(best("Hades II", games).unwrap().id, 2);
    }

    #[test]
    fn exact_name_beats_expansion() {
        let games = vec![game(1, "Elden Ring: Shadow of the Erdtree", 2, 500), game(2, "Elden Ring", 0, 900)];
        assert_eq!(best("Elden Ring", games).unwrap().id, 2);
    }

    #[test]
    fn a_short_name_finds_the_full_title() {
        let games = vec![game(1, "The Witcher 3: Wild Hunt", 0, 900), game(2, "Witcher Tales", 0, 10)];
        assert_eq!(best("Witcher 3", games).unwrap().id, 1);
    }

    #[test]
    fn leaves_a_poor_match_alone() {
        assert!(best("Totally Different Game", vec![game(1, "Different Worlds", 0, 900)]).is_none());
        assert!(best("Hollow", vec![game(1, "Hollow: Add-on", 1, 900)]).is_none());
    }

    #[test]
    fn art_must_come_from_the_same_era() {
        const YEAR: i64 = 31_556_952;
        let found = vec![SteamGridGame { id: 1, name: "Hades II".into(), release_date: Some(54 * YEAR) }];
        assert!(pick_steamgrid(&found, "Hades 2", Some(31 * YEAR)).is_none());
        assert_eq!(pick_steamgrid(&found, "Hades II", Some(55 * YEAR)).unwrap().id, 1);
        assert_eq!(pick_steamgrid(&found, "Hades II", None).unwrap().id, 1);
    }

    #[test]
    fn reads_keys() {
        let keys = parse_keys("# note\nIGDB_CLIENT_ID=a\nIGDB_CLIENT_SECRET = b\nSTEAMGRIDDB_API_KEY=\"c\"\n").unwrap();
        assert_eq!((keys.igdb_id.as_str(), keys.igdb_secret.as_str(), keys.steamgrid.as_str()), ("a", "b", "c"));
        assert!(parse_keys("IGDB_CLIENT_ID=\nIGDB_CLIENT_SECRET=b\nSTEAMGRIDDB_API_KEY=c").is_none());
    }
}
