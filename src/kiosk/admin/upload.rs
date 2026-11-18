// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Maintenance → Données & export: sending an event's photos to the operator's website
//! (`docs/specs/GALLERY_UPLOAD.md`). Zipping, hashing and HTTP run on the `bs-upload`
//! thread; the page only reads progress counters and a channel.

use super::*;
use boothsoft::gallery_upload::{self as upload, Failure, Phase, Prepared, Published, Server, Started};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
    mpsc::{sync_channel, Receiver},
};

/// State of the upload settings and of the sending in progress. Reset when maintenance closes.
#[derive(Default)]
pub struct UploadAdmin {
    /// Saved key, loaded on first display (`None` = not loaded yet).
    key: Option<Option<String>>,
    key_entry: String,
    ping: Option<Receiver<Result<upload::Ping, Failure>>>,
    job: Option<Job>,
}

impl UploadAdmin {
    pub fn job_idle(&self) -> bool { self.job.is_none() }
    pub fn busy(&self) -> bool { self.ping.is_some() || self.job.as_ref().is_some_and(|job| job.rx.is_some()) }
}

struct Job {
    folder: String,
    server: Server,
    stage: Stage,
    rx: Option<Receiver<Msg>>,
    phase: Arc<AtomicU8>,
    done: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
    code: String,
    error: String,
    prepared: Option<Prepared>,
    started: Option<Started>,
}

enum Stage { Preparing, Code, Starting, Confirm, Sending, Done(Published, Option<(usize, Vec<bool>)>), Failed }

enum Msg {
    Prepared(Result<Prepared, String>),
    Started(Result<Started, Failure>),
    Sent(Result<Published, Failure>),
}

fn phase_code(phase: Phase) -> u8 { match phase { Phase::Zip => 0, Phase::Hash => 1, Phase::Send => 2, Phase::Publish => 3 } }

impl Job {
    /// Runs `work` on the upload thread; its answer arrives through `rx`.
    fn spawn(&mut self, ctx: &egui::Context, work: impl FnOnce(&dyn Fn(Phase, u64, u64), &AtomicBool) -> Msg + Send + 'static) {
        let (tx, rx) = sync_channel(1);
        let (phase, done, total, cancel) = (Arc::clone(&self.phase), Arc::clone(&self.done), Arc::clone(&self.total), Arc::clone(&self.cancel));
        let wake = ctx.clone();
        let spawned = std::thread::Builder::new().name("bs-upload".into()).spawn(move || {
            let report = |p: Phase, d: u64, t: u64| {
                phase.store(phase_code(p), Ordering::Relaxed);
                done.store(d, Ordering::Relaxed);
                total.store(t, Ordering::Relaxed);
                wake.request_repaint_after(Duration::from_millis(100));
            };
            let _ = tx.send(work(&report, &cancel));
            wake.request_repaint_after(Duration::from_millis(1));
        });
        match spawned {
            Ok(_) => self.rx = Some(rx),
            Err(error) => { self.error = error.to_string(); self.stage = Stage::Failed; }
        }
    }

    fn clean(&self) {
        if let Some(prepared) = &self.prepared { let _ = std::fs::remove_file(&prepared.zip); }
    }
}

impl Kiosk {
    fn upload_key(&mut self) -> Option<String> {
        if self.upload.key.is_none() { self.upload.key = Some(upload::load_key(&self.storage.root)); }
        self.upload.key.clone().flatten()
    }

    /// Address and key both set: the « Envoyer en ligne » buttons appear.
    pub(super) fn upload_ready(&mut self) -> bool {
        !self.storage.settings.upload_url.trim().is_empty() && self.upload_key().is_some()
    }

    pub(super) fn upload_busy(&self) -> bool { self.upload.busy() }

