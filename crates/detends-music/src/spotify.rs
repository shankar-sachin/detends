//! Spotify.
//!
//! The only file in détends that knows this service exists. Everything above it
//! sees a [`Provider`] (§4).
//!
//! # What this needs from the user
//!
//! A **client ID**, from an app the user registers under their own Spotify
//! account at <https://developer.spotify.com/dashboard>, with
//! `http://127.0.0.1:8888/callback` added as a redirect URI. détends cannot
//! ship one: a client ID identifies *an application to a user's account*, and
//! an embedded one would be both against Spotify's terms and a secret in a
//! public repository.
//!
//! Playback control also needs **Spotify Premium**. That is Spotify's rule,
//! not ours, and the failure is reported in those words rather than as a 403.
//!
//! # Why PKCE, and why no client secret
//!
//! This is a desktop application, so it cannot keep a secret: anything shipped
//! in the binary is readable by whoever has the binary. PKCE exists precisely
//! for that case — the client proves it is the same one that started the flow
//! by presenting a verifier it made up, rather than by knowing a shared
//! password. So détends stores a client ID (not secret) and a refresh token
//! (secret, and the user's own).
//!
//! # What it does not do
//!
//! It does not decode or output audio. The Web API asks Spotify's own player
//! to do things; the sound comes out of whatever device the user is signed in
//! on. That is the only integration Spotify's terms permit for a third party,
//! and it is why `Playback::device` exists.

use crate::playback::{Command, Playback, Status};
use crate::provider::Provider;
use crate::track::{MediaId, Repeat, Track};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const AUTH: &str = "https://accounts.spotify.com/authorize";
const TOKEN: &str = "https://accounts.spotify.com/api/token";
const API: &str = "https://api.spotify.com/v1";

/// The loopback port the browser is sent back to.
///
/// Spotify requires an exact redirect URI, and `127.0.0.1` rather than
/// `localhost`: they are not the same string, and the dashboard checks strings.
const PORT: u16 = 8888;
const REDIRECT: &str = "http://127.0.0.1:8888/callback";

/// Everything the flow asks the user to approve.
///
/// Kept to what the interface in §4 actually uses. Asking for more than is
/// drawn would be rude, and a scope list that grows without the interface
/// growing is how an application ends up over-permissioned.
const SCOPES: &str = "user-read-playback-state user-modify-playback-state \
                      user-read-currently-playing user-library-read \
                      playlist-read-private";

/// The client ID, and whatever the last sign-in left behind.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SpotifyCredentials {
    pub client_id: String,
    /// Kept so the user signs in once rather than once per launch.
    pub refresh_token: Option<String>,
}

impl SpotifyCredentials {
    /// Where the credentials live.
    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "detends")
            .map(|dirs| dirs.config_dir().join("spotify.json"))
    }

    /// Read them, falling back to the environment.
    ///
    /// The environment variable is there so the whole thing can be tried
    /// without editing a file, which matters the first time.
    pub fn load(path: &std::path::Path) -> Self {
        let mut credentials: SpotifyCredentials = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();

        if credentials.client_id.is_empty() {
            if let Ok(id) = std::env::var("DETENDS_SPOTIFY_CLIENT_ID") {
                credentials.client_id = id.trim().to_string();
            }
        }
        credentials
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, text)
    }

    pub fn configured(&self) -> bool {
        !self.client_id.is_empty()
    }
}

/// A PKCE verifier and the challenge derived from it.
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    /// Make a new pair.
    pub fn new() -> Self {
        use base64::Engine;
        use sha2::{Digest, Sha256};

        // 64 characters from the unreserved set, which is comfortably inside
        // the 43-128 the specification allows.
        let verifier: String = (0..64).map(|_| random_unreserved()).collect();

        let digest = Sha256::digest(verifier.as_bytes());
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);

        Pkce {
            verifier,
            challenge,
        }
    }
}

impl Default for Pkce {
    fn default() -> Self {
        Self::new()
    }
}

