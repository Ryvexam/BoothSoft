// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Native kiosk UI and its state machine. The CLI entrypoint lives in `main.rs`.
use boothsoft::{
    camera::{self, CameraCommand, CameraEvent, CameraWorker},
    compositor, devices, effects::{self, PhotoFilter}, model::{CurrentEvent, Screen, Session},
    photo_worker::{PhotoCommand, PhotoEvent, PhotoWorker},
    print_jobs, print_quota::PrintLedger, printer::{self, PrintEvent, PrintRequest, PrinterWorker},
    storage::{self, Storage}, theme, theme_config, theme_import,
};
use eframe::egui::{self, Align2, Color32, ColorImage, CornerRadius, RichText, TextureHandle, TextureOptions, Pos2, Rect, Stroke, Vec2};
use image::RgbImage;
use std::{path::PathBuf, sync::Arc, time::{Duration, Instant}};

mod admin;
mod burst;
mod guestbook;
mod live_gl;
mod ui;

use ui::{Icon, Kind, Palette};
use boothsoft::tr;

const COUNTDOWN_SECONDS: u64 = 7;
/// Worker wake-up: one repaint (see `Kiosk::new`).
const WAKE: Duration = Duration::from_millis(1);
/// Thank-you screen duration before returning to the attract screen.
const THANKS_SECONDS: u64 = 5;
/// White flash after the shutter fires. Drawn in a few discrete steps, not at 60 FPS.
const FLASH_MS: u64 = 360;
const FLASH_STEP_MS: u64 = 60;
/// Five taps in this corner within this window open maintenance (touch-only kiosks).
const ADMIN_TAPS: u8 = 5;
const ADMIN_TAP_WINDOW: Duration = Duration::from_secs(4);
/// Pause between two burst photos: time to change pose, and for the photo
/// worker to encode the previous one (so its queue never fills).
const BURST_GAP: Duration = Duration::from_millis(900);
/// Longest edge of burst thumbnails (10 × ~0.6 MB of texture at most).
const BURST_THUMB_EDGE: u32 = 420;

/// One burst photo waiting in the picker.
struct BurstShot {
    framed: PathBuf,
    thumb: TextureHandle,
    copies: u8,
}

/// Burst in progress: captures requested, photos developed, picker state.
struct Burst {
    total: u8,
    requested: u8,
    failed: u8,
    shots: Vec<BurstShot>,
    next_at: Option<Instant>,
    selected: usize,
    /// Full-size texture of the selected photo, decoded on selection only.
    big: Option<(usize, TextureHandle)>,
    swipe_from: Option<f32>,
    /// Horizontal finger offset while the big photo is being dragged.
    drag_dx: f32,
    /// Side the newly selected photo slides in from (+1 right, −1 left).
    slide_dir: f32,
    confirm: bool,
}

impl Burst {
    fn new(total: u8) -> Self {
        Self { total, requested: 0, failed: 0, shots: Vec::new(), next_at: None, selected: 0, big: None, swipe_from: None, drag_dx: 0.0, slide_dir: 0.0, confirm: false }
    }
    fn developed(&self) -> bool { self.shots.len() + usize::from(self.failed) >= usize::from(self.total) }
    fn copies(&self) -> u32 { self.shots.iter().map(|s| u32::from(s.copies)).sum() }
}
/// The review screen shows its "back to live" countdown (one repaint a
/// second) during this final window; with the default 30 s it is always on.
const REVIEW_HINT: Duration = Duration::from_secs(30);
const REVIEW_TITLES: [(&str, &str); 5] = [("Magnifique !", "Gorgeous!"), ("Superbe !", "Superb!"), ("Quelle allure !", "Looking sharp!"), ("Parfait !", "Perfect!"), ("Sublime !", "Stunning!")];

/// What the shutter does on the live view.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GuestMode { Photo, Burst, GuestBook }

/// How long the licence attribution stays on screen after launch.
const ATTRIBUTION_SPLASH: Duration = Duration::from_secs(5);

/// "BoothSoft, créé par Dôm'Booth — dombooth.com" at the bottom of the screen,
/// above everything: a pill at launch, a discreet line on the sleep screen.
fn paint_attribution(ctx: &egui::Context, splash: bool) {
    let rect = ctx.screen_rect();
    let scale = (rect.height() / 800.0).clamp(0.8, 1.6);
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("attribution")));
    let galley = painter.layout_no_wrap(boothsoft::ATTRIBUTION.to_owned(), ui::bold(15.0 * scale), Color32::WHITE);
    let center = Pos2::new(rect.center().x, rect.bottom() - 22.0 * scale);
    if splash {
        // Under the mode switch, clear of the shutter and its label.
        let pill = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 162.0 * scale), galley.size() + Vec2::new(32.0, 14.0) * scale);
        painter.rect_filled(pill, CornerRadius::same(255), Color32::from_black_alpha(160));
        painter.galley(pill.center() - galley.size() / 2.0, galley, Color32::WHITE);
    } else {
        ui::shadowed_text(&painter, center, Align2::CENTER_CENTER, boothsoft::ATTRIBUTION, ui::bold(13.0 * scale), Color32::from_white_alpha(170));
    }
}

/// Marker for "portrait print without a frame" in `frame_sources`.
const PLAIN_PORTRAIT: &str = "<plain-portrait>";

/// Theme artwork decoded once per theme and sized to the display, never per frame.
struct GuestArt {
    theme_id: String,
    background: Option<TextureHandle>,
    logo: Option<TextureHandle>,
}

/// Event being zipped: folder, files done, total, result (count and path, or error).
type EventExport = (String, Arc<std::sync::atomic::AtomicUsize>, usize, std::sync::mpsc::Receiver<Result<(usize, std::path::PathBuf), String>>);

pub struct Kiosk {
    storage: Storage,
    themes_root: PathBuf,
    installed_themes: Vec<theme_config::LoadedTheme>,
    theme_choice: String,
    /// Logo thumbnails of the installed themes, loaded when the Thème page opens.
    theme_thumbs: Option<Vec<(String, TextureHandle)>>,
    admin_section: admin::AdminSection,
    /// Action waiting for the « Quitter sans enregistrer ? » answer.
    pending_leave: Option<admin::PendingLeave>,
    /// Event of the schedule waiting for a delete confirmation.
    pending_event_delete: Option<usize>,
    /// Settings as last saved, to detect unsaved maintenance edits.
    admin_saved: Option<boothsoft::model::Settings>,
    themes_loaded: bool,
    theme_status: String,
    theme_import_rx: Option<std::sync::mpsc::Receiver<std::io::Result<String>>>,
    print_ledger: PrintLedger,
    quota_draft_event: String,
    quota_draft: u32,
    print_jobs: print_jobs::PrintJobs,
    camera: CameraWorker,
    processor: PhotoWorker,
    printer: PrinterWorker,
    device_scan: Option<std::sync::mpsc::Receiver<devices::DeviceInventory>>,
    cameras: Vec<camera::CameraDevice>,
    printers: Vec<String>,
    devices_scanned: bool,
    device_status: String,
    screen: Screen,
    status: String,
    session: Option<Session>,
    review: Option<TextureHandle>,
    review_backdrop: Option<TextureHandle>,
    /// The printed photo, shown on the thank-you screen.
    thanks_photo: Option<TextureHandle>,
    admin_logo: Option<TextureHandle>,
    shot_started: Option<Instant>,
    last_touch: Instant,
    pin_entry: String,
    new_pin: String,
    admin_authorized: bool,
    last_export: Option<PathBuf>,
    capture_pending: Option<Instant>,
    active_filter: PhotoFilter,
    preview_filtered: Option<TextureHandle>,
    /// Print frames with their photo window: [landscape, portrait]. Only the
    /// orientation(s) in use are kept in memory (≈ 8.6 MB each).
    frames: [Option<compositor::FrameRef>; 2],
    frame_path_loaded: String,
    /// Guest's choice when the orientation is left to guests (reset each session).
    guest_portrait: bool,
    failed_pin_attempts: u8,
    pin_blocked_until: Option<Instant>,
    art: Option<GuestArt>,
    flash_at: Option<Instant>,
    thanks_at: Option<Instant>,
    /// Copies sent to the printer for the thank-you screen (0 = no print).
    thanks_copies: u16,
    /// Copies picker open over the review, with the current choice.
    copies_picker: Option<u8>,
    /// Second step of the picker for more than `CONFIRM_ABOVE` copies.
    copies_confirm: bool,
    print_seq: u32,
    /// System printer used when none was chosen in maintenance (cached lookup).
    pub(crate) system_printer: Option<String>,
    burst: Option<Burst>,
    /// Heartbeat for boothsoft-guard (None when not guarded).
    pulse: Option<boothsoft::watchdog::UiPulse>,
    started_at: Instant,
    /// Last status and screen written to the log (changes are logged once).
    logged_status: String,
    logged_screen: Option<Screen>,
    /// When the error screen appeared (it retries by itself).
    error_since: Option<Instant>,
    /// The camera thread reported the camera gone and is reconnecting by itself.
    camera_lost: bool,
    /// Last time the error screen asked the camera thread to start over.
    camera_kick: Option<Instant>,
    /// Free space on the data disk at the last check (start-up, every session).
    disk_free: Option<u64>,
    /// Maintenance: stored events and their size (computed when Data opens), event to delete.
    event_sizes: Option<Vec<(String, u64)>>,
    pending_disk_delete: Option<String>,
    event_export: Option<EventExport>,
    /// Guest's choice on the live view: single photo (default), burst or guest book.
    guest_mode: GuestMode,
    /// Guest-book screens' state (microphone, recording, playback).
    guestbook: Option<guestbook::GuestBook>,
    /// Thank-you screen after a guest-book message: its recipient.
    thanks_message: Option<String>,
    /// Guest book in maintenance: microphone test, messages, export.
    gb_admin: admin::GuestBookAdmin,
    upload: admin::UploadAdmin,
    /// Start-with-the-computer state, read when the maintenance page opens.
    autostart: Option<bool>,
    admin_taps: (u8, Instant),
    notice: Option<(String, Instant)>,
    review_title: usize,
    /// GPU live view; `None` means the egui-texture fallback is used.
    live: Option<live_gl::LiveView>,
    live_checked: bool,
    numeral: ui::BigNumeral,
    /// OpenGL renderer string, e.g. "Apple M2" or "llvmpipe (LLVM 17, 256 bits)".
    renderer: String,
}