    /// Settings card: website address, booth key, connection test.
    pub(super) fn card_gallery_upload(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        if let Some(result) = self.upload.ping.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.upload.ping = None;
            self.status = match result {
                Ok(ping) if ping.two_factor => tr!(fr: format!("Connexion réussie : {}", ping.booth), en: format!("Connected: {}", ping.booth)),
                Ok(ping) => tr!(fr: format!("Connexion réussie ({}), mais la double authentification n'est pas activée sur le site : les envois seront refusés.", ping.booth),
                                en: format!("Connected ({}), but two-factor authentication is off on the website: sendings will be refused.", ping.booth)),
                Err(failure) => tr!(fr: format!("Connexion impossible : {failure}"), en: format!("Connection failed: {failure}")),
            };
        }
        let key = self.upload_key();
        card(ui, pal, tr!(fr: "Envoi des photos en ligne", en: "Online photo sending"),
             tr!(fr: "Facultatif. Après l'événement, envoie le ZIP de l'événement à votre site, qui publie une galerie privée pour le client. Seule sortie réseau de la borne, toujours à la main.",
                 en: "Optional. After the event, sends the event's ZIP to your website, which publishes a private gallery for the client. The booth's only network exit, always by hand."), |ui| {
            row(ui, pal, tr!(fr: "Adresse du site", en: "Website address"), tr!(fr: "Vide : envoi désactivé.", en: "Empty: sending off."), |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.storage.settings.upload_url)
                    .font(ui::body(17.0)).desired_width(300.0).margin(Vec2::new(12.0, 10.0)).hint_text("https://…").char_limit(200));
            });
            divider(ui, pal);
            match &key {
                Some(saved) => {
                    let masked = upload::mask(saved);
                    row(ui, pal, tr!(fr: "Clé de la borne", en: "Booth key"), &masked, |ui| {
                        if btn(ui, pal, tr!(fr: "Oublier la clé", en: "Forget the key"), Btn::Danger).clicked() {
                            self.status = match upload::forget_key(&self.storage.root) {
                                Ok(()) => { boothsoft::log::info("Upload key forgotten in maintenance"); tr!(fr: "Clé oubliée", en: "Key forgotten").into() }
                                Err(error) => tr!(fr: format!("Impossible d'oublier la clé : {error}"), en: format!("Cannot forget the key: {error}")),
                            };
                            self.upload.key = None;
                        }
                    });
                }
                None => {
                    row(ui, pal, tr!(fr: "Clé de la borne", en: "Booth key"), tr!(fr: "Créée sur le site (bsk_…), affichée une seule fois.", en: "Created on the website (bsk_…), shown only once."), |ui| {
                        let valid = upload::valid_key(&self.upload.key_entry).is_ok();
                        if btn_enabled(ui, pal, valid, tr!(fr: "Enregistrer", en: "Save"), Btn::Primary) {
                            self.status = match upload::save_key(&self.storage.root, &self.upload.key_entry) {
                                Ok(()) => { boothsoft::log::info("Upload key saved in maintenance"); tr!(fr: "Clé enregistrée", en: "Key saved").into() }
                                Err(error) => tr!(fr: format!("Clé non enregistrée : {error}"), en: format!("Key not saved: {error}")),
                            };
                            self.upload.key_entry.clear();
                            self.upload.key = None;
                        }
                        ui.add(egui::TextEdit::singleline(&mut self.upload.key_entry).password(true)
                            .font(ui::body(17.0)).desired_width(240.0).margin(Vec2::new(12.0, 10.0)).hint_text("bsk_…").char_limit(200));
                    });
                }
            }
            divider(ui, pal);
            let testing = self.upload.ping.is_some();
            row(ui, pal, tr!(fr: "Tester la connexion", en: "Test the connection"), tr!(fr: "Vérifie l'adresse et la clé.", en: "Checks the address and the key."), |ui| {
                let label = if testing { tr!(fr: "Test…", en: "Testing…") } else { tr!(fr: "Tester", en: "Test") };
                if btn_enabled(ui, pal, key.is_some() && !testing, label, Btn::Secondary) {
                    match Server::new(&self.storage.settings.upload_url, key.as_deref().unwrap_or_default()) {
                        Ok(server) => {
                            let (tx, rx) = sync_channel(1);
                            let wake = ui.ctx().clone();
                            if std::thread::Builder::new().name("bs-upload-ping".into()).spawn(move || {
                                let _ = tx.send(upload::ping(&server));
                                wake.request_repaint_after(Duration::from_millis(1));
                            }).is_ok() { self.upload.ping = Some(rx); }
                        }
                        Err(error) => self.status = error,
                    }
                }
            });
        });
    }

    /// « Envoyer en ligne » on a stored event: zips and hashes it, then asks for the code.
    pub(super) fn start_gallery_upload(&mut self, ctx: &egui::Context, folder: String) {
        let key = self.upload_key().unwrap_or_default();
        let server = match Server::new(&self.storage.settings.upload_url, &key) {
            Ok(server) => server,
            Err(error) => { self.status = error; return; }
        };
        let mut job = Job {
            folder: folder.clone(), server, stage: Stage::Preparing, rx: None,
            phase: Arc::default(), done: Arc::default(), total: Arc::default(), cancel: Arc::default(),
            code: String::new(), error: String::new(), prepared: None, started: None,
        };
        let sessions = self.storage.root.join("sessions");
        let work = self.storage.root.join("uploads");
        job.spawn(ctx, move |report, _| Msg::Prepared(upload::prepare(&sessions, &folder, &work, report).map_err(|e| e.to_string())));
        boothsoft::log::info(format!("Upload of event {} started in maintenance", job.folder));
        self.upload.job = Some(job);
    }

    /// The sending dialog: progress, two-factor code, booking check, link and QR code.
    pub(super) fn render_upload_dialog(&mut self, ctx: &egui::Context, full: Rect, pal: &Pal) {
        let Some(job) = self.upload.job.as_mut() else { return };
        if let Some(msg) = job.rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            job.rx = None;
            match msg {
                Msg::Prepared(Ok(prepared)) => { job.prepared = Some(prepared); job.stage = Stage::Code; }
                Msg::Prepared(Err(error)) => { job.error = tr!(fr: format!("Préparation du ZIP impossible : {error}"), en: format!("Cannot prepare the ZIP: {error}")); job.stage = Stage::Failed; }
                // Wrong or used code: type another one.
                Msg::Started(Err(failure)) if failure.code == 401 => { job.error = failure.message; job.code.clear(); job.stage = Stage::Code; }
                Msg::Started(Err(failure)) => { job.error = failure.message; job.stage = Stage::Failed; }
                Msg::Started(Ok(started)) => { job.started = Some(started); job.error.clear(); job.stage = Stage::Confirm; }
                Msg::Sent(Ok(published)) => {
                    boothsoft::log::info(format!("Event {} published online ({} photos, {} videos)", job.folder, published.photos, published.videos));
                    let qr = upload::qr_modules(&published.url);
                    job.clean();
                    job.stage = Stage::Done(published, qr);
                }
                Msg::Sent(Err(failure)) => {
                    boothsoft::log::info(format!("Upload of event {} failed: {}", job.folder, failure.message));
                    job.error = failure.message;
                    job.stage = Stage::Failed;
                }
            }
        }
        let mut close = false;
        let mut retry = false;
        let outside = modal(ctx, "gallery-upload", full, pal, 600.0, |ui| {
            let title = match &job.stage {
                Stage::Done(..) => tr!(fr: "Galerie en ligne", en: "Gallery online"),
                Stage::Failed => tr!(fr: "Envoi interrompu", en: "Sending interrupted"),
                _ => tr!(fr: "Envoyer en ligne", en: "Send online"),
            };
            ui.label(RichText::new(title).font(ui::display(28.0)).color(pal.ink));
            ui.label(text(job.folder.clone(), 15.0, pal.muted));
            ui.add_space(6.0);
            let (done, total) = (job.done.load(Ordering::Relaxed), job.total.load(Ordering::Relaxed));
            let fraction = if total == 0 { 0.0 } else { done as f32 / total as f32 };
            match &job.stage {
                Stage::Preparing => {
                    let line = if job.phase.load(Ordering::Relaxed) == 0 {
                        tr!(fr: format!("Préparation du ZIP… {done} / {total} fichiers"), en: format!("Preparing the ZIP… {done} / {total} files"))
                    } else {
                        tr!(fr: format!("Vérification du ZIP… {done} / {total} Mo"), en: format!("Checking the ZIP… {done} / {total} MB"))
                    };
                    progress(ui, pal, fraction, pal.primary);
                    ui.label(text(line, 15.0, pal.muted));
                }
                Stage::Code | Stage::Starting => {
                    let size = job.prepared.as_ref().map_or(0, |p| p.size);
                    ui.label(text(tr!(fr: format!("ZIP prêt ({}). Saisissez le code à 6 chiffres de l'application d'authentification du site.", boothsoft::disk::human(size)),
                                      en: format!("ZIP ready ({}). Enter the 6-digit code of the website's authenticator app.", boothsoft::disk::human(size))), 16.0, pal.ink));
                    if !job.error.is_empty() { ui.label(text(job.error.clone(), 14.0, pal.warn)); }
                    let starting = matches!(job.stage, Stage::Starting);
                    if code_pad(ui, pal, &mut job.code, !starting) {
                        let (server, prepared, code) = (job.server.clone(), job.prepared.clone().expect("prepared"), job.code.clone());
                        job.stage = Stage::Starting;
                        job.error.clear();
                        job.spawn(ctx, move |_, _| Msg::Started(upload::start(&server, &prepared, &code)));
                    }
                    if starting { ui.label(text(tr!(fr: "Vérification du code…", en: "Checking the code…"), 14.0, pal.muted)); }
                }
                Stage::Confirm => {
                    let started = job.started.clone().expect("started");
                    let booking = &started.reservation;
                    let day = day_label(&booking.start_date);
                    let size = boothsoft::disk::human(job.prepared.as_ref().map_or(0, |p| p.size));
                    ui.label(strong(tr!(fr: format!("Envoyer les photos de {} ({day}) ?", booking.client), en: format!("Send the photos of {} ({day})?", booking.client)), 19.0, pal.ink));
                    ui.label(text(tr!(fr: format!("Réservation {} · {size}. Vérifiez que c'est le bon événement.", booking.reference),
                                      en: format!("Booking {} · {size}. Check it is the right event.", booking.reference)), 15.0, pal.muted));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if btn(ui, pal, tr!(fr: "Annuler", en: "Cancel"), Btn::Secondary).clicked() { close = true; }
                        if btn(ui, pal, tr!(fr: "Envoyer", en: "Send"), Btn::Primary).clicked() {
                            let (server, prepared, started) = (job.server.clone(), job.prepared.clone().expect("prepared"), started.clone());
                            job.stage = Stage::Sending;
                            job.spawn(ctx, move |report, cancel| Msg::Sent(upload::send(&server, &prepared, &started, report, cancel)));
                        }
                    });
                }
                Stage::Sending => {
                    let line = if job.phase.load(Ordering::Relaxed) == 3 {
                        tr!(fr: "Envoi terminé. Le site publie la galerie…", en: "Sending done. The website is publishing the gallery…").to_string()
                    } else {
                        tr!(fr: format!("Envoi… morceau {} / {total}", (done + 1).min(total)), en: format!("Sending… piece {} / {total}", (done + 1).min(total)))
                    };
                    progress(ui, pal, if job.phase.load(Ordering::Relaxed) == 3 { 1.0 } else { fraction }, pal.primary);
                    ui.label(text(line, 15.0, pal.muted));
                    ui.label(text(tr!(fr: "Une coupure réseau est reprise automatiquement.", en: "A network cut is retried automatically."), 13.5, pal.muted));
                    if btn(ui, pal, tr!(fr: "Arrêter l'envoi", en: "Stop sending"), Btn::Danger).clicked() { job.cancel.store(true, Ordering::Relaxed); }
                }
                Stage::Done(published, qr) => {
                    ui.label(text(tr!(fr: format!("En ligne : {} photo(s), {} vidéo(s). Le client a reçu le lien par e-mail.", published.photos, published.videos),
                                      en: format!("Online: {} photo(s), {} video(s). The client received the link by e-mail.", published.photos, published.videos)), 16.0, pal.ink));
                    if let Some((width, modules)) = qr { ui.vertical_centered(|ui| qr_code(ui, *width, modules, 260.0)); }
                    ui.label(strong(published.url.clone(), 14.0, pal.ink));
                    if btn(ui, pal, tr!(fr: "Fermer", en: "Close"), Btn::Primary).clicked() { close = true; }
                }
                Stage::Failed => {
                    ui.label(text(job.error.clone(), 16.0, pal.warn));
                    ui.horizontal(|ui| {
                        if btn(ui, pal, tr!(fr: "Fermer", en: "Close"), Btn::Secondary).clicked() { close = true; }
                        if job.prepared.is_some() && btn(ui, pal, tr!(fr: "Recommencer", en: "Try again"), Btn::Primary).clicked() { retry = true; }
                    });
                }
            }
        });
        // A tap outside closes, except while something runs.
        if outside && job.rx.is_none() && !matches!(job.stage, Stage::Sending | Stage::Starting) { close = true; }
        if retry {
            job.code.clear();
            job.error.clear();
            job.started = None;
            job.cancel.store(false, Ordering::Relaxed);
            job.stage = Stage::Code;
        }
        if close {
            job.cancel.store(true, Ordering::Relaxed);
            job.clean();
            self.upload.job = None;
        }
    }
}

