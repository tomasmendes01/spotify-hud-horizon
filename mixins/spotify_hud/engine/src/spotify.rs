// Spotify through its Web API: the player read every two seconds; play, pause, skip, shuffle,
// repeat and volume sent as the Web API's own requests; ten failures in a row stop the service
// (the polling model of ThatGravyBoat's Jukebox, MIT). The login is Spotify's authorization
// code flow with PKCE, against the player's own Spotify app.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest as _;
use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const API: &str = "https://api.spotify.com/v1/me/player";
pub const PORT: u16 = 21851;
pub const REDIRECT: &str = "http://127.0.0.1:21851/spotify/callback";
const SCOPES: &str = "user-read-playback-state user-modify-playback-state user-read-currently-playing";
const POLL: Duration = Duration::from_secs(2);
const LOGIN_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    Off,
    Song,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Track,
    Ad,
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    pub title: String,
    pub artists: Vec<String>,
    /// The largest cover image, empty when there is none.
    pub cover: String,
    pub url: String,
    pub kind: Kind,
}

/// One answer of the player endpoint, as Jukebox's `State`.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub shuffle: bool,
    pub repeat: Repeat,
    pub volume: u8,
    pub playing: bool,
    pub progress_ms: u64,
    pub duration_ms: u64,
    pub song: Song,
}

impl State {
    /// Jukebox's `isSame`: the same title.
    pub fn same(&self, other: &State) -> bool {
        self.song.title == other.song.title
    }
}

fn song(title: &str, artist: &str) -> Song {
    Song {
        title: title.into(),
        artists: if artist.is_empty() { vec![] } else { vec![artist.into()] },
        cover: String::new(),
        url: "https://open.spotify.com".into(),
        kind: Kind::Unknown,
    }
}

/// Reads the player endpoint's JSON as Jukebox's serializer does: an error, a private
/// session (no song), a local file (title only) or a normal track.
pub fn parse_player(body: &str) -> Result<State, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("Error parsing Player JSON: {e}"))?;
    if let Some(e) = v.get("error") {
        return Err(format!("Polling error: {}", e.get("message").and_then(Value::as_str).unwrap_or("unknown")));
    }
    let device = v.get("device").unwrap_or(&Value::Null);
    let volume = device.get("volume_percent").and_then(Value::as_u64).unwrap_or(0).min(100) as u8;
    let playing = v.get("is_playing").and_then(Value::as_bool).ok_or("Unknown player state")?;
    let shuffle = v.get("shuffle_state").and_then(Value::as_bool).unwrap_or(false);
    let repeat = match v.get("repeat_state").and_then(Value::as_str) {
        Some("track") => Repeat::Song,
        Some("context") => Repeat::All,
        _ => Repeat::Off,
    };
    let kind = match v.get("currently_playing_type").and_then(Value::as_str) {
        Some("track") => Kind::Track,
        Some("ad") => Kind::Ad,
        _ => Kind::Unknown,
    };
    let base = |song: Song, progress_ms: u64, duration_ms: u64| State {
        shuffle,
        repeat,
        volume,
        playing,
        progress_ms,
        duration_ms,
        song,
    };
    if device.get("is_private_session").and_then(Value::as_bool) == Some(true) {
        return Ok(base(song("Private Session", "Cant get song info"), 0, 0));
    }
    let progress = v.get("progress_ms").and_then(Value::as_u64).unwrap_or(0);
    let item = v.get("item").filter(|i| i.is_object());
    let Some(item) = item else {
        return Ok(base(song("No song playing", "No artist"), progress, 0));
    };
    let duration = item.get("duration_ms").and_then(Value::as_u64).unwrap_or(0);
    let title = item.get("name").and_then(Value::as_str).unwrap_or("").to_owned();
    if item.get("is_local").and_then(Value::as_bool) == Some(true) {
        let mut s = song(&title, "");
        s.kind = kind;
        return Ok(base(s, progress, duration));
    }
    let artists = item
        .get("artists")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.get("name")?.as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    // An episode keeps its images on the item, a track on its album.
    let images = item
        .get("album")
        .and_then(|a| a.get("images"))
        .or_else(|| item.get("images"))
        .and_then(Value::as_array);
    let cover = images
        .and_then(|imgs| {
            imgs.iter()
                .max_by_key(|i| i.get("width").and_then(Value::as_u64).unwrap_or(0))
                .and_then(|i| i.get("url")?.as_str())
        })
        .unwrap_or("")
        .to_owned();
    let url = item
        .get("external_urls")
        .and_then(|u| u.get("spotify"))
        .and_then(Value::as_str)
        .unwrap_or("https://open.spotify.com")
        .to_owned();
    Ok(base(Song { title, artists, cover, url, kind }, progress, duration))
}