impl Kiosk {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        mut storage: Storage,
        themes_root: PathBuf,
        startup_warning: Option<String>,
    ) -> Self {
        let wake = cc.egui_ctx.clone();
        let pulse = boothsoft::watchdog::start(move || wake.request_repaint());
        theme::install(&cc.egui_ctx);
        // Messages cut by a crash or a power cut become playable files again.
        boothsoft::guestbook::repair_all(&storage.root.join("sessions"));
        if storage.settings.migrate() {
            let _ = storage.save_settings();
        }
        if let Some(selected) = theme::active() {
            // A legacy frame_path in settings.json is not a trust boundary.
            // Only use the validated frame belonging to the active theme.
            // Event assets will require a separate verified cache in the future.
            storage.settings.frame_path = selected.frame.to_string_lossy().into_owned();
        }
        let print_ledger = PrintLedger::open(&storage.root.join("print-ledger.json"))
            .unwrap_or_else(|error| panic!("Cannot read print ledger safely: {error}"));
        let print_jobs = print_jobs::PrintJobs::open(&storage.root.join("print-jobs.json"))
            .unwrap_or_else(|error| panic!("Cannot load print jobs safely: {error}"));
        // The native worker wakes egui only when a fresh frame or status arrives.
        let wake_context = cc.egui_ctx.clone();
        // egui turns a zero-delay `request_repaint()` into *two* frames (a settling
        // pass). For a 30 FPS feed that doubles the rendering cost, which without a
        // GPU is the dominant cost. A non-zero delay schedules exactly one frame.
        let camera = CameraWorker::new(Arc::new(move || wake_context.request_repaint_after(WAKE)));
        let wake_processor = cc.egui_ctx.clone();
        let processor = PhotoWorker::start(Arc::new(move || wake_processor.request_repaint_after(WAKE)));
        let wake_printer = cc.egui_ctx.clone();
        let printer = PrinterWorker::start(Arc::new(move || wake_printer.request_repaint_after(WAKE)));
        let status = startup_warning.unwrap_or_else(|| format!("Data: {}", storage.root.display()));
        let theme_choice = theme::active().map_or_else(|| "default".to_owned(), |theme| theme.id.clone());
        ui::install_fonts(&cc.egui_ctx);
        if !storage.settings.demo_mode {
            camera.send(CameraCommand::Start { index: storage.settings.camera_index });
            camera.send(CameraCommand::SetFrameRate(storage.settings.live_fps));
        }
        Self {
            storage, themes_root, installed_themes: Vec::new(), theme_choice,
            theme_thumbs: None, admin_section: admin::AdminSection::default(), pending_leave: None, pending_event_delete: None, admin_saved: None,
            themes_loaded: false, theme_status: String::new(), theme_import_rx: None,
            print_ledger, quota_draft_event: String::new(), quota_draft: 0,
            print_jobs, camera, processor, printer,
            device_scan: None, cameras: Vec::new(), printers: Vec::new(),
            devices_scanned: false, device_status: String::new(),
            screen: Screen::Ready, status, session: None,
            review: None, review_backdrop: None, thanks_photo: None, admin_logo: None,
            shot_started: None, last_touch: Instant::now(), pin_entry: String::new(),
            new_pin: String::new(), admin_authorized: false,
            last_export: None, capture_pending: None,
            failed_pin_attempts: 0, pin_blocked_until: None,
            active_filter: PhotoFilter::Original, preview_filtered: None,
            frames: [None, None], frame_path_loaded: String::new(), guest_portrait: false,
            art: None, flash_at: None, thanks_at: None, thanks_copies: 0, copies_picker: None, copies_confirm: false, print_seq: 0, system_printer: None, burst: None, pulse, started_at: Instant::now(), logged_status: String::new(), logged_screen: None, error_since: None, camera_lost: false, camera_kick: None, disk_free: None, event_sizes: None, pending_disk_delete: None, event_export: None, guest_mode: GuestMode::Photo, guestbook: None, thanks_message: None, gb_admin: admin::GuestBookAdmin::default(), upload: admin::UploadAdmin::default(), autostart: None,
            admin_taps: (0, Instant::now()), notice: None, review_title: 0,
            live: None, live_checked: false, numeral: ui::BigNumeral::default(), renderer: String::new(),
        }
    }

    fn process_camera(&mut self, ctx: &egui::Context) {
        // Full-resolution captures and statuses are independent of preview traffic.
        while let Ok(event) = self.camera.rx.try_recv() {
            match event {
                CameraEvent::Captured(image) => self.accept_capture(ctx, image),
                CameraEvent::Motion => match self.screen {
                    Screen::Welcome if self.storage.settings.wake_on_motion => self.enter_ready(),
                    // Guests moving in front of the live view count as activity.
                    Screen::Ready => self.last_touch = Instant::now(),
                    _ => {}
                },
                CameraEvent::Status(status) => self.status = status,
                CameraEvent::Error(error) => {
                    self.status = format!("Camera error: {error}");
                    if self.capture_pending.is_some() { self.capture_pending = None; self.screen = Screen::Error; }
                },
                CameraEvent::Lost(error) => {
                    self.status = format!("Camera lost: {error}");
                    self.camera_lost = true;
                    // Guests see the pause screen instead of a frozen picture; any
                    // countdown or video message in progress is abandoned (photos taken stay).
                    if matches!(self.screen, Screen::Ready | Screen::Countdown | Screen::MicCheck | Screen::Recording) {
                        if self.guestbook.take().is_some() { self.camera.send(CameraCommand::Record(None)); }
                        self.capture_pending = None;
                        self.screen = Screen::Error;
                        self.error_since = Some(Instant::now());
                    }
                }
                CameraEvent::Connected => {
                    self.camera_lost = false;
                    self.camera_kick = None;
                    if self.screen == Screen::Error { self.error_since = None; self.finish_session(); }
                }
            }
        }
        if let Some(preview) = self.camera.take_preview() {
            let shown = !self.storage.settings.demo_mode && matches!(self.screen, Screen::Ready | Screen::Countdown | Screen::MicCheck | Screen::Recording);
            match self.live.as_mut() {
                // GPU path: the full frame is uploaded as-is; filter, crop and mirror happen in the shader.
                Some(live) if shown => live.submit(preview),
                None if shown => {
                    // Texture fallback only: repack to RGB for the CPU filter.
                    let rgb = image::DynamicImage::ImageRgba8(preview.clone()).into_rgb8();
                    let mut small = compositor::fit_within(&rgb, camera::PREVIEW_WIDTH);
                    let image = small.get_or_insert(rgb);
                    effects::apply_in_place(image, self.active_filter);
                    update_texture(ctx, &mut self.preview_filtered, "live-filtered", image);
                    let _ = self.camera.recycler().try_send(preview);
                }
                _ => { let _ = self.camera.recycler().try_send(preview); }
            }
        }
    }

    /// Free space check (one system call). Logged when the level changes.
    fn check_disk(&mut self) -> boothsoft::disk::Space {
        use boothsoft::disk::{free_bytes, human, Space};
        let before = self.disk_free.map(Space::of);
        self.disk_free = free_bytes(&self.storage.root);
        let Some(free) = self.disk_free else { return Space::Ok };
        let space = Space::of(free);
        if before != Some(space) && space != Space::Ok {
            boothsoft::log::info(format!("Disk space {space:?}: {} free", human(free)));
        }
        space
    }

    fn start_session(&mut self) {
        // Never start writing photos on a nearly full disk (ROADMAP B4).
        if self.check_disk() == boothsoft::disk::Space::Full {
            self.status = "Disk full".into();
            self.notice = Some((tr!(fr: "La borne fait une pause : sa mémoire est pleine.", en: "The booth is taking a break: its storage is full.").into(), Instant::now()));
            return;
        }

        let event = self.current_event();
        let event_sessions = self.storage.sessions.iter().filter(|s|
            s.folder == event.folder && !s.shots.is_empty()).count();
        if self.storage.settings.session_limit > 0
            && event_sessions as u32 >= self.storage.settings.session_limit {
            self.status = "Session limit reached".into();
            self.notice = Some((tr!(fr: "La borne fait une pause : le nombre de photos prévu est atteint.", en: "The booth is taking a break: the planned number of photos has been reached.").into(), Instant::now()));
            return;
        }
        match self.storage.new_session() {
            Ok(session) => {
                boothsoft::log::info(format!("Session {} started (event: {}, mode: {})", session.id, session.event_name,
                    if self.burst_choice() > 1 { "burst" } else { "single" }));
                self.session = Some(session);
                self.review_title = (self.review_title + 1) % REVIEW_TITLES.len();
                // Do not reopen the USB device on every new guest session.
                if !self.storage.settings.demo_mode {
                    self.camera.send(CameraCommand::SetPreviewEnabled(true));
                }
                self.screen = Screen::Ready;
                self.review = None;
                self.capture_pending = None;
                self.last_touch = Instant::now();
            }
            Err(error) => self.status = format!("Storage error: {error}"),
        }
    }

    fn begin_countdown(&mut self) {
        if self.session.is_none() { self.start_session(); }
        if self.session.is_none() { return; }
        self.shot_started = Some(Instant::now());
        let burst = self.burst_choice();
        self.burst = (burst > 1).then(|| Burst::new(burst));
        self.screen = Screen::Countdown;
    }

    fn accept_capture(&mut self, _ctx: &egui::Context, image: RgbImage) {
        if self.screen != Screen::Countdown || self.capture_pending.is_none() { return; }
        self.capture_pending = None;
        self.flash_at = Some(Instant::now());
        self.shot_started = None;
        let Some(session) = self.session.as_ref() else { return; };
        let (original, framed) = self.storage.next_shot_targets(session);
        let request = PhotoCommand {
            session_id: session.id.clone(), image, filter: self.active_filter,
            frame: self.active_frame().cloned(), original, framed,
        };
        let submitted = self.processor.submit(request);
        if let Some(burst) = self.burst.as_mut() {
            match submitted {
                Ok(()) if burst.requested < burst.total => burst.next_at = Some(Instant::now() + BURST_GAP),
                Ok(()) => {
                    burst.next_at = None;
                    self.camera.send(CameraCommand::SetPreviewEnabled(false));
                    if !burst.developed() { self.screen = Screen::Processing; }
                }
                // Encoder still busy: shoot this photo again a moment later.
                Err(_) => {
                    burst.requested -= 1;
                    burst.next_at = Some(Instant::now() + Duration::from_millis(300));
                }
            }
            return;
        }
        match submitted {
            Ok(()) => {
                self.camera.send(CameraCommand::SetPreviewEnabled(false));
                self.screen = Screen::Processing;
            }
            Err(error) => { self.status = error.to_owned(); self.screen = Screen::Error; }
        }
    }

    fn process_photo_results(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.processor.results.try_recv() {
            match event {
                PhotoEvent::Saved(result) if self.burst.is_some() => {
                    let Some(session) = self.session.as_mut().filter(|s| s.id == result.session_id) else { continue; };
                    let registered = self.storage.register_shot(session, &result.original);
                    let burst = self.burst.as_mut().expect("checked");
                    match registered {
                        Ok(()) => {
                            if self.review_backdrop.is_none() {
                                self.review_backdrop = Some(texture(ctx, "photo-backdrop", &result.backdrop));
                            }
                            let small = compositor::fit_within(&result.preview, BURST_THUMB_EDGE);
                            let index = burst.shots.len();
                            let thumb = texture(ctx, &format!("burst-{index}"), small.as_ref().unwrap_or(&result.preview));
                            burst.shots.push(BurstShot { framed: result.framed, thumb, copies: 0 });
                        }
                        Err(error) => { self.status = format!("Cannot register photo: {error}"); burst.failed += 1; }
                    }
                    self.burst_progress();
                }
                PhotoEvent::Failed(error) if self.burst.is_some() => {
                    self.status = format!("Photo processing failed: {error}");
                    if let Some(burst) = self.burst.as_mut() { burst.failed += 1; }
                    self.burst_progress();
                }
                PhotoEvent::Saved(result) => {
                    let Some(session) = self.session.as_mut().filter(|s| s.id == result.session_id) else { continue; };
                    match self.storage.register_shot(session, &result.original) {
                        Ok(()) => {
                            self.review = Some(texture(ctx, "photo-review", &result.preview));
                            self.review_backdrop = Some(texture(ctx, "photo-backdrop", &result.backdrop));
                            // The still preview replaces the live feed. Releasing the
                            // camera texture here reduces retained GPU/CPU memory.
                            self.preview_filtered = None;
                            self.last_export = Some(result.framed);
                            self.status = "Photo ready".into();
                            if self.screen != Screen::Admin {
                                self.screen = Screen::Review;
                            }
                            self.last_touch = Instant::now();
                        }
                        Err(error) => {
                            self.status = format!("Cannot register photo: {error}");
                            self.screen = Screen::Error;
                        }
                    }
                }
                PhotoEvent::Failed(error) => {
                    self.status = format!("Photo processing failed: {error}");
                    self.screen = Screen::Error;
                }
            }
        }
    }

    /// Every burst photo developed: open the picker (or fail if none survived).
    fn burst_progress(&mut self) {
        let can_print = self.storage.settings.print_enabled && self.copies_available() > 0;
        let Some(burst) = self.burst.as_mut() else { return; };
        if !burst.developed() || burst.requested < burst.total { return; }
        if burst.shots.is_empty() { self.screen = Screen::Error; return; }
        // One copy of the first photo preselected: printing is one tap away.
        if can_print { burst.shots[0].copies = 1; }
        burst.selected = 0;
        if self.screen != Screen::Admin { self.screen = Screen::Pick; }
        self.last_touch = Instant::now();
    }

    fn current_event(&self) -> CurrentEvent {
        self.storage.settings.current_event(chrono::Local::now().naive_local())
    }

    /// Event the next print is charged to: the guest's session, else the current one.
    fn print_event(&self) -> String {
        self.session.as_ref().map_or_else(|| self.current_event().name, |s| s.event_name.clone())
    }

    fn process_printer_results(&mut self) {
        while let Ok(event) = self.printer.events.try_recv() {
            match event {
                PrintEvent::Submitted { job_id, spooler_id } => {
                    match self.print_jobs.mark_submitted(&job_id, spooler_id,
                        &self.storage.root.join("print-jobs.json")) {
                        Ok(()) => self.status = "Job accepted by printer spooler; awaiting confirmation".into(),
                        Err(error) => self.status = format!("Print job persistence failed: {error}"),
                    }
                }
                PrintEvent::Failed { job_id, reason } => {
                    // A driver may fail after accepting paper. Never release quota
                    // automatically on ambiguous outcomes.
                    self.status = format!("Print job {job_id} requires admin review: {reason}");
                }
            }
        }
    }

    fn submit_print(&mut self, path: PathBuf) -> Result<(), String> {
        if !self.storage.settings.print_enabled {
            return Err("Printing disabled by administrator".into());
        }
        let Some(printer_name) = self.resolve_printer() else {
            return Err(tr!(fr: "Aucune imprimante détectée : branchez-la puis réessayez", en: "No printer found: plug it in and try again").into());
        };
        let reservation_id = self.print_event();
        self.print_seq = self.print_seq.wrapping_add(1);
        let job_id = format!("{}-{}-{}", self.session.as_ref().map_or("unknown", |s| s.id.as_str()),
            chrono::Utc::now().timestamp_micros(), self.print_seq);
        let ledger_path = self.storage.root.join("print-jobs.json");
        self.print_jobs.schedule(&self.print_ledger, &reservation_id, &job_id,
            &path, &ledger_path).map_err(|error| format!("Print quota denied: {error:?}"))?;
        if let Err(error) = self.printer.queue(PrintRequest {
            job_id: job_id.clone(),
            printer: printer_name,
            jpeg: path,
        }) {
            // Nothing has been sent to the printer; safely release reservation.
            let _ = self.print_jobs.cancel_verified_not_printed(&job_id, &ledger_path);
            return Err(error.to_owned());
        }
        self.status = "Print job queued; awaiting printer feedback".into();
        Ok(())
    }

    /// The chosen printer, else the system default, else the only installed one.
    /// Resolved once and cached: no subprocess per copy.
    fn resolve_printer(&mut self) -> Option<String> {
        let chosen = self.storage.settings.printer_name.trim();
        if !chosen.is_empty() {
            return printer::valid_printer_name(chosen).then(|| chosen.to_owned());
        }
        if self.system_printer.is_none() {
            self.system_printer = printer::default_printer().or_else(|| {
                let installed = printer::list_printers().ok()?;
                (installed.len() == 1).then(|| installed[0].clone())
            });
        }
        self.system_printer.clone()
    }

    fn take_demo_photo(&mut self, ctx: &egui::Context) {
        self.accept_capture(ctx, camera::demo_frame());
    }

    /// Forget the current guest session and every per-session texture.
    fn reset_session(&mut self) {
        self.copies_picker = None;
        self.copies_confirm = false;
        self.burst = None;
        self.guest_mode = GuestMode::Photo;
        self.guest_portrait = false;
        if self.guestbook.take().is_some_and(|g| g.is_recording()) { self.camera.send(CameraCommand::Record(None)); }
        self.thanks_message = None;
        self.session = None;
        self.capture_pending = None;
        self.review = None;
        self.shot_started = None;
        self.thanks_at = None;
        self.preview_filtered = None;
        self.review_backdrop = None;
        self.thanks_photo = None;
        if let Some(live) = self.live.as_mut() { live.clear(); }
        self.numeral.clear();
        self.last_touch = Instant::now();
    }

    /// The booth's normal state: live view, ready for the next guest.
    fn enter_ready(&mut self) {
        self.reset_session();
        self.screen = Screen::Ready;
        if !self.storage.settings.demo_mode {
            self.camera.send(CameraCommand::SetSleep(false));
            self.camera.send(CameraCommand::SetPreviewEnabled(true));
        }
    }

    /// Sleep ("veille") after inactivity: attract screen, no live view. The
    /// camera keeps watching at 4 frames/s to wake up when someone comes.
    fn enter_welcome(&mut self) {
        self.reset_session();
        self.screen = Screen::Welcome;
        if !self.storage.settings.demo_mode {
            self.camera.send(CameraCommand::SetPreviewEnabled(false));
            self.camera.send(CameraCommand::SetSleep(self.storage.settings.wake_on_motion));
        }
    }

    /// End of a guest session: straight back to the live view for the next one.
    fn finish_session(&mut self) { self.enter_ready(); }

    fn show_thanks(&mut self, copies: u16) {
        let photo = self.review.take();
        self.reset_session();
        self.thanks_photo = photo;
        self.screen = Screen::Thanks;
        self.thanks_copies = copies;
        self.thanks_at = Some(Instant::now());
        // No live view behind the thank-you screen: stop decoding until it ends.
        if !self.storage.settings.demo_mode { self.camera.send(CameraCommand::SetPreviewEnabled(false)); }
    }

    /// Hidden maintenance access for touch-only booths: five quick taps.
    fn admin_tap(&mut self) {
        let (count, since) = self.admin_taps;
        let count = if since.elapsed() > ADMIN_TAP_WINDOW { 1 } else { count + 1 };
        self.admin_taps = (count, if count == 1 { Instant::now() } else { since });
        if count >= ADMIN_TAPS { self.admin_taps = (0, Instant::now()); self.open_admin(); }
    }

    /// Short guest-facing message (e.g. session limit), on any guest screen.
    fn paint_notice(&mut self, ui: &egui::Ui) {
        let Some((message, at)) = &self.notice else { return; };
        if at.elapsed() >= Duration::from_secs(6) { self.notice = None; return; }
        let rect = ui.max_rect();
        let scale = (rect.height() / 800.0).clamp(0.8, 1.6);
        let painter = ui.painter();
        let galley = painter.layout(message.clone(), ui::bold(18.0 * scale), Color32::WHITE, rect.width() * 0.6);
        let toast = Align2::CENTER_TOP.anchor_size(rect.center_top() + Vec2::new(0.0, 110.0 * scale), galley.size() + Vec2::new(48.0, 26.0) * scale);
        painter.rect_filled(toast, CornerRadius::same(255), Color32::from_black_alpha(200));
        painter.galley(toast.center() - galley.size() / 2.0, galley, Color32::WHITE);
        ui.ctx().request_repaint_after(Duration::from_secs(6).saturating_sub(at.elapsed()));
    }

    /// Decode the theme's welcome artwork and logo once, scaled to the screen.
    fn ensure_art(&mut self, ctx: &egui::Context) {
        let Some(active) = theme::active() else { return; };
        if self.art.as_ref().is_some_and(|art| art.theme_id == active.id) { return; }
        let screen = ctx.screen_rect().size() * ctx.pixels_per_point();
        let (max_w, max_h) = (screen.x.clamp(640.0, 1920.0) as u32, screen.y.clamp(400.0, 1200.0) as u32);
        let background = active.background.as_deref()
            .and_then(|path| load_texture(ctx, "guest-background", path, max_w, max_h));
        let logo = load_texture(ctx, "guest-logo", &active.logo, 720, 240);
        self.art = Some(GuestArt { theme_id: active.id.clone(), background, logo });
        theme::install(ctx);
    }

    /// Frame files in use: [landscape, portrait]; empty when that orientation is not used.
    fn frame_sources(&self) -> [String; 2] {
        use boothsoft::model::PrintOrientation as O;
        let settings = &self.storage.settings;
        let frames_dir = self.storage.root.join("frames");
        let landscape = if settings.frame_landscape_custom && frames_dir.join("landscape.png").is_file() {
            frames_dir.join("landscape.png").to_string_lossy().into_owned()
        } else { settings.frame_path.clone() };
        let portrait = if settings.frame_portrait_custom && frames_dir.join("portrait.png").is_file() {
            frames_dir.join("portrait.png").to_string_lossy().into_owned()
        } else {
            theme::active().and_then(|t| t.frame_portrait.clone()).map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| PLAIN_PORTRAIT.to_owned())
        };
        match settings.print_orientation {
            O::Landscape => [landscape, String::new()],
            O::Portrait => [String::new(), portrait],
            O::Guest => [landscape, portrait],
        }
    }

    fn reload_frame(&mut self) {
        let sources = self.frame_sources();
        let key = sources.join("|");
        if self.frame_path_loaded == key { return; }
        self.frame_path_loaded = key;
        for (slot, path) in sources.iter().enumerate() {
            self.frames[slot] = if path.is_empty() { None }
                else if path == PLAIN_PORTRAIT {
                    // No portrait frame: the photo covers the whole 10×15 print.
                    Some(compositor::FrameRef::plain(1200, 1800))
                } else {
                    match compositor::FrameRef::open(std::path::Path::new(path)) {
                        Ok(frame) => {
                            let w = frame.window;
                            boothsoft::log::info(format!("Frame {path}: {}x{}, photo window {}x{} at ({}, {})",
                                frame.width, frame.height, w.width, w.height, w.x, w.y));
                            Some(frame)
                        }
                        Err(error) => { self.status = format!("Frame unavailable: {path} ({error})"); None }
                    }
                };
        }
    }

    /// Orientation of the next photo.
    fn print_portrait(&self) -> bool {
        use boothsoft::model::PrintOrientation as O;
        match self.storage.settings.print_orientation { O::Landscape => false, O::Portrait => true, O::Guest => self.guest_portrait }
    }

    /// Frame of the next photo (landscape or portrait).
    fn active_frame(&self) -> Option<&compositor::FrameRef> {
        self.frames[usize::from(self.print_portrait())].as_ref()
    }

    fn palette(&self) -> Palette { Palette::current() }

    /// True when OpenGL is rendered by the CPU (no usable GPU or driver).
    fn software_renderer(&self) -> bool {
        let r = self.renderer.to_ascii_lowercase();
        ["llvmpipe", "softpipe", "swrast", "gdi generic", "basic render", "swiftshader", "lavapipe"].iter().any(|n| r.contains(n))
    }

    pub(super) fn renderer_summary(&self) -> String {
        if self.renderer.is_empty() { return tr!(fr: "Rendu : inconnu", en: "Renderer: unknown").into(); }
        if self.software_renderer() {
            tr!(fr: format!("Rendu : logiciel, sans GPU ({}). Conseil : 15-20 images/s pour le live.", self.renderer), en: format!("Renderer: software, no GPU ({}). Tip: 15-20 fps for the live view.", self.renderer))
        } else {
            tr!(fr: format!("Rendu : GPU ({})", self.renderer), en: format!("Renderer: GPU ({})", self.renderer))
        }
    }

    /// Theme background artwork, or a soft gradient when the theme has none.
    fn paint_backdrop(&self, painter: &egui::Painter, rect: Rect, pal: &Palette, veil: u8) {
        if let Some(texture) = self.art.as_ref().and_then(|art| art.background.as_ref()) {
            ui::image_cover(painter, texture, rect, false, Color32::WHITE);
        } else {
            ui::gradient(painter, rect, pal.bg, ui::shade(pal.bg, if pal.dark_background() { 0.8 } else { 0.95 }));
        }
        if veil > 0 { painter.rect_filled(rect, 0.0, ui::alpha(pal.bg, veil)); }
    }

    /// Logos have arbitrary aspect ratios: fit them in a box rather than a fixed height.
    fn paint_logo(&self, painter: &egui::Painter, anchor: Pos2, bounds: Vec2, align: Align2) -> Rect {
        let Some(logo) = self.art.as_ref().and_then(|art| art.logo.as_ref()) else { return Rect::NOTHING; };
        let natural = logo.size_vec2();
        let size = natural * (bounds.x / natural.x.max(1.0)).min(bounds.y / natural.y.max(1.0));
        let rect = align.anchor_size(anchor, size);
        painter.image(logo.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        rect
    }

    /// Prints still allowed for the current event: (left, maximum). `None` when
    /// printing is off or no quota is configured. Jobs already queued count as used.
    fn prints_left(&self) -> Option<(u32, u32)> {
        if !self.storage.settings.print_enabled { return None; }
        let event = &self.print_event();
        let quota = self.print_ledger.events.get(event).filter(|q| q.included > 0)?;
        let used = quota.completed.saturating_add(self.print_jobs.outstanding(event));
        Some((quota.maximum().saturating_sub(used), quota.maximum()))
    }

    fn event_label(&self) -> Option<String> {
        let event = self.current_event();
        let name = event.name.trim();
        (!name.is_empty() && name != "Votre événement" && name != "Your event").then(|| name.to_owned())
    }

    fn render_welcome(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let pal = self.palette();
        let scale = (rect.height() / 800.0).clamp(0.8, 1.6);
        let painter = ui.painter().clone();
        self.paint_backdrop(&painter, rect, &pal, 0);
        let labels = theme::active().map(|t| t.config.ui.clone());
        // Waking screen: the text column slides in from the left.
        let enter = ui::appear(ui.ctx(), "screen-welcome", 0.4);
        let left = rect.left() + 72.0 * scale - (1.0 - enter) * 60.0 * scale;
        let logo = self.paint_logo(&painter, Pos2::new(left - 6.0 * scale, rect.top() + 48.0 * scale), Vec2::new(380.0, 120.0) * scale, Align2::LEFT_TOP);
        // Top-right status chips, right to left: prints left, then the event name.
        let chip_y = logo.center().y.max(rect.top() + 79.0 * scale);
        let mut chip_right = rect.right() - 56.0 * scale;
        if let Some((left, maximum)) = self.prints_left() {
            let low = left == 0 || left * 10 <= maximum;
            let text = match left { 0 => tr!(fr: "Plus de tirages", en: "No prints left").to_owned(), 1 => tr!(fr: "1 tirage restant", en: "1 print left").to_owned(), n => tr!(fr: format!("{n} tirages restants"), en: format!("{n} prints left")) };
            let color = if low { pal.on_accent } else { pal.fg };
            let galley = painter.layout_no_wrap(text, ui::bold(15.0 * scale), color);
            let chip = Align2::RIGHT_CENTER.anchor_size(Pos2::new(chip_right, chip_y), galley.size() + Vec2::new(46.0, 18.0) * scale);
            painter.rect_filled(chip, CornerRadius::same(255), if low { pal.accent } else { ui::alpha(pal.surface, 225) });
            ui::icon(&painter, Pos2::new(chip.left() + 21.0 * scale, chip.center().y), 15.0 * scale, if low { pal.on_accent } else { pal.accent }, Icon::Print);
            painter.galley(Pos2::new(chip.left() + 35.0 * scale, chip.center().y - galley.size().y / 2.0), galley, color);
            chip_right = chip.left() - 10.0 * scale;
        }
        if let Some(event) = self.event_label() {
            let font = ui::bold(15.0 * scale);
            let galley = painter.layout_no_wrap(event, font, pal.fg);
            let chip = Align2::RIGHT_CENTER.anchor_size(Pos2::new(chip_right, chip_y), galley.size() + Vec2::new(46.0, 18.0) * scale);
            painter.rect_filled(chip, CornerRadius::same(255), ui::alpha(pal.surface, 225));
            ui::icon(&painter, Pos2::new(chip.left() + 21.0 * scale, chip.center().y), 15.0 * scale, pal.accent, Icon::Heart);
            painter.galley(Pos2::new(chip.left() + 35.0 * scale, chip.center().y - galley.size().y / 2.0), galley, pal.fg);
        }
        let column = (rect.width() * 0.48).max(420.0);
        let mut y = rect.top() + rect.height() * 0.25;
        let eyebrow = labels.as_ref().map_or("Photobooth", |l| l.welcome_eyebrow());
        y = painter.text(Pos2::new(left, y), Align2::LEFT_TOP, ui::eyebrow(eyebrow), ui::bold(16.0 * scale), pal.accent).bottom() + 18.0 * scale;
        let title = labels.as_ref().map_or(tr!(fr: "Souriez !", en: "Smile!"), |l| l.welcome_title());
        let galley = painter.layout(title.to_owned(), ui::display(72.0 * scale), pal.primary, column);
        let title_height = galley.size().y;
        painter.galley(Pos2::new(left, y), galley, pal.primary);
        y += title_height + 22.0 * scale;
        let subtitle = labels.as_ref().map_or("", |l| l.welcome_subtitle());
        let galley = painter.layout(subtitle.to_owned(), ui::body(23.0 * scale), pal.fg, column * 0.9);
        let sub_height = galley.size().y;
        painter.galley(Pos2::new(left, y), galley, ui::alpha(pal.fg, 210));
        y += sub_height + 40.0 * scale;
        let start = labels.as_ref().map_or(tr!(fr: "Commencer", en: "Start"), |l| l.start());
        let height = 84.0 * scale;
        let cta = Rect::from_min_size(Pos2::new(left, y), Vec2::new(ui::button_width(ui, start, height, true), height));
        // The entire screen is a start target; the button gives it an obvious place.
        let anywhere = ui.interact(rect, ui.id().with("welcome-anywhere"), egui::Sense::click());
        let corner = Rect::from_min_size(Pos2::new(rect.right() - 90.0, rect.top()), Vec2::splat(90.0));
        let pressed_cta = ui::button(ui, cta, "start", start, Some(Icon::Arrow), Kind::Primary, &pal);
        if anywhere.clicked() && anywhere.interact_pointer_pos().is_some_and(|pos| corner.contains(pos)) {
            self.admin_tap();
            return;
        }
        if pressed_cta || anywhere.clicked() {
            // Wake up: straight to the live view, the session starts at the shutter.
            self.enter_ready();
        }
    }

    fn open_admin(&mut self) {
        self.screen = Screen::Admin;
        self.guestbook = None; // closes the microphone; the camera stop ends any recording
        // Maintenance does not display the live camera texture.
        self.preview_filtered = None;
        self.camera.send(CameraCommand::Stop);
        self.admin_authorized = false;
        self.admin_saved = None;
        self.status.clear();
        self.last_touch = Instant::now();
    }

    /// Returns true when the GPU path drew the frame *with* `shade` applied;
    /// otherwise the caller must draw the overlays itself.
    fn draw_live(&self, ui: &egui::Ui, rect: Rect, shade: live_gl::Shade) -> bool {
        let painter = ui.painter();
        if self.live.as_ref().is_some_and(|live| live.paint(painter, rect, true, self.active_filter, 0.0, shade)) {
            return true;
        }
        painter.rect_filled(rect, 0.0, Color32::from_rgb(18, 20, 19));
        match self.preview_filtered.as_ref() {
            // Guests expect a mirror; the saved photo is not mirrored.
            Some(texture) => ui::image_cover(painter, texture, rect, true, Color32::WHITE),
            None => {
                ui::icon(painter, rect.center() - Vec2::new(0.0, 30.0), 64.0, Color32::from_white_alpha(160), Icon::Camera);
                painter.text(rect.center() + Vec2::new(0.0, 40.0), Align2::CENTER_CENTER,
                    tr!(fr: "Connexion à l’appareil photo…", en: "Connecting to the camera…"), ui::body(22.0), Color32::from_white_alpha(200));
            }
        }
        false
    }

    /// Screen area that will be printed (centre crop to the frame's ratio).
    fn print_area(&self, rect: Rect) -> Option<Rect> {
        let camera = self.live.as_ref().and_then(live_gl::LiveView::frame_size)
            .map(|[w, h]| Vec2::new(w as f32, h as f32))
            .or_else(|| self.preview_filtered.as_ref().map(TextureHandle::size_vec2));
        let (Some(camera), Some(frame)) = (camera, self.active_frame()) else { return None; };
        // The guide shows what will be printed: the frame's photo window.
        let print_ratio = frame.window.ratio();
        // Printed region in camera UV (centre crop), then in screen space.
        let print_uv = ui::cover_uv(camera, Vec2::new(print_ratio, 1.0));
        let shown_uv = ui::cover_uv(camera, rect.size());
        let to_x = |u: f32| rect.left() + (u - shown_uv.min.x) / (shown_uv.max.x - shown_uv.min.x) * rect.width();
        let to_y = |v: f32| rect.top() + (v - shown_uv.min.y) / (shown_uv.max.y - shown_uv.min.y) * rect.height();
        let area = Rect::from_min_max(Pos2::new(to_x(print_uv.min.x), to_y(print_uv.min.y)), Pos2::new(to_x(print_uv.max.x), to_y(print_uv.max.y)))
            .intersect(rect);
        (area.width() < rect.width() - 2.0 || area.height() < rect.height() - 2.0).then_some(area)
    }

    /// Shader parameters for the live view: vignettes + print-area shading.
    fn live_shade(&self, rect: Rect, scale: f32, vignettes: bool, dim: f32) -> live_gl::Shade {
        let mut shade = live_gl::Shade { dim, ..Default::default() };
        if vignettes {
            shade.top = (150.0 * scale / rect.height(), 120.0 / 255.0);
            shade.bottom = (230.0 * scale / rect.height(), 165.0 / 255.0);
        }
        if let Some(area) = self.print_area(rect) {
            let n = |p: Pos2| Pos2::new((p.x - rect.left()) / rect.width(), (p.y - rect.top()) / rect.height());
            shade.keep = Rect::from_min_max(n(area.min), n(area.max));
            shade.outside = 110.0 / 255.0;
        }
        shade
    }

    /// Overlay version (texture fallback) + the crop hairlines (both paths).
    fn paint_print_guide(&self, painter: &egui::Painter, rect: Rect, scale: f32, shaded_by_gpu: bool) {
        let Some(area) = self.print_area(rect) else { return; };
        if !shaded_by_gpu {
            let shade = Color32::from_black_alpha(110);
            painter.rect_filled(Rect::from_min_max(rect.min, Pos2::new(area.left(), rect.bottom())), 0.0, shade);
            painter.rect_filled(Rect::from_min_max(Pos2::new(area.right(), rect.top()), rect.max), 0.0, shade);
            painter.rect_filled(Rect::from_min_max(Pos2::new(area.left(), rect.top()), Pos2::new(area.right(), area.top())), 0.0, shade);
            painter.rect_filled(Rect::from_min_max(Pos2::new(area.left(), area.bottom()), Pos2::new(area.right(), rect.bottom())), 0.0, shade);
        }
        // A hairline on the crop boundary; nothing at all when the print shows everything.
        let line = Stroke::new(1.5_f32 * scale.max(1.0), Color32::from_white_alpha(150));
        if area.left() > rect.left() + 2.0 {
            painter.line_segment([area.left_top(), area.left_bottom()], line);
            painter.line_segment([area.right_top(), area.right_bottom()], line);
        }
        if area.top() > rect.top() + 2.0 {
            painter.line_segment([area.left_top(), area.right_top()], line);
            painter.line_segment([area.left_bottom(), area.right_bottom()], line);
        }
    }

    /// Photos of the next burst, or 1 when the guest chose a single photo.
    fn burst_choice(&self) -> u8 {
        if self.guest_mode == GuestMode::Burst { self.storage.settings.burst_size() } else { 1 }
    }

    fn shot_progress(&self) -> Option<(usize, usize)> {
        if self.burst_choice() > 1 { return None; }
        let total = usize::from(self.storage.settings.shots_per_session.max(1));
        (total > 1).then(|| (self.session.as_ref().map_or(0, |s| s.shots.len()) + 1, total))
    }

    fn render_ready(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let pal = self.palette();
        let scale = (rect.height() / 800.0).clamp(0.8, 1.6);
        let gpu = self.draw_live(ui, rect, self.live_shade(rect, scale, true, 0.0));
        let painter = ui.painter().clone();
        self.paint_print_guide(&painter, rect, scale, gpu);
        if !gpu {
            ui::gradient(&painter, Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + 150.0 * scale)),
                Color32::from_black_alpha(120), Color32::TRANSPARENT);
            ui::gradient(&painter, Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - 230.0 * scale), rect.max),
                Color32::TRANSPARENT, Color32::from_black_alpha(165));
        }
        // Logo on a chip in the theme's own background color, so any logo stays legible.
        // No branding over the guests: the top-left corner is only an invisible
        // maintenance hotspot (5 quick taps).
        let hotspot = Rect::from_min_size(rect.min, Vec2::splat(110.0 * scale));
        if ui.interact(hotspot, ui.id().with("admin-hotspot"), egui::Sense::click()).clicked() { self.admin_tap(); }
        // Controls slide into place when the live view (re)appears.
        let enter = ui::appear(ui.ctx(), "screen-ready", ui::ENTER);
        let slide = (1.0 - enter) * 70.0 * scale;
        let offer_modes = self.guest_modes().len() > 1 && self.session.as_ref().is_none_or(|s| s.shots.is_empty());
        if offer_modes {
            self.paint_mode_switch(ui, &painter, rect.translate(Vec2::new(0.0, -slide)), scale, &pal);
        } else {
            let hint = match self.shot_progress() {
                Some((current, total)) => tr!(fr: format!("Photo {current} / {total}  ·  Placez-vous dans le cadre"), en: format!("Photo {current} of {total}  ·  Step into the frame")),
                None => tr!(fr: "Placez-vous dans le cadre", en: "Step into the frame").to_owned(),
            };
            ui::shadowed_text(&painter, Pos2::new(rect.center().x, rect.top() + 61.0 * scale), Align2::CENTER_CENTER,
                &hint, ui::bold(22.0 * scale), Color32::WHITE);
        }
        let fresh = self.session.as_ref().is_none_or(|s| s.shots.is_empty());
        if fresh && self.guest_mode != GuestMode::GuestBook
            && self.storage.settings.print_orientation == boothsoft::model::PrintOrientation::Guest {
            self.paint_orientation_switch(ui, &painter, rect.translate(Vec2::new(0.0, -slide)), scale, &pal);
        }
        // Cancel only makes sense mid-session (multi-shot): start over from scratch.
        let close = Rect::from_center_size(Pos2::new(rect.right() - 62.0 * scale, rect.top() + 61.0 * scale), Vec2::splat(58.0 * scale));
        if self.session.as_ref().is_some_and(|s| !s.shots.is_empty())
            && ui::button(ui, close, "close", "", Some(Icon::Close), Kind::Glass, &pal) {
            self.finish_session();
            return;
        }
        let shutter_center = Pos2::new(rect.center().x, rect.bottom() - 112.0 * scale + slide);
        if self.guest_mode == GuestMode::GuestBook {
            // No filters in video: the shutter opens the microphone test.
            ui::shadowed_text(&painter, shutter_center + Vec2::new(0.0, 84.0 * scale), Align2::CENTER_CENTER, tr!(fr: "Laisser un message", en: "Leave a message"), ui::bold(18.0 * scale), Color32::WHITE);
            if ui::shutter_with(ui, shutter_center, 60.0 * scale, &pal, Icon::Mic) { self.open_mic_check(); }
        } else {
            let label = theme::active().map_or_else(|| tr!(fr: "Prendre la photo", en: "Take the photo").to_owned(), |t| t.config.ui.shutter().to_owned());
            ui::shadowed_text(&painter, shutter_center + Vec2::new(0.0, 84.0 * scale), Align2::CENTER_CENTER, &label, ui::bold(18.0 * scale), Color32::WHITE);
            if ui::shutter(ui, shutter_center, 60.0 * scale, &pal) { self.begin_countdown(); }
            self.paint_filter_strip(ui, &painter, shutter_center, rect, scale, &pal);
        }
        if self.storage.settings.demo_mode {
            ui::shadowed_text(&painter, Pos2::new(rect.right() - 110.0 * scale, rect.top() + 61.0 * scale), Align2::RIGHT_CENTER,
                tr!(fr: "DÉMO", en: "DEMO"), ui::bold(15.0 * scale), Color32::from_white_alpha(200));
        }
    }

    /// Modes offered on the live view, « Une photo » first (and preselected).
    fn guest_modes(&self) -> Vec<GuestMode> {
        let mut modes = vec![GuestMode::Photo];
        if self.storage.settings.burst_enabled { modes.push(GuestMode::Burst); }
        if self.storage.settings.guestbook_enabled { modes.push(GuestMode::GuestBook); }
        modes
    }

    /// "Une photo | Rafale ×5 | Livre d'or" switch at the top of the live view.
    fn paint_mode_switch(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect, scale: f32, pal: &Palette) {
        let count = self.storage.settings.burst_size();
        let modes = self.guest_modes();
        if !modes.contains(&self.guest_mode) { self.guest_mode = GuestMode::Photo; }
        let labels: Vec<String> = modes.iter().map(|m| match m {
            GuestMode::Photo => tr!(fr: "Une photo", en: "Single photo").to_owned(),
            GuestMode::Burst => tr!(fr: format!("Rafale ×{count}"), en: format!("Burst ×{count}")),
            GuestMode::GuestBook => tr!(fr: "Livre d’or", en: "Guest book").to_owned(),
        }).collect();
        let font = ui::bold(19.0 * scale);
        let seg_h = 50.0 * scale;
        let icon_w = 30.0 * scale;
        let widths: Vec<f32> = labels.iter().zip(&modes)
            .map(|(l, m)| painter.layout_no_wrap(l.clone(), font.clone(), Color32::WHITE).size().x + 56.0 * scale
                + if *m == GuestMode::GuestBook { icon_w } else { 0.0 }).collect();
        let pad = 5.0 * scale;
        let total = widths.iter().sum::<f32>() + 2.0 * pad;
        let outer = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 58.0 * scale), Vec2::new(total, seg_h + 2.0 * pad));
        painter.rect(outer, CornerRadius::same(255), Color32::from_black_alpha(110), Stroke::new(1.0_f32, Color32::from_white_alpha(45)), egui::StrokeKind::Inside);
        // The selection pill glides from one mode to the other.
        let selected_index = modes.iter().position(|m| *m == self.guest_mode).unwrap_or(0);
        let target_x: f32 = widths[..selected_index].iter().sum();
        let pill_x = ui::glide(ui.ctx(), "mode-x", target_x);
        let pill_w = ui::glide(ui.ctx(), "mode-w", widths[selected_index]);
        painter.rect_filled(Rect::from_min_size(Pos2::new(outer.left() + pad + pill_x, outer.top() + pad), Vec2::new(pill_w, seg_h)),
            CornerRadius::same(255), pal.accent);
        let mut x = outer.left() + pad;
        for (index, ((label, width), mode)) in labels.iter().zip(&widths).zip(&modes).enumerate() {
            let segment = Rect::from_min_size(Pos2::new(x, outer.top() + pad), Vec2::new(*width, seg_h));
            x += width;
            let selected = *mode == self.guest_mode;
            let ink = if selected { pal.on_accent } else { Color32::WHITE };
            if *mode == GuestMode::GuestBook {
                let galley = painter.layout_no_wrap(label.clone(), font.clone(), ink);
                let left = segment.center().x - (galley.size().x + icon_w) / 2.0;
                ui::icon(painter, Pos2::new(left + icon_w * 0.4, segment.center().y), 22.0 * scale, ink, Icon::Mic);
                painter.galley(Pos2::new(left + icon_w, segment.center().y - galley.size().y / 2.0), galley, ink);
            } else {
                painter.text(segment.center(), Align2::CENTER_CENTER, label, font.clone(), ink);
            }
            if ui.interact(segment, ui.id().with(("mode", index)), egui::Sense::click()).clicked() {
                self.guest_mode = *mode;
                self.last_touch = Instant::now();
            }
        }
        let hint = match self.guest_mode {
            GuestMode::Burst => tr!(fr: format!("{count} photos à la suite : changez de pose entre chaque !"), en: format!("{count} photos in a row: change your pose each time!")),
            GuestMode::GuestBook => tr!(fr: format!("Laissez un message vidéo pour {}", self.recipient()), en: format!("Leave a video message for {}", self.recipient())),
            GuestMode::Photo => tr!(fr: "Placez-vous dans le cadre", en: "Step into the frame").to_owned(),
        };
        ui::shadowed_text(painter, Pos2::new(rect.center().x, outer.bottom() + 24.0 * scale), Align2::CENTER_CENTER,
            &hint, ui::bold(17.0 * scale), Color32::from_white_alpha(230));
    }

    /// Landscape / portrait choice at the top right of the live view (only when
    /// the orientation is left to guests). The crop guide follows the choice.
    fn paint_orientation_switch(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect, scale: f32, pal: &Palette) {
        let seg = Vec2::new(64.0, 50.0) * scale;
        let pad = 5.0 * scale;
        let outer = Rect::from_min_size(Pos2::new(rect.right() - 40.0 * scale - 2.0 * seg.x - 2.0 * pad, rect.top() + 58.0 * scale - seg.y / 2.0 - pad),
            Vec2::new(2.0 * seg.x + 2.0 * pad, seg.y + 2.0 * pad));
        painter.rect(outer, CornerRadius::same(255), Color32::from_black_alpha(110), Stroke::new(1.0_f32, Color32::from_white_alpha(45)), egui::StrokeKind::Inside);
        let pill_x = ui::glide(ui.ctx(), "orient-x", if self.guest_portrait { seg.x } else { 0.0 });
        painter.rect_filled(Rect::from_min_size(Pos2::new(outer.left() + pad + pill_x, outer.top() + pad), seg), CornerRadius::same(255), pal.accent);
        for (index, portrait) in [false, true].into_iter().enumerate() {
            let cell = Rect::from_min_size(Pos2::new(outer.left() + pad + index as f32 * seg.x, outer.top() + pad), seg);
            let ink = if portrait == self.guest_portrait { pal.on_accent } else { Color32::WHITE };
            let shape = if portrait { Vec2::new(18.0, 26.0) } else { Vec2::new(28.0, 19.0) } * scale;
            painter.rect_stroke(Rect::from_center_size(cell.center(), shape), CornerRadius::same(3), Stroke::new(2.5 * scale, ink), egui::StrokeKind::Middle);
            if ui.interact(cell, ui.id().with(("orientation", index)), egui::Sense::click()).clicked() {
                self.guest_portrait = portrait;
                self.last_touch = Instant::now();
            }
        }
        let label = if self.guest_portrait { tr!(fr: "Portrait", en: "Portrait") } else { tr!(fr: "Paysage", en: "Landscape") };
        ui::shadowed_text(painter, Pos2::new(outer.center().x, outer.bottom() + 20.0 * scale), Align2::CENTER_CENTER, label, ui::bold(15.0 * scale), Color32::from_white_alpha(230));
    }

    /// Six live thumbnails, three on each side of the shutter: each one is the
    /// camera feed through its own filter. On the GPU path they reuse the frame
    /// already uploaded for the main view (six extra small quads, no CPU work).
    fn paint_filter_strip(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, shutter: Pos2, rect: Rect, scale: f32, pal: &Palette) {
        let clearance = 92.0 * scale;
        let margin = 28.0 * scale;
        let base = Vec2::new(132.0, 88.0) * scale;
        let base_gap = 12.0 * scale;
        let side = shutter.x - clearance - (rect.left() + margin);
        let fit = (side / (3.0 * base.x + 2.0 * base_gap)).min(1.0);
        let (card, gap) = (base * fit, base_gap * fit);
        let group = 3.0 * card.x + 2.0 * gap;
        let top = shutter.y - card.y / 2.0 - 12.0 * scale;
        let radius = 12.0 * scale * fit;
        let gpu = self.live.as_ref().is_some_and(live_gl::LiveView::has_frame);
        for (index, filter) in PhotoFilter::ALL.into_iter().enumerate() {
            let start = if index < 3 { shutter.x - clearance - group } else { shutter.x + clearance };
            let x = start + (index % 3) as f32 * (card.x + gap);
            let thumb = Rect::from_min_size(Pos2::new(x, top), card);
            let active = filter == self.active_filter;
            let label_center = Pos2::new(thumb.center().x, thumb.bottom() + 20.0 * scale * fit);
            let hit = thumb.union(Rect::from_center_size(label_center, Vec2::new(card.x, 30.0 * scale)));
            let response = ui.interact(hit, ui.id().with(("filter", index)), egui::Sense::click());
            let pressed = response.is_pointer_button_down_on();
            let thumb = if pressed { thumb.shrink(3.0) } else { thumb };
            let drawn = gpu && self.live.as_ref().is_some_and(|live| live.paint(painter, thumb, true, filter, radius, live_gl::Shade::default()));
            if !drawn {
                // Texture fallback: no per-filter CPU previews (too costly), just names.
                painter.rect_filled(thumb, CornerRadius::same(radius as u8), Color32::from_black_alpha(120));
                painter.text(thumb.center(), Align2::CENTER_CENTER, filter.name(), ui::bold(17.0 * scale * fit), Color32::WHITE);
            }
            let ring = if active { Stroke::new(4.0_f32 * scale, Color32::WHITE) } else { Stroke::new(1.5_f32 * scale, Color32::from_white_alpha(120)) };
            painter.rect_stroke(thumb.expand(if active { 3.0 * scale } else { 0.0 }), CornerRadius::same((radius + if active { 3.0 * scale } else { 0.0 }) as u8),
                ring, egui::StrokeKind::Outside);
            let font = ui::bold(15.0 * scale * fit.max(0.85));
            if active {
                let galley = painter.layout_no_wrap(filter.name().to_owned(), font, pal.on_accent);
                let pill = Rect::from_center_size(label_center, galley.size() + Vec2::new(22.0, 8.0) * scale);
                painter.rect_filled(pill, CornerRadius::same(255), pal.accent);
                painter.galley(pill.center() - galley.size() / 2.0, galley, pal.on_accent);
            } else {
                ui::shadowed_text(painter, label_center, Align2::CENTER_CENTER, filter.name(), font, Color32::WHITE);
            }
            if response.clicked() && !active {
                self.active_filter = filter;
                // The GPU path re-filters for free; only the texture fallback must rebuild.
                if self.storage.settings.demo_mode && self.live.is_none() { self.preview_filtered = None; }
            }
        }
    }

    fn render_countdown(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let elapsed = self.shot_started.map(|start| start.elapsed()).unwrap_or(Duration::from_secs(COUNTDOWN_SECONDS));
        let remaining = COUNTDOWN_SECONDS.saturating_sub(elapsed.as_secs());
        if self.shot_started.is_some() && remaining == 0 && self.capture_pending.is_none() {
            self.shot_started = None;
            self.capture_pending = Some(Instant::now());
            if let Some(burst) = self.burst.as_mut() { burst.requested = 1; }
            if self.storage.settings.demo_mode { self.take_demo_photo(ctx); }
            else { self.camera.send(CameraCommand::Capture); }
        }
        // Burst: next photo once the gap has elapsed.
        let due = self.burst.as_ref().and_then(|b| b.next_at).filter(|_| self.capture_pending.is_none());
        if let Some(at) = due {
            if Instant::now() >= at {
                if let Some(burst) = self.burst.as_mut() { burst.requested += 1; burst.next_at = None; }
                self.capture_pending = Some(Instant::now());
                if self.storage.settings.demo_mode { self.take_demo_photo(ctx); }
                else { self.camera.send(CameraCommand::Capture); }
            } else {
                ctx.request_repaint_after(at - Instant::now());
            }
        }
        if self.capture_pending.is_some_and(|pending| pending.elapsed() > Duration::from_secs(8)) {
            self.capture_pending = None;
            self.status = "Camera capture timed out".into();
            self.screen = Screen::Error;
        }
        let rect = ui.max_rect();
        let pal = self.palette();
        let scale = (rect.height() / 800.0).clamp(0.8, 1.6);
        if let Some((taken, total)) = self.burst.as_ref().filter(|b| b.requested > 0).map(|b| (b.requested, b.total)) {
            self.render_burst_shooting(ui, rect, scale, &pal, taken, total);
            return;
        }
        let gpu = self.draw_live(ui, rect, self.live_shade(rect, scale, false, 70.0 / 255.0));
        let painter = ui.painter();
        self.paint_print_guide(painter, rect, scale, gpu);
        if !gpu { painter.rect_filled(rect, 0.0, Color32::from_black_alpha(70)); }
        let radius = rect.height().min(rect.width()) * 0.2;
        let center = rect.center() - Vec2::new(0.0, 20.0 * scale);
        painter.circle_filled(center, radius, Color32::from_black_alpha(95));
        // One arc segment per second: the ring only changes when the number does.
        let segments = COUNTDOWN_SECONDS as usize;
        let span = std::f32::consts::TAU / segments as f32;
        for i in 0..segments {
            let start = -std::f32::consts::FRAC_PI_2 + i as f32 * span + 0.06;
            let lit = i < remaining as usize;
            ui::arc(painter, center, radius - 8.0 * scale, start, start + span - 0.12,
                Stroke::new(9.0 * scale, if lit { pal.accent } else { Color32::from_white_alpha(45) }));
        }
        if remaining > 0 {
            if let Some(texture) = self.numeral.get(ctx, &remaining.to_string(), radius * 1.45) {
                // Each new second lands with a small pop (300 ms), then holds still.
                let beat = elapsed.subsec_millis() as f32 / 300.0;
                let pop = 1.0 + 0.2 * (1.0 - ui::ease_out(beat));
                ui::numeral(painter, texture, center, pop);
                if beat < 1.0 { ctx.request_repaint(); }
            }
        } else {
            ui::icon(painter, center, radius * 0.6, Color32::WHITE, Icon::Camera);
        }
        let message = match remaining { 4.. => tr!(fr: "Préparez-vous…", en: "Get ready…"), 2..=3 => tr!(fr: "Regardez l’objectif", en: "Look at the lens"), 1 => tr!(fr: "Souriez !", en: "Smile!"), _ => tr!(fr: "Clic !", en: "Click!") };
        let font = ui::bold(28.0 * scale);
        let galley = painter.layout_no_wrap(message.to_owned(), font, Color32::WHITE);
        let pill = Rect::from_center_size(center + Vec2::new(0.0, radius + 56.0 * scale), galley.size() + Vec2::new(56.0, 24.0) * scale);
        painter.rect_filled(pill, CornerRadius::same(255), if remaining == 1 { pal.accent } else { Color32::from_black_alpha(120) });
        painter.galley(pill.center() - galley.size() / 2.0, galley, if remaining == 1 { pal.on_accent } else { Color32::WHITE });
        // The countdown changes once a second; avoid 30-60 FPS UI work.
        if let Some(pending) = self.capture_pending {
            // Camera worker wakes the UI on completion; only schedule the deadline.
            ctx.request_repaint_after(Duration::from_secs(8).saturating_sub(pending.elapsed()));
        } else if remaining > 0 {
            let until_next_second = 1_000 - u64::from(elapsed.subsec_millis());
            ctx.request_repaint_after(Duration::from_millis(until_next_second));
        }
    }

    fn render_processing(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let pal = self.palette();
        let painter = ui.painter().clone();
        self.paint_backdrop(&painter, rect, &pal, 200);
        ui::icon(&painter, rect.center() - Vec2::new(0.0, 40.0), 72.0, pal.accent, Icon::Camera);
        let text = match self.burst.as_ref() {
            Some(burst) => tr!(fr: format!("Développement de vos photos… {} / {}", burst.shots.len(), burst.total), en: format!("Developing your photos… {} of {}", burst.shots.len(), burst.total)),
            None => tr!(fr: "Développement de votre photo…", en: "Developing your photo…").to_owned(),
        };
        painter.text(rect.center() + Vec2::new(0.0, 40.0), Align2::CENTER_CENTER, text, ui::display(34.0), pal.primary);
    }

    /// Review, modelled on dslrBooth's sharing screen: the print fills the
    /// screen over a blurred copy of itself; one clear action bar at the bottom.
    /// Static screen: no repaint until the last 15 s of the auto-return.
    fn render_review(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let pal = self.palette();
        let scale = (rect.height() / 800.0).clamp(0.8, 1.6);
        let painter = ui.painter().clone();
        match self.review_backdrop.as_ref() {
            Some(backdrop) => ui::image_cover(&painter, backdrop, rect, false, Color32::WHITE),
            None => { painter.rect_filled(rect, 0.0, ui::shade(pal.primary, 0.35)); }
        }
        painter.rect_filled(rect, 0.0, Color32::from_black_alpha(150));

        let progress = self.session.as_ref().map_or(0, |s| s.shots.len());
        let total = usize::from(self.storage.settings.shots_per_session.max(1));
        let final_shot = progress >= total;
        let printing = self.storage.settings.print_enabled;
        let labels = theme::active().map(|t| t.config.ui.clone());

        // The print: as large as the screen allows above the action bar.
        let bar_h = 128.0 * scale;
        let area = Rect::from_min_max(rect.min + Vec2::new(40.0, 30.0) * scale, Pos2::new(rect.right() - 40.0 * scale, rect.bottom() - bar_h));
        let enter = ui::appear(ui.ctx(), "screen-review", 0.32);
        if let Some(texture) = self.review.as_ref() {
            // The print lands on the screen: slight zoom-in while the bar slides up.
            let photo = ui::fit(texture.size_vec2(), area.shrink(10.0 * scale));
            let photo = Rect::from_center_size(photo.center() + Vec2::new(0.0, (1.0 - enter) * 26.0 * scale), photo.size() * (0.93 + 0.07 * enter));
            let paper = photo.expand(8.0 * scale);
            ui::soft_shadow(&painter, paper, 4, 40, 16, 120);
            painter.rect_filled(paper, CornerRadius::same(4), Color32::WHITE);
            painter.image(texture.id(), photo, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            if total > 1 {
                let text = tr!(fr: format!("Photo {progress} / {total}"), en: format!("Photo {progress} of {total}"));
                let galley = painter.layout_no_wrap(text, ui::bold(16.0 * scale), Color32::WHITE);
                let pill = Rect::from_min_size(paper.left_top() + Vec2::new(16.0, 16.0) * scale, galley.size() + Vec2::new(28.0, 14.0) * scale);
                painter.rect_filled(pill, CornerRadius::same(255), Color32::from_black_alpha(150));
                painter.galley(pill.center() - galley.size() / 2.0, galley, Color32::WHITE);
            }
        }

        // Action bar: message on the left, actions on the right:
        // Reprendre · Terminer · Imprimer (last shot, printing on), or Photo suivante.
        let bar = Rect::from_min_max(Pos2::new(rect.left() + 48.0 * scale, rect.bottom() - bar_h), Pos2::new(rect.right() - 48.0 * scale, rect.bottom()))
            .translate(Vec2::new(0.0, (1.0 - enter) * bar_h));
        let height = 76.0 * scale;
        let gap = 14.0 * scale;
        let max_copies = self.copies_available();
        let can_print = final_shot && printing && max_copies > 0;
        let finish = labels.as_ref().map_or_else(|| tr!(fr: "Terminer", en: "Done").to_owned(), |l| l.finish().to_owned());
        let retake = labels.as_ref().map_or_else(|| tr!(fr: "Reprendre", en: "Retake").to_owned(), |l| l.retake().to_owned());
        let print_label = labels.as_ref().map_or(tr!(fr: "Imprimer", en: "Print"), |l| l.print()).to_owned();
        // (label, icon, kind, id), right to left.
        let mut actions: Vec<(String, Icon, Kind, &str)> = Vec::new();
        if !final_shot {
            actions.push((tr!(fr: "Photo suivante", en: "Next photo").to_owned(), Icon::Arrow, Kind::Primary, "next"));
        } else if can_print {
            actions.push((print_label, Icon::Print, Kind::Primary, "print"));
            actions.push((finish, Icon::Check, Kind::Glass, "finish"));
        } else {
            actions.push((finish, Icon::Check, Kind::Primary, "finish"));
        }
        actions.push((retake, Icon::Retake, Kind::Glass, "retake"));
        let cy = bar.center().y;
        let mut right = bar.right();
        let mut rects = Vec::with_capacity(actions.len());
        for (label, _, kind, _) in &actions {
            let mut w = ui::button_width(ui, label, height, true);
            if *kind == Kind::Primary { w = w.max(260.0 * scale); }
            rects.push(Rect::from_min_size(Pos2::new(right - w, cy - height / 2.0), Vec2::new(w, height)));
            right -= w + gap;
        }
        let text_w = (right - bar.left() - 12.0 * scale).max(0.0);
        let (title_fr, title_en) = REVIEW_TITLES[self.review_title];
        let title = painter.layout_no_wrap(tr!(fr: title_fr, en: title_en).to_owned(), ui::display(40.0 * scale), Color32::WHITE);
        let title_h = title.size().y;
        let body = if !final_shot { tr!(fr: "On enchaîne avec la suivante ?", en: "Ready for the next one?") }
            else if can_print { tr!(fr: "Imprimez-la ou terminez sans tirage.", en: "Print it, or finish without a print.") }
            else { tr!(fr: "Elle rejoint l’album de l’événement.", en: "It goes into the event’s album.") };
        let timeout = Duration::from_secs(self.storage.settings.photo_timeout_seconds.into());
        let left = timeout.saturating_sub(self.last_touch.elapsed());
        // The auto-return hint appears only for the last seconds: before that the
        // screen is static and needs no repaint at all.
        let body = if left <= REVIEW_HINT { tr!(fr: format!("{body}\nRetour à l’appareil photo dans {} s", left.as_secs() + 1), en: format!("{body}\nBack to the camera in {} s", left.as_secs() + 1)) } else { body.to_owned() };
        let body = painter.layout(body, ui::body(18.0 * scale), Color32::from_white_alpha(210), text_w.max(1.0));
        let block = title_h + 6.0 * scale + body.size().y;
        let top = cy - block / 2.0;
        if text_w > 220.0 * scale {
            painter.galley(Pos2::new(bar.left(), top), title, Color32::WHITE);
            painter.galley(Pos2::new(bar.left(), top + title_h + 6.0 * scale), body, Color32::WHITE);
        }

        // Copies picker: modal over the review, the bar is inert meanwhile.
        if let Some(selected) = self.copies_picker {
            self.render_copies_picker(ui, rect, scale, &pal, selected, max_copies);
            return;
        }
        let mut pressed = None;
        for ((label, icon, kind, id), button) in actions.iter().zip(&rects) {
            if ui::button(ui, *button, id, label, Some(*icon), *kind, &pal) { pressed = Some(*id); }
        }
        match pressed {
            Some("retake") => {
                if let Some(session) = self.session.as_mut() {
                    if let Err(error) = self.storage.remove_last_shot(session) {
                        self.status = format!("Retake error: {error}");
                    } else {
                        // Retakes stay in the same session and do not consume the session allowance.
                        self.review = None;
                        self.screen = Screen::Ready;
                        if !self.storage.settings.demo_mode {
                            self.camera.send(CameraCommand::SetPreviewEnabled(true));
                        }
                        self.last_touch = Instant::now();
                    }
                }
            }
            Some("next") => {
                self.screen = Screen::Ready;
                self.review = None;
                if !self.storage.settings.demo_mode {
                    self.camera.send(CameraCommand::SetPreviewEnabled(true));
                }
                self.last_touch = Instant::now();
            }
            Some("finish") => self.show_thanks(0),
            Some("print") => {
                // One copy needs no question.
                if max_copies <= 1 { self.print_copies(1); } else { self.copies_picker = Some(1); }
            }
            _ => {}
        }
    }

    /// How many copies the guest may still ask for (setting, then event quota).
    fn copies_available(&self) -> u8 {
        let cap = self.storage.settings.max_copies.clamp(1, 10);
        // No quota configured for the event = unlimited printing.
        match self.prints_left() {
            Some((left, _)) => cap.min(u8::try_from(left).unwrap_or(u8::MAX)),
            None if self.storage.settings.print_enabled => cap,
            None => 0,
        }
    }

    /// Copies above this ask for a confirmation before printing.
    const CONFIRM_ABOVE: u8 = 3;

    fn render_copies_picker(&mut self, ui: &mut egui::Ui, rect: Rect, scale: f32, pal: &Palette, selected: u8, max_copies: u8) {
        let painter = ui.painter().clone();
        // Taps outside the card close it.
        let backdrop = ui.interact(rect, ui.id().with("copies-backdrop"), egui::Sense::click());
        let enter = ui::appear(ui.ctx(), "copies-modal", 0.2);
        painter.rect_filled(rect, 0.0, Color32::from_black_alpha((150.0 * enter) as u8));
        let card = Rect::from_center_size(rect.center() + Vec2::new(0.0, (1.0 - enter) * 60.0 * scale), Vec2::new(600.0, 400.0) * scale);
        ui::soft_shadow(&painter, card, 28, 40, 16, 110);
        painter.rect_filled(card, CornerRadius::same((28.0 * scale) as u8), pal.surface);
        let ink = ui::readable_on(pal.surface);
        let count = selected.clamp(1, max_copies.max(1));
        let plural = |n: u8| if n == 1 { tr!(fr: "1 exemplaire", en: "1 copy").to_owned() } else { tr!(fr: format!("{n} exemplaires"), en: format!("{n} copies")) };
        let height = 72.0 * scale;
        let by = card.bottom() - 40.0 * scale - height;

        if self.copies_confirm {
            // Second step for large orders: avoids 8 prints from an excited tap.
            painter.circle_filled(Pos2::new(card.center().x, card.top() + 84.0 * scale), 40.0 * scale, ui::alpha(pal.accent, 40));
            ui::icon(&painter, Pos2::new(card.center().x, card.top() + 84.0 * scale), 40.0 * scale, pal.accent, Icon::Print);
            painter.text(Pos2::new(card.center().x, card.top() + 146.0 * scale), Align2::CENTER_TOP,
                tr!(fr: format!("Imprimer {} ?", plural(count)), en: format!("Print {}?", plural(count))), ui::display(34.0 * scale), ink);
            painter.text(Pos2::new(card.center().x, card.top() + 196.0 * scale), Align2::CENTER_TOP,
                tr!(fr: "On vérifie avant de lancer l’impression.", en: "A quick check before printing."), ui::body(19.0 * scale), ui::alpha(ink, 170));
            let yes = tr!(fr: format!("Oui, imprimer {count}"), en: format!("Yes, print {count}"));
            let yes_w = ui::button_width(ui, &yes, height, true);
            let back_w = ui::button_width(ui, tr!(fr: "Modifier", en: "Change"), height, false);
            let total = yes_w + back_w + 16.0 * scale;
            let back = Rect::from_min_size(Pos2::new(card.center().x - total / 2.0, by), Vec2::new(back_w, height));
            let ok = Rect::from_min_size(Pos2::new(back.right() + 16.0 * scale, by), Vec2::new(yes_w, height));
            if ui::button(ui, back, "copies-back", tr!(fr: "Modifier", en: "Change"), None, Kind::Glass, pal) {
                self.copies_confirm = false;
            } else if ui::button(ui, ok, "copies-yes", &yes, Some(Icon::Print), Kind::Primary, pal) {
                self.copies_confirm = false;
                self.copies_picker = None;
                self.print_copies(count);
            } else if backdrop.clicked() && backdrop.interact_pointer_pos().is_some_and(|p| !card.contains(p)) {
                self.copies_confirm = false;
            }
            return;
        }

        painter.text(Pos2::new(card.center().x, card.top() + 40.0 * scale), Align2::CENTER_TOP, tr!(fr: "Combien d’exemplaires ?", en: "How many copies?"), ui::display(34.0 * scale), ink);
        // Stepper: [−]  N  [+]
        let row_y = card.top() + 170.0 * scale;
        let step = 84.0 * scale;
        let minus = Rect::from_center_size(Pos2::new(card.center().x - 150.0 * scale, row_y), Vec2::splat(step));
        let plus = Rect::from_center_size(Pos2::new(card.center().x + 150.0 * scale, row_y), Vec2::splat(step));
        let mut next = count;
        for (r, delta, enabled) in [(minus, -1i8, count > 1), (plus, 1, count < max_copies)] {
            let response = ui.interact(r, ui.id().with(("copies-step", delta)), egui::Sense::click());
            let pressed = enabled && response.is_pointer_button_down_on();
            let fill = if !enabled { ui::alpha(ink, 8) } else if pressed { ui::shade(pal.accent, 0.85) } else { pal.accent };
            let glyph = if enabled { pal.on_accent } else { ui::alpha(ink, 60) };
            painter.circle_filled(r.center(), step / 2.0, fill);
            let arm = step * 0.22;
            let stroke = Stroke::new(6.0_f32 * scale, glyph);
            painter.line_segment([r.center() - Vec2::new(arm, 0.0), r.center() + Vec2::new(arm, 0.0)], stroke);
            if delta > 0 { painter.line_segment([r.center() - Vec2::new(0.0, arm), r.center() + Vec2::new(0.0, arm)], stroke); }
            if enabled && response.clicked() {
                next = if delta > 0 { count.saturating_add(1).min(max_copies) } else { count.saturating_sub(1).max(1) };
            }
        }
        painter.text(Pos2::new(card.center().x, row_y), Align2::CENTER_CENTER, count.to_string(), ui::display(80.0 * scale), ink);
        painter.text(Pos2::new(card.center().x, row_y + 62.0 * scale), Align2::CENTER_TOP,
            tr!(fr: format!("jusqu’à {max_copies}"), en: format!("up to {max_copies}")), ui::body(16.0 * scale), ui::alpha(ink, 150));
        self.copies_picker = Some(next);

        let confirm_label = tr!(fr: format!("Imprimer {}", plural(next)), en: format!("Print {}", plural(next)));
        let confirm_w = ui::button_width(ui, &confirm_label, height, true);
        let cancel_w = ui::button_width(ui, tr!(fr: "Annuler", en: "Cancel"), height, false);
        let total = confirm_w + cancel_w + 16.0 * scale;
        let cancel = Rect::from_min_size(Pos2::new(card.center().x - total / 2.0, by), Vec2::new(cancel_w, height));
        let confirm = Rect::from_min_size(Pos2::new(cancel.right() + 16.0 * scale, by), Vec2::new(confirm_w, height));
        let cancel_pressed = ui::button(ui, cancel, "copies-cancel", tr!(fr: "Annuler", en: "Cancel"), None, Kind::Glass, pal);
        let confirm_pressed = ui::button(ui, confirm, "copies-confirm", &confirm_label, Some(Icon::Print), Kind::Primary, pal);
        let outside = backdrop.clicked() && backdrop.interact_pointer_pos().is_some_and(|p| !card.contains(p));
        if confirm_pressed {
            if next > Self::CONFIRM_ABOVE {
                self.copies_confirm = true;
            } else {
                self.copies_picker = None;
                self.print_copies(next);
            }
        } else if cancel_pressed || outside {
            self.copies_picker = None;
        }
    }

    /// Queue `copies` print jobs of the framed photo (one quota reservation each,
    /// so a failure part-way leaves exact accounting), then thank the guest.
    fn print_copies(&mut self, copies: u8) {
        let Some(path) = self.last_export.clone().filter(|p| p.is_file()) else {
            self.status = "The framed JPEG is unavailable".into();
            return;
        };
        let mut queued = 0u8;
        for _ in 0..copies {
            match self.submit_print(path.clone()) {
                Ok(()) => queued += 1,
                Err(error) => { self.status = error; break; }
            }
        }
        if queued == 0 {
            self.notice = Some((tr!(fr: "Impression indisponible pour le moment : vos photos sont bien enregistrées.", en: "Printing is unavailable right now: your photos are saved.").into(), Instant::now()));
        } else {
            self.show_thanks(u16::from(queued));
        }
    }

    fn render_thanks(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let pal = self.palette();
        let scale = (rect.height() / 800.0).clamp(0.8, 1.6);
        let painter = ui.painter().clone();
        self.paint_backdrop(&painter, rect, &pal, 215);
        let labels = theme::active().map(|t| t.config.ui.clone());
        let message_for = self.thanks_message.clone();
        let title = if message_for.is_some() { tr!(fr: "Merci pour votre message !", en: "Thank you for your message!") } else { labels.as_ref().map_or(tr!(fr: "Merci !", en: "Thank you!"), |l| l.thanks_title()) };
        let printing_text = match self.thanks_copies {
            0 => String::new(),
            1 => tr!(fr: "Votre photo s’imprime, récupérez-la dans quelques secondes.", en: "Your photo is printing, pick it up in a few seconds.").to_owned(),
            n => tr!(fr: format!("Vos {n} exemplaires s’impriment, récupérez-les dans quelques secondes."), en: format!("Your {n} copies are printing, pick them up in a few seconds.")),
        };
        let message_text = message_for.as_ref().map(|name| tr!(fr: format!("Il sera remis à {name}."), en: format!("It will be given to {name}.")));
        let subtitle = if let Some(text) = message_text.as_deref() { text }
            else if self.thanks_copies > 0 { printing_text.as_str() }
            else { labels.as_ref().map_or("", |l| l.thanks_subtitle()) };
        // The guest's own print, tilted like a fresh instant photo.
        let text_top = if let Some(photo) = self.thanks_photo.as_ref() {
            let area = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + rect.height() * 0.32), Vec2::new(rect.width() * 0.5, rect.height() * 0.44));
            // The print drops onto the table and settles with its tilt.
            let enter = ui::appear(ui.ctx(), "screen-thanks", 0.36);
            let image = ui::fit(photo.size_vec2(), area);
            let image = Rect::from_center_size(image.center() - Vec2::new(0.0, (1.0 - enter) * 40.0 * scale), image.size() * (1.08 - 0.08 * enter));
            let angle = -0.035 * enter;
            let paper = image.expand(10.0 * scale);
            ui::soft_shadow(&painter, paper.translate(Vec2::new(0.0, 6.0)), 4, 36, 14, (80.0 * enter) as u8);
            ui::rotated_rect(&painter, paper, angle, Color32::WHITE);
            ui::rotated_image(&painter, photo, image, angle);
            paper.bottom() + 36.0 * scale
        } else {
            let center = rect.center() - Vec2::new(0.0, 90.0 * scale);
            painter.circle_filled(center, 64.0 * scale, pal.accent);
            ui::icon(&painter, center, 54.0 * scale, pal.on_accent, if self.thanks_copies > 0 { Icon::Print } else { Icon::Heart });
            center.y + 104.0 * scale
        };
        let bottom = painter.text(Pos2::new(rect.center().x, text_top), Align2::CENTER_TOP, title, ui::display(56.0 * scale), pal.primary).bottom();
        let galley = painter.layout(subtitle.to_owned(), ui::body(22.0 * scale), pal.fg, rect.width() * 0.6);
        let sub_bottom = bottom + 12.0 * scale + galley.size().y;
        painter.galley(Pos2::new(rect.center().x - galley.size().x / 2.0, bottom + 12.0 * scale), galley, ui::alpha(pal.fg, 215));
        if self.thanks_copies > 0 {
            if let Some(tagline) = labels.as_ref().map(|l| l.thanks_subtitle()) {
                painter.text(Pos2::new(rect.center().x, sub_bottom + 18.0 * scale), Align2::CENTER_TOP, tagline, ui::display(22.0 * scale), pal.accent);
            }
        }
        let total = Duration::from_secs(THANKS_SECONDS);
        let elapsed = self.thanks_at.map_or(total, |t| t.elapsed());
        let left = total.saturating_sub(elapsed);
        // Say clearly that the booth goes back to the camera by itself: a pill
        // with the seconds left, updated once a second (no continuous repaint).
        let hint = tr!(fr: format!("Prochaine photo dans {} s  ·  touchez l’écran pour y aller", left.as_secs() + 1), en: format!("Next photo in {} s  ·  touch the screen to go now", left.as_secs() + 1));
        let hint = painter.layout_no_wrap(hint, ui::body(18.0 * scale), pal.fg);
        let icon = 22.0 * scale;
        let pill = Rect::from_center_size(Pos2::new(rect.center().x, rect.bottom() - 48.0 * scale), hint.size() + Vec2::new(36.0 * scale + icon + 10.0 * scale, 18.0 * scale));
        painter.rect_filled(pill, pill.height() / 2.0, ui::alpha(pal.fg, 28));
        ui::icon(&painter, Pos2::new(pill.left() + 18.0 * scale + icon / 2.0, pill.center().y), icon, pal.accent, Icon::Camera);
        painter.galley(Pos2::new(pill.left() + 18.0 * scale + icon + 10.0 * scale, pill.center().y - hint.size().y / 2.0), hint, pal.fg);
        let tapped = ui.interact(rect, ui.id().with("thanks"), egui::Sense::click()).clicked();
        if tapped || left.is_zero() {
            self.enter_ready();
        } else {
            let tick = Duration::from_millis(1_000 - u64::from(left.subsec_millis()) % 1_000).min(left);
            ui.ctx().request_repaint_after(tick);
        }
    }

    fn render_error(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let pal = self.palette();
        let scale = (rect.height() / 800.0).clamp(0.8, 1.6);
        let painter = ui.painter().clone();
        self.paint_backdrop(&painter, rect, &pal, 215);
        let center = rect.center() - Vec2::new(0.0, 110.0 * scale);
        painter.circle_filled(center, 60.0 * scale, ui::alpha(pal.primary, 30));
        ui::icon(&painter, center, 58.0 * scale, pal.primary, Icon::Camera);
        painter.line_segment([center + Vec2::new(-34.0, -34.0) * scale, center + Vec2::new(34.0, 34.0) * scale], Stroke::new(6.0 * scale, pal.accent));
        let bottom = painter.text(center + Vec2::new(0.0, 92.0 * scale), Align2::CENTER_TOP, tr!(fr: "L’appareil photo fait une pause", en: "The camera is taking a break"),
            ui::display(46.0 * scale), pal.primary).bottom();
        painter.text(Pos2::new(rect.center().x, bottom + 14.0 * scale), Align2::CENTER_TOP,
            tr!(fr: "Un instant, on relance tout ça automatiquement.", en: "One moment, everything restarts by itself."), ui::body(21.0 * scale), pal.fg);
        // No button: the camera thread reconnects by itself (every 2 s, then 5 s)
        // and the live view comes back on its own. Only the elapsed time moves.
        let since = *self.error_since.get_or_insert_with(Instant::now);
        let waited = since.elapsed().as_secs();
        let line = if waited < 3 { tr!(fr: "Reconnexion…".to_owned(), en: "Reconnecting…".to_owned()) }
            else { tr!(fr: format!("Reconnexion… {waited} s"), en: format!("Reconnecting… {waited} s")) };
        let pill = painter.layout_no_wrap(line, ui::bold(18.0 * scale), pal.fg);
        let pill_rect = Rect::from_center_size(Pos2::new(rect.center().x, bottom + 96.0 * scale), pill.size() + Vec2::new(36.0, 18.0) * scale);
        painter.rect_filled(pill_rect, pill_rect.height() / 2.0, ui::alpha(pal.fg, 26));
        painter.galley(pill_rect.center() - pill.size() / 2.0, pill, pal.fg);
        if !self.camera_lost && waited >= 5 {
            // A failed capture with a working camera: back to the live view.
            self.error_since = None;
            self.finish_session();
            return;
        }
        let kicked = self.camera_kick.get_or_insert(since);
        if self.camera_lost && kicked.elapsed() >= Duration::from_secs(60) && !self.storage.settings.demo_mode {
            // Belt and braces: ask the camera thread to start over once a minute.
            *kicked = Instant::now();
            self.camera.send(CameraCommand::Start { index: self.storage.settings.camera_index });
        }
        ui.ctx().request_repaint_after(Duration::from_secs(1));
    }

    /// Shutter flash in a few discrete steps (about 6 repaints in total).
    fn paint_flash(&mut self, ctx: &egui::Context, rect: Rect, painter: &egui::Painter) {
        let Some(at) = self.flash_at else { return; };
        let elapsed = at.elapsed().as_millis() as u64;
        if elapsed >= FLASH_MS { self.flash_at = None; return; }
        let step = elapsed / FLASH_STEP_MS;
        let alpha = 255 - (step * 255 * FLASH_STEP_MS / FLASH_MS).min(255) as u8;
        painter.rect_filled(rect, 0.0, Color32::from_white_alpha(alpha));
        ctx.request_repaint_after(Duration::from_millis(FLASH_STEP_MS - elapsed % FLASH_STEP_MS));
    }
}

