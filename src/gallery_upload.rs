// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Sends an event's photos to the operator's own website, which publishes a private
//! gallery (`docs/specs/GALLERY_UPLOAD.md`).
//!
//! The only network exit of BoothSoft: off until an address and a key are set, started
//! by hand in maintenance, never during an event. Everything here runs on a worker
//! thread (blocking HTTP, file reading, hashing), never on the interface thread.
//!
//! Trust: a booth key (`bsk_…`, created on the website) plus the website owner's current
//! two-factor code, typed on the booth for each sending. Each piece then carries the
//! sending's own token.

use serde::{de::DeserializeOwned, Deserialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

/// File of the data folder holding the booth key (never shown again once saved).
pub const KEY_FILE: &str = "upload-key";
const KEY_PREFIX: &str = "bsk_";
/// Attempts per piece before the sending stops (network cuts are common on venue Wi-Fi).
const PIECE_ATTEMPTS: u32 = 6;
/// The website publishes the gallery in the background: how long to wait for it.
const PUBLISH_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// Website address and booth key, checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Server {
    base: String,
    key: String,
}

impl Server {
    pub fn new(address: &str, key: &str) -> Result<Self, String> {
        Ok(Self { base: normalize_address(address)?, key: valid_key(key)?.to_string() })
    }
    fn url(&self, path: &str) -> String { format!("{}{path}", self.base) }
    fn bearer(&self) -> String { format!("Bearer {}", self.key) }
}

/// `https://site.example/` → `https://site.example`. Plain HTTP only for this computer
/// (tests against a local copy of the website).
pub fn normalize_address(raw: &str) -> Result<String, String> {
    let address = raw.trim().trim_end_matches('/').to_string();
    let local = ["http://localhost", "http://127.0.0.1"].iter().any(|p| address == *p || address.starts_with(&format!("{p}:")) || address.starts_with(&format!("{p}/")));
    let host = address.strip_prefix("https://").unwrap_or("");
    // No user part (`http://localhost:1@other.site` would send the key in clear to
    // other.site), no query, no fragment.
    if address.len() > 200 || address.chars().any(|c| c.is_whitespace() || matches!(c, '?' | '#' | '@' | '\\')) || (!local && (host.is_empty() || !host.contains('.'))) {
        return Err(crate::tr!(fr: "Adresse du site invalide : elle commence par https://", en: "Invalid website address: it starts with https://").into());
    }
    Ok(address)
}

pub fn valid_key(raw: &str) -> Result<&str, String> {
    let key = raw.trim();
    if key.starts_with(KEY_PREFIX) && (20..=200).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        Ok(key)
    } else {
        Err(crate::tr!(fr: "Clé invalide : elle commence par bsk_ (copiez-la depuis le site)", en: "Invalid key: it starts with bsk_ (copy it from the website)").into())
    }
}

/// `bsk_…a1b2`: enough to recognise the key, not to use it.
pub fn mask(key: &str) -> String {
    let tail: String = key.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    format!("{KEY_PREFIX}…{tail}")
}

pub fn load_key(root: &Path) -> Option<String> {
    let bytes = crate::durable_file::read_limited(&root.join(KEY_FILE), 512).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    valid_key(&text).ok().map(str::to_string)
}

