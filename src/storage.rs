// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
use crate::{durable_file, image_io, model::{data_root, Session, Settings, Shot}};
use chrono::Local;
use image::ImageFormat;
use std::{fs, io, path::{Path, PathBuf}, sync::atomic::{AtomicU64, Ordering}};

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct Storage {
    pub root: PathBuf,
    pub settings: Settings,
    pub sessions: Vec<Session>,
}

impl Storage {
    pub fn open() -> io::Result<Self> {
        let root = data_root();
        fs::create_dir_all(root.join("sessions"))?;
        // A damaged settings file must NOT silently reset the administrator PIN.
        let settings = match durable_file::read_limited(&root.join("settings.json"), 256 * 1024) {
            Ok(data) => serde_json::from_slice::<Settings>(&data)
                .map_err(io::Error::other)?.normalized(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Settings::default(),
            Err(error) => return Err(error),
        };
        let mut sessions = Vec::new();
        // sessions/<event-folder>/<session-id>/ ; legacy sessions sit directly in sessions/.
        let mut budget = 20_000usize;
        for entry in fs::read_dir(root.join("sessions"))?.take(10_000).flatten() {
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else { continue; };
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else { continue; };
            if !meta.is_dir() || meta.file_type().is_symlink() { continue; }
            if path.join("session.json").is_file() {
                if let Some(session) = read_session(&path, &name, "") { sessions.push(session); }
            } else if valid_folder(&name) {
                let Ok(inner_dir) = fs::read_dir(&path) else { continue; };
                for inner in inner_dir.take(10_000).flatten() {
                    if budget == 0 { break; }
                    budget -= 1;
                    let Some(id) = inner.file_name().to_str().map(str::to_owned) else { continue; };
                    let Ok(meta) = fs::symlink_metadata(inner.path()) else { continue; };
                    if !meta.is_dir() || meta.file_type().is_symlink() { continue; }
                    if let Some(session) = read_session(&inner.path(), &id, &name) { sessions.push(session); }
                }
            }
        }
        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(Self { root, settings, sessions })
    }

    pub fn save_settings(&self) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(&self.settings).map_err(io::Error::other)?;
        durable_file::replace(&self.root.join("settings.json"), &json)
    }

    pub fn new_session(&mut self) -> io::Result<Session> {
        let now = Local::now();
        let sequence = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        let id = format!("{}-{}-{sequence:x}", now.format("%Y%m%d-%H%M%S-%3f"), std::process::id());
        let event = self.settings.current_event(now.naive_local());
        fs::create_dir_all(self.root.join("sessions").join(&event.folder))?;
        let session = Session {
            event_name: event.name,
            folder: event.folder,
            id,
            started_at: now.to_rfc3339(),
            shots: Vec::new(),
        };
        let directory = self.session_dir(&session);
        // An existing directory means collision or a stale prior session.
        // Never overwrite the corresponding manifest.
        fs::create_dir(&directory)?;
        if let Err(error) = self.persist_session(&session) {
            let _ = fs::remove_dir(&directory);
            return Err(error);
        }
        self.sessions.insert(0, session.clone());
        Ok(session)
    }

    /// Paths for the next capture. The image worker writes the JPEGs; the UI
    /// registers the shot only after both files are successfully persisted.
    pub fn next_shot_targets(&self, session: &Session) -> (PathBuf, PathBuf) {
        let filename = format!("photo-{:02}.jpg", session.shots.len() + 1);
        let original = self.session_dir(session).join(&filename);
        // The print (frame applied) sits next to the original, in the event folder.
        let framed = self.session_dir(session).join(framed_name(&filename));
        (original, framed)
    }

    pub fn register_shot(&mut self, session: &mut Session, path: &Path) -> io::Result<()> {
        let expected = self.next_shot_targets(session).0;
        if path != expected || !path.is_file() {
            return Err(io::Error::other("Processed photo path mismatch"));
        }
        let filename = path.file_name().and_then(|file| file.to_str())
            .ok_or_else(|| io::Error::other("Invalid photo filename"))?.to_owned();
        session.shots.push(Shot { filename, taken_at: Local::now().to_rfc3339() });
        if let Err(error) = self.persist_session(session) {
            session.shots.pop();
            return Err(error);
        }
        if let Some(record) = self.sessions.iter_mut().find(|item| item.id == session.id) {
            *record = session.clone();
        }
        Ok(())
    }