/// Decode a bounded PNG and downscale it to at most `max_w` x `max_h` before upload.
fn load_texture(ctx: &egui::Context, name: &str, path: &std::path::Path, max_w: u32, max_h: u32) -> Option<TextureHandle> {
    let mut reader = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let (w, h) = (decoded.width(), decoded.height());
    // Scale to cover the target box; never upscale.
    let factor = (f64::from(max_w) / f64::from(w)).max(f64::from(max_h) / f64::from(h)).min(1.0);
    let decoded = if factor < 0.999 {
        decoded.resize((f64::from(w) * factor).round() as u32, (f64::from(h) * factor).round() as u32, image::imageops::FilterType::Triangle)
    } else { decoded };
    let rgba = decoded.into_rgba8();
    let color = ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
    drop(rgba);
    Some(ctx.load_texture(name, color, TextureOptions::LINEAR))
}

/// Field diagnostic: `BOOTHSOFT_FPS_LOG=1` prints the real repaint rate and its causes.
fn log_frame_rate(ctx: &egui::Context) {
    use std::sync::{atomic::{AtomicU32, Ordering}, OnceLock};
    static ENABLED: OnceLock<bool> = OnceLock::new();
    static FRAMES: AtomicU32 = AtomicU32::new(0);
    static WINDOW: OnceLock<std::sync::Mutex<Instant>> = OnceLock::new();
    if !*ENABLED.get_or_init(|| std::env::var_os("BOOTHSOFT_FPS_LOG").is_some_and(|v| v == "1")) { return; }
    let frames = FRAMES.fetch_add(1, Ordering::Relaxed) + 1;
    let Ok(mut since) = WINDOW.get_or_init(|| std::sync::Mutex::new(Instant::now())).lock() else { return; };
    if since.elapsed() >= Duration::from_secs(2) {
        eprintln!("{:.1} repaints/s, causes: {:?}", frames as f32 / since.elapsed().as_secs_f32(), ctx.repaint_causes());
        FRAMES.store(0, Ordering::Relaxed);
        *since = Instant::now();
    }
}

