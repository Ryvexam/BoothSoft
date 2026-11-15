// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
use serde::{Deserialize, Serialize};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub event_name: String,
    /// Selected folder in the installed themes directory; never a filesystem path.
    pub theme_id: String,
    pub frame_path: String,
    pub camera_index: u32,
    pub shots_per_session: u8,
    pub photo_timeout_seconds: u16,
    pub session_limit: u32,
    pub print_enabled: bool,
    pub printer_name: String,
    pub admin_pin_hash: String,
    pub demo_mode: bool,
    pub fullscreen: bool,
    /// Live-view frame rate. 30 by default; lower it on machines without a GPU,
    /// where every displayed frame is rendered by the CPU.
    pub live_fps: u8,
    /// Inactivity before the attract screen ("veille") replaces the live view.
    pub sleep_minutes: u16,
    /// While asleep, wake up to the live view when someone walks up to the booth.
    pub wake_on_motion: bool,
    /// Most copies a guest can order for one photo (still capped by the event quota).
    pub max_copies: u8,
    /// Bumped when a new default must be applied once to existing installations.
    pub defaults_version: u8,
    /// Scheduled events: photos taken during an event's hours are stored in its
    /// own folder and count against its own print quota. Outside every slot the
    /// default event (`event_name`) applies.
    pub events: Vec<EventSlot>,
    /// Burst mode offered on the live view (« Une photo » stays preselected): one countdown,
    /// then `burst_count` quick photos and a picker.
    pub burst_enabled: bool,
    pub burst_count: u8,
    /// Video guest book (`docs/specs/GUEST_BOOK.md`): unlocked in maintenance, off by default.
    pub guestbook_enabled: bool,
    /// Longest message, 10…30 s; guests stop when they want.
    pub guestbook_max_seconds: u8,
    /// Shown to guests: "Un message pour …". Empty = current event's name.
    pub guestbook_recipient: String,
    /// Input device name; empty = system default.
    pub guestbook_mic: String,
    /// Software gain ×1, ×2 or ×4 (Faible · Normale · Forte).
    pub guestbook_gain: u8,
    pub guestbook_playback_volume: u8,
    /// Print orientation: landscape (15×10), portrait (10×15) or the guests' choice.
    pub print_orientation: PrintOrientation,
    /// A frame PNG imported in maintenance replaces the theme's (stored in
    /// `<data>/frames/landscape.png` and `portrait.png`, never a free path).
    pub frame_landscape_custom: bool,
    pub frame_portrait_custom: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrintOrientation {
    #[default]
    Landscape,
    Portrait,
    /// A small switch on the live view; landscape preselected at each session.
    Guest,
}

/// One scheduled event. Times are local; `end` at or before `start` means the
/// event runs past midnight (a wedding from 18:00 to 03:00).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EventSlot {
    pub name: String,
    /// `YYYY-MM-DD`
    pub date: String,
    /// `HH:MM`
    pub start: String,
    /// `HH:MM`
    pub end: String,
}

impl Default for EventSlot {
    fn default() -> Self {
        Self { name: "Nouvel événement".into(), date: chrono::Local::now().date_naive().format("%Y-%m-%d").to_string(),
            start: "14:00".into(), end: "02:00".into() }
    }
}

impl EventSlot {
    pub fn date(&self) -> NaiveDate {
        NaiveDate::parse_from_str(&self.date, "%Y-%m-%d").unwrap_or_else(|_| chrono::Local::now().date_naive())
    }
    pub fn start_time(&self) -> NaiveTime { parse_time(&self.start, 14) }
    pub fn end_time(&self) -> NaiveTime { parse_time(&self.end, 2) }

    /// Local start and end instants (end on the next day when it wraps midnight).
    pub fn window(&self) -> (NaiveDateTime, NaiveDateTime) {
        let start = self.date().and_time(self.start_time());
        let mut end = self.date().and_time(self.end_time());
        if end <= start { end += chrono::Duration::days(1); }
        (start, end)
    }

    pub fn contains(&self, now: NaiveDateTime) -> bool {
        let (start, end) = self.window();
        start <= now && now < end
    }

