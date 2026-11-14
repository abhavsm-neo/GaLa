//! Gaming news from RSS feeds, plus Steam's own news for Steam games in the library.
//! Articles are kept on disk for a month so they read offline. Pictures download only
//! when an article is shown, since the feeds list hundreds of articles at once.

use crate::{
    db::{Article, Db},
    settings::{NewsFeed, Store},
};
use serde::Deserialize;
use std::{fs, iter::once, path::PathBuf, ptr::null, ptr::null_mut, time::Duration};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{sync::Notify, time};
use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

const EVERY: Duration = Duration::from_secs(60 * 60);
const BETWEEN_STEAM_GAMES: Duration = Duration::from_millis(300);
const KEEP_DAYS: i64 = 30;
const PER_FEED: usize = 100;
const SUMMARY_CHARS: usize = 280;
const MAX_IMAGE_BYTES: usize = 6 << 20;
const STEAM_LAUNCH: &str = "steam://rungameid/";
const STEAM_NEWS: &str = "https://api.steampowered.com/ISteamNews/GetNewsForApp/v2/";
const STEAM_CLAN_IMAGES: &str = "https://clan.akamai.steamstatic.com/images/";
const SHELL_EXECUTE_OK: usize = 32;
pub const STEAM_SOURCE: &str = "Steam";

pub struct News {
    http: reqwest::Client,
    wake: Notify,
}

impl Default for News {
    fn default() -> Self {
        let http = reqwest::Client::builder()
            .user_agent("GaLa/0.1")
            .timeout(Duration::from_secs(30))
            .build()
            .expect("the HTTP client failed to start");
        Self { http, wake: Notify::new() }
    }
}

/// An article as it comes out of a feed, before it is stored.
pub struct Fresh {
    pub key: String,
    pub source: String,
    pub title: String,
    pub summary: String,
    pub url: String,
    pub image_url: Option<String>,
    pub published: i64,
    pub game_id: Option<i64>,
}

#[derive(Deserialize)]
struct SteamReply {
    appnews: SteamNews,
}

#[derive(Deserialize)]
struct SteamNews {
    #[serde(default)]
    newsitems: Vec<SteamItem>,
}

#[derive(Deserialize)]
struct SteamItem {
    gid: String,
    title: String,
    url: String,
    contents: String,
    date: i64,
}

fn entity(name: &str) -> Option<char> {
    if let Some(number) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
        return u32::from_str_radix(number, 16).ok().and_then(char::from_u32);
    }
    if let Some(number) = name.strip_prefix('#') {
        return number.parse().ok().and_then(char::from_u32);
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "hellip" => '…',
        "rsquo" => '’',
        "lsquo" => '‘',
        "ldquo" => '“',
        "rdquo" => '”',
        "mdash" => '—',
        "ndash" => '–',
        _ => return None,
    })
}