impl eframe::App for Kiosk {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        log_frame_rate(ctx);
        if !self.live_checked {
            self.live_checked = true;
            let forced_cpu = std::env::var_os("BOOTHSOFT_LIVE_CPU").is_some_and(|v| v == "1");
            if let Some(gl) = frame.gl() {
                use eframe::glow::HasContext;
                // SAFETY: read-only query on the current context, UI thread.
                self.renderer = unsafe { gl.get_parameter_string(eframe::glow::RENDERER) };
            }
            if frame.gl().is_some() && !forced_cpu {
                self.live = Some(live_gl::LiveView::new(self.camera.recycler(), self.software_renderer()));
            }
        }
        if self.live.as_ref().is_some_and(live_gl::LiveView::failed) {
            self.live = None;
        }
        if let Some(pulse) = &self.pulse { pulse.stamp(); }
        // A language change in maintenance applies at once (cached system language).
        boothsoft::i18n::set(self.storage.settings.language.resolve());
        self.scheduled_restart();
        self.process_camera(ctx);
        self.process_photo_results(ctx);
        self.process_theme_import();
        self.log_changes();
        self.process_printer_results();
        self.reload_frame();
        self.ensure_art(ctx);
        if self.pin_blocked_until.is_some_and(|until| Instant::now() >= until) {
            self.pin_blocked_until = None;
        }
        let live = matches!(self.screen, Screen::Ready | Screen::Countdown | Screen::MicCheck | Screen::Recording);
        if self.screen == Screen::MessageReview { self.feed_playback(ctx); }
        if self.storage.settings.demo_mode && live {
            match self.live.as_mut() {
                Some(view) if !view.has_frame() => view.submit(image::DynamicImage::ImageRgb8(camera::demo_frame()).into_rgba8()),
                None if self.preview_filtered.is_none() => {
                    self.preview_filtered = Some(texture(ctx, "demo-live", &effects::apply_owned(camera::demo_frame(), self.active_filter)));
                }
                _ => {}
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::D) && i.modifiers.ctrl && i.modifiers.shift) {
            self.open_admin();
        }
        if ctx.input(|i| i.pointer.any_pressed() || !i.events.is_empty()) {
            self.last_touch = Instant::now();
        }
        let timeout = Duration::from_secs(self.storage.settings.photo_timeout_seconds.into());
        let sleep_after = Duration::from_secs(u64::from(self.storage.settings.sleep_minutes.clamp(1, 60)) * 60);
        // Review left alone: back to the live view. Live view left alone: sleep.
        if matches!(self.screen, Screen::Review | Screen::Pick) && self.last_touch.elapsed() > timeout {
            self.finish_session();
        } else if self.screen == Screen::Ready && self.last_touch.elapsed() > sleep_after {
            self.enter_welcome();
        }
        let is_admin = self.screen == Screen::Admin;
        egui::CentralPanel::default().frame(
            // Guest screens cover every pixel themselves; a panel fill would be one
            // more full-screen layer to rasterise (expensive without a GPU).
            egui::Frame::new().fill(if is_admin { theme::background() } else { Color32::TRANSPARENT })
                .inner_margin(0.0)
        ).show(ctx, |ui| {
            if is_admin { self.render_admin(ui); return; }
            match self.screen {
                Screen::Welcome => self.render_welcome(ui),
                Screen::Ready => self.render_ready(ui),
                Screen::Countdown => self.render_countdown(ui, ctx),
                Screen::Review => self.render_review(ui),
                Screen::Pick => self.render_pick(ui),
                Screen::Processing => self.render_processing(ui),
                Screen::Thanks => self.render_thanks(ui),
                Screen::MicCheck => self.render_mic_check(ui),
                Screen::Recording => self.render_recording(ui, ctx),
                Screen::MessageReview => self.render_message_review(ui, ctx),
                Screen::Error => self.render_error(ui),
                Screen::Admin => unreachable!(),
            }
            let painter = ui.painter().clone();
            self.paint_flash(ctx, ui.max_rect(), &painter);
            self.paint_notice(ui);
        });
        // Licence attribution (CPAL-1.0 Exhibit B): at launch and on the sleep screen.
        let since_start = self.started_at.elapsed();
        if since_start < ATTRIBUTION_SPLASH || self.screen == Screen::Welcome {
            paint_attribution(ctx, since_start < ATTRIBUTION_SPLASH);
            if since_start < ATTRIBUTION_SPLASH { ctx.request_repaint_after(ATTRIBUTION_SPLASH - since_start); }
        }
        // No idle polls: native camera events and user input wake the GUI.
        // Review shows a seconds counter; Ready only needs its idle deadline.
        if matches!(self.screen, Screen::Review | Screen::Pick) {
            let left = timeout.saturating_sub(self.last_touch.elapsed());
            let wake = if left > REVIEW_HINT { left - REVIEW_HINT } else { Duration::from_millis(1_000 - u64::from(left.subsec_millis()) % 1_000).min(left) };
            ctx.request_repaint_after(wake);
        } else if self.screen == Screen::Ready {
            ctx.request_repaint_after(sleep_after.saturating_sub(self.last_touch.elapsed()));
        } else if self.screen == Screen::Admin && self.pin_blocked_until.is_some() {
            // Refresh the PIN lockout countdown only while it is displayed.
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let (Some(live), Some(gl)) = (self.live.as_ref(), gl) { live.destroy(gl); }
    }
}