    /// Folder name: `2026-11-14-mariage-lea-hugo`.
    pub fn folder(&self) -> String {
        format!("{}-{}", self.date().format("%Y-%m-%d"), slug(&self.name))
    }
}

fn parse_time(value: &str, default_hour: u32) -> NaiveTime {
    NaiveTime::parse_from_str(value, "%H:%M")
        .unwrap_or_else(|_| NaiveTime::from_hms_opt(default_hour, 0, 0).expect("valid hour"))
}

/// The event photos are currently attached to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentEvent {
    pub name: String,
    /// Folder under `sessions/` (ASCII letters, digits and dashes only).
    pub folder: String,
    /// End of the scheduled slot; `None` for the default event.
    pub until: Option<NaiveDateTime>,
}

/// Lowercase ASCII slug for folder names: accents dropped, never empty, ≤ 60 chars.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let c = match c {
            'à' | 'â' | 'ä' | 'á' | 'À' | 'Â' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'î' | 'ï' | 'í' | 'Î' | 'Ï' => 'i',
            'ô' | 'ö' | 'ó' | 'Ô' | 'Ö' => 'o',
            'ù' | 'û' | 'ü' | 'ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            other => other,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 60 { break; }
    }
    let out = out.trim_end_matches('-').to_owned();
    if out.is_empty() { "evenement".into() } else { out }
}

/// Current `defaults_version`. 1: printing on by default (system default printer).
/// 2: burst mode offered by default.
pub const DEFAULTS_VERSION: u8 = 3;

impl Default for Settings {
    fn default() -> Self {
        Self {
            event_name: "Votre événement".into(),
            theme_id: "default".into(),
            frame_path: String::new(),
            camera_index: 0,
            shots_per_session: 1,
            photo_timeout_seconds: 30,
            session_limit: 0,
            print_enabled: true,
            printer_name: String::new(),
            admin_pin_hash: String::new(),
            demo_mode: false,
            fullscreen: true,
            live_fps: 30,
            sleep_minutes: 5,
            wake_on_motion: true,
            max_copies: 10,
            defaults_version: DEFAULTS_VERSION,
            events: Vec::new(),
            burst_enabled: true,
            burst_count: 5,
            guestbook_enabled: false,
            guestbook_max_seconds: 30,
            guestbook_recipient: String::new(),
            guestbook_mic: String::new(),
            guestbook_gain: 2,
            guestbook_playback_volume: 80,
            print_orientation: PrintOrientation::Landscape,
            frame_landscape_custom: false,
            frame_portrait_custom: false,
        }
    }
}

impl Settings {
    /// Apply new defaults once to settings saved by an older version.
    /// Returns true when something changed (the caller saves).
    pub fn migrate(&mut self) -> bool {
        if self.defaults_version >= DEFAULTS_VERSION { return false; }
        if self.defaults_version < 1 { self.print_enabled = true; }
        if self.defaults_version < 2 { self.burst_enabled = true; }
        // 3: the photo screen goes back to the live view after 30 s, not 90.
        if self.defaults_version < 3 && self.photo_timeout_seconds == 90 { self.photo_timeout_seconds = 30; }
        self.defaults_version = DEFAULTS_VERSION;
        true
    }

    pub fn normalized(mut self) -> Self {
        self.shots_per_session = self.shots_per_session.clamp(1, 4);
        self.photo_timeout_seconds = self.photo_timeout_seconds.clamp(15, 300);
        self.camera_index = self.camera_index.min(20);
        self.session_limit = self.session_limit.min(100_000);
        self.live_fps = self.live_fps.clamp(10, 30);
        self.sleep_minutes = self.sleep_minutes.clamp(1, 60);
        self.max_copies = self.max_copies.clamp(1, 10);
        self.burst_count = self.burst_count.clamp(3, 10);
        self.guestbook_max_seconds = (self.guestbook_max_seconds.clamp(10, 30) / 5) * 5;
        self.guestbook_recipient = self.guestbook_recipient.trim().chars().take(80).collect();
        self.guestbook_mic = self.guestbook_mic.chars().take(200).collect();
        self.guestbook_gain = match self.guestbook_gain { 0 | 1 => 1, 2 | 3 => 2, _ => 4 };
        self.guestbook_playback_volume = self.guestbook_playback_volume.min(100);
        self.events.truncate(100);
        self.events.sort_by_key(|event| event.window().0);
        for event in &mut self.events {
            event.name = event.name.trim().chars().take(80).collect();
            if event.name.is_empty() { event.name = "Événement".into(); }
            // Canonical forms: anything unparsable becomes a valid default.
            event.date = event.date().format("%Y-%m-%d").to_string();
            event.start = event.start_time().format("%H:%M").to_string();
            event.end = event.end_time().format("%H:%M").to_string();
        }
        self
    }