/// Plain text from a feed's HTML: tags dropped, entities read, spacing tidied.
pub fn plain_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut chars = html.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '<' => {
                in_tag = true;
                text.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if in_tag => {}
            '&' => {
                let mut name = String::new();
                while let Some(&next) = chars.peek() {
                    if next == ';' || name.len() > 8 || next.is_whitespace() {
                        break;
                    }
                    name.push(next);
                    chars.next();
                }
                match (chars.peek() == Some(&';')).then(|| entity(&name)).flatten() {
                    Some(decoded) => {
                        chars.next();
                        text.push(decoded);
                    }
                    None => {
                        text.push('&');
                        text.push_str(&name);
                    }
                }
            }
            _ => text.push(c),
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Like `plain_text`, but keeps line breaks, for text laid out in lines.
pub fn plain_text_lines(html: &str) -> String {
    html.lines().map(plain_text).collect::<Vec<_>>().join("\n")
}

/// Steam posts use BBCode and an image placeholder rather than HTML.
fn steam_text(contents: &str) -> String {
    let mut text = String::with_capacity(contents.len());
    let mut in_tag = false;
    for c in contents.chars() {
        match c {
            '[' => in_tag = true,
            ']' if in_tag => {
                in_tag = false;
                text.push(' ');
            }
            _ if in_tag => {}
            _ => text.push(c),
        }
    }
    plain_text(&text)
        .split(' ')
        .filter(|word| !word.starts_with("{STEAM_CLAN_IMAGE}") && !word.starts_with("http"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn shorten(text: &str) -> String {
    if text.chars().count() <= SUMMARY_CHARS {
        return text.to_string();
    }
    let cut: String = text.chars().take(SUMMARY_CHARS).collect();
    let end = cut.rfind(' ').unwrap_or(cut.len());
    format!("{}…", cut[..end].trim_end_matches([',', '.', ';', ':']))
}

fn first_image_in(html: &str) -> Option<String> {
    let start = html.find("<img")?;
    let rest = &html[start..];
    let src = rest.find("src=")? + 4;
    let quote = rest[src..].chars().next()?;
    let value = &rest[src + 1..];
    let end = value.find(quote)?;
    let url = value[..end].replace("&amp;", "&");
    url.starts_with("http").then_some(url)
}

fn steam_image(contents: &str) -> Option<String> {
    let start = contents.find("{STEAM_CLAN_IMAGE}/")? + "{STEAM_CLAN_IMAGE}/".len();
    let rest = &contents[start..];
    let end = rest.find(|c: char| c.is_whitespace() || c == '[' || c == '"').unwrap_or(rest.len());
    Some(format!("{STEAM_CLAN_IMAGES}{}", &rest[..end]))
}

fn is_image(url: &str) -> bool {
    let path = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    [".jpg", ".jpeg", ".png", ".webp", ".gif"].iter().any(|ext| path.ends_with(ext))
}

/// Feeds tuck their picture in different places: media tags, enclosures, or the article's own HTML.
fn entry_image(entry: &feed_rs::model::Entry) -> Option<String> {
    for media in &entry.media {
        if let Some(thumbnail) = media.thumbnails.first() {
            return Some(thumbnail.image.uri.clone());
        }
        for content in &media.content {
            let typed = content.content_type.as_ref().is_some_and(|kind| kind.ty() == "image");
            if let Some(url) = &content.url {
                if typed || is_image(url.as_str()) {
                    return Some(url.to_string());
                }
            }
        }
    }
    let html = entry.content.as_ref().and_then(|content| content.body.as_deref());
    html.and_then(first_image_in).or_else(|| entry.summary.as_ref().and_then(|summary| first_image_in(&summary.content)))
}

pub fn parse_feed(source: &str, bytes: &[u8], since: i64) -> Result<(String, Vec<Fresh>), String> {
    let feed = feed_rs::parser::parse(bytes).map_err(|_| "That address isn't a news feed".to_string())?;
    let title = feed.title.map(|title| plain_text(&title.content)).unwrap_or_default();
    let mut found = Vec::new();
    for entry in feed.entries.iter().take(PER_FEED) {
        let Some(url) = entry.links.first().map(|link| link.href.clone()) else { continue };
        let Some(title) = entry.title.as_ref().map(|title| plain_text(&title.content)).filter(|t| !t.is_empty()) else { continue };
        let published = entry.published.or(entry.updated).map_or_else(crate::db::now, |when| when.timestamp());
        if published < since {
            continue;
        }
        let body = entry.summary.as_ref().map(|summary| summary.content.clone())
            .or_else(|| entry.content.as_ref().and_then(|content| content.body.clone()))
            .unwrap_or_default();
        let key = if entry.id.trim().is_empty() { url.clone() } else { entry.id.clone() };
        found.push(Fresh {
            key: format!("{source}|{key}"),
            source: source.to_string(),
            title,
            summary: shorten(&plain_text(&body)),
            url,
            image_url: entry_image(entry),
            published,
            game_id: None,
        });
    }
    Ok((title, found))
}

fn steam_id(launch_uri: Option<&str>) -> Option<&str> {
    let id = launch_uri?.strip_prefix(STEAM_LAUNCH)?;
    id.chars().all(|c| c.is_ascii_digit()).then_some(id)
}

fn news_folder(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_local_data_dir().map(|dir| dir.join("art").join("news")).map_err(|e| e.to_string())
}

impl News {
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    async fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        let response = self.http.get(url).send().await.map_err(|e| e.without_url().to_string())?;
        let bytes = response.error_for_status().map_err(|e| e.without_url().to_string())?.bytes().await.map_err(|e| e.to_string())?;
        Ok(bytes.to_vec())
    }

    /// Checks an address before it joins the list, and names it after the feed's own title.
    pub async fn check(&self, url: &str) -> Result<String, String> {
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return Err("Feed addresses start with https://".into());
        }
        let bytes = self.get(url).await.map_err(|_| "Couldn't reach that address".to_string())?;
        let (title, articles) = parse_feed("", &bytes, 0)?;
        if articles.is_empty() {
            return Err("That feed has no articles".into());
        }
        Ok(if title.is_empty() { url.to_string() } else { title })
    }

    async fn fetch_feed(&self, feed: &NewsFeed, since: i64) -> Result<Vec<Fresh>, String> {
        let bytes = self.get(&feed.url).await?;
        Ok(parse_feed(&feed.name, &bytes, since)?.1)
    }

    async fn fetch_steam(&self, game_id: i64, steam: &str, since: i64) -> Result<Vec<Fresh>, String> {
        let url = format!("{STEAM_NEWS}?appid={steam}&count=10&maxlength=0&feeds=steam_community_announcements&format=json");
        let reply: SteamReply = serde_json::from_slice(&self.get(&url).await?).map_err(|e| e.to_string())?;
        Ok(reply
            .appnews
            .newsitems
            .into_iter()
            .filter(|item| item.date >= since)
            .map(|item| Fresh {
                key: format!("steam|{}", item.gid),
                source: STEAM_SOURCE.into(),
                summary: shorten(&steam_text(&item.contents)),
                image_url: steam_image(&item.contents),
                title: plain_text(&item.title),
                url: item.url,
                published: item.date,
                game_id: Some(game_id),
            })
            .collect())
    }

    async fn refresh(&self, app: &AppHandle) -> usize {
        let since = crate::db::now() - KEEP_DAYS * 86400;
        let db = app.state::<Db>();
        let mut added = 0;
        for feed in app.state::<Store>().get().news_feeds {
            match self.fetch_feed(&feed, since).await {
                Ok(articles) => added += articles.iter().filter(|article| db.add_article(article).unwrap_or(false)).count(),
                Err(error) => eprintln!("news from {}: {error}", feed.name),
            }
        }
        let steam_games: Vec<(i64, String)> = db
            .games()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|game| Some((game.id, steam_id(game.launch_uri.as_deref())?.to_string())))
            .collect();
        for (game_id, steam) in steam_games {
            if let Ok(articles) = self.fetch_steam(game_id, &steam, since).await {
                added += articles.iter().filter(|article| db.add_article(article).unwrap_or(false)).count();
            }
            time::sleep(BETWEEN_STEAM_GAMES).await;
        }
        for image in db.prune_articles(since).unwrap_or_default() {
            let _ = fs::remove_file(image);
        }
        added
    }

    /// Downloads an article's picture the first time it's shown.
    pub async fn image(&self, app: &AppHandle, id: i64) -> Result<Option<String>, String> {
        let db = app.state::<Db>();
        let Some((stored, url)) = db.article_image(id).map_err(|e| e.to_string())? else { return Ok(None) };
        if let Some(path) = stored.filter(|path| PathBuf::from(path).is_file()) {
            return Ok(Some(path));
        }
        let Some(url) = url else { return Ok(None) };
        let bytes = self.get(&url).await?;
        if bytes.len() > MAX_IMAGE_BYTES {
            db.set_article_image(id, None, false).map_err(|e| e.to_string())?;
            return Ok(None);
        }
        let folder = news_folder(app)?;
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let ext = url.split('?').next().and_then(|path| path.rsplit_once('.')).map(|(_, ext)| ext.to_ascii_lowercase())
            .filter(|ext| matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif"))
            .unwrap_or_else(|| "jpg".into());
        let path = folder.join(format!("{id}.{ext}"));
        fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        let path = path.to_string_lossy().into_owned();
        db.set_article_image(id, Some(&path), true).map_err(|e| e.to_string())?;
        Ok(Some(path))
    }
}

pub fn list(app: &AppHandle) -> Result<Vec<Article>, String> {
    app.state::<Db>().articles(crate::db::now() - KEEP_DAYS * 86400).map_err(|e| e.to_string())
}

/// Opens an article in the default browser. Only web addresses, so a feed can't open a program.
pub fn open_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("That isn't a web address".into());
    }
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(once(0)).collect() };
    let (verb, target) = (wide("open"), wide(url));
    let result = unsafe { ShellExecuteW(null_mut(), verb.as_ptr(), target.as_ptr(), null(), null(), SW_SHOWNORMAL) };
    if result as usize > SHELL_EXECUTE_OK { Ok(()) } else { Err("Windows couldn't open your browser".into()) }
}