impl Kiosk {
    /// Under the guard: restart once a night at 5 a.m. if the booth is asleep and has
    /// run for over an hour (clears slow leaks, tired drivers). The guard restarts us
    /// at once on `EXIT_RESTART`; nothing is lost (all data is already on disk).
    fn scheduled_restart(&self) {
        use chrono::Timelike;
        if self.pulse.is_none() || self.screen != Screen::Welcome || self.started_at.elapsed() < Duration::from_secs(3600) {
            return;
        }
        let now = chrono::Local::now();
        if now.hour() == 5 && now.minute() < 15 && !self.event_running() {
            boothsoft::log::info("Scheduled nightly restart");
            std::process::exit(boothsoft::watchdog::EXIT_RESTART);
        }
    }

    /// True while a scheduled event slot covers the current time.
    fn event_running(&self) -> bool { self.current_event().until.is_some() }

    /// Write status and screen changes to the disk log (string compare, no allocation when unchanged).
    fn log_changes(&mut self) {
        if self.status != self.logged_status {
            self.logged_status.clone_from(&self.status);
            if !self.status.is_empty() { boothsoft::log::info(format!("Status: {}", self.status)); }
        }
        if self.logged_screen != Some(self.screen) {
            self.logged_screen = Some(self.screen);
            boothsoft::log::info(format!("Screen: {:?}", self.screen));
        }
    }
}