    pub fn remove_last_shot(&mut self, session: &mut Session) -> io::Result<()> {
        let Some(shot) = session.shots.pop() else { return Ok(()); };
        // Persist the new manifest before removing files. If the save fails,
        // both the active state and the original photo remain intact.
        if let Err(error) = self.persist_session(session) {
            session.shots.push(shot);
            return Err(error);
        }
        if let Some(record) = self.sessions.iter_mut().find(|item| item.id == session.id) {
            *record = session.clone();
        }
        // Cleanup is best effort after commit. A leftover orphan is safer than
        // a manifest referencing a photo that was already deleted.
        let _ = fs::remove_file(self.shot_path(session, &shot));
        let composed = self.session_dir(session).join(framed_name(&shot.filename));
        let _ = fs::remove_file(composed);
        Ok(())
    }

    pub fn shot_path(&self, session: &Session, shot: &Shot) -> PathBuf {
        self.session_dir(session).join(&shot.filename)
    }

    pub fn export_shot(&self, session: &Session, shot: &Shot) -> io::Result<PathBuf> {
        let export_dir = self.root.join("print-exports");
        fs::create_dir_all(&export_dir)?;
        let dest = export_dir.join(format!("{}-{}", session.id, shot.filename));
        fs::copy(self.shot_path(session, shot), &dest)?;
        Ok(dest)
    }

    pub fn disk_location(&self) -> &Path {
        &self.root
    }

    pub fn session_dir(&self, session: &Session) -> PathBuf {
        let base = self.root.join("sessions");
        if session.folder.is_empty() { base.join(&session.id) } else { base.join(&session.folder).join(&session.id) }
    }

    /// The framed (printed) version of a stored shot.
    pub fn framed_path(&self, session: &Session, shot: &Shot) -> PathBuf {
        self.session_dir(session).join(framed_name(&shot.filename))
    }

    fn persist_session(&self, session: &Session) -> io::Result<()> {
        let content = serde_json::to_vec_pretty(session).map_err(io::Error::other)?;
        durable_file::replace(&self.session_dir(session).join("session.json"), &content)
    }
}

/// `photo-01.jpg` → `photo-01-cadre.jpg`
fn framed_name(filename: &str) -> String {
    format!("{}-cadre.jpg", filename.trim_end_matches(".jpg"))
}

fn read_session(directory: &Path, id: &str, folder: &str) -> Option<Session> {
    if !valid_session_id(id) { return None; }
    let manifest = directory.join("session.json");
    let metadata = fs::symlink_metadata(&manifest).ok()?;
    if metadata.file_type().is_symlink() || metadata.len() > 64 * 1024 { return None; }
    let mut session = serde_json::from_slice::<Session>(&fs::read(manifest).ok()?).ok()?;
    // The folder comes from where the session was found, never from the manifest.
    session.folder = folder.to_owned();
    (session.id == id && valid_session(&session)).then_some(session)
}