/// Checks the feeds every hour, sooner when the feed list changes.
pub fn start_worker(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let news = app.state::<News>();
            if news.refresh(&app).await > 0 {
                let _ = app.emit("news-changed", ());
            }
            let _ = time::timeout(EVERY, news.wake.notified()).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_drops_tags_and_reads_entities() {
        assert_eq!(plain_text("<p>Macca &amp; Friends&#8217; <b>show</b></p>"), "Macca & Friends’ show");
        assert_eq!(plain_text("Fish &chips"), "Fish &chips");
    }

    #[test]
    fn steam_text_drops_bbcode_and_images() {
        let contents = "[h2]Update 12.3[/h2]{STEAM_CLAN_IMAGE}/1/abc.png [list][*]Fixed a crash[/list]";
        assert_eq!(steam_text(contents), "Update 12.3 Fixed a crash");
        assert_eq!(steam_image(contents).as_deref(), Some("https://clan.akamai.steamstatic.com/images/1/abc.png"));
    }

    #[test]
    fn shorten_cuts_at_a_word() {
        let long = "word ".repeat(100);
        let short = shorten(&long);
        assert!(short.ends_with('…') && short.chars().count() <= SUMMARY_CHARS + 1);
        assert!(!short.contains("wor…"));
    }

    #[test]
    fn finds_an_image_in_html() {
        assert_eq!(first_image_in(r#"<p><img alt="x" src="https://a.b/c.jpg?w=1&amp;h=2"></p>"#).as_deref(), Some("https://a.b/c.jpg?w=1&h=2"));
        assert_eq!(first_image_in("<p>none</p>"), None);
    }

    #[test]
    fn stores_and_prunes_articles() {
        let path = std::env::temp_dir().join(format!("gala-news-{}.db", std::process::id()));
        let _ = fs::remove_file(&path);
        let db = Db::open(&path).unwrap();
        let article = |key: &str, published: i64| Fresh {
            key: key.into(),
            source: "Test".into(),
            title: format!("Title {key}"),
            summary: String::new(),
            url: "https://example.com".into(),
            image_url: Some("https://example.com/a.jpg".into()),
            published,
            game_id: None,
        };
        let now = crate::db::now();
        assert!(db.add_article(&article("a", now)).unwrap());
        assert!(!db.add_article(&article("a", now)).unwrap(), "the same article twice is stored once");
        assert!(db.add_article(&article("old", now - 40 * 86400)).unwrap());
        assert_eq!(db.articles(0).unwrap().len(), 2);

        let id = db.articles(0).unwrap()[0].id;
        db.set_article_image(id, Some("C:\\pic.jpg"), true).unwrap();
        db.mark_read(&[id]).unwrap();
        let first = db.articles(0).unwrap().into_iter().find(|a| a.id == id).unwrap();
        assert!(first.read && first.image.as_deref() == Some("C:\\pic.jpg"));

        db.prune_articles(now - KEEP_DAYS * 86400).unwrap();
        assert_eq!(db.articles(0).unwrap().len(), 1);
        assert_eq!(db.drop_source("Test").unwrap(), ["C:\\pic.jpg"]);
        assert!(db.articles(0).unwrap().is_empty());
        drop(db);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn parses_an_rss_feed() {
        let rss = r#"<?xml version="1.0"?><rss version="2.0" xmlns:media="http://search.yahoo.com/mrss/"><channel><title>Test News</title>
            <item><title>Big &amp; new</title><link>https://example.com/a</link><guid>a1</guid>
            <pubDate>Mon, 10 Nov 2036 10:00:00 GMT</pubDate><description>&lt;p&gt;Hello there&lt;/p&gt;</description>
            <media:content url="https://example.com/a.jpg?width=690" medium="image"/></item></channel></rss>"#;
        let (title, articles) = parse_feed("Test", rss.as_bytes(), 0).unwrap();
        assert_eq!(title, "Test News");
        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0].title, "Big & new");
        assert_eq!(articles[0].summary, "Hello there");
        assert_eq!(articles[0].image_url.as_deref(), Some("https://example.com/a.jpg?width=690"));
        assert_eq!(articles[0].key, "Test|a1");
    }
}