fn texture(ctx: &egui::Context, name: &str, image: &RgbImage) -> TextureHandle {
    let reduced = compositor::fit_within(image, boothsoft::photo_worker::PREVIEW_EDGE);
    let display = reduced.as_ref().unwrap_or(image);
    let size = [display.width() as usize, display.height() as usize];
    let color = ColorImage::from_rgb(size, display.as_raw());
    ctx.load_texture(name, color, TextureOptions::LINEAR)
}

fn valid_pin(value: &str) -> bool {
    value.len() == 6 && value.bytes().all(|byte| byte.is_ascii_digit())
}

// Reuse the GPU allocation instead of recreating a new texture handle each frame.
fn update_texture(ctx: &egui::Context, handle: &mut Option<TextureHandle>, name: &str, image: &RgbImage) {
    let size = [image.width() as usize, image.height() as usize];
    let color = ColorImage::from_rgb(size, image.as_raw());
    if let Some(existing) = handle.as_mut() {
        existing.set(color, TextureOptions::LINEAR);
    } else {
        *handle = Some(ctx.load_texture(name, color, TextureOptions::LINEAR));
    }
}

#[cfg(test)]
mod viewport_tests {
    use super::ui::cover_uv;
    use eframe::egui::{Pos2, Rect, Vec2};
    #[test]
    fn landscape_photo_is_cropped_horizontally_into_square_window() {
        let uv = cover_uv(Vec2::new(1920.0, 1080.0), Vec2::new(700.0, 700.0));
        assert!(uv.min.x > 0.0 && uv.max.x < 1.0);
        assert_eq!(uv.min.y, 0.0);
        assert_eq!(uv.max.y, 1.0);
        assert!((uv.min.x - (1.0 - 1080.0 / 1920.0) / 2.0).abs() < 0.0001);
    }
    #[test]
    fn portrait_photo_is_cropped_vertically_into_landscape_window() {
        let uv = cover_uv(Vec2::new(1080.0, 1920.0), Vec2::new(1600.0, 900.0));
        assert_eq!(uv.min.x, 0.0);
        assert_eq!(uv.max.x, 1.0);
        assert!(uv.min.y > 0.0 && uv.max.y < 1.0);
    }
    #[test]
    fn matching_ratios_keep_the_complete_frame() {
        let uv = cover_uv(Vec2::new(1280.0, 720.0), Vec2::new(800.0, 450.0));
        assert_eq!(uv, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)));
    }
}