/// The tokens, kept in `spotify.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Auth {
    /// The player's Spotify app (developer.spotify.com), whose redirect URI is `REDIRECT`.
    pub client_id: String,
    pub auth: String,
    pub refresh: String,
    /// Unix seconds when `auth` runs out.
    pub expires_at: u64,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Stopped,
    Starting,
    Running,
}

/// What the game reads: the last state and when it came, and the service's phase.
#[derive(Default)]
pub struct Shared {
    pub state: Option<(State, Instant)>,
    pub running: bool,
    pub errors: u32,
    pub auth: Auth,
    /// Lines for chat, taken on the next tick.
    pub chat: Vec<String>,
    /// The title last announced (its announcement).
    pub announce: Option<String>,
    pub login_pending: bool,
    /// Set when a login succeeds: the mod then switches its service to Spotify.
    pub logged_in: bool,
    /// Bumped by every change the box shows.
    pub generation: u64,
}

impl Shared {
    pub fn phase(&self) -> Phase {
        match (self.running, self.state.is_some()) {
            (false, _) => Phase::Stopped,
            (true, false) => Phase::Starting,
            (true, true) => Phase::Running,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Start,
    Stop,
    Restart,
    SetPaused(bool),
    ToggleShuffle,
    ToggleRepeat,
    Move(bool),
    Volume(u8),
    Login,
    Logout,
}

pub struct Service {
    pub shared: Arc<Mutex<Shared>>,
    tx: Sender<Action>,
}

impl Service {
    pub fn new(dir: PathBuf) -> Self {
        let auth: Auth = crate::config::load(&dir, "spotify.json");
        let shared = Arc::new(Mutex::new(Shared { auth, ..Default::default() }));
        let (tx, rx) = mpsc::channel();
        let worker = Worker {
            shared: shared.clone(),
            dir,
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(15))
                .timeout(Duration::from_secs(15))
                .user_agent("Spotify-HUD-Horizon/1.0")
                .build(),
            tx: tx.clone(),
            running: false,
        };
        std::thread::Builder::new()
            .name("spotify-hud-spotify".into())
            .spawn(move || worker.run(rx))
            .expect("spotify_hud: spawn the Spotify thread");
        Self { shared, tx }
    }
    pub fn send(&self, action: Action) {
        let _ = self.tx.send(action);
    }
    pub fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }
}

struct Worker {
    shared: Arc<Mutex<Shared>>,
    dir: PathBuf,
    agent: ureq::Agent,
    tx: Sender<Action>,
    running: bool,
}

enum Reply {
    Ok(u16, String),
    Unauthorized,
    Failed(String),
}

