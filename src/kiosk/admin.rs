// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Protected maintenance UI: device discovery, theming, quota administration.
//! All scanning, ZIP import and printer management stays outside the guest hot path.
//!
//! Layout: top bar (logo, status, "Retour aux photos"), sidebar of sections,
//! one scrolling page of cards, and a sticky save bar while edits are pending.
//! Everything is touch-sized (44 px targets) because the booth has no keyboard.
use super::*;
use boothsoft::model::{EventSlot, Settings};
use eframe::egui::{Align, FontId, Layout, Margin, Sense, StrokeKind, UiBuilder};

/// What the operator was doing when unsaved changes stopped them
/// (`docs/specs/MAINTENANCE_PATTERNS.md` § 2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PendingLeave {
    BackToPhotos,
    CloseApp,
    Lock,
    /// « Annuler » of the bottom bar: abandon without leaving.
    Discard,
}

impl PendingLeave {
    /// (title, save button, discard button)
    fn wording(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::BackToPhotos => (tr!(fr: "Quitter sans enregistrer ?", en: "Leave without saving?"), tr!(fr: "Enregistrer et quitter", en: "Save and leave"), tr!(fr: "Quitter sans enregistrer", en: "Leave without saving")),
            Self::CloseApp => (tr!(fr: "Fermer sans enregistrer ?", en: "Close without saving?"), tr!(fr: "Enregistrer et fermer BoothSoft", en: "Save and close BoothSoft"), tr!(fr: "Fermer sans enregistrer", en: "Close without saving")),
            Self::Lock => (tr!(fr: "Verrouiller sans enregistrer ?", en: "Lock without saving?"), tr!(fr: "Enregistrer et verrouiller", en: "Save and lock"), tr!(fr: "Verrouiller sans enregistrer", en: "Lock without saving")),
            Self::Discard => (tr!(fr: "Abandonner les modifications ?", en: "Discard your changes?"), "", tr!(fr: "Oui, abandonner", en: "Yes, discard")),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum AdminSection {
    #[default]
    Overview,
    Event,
    Printing,
    Camera,
    Theme,
    Security,
    Data,
    GuestBook,
}

impl AdminSection {
    const ALL: [Self; 8] = [Self::Overview, Self::Event, Self::GuestBook, Self::Printing, Self::Camera, Self::Theme, Self::Security, Self::Data];

    fn title(self) -> &'static str {
        match self {
            Self::Overview => tr!(fr: "Vue d'ensemble", en: "Overview"),
            Self::Event => tr!(fr: "Événements", en: "Events"),
            Self::Printing => tr!(fr: "Impression", en: "Printing"),
            Self::Camera => tr!(fr: "Caméra & écran", en: "Camera & screen"),
            Self::Theme => tr!(fr: "Thème", en: "Theme"),
            Self::Security => tr!(fr: "Sécurité", en: "Security"),
            Self::Data => tr!(fr: "Données & export", en: "Data & export"),
            Self::GuestBook => tr!(fr: "Livre d'or", en: "Guest book"),
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Self::Overview => tr!(fr: "L'état de la borne en un coup d'œil.", en: "The booth at a glance."),
            Self::Event => tr!(fr: "Planning des événements et déroulé de la séance photo.", en: "Event schedule and how a photo session runs."),
            Self::Printing => tr!(fr: "Imprimante, exemplaires et tirages prévus pour l'événement.", en: "Printer, copies and prints planned for the event."),
            Self::Camera => tr!(fr: "Appareil photo, fluidité du live et mise en veille.", en: "Camera, live view smoothness and sleep."),
            Self::Theme => tr!(fr: "Habillage de la borne : couleurs, logo, cadre d'impression.", en: "The booth's look: colours, logo, print frame."),
            Self::Security => tr!(fr: "Code d'accès à la maintenance.", en: "Maintenance access code."),
            Self::Data => tr!(fr: "Photos enregistrées, exports et assistance à distance.", en: "Saved photos, exports and remote support."),
            Self::GuestBook => tr!(fr: "Messages vidéo des invités : écouter, supprimer, remettre à l'organisateur.", en: "Guests' video messages: play, delete, hand over to the host."),
        }
    }
}

/// Maintenance colours: always a light, neutral surface (readable whatever the
/// guest theme), with the theme's primary colour for selection and actions.
#[derive(Clone, Copy)]
struct Pal {
    bg: Color32,
    surface: Color32,
    ink: Color32,
    muted: Color32,
    line: Color32,
    primary: Color32,
    on_primary: Color32,
    accent: Color32,
    /// Sidebar: the theme's primary colour (light themes) or a deeper background.
    side: Color32,
    side_ink: Color32,
    ok: Color32,
    warn: Color32,
    dark: bool,
}

impl Pal {
    fn current() -> Self {
        // Maintenance wears the active theme: its colours, its artwork, light or dark.
        let pal = Palette::current();
        let dark = pal.dark_background();
        let side = if dark { ui::shade(pal.bg, 0.7) } else { pal.primary };
        Self {
            bg: pal.bg,
            surface: pal.surface,
            ink: pal.fg,
            muted: theme::muted(),
            line: ui::alpha(pal.fg, if dark { 40 } else { 30 }),
            primary: pal.primary,
            on_primary: ui::readable_on(pal.primary),
            accent: pal.accent,
            side,
            side_ink: ui::readable_on(side),
            ok: Color32::from_rgb(64, 160, 100),
            warn: Color32::from_rgb(214, 96, 70),
            dark,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Btn { Primary, Secondary, Danger }

fn btn(ui: &mut egui::Ui, pal: &Pal, label: &str, kind: Btn) -> egui::Response {
    let (fill, text, stroke) = match kind {
        Btn::Primary => (pal.primary, pal.on_primary, Stroke::NONE),
        Btn::Secondary => (pal.surface, pal.ink, Stroke::new(1.0_f32, ui::alpha(pal.ink, 60))),
        Btn::Danger => (pal.surface, pal.warn, Stroke::new(1.0_f32, ui::alpha(pal.warn, 120))),
    };
    ui.add(egui::Button::new(RichText::new(label).font(ui::bold(15.0)).color(text))
        .fill(fill).stroke(stroke).corner_radius(10).min_size(Vec2::new(0.0, 44.0)))
}

fn btn_enabled(ui: &mut egui::Ui, pal: &Pal, enabled: bool, label: &str, kind: Btn) -> bool {
    ui.add_enabled_ui(enabled, |ui| btn(ui, pal, label, kind).clicked()).inner
}

fn text(value: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(value).font(ui::body(size)).color(color)
}

fn strong(value: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(value).font(ui::bold(size)).color(color)
}

/// White rounded card with an optional title and hint.
fn card(ui: &mut egui::Ui, pal: &Pal, title: &str, hint: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new().fill(pal.surface).corner_radius(16).stroke(Stroke::new(1.0_f32, pal.line))
        .inner_margin(Margin::symmetric(24, 18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !title.is_empty() {
                ui.label(RichText::new(title).font(ui::display(20.0)).color(pal.ink));
                if !hint.is_empty() { ui.label(text(hint, 14.0, pal.muted)); }
                ui.add_space(4.0);
            }
            add(ui);
        });
    ui.add_space(16.0);
}

/// One setting: label and hint on the left, control right-aligned.
fn row(ui: &mut egui::Ui, pal: &Pal, title: &str, hint: &str, add: impl FnOnce(&mut egui::Ui)) {
    let width = ui.available_width();
    ui.allocate_ui_with_layout(Vec2::new(width, 52.0), Layout::left_to_right(Align::Center), |ui| {
        ui.allocate_ui_with_layout(Vec2::new(width * 0.52, 52.0), Layout::top_down(Align::Min), |ui| {
            ui.add_space(6.0);
            ui.label(strong(title, 16.0, pal.ink));
            if !hint.is_empty() { ui.add(egui::Label::new(text(hint, 13.5, pal.muted)).wrap()); }
            ui.add_space(6.0);
        });
        ui.with_layout(Layout::right_to_left(Align::Center), add);
    });
}

fn divider(ui: &mut egui::Ui, pal: &Pal) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, pal.line);
}

/// iOS-style switch. No animation: one repaint per tap.
fn toggle(ui: &mut egui::Ui, pal: &Pal, on: &mut bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(56.0, 32.0), Sense::click());
    if response.clicked() { *on = !*on; }
    let painter = ui.painter();
    let track = if *on { pal.primary } else { ui::alpha(pal.ink, 60) };
    painter.rect_filled(rect, CornerRadius::same(16), track);
    // The knob slides (0.12 s) instead of jumping.
    let t = ui.ctx().animate_bool_with_time(response.id, *on, 0.12);
    let x = rect.left() + 16.0 + t * (rect.width() - 32.0);
    painter.circle_filled(Pos2::new(x, rect.center().y), 12.5, Color32::WHITE);
    response.clicked()
}

/// − value + : large targets, no keyboard needed.
fn stepper(ui: &mut egui::Ui, pal: &Pal, value: &mut i64, min: i64, max: i64, step: i64, show: impl Fn(i64) -> String) -> bool {
    let before = *value;
    // Right-to-left layout: "+" first.
    let round = |ui: &mut egui::Ui, label: &str, enabled: bool| {
        ui.add_enabled(enabled, egui::Button::new(strong(label, 20.0, pal.ink))
            .fill(pal.bg).stroke(Stroke::new(1.0_f32, pal.line)).corner_radius(22)
            .min_size(Vec2::new(44.0, 44.0))).clicked()
    };
    if round(ui, "+", *value < max) { *value = (*value + step).min(max); }
    let (rect, _) = ui.allocate_exact_size(Vec2::new(96.0, 44.0), Sense::hover());
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, show(*value), ui::bold(17.0), pal.ink);
    if round(ui, "−", *value > min) { *value = (*value - step).max(min); }
    *value != before
}

fn step_u8(ui: &mut egui::Ui, pal: &Pal, value: &mut u8, range: (u8, u8), step: u8, show: impl Fn(i64) -> String) {
    let mut v = i64::from(*value);
    if stepper(ui, pal, &mut v, range.0.into(), range.1.into(), step.into(), show) { *value = u8::try_from(v).unwrap_or(*value); }
}

fn step_u16(ui: &mut egui::Ui, pal: &Pal, value: &mut u16, range: (u16, u16), step: u16, show: impl Fn(i64) -> String) {
    let mut v = i64::from(*value);
    if stepper(ui, pal, &mut v, range.0.into(), range.1.into(), step.into(), show) { *value = u16::try_from(v).unwrap_or(*value); }
}

fn step_u32(ui: &mut egui::Ui, pal: &Pal, value: &mut u32, range: (u32, u32), step: u32, show: impl Fn(i64) -> String) {
    let mut v = i64::from(*value);
    if stepper(ui, pal, &mut v, range.0.into(), range.1.into(), step.into(), show) { *value = u32::try_from(v).unwrap_or(*value); }
}

/// Segmented control over a few values (right-to-left callers pass them reversed).
fn segmented(ui: &mut egui::Ui, pal: &Pal, value: &mut u8, options: &[(u8, &str)]) {
    // Fixed width: in a right-to-left row a plain layout would reverse the order.
    let width = options.len() as f32 * 52.0 + 4.0;
    egui::Frame::new().fill(pal.bg).corner_radius(12).stroke(Stroke::new(1.0_f32, pal.line))
        .inner_margin(Margin::same(4)).show(ui, |ui| {
            ui.set_max_width(width);
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                for &(option, label) in options {
                    let selected = *value == option;
                    let button = egui::Button::new(strong(label, 15.0, if selected { pal.on_primary } else { pal.ink }))
                        .fill(if selected { pal.primary } else { Color32::TRANSPARENT })
                        .stroke(Stroke::NONE).corner_radius(9).min_size(Vec2::new(48.0, 38.0));
                    if ui.add(button).clicked() { *value = option; }
                }
            });
        });
}


/// − date + by whole days, shown as « sam. 14 nov. 2026 ».
fn date_stepper(ui: &mut egui::Ui, pal: &Pal, value: &mut String) {
    use chrono::Datelike;
    let slot = EventSlot { date: value.clone(), ..EventSlot::default() };
    let date = slot.date();
    let mut days = i64::from(date.num_days_from_ce());
    let show = |d: i64| chrono::NaiveDate::from_num_days_from_ce_opt(i32::try_from(d).unwrap_or(0)).map_or_else(String::new, |d|
        {
            let (day, month) = (boothsoft::i18n::weekday_short(d.weekday().num_days_from_monday() as usize), boothsoft::i18n::month_short(d.month0() as usize));
            tr!(fr: format!("{day} {} {month} {}", d.day(), d.year()), en: format!("{day}, {month} {}, {}", d.day(), d.year()))
        });
    if stepper_wide(ui, pal, &mut days, 160.0, show) {
        if let Some(d) = chrono::NaiveDate::from_num_days_from_ce_opt(i32::try_from(days).unwrap_or(0)) {
            *value = d.format("%Y-%m-%d").to_string();
        }
    }
}

/// − HH:MM + by 30 minutes, wrapping around midnight.
fn time_stepper(ui: &mut egui::Ui, pal: &Pal, value: &mut String, default_hour: u32) {
    let parsed = chrono::NaiveTime::parse_from_str(value, "%H:%M")
        .unwrap_or_else(|_| chrono::NaiveTime::from_hms_opt(default_hour, 0, 0).expect("hour"));
    let mut minutes = i64::from(chrono::Timelike::num_seconds_from_midnight(&parsed) / 60);
    if stepper_wide(ui, pal, &mut minutes, 64.0, |m| format!("{:02}:{:02}", m.rem_euclid(1440) / 60, m.rem_euclid(1440) % 60)) {
        let m = minutes.rem_euclid(1440);
        *value = format!("{:02}:{:02}", m / 60, m % 60);
    }
}

/// Unbounded stepper (dates, times) with a configurable value width.
fn stepper_wide(ui: &mut egui::Ui, pal: &Pal, value: &mut i64, width: f32, show: impl Fn(i64) -> String) -> bool {
    let before = *value;
    let step = if width < 100.0 { 30 } else { 1 };
    let round = |ui: &mut egui::Ui, label: &str| {
        ui.add(egui::Button::new(strong(label, 20.0, pal.ink)).fill(pal.bg).stroke(Stroke::new(1.0_f32, pal.line))
            .corner_radius(22).min_size(Vec2::new(44.0, 44.0))).clicked()
    };
    if round(ui, "+") { *value += step; }
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 44.0), Sense::hover());
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, show(*value), ui::bold(16.0), pal.ink);
    if round(ui, "−") { *value -= step; }
    *value != before
}