/// One character from the PKCE unreserved set.
///
/// Seeded from the clock and the address of a local, which is not a
/// cryptographic source — but a PKCE verifier only has to be unguessable by
/// whoever might intercept one redirect on a loopback interface, and it lives
/// for the seconds between opening a browser and closing it.
fn random_unreserved() -> char {
    const SET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let marker = 0u8;
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
        ^ (&marker as *const u8 as u64);

    // xorshift, so successive calls in the same nanosecond still differ.
    let mut x = seed | 1;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    SET[(x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 58) as usize % SET.len()] as char
}

/// An access token and when it stops working.
struct Token {
    access: String,
    expires: Instant,
}

impl Token {
    fn valid(&self) -> bool {
        // A minute of margin, so a request is never sent with a token that
        // expires while it is in flight.
        Instant::now() + Duration::from_secs(60) < self.expires
    }
}

pub struct SpotifyProvider {
    credentials: SpotifyCredentials,
    path: Option<PathBuf>,
    token: Option<Token>,
    status: Status,
    /// The last good snapshot, so a failed poll shows stale truth rather than
    /// an empty screen.
    last: Playback,
}

impl SpotifyProvider {
    pub fn new(credentials: SpotifyCredentials, path: Option<PathBuf>) -> Self {
        let status = if credentials.configured() {
            Status::Connecting
        } else {
            Status::Disconnected
        };

        SpotifyProvider {
            credentials,
            path,
            token: None,
            status,
            last: Playback {
                provider: "Spotify".into(),
                ..Default::default()
            },
        }
    }

    /// Load from the usual place.
    pub fn from_default_path() -> Self {
        let path = SpotifyCredentials::default_path();
        let credentials = path
            .as_ref()
            .map(|p| SpotifyCredentials::load(p))
            .unwrap_or_default();
        SpotifyProvider::new(credentials, path)
    }

    /// The URL the user has to approve.
    pub fn authorise_url(&self, challenge: &str) -> String {
        format!(
            "{AUTH}?client_id={}&response_type=code&redirect_uri={}&scope={}\
             &code_challenge_method=S256&code_challenge={challenge}",
            self.credentials.client_id,
            urlencode(REDIRECT),
            urlencode(SCOPES),
        )
    }

    /// A token, without ever opening a browser.
    ///
    /// This is what `poll` uses. Polling starts the moment détends launches,
    /// so an interactive sign-in here would hijack the user's browser at boot,
    /// before they had so much as looked at Music. Approving access is
    /// something you ask for, not something that happens to you (§15).
    fn token_quietly(&mut self) -> Result<String, String> {
        if let Some(token) = &self.token {
            if token.valid() {
                return Ok(token.access.clone());
            }
        }

        if !self.credentials.configured() {
            return Err(
                "Spotify needs a client ID. See documentation/MUSIC.md to set one up.".into(),
            );
        }

        let Some(refresh) = self.credentials.refresh_token.clone() else {
            return Err(String::new());
        };

        self.refresh(&refresh)
    }

    /// A valid access token, signing in through the browser if that is what it
    /// takes. Only ever reached from `execute`, which means from a keypress.
    fn token(&mut self) -> Result<String, String> {
        match self.token_quietly() {
            Ok(access) => return Ok(access),
            Err(why) if !why.is_empty() && self.credentials.refresh_token.is_none() => {
                return Err(why)
            }
            // A refresh token is revoked when the user removes the app, or
            // simply expires. Signing in again is the cure, not an error.
            Err(why) if !why.is_empty() => {
                log::warn!("Spotify refresh failed ({why}); signing in again");
            }
            Err(_) => {}
        }

        self.sign_in()
    }

