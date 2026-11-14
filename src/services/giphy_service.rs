//! GIPHY search for the `/giphy` picker.
//!
//! The API key is a GIPHY test key: about 100 calls an hour, shared by everyone
//! running the build. Only search calls count; the GIFs themselves come from
//! GIPHY's media hosts and don't use the key. So the service spends calls
//! sparingly:
//!
//! - a bare `/giphy` shows the GIFs you sent before, which needs no call;
//! - the picker searches once typing pauses, for queries of two characters or
//!   more (the caller debounces);
//! - one page per query, without trending or analytics calls;
//! - results are kept per query, in memory and on disk, for a day;
//! - a query is never in flight twice;
//! - each client stops at `CLIENT_HOURLY_BUDGET` calls an hour, and after a
//!   429 it pauses for `RATE_LIMIT_PAUSE` before the picker offers a retry.
//!   GIPHY sends no reset time, so the pause is a guess.

use crate::services::local_store::paths::data_root;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Build-time key; `ZBASE_GIPHY_API_KEY` at run time overrides it for development.
const BUILD_API_KEY: Option<&str> = option_env!("ZBASE_GIPHY_API_KEY");
const API_KEY_ENV: &str = "ZBASE_GIPHY_API_KEY";
const SEARCH_ENDPOINT: &str = "https://api.giphy.com/v1/gifs/search";
const RESULTS_PER_SEARCH: usize = 24;
pub const MIN_QUERY_CHARS: usize = 2;
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_CACHED_QUERIES: usize = 200;
const MAX_RECENTS: usize = RESULTS_PER_SEARCH;
const CLIENT_HOURLY_BUDGET: usize = 30;
const HOUR: Duration = Duration::from_secs(60 * 60);
const RATE_LIMIT_PAUSE: Duration = Duration::from_secs(60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PREVIEW_BYTES: usize = 5 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GiphyGif {
    pub id: String,
    /// A small looping GIF for the picker.
    pub preview_url: String,
    pub preview_width: u32,
    pub preview_height: u32,
    /// What gets sent: the link the Keybase `/giphy` command sends, so every
    /// client renders it as a GIPHY GIF.
    pub target_url: String,
}

/// What the picker shows for a query.
#[derive(Clone, Copy, Debug)]
pub enum GiphyView<'a> {
    /// This build has no API key.
    Unavailable,
    /// Bare `/giphy`: the GIFs you sent before, possibly none.
    Recents(&'a [GiphyGif]),
    /// Too short to search.
    KeepTyping,
    /// Waiting for typing to pause, or for the response.
    Loading,
    Results(&'a [GiphyGif]),
    /// GIPHY or the hourly budget refused; `can_retry` once the pause is over.
    RateLimited {
        can_retry: bool,
    },
    Failed,
}

#[derive(Debug)]
enum GiphyEvent {
    Search {
        query: String,
        result: Result<Vec<GiphyGif>, SearchError>,
    },
    Preview {
        url: String,
        path: Option<PathBuf>,
    },
}

#[derive(Debug)]
enum SearchError {
    RateLimited,
    Other,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CachedSearch {
    fetched_at_secs: u64,
    gifs: Vec<GiphyGif>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DiskCache {
    searches: HashMap<String, CachedSearch>,
    recents: Vec<GiphyGif>,
}

pub struct GiphyService {
    api_key: Option<String>,
    disk: DiskCache,
    cache_path: PathBuf,
    preview_dir: PathBuf,
    pending: HashSet<String>,
    failed: HashMap<String, bool>,
    blocked_until: Option<Instant>,
    calls: VecDeque<Instant>,
    previews: HashMap<String, Option<PathBuf>>,
    sender: Sender<GiphyEvent>,
    receiver: Receiver<GiphyEvent>,
}

impl GiphyService {
    pub fn new() -> Self {
        let api_key = std::env::var(API_KEY_ENV)
            .ok()
            .or_else(|| BUILD_API_KEY.map(str::to_string))
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty());
        Self::with_paths(
            api_key,
            data_root().join("giphy_cache.json"),
            data_root().join("giphy_previews"),
        )
    }

    fn with_paths(api_key: Option<String>, cache_path: PathBuf, preview_dir: PathBuf) -> Self {
        let disk = std::fs::read(&cache_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<DiskCache>(&bytes).ok())
            .unwrap_or_default();
        let (sender, receiver) = mpsc::channel();
        Self {
            api_key,
            disk,
            cache_path,
            preview_dir,
            pending: HashSet::new(),
            failed: HashMap::new(),
            blocked_until: None,
            calls: VecDeque::new(),
            previews: HashMap::new(),
            sender,
            receiver,
        }
    }

    /// Lower-cased, with runs of whitespace collapsed, so "Cats " and "cats"
    /// share one call.
    pub fn normalize_query(query: &str) -> String {
        query
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    pub fn view(&self, query: &str) -> GiphyView<'_> {
        let query = Self::normalize_query(query);
        if query.is_empty() {
            return GiphyView::Recents(&self.disk.recents);
        }
        if self.api_key.is_none() {
            return GiphyView::Unavailable;
        }
        if query.chars().count() < MIN_QUERY_CHARS {
            return GiphyView::KeepTyping;
        }
        if let Some(cached) = self.fresh(&query) {
            return GiphyView::Results(&cached.gifs);
        }
        let can_retry = !self.is_blocked(Instant::now());
        match self.failed.get(&query) {
            Some(true) => GiphyView::RateLimited { can_retry },
            Some(false) => GiphyView::Failed,
            None if !can_retry => GiphyView::RateLimited { can_retry },
            None => GiphyView::Loading,
        }
    }

    /// Starts a search unless the answer is cached, already on its way, or
    /// the hourly budget is spent. Returns whether a call went out.
    pub fn search(&mut self, query: &str) -> bool {
        let query = Self::normalize_query(query);
        let Some(api_key) = self.api_key.clone() else {
            return false;
        };
        if query.chars().count() < MIN_QUERY_CHARS
            || self.fresh(&query).is_some()
            || self.pending.contains(&query)
        {
            return false;
        }
        let now = Instant::now();
        while self
            .calls
            .front()
            .is_some_and(|call| now.duration_since(*call) >= HOUR)
        {
            self.calls.pop_front();
        }
        if self.is_blocked(now) {
            self.failed.insert(query, true);
            return false;
        }
        self.failed.remove(&query);
        self.pending.insert(query.clone());
        self.calls.push_back(now);
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let result =
                run_async(fetch_search(&api_key, &query)).unwrap_or(Err(SearchError::Other));
            let _ = sender.send(GiphyEvent::Search { query, result });
        });
        true
    }

    /// The downloaded preview for `url`, once `fetch_previews` has it.
    pub fn preview_path(&self, url: &str) -> Option<&PathBuf> {
        self.previews.get(url).and_then(Option::as_ref)
    }

    /// Downloads the previews of `gifs` that are not on disk or on their way.
    pub fn fetch_previews(&mut self, gifs: &[GiphyGif]) {
        let mut urls = Vec::new();
        for gif in gifs {
            if self.previews.contains_key(&gif.preview_url) {
                continue;
            }
            let path = preview_cache_path(&self.preview_dir, &gif.preview_url);
            if path.exists() {
                self.previews.insert(gif.preview_url.clone(), Some(path));
            } else {
                self.previews.insert(gif.preview_url.clone(), None);
                urls.push((gif.preview_url.clone(), path));
            }
        }
        if urls.is_empty() {
            return;
        }
        let sender = self.sender.clone();
        let dir = self.preview_dir.clone();
        std::thread::spawn(move || {
            let _ = std::fs::create_dir_all(&dir);
            let _ = run_async(async move {
                let client = reqwest::Client::builder()
                    .timeout(FETCH_TIMEOUT)
                    .build()
                    .ok()?;
                let downloads = urls.into_iter().map(|(url, path)| {
                    let client = client.clone();
                    let sender = sender.clone();
                    tokio::spawn(async move {
                        let path = download(&client, &url, &path).await;
                        let _ = sender.send(GiphyEvent::Preview { url, path });
                    })
                });
                for download in downloads.collect::<Vec<_>>() {
                    let _ = download.await;
                }
                Some(())
            });
        });
    }

    /// Applies finished calls and downloads; returns whether anything changed.
    pub fn drain(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.receiver.try_recv() {
            changed = true;
            match event {
                GiphyEvent::Search { query, result } => {
                    self.pending.remove(&query);
                    match result {
                        Ok(gifs) => {
                            self.fetch_previews(&gifs);
                            self.disk.searches.insert(
                                query,
                                CachedSearch {
                                    fetched_at_secs: unix_secs(),
                                    gifs,
                                },
                            );
                            self.persist();
                        }
                        Err(SearchError::RateLimited) => {
                            self.blocked_until = Some(Instant::now() + RATE_LIMIT_PAUSE);
                            self.failed.insert(query, true);
                        }
                        Err(SearchError::Other) => {
                            self.failed.insert(query, false);
                        }
                    }
                }
                GiphyEvent::Preview { url, path } => {
                    if path.is_some() {
                        self.previews.insert(url, path);
                    }
                }
            }
        }
        changed
    }

    /// Puts `gif` first among the GIFs a bare `/giphy` shows.
    pub fn record_sent(&mut self, gif: &GiphyGif) {
        self.disk.recents.retain(|recent| recent.id != gif.id);
        self.disk.recents.insert(0, gif.clone());
        self.disk.recents.truncate(MAX_RECENTS);
        self.persist();
    }

    /// When a refused search may run again, while one is refused.
    pub fn retry_at(&self) -> Option<Instant> {
        let now = Instant::now();
        let paused = self.blocked_until.filter(|until| now < *until);
        let recent = self
            .calls
            .iter()
            .filter(|call| now.duration_since(**call) < HOUR)
            .collect::<Vec<_>>();
        let budget_frees = (recent.len() >= CLIENT_HOURLY_BUDGET)
            .then(|| recent[recent.len() - CLIENT_HOURLY_BUDGET])
            .map(|oldest| *oldest + HOUR);
        paused.max(budget_frees)
    }

    fn fresh(&self, query: &str) -> Option<&CachedSearch> {
        self.disk.searches.get(query).filter(|cached| {
            unix_secs().saturating_sub(cached.fetched_at_secs) < CACHE_TTL.as_secs()
        })
    }

    fn is_blocked(&self, now: Instant) -> bool {
        self.blocked_until.is_some_and(|until| now < until)
            || self
                .calls
                .iter()
                .filter(|call| now.duration_since(**call) < HOUR)
                .count()
                >= CLIENT_HOURLY_BUDGET
    }

    fn persist(&mut self) {
        let now = unix_secs();
        self.disk
            .searches
            .retain(|_, cached| now.saturating_sub(cached.fetched_at_secs) < CACHE_TTL.as_secs());
        if self.disk.searches.len() > MAX_CACHED_QUERIES {
            let mut by_age = self
                .disk
                .searches
                .iter()
                .map(|(query, cached)| (cached.fetched_at_secs, query.clone()))
                .collect::<Vec<_>>();
            by_age.sort();
            let excess = self.disk.searches.len() - MAX_CACHED_QUERIES;
            for (_, query) in by_age.into_iter().take(excess) {
                self.disk.searches.remove(&query);
            }
        }
        if let Ok(bytes) = serde_json::to_vec(&self.disk) {
            if let Some(parent) = self.cache_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&self.cache_path, bytes);
        }
    }
}