/// Rounded progress bar.
fn progress(ui: &mut egui::Ui, pal: &Pal, fraction: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 12.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(6), ui::alpha(pal.ink, 20));
    let mut filled = rect;
    filled.set_width(rect.width() * fraction.clamp(0.0, 1.0));
    if filled.width() > 0.5 { ui.painter().rect_filled(filled, CornerRadius::same(6), color); }
}

/// Status pill: coloured dot + label.
fn pill(ui: &mut egui::Ui, pal: &Pal, label: &str, ok: bool) {
    egui::Frame::new().fill(ui::alpha(if ok { pal.ok } else { pal.warn }, 26)).corner_radius(255)
        .inner_margin(Margin::symmetric(12, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 7.0;
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.0, if ok { pal.ok } else { pal.warn });
                ui.label(strong(label, 13.5, pal.ink));
            });
        });
}

/// Backspace key: a tag shape with a cross (the font has no ⌫ glyph).
fn backspace_glyph(painter: &egui::Painter, center: Pos2, color: Color32) {
    let stroke = Stroke::new(2.5_f32, color);
    let (w, h) = (30.0, 22.0);
    let left = center.x - w / 2.0;
    let points = vec![
        Pos2::new(left, center.y), Pos2::new(left + 9.0, center.y - h / 2.0), Pos2::new(left + w, center.y - h / 2.0),
        Pos2::new(left + w, center.y + h / 2.0), Pos2::new(left + 9.0, center.y + h / 2.0),
    ];
    painter.add(egui::Shape::closed_line(points, stroke));
    let c = Pos2::new(left + 19.0, center.y);
    painter.line_segment([c + Vec2::new(-5.0, -5.0), c + Vec2::new(5.0, 5.0)], stroke);
    painter.line_segment([c + Vec2::new(-5.0, 5.0), c + Vec2::new(5.0, -5.0)], stroke);
}

/// Centered dialog over a dimmed screen, in a foreground layer so nothing
/// underneath can be touched. Returns true when the dim area was tapped.
fn modal(ctx: &egui::Context, id: &str, full: Rect, pal: &Pal, width: f32,
         add: impl FnOnce(&mut egui::Ui)) -> bool {
    let enter = ui::appear(ctx, ("modal", id), 0.2);
    egui::Area::new(egui::Id::new(("modal-area", id))).order(egui::Order::Foreground)
        .fixed_pos(full.min).movable(false).show(ctx, |ui| -> bool {
            let backdrop = ui.allocate_rect(full, Sense::click());
            ui.painter().rect_filled(full, 0.0, Color32::from_black_alpha((140.0 * enter) as u8));
            let card_rect = Rect::from_center_size(full.center() + Vec2::new(0.0, (1.0 - enter) * 40.0), Vec2::new(width, 10.0));
            let mut card = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(card_rect.min - Vec2::new(0.0, 160.0), Vec2::new(width, full.height())))
                .layout(Layout::top_down(Align::Min)));
            let inner = egui::Frame::new().fill(pal.surface).corner_radius(22).stroke(Stroke::new(1.0_f32, pal.line))
                .inner_margin(Margin::same(28)).show(&mut card, |ui| {
                    ui.set_width(width - 56.0);
                    ui.spacing_mut().item_spacing.y = 8.0;
                    add(ui);
                });
            backdrop.clicked() && backdrop.interact_pointer_pos().is_some_and(|p| !inner.response.rect.contains(p))
        }).inner
}

fn hex(color: &str) -> Color32 {
    let part = |start: usize| color.get(start..start + 2).and_then(|v| u8::from_str_radix(v, 16).ok()).unwrap_or(0);
    Color32::from_rgb(part(1), part(3), part(5))
}

fn logo_texture(ctx: &egui::Context, name: &str, path: &std::path::Path, max: (u32, u32)) -> Option<TextureHandle> {
    let image = image::open(path).ok()?.thumbnail(max.0, max.1).to_rgba8();
    let pixels = ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize], image.as_raw());
    Some(ctx.load_texture(name, pixels, TextureOptions::LINEAR))
}

impl Kiosk {
    fn refresh_theme_catalog(&mut self) {
        self.themes_loaded = true;
        self.theme_thumbs = None;
        match theme_config::discover_all(&self.themes_root) {
            Ok(catalog) => {
                self.installed_themes = catalog.installed;
                let current = theme::active().map(|theme| theme.id.clone());
                if !self.installed_themes.iter().any(|theme| theme.id == self.theme_choice) {
                    self.theme_choice = current.unwrap_or_else(|| "default".to_owned());
                }
                self.theme_status = if catalog.rejected == 0 {
                    tr!(fr: format!("{} thème(s) installé(s)", self.installed_themes.len()), en: format!("{} theme(s) installed", self.installed_themes.len()))
                } else {
                    tr!(fr: format!("{} thème(s) disponible(s) ; {} invalide(s) ignoré(s)",
                        self.installed_themes.len(), catalog.rejected), en: format!("{} theme(s) available; {} invalid one(s) skipped",
                        self.installed_themes.len(), catalog.rejected))
                };
            }
            Err(error) => {
                self.installed_themes.clear();
                self.theme_status = tr!(fr: format!("Lecture des thèmes impossible : {error}"), en: format!("Cannot read the themes: {error}"));
            }
        }
    }

    /// Persist one change on top of the *saved* settings, leaving other pending
    /// edits unsaved (theme and PIN apply immediately, the rest waits for Save).
    fn save_only(&mut self, apply: impl Fn(&mut Settings)) -> std::io::Result<()> {
        let mut base = self.admin_saved.clone().unwrap_or_else(|| self.storage.settings.clone());
        apply(&mut base);
        let edited = std::mem::replace(&mut self.storage.settings, base.clone());
        let result = self.storage.save_settings();
        self.storage.settings = edited;
        if result.is_ok() {
            apply(&mut self.storage.settings);
            if self.admin_saved.is_some() { self.admin_saved = Some(base); }
        }
        result
    }

    fn apply_selected_theme(&mut self, ctx: &egui::Context) {
        // Validate again at application time: installed files can change after discovery.
        let selected = match theme_config::load_any(&self.themes_root, &self.theme_choice) {
            Ok(selected) => selected,
            Err(error) => {
                self.refresh_theme_catalog();
                self.theme_status = tr!(fr: format!("Impossible d'appliquer le thème : {error}"), en: format!("Cannot apply the theme: {error}"));
                return;
            }
        };
        let id = selected.id.clone();
        let frame = selected.frame.to_string_lossy().into_owned();
        if let Err(error) = self.save_only(|s| { s.theme_id.clone_from(&id); s.frame_path.clone_from(&frame); }) {
            self.theme_status = tr!(fr: format!("Sélection non enregistrée : {error}"), en: format!("Choice not saved: {error}"));
            return;
        }
        theme::replace_active(selected);
        theme::install(ctx);
        self.admin_logo = None;
        self.art = None;
        self.frame_path_loaded.clear();
        self.reload_frame();
        self.theme_status = tr!(fr: format!("Thème « {} » appliqué", self.theme_choice), en: format!("Theme “{}” applied", self.theme_choice));
        ctx.request_repaint();
    }