pub fn save_key(root: &Path, key: &str) -> io::Result<()> {
    let key = valid_key(key).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let path = root.join(KEY_FILE);
    crate::durable_file::replace(&path, key.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn forget_key(root: &Path) -> io::Result<()> {
    match fs::remove_file(root.join(KEY_FILE)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[derive(Debug, Deserialize)]
pub struct Ping {
    pub booth: String,
    #[serde(rename = "twoFactor")]
    pub two_factor: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Booking {
    pub reference: String,
    pub client: String,
    #[serde(rename = "startDate")]
    pub start_date: String,
    #[serde(rename = "endDate")]
    pub end_date: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Started {
    #[serde(rename = "uploadId")]
    pub upload_id: u64,
    #[serde(rename = "uploadToken")]
    pub upload_token: String,
    #[serde(rename = "chunkSize")]
    pub chunk_size: u64,
    pub chunks: u64,
    pub reservation: Booking,
}

#[derive(Debug, Deserialize)]
struct Status {
    status: String,
    url: Option<String>,
    photos: Option<u64>,
    videos: Option<u64>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    error: Option<String>,
}

/// The ZIP ready to send.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub zip: PathBuf,
    pub size: u64,
    pub sha256: String,
}

/// The gallery the website published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub url: String,
    pub photos: u64,
    pub videos: u64,
}

/// Why a request failed: `code` is the HTTP status (0 when the site was not reached).
#[derive(Debug, Clone)]
pub struct Failure {
    pub code: u16,
    pub message: String,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.message) }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        // A 32 MB piece on slow venue Wi-Fi.
        .timeout(Duration::from_secs(600))
        .user_agent(concat!("BoothSoft/", env!("CARGO_PKG_VERSION")))
        // Never follow a redirect: the upload token must only reach the address set
        // in maintenance (see `answer` for the message shown instead).
        .redirects(0)
        .build()
}

fn answer<T: DeserializeOwned>(result: Result<ureq::Response, ureq::Error>) -> Result<T, Failure> {
    match result {
        Ok(response) if (300..400).contains(&response.status()) => {
            let target = response.header("Location").unwrap_or("?").chars().take(120).collect::<String>();
            Err(Failure { code: response.status(), message: crate::tr!(
                fr: format!("Le site redirige vers {target} : réglez directement cette adresse."),
                en: format!("The website redirects to {target}: set that address instead.")) })
        }
        Ok(response) => response.into_json::<T>().map_err(|error| Failure {
            code: 200,
            message: crate::tr!(fr: format!("Réponse du site illisible : {error}"), en: format!("Unreadable answer from the website: {error}")),
        }),
        Err(ureq::Error::Status(code, response)) => {
            let message = response.into_json::<ErrorBody>().ok().and_then(|body| body.error).unwrap_or_else(|| {
                crate::tr!(fr: format!("Le site a répondu {code}."), en: format!("The website answered {code}."))
            });
            Err(Failure { code, message })
        }
        Err(ureq::Error::Transport(error)) => Err(Failure {
            code: 0,
            message: crate::tr!(fr: format!("Site injoignable : {error}"), en: format!("Website unreachable: {error}")),
        }),
    }
}

/// Checks the address and the key.
pub fn ping(server: &Server) -> Result<Ping, Failure> {
    answer(agent().get(&server.url("/api/booth/ping")).set("Authorization", &server.bearer()).call())
}

/// Zips the event (`storage::export_event_zip`) into `work_dir`, then hashes it.
/// `progress(done, total)` counts files, then megabytes.
pub fn prepare(sessions: &Path, folder: &str, work_dir: &Path, progress: &dyn Fn(Phase, u64, u64)) -> io::Result<Prepared> {
    fs::create_dir_all(work_dir)?;
    // The ZIP is a full copy of the event: check there is room for it and the
    // booth's own safety margin, rather than filling the disk halfway through.
    let need = crate::disk::dir_size(&sessions.join(folder));
    if let Some(free) = crate::disk::free_bytes(work_dir) {
        if free < need.saturating_add(crate::disk::FULL) {
            return Err(io::Error::new(io::ErrorKind::StorageFull, crate::tr!(
                fr: format!("Pas assez de place sur le disque : il faut {} libres, il reste {}.", crate::disk::human(need + crate::disk::FULL), crate::disk::human(free)),
                en: format!("Not enough disk space: {} needed, {} free.", crate::disk::human(need + crate::disk::FULL), crate::disk::human(free)))));
        }
    }
    let zip = work_dir.join(format!("{folder}.zip"));
    crate::storage::export_event_zip(sessions, folder, &zip, &|done, total| progress(Phase::Zip, done as u64, total as u64))?;
    let size = fs::metadata(&zip)?.len();
    // Read straight into one 1 MB buffer (a BufReader would copy twice).
    let mut file = fs::File::open(&zip)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    let mut read = 0u64;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 { break; }
        hasher.update(&buffer[..n]);
        read += n as u64;
        if read % (64 << 20) < n as u64 { progress(Phase::Hash, read >> 20, size >> 20); }
    }
    let sha256 = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Ok(Prepared { zip, size, sha256 })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase { Zip, Hash, Send, Publish }

/// Opens the sending with the owner's two-factor code; tells which booking it goes to.
pub fn start(server: &Server, prepared: &Prepared, code: &str) -> Result<Started, Failure> {
    let file_name = prepared.zip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    answer(agent().post(&server.url("/api/booth/uploads")).set("Authorization", &server.bearer())
        .send_json(serde_json::json!({ "totp": code, "fileName": file_name, "size": prepared.size, "sha256": prepared.sha256 })))
}

/// Byte range of piece `index`.
pub fn piece_range(index: u64, chunk_size: u64, size: u64) -> (u64, u64) {
    let start = index * chunk_size;
    (start, (start + chunk_size).min(size))
}

/// Sends every piece (each retried after a network cut), asks the website to publish,
/// then waits for the gallery link. `cancel` stops between pieces.
pub fn send(server: &Server, prepared: &Prepared, started: &Started, progress: &dyn Fn(Phase, u64, u64), cancel: &AtomicBool) -> Result<Published, Failure> {
    let agent = agent();
    let stopped = || Failure { code: 0, message: crate::tr!(fr: "Envoi arrêté.", en: "Sending stopped.").into() };
    let io_failure = |error: io::Error| Failure { code: 0, message: crate::tr!(fr: format!("Lecture du ZIP impossible : {error}"), en: format!("Cannot read the ZIP: {error}")) };
    if started.chunk_size == 0 || started.chunks != prepared.size.div_ceil(started.chunk_size) {
        return Err(Failure { code: 0, message: crate::tr!(fr: "Découpage annoncé par le site incohérent.", en: "Inconsistent pieces announced by the website.").into() });
    }
    let base = server.url(&format!("/api/booth/uploads/{}", started.upload_id));
    let mut file = fs::File::open(&prepared.zip).map_err(io_failure)?;
    let mut buffer = Vec::new();
    for index in 0..started.chunks {
        progress(Phase::Send, index, started.chunks);
        let (from, to) = piece_range(index, started.chunk_size, prepared.size);
        buffer.resize((to - from) as usize, 0);
        file.seek(SeekFrom::Start(from)).map_err(io_failure)?;
        file.read_exact(&mut buffer).map_err(io_failure)?;
        let mut attempt = 0;
        loop {
            if cancel.load(Ordering::Relaxed) { return Err(stopped()); }
            let result: Result<serde_json::Value, Failure> = answer(agent.put(&format!("{base}/chunks/{index}"))
                .set("Authorization", &server.bearer()).set("X-Upload-Token", &started.upload_token)
                .set("Content-Type", "application/octet-stream").send_bytes(&buffer));
            match result {
                Ok(_) => break,
                // Network cut or busy server: try again a little later.
                Err(failure) if (failure.code == 0 || failure.code == 429 || failure.code >= 500) && attempt + 1 < PIECE_ATTEMPTS => {
                    attempt += 1;
                    crate::log::info(format!("Upload piece {index} failed ({}), retry {attempt}", failure.message));
                    wait(Duration::from_secs(2u64 << attempt.min(4)), cancel);
                }
                Err(failure) => return Err(failure),
            }
        }
    }
    progress(Phase::Send, started.chunks, started.chunks);
    drop(buffer);
    let _: serde_json::Value = answer(agent.post(&format!("{base}/complete")).set("Authorization", &server.bearer())
        .set("X-Upload-Token", &started.upload_token).call())?;
    let since = Instant::now();
    loop {
        progress(Phase::Publish, since.elapsed().as_secs(), 0);
        let status: Result<Status, Failure> = answer(agent.get(&base).set("Authorization", &server.bearer()).set("X-Upload-Token", &started.upload_token).call());
        match status {
            Ok(Status { status, url: Some(url), photos, videos, .. }) if status == "done" => {
                return Ok(Published { url, photos: photos.unwrap_or(0), videos: videos.unwrap_or(0) });
            }
            Ok(Status { status, error, .. }) if status == "failed" => {
                return Err(Failure { code: 422, message: error.unwrap_or_else(|| crate::tr!(fr: "Le site n'a pas pu publier la galerie.", en: "The website could not publish the gallery.").into()) });
            }
            Ok(_) => {}
            Err(failure) if failure.code == 0 || failure.code >= 500 => {}
            Err(failure) => return Err(failure),
        }
        if since.elapsed() > PUBLISH_TIMEOUT {
            return Err(Failure { code: 0, message: crate::tr!(fr: "Le site met trop de temps à publier : regardez la galerie sur le site.", en: "The website takes too long to publish: check the gallery on the website.").into() });
        }
        wait(Duration::from_secs(3), cancel);
        if cancel.load(Ordering::Relaxed) { return Err(stopped()); }
    }
}

fn wait(duration: Duration, cancel: &AtomicBool) {
    let until = Instant::now() + duration;
    while Instant::now() < until && !cancel.load(Ordering::Relaxed) { std::thread::sleep(Duration::from_millis(200)); }
}

/// QR code modules of `text` (true = dark), row by row, for drawing in maintenance.
pub fn qr_modules(text: &str) -> Option<(usize, Vec<bool>)> {
    let code = qrcode::QrCode::with_error_correction_level(text.as_bytes(), qrcode::EcLevel::M).ok()?;
    let width = code.width();
    Some((width, code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_https_except_this_computer() {
        assert_eq!(normalize_address(" https://site.example/ ").unwrap(), "https://site.example");
        assert_eq!(normalize_address("http://127.0.0.1:4399").unwrap(), "http://127.0.0.1:4399");
        assert!(normalize_address("http://site.example").is_err());
        assert!(normalize_address("https://localhost").is_err());
        assert!(normalize_address("https://site.example/?x=1").is_err());
        // A user part would hide the real host behind an accepted prefix.
        assert!(normalize_address("http://localhost:1@other.example").is_err());
        assert!(normalize_address("http://127.0.0.1@other.example").is_err());
        assert!(normalize_address("https://user@site.example").is_err());
        assert!(normalize_address("").is_err());
    }

    #[test]
    fn redirects_are_refused_not_followed() {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let site = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: https://other.example/api\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let server = Server::new(&format!("http://127.0.0.1:{port}"), "bsk_0123456789abcdefghij").unwrap();
        let failure = ping(&server).unwrap_err();
        assert_eq!(failure.code, 302);
        assert!(failure.message.contains("https://other.example/api"), "{}", failure.message);
        site.join().unwrap();
    }

    #[test]
    fn keys_are_checked_and_masked() {
        let key = "bsk_ABCdef123456789_xyz";
        assert_eq!(valid_key(&format!(" {key}\n")).unwrap(), key);
        assert!(valid_key("sk_live_123456789012345").is_err());
        assert!(valid_key("bsk_short").is_err());
        assert!(valid_key("bsk_with space 1234567890").is_err());
        assert_eq!(mask(key), "bsk_…_xyz");
    }

    #[test]
    fn key_file_round_trip() {
        let root = std::env::temp_dir().join(format!("boothsoft-upload-key-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        assert!(load_key(&root).is_none());
        save_key(&root, "bsk_0123456789abcdefghij").unwrap();
        assert_eq!(load_key(&root).as_deref(), Some("bsk_0123456789abcdefghij"));
        assert!(save_key(&root, "nope").is_err());
        forget_key(&root).unwrap();
        forget_key(&root).unwrap();
        assert!(load_key(&root).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn pieces_cover_the_file_exactly() {
        assert_eq!(piece_range(0, 10, 25), (0, 10));
        assert_eq!(piece_range(2, 10, 25), (20, 25));
        assert_eq!(25u64.div_ceil(10), 3);
    }

    #[test]
    fn qr_code_for_a_gallery_link() {
        let (width, modules) = qr_modules("https://site.example/g/TutjYwoFB0gpp9_vRvC_9i_M").unwrap();
        assert_eq!(modules.len(), width * width);
        assert!(width >= 21);
    }

    /// End to end against a running copy of the website:
    /// `BS_UPLOAD_URL=http://127.0.0.1:4399 BS_UPLOAD_KEY=bsk_… BS_UPLOAD_TOTP=123456 BS_UPLOAD_ZIP=/tmp/event.zip cargo test -- --ignored live_site`
    #[test]
    #[ignore]
    fn live_site() {
        let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} missing"));
        let server = Server::new(&var("BS_UPLOAD_URL"), &var("BS_UPLOAD_KEY")).unwrap();
        let ping = ping(&server).unwrap();
        assert!(ping.two_factor, "two-factor not enabled on the website");
        let zip = PathBuf::from(var("BS_UPLOAD_ZIP"));
        let bytes = fs::read(&zip).unwrap();
        let prepared = Prepared { zip, size: bytes.len() as u64, sha256: Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect() };
        let started = start(&server, &prepared, &var("BS_UPLOAD_TOTP")).unwrap();
        println!("booking {} ({}, {})", started.reservation.reference, started.reservation.client, started.reservation.start_date);
        let published = send(&server, &prepared, &started, &|phase, done, total| println!("{phase:?} {done}/{total}"), &AtomicBool::new(false)).unwrap();
        println!("{published:?}");
        assert!(published.url.contains("/g/"));
    }
}