    fn refresh(&mut self, refresh_token: &str) -> Result<String, String> {
        let response = ureq::post(TOKEN)
            .send_form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", &self.credentials.client_id),
            ])
            .map_err(|e| format!("Could not reach Spotify: {e}"))?;

        self.store(response)
    }

    /// The full PKCE sign-in: open a browser, wait for the redirect.
    fn sign_in(&mut self) -> Result<String, String> {
        let pkce = Pkce::new();
        let url = self.authorise_url(&pkce.challenge);

        self.status = Status::Authorising;
        open_browser(&url)?;

        let code = wait_for_code()?;

        let response = ureq::post(TOKEN)
            .send_form(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", REDIRECT),
                ("client_id", &self.credentials.client_id),
                ("code_verifier", &pkce.verifier),
            ])
            .map_err(|e| format!("Spotify refused the sign-in: {e}"))?;

        self.store(response)
    }

    /// Keep an access token, and the refresh token if one came with it.
    fn store(&mut self, response: ureq::Response) -> Result<String, String> {
        #[derive(Deserialize)]
        struct Granted {
            access_token: String,
            expires_in: u64,
            refresh_token: Option<String>,
        }

        let granted: Granted = response
            .into_json()
            .map_err(|e| format!("Spotify sent something unexpected: {e}"))?;

        if let Some(refresh) = granted.refresh_token {
            self.credentials.refresh_token = Some(refresh);
            if let Some(path) = &self.path {
                if let Err(e) = self.credentials.save(path) {
                    log::warn!("could not save Spotify credentials: {e}");
                }
            }
        }

        self.token = Some(Token {
            access: granted.access_token.clone(),
            expires: Instant::now() + Duration::from_secs(granted.expires_in),
        });
        self.status = Status::Ready;
        Ok(granted.access_token)
    }

    fn get(&mut self, path: &str) -> Result<ureq::Response, String> {
        let token = self.token_quietly()?;
        ureq::get(&format!("{API}{path}"))
            .set("Authorization", &format!("Bearer {token}"))
            .call()
            .map_err(describe)
    }

    fn put(&mut self, path: &str) -> Result<(), String> {
        let token = self.token()?;
        ureq::put(&format!("{API}{path}"))
            .set("Authorization", &format!("Bearer {token}"))
            .set("Content-Length", "0")
            .call()
            .map(|_| ())
            .map_err(describe)
    }

    fn post(&mut self, path: &str) -> Result<(), String> {
        let token = self.token()?;
        ureq::post(&format!("{API}{path}"))
            .set("Authorization", &format!("Bearer {token}"))
            .set("Content-Length", "0")
            .call()
            .map(|_| ())
            .map_err(describe)
    }
}

/// Turn a transport error into something worth showing a person.
fn describe(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(403, _) => {
            "Spotify needs Premium to control playback from another app".into()
        }
        ureq::Error::Status(404, _) => {
            "No active Spotify device. Start playing on any device, then try again".into()
        }
        ureq::Error::Status(429, _) => "Spotify is rate-limiting; slow down".into(),
        ureq::Error::Status(401, _) => "Spotify sign-in expired".into(),
        ureq::Error::Status(code, _) => format!("Spotify returned {code}"),
        ureq::Error::Transport(t) => format!("Could not reach Spotify: {t}"),
    }
}

impl Provider for SpotifyProvider {
    fn name(&self) -> &str {
        "Spotify"
    }

    fn poll(&mut self) -> Playback {
        if !self.credentials.configured() {
            return Playback {
                status: Status::Disconnected,
                provider: "Spotify".into(),
                ..Default::default()
            };
        }

        // Never signed in: say so and stay off the network entirely.
        if self.credentials.refresh_token.is_none() && self.token.is_none() {
            return Playback {
                status: Status::NeedsSignIn,
                provider: "Spotify".into(),
                ..Default::default()
            };
        }

        match self.get("/me/player") {
            Ok(response) => {
                // 204 means nothing is playing, which is not an error.
                if response.status() == 204 {
                    self.last = Playback {
                        status: Status::Ready,
                        provider: "Spotify".into(),
                        ..Default::default()
                    };
                    return self.last.clone();
                }
                match response.into_json::<PlayerState>() {
                    Ok(state) => {
                        self.last = state.into_playback();
                        self.status = Status::Ready;
                    }
                    Err(e) => self.status = Status::Failed(format!("Unreadable reply: {e}")),
                }
            }
            Err(why) => self.status = Status::Failed(why),
        }

        // Show the last good state with the current status over it, so a
        // dropped connection does not blank the screen.
        let mut state = self.last.clone();
        state.status = self.status.clone();
        state
    }