    pub(super) fn process_theme_import(&mut self) {
        let Some(rx) = self.theme_import_rx.as_ref() else { return; };
        match rx.try_recv() {
            Ok(Ok(id)) => {
                self.theme_import_rx = None;
                self.refresh_theme_catalog();
                self.theme_choice = id.clone();
                self.theme_status = tr!(fr: format!("Thème « {id} » importé. Touchez Appliquer pour l'activer."), en: format!("Theme “{id}” imported. Tap Apply to use it."));
            }
            Ok(Err(error)) => {
                self.theme_import_rx = None;
                self.theme_status = tr!(fr: format!("Import refusé : {error}"), en: format!("Import refused: {error}"));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.theme_import_rx = None;
                self.theme_status = tr!(fr: "L'import du thème s'est interrompu", en: "The theme import was interrupted").into();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    fn begin_theme_import(&mut self, ctx: &egui::Context) {
        if self.theme_import_rx.is_some() { return; }
        // macOS requires the native dialog on the application thread.
        let path = rfd::FileDialog::new()
            .set_title(tr!(fr: "Importer un thème BoothSoft (ZIP)", en: "Import a BoothSoft theme (ZIP)"))
            .add_filter(tr!(fr: "Archive ZIP", en: "ZIP archive"), &["zip"])
            .pick_file();
        let Some(path) = path else { return; };
        let installed_root = self.themes_root.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let wake = ctx.clone();
        match std::thread::Builder::new().name("theme-import".into()).spawn(move || {
            let result = theme_import::import_zip(&installed_root, &path);
            let _ = tx.send(result);
            wake.request_repaint();
        }) {
            Ok(_) => {
                self.theme_import_rx = Some(rx);
                self.theme_status = tr!(fr: "Vérification et installation du thème…", en: "Checking and installing the theme…").into();
            }
            Err(error) => self.theme_status = tr!(fr: format!("Import impossible : {error}"), en: format!("Import failed: {error}")),
        }
    }

    fn refresh_devices(&mut self, ctx: &egui::Context) {
        self.device_status = tr!(fr: "Recherche des appareils…", en: "Looking for devices…").into();
        self.device_scan = Some(devices::scan(ctx.clone()));
        self.devices_scanned = true;
    }

    fn process_device_scan(&mut self) {
        let Some(scan) = &self.device_scan else { return; };
        match scan.try_recv() {
            Ok(result) => {
                self.cameras = result.cameras;
                self.printers = result.printers;
                self.system_printer = result.default_printer
                    .or_else(|| (self.printers.len() == 1).then(|| self.printers[0].clone()));
                self.device_status = tr!(fr: format!("{} caméra(s), {} imprimante(s) détectée(s)",
                    self.cameras.len(), self.printers.len()), en: format!("{} camera(s), {} printer(s) found",
                    self.cameras.len(), self.printers.len()));
                if let Some(error) = result.camera_error {
                    self.device_status.push_str(&tr!(fr: format!(" · Caméra : {error}"), en: format!(" · Camera: {error}")));
                }
                if let Some(error) = result.printer_error {
                    self.device_status.push_str(&tr!(fr: format!(" · Imprimante : {error}"), en: format!(" · Printer: {error}")));
                }
                self.device_scan = None;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.device_status = tr!(fr: "La recherche d'appareils a échoué", en: "The device search failed").into();
                self.device_scan = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    // ------------------------------------------------------------- state helpers

    fn admin_dirty(&self) -> bool {
        !self.unsaved().is_empty()
    }

    /// Saved number of prints for the event whose quota is being edited.
    fn saved_quota(&self) -> u32 {
        self.print_ledger.events.get(&self.quota_draft_event).map_or(0, |quota| quota.included)
    }

    /// Readable list of everything that would be lost by leaving now.
    pub(super) fn unsaved(&self) -> Vec<&'static str> {
        let mut items = Vec::new();
        if let Some(saved) = &self.admin_saved {
            let current = &self.storage.settings;
            if saved.events != current.events || saved.event_name != current.event_name {
                items.push(tr!(fr: "Planning des événements", en: "Event schedule"));
            }
            let mut others = current.clone();
            others.events.clone_from(&saved.events);
            others.event_name.clone_from(&saved.event_name);
            if others != *saved { items.push(tr!(fr: "Réglages", en: "Settings")); }
        }
        if self.admin_authorized && !self.quota_draft_event.is_empty() && self.quota_draft != self.saved_quota() {
            items.push(tr!(fr: "Tirages prévus", en: "Planned prints"));
        }
        items
    }

    /// Run `action` now, or ask first if something is unsaved.
    fn request_leave(&mut self, ctx: &egui::Context, action: PendingLeave) {
        if action == PendingLeave::Discard || self.admin_dirty() {
            self.pending_leave = Some(action);
        } else {
            self.perform_leave(ctx, action);
        }
    }

    fn perform_leave(&mut self, ctx: &egui::Context, action: PendingLeave) {
        self.pending_leave = None;
        match action {
            PendingLeave::BackToPhotos => self.leave_admin(),
            PendingLeave::CloseApp => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            PendingLeave::Lock => {
                self.admin_authorized = false;
                self.admin_saved = None;
                self.pin_entry.clear();
                self.status.clear();
            }
            PendingLeave::Discard => {}
        }
    }

    /// Save everything listed by `unsaved`. False (and an error status) if anything failed.
    fn save_everything(&mut self, ctx: &egui::Context) -> bool {
        if self.admin_saved.as_ref().is_some_and(|saved| *saved != self.storage.settings) && !self.save_admin_settings(ctx) {
            return false;
        }
        if self.admin_authorized && !self.quota_draft_event.is_empty() && self.quota_draft != self.saved_quota() {
            let event = self.quota_draft_event.clone();
            let outstanding = self.print_jobs.outstanding(&event);
            if let Err(error) = self.print_ledger.update_included(&event, self.quota_draft, outstanding, &self.storage.root.join("print-ledger.json")) {
                self.status = tr!(fr: format!("Tirages refusés : {error}"), en: format!("Prints refused: {error}"));
                return false;
            }
        }
        true
    }

    fn discard_everything(&mut self) {
        if let Some(saved) = self.admin_saved.clone() { self.storage.settings = saved; }
        self.quota_draft = self.saved_quota();
        self.pending_event_delete = None;
        self.status = tr!(fr: "Modifications annulées", en: "Changes discarded").into();
    }

    fn unlock_admin(&mut self) {
        boothsoft::log::info("Maintenance unlocked");
        self.admin_authorized = true;
        self.admin_saved = Some(self.storage.settings.clone());
        self.admin_section = AdminSection::Overview;
    }

    /// Printer that will actually receive prints, for display (no subprocess).
    fn effective_printer(&self) -> Option<String> {
        let chosen = self.storage.settings.printer_name.trim();
        if chosen.is_empty() { self.system_printer.clone() } else { Some(chosen.to_owned()) }
    }

    fn camera_label(&self) -> String {
        if self.storage.settings.demo_mode { return tr!(fr: "Mode démo", en: "Demo mode").into(); }
        self.cameras.iter().find(|c| c.index == self.storage.settings.camera_index)
            .map_or_else(|| tr!(fr: format!("Caméra n°{} (non détectée)", self.storage.settings.camera_index), en: format!("Camera #{} (not found)", self.storage.settings.camera_index)), |c| c.name.clone())
    }

    fn save_admin_settings(&mut self, ctx: &egui::Context) -> bool {
        let previous = self.admin_saved.clone();
        self.storage.settings = self.storage.settings.clone().normalized();
        let name = self.storage.settings.printer_name.trim().to_owned();
        if !name.is_empty() && (!printer::valid_printer_name(&name) || (!self.printers.is_empty() && !self.printers.contains(&name))) {
            self.status = tr!(fr: "Imprimante introuvable : choisissez-en une détectée ou « Par défaut du système ».", en: "Printer not found: pick a detected one or “System default”.").into();
            return false;
        }
        match self.storage.save_settings() {
            Ok(()) => {
                self.status = tr!(fr: "Paramètres enregistrés", en: "Settings saved").into();
                self.frame_path_loaded.clear();
                self.preview_filtered = None;
                self.system_printer = self.system_printer.take().or_else(printer::default_printer);
                if previous.as_ref().is_some_and(|p| p.fullscreen != self.storage.settings.fullscreen) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.storage.settings.fullscreen));
                }
                self.admin_saved = Some(self.storage.settings.clone());
                true
            }
            Err(error) => { self.status = tr!(fr: format!("Erreur d'enregistrement : {error}"), en: format!("Save error: {error}")); false }
        }
    }

    /// Back to the guests. Only called once nothing is left unsaved.
    fn leave_admin(&mut self) {
        if !self.storage.settings.demo_mode {
            self.camera.send(CameraCommand::Start { index: self.storage.settings.camera_index });
            self.camera.send(CameraCommand::SetFrameRate(self.storage.settings.live_fps));
        }
        self.admin_authorized = false;
        self.admin_saved = None;
        self.gb_admin = GuestBookAdmin::default();
        self.pin_entry.clear();
        self.new_pin.clear();
        self.enter_ready();
    }

    fn submit_pin(&mut self) {
        let setup = self.storage.settings.admin_pin_hash.is_empty();
        if setup {
            if valid_pin(&self.pin_entry) {
                match bcrypt::hash(&self.pin_entry, bcrypt::DEFAULT_COST) {
                    Ok(hash) => {
                        self.storage.settings.admin_pin_hash = hash;
                        if self.storage.save_settings().is_ok() {
                            self.unlock_admin();
                        } else {
                            self.storage.settings.admin_pin_hash.clear();
                            self.status = tr!(fr: "Impossible d'enregistrer le code", en: "Could not save the code").into();
                        }
                    }
                    Err(error) => self.status = error.to_string(),
                }
            } else { self.status = tr!(fr: "Le code doit comporter 6 chiffres", en: "The code must have 6 digits").into(); }
        } else if bcrypt::verify(&self.pin_entry, &self.storage.settings.admin_pin_hash).unwrap_or(false) {
            self.failed_pin_attempts = 0;
            self.pin_blocked_until = None;
            self.status.clear();
            self.unlock_admin();
        } else {
            self.failed_pin_attempts = self.failed_pin_attempts.saturating_add(1);
            boothsoft::log::info(format!("Maintenance: wrong PIN ({} in a row)", self.failed_pin_attempts));
            self.status = tr!(fr: "Code incorrect", en: "Wrong code").into();
            if self.failed_pin_attempts >= 5 {
                self.pin_blocked_until = Some(Instant::now() + Duration::from_secs(60));
                self.failed_pin_attempts = 0;
            }
        }
        self.pin_entry.clear();
    }

    // ------------------------------------------------------------------ layout

    pub(super) fn render_admin(&mut self, ui: &mut egui::Ui) {
        if !self.themes_loaded { self.refresh_theme_catalog(); }
        if !self.devices_scanned { self.refresh_devices(ui.ctx()); }
        self.process_device_scan();
        if self.admin_logo.is_none() {
            self.admin_logo = theme::active().and_then(|t| logo_texture(ui.ctx(), "boothsoft-admin-logo", &t.logo, (420, 140)));
        }
        let pal = Pal::current();
        {
            let style = ui.style_mut();
            if pal.dark { style.visuals = egui::Visuals::dark(); }
            style.visuals.override_text_color = Some(pal.ink);
            style.visuals.window_fill = pal.surface;
            style.visuals.panel_fill = pal.bg;
            style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.line);
            style.spacing.item_spacing = Vec2::new(10.0, 10.0);
            style.spacing.interact_size.y = 44.0;
            style.spacing.button_padding = Vec2::new(16.0, 10.0);
            style.visuals.selection.bg_fill = ui::alpha(pal.primary, 50);
            style.visuals.widgets.inactive.bg_fill = pal.surface;
            style.visuals.widgets.inactive.weak_bg_fill = pal.surface;
            style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, ui::alpha(pal.ink, 60));
            style.visuals.widgets.hovered.weak_bg_fill = pal.bg;
            style.visuals.extreme_bg_color = pal.surface;
            style.visuals.widgets.hovered.bg_fill = pal.bg;
            style.visuals.widgets.active.weak_bg_fill = ui::alpha(pal.primary, 40);
        }
        let full = ui.max_rect();
        let painter = ui.painter().clone();
        painter.rect_filled(full, 0.0, pal.bg);

        let top = Rect::from_min_size(full.min, Vec2::new(full.width(), 80.0));
        painter.rect_filled(top, 0.0, pal.surface);
        // Theme signature: a band of the accent colour under the top bar.
        painter.rect_filled(Rect::from_min_max(Pos2::new(top.left(), top.bottom() - 4.0), top.max), 0.0, pal.accent);
        let mut bar = ui.new_child(UiBuilder::new().max_rect(top.shrink2(Vec2::new(28.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
        self.render_top_bar(&mut bar, &pal);

        let body = Rect::from_min_max(Pos2::new(full.left(), top.bottom() + 1.0), full.max);
        if !self.admin_authorized {
            // Locked: the guest artwork behind the keypad.
            let guest = Palette::current();
            self.paint_backdrop(&painter, body, &guest, 120);
            let mut pin = ui.new_child(UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Center)));
            self.render_pin(&mut pin, &pal);
            return;
        }

        let side = Rect::from_min_size(body.min, Vec2::new(250.0, body.height()));
        painter.rect_filled(side, 0.0, pal.side);
        let mut nav = ui.new_child(UiBuilder::new().max_rect(side.shrink2(Vec2::new(14.0, 18.0))).layout(Layout::top_down(Align::Min)));
        self.render_sidebar(&mut nav, &pal);

        let dirty = self.admin_dirty();
        let footer_h = if dirty { 76.0 } else { 0.0 };
        let content = Rect::from_min_max(Pos2::new(side.right() + 1.0, body.top()), Pos2::new(body.right(), body.bottom() - footer_h));
        let mut page = ui.new_child(UiBuilder::new().max_rect(content).layout(Layout::top_down(Align::Min)));
        egui::ScrollArea::vertical().id_salt(("admin-page", self.admin_section as u8)).auto_shrink(false).show(&mut page, |ui| {
            let margin = ((ui.available_width() - 932.0) / 2.0).clamp(36.0, 120.0);
            egui::Frame::new().inner_margin(Margin { left: margin as i8, right: 36, top: 30, bottom: 40 }).show(ui, |ui| {
                ui.set_max_width(860.0);
                let section = self.admin_section;
                // Each page settles into place when its section is opened.
                let enter = ui::appear(ui.ctx(), ("admin-page", section as u8), 0.2);
                ui.add_space((1.0 - enter) * 18.0);
                ui.label(RichText::new(section.title()).font(ui::display(34.0)).color(if pal.dark { pal.primary } else { pal.ink }));
                ui.label(text(section.subtitle(), 16.0, pal.muted));
                ui.add_space(14.0);
                match section {
                    AdminSection::Overview => self.page_overview(ui, &pal),
                    AdminSection::Event => self.page_event(ui, &pal),
                    AdminSection::Printing => self.page_printing(ui, &pal),
                    AdminSection::Camera => self.page_camera(ui, &pal),
                    AdminSection::Theme => self.page_theme(ui, &pal),
                    AdminSection::Security => self.page_security(ui, &pal),
                    AdminSection::Data => self.page_data(ui, &pal),
                    AdminSection::GuestBook => self.page_guestbook(ui, &pal),
                }
            });
        });

        if dirty {
            let footer = Rect::from_min_max(Pos2::new(side.right() + 1.0, content.bottom()), body.max);
            // Same dark band as the menu, readable in light and dark themes.
            painter.rect_filled(footer, 0.0, pal.side);
            painter.hline(footer.x_range(), footer.top(), Stroke::new(3.0_f32, pal.accent));
            let mut bar = ui.new_child(UiBuilder::new().max_rect(footer.shrink2(Vec2::new(32.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
            bar.label(strong(tr!(fr: "Modifications non enregistrées", en: "Unsaved changes"), 16.0, pal.side_ink));
            bar.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let save = egui::Button::new(strong(tr!(fr: "Enregistrer", en: "Save"), 15.0, Color32::from_gray(25))).fill(Color32::WHITE)
                    .corner_radius(10).min_size(Vec2::new(140.0, 44.0));
                if ui.add(save).clicked() && self.save_everything(ui.ctx()) { self.status = tr!(fr: "Modifications enregistrées", en: "Changes saved").into(); }
                let cancel = egui::Button::new(strong(tr!(fr: "Annuler", en: "Cancel"), 15.0, pal.side_ink)).fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0_f32, ui::alpha(pal.side_ink, 110))).corner_radius(10).min_size(Vec2::new(110.0, 44.0));
                if ui.add(cancel).clicked() { self.request_leave(ui.ctx(), PendingLeave::Discard); }
            });
        }
        self.render_leave_dialog(ui.ctx(), full, &pal);
        self.render_delete_event_dialog(ui.ctx(), full, &pal);
        self.render_message_dialogs(ui.ctx(), full, &pal);
        // Microphone test and playback live only on their own page.
        if self.admin_section != AdminSection::Event { self.gb_admin.test_mic = None; }
        if self.admin_section != AdminSection::GuestBook { self.gb_admin.playing = None; }
        if self.gb_admin.test_mic.is_some() || self.gb_admin.playing.is_some() || self.gb_admin.export.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(66));
        }
    }

    /// Modal on top of everything: « Quitter sans enregistrer ? ».
    fn render_leave_dialog(&mut self, ctx: &egui::Context, full: Rect, pal: &Pal) {
        let Some(action) = self.pending_leave else { return; };
        let items = self.unsaved();
        let (title, save_label, discard_label) = action.wording();
        let mut choice: Option<u8> = None;
        let outside = modal(ctx, "leave-dialog", full, pal, 560.0, |ui| {
            ui.label(RichText::new(title).font(ui::display(28.0)).color(pal.ink));
            ui.add_space(6.0);
            if action != PendingLeave::Discard {
                ui.label(text(tr!(fr: "Vous avez des modifications non enregistrées :", en: "You have unsaved changes:"), 16.0, pal.muted));
            }
            for item in items.iter().take(5) {
                ui.label(strong(format!("•  {item}"), 16.0, pal.ink));
            }
            if items.len() > 5 { ui.label(text(tr!(fr: format!("et {} autres", items.len() - 5), en: format!("and {} more", items.len() - 5)), 15.0, pal.muted)); }
            ui.add_space(14.0);
            let wide = |ui: &mut egui::Ui, label: &str, fill: Color32, ink: Color32, stroke: Stroke| {
                ui.add(egui::Button::new(strong(label, 17.0, ink)).fill(fill).stroke(stroke).corner_radius(12)
                    .min_size(Vec2::new(ui.available_width(), 58.0))).clicked()
            };
            if !save_label.is_empty() && wide(ui, save_label, pal.primary, pal.on_primary, Stroke::NONE) { choice = Some(0); }
            if wide(ui, discard_label, pal.surface, pal.warn, Stroke::new(1.5_f32, pal.warn)) { choice = Some(1); }
            let stay = if action == PendingLeave::Discard { tr!(fr: "Continuer à modifier", en: "Keep editing") } else { tr!(fr: "Rester sur cette page", en: "Stay on this page") };
            if wide(ui, stay, pal.surface, pal.ink, Stroke::new(1.0_f32, ui::alpha(pal.ink, 60))) { choice = Some(2); }
        });
        if outside { choice = choice.or(Some(2)); }
        match choice {
            Some(0) => {
                if self.save_everything(ctx) { self.perform_leave(ctx, action); } else { self.pending_leave = None; }
            }
            Some(1) => { self.discard_everything(); self.perform_leave(ctx, action); }
            Some(_) => self.pending_leave = None,
            None => {}
        }
    }