    /// Scheduled event covering `now`, else the default event.
    pub fn current_event(&self, now: NaiveDateTime) -> CurrentEvent {
        if let Some(slot) = self.events.iter().find(|slot| slot.contains(now)) {
            return CurrentEvent { name: slot.name.clone(), folder: slot.folder(), until: Some(slot.window().1) };
        }
        CurrentEvent { name: self.event_name.clone(), folder: slug(&self.event_name), until: None }
    }

    /// Photos taken in one burst (1 when burst mode is off).
    pub fn burst_size(&self) -> u8 {
        if self.burst_enabled { self.burst_count.clamp(3, 10) } else { 1 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shot {
    pub filename: String,
    pub taken_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    #[serde(default)]
    pub event_name: String,
    /// Event folder under `sessions/`; empty for sessions saved before events
    /// existed (stored directly in `sessions/`).
    #[serde(default)]
    pub folder: String,
    pub id: String,
    pub started_at: String,
    pub shots: Vec<Shot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Attract screen shown after inactivity ("veille"): static artwork, the
    /// camera only watches for someone walking up (4 frames/s, nothing displayed).
    Welcome,
    Ready,
    Countdown,
    Processing,
    Review,
    Admin,
    Error,
    /// Short thank-you after a session, then back to Welcome.
    Thanks,
    /// After a burst: carousel to pick photos and copies per photo.
    Pick,
    /// Guest book: microphone test with a live level meter.
    MicCheck,
    /// Guest book: 3 s countdown, then the video message is recorded.
    Recording,
    /// Guest book: the message plays back once; keep it or record again.
    MessageReview,
}

pub fn data_root() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("BoothSoft")
}

#[cfg(test)]
mod theme_setting_tests {
    use super::Settings;

    #[test]
    fn events_cover_their_hours_even_past_midnight() {
        use chrono::NaiveDate;
        let slot = super::EventSlot { name: "Mariage Léa & Hugo".into(), date: "2026-11-14".into(), start: "18:00".into(), end: "03:00".into() };
        let at = |d: u32, h: u32| NaiveDate::from_ymd_opt(2026, 11, d).unwrap().and_hms_opt(h, 0, 0).unwrap();
        assert!(!slot.contains(at(14, 17)));
        assert!(slot.contains(at(14, 18)));
        assert!(slot.contains(at(15, 2)));
        assert!(!slot.contains(at(15, 3)));
        assert_eq!(slot.folder(), "2026-11-14-mariage-lea-hugo");
        let settings = Settings { events: vec![slot], ..Settings::default() };
        assert_eq!(settings.current_event(at(14, 20)).name, "Mariage Léa & Hugo");
        assert_eq!(settings.current_event(at(16, 20)).name, settings.event_name);
    }

    #[test]
    fn slugs_are_safe_folder_names() {
        assert_eq!(super::slug("../../etc"), "etc");
        assert_eq!(super::slug("   "), "evenement");
        assert!(super::slug(&"é".repeat(200)).len() <= 60);
    }

    #[test]
    fn older_settings_default_to_bundled_theme() {
        let settings: Settings = serde_json::from_str(r#"{"event_name":"Legacy event"}"#)
            .expect("old settings should remain compatible");
        assert_eq!(settings.theme_id, "default");
    }

    #[test]
    fn selected_theme_survives_settings_serialization() {
        let settings = Settings { theme_id: "my-brand".into(), ..Settings::default() };
        let data = serde_json::to_string(&settings).expect("serialize settings");
        let restored: Settings = serde_json::from_str(&data).expect("restore settings");
        assert_eq!(restored.theme_id, "my-brand");
    }
}