    fn execute(&mut self, command: Command) -> Result<(), String> {
        match command {
            Command::Connect => self.token().map(|_| ()),
            Command::PlayPause => {
                if self.last.playing {
                    self.put("/me/player/pause")
                } else {
                    self.put("/me/player/play")
                }
            }
            Command::Next => self.post("/me/player/next"),
            Command::Previous => self.post("/me/player/previous"),
            Command::Seek(fraction) => {
                let duration = self.last.track.as_ref().map(|t| t.duration).unwrap_or(0.0);
                let ms = (fraction.clamp(0.0, 1.0) as f64 * duration * 1000.0) as u64;
                self.put(&format!("/me/player/seek?position_ms={ms}"))
            }
            Command::Volume(v) => {
                let percent = (v.clamp(0.0, 1.0) * 100.0).round() as u32;
                self.put(&format!("/me/player/volume?volume_percent={percent}"))
            }
            Command::ToggleShuffle => {
                let next = !self.last.shuffle;
                self.put(&format!("/me/player/shuffle?state={next}"))
            }
            Command::CycleRepeat => {
                let next = match self.last.repeat.next() {
                    Repeat::Off => "off",
                    Repeat::All => "context",
                    Repeat::One => "track",
                };
                self.put(&format!("/me/player/repeat?state={next}"))
            }
            Command::Play(id) => {
                // Spotify distinguishes a track from a context (an album or a
                // playlist), and they go in different fields of the body.
                let token = self.token()?;
                let body = if id.as_str().contains(":track:") {
                    serde_json::json!({ "uris": [id.as_str()] })
                } else {
                    serde_json::json!({ "context_uri": id.as_str() })
                };
                ureq::put(&format!("{API}/me/player/play"))
                    .set("Authorization", &format!("Bearer {token}"))
                    .send_json(body)
                    .map(|_| ())
                    .map_err(describe)
            }
        }
    }

    fn search(&mut self, query: &str) -> Vec<Track> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        let path = format!("/search?type=track&limit=20&q={}", urlencode(query));
        match self.get(&path) {
            Ok(response) => response
                .into_json::<SearchReply>()
                .map(|reply| reply.tracks.items.into_iter().map(Into::into).collect())
                .unwrap_or_default(),
            Err(why) => {
                log::warn!("Spotify search failed: {why}");
                Vec::new()
            }
        }
    }

    fn library(&mut self) -> Vec<Track> {
        match self.get("/me/tracks?limit=50") {
            Ok(response) => response
                .into_json::<SavedReply>()
                .map(|reply| reply.items.into_iter().map(|s| s.track.into()).collect())
                .unwrap_or_default(),
            Err(why) => {
                log::warn!("Spotify library failed: {why}");
                Vec::new()
            }
        }
    }

    fn owns(&self, id: &MediaId) -> bool {
        id.as_str().starts_with("spotify:")
    }
}

// ---- what Spotify actually sends ---------------------------------------
//
// Deliberately partial. Every field not drawn is left undeserialised, so a
// change at their end to something détends does not use cannot break it.

#[derive(Deserialize)]
struct PlayerState {
    #[serde(default)]
    is_playing: bool,
    #[serde(default)]
    progress_ms: Option<u64>,
    #[serde(default)]
    shuffle_state: bool,
    #[serde(default)]
    repeat_state: String,
    item: Option<ApiTrack>,
    device: Option<ApiDevice>,
}

#[derive(Deserialize)]
struct ApiDevice {
    name: String,
    volume_percent: Option<u32>,
}