    fn render_delete_event_dialog(&mut self, ctx: &egui::Context, full: Rect, pal: &Pal) {
        let Some(index) = self.pending_event_delete else { return; };
        let Some(name) = self.storage.settings.events.get(index).map(|e| e.name.clone()) else {
            self.pending_event_delete = None;
            return;
        };
        let mut choice: Option<bool> = None;
        let outside = modal(ctx, "delete-event", full, pal, 520.0, |ui| {
            ui.label(RichText::new(tr!(fr: "Supprimer cet événement ?", en: "Delete this event?")).font(ui::display(28.0)).color(pal.ink));
            ui.add_space(4.0);
            ui.label(text(tr!(fr: format!("« {name} » sera retiré du planning. Ses photos déjà prises restent sur le disque."), en: format!("“{name}” will be removed from the schedule. Its photos stay on the disk.")), 16.0, pal.muted));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(strong(tr!(fr: "Annuler", en: "Cancel"), 16.0, pal.ink)).fill(pal.surface)
                    .stroke(Stroke::new(1.0_f32, ui::alpha(pal.ink, 60))).corner_radius(12).min_size(Vec2::new(150.0, 54.0))).clicked() { choice = Some(false); }
                if ui.add(egui::Button::new(strong(tr!(fr: "Supprimer", en: "Delete"), 16.0, Color32::WHITE)).fill(pal.warn)
                    .corner_radius(12).min_size(Vec2::new(170.0, 54.0))).clicked() { choice = Some(true); }
            });
        });
        if outside { choice = choice.or(Some(false)); }
        match choice {
            Some(true) => {
                if index < self.storage.settings.events.len() { self.storage.settings.events.remove(index); }
                self.pending_event_delete = None;
            }
            Some(false) => self.pending_event_delete = None,
            None => {}
        }
    }

    fn render_top_bar(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        if let Some(logo) = &self.admin_logo {
            let size = logo.size_vec2();
            let height = 44.0;
            ui.add(egui::Image::new(logo).fit_to_exact_size(Vec2::new(size.x / size.y.max(1.0) * height, height).min(Vec2::new(170.0, height))));
            let (sep, _) = ui.allocate_exact_size(Vec2::new(1.0, 36.0), Sense::hover());
            ui.painter().rect_filled(sep, 0.0, pal.line);
        }
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(4.0);
            ui.label(RichText::new("Maintenance").font(ui::display(22.0)).color(pal.primary));
            // Worker statuses ("Camera…", "Photo…") are diagnostics, shown on the Données page.
            let technical = ["Data:", "Camera", "Photo", "Print job", "Job ", "Session", "Storage"]
                .iter().any(|prefix| self.status.starts_with(prefix));
            let line = if self.status.is_empty() || technical {
                self.event_label().map_or_else(|| "BoothSoft".to_owned(), |e| tr!(fr: format!("Événement : {e}"), en: format!("Event: {e}")))
            } else { self.status.clone() };
            ui.add(egui::Label::new(text(line, 13.5, pal.muted)).truncate());
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let back = egui::Button::new(strong(tr!(fr: "Retour aux photos  →", en: "Back to photos  →"), 15.0, pal.on_primary)).fill(pal.primary)
                .corner_radius(22).min_size(Vec2::new(0.0, 44.0));
            if ui.add(back).clicked() { self.request_leave(ui.ctx(), PendingLeave::BackToPhotos); }
            // Full-screen kiosk has no window controls: closing is an operator action.
            if self.admin_authorized && btn(ui, pal, tr!(fr: "Fermer BoothSoft", en: "Close BoothSoft"), Btn::Secondary).clicked() {
                self.request_leave(ui.ctx(), PendingLeave::CloseApp);
            }
            if self.admin_authorized && ui.available_width() > 520.0 {
                ui.add_space(8.0);
                let printing = self.storage.settings.print_enabled;
                let printer_ok = !printing || self.effective_printer().is_some();
                pill(ui, pal, if !printing { tr!(fr: "Impression désactivée", en: "Printing off") } else if printer_ok { tr!(fr: "Imprimante prête", en: "Printer ready") } else { tr!(fr: "Pas d'imprimante", en: "No printer") }, printer_ok);
                let camera_ok = self.storage.settings.demo_mode || self.cameras.iter().any(|c| c.index == self.storage.settings.camera_index);
                pill(ui, pal, if self.storage.settings.demo_mode { tr!(fr: "Mode démo", en: "Demo mode") } else if camera_ok { tr!(fr: "Caméra prête", en: "Camera ready") } else { tr!(fr: "Pas de caméra", en: "No camera") }, camera_ok);
            }
        });
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        ui.spacing_mut().item_spacing.y = 4.0;
        let printer_missing = self.storage.settings.print_enabled && self.effective_printer().is_none() && self.device_scan.is_none();
        let show_guestbook = self.storage.settings.guestbook_enabled
            || !boothsoft::guestbook::event_folders(&self.storage.root.join("sessions")).is_empty();
        for section in AdminSection::ALL {
            if section == AdminSection::GuestBook && !show_guestbook { continue; }
            let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::click());
            let selected = self.admin_section == section;
            let painter = ui.painter();
            if selected {
                // The highlight glides from the previous section.
                let y = ui::glide(ui.ctx(), "admin-nav-y", rect.top());
                let pill = Rect::from_min_size(Pos2::new(rect.left(), y), rect.size());
                painter.rect_filled(pill, CornerRadius::same(12), ui::alpha(pal.side_ink, 34));
                painter.rect_filled(Rect::from_min_size(pill.left_top() + Vec2::new(0.0, 11.0), Vec2::new(4.0, 24.0)), CornerRadius::same(2), pal.side_ink);
            } else if response.hovered() {
                painter.rect_filled(rect, CornerRadius::same(12), ui::alpha(pal.side_ink, 14));
            }
            let font = if selected { ui::bold(16.0) } else { ui::body(16.0) };
            painter.text(rect.left_center() + Vec2::new(18.0, 0.0), Align2::LEFT_CENTER, section.title(), font,
                if selected { pal.side_ink } else { ui::alpha(pal.side_ink, 190) });
            if section == AdminSection::Printing && printer_missing {
                painter.circle_filled(Pos2::new(rect.right() - 18.0, rect.center().y), 5.0, pal.warn);
            }
            if response.clicked() { self.admin_section = section; }
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let faint = ui::alpha(pal.side_ink, 140);
            ui.label(text(if self.software_renderer() { tr!(fr: "Rendu logiciel (sans GPU)", en: "Software rendering (no GPU)") } else { tr!(fr: "Rendu GPU", en: "GPU rendering") }, 12.5, faint));
            ui.label(text(format!("{} · v{}", boothsoft::ATTRIBUTION, env!("CARGO_PKG_VERSION")), 12.5, faint));
            if let Some(active) = theme::active() {
                ui.label(strong(tr!(fr: format!("Thème {}", active.config.name), en: format!("Theme {}", active.config.name)), 13.5, pal.side_ink));
            }
        });
    }

    // --------------------------------------------------------------------- PIN

    fn render_pin(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        let setup = self.storage.settings.admin_pin_hash.is_empty();
        let locked = self.pin_blocked_until.is_some_and(|until| Instant::now() < until);
        // Physical keyboard too: digits, backspace, enter.
        if !locked {
            let events = ui.input(|i| i.events.clone());
            for event in events {
                match event {
                    egui::Event::Text(t) => for c in t.chars().filter(char::is_ascii_digit) {
                        if self.pin_entry.len() < 6 { self.pin_entry.push(c); }
                    },
                    egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => { self.pin_entry.pop(); }
                    egui::Event::Key { key: egui::Key::Enter, pressed: true, .. } if self.pin_entry.len() == 6 => self.submit_pin(),
                    _ => {}
                }
            }
        }
        let card_w = 380.0;
        let card_h = 600.0;
        ui.add_space(((ui.available_height() - card_h) / 2.0).max(16.0));
        egui::Frame::new().fill(pal.surface).corner_radius(24).stroke(Stroke::new(1.0_f32, pal.line))
            .inner_margin(Margin::symmetric(36, 30)).show(ui, |ui| {
                ui.set_width(card_w - 72.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(if setup { tr!(fr: "Créez votre code", en: "Create your code") } else { tr!(fr: "Code administrateur", en: "Admin code") }).font(ui::display(26.0)).color(pal.ink));
                    ui.label(text(if setup { tr!(fr: "6 chiffres, à garder précieusement.", en: "6 digits, keep them safe.") } else { tr!(fr: "Saisissez les 6 chiffres.", en: "Enter the 6 digits.") }, 15.0, pal.muted));
                    ui.add_space(18.0);
                    // Six dots.
                    let (dots, _) = ui.allocate_exact_size(Vec2::new(6.0 * 34.0, 22.0), Sense::hover());
                    for i in 0..6 {
                        let center = Pos2::new(dots.left() + 17.0 + i as f32 * 34.0, dots.center().y);
                        if i < self.pin_entry.len() {
                            ui.painter().circle_filled(center, 9.0, pal.primary);
                        } else {
                            ui.painter().circle_stroke(center, 9.0, Stroke::new(2.0_f32, ui::alpha(pal.ink, 70)));
                        }
                    }
                    ui.add_space(10.0);
                    let message = if locked {
                        let seconds = self.pin_blocked_until.map_or(0, |u| u.saturating_duration_since(Instant::now()).as_secs());
                        tr!(fr: format!("Trop d'essais. Réessayez dans {seconds} s."), en: format!("Too many tries. Try again in {seconds} s."))
                    } else if ["Code", "Le code", "Impossible", "Wrong code", "The code", "Could not"].iter().any(|p| self.status.starts_with(p)) {
                        self.status.clone()
                    } else { String::new() };
                    ui.label(text(message, 14.0, pal.warn));
                    ui.add_space(10.0);
                    let key = 84.0;
                    let gap = 14.0;
                    let (grid, _) = ui.allocate_exact_size(Vec2::new(3.0 * key + 2.0 * gap, 4.0 * key + 3.0 * gap), Sense::hover());
                    let keys = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "⌫", "0", "OK"];
                    for (i, label) in keys.iter().enumerate() {
                        let (col, line) = ((i % 3) as f32, (i / 3) as f32);
                        let rect = Rect::from_min_size(grid.min + Vec2::new(col * (key + gap), line * (key + gap)), Vec2::splat(key));
                        let ok = *label == "OK";
                        let enabled = !locked && (!ok || self.pin_entry.len() == 6);
                        let response = ui.interact(rect, ui.id().with(("pin-key", i)), if enabled { Sense::click() } else { Sense::hover() });
                        let pressed = response.is_pointer_button_down_on();
                        let fill = if ok {
                            if enabled { pal.primary } else { ui::alpha(pal.primary, 70) }
                        } else if pressed { ui::alpha(pal.ink, 30) } else { pal.bg };
                        ui.painter().rect(rect, CornerRadius::same(255), fill, Stroke::new(1.0_f32, if ok { Color32::TRANSPARENT } else { pal.line }), StrokeKind::Inside);
                        let color = if ok { pal.on_primary } else if enabled { pal.ink } else { pal.muted };
                                if *label == "⌫" {
                            backspace_glyph(ui.painter(), rect.center(), color);
                        } else {
                            let font: FontId = ui::bold(if ok { 20.0 } else { 28.0 });
                            ui.painter().text(rect.center(), Align2::CENTER_CENTER, *label, font, color);
                        }
                        if response.clicked() {
                            match *label {
                                "⌫" => { self.pin_entry.pop(); }
                                "OK" => self.submit_pin(),
                                digit => if self.pin_entry.len() < 6 { self.pin_entry.push_str(digit); },
                            }
                        }
                    }
                });
            });
        // Verify as soon as the 6th digit is in (setup still needs OK to confirm).
        if !setup && !locked && self.pin_entry.len() == 6 { self.submit_pin(); }
    }

    // ------------------------------------------------------------------- pages

    /// Theme banner: guest artwork, event in the theme's display font, logo.
    fn paint_hero(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        let ctx = ui.ctx().clone();
        self.ensure_art(&ctx);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 196.0), Sense::hover());
        let painter = ui.painter();
        let radius = CornerRadius::same(20);
        match self.art.as_ref().and_then(|art| art.background.as_ref()) {
            Some(texture) => {
                let uv = ui::cover_uv(texture.size_vec2(), rect.size());
                painter.add(egui::epaint::RectShape::filled(rect, radius, Color32::WHITE).with_texture(texture.id(), uv));
                painter.rect_filled(rect, radius, ui::alpha(pal.bg, 150));
            }
            None => { painter.rect_filled(rect, radius, pal.primary); }
        }
        let on_art = if self.art.as_ref().is_some_and(|a| a.background.is_some()) { pal.ink } else { pal.on_primary };
        let event = self.current_event();
        let inner = rect.shrink2(Vec2::new(32.0, 28.0));
        let eyebrow = if event.until.is_some() { tr!(fr: "ÉVÉNEMENT EN COURS", en: "CURRENT EVENT") } else { tr!(fr: "ÉVÉNEMENT PAR DÉFAUT", en: "DEFAULT EVENT") };
        painter.text(inner.left_top(), Align2::LEFT_TOP, eyebrow, ui::bold(14.0), pal.accent);
        let name = painter.layout(event.name.clone(), ui::display(40.0), if pal.dark { pal.primary } else { on_art }, inner.width() * 0.6);
        let name_h = name.size().y;
        painter.galley(inner.left_top() + Vec2::new(0.0, 26.0), name, on_art);
        let photos: usize = self.storage.sessions.iter().filter(|s| s.folder == event.folder).map(|s| s.shots.len()).sum();
        let mut detail = tr!(fr: format!("{photos} photo(s)"), en: format!("{photos} photo(s)"));
        if let Some(until) = event.until { detail = tr!(fr: format!("Jusqu'à {} · {detail}", until.format("%H:%M")), en: format!("Until {} · {detail}", until.format("%H:%M"))); }
        match self.prints_left() {
            Some((left, _)) => detail.push_str(&tr!(fr: format!(" · {left} tirage(s) restant(s)"), en: format!(" · {left} print(s) left"))),
            None if self.storage.settings.print_enabled => detail.push_str(tr!(fr: " · tirages illimités", en: " · unlimited prints")),
            None => {}
        }
        painter.text(inner.left_top() + Vec2::new(0.0, 34.0 + name_h), Align2::LEFT_TOP, detail, ui::body(16.0), on_art);
        if let Some(logo) = self.art.as_ref().and_then(|art| art.logo.as_ref()) {
            let area = Rect::from_min_max(Pos2::new(inner.right() - inner.width() * 0.32, inner.top()), inner.max);
            let fitted = ui::fit(logo.size_vec2(), area);
            painter.image(logo.id(), fitted, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        ui.add_space(8.0);
    }

    fn page_overview(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        self.paint_hero(ui, pal);
        let printer = self.effective_printer();
        let printing = self.storage.settings.print_enabled;
        let left = self.prints_left();
        let photos: usize = self.storage.sessions.iter().map(|s| s.shots.len()).sum();
        let camera_ok = self.storage.settings.demo_mode || self.cameras.iter().any(|c| c.index == self.storage.settings.camera_index);
        let tiles: [(&str, String, String, bool, AdminSection); 6] = [
            (tr!(fr: "Caméra", en: "Camera"), self.camera_label(), tr!(fr: format!("Live {} images/s", self.storage.settings.live_fps), en: format!("Live {} fps", self.storage.settings.live_fps)), camera_ok, AdminSection::Camera),
            (tr!(fr: "Imprimante", en: "Printer"), if !printing { tr!(fr: "Impression désactivée", en: "Printing off").into() } else { printer.clone().unwrap_or_else(|| tr!(fr: "Aucune détectée", en: "None found").into()) },
                if self.storage.settings.printer_name.is_empty() { tr!(fr: "Imprimante par défaut du système", en: "System default printer").into() } else { tr!(fr: "Choisie manuellement", en: "Chosen manually").into() },
                !printing || printer.is_some(), AdminSection::Printing),
            (tr!(fr: "Tirages restants", en: "Prints left"), match left { Some((l, m)) => format!("{l} / {m}"), None => tr!(fr: "Illimités", en: "Unlimited").into() },
                tr!(fr: format!("{} en attente", self.print_jobs.outstanding(&self.current_event().name)), en: format!("{} pending", self.print_jobs.outstanding(&self.current_event().name))),
                left.is_none_or(|(l, m)| l * 10 >= m), AdminSection::Printing),
            (tr!(fr: "Événement", en: "Event"), self.event_label().unwrap_or_else(|| tr!(fr: "Sans nom", en: "Unnamed").to_owned()),
                tr!(fr: format!("{} photo(s) par séance", self.storage.settings.shots_per_session), en: format!("{} photo(s) per session", self.storage.settings.shots_per_session)), true, AdminSection::Event),
            (tr!(fr: "Photos prises", en: "Photos taken"), photos.to_string(), tr!(fr: format!("{} séance(s)", self.storage.sessions.len()), en: format!("{} session(s)", self.storage.sessions.len())), true, AdminSection::Data),
            (tr!(fr: "Affichage", en: "Display"), if self.software_renderer() { tr!(fr: "Sans GPU", en: "No GPU").into() } else { "GPU".into() },
                theme::active().map_or_else(|| tr!(fr: "Thème par défaut", en: "Default theme").to_owned(), |t| tr!(fr: format!("Thème {}", t.config.name), en: format!("Theme {}", t.config.name))),
                !self.software_renderer() || self.storage.settings.live_fps <= 20, AdminSection::Theme),
        ];
        let width = ui.available_width();
        let columns = if width > 760.0 { 3 } else { 2 };
        let gap = 14.0;
        let tile_w = (width - gap * (columns as f32 - 1.0)) / columns as f32;
        for chunk in tiles.chunks(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (label, value, hint, ok, target) in chunk {
                    let (rect, response) = ui.allocate_exact_size(Vec2::new(tile_w, 118.0), Sense::click());
                    let painter = ui.painter();
                    painter.rect(rect, CornerRadius::same(16), pal.surface,
                        Stroke::new(1.0_f32, if response.hovered() { ui::alpha(pal.primary, 120) } else { pal.line }), StrokeKind::Inside);
                    let inner = rect.shrink(18.0);
                    painter.circle_filled(Pos2::new(inner.right() - 5.0, inner.top() + 8.0), 5.0, if *ok { pal.ok } else { pal.warn });
                    painter.text(inner.left_top(), Align2::LEFT_TOP, *label, ui::body(14.0), pal.muted);
                    // One line, cut with an ellipsis: a long camera or printer name must not
                    // run over the hint below.
                    let mut job = egui::text::LayoutJob::simple_singleline(value.clone(), ui::bold(21.0), pal.ink);
                    job.wrap = egui::text::TextWrapping { max_width: inner.width() - 16.0, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
                    let value_galley = painter.layout_job(job);
                    painter.galley(inner.left_top() + Vec2::new(0.0, 24.0), value_galley, pal.ink);
                    painter.text(inner.left_bottom(), Align2::LEFT_BOTTOM, hint, ui::body(13.5), pal.muted);
                    if response.clicked() { self.admin_section = *target; }
                }
            });
            ui.add_space(gap - 10.0);
        }
        ui.add_space(10.0);
        if printing && printer.is_none() && self.device_scan.is_none() {
            card(ui, pal, tr!(fr: "Aucune imprimante détectée", en: "No printer found"), tr!(fr: "Branchez et allumez l'imprimante, puis actualisez. Les invités verront « Imprimer » dès qu'elle est prête.", en: "Plug in and switch on the printer, then refresh. Guests will see “Print” as soon as it is ready."), |ui| {
                if btn(ui, pal, tr!(fr: "Actualiser les appareils", en: "Refresh devices"), Btn::Primary).clicked() { self.refresh_devices(ui.ctx()); }
            });
        }
    }

    fn page_event(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        let now = chrono::Local::now().naive_local();
        let current = self.current_event();
        card(ui, pal, "", "", |ui| {
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 6.0, if current.until.is_some() { pal.ok } else { pal.muted });
                ui.label(strong(tr!(fr: format!("En ce moment : {}", current.name), en: format!("Right now: {}", current.name)), 18.0, pal.ink));
            });
            let detail = match current.until {
                Some(until) => tr!(fr: format!("Créneau en cours jusqu'à {}. Photos rangées dans « {} ».", until.format("%H:%M"), current.folder), en: format!("Slot running until {}. Photos saved in “{}”.", until.format("%H:%M"), current.folder)),
                None => tr!(fr: format!("Aucun créneau en cours : événement par défaut. Photos rangées dans « {} ».", current.folder), en: format!("No slot running: default event. Photos saved in “{}”.", current.folder)),
            };
            ui.label(text(detail, 14.0, pal.muted));
        });

        let mut remove = None;
        card(ui, pal, tr!(fr: "Planning", en: "Schedule"), tr!(fr: "Chaque événement a ses horaires : ses photos ont leur propre dossier et leur propre quota de tirages.", en: "Each event has its own hours: its photos get their own folder and print quota."), |ui| {
            let count = self.storage.settings.events.len();
            for index in 0..count {
                if index > 0 { divider(ui, pal); }
                let event = &mut self.storage.settings.events[index];
                let (start, end) = event.window();
                let state = if now >= end { tr!(fr: "Terminé", en: "Over") } else if now >= start { tr!(fr: "En cours", en: "Running") } else { tr!(fr: "À venir", en: "Upcoming") };
                row(ui, pal, tr!(fr: "Nom", en: "Name"), state, |ui| {
                    if btn(ui, pal, tr!(fr: "Supprimer", en: "Delete"), Btn::Danger).clicked() { remove = Some(index); }
                    ui.add(egui::TextEdit::singleline(&mut event.name).font(ui::body(17.0)).desired_width(260.0)
                        .margin(Vec2::new(12.0, 10.0)).text_color(pal.ink));
                });
                row(ui, pal, tr!(fr: "Date", en: "Date"), "", |ui| { date_stepper(ui, pal, &mut event.date); });
                row(ui, pal, tr!(fr: "Horaires", en: "Hours"), if event.end_time() <= event.start_time() { tr!(fr: "Se termine le lendemain.", en: "Ends the next day.") } else { "" }, |ui| {
                    time_stepper(ui, pal, &mut event.end, 2);
                    ui.label(text("→", 18.0, pal.muted));
                    time_stepper(ui, pal, &mut event.start, 14);
                });
            }
            if count == 0 {
                ui.label(text(tr!(fr: "Aucun événement planifié : toutes les photos vont dans l'événement par défaut.", en: "No event planned: every photo goes to the default event."), 14.0, pal.muted));
            }
            ui.add_space(4.0);
            if btn(ui, pal, tr!(fr: "+  Ajouter un événement", en: "+  Add an event"), Btn::Primary).clicked() {
                let mut slot = EventSlot::default();
                // Next free evening by default: after the last planned event.
                if let Some(last) = self.storage.settings.events.iter().map(EventSlot::date).max() {
                    slot.date = (last + chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
                }
                self.storage.settings.events.push(slot);
            }
        });
        if remove.is_some() { self.pending_event_delete = remove; }

        card(ui, pal, tr!(fr: "Événement par défaut", en: "Default event"), tr!(fr: "Utilisé en dehors des créneaux du planning.", en: "Used outside the scheduled slots."), |ui| {
            row(ui, pal, tr!(fr: "Nom", en: "Name"), tr!(fr: "Affiché sur l'écran de veille.", en: "Shown on the sleep screen."), |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.storage.settings.event_name)
                    .font(ui::body(17.0)).desired_width(300.0).margin(Vec2::new(12.0, 10.0)).hint_text(tr!(fr: "Mariage Léa & Hugo", en: "Léa & Hugo's wedding")));
            });
        });

        card(ui, pal, tr!(fr: "Séance photo", en: "Photo session"), tr!(fr: "Compte à rebours de 7 secondes, puis…", en: "7-second countdown, then…"), |ui| {
            row(ui, pal, tr!(fr: "Proposer le mode rafale", en: "Offer burst mode"), tr!(fr: "Sur le live, les invités choisissent « Une photo » ou « Rafale » (« Une photo » est présélectionnée).", en: "On the live view, guests choose “Single photo” or “Burst” (“Single photo” is preselected)."), |ui| {
                toggle(ui, pal, &mut self.storage.settings.burst_enabled);
            });
            if self.storage.settings.burst_enabled {
                divider(ui, pal);
                row(ui, pal, tr!(fr: "Photos par rafale", en: "Photos per burst"), tr!(fr: "Environ une photo par seconde, puis choix des tirages.", en: "About one photo per second, then the print picker."), |ui| {
                    step_u8(ui, pal, &mut self.storage.settings.burst_count, (3, 10), 1, |v| tr!(fr: format!("{v} photos"), en: format!("{v} photos")));
                });
            }
            divider(ui, pal);
            row(ui, pal, tr!(fr: "Photos par séance (mode photo)", en: "Photos per session (photo mode)"), tr!(fr: "1 photo, ou une série validée une à une.", en: "1 photo, or a series approved one by one."), |ui| {
                segmented(ui, pal, &mut self.storage.settings.shots_per_session, &[(1, "1"), (2, "2"), (3, "3"), (4, "4")]);
            });
            divider(ui, pal);
            row(ui, pal, tr!(fr: "Retour automatique au live", en: "Automatic return to live view"), tr!(fr: "Si les invités laissent l'aperçu sans rien toucher.", en: "When guests leave the preview untouched."), |ui| {
                step_u16(ui, pal, &mut self.storage.settings.photo_timeout_seconds, (15, 300), 15, |v| format!("{v} s"));
            });
            divider(ui, pal);
            row(ui, pal, tr!(fr: "Limite de séances", en: "Session limit"), tr!(fr: "Par événement.", en: "Per event."), |ui| {
                step_u32(ui, pal, &mut self.storage.settings.session_limit, (0, 1000), 10,
                    |v| if v == 0 { tr!(fr: "Illimité", en: "Unlimited").into() } else { v.to_string() });
            });
        });
        self.card_guestbook(ui, pal);
    }

    fn page_printing(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        self.card_print_format(ui, pal);
        let printing = self.storage.settings.print_enabled;
        card(ui, pal, "", "", |ui| {
            row(ui, pal, tr!(fr: "Proposer l'impression", en: "Offer printing"), tr!(fr: "Bouton « Imprimer » sur l'aperçu de la photo.", en: "“Print” button on the photo preview."), |ui| {
                toggle(ui, pal, &mut self.storage.settings.print_enabled);
            });
            if !printing { return; }
            divider(ui, pal);
            row(ui, pal, tr!(fr: "Imprimante", en: "Printer"), tr!(fr: "« Par défaut » suit l'imprimante par défaut de l'ordinateur.", en: "“Default” follows the computer's default printer."), |ui| {
                let default_label = tr!(fr: format!("Par défaut du système{}",
                    self.system_printer.as_ref().map_or_else(|| tr!(fr: " (aucune)", en: " (none)").to_owned(), |p| format!(" · {p}"))), en: format!("System default{}",
                    self.system_printer.as_ref().map_or_else(|| tr!(fr: " (aucune)", en: " (none)").to_owned(), |p| format!(" · {p}"))));
                let selected = if self.storage.settings.printer_name.is_empty() { default_label.clone() } else { self.storage.settings.printer_name.clone() };
                egui::ComboBox::from_id_salt("system-printer").width(300.0).height(400.0)
                    .selected_text(text(selected, 15.0, pal.ink))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.storage.settings.printer_name, String::new(), default_label);
                        for name in &self.printers {
                            ui.selectable_value(&mut self.storage.settings.printer_name, name.clone(), name);
                        }
                    });
            });
            ui.horizontal(|ui| {
                let busy = self.device_scan.is_some();
                if btn_enabled(ui, pal, !busy, if busy { tr!(fr: "Recherche…", en: "Searching…") } else { tr!(fr: "Actualiser", en: "Refresh") }, Btn::Secondary) {
                    self.refresh_devices(ui.ctx());
                }
                ui.label(text(&self.device_status, 13.5, pal.muted));
            });
            if self.effective_printer().is_none() && self.device_scan.is_none() {
                ui.label(text(tr!(fr: "Aucune imprimante détectée : les invités verront un message s'ils demandent une impression.", en: "No printer found: guests will see a message if they ask for a print."), 14.0, pal.warn));
            }
            divider(ui, pal);
            row(ui, pal, tr!(fr: "Exemplaires maximum", en: "Maximum copies"), tr!(fr: "Par photo. Au-delà de 3, l'invité confirme son choix.", en: "Per photo. Above 3, the guest confirms."), |ui| {
                step_u8(ui, pal, &mut self.storage.settings.max_copies, (1, 10), 1, |v| v.to_string());
            });
        });
        if !printing { return; }

        let event = self.current_event().name;
        let outstanding = self.print_jobs.outstanding(&event);
        if self.quota_draft_event != event {
            self.quota_draft = self.print_ledger.events.get(&event).map_or(0, |quota| quota.included);
            self.quota_draft_event.clone_from(&event);
        }
        let quota = self.print_ledger.events.get(&event).cloned().unwrap_or_default();
        card(ui, pal, &tr!(fr: format!("Tirages · {event}"), en: format!("Prints · {event}")), tr!(fr: "Le nombre de tirages prévus au contrat pour l'événement en cours. 0 = illimité.", en: "Prints included in the contract for the current event. 0 = unlimited."), |ui| {
            row(ui, pal, tr!(fr: "Tirages prévus", en: "Planned prints"), "", |ui| {
                step_u32(ui, pal, &mut self.quota_draft, (0, 100_000), 10, |v| if v == 0 { tr!(fr: "Illimité", en: "Unlimited").into() } else { v.to_string() });
            });
            let maximum = quota.maximum();
            let used = quota.completed.saturating_add(outstanding);
            if quota.included > 0 {
                let remaining = maximum.saturating_sub(used);
                progress(ui, pal, used as f32 / maximum.max(1) as f32, if remaining * 10 < maximum { pal.warn } else { pal.primary });
                ui.label(text(tr!(fr: format!("{} imprimé(s) · {outstanding} en attente · {remaining} restant(s) sur {maximum}", quota.completed), en: format!("{} printed · {outstanding} pending · {remaining} left of {maximum}", quota.completed)), 14.0, pal.muted));
            } else {
                ui.label(text(tr!(fr: format!("Aucune limite · {} tirage(s) imprimé(s)", quota.completed), en: format!("No limit · {} print(s) made", quota.completed)), 14.0, pal.muted));
            }
            ui.horizontal(|ui| {
                let changed = self.quota_draft != quota.included;
                if btn_enabled(ui, pal, changed, tr!(fr: "Enregistrer les tirages", en: "Save prints"), Btn::Primary) {
                    self.status = match self.print_ledger.update_included(
                        &event, self.quota_draft, outstanding, &self.storage.root.join("print-ledger.json")) {
                        Ok(()) => tr!(fr: "Tirages enregistrés", en: "Prints saved").into(),
                        Err(error) => tr!(fr: format!("Tirages refusés : {error}"), en: format!("Prints refused: {error}")),
                    };
                }
                if quota.included > 0 && !quota.bonus_unlocked
                    && btn(ui, pal, &tr!(fr: format!("Débloquer +5 % (+{})", quota.bonus().max(1)), en: format!("Unlock +5 % (+{})", quota.bonus().max(1))), Btn::Secondary).clicked() {
                    self.status = match self.print_ledger.unlock_bonus(&event, &self.storage.root.join("print-ledger.json")) {
                        Ok(()) => tr!(fr: "Marge de +5 % débloquée", en: "+5 % margin unlocked").into(),
                        Err(error) => tr!(fr: format!("Déblocage impossible : {error}"), en: format!("Cannot unlock: {error}")),
                    };
                }
            });
        });

        let jobs: Vec<_> = self.print_jobs.jobs.iter()
            .filter(|(_, job)| job.reservation_id == event
                && matches!(job.status, print_jobs::JobStatus::Pending | print_jobs::JobStatus::Submitted))
            .take(30)
            .map(|(id, job)| (id.clone(), job.status, job.spooler_id.clone()))
            .collect();
        if jobs.is_empty() { return; }
        card(ui, pal, tr!(fr: "Impressions à vérifier", en: "Prints to check"), tr!(fr: "L'ordinateur a accepté ces impressions : vérifiez que le papier est bien sorti.", en: "The computer accepted these prints: check that the paper came out."), |ui| {
            for (index, (id, status, spooler_id)) in jobs.into_iter().enumerate() {
                if index > 0 { divider(ui, pal); }
                let state = match status {
                    print_jobs::JobStatus::Pending => tr!(fr: "En file d'attente", en: "Queued"),
                    _ => tr!(fr: "Envoyée à l'imprimante", en: "Sent to the printer"),
                };
                let reference = spooler_id.unwrap_or_else(|| id.chars().rev().take(8).collect::<String>().chars().rev().collect());
                row(ui, pal, state, &tr!(fr: format!("Réf. {reference}"), en: format!("Ref. {reference}")), |ui| {
                    if btn(ui, pal, tr!(fr: "Pas imprimée", en: "Not printed"), Btn::Danger).clicked() {
                        let jobs_path = self.storage.root.join("print-jobs.json");
                        self.status = match self.print_jobs.cancel_verified_not_printed(&id, &jobs_path) {
                            Ok(()) => tr!(fr: "Impression annulée, tirage rendu", en: "Print cancelled, print credited back").into(),
                            Err(error) => tr!(fr: format!("Annulation impossible : {error}"), en: format!("Cannot cancel: {error}")),
                        };
                    }
                    if btn(ui, pal, tr!(fr: "Imprimée ✓", en: "Printed ✓"), Btn::Primary).clicked() {
                        let jobs_path = self.storage.root.join("print-jobs.json");
                        let ledger_path = self.storage.root.join("print-ledger.json");
                        self.status = match self.print_jobs.confirm(&mut self.print_ledger, &id, &jobs_path, &ledger_path) {
                            Ok(()) => tr!(fr: "Impression confirmée", en: "Print confirmed").into(),
                            Err(error) => tr!(fr: format!("Confirmation impossible : {error}"), en: format!("Cannot confirm: {error}")),
                        };
                    }
                });
            }
        });
    }

    fn page_camera(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        // Interface language: the computer's (if available) or a fixed one; applied at once.
        let system = boothsoft::i18n::system();
        let mut choice = match self.storage.settings.language { boothsoft::i18n::LanguageSetting::Auto => 0u8, boothsoft::i18n::LanguageSetting::En => 1, boothsoft::i18n::LanguageSetting::Fr => 2 };
        let auto_hint = match system {
            Some(lang) => tr!(fr: format!("Automatique : langue de l'ordinateur ({}).", lang.native_name()), en: format!("Automatic: the computer's language ({}).", lang.native_name())),
            None => tr!(fr: "Automatique : l'ordinateur n'est ni en anglais ni en français, la borne est en anglais.".to_owned(), en: "Automatic: the computer is neither in English nor French, so the booth uses English.".to_owned()),
        };
        card(ui, pal, tr!(fr: "Langue", en: "Language"), "", |ui| {
            row(ui, pal, tr!(fr: "Langue de la borne", en: "Booth language"), &auto_hint, |ui| {
                segmented_wide(ui, pal, &mut choice, &[(0, tr!(fr: "Auto", en: "Auto")), (1, "English"), (2, "Français")]);
            });
        });
        self.storage.settings.language = match choice { 1 => boothsoft::i18n::LanguageSetting::En, 2 => boothsoft::i18n::LanguageSetting::Fr, _ => boothsoft::i18n::LanguageSetting::Auto };
        card(ui, pal, tr!(fr: "Appareil photo", en: "Camera"), "", |ui| {
            let hint = if self.device_scan.is_some() { tr!(fr: "Recherche…", en: "Searching…").to_owned() } else { tr!(fr: format!("{} caméra(s) détectée(s)", self.cameras.len()), en: format!("{} camera(s) found", self.cameras.len())) };
            row(ui, pal, tr!(fr: "Caméra", en: "Camera"), &hint, |ui| {
                if btn_enabled(ui, pal, self.device_scan.is_none(), tr!(fr: "Actualiser", en: "Refresh"), Btn::Secondary) { self.refresh_devices(ui.ctx()); }
                egui::ComboBox::from_id_salt("system-camera").width(260.0)
                    .selected_text(text(self.camera_label(), 15.0, pal.ink))
                    .show_ui(ui, |ui| {
                        for camera in &self.cameras {
                            ui.selectable_value(&mut self.storage.settings.camera_index, camera.index, &camera.name);
                        }
                    });
            });
            divider(ui, pal);
            row(ui, pal, tr!(fr: "Mode démo", en: "Demo mode"), tr!(fr: "Photos de démonstration, sans caméra branchée.", en: "Demo photos, no camera plugged in."), |ui| {
                toggle(ui, pal, &mut self.storage.settings.demo_mode);
            });
        });
        card(ui, pal, tr!(fr: "Live et veille", en: "Live view and sleep"), "", |ui| {
            row(ui, pal, tr!(fr: "Fluidité du live", en: "Live view smoothness"), if self.software_renderer() { tr!(fr: "Sans GPU : 15 à 20 images/s conseillées.", en: "No GPU: 15 to 20 fps advised.") } else { tr!(fr: "30 images/s recommandées.", en: "30 fps recommended.") }, |ui| {
                step_u8(ui, pal, &mut self.storage.settings.live_fps, (10, 30), 5, |v| tr!(fr: format!("{v} img/s"), en: format!("{v} fps")));
            });
            divider(ui, pal);
            row(ui, pal, tr!(fr: "Mise en veille", en: "Sleep"), tr!(fr: "Écran d'accueil après cette durée sans personne.", en: "Welcome screen after this long without anyone."), |ui| {
                step_u16(ui, pal, &mut self.storage.settings.sleep_minutes, (1, 60), 1, |v| format!("{v} min"));
            });
            divider(ui, pal);
            row(ui, pal, tr!(fr: "Réveil automatique", en: "Automatic wake-up"), tr!(fr: "La borne se réveille quand quelqu'un s'approche.", en: "The booth wakes up when someone walks up."), |ui| {
                toggle(ui, pal, &mut self.storage.settings.wake_on_motion);
            });
            divider(ui, pal);
            row(ui, pal, tr!(fr: "Plein écran", en: "Full screen"), tr!(fr: "Mode borne : aucune fenêtre ni barre visible.", en: "Kiosk mode: no window or bar visible."), |ui| {
                toggle(ui, pal, &mut self.storage.settings.fullscreen);
            });
            divider(ui, pal);
            let mut on = *self.autostart.get_or_insert_with(boothsoft::autostart::is_enabled);
            row(ui, pal, tr!(fr: "Lancer au démarrage de l'ordinateur", en: "Start with the computer"), tr!(fr: "L'ordinateur s'allume, BoothSoft s'ouvre en plein écran sur l'événement du planning. Appliqué immédiatement.", en: "The computer starts, BoothSoft opens full screen on the scheduled event. Applied immediately."), |ui| {
                if toggle(ui, pal, &mut on) {
                    match boothsoft::autostart::set(on) {
                        Ok(()) => {
                            self.autostart = Some(on);
                            self.status = if on { tr!(fr: "BoothSoft se lancera au démarrage", en: "BoothSoft will start with the computer").into() } else { tr!(fr: "Lancement au démarrage désactivé", en: "Start with the computer turned off").into() };
                        }
                        Err(error) => self.status = tr!(fr: format!("Démarrage automatique impossible : {error}"), en: format!("Cannot set start with the computer: {error}")),
                    }
                }
            });
        });
        card(ui, pal, "", "", |ui| {
            ui.label(text(self.renderer_summary(), 14.0, pal.muted));
        });
    }

    fn page_theme(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        if self.theme_thumbs.is_none() {
            let ctx = ui.ctx().clone();
            self.theme_thumbs = Some(self.installed_themes.iter()
                .filter_map(|t| logo_texture(&ctx, &format!("theme-thumb-{}", t.id), &t.logo, (360, 150)).map(|tex| (t.id.clone(), tex)))
                .collect());
        }
        let active_id = theme::active().map_or_else(|| "default".to_owned(), |t| t.id.clone());
        let width = ui.available_width();
        let columns = ((width + 14.0) / 214.0).floor().max(1.0) as usize;
        let tile_w = (width - 14.0 * (columns as f32 - 1.0)) / columns as f32;
        let themes: Vec<_> = self.installed_themes.iter()
            .map(|t| (t.id.clone(), t.config.name.clone(), hex(&t.config.colors.background), [hex(&t.config.colors.primary), hex(&t.config.colors.accent), hex(&t.config.colors.foreground)]))
            .collect();
        for chunk in themes.chunks(columns) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                for (id, name, bg, dots) in chunk {
                    let (rect, response) = ui.allocate_exact_size(Vec2::new(tile_w, 196.0), Sense::click());
                    let selected = self.theme_choice == *id;
                    let painter = ui.painter();
                    painter.rect(rect, CornerRadius::same(16), pal.surface,
                        Stroke::new(if selected { 2.5_f32 } else { 1.0 }, if selected { pal.primary } else { pal.line }), StrokeKind::Inside);
                    let preview = Rect::from_min_size(rect.min + Vec2::splat(8.0), Vec2::new(rect.width() - 16.0, 116.0));
                    painter.rect_filled(preview, CornerRadius::same(11), *bg);
                    if let Some((_, tex)) = self.theme_thumbs.as_ref().and_then(|v| v.iter().find(|(tid, _)| tid == id)) {
                        let logo = ui::fit(tex.size_vec2(), preview.shrink(18.0));
                        painter.image(tex.id(), logo, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    painter.text(Pos2::new(rect.left() + 16.0, preview.bottom() + 14.0), Align2::LEFT_TOP, name, ui::bold(16.0), pal.ink);
                    for (i, color) in dots.iter().enumerate() {
                        painter.circle(Pos2::new(rect.left() + 24.0 + i as f32 * 20.0, rect.bottom() - 22.0), 7.0, *color, Stroke::new(1.0_f32, pal.line));
                    }
                    if *id == active_id {
                        let galley = painter.layout_no_wrap(tr!(fr: "Actif", en: "Active").into(), ui::bold(12.5), pal.on_primary);
                        let badge = Rect::from_min_size(Pos2::new(rect.right() - 16.0 - galley.size().x - 20.0, rect.bottom() - 34.0), galley.size() + Vec2::new(20.0, 10.0));
                        painter.rect_filled(badge, CornerRadius::same(255), pal.primary);
                        painter.galley(badge.min + Vec2::new(10.0, 5.0), galley, pal.on_primary);
                    }
                    if response.clicked() { self.theme_choice.clone_from(id); }
                }
            });
            ui.add_space(4.0);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let can_apply = self.theme_choice != active_id && !self.installed_themes.is_empty();
            if btn_enabled(ui, pal, can_apply, tr!(fr: "Appliquer ce thème", en: "Apply this theme"), Btn::Primary) { self.apply_selected_theme(ui.ctx()); }
            let idle = self.theme_import_rx.is_none();
            if btn_enabled(ui, pal, idle, tr!(fr: "Importer un thème (ZIP)", en: "Import a theme (ZIP)"), Btn::Secondary) { self.begin_theme_import(ui.ctx()); }
            if btn_enabled(ui, pal, idle, tr!(fr: "Actualiser", en: "Refresh"), Btn::Secondary) { self.refresh_theme_catalog(); }
        });
        if !self.theme_status.is_empty() { ui.label(text(&self.theme_status, 14.0, pal.muted)); }
        ui.label(text(tr!(fr: format!("Dossier : themes/{}", self.theme_choice), en: format!("Folder: themes/{}", self.theme_choice)), 13.0, pal.muted));
    }

    fn page_security(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        card(ui, pal, tr!(fr: "Changer le code", en: "Change the code"), tr!(fr: "6 chiffres. Il est demandé à chaque ouverture de la maintenance.", en: "6 digits. Asked every time maintenance opens."), |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.new_pin).password(true).char_limit(6)
                    .font(ui::bold(20.0)).desired_width(180.0).margin(Vec2::new(12.0, 10.0)).hint_text(tr!(fr: "Nouveau code", en: "New code")));
                if btn_enabled(ui, pal, self.new_pin.len() == 6, tr!(fr: "Changer le code", en: "Change the code"), Btn::Primary) {
                    if valid_pin(&self.new_pin) {
                        if let Ok(hash) = bcrypt::hash(&self.new_pin, bcrypt::DEFAULT_COST) {
                            self.status = match self.save_only(|s| s.admin_pin_hash.clone_from(&hash)) {
                                Ok(()) => tr!(fr: "Code mis à jour", en: "Code updated").into(),
                                Err(error) => tr!(fr: format!("Code non enregistré : {error}"), en: format!("Code not saved: {error}")),
                            };
                        }
                    } else { self.status = tr!(fr: "Le code doit comporter 6 chiffres", en: "The code must have 6 digits").into(); }
                    self.new_pin.clear();
                }
            });
        });
        card(ui, pal, tr!(fr: "Verrouiller", en: "Lock"), tr!(fr: "Referme la maintenance sans quitter cet écran.", en: "Closes maintenance without leaving this screen."), |ui| {
            if btn(ui, pal, tr!(fr: "Verrouiller maintenant", en: "Lock now"), Btn::Secondary).clicked() {
                self.request_leave(ui.ctx(), PendingLeave::Lock);
            }
        });
    }

    fn page_data(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        let photos: usize = self.storage.sessions.iter().map(|session| session.shots.len()).sum();
        let messages = boothsoft::guestbook::list(&self.storage.root.join("sessions").join(self.current_event().folder));
        let message_seconds: f64 = messages.iter().map(|m| m.info.seconds).sum();
        card(ui, pal, tr!(fr: "Photos et messages", en: "Photos and messages"), "", |ui| {
            row(ui, pal, tr!(fr: "Séances enregistrées", en: "Saved sessions"), "", |ui| { ui.label(strong(self.storage.sessions.len().to_string(), 17.0, pal.ink)); });
            divider(ui, pal);
            row(ui, pal, "Photos", "", |ui| { ui.label(strong(photos.to_string(), 17.0, pal.ink)); });
            if !messages.is_empty() || self.storage.settings.guestbook_enabled {
                divider(ui, pal);
                row(ui, pal, tr!(fr: "Messages du livre d'or", en: "Guest book messages"), tr!(fr: "Événement en cours.", en: "Current event."), |ui| {
                    ui.label(strong(format!("{} · {}", messages.len(), duration_label(message_seconds)), 17.0, pal.ink));
                });
            }
            divider(ui, pal);
            ui.label(text(tr!(fr: format!("Stockage : {}", self.storage.disk_location().display()), en: format!("Storage: {}", self.storage.disk_location().display())), 13.5, pal.muted));
            if let Some(logs) = boothsoft::log::directory() {
                ui.label(text(tr!(fr: format!("Journaux ({} derniers jours) : {}", boothsoft::log::KEEP_DAYS, logs.display()), en: format!("Logs (last {} days): {}", boothsoft::log::KEEP_DAYS, logs.display())), 13.5, pal.muted));
            }
        });
        card(ui, pal, tr!(fr: "Exporter", en: "Export"), tr!(fr: "Dernière séance contenant une photo.", en: "Last session with a photo."), |ui| {
            ui.horizontal(|ui| {
                if btn(ui, pal, tr!(fr: "Dernière photo (JPEG)", en: "Last photo (JPEG)"), Btn::Secondary).clicked() {
                    let found = self.storage.sessions.iter().find(|s| !s.shots.is_empty());
                    if let Some(session) = found {
                        let shot = session.shots.last().expect("nonempty");
                        match self.storage.export_shot(session, shot) {
                            Ok(path) => { self.status = tr!(fr: format!("Exportée : {}", path.display()), en: format!("Exported: {}", path.display())); self.last_export = Some(path); }
                            Err(error) => self.status = tr!(fr: format!("Export impossible : {error}"), en: format!("Export failed: {error}")),
                        }
                    } else { self.status = tr!(fr: "Aucune photo à exporter", en: "No photo to export").into(); }
                }
                if btn(ui, pal, tr!(fr: "Planche de la séance", en: "Session contact sheet"), Btn::Secondary).clicked() { self.export_contact_sheet(); }
            });
            if let Some(path) = &self.last_export {
                ui.label(text(tr!(fr: format!("Dernier export : {}", path.display()), en: format!("Last export: {}", path.display())), 13.5, pal.muted));
            }
        });
        let camera_status = self.status.clone();
        card(ui, pal, tr!(fr: "Diagnostic caméra", en: "Camera check"), &camera_status, |ui| {
            ui.horizontal(|ui| {
                if btn_enabled(ui, pal, !self.storage.settings.demo_mode, tr!(fr: "Tester la caméra", en: "Test the camera"), Btn::Secondary) {
                    self.camera.send(CameraCommand::Start { index: self.storage.settings.camera_index });
                }
                if btn(ui, pal, tr!(fr: "Arrêter", en: "Stop"), Btn::Secondary).clicked() { self.camera.send(CameraCommand::Stop); }
            });
        });
        card(ui, pal, tr!(fr: "Assistance à distance", en: "Remote support"), "", |ui| {
            ui.label(text(tr!(fr: "Accès distant via un VPN installé sur l'ordinateur (ex. Tailscale).", en: "Remote access through a VPN installed on the computer (e.g. Tailscale)."), 14.0, pal.muted));
            ui.label(text(tr!(fr: "Connexion 4G et journaux : à contrôler depuis le système.", en: "4G connection and logs: check them from the system."), 14.0, pal.muted));
            ui.label(text(tr!(fr: "BoothSoft n'ouvre aucun port réseau.", en: "BoothSoft opens no network port."), 14.0, pal.muted));
        });
    }

    fn export_contact_sheet(&mut self) {
        let Some(session) = self.storage.sessions.iter().find(|s| !s.shots.is_empty()) else {
            self.status = tr!(fr: "Aucune photo à exporter", en: "No photo to export").into();
            return;
        };
        let paths: Vec<_> = session.shots.iter()
            .map(|shot| self.storage.shot_path(session, shot)).collect();
        let path = self.storage.root.join("print-exports").join(format!("{}-strip.jpg", session.id));
        if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
        match storage::export_contact_sheet(&paths, &path) {
            Ok(()) => { self.last_export = Some(path.clone()); self.status = tr!(fr: format!("Exportée : {}", path.display()), en: format!("Exported: {}", path.display())); }
            Err(error) => self.status = error.to_string(),
        }
    }
}