impl Default for GiphyService {
    fn default() -> Self {
        Self::new()
    }
}

fn run_async<F: std::future::Future>(future: F) -> Option<F::Output> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .ok()
        .map(|runtime| runtime.block_on(future))
}

async fn fetch_search(api_key: &str, query: &str) -> Result<Vec<GiphyGif>, SearchError> {
    let mut url = Url::parse(SEARCH_ENDPOINT).map_err(|_| SearchError::Other)?;
    url.query_pairs_mut()
        .append_pair("api_key", api_key)
        .append_pair("q", query)
        .append_pair("limit", &RESULTS_PER_SEARCH.to_string());
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .map_err(|_| SearchError::Other)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| SearchError::Other)?;
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(SearchError::RateLimited);
    }
    if !response.status().is_success() {
        return Err(SearchError::Other);
    }
    let bytes = response.bytes().await.map_err(|_| SearchError::Other)?;
    let body = serde_json::from_slice::<Value>(&bytes).map_err(|_| SearchError::Other)?;
    Ok(parse_search_response(&body))
}

fn parse_search_response(body: &Value) -> Vec<GiphyGif> {
    body.get("data")
        .and_then(Value::as_array)
        .map(|data| data.iter().filter_map(parse_gif).collect())
        .unwrap_or_default()
}