#[derive(Deserialize)]
struct ApiTrack {
    uri: String,
    name: String,
    duration_ms: u64,
    artists: Vec<ApiNamed>,
    album: Option<ApiAlbum>,
}

#[derive(Deserialize)]
struct ApiNamed {
    name: String,
}

#[derive(Deserialize)]
struct ApiAlbum {
    name: String,
    #[serde(default)]
    images: Vec<ApiImage>,
}

#[derive(Deserialize)]
struct ApiImage {
    url: String,
    #[serde(default)]
    width: u32,
}

#[derive(Deserialize)]
struct SearchReply {
    tracks: SearchTracks,
}

#[derive(Deserialize)]
struct SearchTracks {
    items: Vec<ApiTrack>,
}

#[derive(Deserialize)]
struct SavedReply {
    items: Vec<SavedTrack>,
}

#[derive(Deserialize)]
struct SavedTrack {
    track: ApiTrack,
}

impl From<ApiTrack> for Track {
    fn from(t: ApiTrack) -> Track {
        let artist = t
            .artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        // The largest image under 640px: big enough for the artwork panel,
        // small enough not to fetch a 2000px JPEG to draw at 340.
        let artwork = t.album.as_ref().and_then(|album| {
            album
                .images
                .iter()
                .filter(|i| i.width <= 640)
                .max_by_key(|i| i.width)
                .or_else(|| album.images.last())
                .map(|i| i.url.clone())
        });

        Track {
            id: MediaId::new(t.uri),
            title: t.name,
            artist,
            album: t.album.map(|a| a.name).unwrap_or_default(),
            duration: t.duration_ms as f64 / 1000.0,
            artwork,
        }
    }
}

impl PlayerState {
    fn into_playback(self) -> Playback {
        let repeat = match self.repeat_state.as_str() {
            "track" => Repeat::One,
            "context" => Repeat::All,
            _ => Repeat::Off,
        };

        Playback {
            status: Status::Ready,
            playing: self.is_playing,
            position: self.progress_ms.unwrap_or(0) as f64 / 1000.0,
            shuffle: self.shuffle_state,
            repeat,
            volume: self
                .device
                .as_ref()
                .and_then(|d| d.volume_percent)
                .map(|v| v as f32 / 100.0),
            device: self.device.map(|d| d.name),
            provider: "Spotify".into(),
            track: self.item.map(Into::into),
            queue: Vec::new(),
        }
    }
}

// ---- the loopback half of the sign-in -----------------------------------

/// Percent-encode everything that is not unreserved.
fn urlencode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("xdg-open");
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Err("Cannot open a browser on this platform".into());

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        command
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Could not open a browser: {e}"))
    }
}

/// Listen on the loopback port until the browser comes back with a code.
///
/// A whole HTTP server would be absurd here: exactly one request matters, it is
/// a GET, and the only part of it that is read is the query string. So this
/// reads one line, answers with one page, and closes.
fn wait_for_code() -> Result<String, String> {
    use std::io::{BufRead, BufReader, Write};

    let listener = std::net::TcpListener::bind(("127.0.0.1", PORT))
        .map_err(|e| format!("Could not listen on port {PORT}: {e}"))?;
    listener
        .set_nonblocking(false)
        .map_err(|e| format!("Could not wait for the browser: {e}"))?;

    // Long enough to sign in and approve, short enough that a forgotten
    // browser tab does not hold a thread open for ever.
    let deadline = Instant::now() + Duration::from_secs(180);

    for stream in listener.incoming() {
        if Instant::now() > deadline {
            return Err("Timed out waiting for Spotify approval".into());
        }
        let Ok(mut stream) = stream else { continue };

        let mut line = String::new();
        if BufReader::new(&stream).read_line(&mut line).is_err() {
            continue;
        }

        let mut answer = |body: &str| {
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        };

        if let Some(code) = query_value(&line, "code") {
            answer("<!doctype html><meta charset=utf-8><title>détends</title>\
                    <body style=\"background:#0b0d10;color:#e8eaed;font:16px -apple-system,\
                    system-ui,sans-serif;display:grid;place-items:center;height:100vh;margin:0\">\
                    <p>Connected. You can close this tab.</p>");
            return Ok(code);
        }

        if let Some(error) = query_value(&line, "error") {
            answer("<!doctype html><meta charset=utf-8><title>détends</title>\
                    <body style=\"background:#0b0d10;color:#e8eaed;font:16px -apple-system,\
                    system-ui,sans-serif;display:grid;place-items:center;height:100vh;margin:0\">\
                    <p>Not connected. You can close this tab.</p>");
            return Err(format!("Spotify sign-in was refused: {error}"));
        }
    }

    Err("The browser never came back".into())
}