/// « 1 min 05 s », « 12 s ».
fn duration_label(seconds: f64) -> String {
    let total = seconds.round() as u64;
    if total >= 60 { format!("{} min {:02} s", total / 60, total % 60) } else { format!("{total} s") }
}

/// Copy in progress: files done, total, result.
type ExportJob = (Arc<std::sync::atomic::AtomicUsize>, usize, std::sync::mpsc::Receiver<Result<usize, String>>);

/// Maintenance state of the guest book: microphone test, message list,
/// playback and export. Reset when maintenance closes.
#[derive(Default)]
pub(super) struct GuestBookAdmin {
    mics: Option<Vec<String>>,
    mic_scan: Option<std::sync::mpsc::Receiver<Vec<String>>>,
    test_mic: Option<boothsoft::audio::MicStream>,
    test_meter: f32,
    test_error: String,
    event: Option<String>,
    messages: Vec<boothsoft::guestbook::Message>,
    loaded: Option<String>,
    thumbs: std::collections::HashMap<PathBuf, Option<TextureHandle>>,
    playing: Option<(usize, boothsoft::guestbook::Player, Option<TextureHandle>)>,
    delete: Option<usize>,
    export: Option<ExportJob>,
    export_status: String,
}

impl Kiosk {
    fn card_guestbook(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        card(ui, pal, tr!(fr: "Livre d'or vidéo", en: "Video guest book"), tr!(fr: "Option à débloquer : les invités laissent un message vidéo avec le son (30 s au plus) pour l'organisateur.", en: "Optional feature: guests leave a video message with sound (30 s max) for the host."), |ui| {
            row(ui, pal, tr!(fr: "Activer le livre d'or", en: "Turn on the guest book"), tr!(fr: "Ajoute « Livre d'or » au sélecteur du live (« Une photo » reste présélectionnée).", en: "Adds “Guest book” to the live view's mode switch (“Single photo” stays preselected)."), |ui| {
                toggle(ui, pal, &mut self.storage.settings.guestbook_enabled);
            });
            if !self.storage.settings.guestbook_enabled { return; }
            divider(ui, pal);
            row(ui, pal, tr!(fr: "Durée maximale", en: "Maximum length"), tr!(fr: "Les invités arrêtent quand ils veulent ; la borne arrête à cette durée.", en: "Guests stop when they want; the booth stops at this length."), |ui| {
                step_u8(ui, pal, &mut self.storage.settings.guestbook_max_seconds, (10, 30), 5, |v| format!("{v} s"));
            });
            divider(ui, pal);
            let event_name = self.current_event().name;
            row(ui, pal, tr!(fr: "Destinataire", en: "Recipient"), tr!(fr: "« Un message pour … ». Vide : nom de l'événement.", en: "“A message for …”. Empty: the event's name."), |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.storage.settings.guestbook_recipient)
                    .font(ui::body(17.0)).desired_width(300.0).margin(Vec2::new(12.0, 10.0)).hint_text(event_name).char_limit(80));
            });
            divider(ui, pal);
            // Microphone list, scanned once on a worker thread (can be slow).
            if self.gb_admin.mics.is_none() && self.gb_admin.mic_scan.is_none() {
                let (tx, rx) = std::sync::mpsc::sync_channel(1);
                let wake = ui.ctx().clone();
                let _ = std::thread::Builder::new().name("bs-mic-scan".into()).spawn(move || {
                    let _ = tx.send(boothsoft::audio::list_inputs());
                    wake.request_repaint();
                });
                self.gb_admin.mic_scan = Some(rx);
            }
            if let Some(list) = self.gb_admin.mic_scan.as_ref().and_then(|rx| rx.try_recv().ok()) {
                self.gb_admin.mics = Some(list);
                self.gb_admin.mic_scan = None;
            }
            let mics = self.gb_admin.mics.clone().unwrap_or_default();
            let current = if self.storage.settings.guestbook_mic.is_empty() { tr!(fr: "Par défaut du système", en: "System default").to_owned() } else { self.storage.settings.guestbook_mic.clone() };
            let mut changed = false;
            row(ui, pal, tr!(fr: "Micro", en: "Microphone"), if self.gb_admin.mics.is_none() { tr!(fr: "Recherche des micros…", en: "Looking for microphones…") } else { tr!(fr: "Un micro USB capte mieux la voix dans le bruit d'une soirée.", en: "A USB microphone picks up voices better in a noisy party.") }, |ui| {
                egui::ComboBox::from_id_salt("gb-mic").selected_text(strong(current, 15.0, pal.ink)).width(300.0).show_ui(ui, |ui| {
                    changed |= ui.selectable_value(&mut self.storage.settings.guestbook_mic, String::new(), tr!(fr: "Par défaut du système", en: "System default")).changed();
                    for name in &mics {
                        changed |= ui.selectable_value(&mut self.storage.settings.guestbook_mic, name.clone(), name.as_str()).changed();
                    }
                });
            });
            if changed { self.gb_admin.test_mic = None; }
            divider(ui, pal);
            let before = self.storage.settings.guestbook_gain;
            row(ui, pal, tr!(fr: "Sensibilité", en: "Sensitivity"), tr!(fr: "Plus forte si les invités paraissent loin ou la salle bruyante.", en: "Higher if guests sound far away or the room is loud."), |ui| {
                segmented_wide(ui, pal, &mut self.storage.settings.guestbook_gain, &[(1, tr!(fr: "Faible", en: "Low")), (2, tr!(fr: "Normale", en: "Normal")), (4, tr!(fr: "Forte", en: "High"))]);
            });
            if before != self.storage.settings.guestbook_gain {
                if let Some(mic) = self.gb_admin.test_mic.as_ref() { mic.set_gain(self.storage.settings.guestbook_gain); }
            }
            divider(ui, pal);
            row(ui, pal, tr!(fr: "Volume de relecture", en: "Playback volume"), tr!(fr: "Quand l'invité revoit son message.", en: "When the guest replays their message."), |ui| {
                step_u8(ui, pal, &mut self.storage.settings.guestbook_playback_volume, (0, 100), 10, |v| format!("{v} %"));
            });
            divider(ui, pal);
            // Microphone test with a level meter (15 redraws per second while open).
            let testing = self.gb_admin.test_mic.is_some();
            let peak = self.gb_admin.test_mic.as_ref().map_or(0.0, boothsoft::audio::MicStream::take_peak);
            self.gb_admin.test_meter = boothsoft::audio::meter(peak).max(self.gb_admin.test_meter - 0.03);
            let meter = self.gb_admin.test_meter;
            let error = self.gb_admin.test_error.clone();
            let mut toggle_test = false;
            row(ui, pal, tr!(fr: "Tester le micro", en: "Test the microphone"), if !error.is_empty() { error.as_str() } else if testing { tr!(fr: "Parlez : la jauge doit bouger.", en: "Speak: the meter should move.") } else { "" }, |ui| {
                toggle_test = btn(ui, pal, if testing { tr!(fr: "Arrêter", en: "Stop") } else { tr!(fr: "Tester", en: "Test") }, Btn::Secondary).clicked();
                let (rect, _) = ui.allocate_exact_size(Vec2::new(220.0, 22.0), Sense::hover());
                let segments = 16;
                let w = (rect.width() - 3.0 * (segments - 1) as f32) / segments as f32;
                for i in 0..segments {
                    let r = Rect::from_min_size(Pos2::new(rect.left() + i as f32 * (w + 3.0), rect.top()), Vec2::new(w, rect.height()));
                    let lit = (i as f32) < meter * segments as f32 && testing;
                    ui.painter().rect_filled(r, CornerRadius::same(3), if lit { if i >= 13 { pal.warn } else { pal.ok } } else { pal.line });
                }
            });
            if toggle_test {
                if testing { self.gb_admin.test_mic = None; } else {
                    match boothsoft::audio::MicStream::open(&self.storage.settings.guestbook_mic, self.storage.settings.guestbook_gain) {
                        Ok(mic) => { self.gb_admin.test_mic = Some(mic); self.gb_admin.test_error.clear(); }
                        Err(error) => self.gb_admin.test_error = tr!(fr: format!("Micro indisponible : {error}"), en: format!("Microphone unavailable: {error}")),
                    }
                }
            }
        });
    }

    fn page_guestbook(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        let sessions = self.storage.root.join("sessions");
        let mut folders = boothsoft::guestbook::event_folders(&sessions);
        let current = self.current_event().folder;
        if !folders.contains(&current) { folders.insert(0, current.clone()); }
        let selected = self.gb_admin.event.clone().filter(|e| folders.contains(e)).unwrap_or(current);
        if self.gb_admin.loaded.as_ref() != Some(&selected) {
            self.gb_admin.messages = boothsoft::guestbook::list(&sessions.join(&selected));
            self.gb_admin.loaded = Some(selected.clone());
            self.gb_admin.playing = None;
        }
        if !self.storage.settings.guestbook_enabled {
            card(ui, pal, "", "", |ui| {
                ui.label(text(tr!(fr: "Le livre d'or est désactivé : activez-le dans Événements › Livre d'or vidéo.", en: "The guest book is off: turn it on in Events › Video guest book."), 15.0, pal.muted));
            });
        }
        let mut choice = selected.clone();
        card(ui, pal, "", "", |ui| {
            row(ui, pal, tr!(fr: "Événement", en: "Event"), "", |ui| {
                egui::ComboBox::from_id_salt("gb-event").selected_text(strong(choice.clone(), 15.0, pal.ink)).width(320.0).show_ui(ui, |ui| {
                    for folder in &folders { ui.selectable_value(&mut choice, folder.clone(), folder.as_str()); }
                });
            });
            let total: f64 = self.gb_admin.messages.iter().map(|m| m.info.seconds).sum();
            ui.label(text(tr!(fr: format!("{} message(s) · {}", self.gb_admin.messages.len(), duration_label(total)), en: format!("{} message(s) · {}", self.gb_admin.messages.len(), duration_label(total))), 14.0, pal.muted));
        });
        if choice != selected { self.gb_admin.event = Some(choice); return; }

        // Playback (modal-free: the player sits on top of the list).
        let ctx = ui.ctx().clone();
        if let Some((index, player, texture)) = self.gb_admin.playing.as_mut() {
            if let Some(frame) = player.take_frame() {
                let rgb = image::DynamicImage::ImageRgba8(frame).into_rgb8();
                let mut small = compositor::fit_within(&rgb, 854);
                update_texture(&ctx, texture, "gb-admin-play", small.get_or_insert(rgb));
            }
            let index = *index;
            let finished = player.finished();
            let mut close = false;
            card(ui, pal, &tr!(fr: format!("Lecture : {}", self.gb_admin.messages.get(index).map(|m| m.stem()).unwrap_or_default()), en: format!("Playing: {}", self.gb_admin.messages.get(index).map(|m| m.stem()).unwrap_or_default())), "", |ui| {
                if let Some(texture) = texture.as_ref() {
                    let width = ui.available_width().min(760.0);
                    let size = texture.size_vec2() * (width / texture.size_vec2().x);
                    ui.image((texture.id(), size));
                }
                if btn(ui, pal, if finished { tr!(fr: "Fermer", en: "Close") } else { tr!(fr: "Arrêter", en: "Stop") }, Btn::Secondary).clicked() { close = true; }
            });
            if close { self.gb_admin.playing = None; }
        }

        let mut play = None;
        card(ui, pal, tr!(fr: "Messages", en: "Messages"), tr!(fr: "Du plus ancien au plus récent.", en: "Oldest first."), |ui| {
            if self.gb_admin.messages.is_empty() {
                ui.label(text(tr!(fr: "Aucun message pour cet événement.", en: "No message for this event."), 15.0, pal.muted));
            }
            for (index, message) in self.gb_admin.messages.iter().enumerate() {
                if index > 0 { divider(ui, pal); }
                let thumb = self.gb_admin.thumbs.entry(message.video.clone())
                    .or_insert_with(|| load_texture(&ctx, &format!("gb-thumb-{index}"), &message.thumbnail(), 160, 90)).clone();
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(128.0, 72.0), Sense::hover());
                    match thumb.as_ref() {
                        Some(texture) => { ui.painter().image(texture.id(), ui::fit(texture.size_vec2(), rect), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE); }
                        None => { ui.painter().rect_filled(rect, CornerRadius::same(8), pal.line); }
                    }
                    ui.vertical(|ui| {
                        let when = chrono::DateTime::parse_from_rfc3339(&message.info.recorded_at).map(|d| d.format(tr!(fr: "%d/%m à %H:%M", en: "%b %-d, %H:%M")).to_string())
                            .unwrap_or_else(|_| message.stem());
                        ui.label(strong(when, 16.0, pal.ink));
                        let sound = if message.info.has_audio { tr!(fr: "avec le son", en: "with sound") } else { tr!(fr: "sans le son", en: "without sound") };
                        ui.label(text(format!("{} · {sound}", duration_label(message.info.seconds)), 13.5, pal.muted));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if btn(ui, pal, tr!(fr: "Supprimer", en: "Delete"), Btn::Danger).clicked() { self.gb_admin.delete = Some(index); }
                        if btn(ui, pal, tr!(fr: "▶  Lire", en: "▶  Play"), Btn::Secondary).clicked() { play = Some(index); }
                    });
                });
            }
        });
        if let Some(index) = play {
            let volume = self.storage.settings.guestbook_playback_volume;
            let wake: boothsoft::camera::WakeUi = { let c = ctx.clone(); Arc::new(move || c.request_repaint_after(WAKE)) };
            self.gb_admin.playing = None;
            match self.gb_admin.messages.get(index).map(|m| boothsoft::guestbook::Player::start(&m.video, volume, wake)) {
                Some(Ok(player)) => self.gb_admin.playing = Some((index, player, None)),
                Some(Err(error)) => self.status = tr!(fr: format!("Lecture impossible : {error}"), en: format!("Cannot play: {error}")),
                None => {}
            }
        }

        // Export to a USB stick (copy on a worker thread, with progress).
        let mut start_export = false;
        card(ui, pal, tr!(fr: "Remettre les messages", en: "Hand over the messages"), tr!(fr: "Copie les vidéos (.avi) et un mode d'emploi dans le dossier choisi, par exemple une clé USB.", en: "Copies the videos (.avi) and a read-me into the chosen folder, for example a USB stick."), |ui| {
            if let Some((done, total, rx)) = self.gb_admin.export.as_ref() {
                let done = done.load(std::sync::atomic::Ordering::Relaxed);
                progress(ui, pal, if *total == 0 { 1.0 } else { done as f32 / *total as f32 }, pal.primary);
                ui.label(text(tr!(fr: format!("Copie… {done} / {total}"), en: format!("Copying… {done} / {total}")), 14.0, pal.muted));
                if let Ok(result) = rx.try_recv() {
                    self.gb_admin.export_status = match result {
                        Ok(count) => tr!(fr: format!("{count} message(s) copiés. Ouvrez-les avec VLC."), en: format!("{count} message(s) copied. Open them with VLC.")),
                        Err(error) => tr!(fr: format!("Copie impossible : {error}"), en: format!("Copy failed: {error}")),
                    };
                    self.gb_admin.export = None;
                }
            } else {
                start_export = btn_enabled(ui, pal, !self.gb_admin.messages.is_empty(), tr!(fr: "Exporter les messages…", en: "Export the messages…"), Btn::Primary);
            }
            if !self.gb_admin.export_status.is_empty() { ui.label(text(self.gb_admin.export_status.clone(), 14.0, pal.muted)); }
        });
        if start_export {
            // Native dialog on the application thread (macOS requirement).
            if let Some(target) = rfd::FileDialog::new().set_title(tr!(fr: "Où copier les messages ?", en: "Where should the messages go?")).pick_folder() {
                let event = sessions.join(&selected);
                let done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                let counter = Arc::clone(&done);
                let (tx, rx) = std::sync::mpsc::sync_channel(1);
                let total = self.gb_admin.messages.len();
                let wake = ctx.clone();
                let spawned = std::thread::Builder::new().name("bs-gb-export".into()).spawn(move || {
                    let result = boothsoft::guestbook::export(&event, &target, &|i, _| counter.store(i, std::sync::atomic::Ordering::Relaxed))
                        .map_err(|e| e.to_string());
                    let _ = tx.send(result);
                    wake.request_repaint();
                });
                if spawned.is_ok() { self.gb_admin.export = Some((done, total, rx)); self.gb_admin.export_status.clear(); }
            }
        }
    }

    fn render_message_dialogs(&mut self, ctx: &egui::Context, full: Rect, pal: &Pal) {
        let Some(index) = self.gb_admin.delete else { return };
        let Some(message) = self.gb_admin.messages.get(index).cloned() else { self.gb_admin.delete = None; return };
        let mut choice: Option<bool> = None;
        let outside = modal(ctx, "delete-message", full, pal, 520.0, |ui| {
            ui.label(RichText::new(tr!(fr: "Supprimer ce message ?", en: "Delete this message?")).font(ui::display(28.0)).color(pal.ink));
            ui.add_space(4.0);
            ui.label(text(tr!(fr: format!("Le message de {} sera effacé du disque. C'est définitif.", duration_label(message.info.seconds)), en: format!("The {} message will be erased from the disk. This cannot be undone.", duration_label(message.info.seconds))), 16.0, pal.muted));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(strong(tr!(fr: "Annuler", en: "Cancel"), 16.0, pal.ink)).fill(pal.surface)
                    .stroke(Stroke::new(1.0_f32, ui::alpha(pal.ink, 60))).corner_radius(12).min_size(Vec2::new(150.0, 54.0))).clicked() { choice = Some(false); }
                if ui.add(egui::Button::new(strong(tr!(fr: "Supprimer", en: "Delete"), 16.0, Color32::WHITE)).fill(pal.warn)
                    .corner_radius(12).min_size(Vec2::new(170.0, 54.0))).clicked() { choice = Some(true); }
            });
        });
        if outside { choice = choice.or(Some(false)); }
        match choice {
            Some(true) => {
                if self.gb_admin.playing.as_ref().is_some_and(|p| p.0 == index) { self.gb_admin.playing = None; }
                match message.delete() {
                    Ok(()) => boothsoft::log::info(format!("Guest book: {} deleted in maintenance", message.stem())),
                    Err(error) => self.status = tr!(fr: format!("Suppression impossible : {error}"), en: format!("Cannot delete: {error}")),
                }
                self.gb_admin.delete = None;
                self.gb_admin.loaded = None;
                self.gb_admin.thumbs.clear();
            }
            Some(false) => self.gb_admin.delete = None,
            None => {}
        }
    }
}