/// Event folder names are generated by `model::slug`: ASCII, dashes, bounded.
fn valid_folder(value: &str) -> bool {
    !value.is_empty() && value.len() <= 80 && !value.starts_with('-')
        && value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn valid_session_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 90
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn valid_session(session: &Session) -> bool {
    valid_session_id(&session.id)
        && (session.folder.is_empty() || valid_folder(&session.folder))
        && session.shots.len() <= 10
        && session.event_name.len() <= 256
        && session.shots.iter().all(|shot| {
            let filename = shot.filename.as_str();
            filename.len() == "photo-01.jpg".len()
                && filename.starts_with("photo-")
                && filename.ends_with(".jpg")
                && filename.as_bytes()[6..8].iter().all(u8::is_ascii_digit)
        })
}

/// Compose a contact sheet by decoding one input at a time. Avoid retaining
/// every full-resolution image in memory during an administrative export.
pub fn export_contact_sheet(paths: &[PathBuf], output: &Path) -> io::Result<()> {
    if paths.is_empty() { return Err(io::Error::other("No photographs to export")); }
    if paths.len() > 4 { return Err(io::Error::other("Maximum four photos per contact sheet")); }
    const WIDTH: u32 = 1200;
    const HEIGHT: u32 = 1800;
    const MARGIN: u32 = 64;
    const GAP: u32 = 22;
    let bg = image::Rgb([249, 245, 236]);
    let mut canvas = image::RgbImage::from_pixel(WIDTH, HEIGHT, bg);
    let slot_height = (HEIGHT - MARGIN * 2 - GAP * (paths.len() as u32 - 1)) / paths.len() as u32;
    for (index, path) in paths.iter().enumerate() {
        let shot = image_io::read_print_jpeg(path)?;
        let scaled = image::imageops::resize(
            &shot,
            WIDTH - MARGIN * 2,
            slot_height,
            image::imageops::FilterType::Lanczos3,
        );
        let x = (WIDTH - scaled.width()) / 2;
        let y = MARGIN + (index as u32) * (slot_height + GAP)
            + (slot_height - scaled.height()) / 2;
        image::imageops::overlay(&mut canvas, &scaled, x.into(), y.into());
    }
    canvas.save_with_format(output, ImageFormat::Jpeg).map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_manifest_rejects_path_traversal() {
        let mut malicious = Session {
            id: "../../escape".into(),
            event_name: "event".into(),
            folder: String::new(),
            started_at: String::new(),
            shots: vec![Shot { filename: "photo-01.jpg".into(), taken_at: String::new() }],
        };
        assert!(!valid_session(&malicious));
        malicious.id = "20261109-130000-123".into();
        malicious.shots[0].filename = "../../settings.json".into();
        assert!(!valid_session(&malicious));
        malicious.shots[0].filename = "photo-01.jpg".into();
        assert!(valid_session(&malicious));
    }

    #[test]
    fn contact_sheet_decodes_stored_photos_one_at_a_time() {
        let root = std::env::temp_dir().join(format!(
            "boothsoft-sheet-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .expect("clock").as_nanos(),
        ));
        fs::create_dir_all(&root).expect("create fixture");
        let photos: Vec<PathBuf> = (0..2).map(|index| {
            let path = root.join(format!("photo-{index}.jpg"));
            image::RgbImage::from_pixel(100, 80, image::Rgb([30, 90, 130]))
                .save_with_format(&path, ImageFormat::Jpeg).expect("test photo");
            path
        }).collect();
        let output = root.join("sheet.jpg");
        export_contact_sheet(&photos, &output).expect("export sheet");
        assert_eq!(image::open(output).expect("read sheet").width(), 1200);
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn empty_contact_sheet_is_rejected() {
        let error = export_contact_sheet(&[], Path::new("never-created.jpg"));
        assert!(error.is_err());
    }
    #[test]
    fn new_sessions_do_not_overwrite_existing_manifests() {
        let root = std::env::temp_dir().join(format!("boothsoft-new-session-{}-{}",
            std::process::id(), std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(root.join("sessions")).unwrap();
        let mut storage = Storage { root: root.clone(), settings: Settings::default(), sessions: Vec::new() };
        let first = storage.new_session().unwrap();
        let second = storage.new_session().unwrap();
        assert_ne!(first.id, second.id);
        assert!(storage.session_dir(&first).join("session.json").is_file());
        let folder = crate::model::slug(&storage.settings.event_name);
        assert!(storage.session_dir(&first).starts_with(root.join("sessions").join(folder)));
        assert!(storage.session_dir(&second).join("session.json").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_retake_persistence_keeps_original_shot() {
        let root = std::env::temp_dir().join(format!("boothsoft-retake-{}-{}",
            std::process::id(), std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(root.join("sessions")).unwrap();
        let mut storage = Storage { root: root.clone(), settings: Settings::default(), sessions: Vec::new() };
        let mut session = storage.new_session().unwrap();
        let shot_path = storage.next_shot_targets(&session).0;
        fs::write(&shot_path, b"photo").unwrap();
        storage.register_shot(&mut session, &shot_path).unwrap();
        // Force an atomic manifest rename failure on every supported OS.
        let manifest = storage.session_dir(&session).join("session.json");
        fs::remove_file(&manifest).unwrap();
        fs::create_dir(&manifest).unwrap();
        assert!(storage.remove_last_shot(&mut session).is_err());
        assert_eq!(session.shots.len(), 1);
        assert!(shot_path.is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn settings_clamp_values() {
        let settings = Settings { shots_per_session: 99, ..Settings::default() }.normalized();
        assert_eq!(settings.shots_per_session, 4);
    }
}