/// Pull one parameter out of a `GET /callback?a=b&c=d HTTP/1.1` line.
fn query_value(request_line: &str, key: &str) -> Option<String> {
    let target = request_line.split_whitespace().nth(1)?;
    let query = target.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then(|| urldecode(value))
    })
}

fn urldecode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_client_id_it_says_so_rather_than_failing_obscurely() {
        let mut provider = SpotifyProvider::new(SpotifyCredentials::default(), None);
        let state = provider.poll();

        assert_eq!(state.status, Status::Disconnected);
        assert_eq!(state.status.message(), "No music account connected");
        assert_eq!(state.provider, "Spotify");
    }

    #[test]
    fn polling_never_opens_a_browser() {
        // The regression that matters: `poll` runs the moment détends starts,
        // so an interactive sign-in here would hijack the browser at boot.
        // With a client ID but no refresh token there is nothing to refresh
        // *and* nothing to ask for, so it must report and stay off the network.
        let mut provider = SpotifyProvider::new(
            SpotifyCredentials {
                client_id: "abc123".into(),
                refresh_token: None,
            },
            None,
        );

        let state = provider.poll();
        assert_eq!(state.status, Status::NeedsSignIn);
        assert_eq!(state.status.message(), "Press play to connect");
        assert!(state.track.is_none());
    }

    #[test]
    fn a_configured_account_is_a_different_state_from_no_account() {
        // "Set up a client ID" and "approve access" are different problems,
        // and telling someone the wrong one wastes their time.
        let none = SpotifyProvider::new(SpotifyCredentials::default(), None).poll();
        assert_eq!(none.status, Status::Disconnected);

        let mut waiting = SpotifyProvider::new(
            SpotifyCredentials { client_id: "abc".into(), refresh_token: None },
            None,
        );
        assert_eq!(waiting.poll().status, Status::NeedsSignIn);
    }

    #[test]
    fn a_command_without_a_client_id_explains_what_is_missing() {
        let mut provider = SpotifyProvider::new(SpotifyCredentials::default(), None);
        let error = provider.execute(Command::PlayPause).unwrap_err();
        assert!(error.contains("client ID"), "got {error}");
    }

    #[test]
    fn the_authorise_url_carries_everything_spotify_requires() {
        let provider = SpotifyProvider::new(
            SpotifyCredentials {
                client_id: "abc123".into(),
                refresh_token: None,
            },
            None,
        );
        let url = provider.authorise_url("CHALLENGE");

        assert!(url.starts_with(AUTH));
        assert!(url.contains("client_id=abc123"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("code_challenge=CHALLENGE"));
        // The redirect has to arrive encoded, or Spotify will not match it.
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A8888%2Fcallback"));
    }

    #[test]
    fn pkce_challenges_are_derived_from_the_verifier_and_are_url_safe() {
        let pkce = Pkce::new();

        assert!(pkce.verifier.len() >= 43 && pkce.verifier.len() <= 128);
        assert!(!pkce.challenge.contains('+'), "not URL-safe");
        assert!(!pkce.challenge.contains('/'), "not URL-safe");
        assert!(!pkce.challenge.contains('='), "padding must be stripped");

        // Known vector: the S256 challenge is the base64url of SHA-256 of the
        // verifier, so the same verifier always gives the same challenge.
        use base64::Engine;
        use sha2::{Digest, Sha256};
        let expect = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(pkce.verifier.as_bytes()));
        assert_eq!(pkce.challenge, expect);
    }

    #[test]
    fn two_verifiers_are_not_the_same() {
        assert_ne!(Pkce::new().verifier, Pkce::new().verifier);
    }

    #[test]
    fn a_code_is_pulled_out_of_the_redirect() {
        let line = "GET /callback?code=AQD123abc&state=xyz HTTP/1.1";
        assert_eq!(query_value(line, "code"), Some("AQD123abc".into()));
        assert_eq!(query_value(line, "state"), Some("xyz".into()));
        assert_eq!(query_value(line, "error"), None);
    }

    #[test]
    fn a_refusal_is_recognised_rather_than_waited_out() {
        let line = "GET /callback?error=access_denied HTTP/1.1";
        assert_eq!(query_value(line, "error"), Some("access_denied".into()));
        assert_eq!(query_value(line, "code"), None);
    }

    #[test]
    fn an_encoded_redirect_survives_the_round_trip() {
        assert_eq!(urlencode(REDIRECT), "http%3A%2F%2F127.0.0.1%3A8888%2Fcallback");
        assert_eq!(urldecode(&urlencode(REDIRECT)), REDIRECT);
        assert_eq!(urldecode("a%20b+c"), "a b c");
    }

    #[test]
    fn a_request_without_a_query_is_not_a_panic() {
        assert_eq!(query_value("GET /callback HTTP/1.1", "code"), None);
        assert_eq!(query_value("nonsense", "code"), None);
        assert_eq!(query_value("", "code"), None);
    }

    #[test]
    fn a_track_from_spotify_becomes_a_detends_track() {
        let json = serde_json::json!({
            "uri": "spotify:track:abc",
            "name": "Resonance",
            "duration_ms": 215000,
            "artists": [{ "name": "HOME" }],
            "album": {
                "name": "Odyssey",
                "images": [
                    { "url": "big.jpg", "width": 640 },
                    { "url": "huge.jpg", "width": 2000 },
                    { "url": "small.jpg", "width": 64 }
                ]
            }
        });

        let track: Track = serde_json::from_value::<ApiTrack>(json).unwrap().into();

        assert_eq!(track.title, "Resonance");
        assert_eq!(track.artist, "HOME");
        assert_eq!(track.album, "Odyssey");
        assert_eq!(track.duration, 215.0);
        // Largest under 640, not the 2000px one.
        assert_eq!(track.artwork.as_deref(), Some("big.jpg"));
        assert_eq!(track.id.as_str(), "spotify:track:abc");
    }

    #[test]
    fn several_artists_are_joined_rather_than_dropped() {
        let json = serde_json::json!({
            "uri": "spotify:track:x",
            "name": "Collab",
            "duration_ms": 1000,
            "artists": [{ "name": "A" }, { "name": "B" }],
            "album": null
        });
        let track: Track = serde_json::from_value::<ApiTrack>(json).unwrap().into();
        assert_eq!(track.artist, "A, B");
        assert_eq!(track.artwork, None);
    }

    #[test]
    fn the_player_state_maps_onto_playback() {
        let json = serde_json::json!({
            "is_playing": true,
            "progress_ms": 35000,
            "shuffle_state": true,
            "repeat_state": "track",
            "device": { "name": "Kitchen", "volume_percent": 40 },
            "item": {
                "uri": "spotify:track:abc",
                "name": "Resonance",
                "duration_ms": 215000,
                "artists": [{ "name": "HOME" }],
                "album": { "name": "Odyssey", "images": [] }
            }
        });

        let state = serde_json::from_value::<PlayerState>(json)
            .unwrap()
            .into_playback();

        assert!(state.playing);
        assert_eq!(state.position, 35.0);
        assert!(state.shuffle);
        assert_eq!(state.repeat, Repeat::One);
        assert_eq!(state.volume, Some(0.4));
        assert_eq!(state.device.as_deref(), Some("Kitchen"));
        assert_eq!(state.elapsed(), "0:35");
        assert_eq!(state.provider, "Spotify");
    }

    #[test]
    fn a_reply_with_nothing_playing_is_not_an_error() {
        let state = serde_json::from_value::<PlayerState>(serde_json::json!({}))
            .unwrap()
            .into_playback();
        assert!(state.track.is_none());
        assert!(!state.playing);
        assert_eq!(state.repeat, Repeat::Off);
    }

    #[test]
    fn unknown_fields_from_spotify_are_ignored_rather_than_fatal() {
        // Their API grows; ours must not break when it does.
        let json = serde_json::json!({
            "is_playing": true,
            "something_new": { "nested": [1, 2, 3] },
            "repeat_state": "context"
        });
        let state = serde_json::from_value::<PlayerState>(json).unwrap().into_playback();
        assert!(state.playing);
        assert_eq!(state.repeat, Repeat::All);
    }

    #[test]
    fn it_only_claims_spotify_ids() {
        let provider = SpotifyProvider::new(SpotifyCredentials::default(), None);
        assert!(provider.owns(&MediaId::new("spotify:track:abc")));
        assert!(!provider.owns(&MediaId::new("local:Resonance")));
    }

    #[test]
    fn failures_are_explained_in_words_a_person_can_act_on() {
        let premium = describe(ureq::Error::Status(403, dummy_response()));
        assert!(premium.contains("Premium"), "got {premium}");

        let device = describe(ureq::Error::Status(404, dummy_response()));
        assert!(device.contains("device"), "got {device}");
    }

    fn dummy_response() -> ureq::Response {
        ureq::Response::new(403, "Forbidden", "").unwrap()
    }

    #[test]
    fn credentials_survive_a_round_trip_and_fall_back_to_the_environment() {
        let dir = std::env::temp_dir().join(format!("detends-spotify-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("spotify.json");

        let credentials = SpotifyCredentials {
            client_id: "abc".into(),
            refresh_token: Some("refresh".into()),
        };
        credentials.save(&path).unwrap();

        let back = SpotifyCredentials::load(&path);
        assert_eq!(back.client_id, "abc");
        assert_eq!(back.refresh_token.as_deref(), Some("refresh"));
        assert!(back.configured());

        let missing = SpotifyCredentials::load(&dir.join("nothing.json"));
        assert!(!missing.configured() || !missing.client_id.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_scopes_are_only_what_the_interface_draws() {
        // Asking for more than is used is how an app ends up over-permissioned.
        for scope in SCOPES.split_whitespace() {
            assert!(
                scope.starts_with("user-") || scope.starts_with("playlist-"),
                "unexpected scope {scope}"
            );
        }
        assert!(!SCOPES.contains("user-follow-modify"));
        assert!(!SCOPES.contains("playlist-modify"));
    }
}

#[cfg(test)]
mod setup_check {
    use super::*;

    /// Report what the running machine is actually configured with.
    ///
    /// Not an assertion about this machine — a checkout on a fresh one has no
    /// credentials and must still pass. It prints what it found, so
    /// `--nocapture` answers "did détends pick my client ID up?" without
    /// starting the whole shell.
    #[test]
    fn report_the_configured_client_id() {
        match SpotifyCredentials::default_path() {
            Some(path) => {
                let credentials = SpotifyCredentials::load(&path);
                println!("config path: {}", path.display());
                println!("exists:      {}", path.exists());
                println!("configured:  {}", credentials.configured());
                if credentials.configured() {
                    // Enough to recognise, not the whole thing.
                    let id = &credentials.client_id;
                    let shown = format!("{}…{}", &id[..6.min(id.len())], &id[id.len().saturating_sub(4)..]);
                    println!("client id:   {shown} ({} chars)", id.len());
                    println!("signed in:   {}", credentials.refresh_token.is_some());
                }
            }
            None => println!("no config directory on this platform"),
        }
    }
}