/// Segmented control with word labels (wider segments).
fn segmented_wide(ui: &mut egui::Ui, pal: &Pal, value: &mut u8, options: &[(u8, &str)]) {
    egui::Frame::new().fill(pal.bg).corner_radius(12).stroke(Stroke::new(1.0_f32, pal.line))
        .inner_margin(Margin::same(4)).show(ui, |ui| {
            ui.set_max_width(options.len() as f32 * 100.0);
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                for &(option, label) in options {
                    let selected = *value == option;
                    let button = egui::Button::new(strong(label, 15.0, if selected { pal.on_primary } else { pal.ink }))
                        .fill(if selected { pal.primary } else { Color32::TRANSPARENT })
                        .stroke(Stroke::NONE).corner_radius(9).min_size(Vec2::new(86.0, 38.0));
                    if ui.add(button).clicked() { *value = option; }
                }
            });
        });
}

impl Kiosk {
    /// Orientation of the 10×15 print and its frames (`docs/specs/EVENT_FRAMES.md` § 7 bis).
    fn card_print_format(&mut self, ui: &mut egui::Ui, pal: &Pal) {
        use boothsoft::model::PrintOrientation as O;
        let mut choice = match self.storage.settings.print_orientation { O::Landscape => 0u8, O::Portrait => 1, O::Guest => 2 };
        let mut import = None;
        let mut reset = None;
        card(ui, pal, tr!(fr: "Format du tirage", en: "Print format"), tr!(fr: "10×15 en paysage ou en portrait. La photo est placée dans la zone transparente du cadre, détectée automatiquement.", en: "4×6 in landscape or portrait. The photo goes into the frame's transparent area, found automatically."), |ui| {
            row(ui, pal, tr!(fr: "Orientation", en: "Orientation"), tr!(fr: "« Au choix » : les invités choisissent sur le live (paysage présélectionné).", en: "“Guest's choice”: guests choose on the live view (landscape preselected)."), |ui| {
                segmented_wide(ui, pal, &mut choice, &[(0, tr!(fr: "Paysage", en: "Landscape")), (1, tr!(fr: "Portrait", en: "Portrait")), (2, tr!(fr: "Au choix", en: "Guest's choice"))]);
            });
            for (slot, portrait) in [(0usize, false), (1, true)] {
                let used = match choice { 0 => !portrait, 1 => portrait, _ => true };
                if !used { continue; }
                divider(ui, pal);
                let custom = if portrait { self.storage.settings.frame_portrait_custom } else { self.storage.settings.frame_landscape_custom };
                let theme_has = !portrait || theme::active().is_some_and(|t| t.frame_portrait.is_some());
                let source = if custom { tr!(fr: "PNG importé", en: "imported PNG") } else if theme_has { tr!(fr: "cadre du thème", en: "theme frame") } else { tr!(fr: "sans cadre (photo pleine page)", en: "no frame (full-page photo)") };
                let detail = self.frames[slot].as_ref().map_or_else(|| tr!(fr: "Chargement…", en: "Loading…").to_owned(), |f| {
                    let w = f.window;
                    if f.full_bleed() { tr!(fr: format!("{} · {}×{} · photo sur tout le tirage", source, f.width, f.height), en: format!("{} · {}×{} · photo over the whole print", source, f.width, f.height)) }
                    else { tr!(fr: format!("{} · {}×{} · photo {}×{} à ({}, {})", source, f.width, f.height, w.width, w.height, w.x, w.y), en: format!("{} · {}×{} · photo {}×{} at ({}, {})", source, f.width, f.height, w.width, w.height, w.x, w.y)) }
                });
                row(ui, pal, if portrait { tr!(fr: "Cadre portrait (1200×1800)", en: "Portrait frame (1200×1800)") } else { tr!(fr: "Cadre paysage (1800×1200)", en: "Landscape frame (1800×1200)") }, &detail, |ui| {
                    if custom && btn(ui, pal, tr!(fr: "Cadre du thème", en: "Theme frame"), Btn::Secondary).clicked() { reset = Some(portrait); }
                    if btn(ui, pal, tr!(fr: "Importer un PNG…", en: "Import a PNG…"), Btn::Secondary).clicked() { import = Some(portrait); }
                });
            }
        });
        self.storage.settings.print_orientation = match choice { 0 => O::Landscape, 1 => O::Portrait, _ => O::Guest };
        if let Some(portrait) = reset {
            if portrait { self.storage.settings.frame_portrait_custom = false; } else { self.storage.settings.frame_landscape_custom = false; }
        }
        if let Some(portrait) = import { self.import_frame(portrait); }
    }