impl Worker {
    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn chat(&self, line: impl Into<String>) {
        let mut s = self.lock();
        s.chat.push(line.into());
    }
    fn run(mut self, rx: Receiver<Action>) {
        let mut next_poll = Instant::now();
        loop {
            let wait = if self.running {
                next_poll.saturating_duration_since(Instant::now())
            } else {
                Duration::from_secs(3600)
            };
            match rx.recv_timeout(wait) {
                Ok(action) => {
                    let polls_soon = self.act(action);
                    if polls_soon && self.running {
                        // The player shows the change soon after (Jukebox waits for its next poll).
                        next_poll = next_poll.min(Instant::now() + Duration::from_millis(400));
                    }
                    if self.running && self.lock().state.is_none() {
                        next_poll = next_poll.min(Instant::now());
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if self.running {
                        self.poll();
                        next_poll = Instant::now() + POLL;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn set_running(&mut self, running: bool) {
        self.running = running;
        let mut s = self.lock();
        s.running = running;
        if !running {
            s.state = None;
        }
        s.errors = 0;
        s.generation += 1;
    }

    /// Runs `action`; true when the player changed and should be read again soon.
    fn act(&mut self, action: Action) -> bool {
        match action {
            Action::Start => {
                self.set_running(true);
                false
            }
            Action::Stop => {
                self.set_running(false);
                false
            }
            Action::Restart => {
                self.set_running(false);
                self.set_running(true);
                false
            }
            Action::Login => {
                self.login();
                false
            }
            Action::Logout => {
                let mut s = self.lock();
                s.auth.auth.clear();
                s.auth.refresh.clear();
                s.auth.expires_at = 0;
                s.state = None;
                s.generation += 1;
                let auth = s.auth.clone();
                drop(s);
                crate::config::save(&self.dir, "spotify.json", &auth);
                self.chat("§aSpotify HUD §8» §7Spotify login forgotten.");
                false
            }
            Action::SetPaused(paused) => {
                let playing = self.lock().state.as_ref().map(|s| s.0.playing);
                let path = match playing {
                    Some(true) if paused => "pause",
                    Some(false) if !paused => "play",
                    _ => return false,
                };
                self.optimistic(|s| s.playing = !paused);
                self.send("PUT", path);
                true
            }
            Action::ToggleShuffle => {
                let Some(on) = self.lock().state.as_ref().map(|s| s.0.shuffle) else { return false };
                self.optimistic(|s| s.shuffle = !on);
                self.send("PUT", &format!("shuffle?state={}", !on));
                true
            }
            Action::ToggleRepeat => {
                let Some(r) = self.lock().state.as_ref().map(|s| s.0.repeat) else { return false };
                let (next, state) = match r {
                    Repeat::Off => (Repeat::Song, "track"),
                    Repeat::Song => (Repeat::All, "context"),
                    Repeat::All => (Repeat::Off, "off"),
                };
                self.optimistic(|s| s.repeat = next);
                self.send("PUT", &format!("repeat?state={state}"));
                true
            }
            Action::Move(forward) => {
                self.send("POST", if forward { "next" } else { "previous" });
                true
            }
            Action::Volume(v) => {
                let v = v.min(100);
                self.optimistic(|s| s.volume = v);
                self.send("PUT", &format!("volume?volume_percent={v}"));
                true
            }
        }
    }

    fn optimistic(&self, f: impl FnOnce(&mut State)) {
        let mut s = self.lock();
        if let Some((state, at)) = s.state.as_mut() {
            // Keep the progress where it is shown when playing stops or starts.
            let shown = progress_now(state, *at);
            state.progress_ms = shown;
            *at = Instant::now();
            f(state);
            s.generation += 1;
        }
    }

    fn token(&mut self) -> Option<String> {
        let (auth, expired, has_refresh) = {
            let s = self.lock();
            (s.auth.auth.clone(), s.auth.expires_at != 0 && now_secs() + 30 >= s.auth.expires_at, !s.auth.refresh.is_empty())
        };
        if auth.is_empty() && !has_refresh {
            return None;
        }
        if (auth.is_empty() || expired) && has_refresh {
            self.refresh();
        }
        let t = self.lock().auth.auth.clone();
        (!t.is_empty()).then_some(t)
    }

    fn request(&self, method: &str, url: &str, token: &str) -> Reply {
        let req = self.agent.request(method, url).set("Authorization", &format!("Bearer {token}"));
        let res = if method == "GET" { req.call() } else { req.set("Content-Length", "0").send_string("") };
        match res {
            Ok(r) => {
                let status = r.status();
                Reply::Ok(status, r.into_string().unwrap_or_default())
            }
            Err(ureq::Error::Status(401, _)) => Reply::Unauthorized,
            Err(ureq::Error::Status(code, r)) => {
                let body = r.into_string().unwrap_or_default();
                let message = serde_json::from_str::<Value>(&body)
                    .ok()
                    .and_then(|v| v.pointer("/error/message").and_then(Value::as_str).map(str::to_owned))
                    .unwrap_or_default();
                Reply::Failed(format!("HTTP {code} {message}").trim().to_owned())
            }
            Err(e) => Reply::Failed(e.to_string()),
        }
    }

    /// A control request (`PUT`/`POST` on the player); a 401 refreshes the login once.
    fn send(&mut self, method: &str, path: &str) {
        let Some(token) = self.token() else { return };
        let url = format!("{API}/{path}");
        match self.request(method, &url, &token) {
            Reply::Unauthorized => {
                if self.refresh() {
                    let token = self.lock().auth.auth.clone();
                    if let Reply::Failed(e) = self.request(method, &url, &token) {
                        self.control_failed(&e);
                    }
                } else {
                    self.on_error("Unauthorized");
                }
            }
            Reply::Failed(e) => self.control_failed(&e),
            Reply::Ok(..) => {}
        }
    }

    fn control_failed(&self, error: &str) {
        // Spotify answers 403 to a free account's controls.
        if error.starts_with("HTTP 403") {
            self.chat(format!("§cSpotify HUD §8» §7Spotify said no ({error}): its controls are for Premium accounts."));
        } else if error.starts_with("HTTP 404") {
            self.chat("§cSpotify HUD §8» §7Nothing is playing on any device: start a song in Spotify first.");
        }
    }

    fn poll(&mut self) {
        let Some(token) = self.token() else { return };
        match self.request("GET", API, &token) {
            Reply::Unauthorized => {
                if !self.refresh() {
                    self.on_error("Unauthorized");
                }
            }
            Reply::Failed(e) => self.on_error(&e),
            Reply::Ok(200, body) if !body.trim().is_empty() => match parse_player(&body) {
                Ok(state) => self.on_success(state),
                Err(e) => self.on_error(&e),
            },
            // 204: nothing is playing anywhere; Jukebox keeps the last state.
            Reply::Ok(..) => {}
        }
    }

    fn on_success(&self, state: State) {
        let mut s = self.lock();
        s.errors = 0;
        let changed = s.state.as_ref().is_none_or(|(old, _)| *old != state);
        s.state = Some((state, Instant::now()));
        if changed {
            s.generation += 1;
        }
    }

    /// Jukebox's `onError`: the tenth error in a row stops the service.
    fn on_error(&mut self, error: &str) {
        let stop = {
            let mut s = self.lock();
            if s.errors >= 10 {
                true
            } else {
                s.errors += 1;
                false
            }
        };
        if stop {
            self.set_running(false);
            self.chat(format!("§cSpotify HUD §8» §7Gave up after ten errors in a row ({error}). §9/music restart§7 tries again."));
        }
    }

    /// A new access token from the refresh token; false when Spotify refused it.
    fn refresh(&mut self) -> bool {
        let (client_id, refresh) = {
            let s = self.lock();
            (s.auth.client_id.clone(), s.auth.refresh.clone())
        };
        if client_id.is_empty() || refresh.is_empty() {
            return false;
        }
        let res = self.agent.post("https://accounts.spotify.com/api/token").send_form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", &client_id),
        ]);
        match res.ok().and_then(|r| r.into_string().ok()).and_then(|b| serde_json::from_str::<Value>(&b).ok()) {
            Some(v) if v.get("access_token").is_some() => {
                self.store_tokens(&v);
                true
            }
            _ => {
                self.chat("§cSpotify HUD §8» §7The Spotify login expired: connect again with §9/music login§7.");
                let mut s = self.lock();
                s.auth.auth.clear();
                false
            }
        }
    }

    fn store_tokens(&self, v: &Value) {
        let mut s = self.lock();
        if let Some(a) = v.get("access_token").and_then(Value::as_str) {
            s.auth.auth = a.to_owned();
        }
        if let Some(r) = v.get("refresh_token").and_then(Value::as_str) {
            s.auth.refresh = r.to_owned();
        }
        let expires = v.get("expires_in").and_then(Value::as_u64).unwrap_or(3600);
        s.auth.expires_at = now_secs() + expires;
        let auth = s.auth.clone();
        drop(s);
        crate::config::save(&self.dir, "spotify.json", &auth);
    }

    /// Opens the browser on Spotify's login and waits up to two minutes for its answer on
    /// `REDIRECT`.
    fn login(&mut self) {
        let client_id = self.lock().auth.client_id.clone();
        if client_id.is_empty() {
            for line in crate::CLIENT_ID_HELP {
                self.chat(*line);
            }
            return;
        }
        if self.lock().login_pending {
            let _ = open::that_detached(format!("http://127.0.0.1:{PORT}/"));
            return;
        }
        let listener = match TcpListener::bind(("127.0.0.1", PORT)) {
            Ok(l) => l,
            Err(e) => {
                self.chat(format!("§cSpotify HUD §8» §7Port {PORT} is busy, so the login can't start: {e}"));
                return;
            }
        };
        let verifier = random_token(64);
        let state = random_token(16);
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.as_bytes()));
        let authorize = format!(
            "https://accounts.spotify.com/authorize?response_type=code&client_id={}&scope={}&redirect_uri={}&code_challenge_method=S256&code_challenge={}&state={}",
            encode(&client_id),
            encode(SCOPES),
            encode(REDIRECT),
            challenge,
            state
        );
        self.lock().login_pending = true;
        let shared = self.shared.clone();
        let agent = self.agent.clone();
        let dir = self.dir.clone();
        let tx = self.tx.clone();
        let page = authorize.clone();
        std::thread::Builder::new()
            .name("spotify-hud-login".into())
            .spawn(move || {
                let result = serve_login(&listener, &page, &state);
                let push = |line: String| shared.lock().unwrap_or_else(|e| e.into_inner()).chat.push(line);
                match result {
                    None => push("§cSpotify HUD §8» §7No answer from Spotify within two minutes; try §9/music login§7 again.".into()),
                    Some(Err(e)) => push(format!("§cSpotify HUD §8» §7Spotify didn't connect ({e}). Try §9/music login§7 again.")),
                    Some(Ok(code)) => {
                        let res = agent.post("https://accounts.spotify.com/api/token").send_form(&[
                            ("grant_type", "authorization_code"),
                            ("code", &code),
                            ("redirect_uri", REDIRECT),
                            ("client_id", &client_id),
                            ("code_verifier", &verifier),
                        ]);
                        let body = match res {
                            Ok(r) => r.into_string().unwrap_or_default(),
                            Err(ureq::Error::Status(_, r)) => r.into_string().unwrap_or_default(),
                            Err(e) => e.to_string(),
                        };
                        match serde_json::from_str::<Value>(&body) {
                            Ok(v) if v.get("access_token").is_some() => {
                                let w = Worker { shared: shared.clone(), dir, agent, tx: tx.clone(), running: false };
                                w.store_tokens(&v);
                                push("§aSpotify HUD §8» §7Connected to Spotify.".into());
                                let _ = tx.send(Action::Restart);
                                shared.lock().unwrap_or_else(|e| e.into_inner()).logged_in = true;
                            }
                            Ok(v) => {
                                let reason = v
                                    .get("error_description")
                                    .or_else(|| v.get("error"))
                                    .and_then(Value::as_str)
                                    .unwrap_or("Unknown error")
                                    .to_owned();
                                push(format!("§cSpotify HUD §8» §7Spotify didn't connect ({reason}). Try §9/music login§7 again."));
                            }
                            Err(_) => push(format!("§cSpotify HUD §8» §7Spotify didn't connect ({body}). Try §9/music login§7 again.")),
                        }
                    }
                }
                shared.lock().unwrap_or_else(|e| e.into_inner()).login_pending = false;
            })
            .expect("spotify_hud: spawn the login thread");
        if open::that_detached(&authorize).is_err() {
            self.chat(format!("§aSpotify HUD §8» §7Open this page to connect: {authorize}"));
        }
    }
}

/// The shown progress: the last answer's, plus the time since when playing.
pub fn progress_now(s: &State, at: Instant) -> u64 {
    if s.playing { s.progress_ms + at.elapsed().as_millis() as u64 } else { s.progress_ms }
}

fn random_token(len: usize) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut bytes = vec![0u8; len];
    if getrandom::fill(&mut bytes).is_err() {
        // A clock-seeded fallback; the code is also bound to this one listener.
        let mut x = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7) | 1;
        for b in &mut bytes {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = x as u8;
        }
    }
    bytes.iter().map(|b| CHARS[*b as usize % CHARS.len()] as char).collect()
}

