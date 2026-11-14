use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Manager};

#[derive(Clone, Serialize, Deserialize)]
pub struct NewsFeed {
    pub name: String,
    pub url: String,
}

const DEFAULT_FEEDS: &[(&str, &str)] = &[
    ("PC Gamer", "https://www.pcgamer.com/rss/"),
    ("Rock Paper Shotgun", "https://www.rockpapershotgun.com/feed"),
    ("Eurogamer", "https://www.eurogamer.net/feed"),
    ("IGN", "https://feeds.feedburner.com/ign/all"),
    ("GameSpot", "https://www.gamespot.com/feeds/mashup/"),
    ("Polygon", "https://www.polygon.com/rss/index.xml"),
];

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub download_dir: String,
    pub install_dir: String,
    pub down_limit: u64,
    pub up_limit: u64,
    pub backup_dir: String,
    pub seed_rule: String,
    pub news_feeds: Vec<NewsFeed>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: r"E:\GaLa\Downloads".into(),
            install_dir: r"E:\GaLa\Games".into(),
            down_limit: 0,
            up_limit: 0,
            backup_dir: r"E:\GaLa\Save Backups".into(),
            seed_rule: "forever".into(),
            news_feeds: DEFAULT_FEEDS.iter().map(|(name, url)| NewsFeed { name: (*name).into(), url: (*url).into() }).collect(),
        }
    }
}

pub struct Store {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl Store {
    pub fn load(app: &AppHandle) -> tauri::Result<Self> {
        let folder = app.path().app_config_dir()?;
        fs::create_dir_all(&folder)?;
        let path = folder.join("settings.json");
        let current = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Ok(Self { path, current: Mutex::new(current) })
    }

    pub fn get(&self) -> Settings {
        self.current.lock().unwrap().clone()
    }

    pub fn update(&self, change: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
        let mut current = self.current.lock().unwrap();
        change(&mut current);
        let bytes = serde_json::to_vec_pretty(&*current).map_err(|e| e.to_string())?;
        let temp = self.path.with_extension("tmp");
        fs::write(&temp, bytes).map_err(|e| e.to_string())?;
        fs::rename(&temp, &self.path).map_err(|e| e.to_string())?;
        Ok(current.clone())
    }
}