    /// Copies a frame PNG into `<data>/frames/`, keeping the 3 previous ones in `frames/history/`.
    fn import_frame(&mut self, portrait: bool) {
        let Some(source) = rfd::FileDialog::new().set_title(if portrait { tr!(fr: "Cadre portrait (PNG 1200×1800)", en: "Portrait frame (PNG 1200×1800)") } else { tr!(fr: "Cadre paysage (PNG 1800×1200)", en: "Landscape frame (PNG 1800×1200)") })
            .add_filter(tr!(fr: "Image PNG", en: "PNG image"), &["png"]).pick_file() else { return };
        let frame = match compositor::load_print_frame(&source) {
            Ok(frame) => frame,
            Err(error) => { self.status = tr!(fr: format!("Cadre refusé : {error}"), en: format!("Frame refused: {error}")); return; }
        };
        if frame.portrait() != portrait {
            self.status = if portrait { tr!(fr: "Ce cadre est en paysage : il faut un PNG plus haut que large.", en: "This frame is landscape: a portrait one must be taller than wide.").into() }
                else { tr!(fr: "Ce cadre est en portrait : il faut un PNG plus large que haut.", en: "This frame is portrait: a landscape one must be wider than tall.").into() };
            return;
        }
        let dir = self.storage.root.join("frames");
        let name = if portrait { "portrait" } else { "landscape" };
        let target = dir.join(format!("{name}.png"));
        let result = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(dir.join("history"))?;
            if target.is_file() {
                let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                std::fs::rename(&target, dir.join("history").join(format!("{name}-{stamp}.png")))?;
                // Three previous frames kept per orientation.
                let mut old: Vec<_> = std::fs::read_dir(dir.join("history"))?.filter_map(Result::ok).map(|e| e.path())
                    .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&format!("{name}-")))).collect();
                old.sort();
                while old.len() > 3 { let _ = std::fs::remove_file(old.remove(0)); }
            }
            std::fs::copy(&source, &target).map(|_| ())
        })();
        match result {
            Ok(()) => {
                if portrait { self.storage.settings.frame_portrait_custom = true; } else { self.storage.settings.frame_landscape_custom = true; }
                self.frame_path_loaded.clear();
                let w = frame.window;
                self.status = tr!(fr: format!("Cadre importé : photo {}×{} à ({}, {})", w.width, w.height, w.x, w.y), en: format!("Frame imported: photo {}×{} at ({}, {})", w.width, w.height, w.x, w.y));
                boothsoft::log::info(format!("Frame imported ({name}): window {}x{} at ({}, {})", w.width, w.height, w.x, w.y));
            }
            Err(error) => self.status = tr!(fr: format!("Import du cadre impossible : {error}"), en: format!("Cannot import the frame: {error}")),
        }
    }
}