/// Percent-encodes a query value.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                match std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()).ok_or(()) {
                    Ok(v) => {
                        out.push(v);
                        i += 3;
                        continue;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A query parameter of a request path.
pub fn query_param(path: &str, key: &str) -> Option<String> {
    let q = path.split_once('?')?.1;
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (k == key).then(|| decode(v))
    })
}

const PAGE: &str = "<!doctype html><html><head><meta charset=utf-8><title>Spotify HUD</title><style>body{background:#121212;color:#fff;font-family:sans-serif;display:grid;place-items:center;height:100vh;margin:0}div{text-align:center}h1{color:#1db954}</style></head><body><div><h1>Spotify HUD</h1><p>{}</p></div></body></html>";

fn respond(stream: &mut std::net::TcpStream, status: &str, extra: &str, message: &str) {
    let body = PAGE.replace("{}", message);
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}

/// Serves the login's local pages until Spotify sends the player back with a code, an error,
/// or two minutes pass (None).
fn serve_login(listener: &TcpListener, authorize: &str, state: &str) -> Option<Result<String, String>> {
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    listener.set_nonblocking(true).ok()?;
    while Instant::now() < deadline {
        let mut stream = match listener.accept() {
            Ok((s, _)) => s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(_) => continue,
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut line = String::new();
        if BufReader::new(&stream).read_line(&mut line).is_err() {
            continue;
        }
        let path = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
        if path.starts_with("/spotify/callback") {
            if query_param(&path, "state").as_deref() != Some(state) {
                respond(&mut stream, "400 Bad Request", "", "This login link is out of date. Run /music login in Minecraft again.");
                continue;
            }
            if let Some(code) = query_param(&path, "code") {
                respond(&mut stream, "200 OK", "", "You're logged in to Spotify. You can close this tab and go back to Minecraft.");
                return Some(Ok(code));
            }
            let error = query_param(&path, "error").unwrap_or_else(|| "no code".into());
            respond(&mut stream, "200 OK", "", "Spotify didn't log you in. Go back to Minecraft and try again.");
            return Some(Err(error));
        } else if path == "/" || path.starts_with("/login") {
            respond(&mut stream, "302 Found", &format!("Location: {authorize}\r\n"), "Redirecting to Spotify…");
        } else {
            respond(&mut stream, "404 Not Found", "", "Not found.");
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACK: &str = r#"{"device":{"id":"x","is_private_session":false,"volume_percent":64},"shuffle_state":true,
      "repeat_state":"context","progress_ms":61000,"is_playing":true,"currently_playing_type":"track",
      "item":{"duration_ms":200000,"name":"Song","is_local":false,"artists":[{"name":"A"},{"name":"B"}],
      "album":{"images":[{"height":64,"width":64,"url":"small"},{"height":640,"width":640,"url":"big"}]},
      "external_urls":{"spotify":"https://open.spotify.com/track/1"}}}"#;

    #[test]
    fn reads_a_track() {
        let s = parse_player(TRACK).unwrap();
        assert_eq!(s.song.title, "Song");
        assert_eq!(s.song.artists, vec!["A", "B"]);
        assert_eq!(s.song.cover, "big");
        assert_eq!(s.song.url, "https://open.spotify.com/track/1");
        assert_eq!((s.shuffle, s.repeat, s.volume, s.playing), (true, Repeat::All, 64, true));
        assert_eq!((s.progress_ms, s.duration_ms), (61000, 200000));
    }

    #[test]
    fn reads_private_local_and_errors() {
        let p = parse_player(r#"{"device":{"is_private_session":true,"volume_percent":null},"is_playing":false}"#).unwrap();
        assert_eq!(p.song.title, "Private Session");
        let l = parse_player(r#"{"device":{"volume_percent":10},"is_playing":true,"progress_ms":5,"item":{"duration_ms":9,"name":"mine.mp3","is_local":true}}"#).unwrap();
        assert_eq!(l.song.title, "mine.mp3");
        assert!(l.song.artists.is_empty());
        assert!(parse_player(r#"{"error":{"status":401,"message":"The access token expired"}}"#).is_err());
        let none = parse_player(r#"{"device":{"volume_percent":10},"is_playing":false,"item":null}"#).unwrap();
        assert_eq!(none.song.title, "No song playing");
    }

    #[test]
    fn queries_decode() {
        assert_eq!(query_param("/spotify/callback?code=a%2Fb+c&state=xy", "code").as_deref(), Some("a/b c"));
        assert_eq!(query_param("/spotify/callback?code=a&state=xy", "state").as_deref(), Some("xy"));
        assert_eq!(query_param("/x", "code"), None);
        assert_eq!(encode("a b/c"), "a%20b%2Fc");
    }

    #[test]
    fn tokens_are_random_and_long() {
        let a = random_token(64);
        assert_eq!(a.len(), 64);
        assert_ne!(a, random_token(64));
    }
}