fn parse_gif(gif: &Value) -> Option<GiphyGif> {
    let id = gif.get("id")?.as_str()?.to_string();
    let images = gif.get("images")?;
    let preview = images.get("fixed_height_small")?;
    let original = images.get("original")?;
    let dimension = |image: &Value, key: &str| {
        image
            .get(key)
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0)
    };
    let without_query = |url: &str| url.split(['?', '#']).next().unwrap_or(url).to_string();
    let preview_url = without_query(preview.get("url")?.as_str()?);
    let (width, height) = (
        dimension(original, "width")?,
        dimension(original, "height")?,
    );
    let mp4 = original
        .get("mp4")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty());
    let (target, is_video) = match mp4 {
        Some(mp4) => (without_query(mp4), true),
        None => (without_query(original.get("url")?.as_str()?), false),
    };
    Some(GiphyGif {
        id,
        preview_url,
        preview_width: dimension(preview, "width")?,
        preview_height: dimension(preview, "height")?,
        target_url: format!("{target}#height={height}&width={width}&isvideo={is_video}"),
    })
}

fn preview_cache_path(dir: &std::path::Path, url: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    dir.join(format!("{:x}.gif", Sha256::digest(url.as_bytes())))
}

async fn download(client: &reqwest::Client, url: &str, path: &PathBuf) -> Option<PathBuf> {
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    if bytes.is_empty() || bytes.len() > MAX_PREVIEW_BYTES {
        return None;
    }
    std::fs::write(path, &bytes).ok()?;
    Some(path.clone())
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "zbase-giphy-test-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn service(label: &str, key: Option<&str>) -> (GiphyService, PathBuf) {
        let dir = temp_dir(label);
        let service = GiphyService::with_paths(
            key.map(str::to_string),
            dir.join("giphy_cache.json"),
            dir.join("previews"),
        );
        (service, dir)
    }

    fn gif(id: &str) -> GiphyGif {
        GiphyGif {
            id: id.into(),
            preview_url: format!("https://media.giphy.com/media/{id}/100.gif"),
            preview_width: 100,
            preview_height: 100,
            target_url: format!(
                "https://media.giphy.com/media/{id}/giphy.mp4#height=200&width=200&isvideo=true"
            ),
        }
    }

    #[test]
    fn parses_a_search_result_into_the_keybase_giphy_link() {
        let body = serde_json::json!({
            "data": [{
                "id": "abc",
                "images": {
                    "fixed_height_small": {
                        "url": "https://media1.giphy.com/media/v1.x/abc/100.gif?cid=1&rid=100.gif",
                        "width": "178", "height": "100"
                    },
                    "original": {
                        "url": "https://media1.giphy.com/media/v1.x/abc/giphy.gif?cid=1",
                        "mp4": "https://media1.giphy.com/media/v1.x/abc/giphy.mp4?cid=1&rid=giphy.mp4",
                        "width": "480", "height": "270"
                    }
                }
            }, { "id": "broken", "images": {} }]
        });
        assert_eq!(
            parse_search_response(&body),
            vec![GiphyGif {
                id: "abc".into(),
                preview_url: "https://media1.giphy.com/media/v1.x/abc/100.gif".into(),
                preview_width: 178,
                preview_height: 100,
                target_url: "https://media1.giphy.com/media/v1.x/abc/giphy.mp4#height=270&width=480&isvideo=true".into(),
            }]
        );
    }

    #[test]
    fn queries_that_differ_in_case_or_spacing_share_one_call() {
        assert_eq!(GiphyService::normalize_query("  Happy   Cat "), "happy cat");
    }

    #[test]
    fn spends_no_call_on_bare_short_cached_or_pending_queries() {
        let (mut service, dir) = service("no-call", Some("key"));
        assert!(matches!(service.view(""), GiphyView::Recents(recents) if recents.is_empty()));
        assert!(!service.search(""));
        assert!(!service.search("a"));
        assert!(matches!(service.view("a"), GiphyView::KeepTyping));

        service.disk.searches.insert(
            "cats".into(),
            CachedSearch {
                fetched_at_secs: unix_secs(),
                gifs: vec![gif("1")],
            },
        );
        assert!(!service.search("Cats"));
        assert!(matches!(service.view("CATS"), GiphyView::Results(gifs) if gifs.len() == 1));

        service.pending.insert("dogs".into());
        assert!(!service.search("dogs"));
        assert!(matches!(service.view("dogs"), GiphyView::Loading));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stops_at_the_hourly_budget_and_after_a_rate_limit() {
        let (mut service, dir) = service("budget", Some("key"));
        let now = Instant::now();
        service.calls = std::iter::repeat_n(now, CLIENT_HOURLY_BUDGET).collect();
        assert!(!service.search("cats"));
        assert!(matches!(
            service.view("cats"),
            GiphyView::RateLimited { can_retry: false }
        ));
        assert_eq!(service.retry_at(), Some(now + HOUR));

        service.calls.clear();
        service.failed.clear();
        service.blocked_until = Some(now + RATE_LIMIT_PAUSE);
        assert!(!service.search("dogs"));
        assert!(matches!(
            service.view("dogs"),
            GiphyView::RateLimited { can_retry: false }
        ));
        assert_eq!(service.retry_at(), Some(now + RATE_LIMIT_PAUSE));

        // The pause is over: the picker offers a retry, and it goes out.
        service.blocked_until = Some(Instant::now());
        assert!(matches!(
            service.view("dogs"),
            GiphyView::RateLimited { can_retry: true }
        ));
        assert_eq!(service.retry_at(), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_a_key_only_recents_show() {
        let (mut service, dir) = service("no-key", None);
        assert!(!service.search("cats"));
        assert!(matches!(service.view("cats"), GiphyView::Unavailable));
        service.record_sent(&gif("1"));
        assert!(matches!(service.view(""), GiphyView::Recents(recents) if recents.len() == 1));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn recents_and_results_survive_a_restart_and_expire_after_a_day() {
        let (mut service, dir) = service("disk", Some("key"));
        service.record_sent(&gif("1"));
        service.record_sent(&gif("2"));
        service.record_sent(&gif("1"));
        service.disk.searches.insert(
            "cats".into(),
            CachedSearch {
                fetched_at_secs: unix_secs(),
                gifs: vec![gif("3")],
            },
        );
        service.disk.searches.insert(
            "stale".into(),
            CachedSearch {
                fetched_at_secs: unix_secs() - CACHE_TTL.as_secs() - 1,
                gifs: vec![gif("4")],
            },
        );
        service.persist();

        let reopened = GiphyService::with_paths(
            Some("key".into()),
            dir.join("giphy_cache.json"),
            dir.join("previews"),
        );
        let GiphyView::Recents(recents) = reopened.view("") else {
            panic!("expected recents");
        };
        assert_eq!(
            recents
                .iter()
                .map(|gif| gif.id.as_str())
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
        assert!(matches!(reopened.view("cats"), GiphyView::Results(_)));
        assert!(matches!(reopened.view("stale"), GiphyView::Loading));
        let _ = std::fs::remove_dir_all(dir);
    }
}