/// `2026-12-12` → `12/12/2026` in French, unchanged in English.
fn day_label(iso: &str) -> String {
    match (iso.get(0..4), iso.get(5..7), iso.get(8..10)) {
        (Some(y), Some(m), Some(d)) if iso.len() == 10 => tr!(fr: format!("{d}/{m}/{y}"), en: iso.to_string()),
        _ => iso.to_string(),
    }
}

/// Six dots and a touch keypad; physical digits work too. True when OK is pressed.
fn code_pad(ui: &mut egui::Ui, pal: &Pal, code: &mut String, enabled: bool) -> bool {
    let mut submit = false;
    if enabled {
        for event in ui.input(|i| i.events.clone()) {
            match event {
                egui::Event::Text(t) => for c in t.chars().filter(char::is_ascii_digit) { if code.len() < 6 { code.push(c); } },
                egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => { code.pop(); }
                egui::Event::Key { key: egui::Key::Enter, pressed: true, .. } if code.len() == 6 => submit = true,
                _ => {}
            }
        }
    }
    ui.vertical_centered(|ui| {
        let (dots, _) = ui.allocate_exact_size(Vec2::new(6.0 * 34.0, 22.0), Sense::hover());
        for i in 0..6 {
            let center = Pos2::new(dots.left() + 17.0 + i as f32 * 34.0, dots.center().y);
            if i < code.len() { ui.painter().circle_filled(center, 9.0, pal.primary); }
            else { ui.painter().circle_stroke(center, 9.0, Stroke::new(2.0_f32, ui::alpha(pal.ink, 70))); }
        }
        ui.add_space(10.0);
        let (key, gap) = (72.0, 12.0);
        let (grid, _) = ui.allocate_exact_size(Vec2::new(3.0 * key + 2.0 * gap, 4.0 * key + 3.0 * gap), Sense::hover());
        for (i, label) in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "⌫", "0", "OK"].iter().enumerate() {
            let rect = Rect::from_min_size(grid.min + Vec2::new((i % 3) as f32 * (key + gap), (i / 3) as f32 * (key + gap)), Vec2::splat(key));
            let ok = *label == "OK";
            let active = enabled && (!ok || code.len() == 6);
            let response = ui.interact(rect, ui.id().with(("upload-key", i)), if active { Sense::click() } else { Sense::hover() });
            let fill = if ok { if active { pal.primary } else { ui::alpha(pal.primary, 70) } } else if response.is_pointer_button_down_on() { ui::alpha(pal.ink, 30) } else { pal.bg };
            ui.painter().rect(rect, CornerRadius::same(255), fill, Stroke::new(1.0_f32, if ok { Color32::TRANSPARENT } else { pal.line }), StrokeKind::Inside);
            let color = if ok { pal.on_primary } else if active { pal.ink } else { pal.muted };
            if *label == "⌫" { backspace_glyph(ui.painter(), rect.center(), color); }
            else { ui.painter().text(rect.center(), Align2::CENTER_CENTER, *label, ui::bold(if ok { 20.0 } else { 26.0 }), color); }
            if response.clicked() {
                match *label {
                    "⌫" => { code.pop(); }
                    "OK" => submit = true,
                    digit => if code.len() < 6 { code.push_str(digit); },
                }
            }
        }
    });
    submit && code.len() == 6
}

/// Static QR code: drawn once per frame from its modules, nothing animated.
fn qr_code(ui: &mut egui::Ui, width: usize, modules: &[bool], side: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(8), Color32::WHITE);
    let quiet = 4.0;
    let cell = side / (width as f32 + 2.0 * quiet);
    for (index, dark) in modules.iter().enumerate() {
        if !*dark { continue; }
        let (x, y) = ((index % width) as f32, (index / width) as f32);
        let min = rect.min + Vec2::new((x + quiet) * cell, (y + quiet) * cell);
        ui.painter().rect_filled(Rect::from_min_size(min, Vec2::splat(cell + 0.3)), 0.0, Color32::BLACK);
    }
}
