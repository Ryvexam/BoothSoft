// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Video guest book screens: microphone test, recording, playback.
//! Spec: `docs/specs/GUEST_BOOK.md`. The microphone is open only while
//! [`GuestBook::mic`] exists, i.e. on the test and recording screens.
use super::{ui, Kiosk, WAKE};
use super::ui::{Icon, Kind, Palette};
use boothsoft::{audio::{self, MicStream}, camera::CameraCommand, effects::PhotoFilter,
    guestbook::{self, Message, Player, RecordResult, Recording}, model::Screen};
use eframe::egui::{self, Align2, Color32, CornerRadius, Pos2, Rect, Stroke, Vec2};
use std::{collections::VecDeque, sync::Arc, time::{Duration, Instant}};

/// Countdown before recording starts.
const PRE_ROLL: Duration = Duration::from_secs(3);
/// "Terminer" is inactive this long (a message is at least 2 s).
const MIN_LENGTH: Duration = Duration::from_secs(2);
/// Microphone test left alone: back to the live view.
const MIC_IDLE: Duration = Duration::from_secs(45);
/// Level meter: 15 redraws per second at most.
const METER_TICK: Duration = Duration::from_millis(66);

#[derive(Default)]
pub(super) struct GuestBook {
    mic: Option<MicStream>,
    mic_error: Option<String>,
    mic_retry: Option<Instant>,
    /// Displayed meter (0…1), falling back at 24 dB/s.
    meter: f32,
    meter_at: Option<Instant>,
    /// Last time the voice was clearly heard (> −35 dB), any sound (> −50 dB), saturation (> −1 dB).
    heard_at: Option<Instant>,
    sound_at: Option<Instant>,
    clips: VecDeque<Instant>,
    /// First time "On vous entend bien !" appeared (one-off highlight of the button).
    first_heard: Option<Instant>,
    without_sound: bool,
    countdown: Option<Instant>,
    recording: Option<Recording>,
    stopping: bool,
    message: Option<Message>,
    player: Option<Player>,
    message_texture: Option<egui::TextureHandle>,
}

impl GuestBook {
    pub(super) fn is_recording(&self) -> bool { self.recording.is_some() }

    fn level(&mut self, now: Instant) {
        let Some(mic) = self.mic.as_ref() else { self.meter = 0.0; return };
        let peak = mic.take_peak();
        let db = audio::db(peak);
        if db > -35.0 { self.heard_at = Some(now); self.first_heard.get_or_insert(now); }
        if db > -50.0 { self.sound_at = Some(now); }
        if db > -1.0 { self.clips.push_back(now); }
        while self.clips.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(2)) { self.clips.pop_front(); }
        let dt = self.meter_at.map_or(0.0, |t| now.duration_since(t).as_secs_f32());
        self.meter_at = Some(now);
        self.meter = audio::meter(peak).max(self.meter - dt * 24.0 / 60.0);
    }

    fn feedback(&self, now: Instant) -> (&'static str, bool) {
        let recent = |t: Option<Instant>| t.is_some_and(|t| now.duration_since(t) < Duration::from_secs(2));
        if self.clips.len() >= 3 { ("Un peu moins fort, c’est parfait.", true) }
        else if recent(self.heard_at) { ("On vous entend bien !", true) }
        else if !recent(self.sound_at) { ("Nous ne vous entendons pas encore : parlez près de l’écran.", false) }
        else { ("Parlez un peu plus fort…", false) }
    }
}

impl Kiosk {
    /// Name shown to guests: the setting, else the current event.
    pub(super) fn recipient(&self) -> String {
        let name = self.storage.settings.guestbook_recipient.trim();
        if name.is_empty() { self.current_event().name } else { name.to_owned() }
    }

    fn wake(ctx: &egui::Context) -> boothsoft::camera::WakeUi {
        let ctx = ctx.clone();
        Arc::new(move || ctx.request_repaint_after(WAKE))
    }

    fn open_mic(&mut self) {
        let settings = &self.storage.settings;
        let gb = self.guestbook.get_or_insert_with(GuestBook::default);
        match MicStream::open(&settings.guestbook_mic, settings.guestbook_gain) {
            Ok(mic) => { gb.mic = Some(mic); gb.mic_error = None; }
            Err(error) => {
                if gb.mic_error.is_none() { boothsoft::log::info(format!("Guest book: microphone unavailable: {error}")); }
                gb.mic_error = Some(error);
            }
        }
        gb.mic_retry = Some(Instant::now());
    }

    /// Shutter in guest-book mode: microphone test (no countdown).
    pub(super) fn open_mic_check(&mut self) {
        self.guestbook = Some(GuestBook::default());
        self.open_mic();
        self.screen = Screen::MicCheck;
        self.last_touch = Instant::now();
        if !self.storage.settings.demo_mode { self.camera.send(CameraCommand::SetPreviewEnabled(true)); }
        boothsoft::log::info("Guest book: microphone test");
    }

    /// Live view without filter (messages are recorded without one).
    fn paint_live_plain(&self, ui: &egui::Ui, rect: Rect, mirror: bool, radius: f32) {
        let painter = ui.painter();
        if self.live.as_ref().is_some_and(|live| live.paint(painter, rect, mirror, PhotoFilter::Original, radius, super::live_gl::Shade::default())) {
            return;
        }
        painter.rect_filled(rect, CornerRadius::same(radius as u8), Color32::from_rgb(18, 20, 19));
        let texture = if self.screen == Screen::MessageReview { self.guestbook.as_ref().and_then(|g| g.message_texture.as_ref()) } else { self.preview_filtered.as_ref() };
        if let Some(texture) = texture { ui::image_cover(painter, texture, rect, mirror, Color32::WHITE); }
    }

    fn paint_meter(painter: &egui::Painter, rect: Rect, value: f32, segments: usize, pal: &Palette) {
        let gap = rect.width() * 0.012;
        let w = (rect.width() - gap * (segments - 1) as f32) / segments as f32;
        let lit = (value * segments as f32).round() as usize;
        for i in 0..segments {
            let r = Rect::from_min_size(Pos2::new(rect.left() + i as f32 * (w + gap), rect.top()), Vec2::new(w, rect.height()));
            let hot = i + 3 >= segments;
            let color = if i < lit { if hot { Color32::from_rgb(255, 150, 40) } else { pal.accent } } else { Color32::from_white_alpha(40) };
            painter.rect_filled(r, CornerRadius::same((w * 0.3).min(6.0) as u8), color);
        }
    }

    pub(super) fn render_mic_check(&mut self, ui: &mut egui::Ui) {
        let now = Instant::now();
        let rect = ui.max_rect();
        let pal = self.palette();
        let scale = (rect.height() / 800.0).clamp(0.8, 1.6);
        if self.guestbook.is_none() { self.open_mic_check(); }
        // Microphone gone or never there: retry every 2 s.
        let failed = self.guestbook.as_ref().is_some_and(|g| g.mic.as_ref().is_none_or(MicStream::failed));
        if failed && self.guestbook.as_ref().and_then(|g| g.mic_retry).is_none_or(|t| t.elapsed() > Duration::from_secs(2)) {
            if let Some(gb) = self.guestbook.as_mut() { gb.mic = None; }
            self.open_mic();
        }
        let failed = self.guestbook.as_ref().is_some_and(|g| g.mic.as_ref().is_none_or(MicStream::failed));
        if let Some(gb) = self.guestbook.as_mut() { gb.level(now); }
        let recipient = self.recipient();
        let painter = ui.painter().clone();
        self.paint_backdrop(&painter, rect, &pal, 200);
        let enter = ui::appear(ui.ctx(), "screen-miccheck", ui::ENTER);

        // Title and back button.
        let top = rect.top() + 64.0 * scale;
        painter.text(Pos2::new(rect.center().x, top), Align2::CENTER_CENTER, format!("Un message pour {recipient}"), ui::display(38.0 * scale), pal.primary);
        let back = Rect::from_center_size(Pos2::new(rect.left() + 62.0 * scale, top), Vec2::splat(58.0 * scale));
        if ui::button(ui, back, "gb-back", "", Some(Icon::Close), Kind::Glass, &pal) {
            self.enter_ready();
            return;
        }

        // Live (mirror) on the left, 16:9.
        let live_w = rect.width() * 0.56;
        let live = Rect::from_min_size(Pos2::new(rect.left() + 56.0 * scale, top + 56.0 * scale), Vec2::new(live_w, live_w * 9.0 / 16.0))
            .translate(Vec2::new(-(1.0 - enter) * 40.0 * scale, 0.0));
        ui::soft_shadow(&painter, live, 18, 30, 10, 90);
        self.paint_live_plain(ui, live, true, 18.0 * scale);

        // Meter column on the right.
        let col = Rect::from_min_max(Pos2::new(live.right() + 48.0 * scale, live.top()), Pos2::new(rect.right() - 56.0 * scale, live.bottom()));
        let mut y = col.top() + 20.0 * scale;
        painter.text(Pos2::new(col.left(), y), Align2::LEFT_TOP, "Dites quelque chose !", ui::display(34.0 * scale), pal.fg);
        y += 70.0 * scale;
        let gb = self.guestbook.as_ref();
        let highlight = if failed {
            ui::icon(&painter, Pos2::new(col.left() + 22.0 * scale, y + 22.0 * scale), 40.0 * scale, ui::alpha(pal.fg, 150), Icon::Mic);
            painter.text(Pos2::new(col.left() + 60.0 * scale, y + 22.0 * scale), Align2::LEFT_CENTER, "Le micro ne répond pas.", ui::bold(22.0 * scale), pal.fg);
            false
        } else {
            ui::icon(&painter, Pos2::new(col.left() + 20.0 * scale, y + 22.0 * scale), 40.0 * scale, pal.accent, Icon::Mic);
            let bar = Rect::from_min_max(Pos2::new(col.left() + 56.0 * scale, y + 6.0 * scale), Pos2::new(col.right(), y + 38.0 * scale));
            Self::paint_meter(&painter, bar, gb.map_or(0.0, |g| g.meter), 16, &pal);
            let (text, good) = gb.map_or(("", false), |g| g.feedback(now));
            let galley = painter.layout(text.to_owned(), ui::bold(21.0 * scale), if good { pal.accent } else { pal.fg }, col.width() - 56.0 * scale);
            painter.galley(Pos2::new(col.left() + 56.0 * scale, y + 54.0 * scale), galley, pal.fg);
            good
        };

        // Privacy notice (D10) and the main button.
        let max = self.storage.settings.guestbook_max_seconds;
        let notice = format!("Votre message vidéo ({max} s au plus) sera enregistré et remis à {recipient}.");
        let galley = painter.layout(notice, ui::body(19.0 * scale), ui::alpha(pal.fg, 220), rect.width() - 112.0 * scale);
        painter.galley(Pos2::new(rect.left() + 56.0 * scale, live.bottom() + 30.0 * scale), galley, pal.fg);
        let label = if failed { "Enregistrer sans le son" } else { "Commencer l’enregistrement" };
        let height = 80.0 * scale;
        let w = ui::button_width(ui, label, height, true).max(340.0 * scale);
        let button = Rect::from_min_size(Pos2::new(rect.right() - 56.0 * scale - w, rect.bottom() - 48.0 * scale - height), Vec2::new(w, height))
            .translate(Vec2::new(0.0, (1.0 - enter) * 60.0 * scale));
        // One-off ring when the voice is first heard (300 ms), then still.
        if highlight {
            let t = self.guestbook.as_ref().and_then(|g| g.first_heard).map_or(1.0, |t| (now.duration_since(t).as_secs_f32() / 0.6).min(1.0));
            painter.rect_stroke(button.expand(4.0 + 10.0 * (1.0 - t)), CornerRadius::same(255), Stroke::new(3.0_f32, ui::alpha(pal.accent, (255.0 * (1.0 - t * 0.5)) as u8)), egui::StrokeKind::Outside);
        }
        if ui::button(ui, button, "gb-start", label, Some(if failed { Icon::Camera } else { Icon::Mic }), Kind::Primary, &pal) {
            if let Some(gb) = self.guestbook.as_mut() {
                gb.without_sound = failed;
                if failed { gb.mic = None; }
                gb.countdown = Some(Instant::now());
            }
            self.screen = Screen::Recording;
            self.last_touch = Instant::now();
            return;
        }
        if self.last_touch.elapsed() > MIC_IDLE { self.enter_ready(); return; }
        ui.ctx().request_repaint_after(METER_TICK);
    }

    fn begin_recording(&mut self, ctx: &egui::Context) {
        let dir = guestbook::folder(&self.storage.root.join("sessions").join(self.current_event().folder));
        let Some(gb) = self.guestbook.as_mut() else { return };
        let tap = if gb.without_sound { None } else { gb.mic.as_ref().map(|m| (m.start_recording(), m.device.clone())) };
        match Recording::start(&dir, tap, Self::wake(ctx)) {
            Ok((recording, frames)) => {
                gb.recording = Some(recording);
                self.camera.send(CameraCommand::Record(Some(frames)));
                boothsoft::log::info("Guest book: recording");
            }
            Err(error) => self.message_failed(&error.to_string()),
        }
    }

    fn stop_recording(&mut self) {
        let Some(gb) = self.guestbook.as_mut() else { return };
        if gb.stopping { return; }
        gb.stopping = true;
        self.camera.send(CameraCommand::Record(None));
        if let Some(mic) = gb.mic.as_ref() { mic.stop_recording(); }
        if let Some(recording) = gb.recording.as_ref() { recording.stop(); }
    }

    fn message_failed(&mut self, error: &str) {
        boothsoft::log::info(format!("Guest book: message lost: {error}"));
        self.camera.send(CameraCommand::Record(None));
        self.enter_ready();
        self.notice = Some(("Le message n’a pas pu être enregistré.".into(), Instant::now()));
    }

    pub(super) fn render_recording(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let now = Instant::now();
        let rect = ui.max_rect();
        let pal = self.palette();
        let scale = (rect.height() / 800.0).clamp(0.8, 1.6);
        let Some(countdown) = self.guestbook.as_ref().and_then(|g| g.countdown) else { self.enter_ready(); return };
        let max = Duration::from_secs(self.storage.settings.guestbook_max_seconds.into());
        // Pre-roll over, start writing.
        if countdown.elapsed() >= PRE_ROLL && self.guestbook.as_ref().is_some_and(|g| g.recording.is_none() && !g.stopping) {
            self.begin_recording(ctx);
        }
        // Result of the recording thread.
        let result = self.guestbook.as_ref().and_then(|g| g.recording.as_ref()).and_then(|r| r.done.try_recv().ok());
        match result {
            Some(RecordResult::Saved(message)) => { self.show_message(ctx, message); return; }
            Some(RecordResult::TooShort) => { self.message_failed("too short"); return; }
            Some(RecordResult::Failed(error)) => { self.message_failed(&error); return; }
            None => {}
        }
        if let Some(gb) = self.guestbook.as_mut() { gb.level(now); }
        let started = self.guestbook.as_ref().and_then(|g| g.recording.as_ref()).map(|r| r.started);
        let elapsed = started.map_or(Duration::ZERO, |s| s.elapsed());
        if started.is_some() && elapsed >= max { self.stop_recording(); }
        let stopping = self.guestbook.as_ref().is_some_and(|g| g.stopping);

        self.paint_live_plain(ui, rect, true, 0.0);
        let painter = ui.painter().clone();
        ui::gradient(&painter, Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + 140.0 * scale)), Color32::from_black_alpha(110), Color32::TRANSPARENT);
        ui::gradient(&painter, Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - 200.0 * scale), rect.max), Color32::TRANSPARENT, Color32::from_black_alpha(150));

        if started.is_none() && !stopping {
            // 3 s countdown with the same big numerals as photos.
            let left = PRE_ROLL.saturating_sub(countdown.elapsed());
            let n = left.as_secs() + 1;
            let center = rect.center() - Vec2::new(0.0, 20.0 * scale);
            let radius = rect.height().min(rect.width()) * 0.18;
            painter.circle_filled(center, radius, Color32::from_black_alpha(95));
            if let Some(texture) = self.numeral.get(ctx, &n.to_string(), radius * 1.45) { ui::numeral(&painter, texture, center, 1.0); }
            ui::shadowed_text(&painter, center + Vec2::new(0.0, radius + 50.0 * scale), Align2::CENTER_CENTER,
                "Regardez l’écran et parlez !", ui::bold(28.0 * scale), Color32::WHITE);
            ctx.request_repaint_after(Duration::from_millis(1_000 - u64::from(left.subsec_millis()) % 1_000).min(left.max(Duration::from_millis(16))));
            return;
        }

        // ● REC pill: the only looping animation in the project; the screen
        // already redraws for every live frame, so it costs nothing extra.
        let pulse = 0.55 + 0.45 * (now.duration_since(started.unwrap_or(now)).as_secs_f32() * std::f32::consts::PI).cos().abs();
        let rec = painter.layout_no_wrap("REC".into(), ui::bold(20.0 * scale), Color32::WHITE);
        let pill = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 52.0 * scale), rec.size() + Vec2::new(64.0, 22.0) * scale);
        painter.rect_filled(pill, CornerRadius::same(255), Color32::from_black_alpha(140));
        painter.circle_filled(Pos2::new(pill.left() + 24.0 * scale, pill.center().y), 8.0 * scale, Color32::from_rgba_unmultiplied(235, 50, 50, (255.0 * pulse) as u8));
        painter.galley(Pos2::new(pill.left() + 40.0 * scale, pill.center().y - rec.size().y / 2.0), rec, Color32::WHITE);

        // Terminer with a progress ring and the time left.
        let center = Pos2::new(rect.center().x, rect.bottom() - 110.0 * scale);
        let radius = 62.0 * scale;
        let progress = (elapsed.as_secs_f32() / max.as_secs_f32()).min(1.0);
        painter.circle_filled(center, radius + 10.0 * scale, Color32::from_black_alpha(110));
        ui::arc(&painter, center, radius + 4.0 * scale, -std::f32::consts::FRAC_PI_2, -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * (1.0 - progress).max(0.001),
            Stroke::new(7.0 * scale, pal.accent));
        let left = max.saturating_sub(elapsed).as_secs() + u64::from(!stopping);
        let can_stop = elapsed >= MIN_LENGTH && !stopping;
        let hit = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
        let response = ui.interact(hit, ui.id().with("gb-stop"), egui::Sense::click());
        let core = if can_stop { Color32::from_rgb(225, 45, 45) } else { Color32::from_rgba_unmultiplied(225, 45, 45, 110) };
        painter.circle_filled(center, radius * 0.72, core);
        painter.rect_filled(Rect::from_center_size(center - Vec2::new(0.0, 8.0 * scale), Vec2::splat(radius * 0.42)), CornerRadius::same(4), Color32::WHITE);
        painter.text(center + Vec2::new(0.0, radius * 0.34), Align2::CENTER_CENTER, format!("0:{:02}", left.min(59)), ui::bold(17.0 * scale), Color32::WHITE);
        let caption = if stopping { "C’est dans la boîte !" } else { "Terminer" };
        ui::shadowed_text(&painter, center + Vec2::new(0.0, radius + 34.0 * scale), Align2::CENTER_CENTER, caption, ui::bold(19.0 * scale), Color32::WHITE);
        if response.clicked() && can_stop { self.stop_recording(); }

        // Compact meter bottom left.
        if self.guestbook.as_ref().is_some_and(|g| g.mic.is_some() && !g.without_sound) {
            let bar = Rect::from_min_size(Pos2::new(rect.left() + 90.0 * scale, rect.bottom() - 126.0 * scale), Vec2::new(200.0 * scale, 26.0 * scale));
            ui::icon(&painter, Pos2::new(rect.left() + 60.0 * scale, bar.center().y), 32.0 * scale, Color32::WHITE, Icon::Mic);
            Self::paint_meter(&painter, bar, self.guestbook.as_ref().map_or(0.0, |g| g.meter), 8, &pal);
        }
        self.last_touch = now;
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn show_message(&mut self, ctx: &egui::Context, message: Message) {
        if let Some(gb) = self.guestbook.as_mut() {
            gb.recording = None;
            gb.stopping = false;
            gb.mic = None; // microphone closed while the message plays
            gb.message = Some(message);
        }
        if !self.storage.settings.demo_mode { self.camera.send(CameraCommand::SetPreviewEnabled(false)); }
        self.screen = Screen::MessageReview;
        self.last_touch = Instant::now();
        self.play_message(ctx);
    }

    fn play_message(&mut self, ctx: &egui::Context) {
        let volume = self.storage.settings.guestbook_playback_volume;
        let wake = Self::wake(ctx);
        let Some(gb) = self.guestbook.as_mut() else { return };
        let Some(message) = gb.message.as_ref() else { return };
        gb.player = None;
        match Player::start(&message.video, volume, wake) {
            Ok(player) => gb.player = Some(player),
            Err(error) => boothsoft::log::info(format!("Guest book: playback failed: {error}")),
        }
    }

    /// Playback frames go through the live view's texture (same GPU path).
    pub(super) fn feed_playback(&mut self, ctx: &egui::Context) {
        let Some(frame) = self.guestbook.as_ref().and_then(|g| g.player.as_ref()).and_then(Player::take_frame) else { return };
        match self.live.as_mut() {
            Some(live) => live.submit(frame),
            None => {
                let rgb = image::DynamicImage::ImageRgba8(frame).into_rgb8();
                let mut small = boothsoft::compositor::fit_within(&rgb, boothsoft::camera::PREVIEW_WIDTH);
                let image = small.get_or_insert(rgb);
                if let Some(gb) = self.guestbook.as_mut() { super::update_texture(ctx, &mut gb.message_texture, "message-frame", image); }
            }
        }
    }

    pub(super) fn render_message_review(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let rect = ui.max_rect();
        let pal = self.palette();
        let scale = (rect.height() / 800.0).clamp(0.8, 1.6);
        let painter = ui.painter().clone();
        self.paint_backdrop(&painter, rect, &pal, 215);
        let bar_h = 128.0 * scale;
        let area = Rect::from_min_max(rect.min + Vec2::new(40.0, 30.0) * scale, Pos2::new(rect.right() - 40.0 * scale, rect.bottom() - bar_h));
        let size = self.live.as_ref().and_then(super::live_gl::LiveView::frame_size).map_or(Vec2::new(16.0, 9.0), |[w, h]| Vec2::new(w as f32, h as f32));
        let video = ui::fit(size, area.shrink(10.0 * scale));
        let enter = ui::appear(ctx, "screen-message", 0.32);
        let video = Rect::from_center_size(video.center(), video.size() * (0.93 + 0.07 * enter));
        ui::soft_shadow(&painter, video, 16, 40, 16, 120);
        // As recorded: not mirrored (D7).
        self.paint_live_plain(ui, video, false, 16.0 * scale);
        let (finished, progress) = self.guestbook.as_ref().and_then(|g| g.player.as_ref())
            .map_or((true, 1.0), |p| (p.finished(), (p.elapsed() / p.duration.max(0.1)) as f32));
        // Thin progress line under the video.
        let line = Rect::from_min_size(Pos2::new(video.left(), video.bottom() + 10.0 * scale), Vec2::new(video.width(), 4.0 * scale));
        painter.rect_filled(line, CornerRadius::same(2), Color32::from_white_alpha(40));
        painter.rect_filled(Rect::from_min_size(line.min, Vec2::new(line.width() * progress.min(1.0), line.height())), CornerRadius::same(2), pal.accent);

        // Action bar: title left, Recommencer · (Revoir) · Garder.
        let bar = Rect::from_min_max(Pos2::new(rect.left() + 48.0 * scale, rect.bottom() - bar_h), Pos2::new(rect.right() - 48.0 * scale, rect.bottom()))
            .translate(Vec2::new(0.0, (1.0 - enter) * bar_h));
        let cy = bar.center().y;
        let height = 76.0 * scale;
        let gap = 14.0 * scale;
        let mut actions = vec![("Garder", Icon::Check, Kind::Primary, "gb-keep")];
        if finished { actions.push(("Revoir", Icon::Play, Kind::Glass, "gb-replay")); }
        actions.push(("Recommencer", Icon::Retake, Kind::Glass, "gb-redo"));
        let mut right = bar.right();
        let mut pressed = None;
        for (label, icon, kind, id) in actions {
            let mut w = ui::button_width(ui, label, height, true);
            if kind == Kind::Primary { w = w.max(220.0 * scale); }
            let r = Rect::from_min_size(Pos2::new(right - w, cy - height / 2.0), Vec2::new(w, height));
            right -= w + gap;
            if ui::button(ui, r, id, label, Some(icon), kind, &pal) { pressed = Some(id); }
        }
        let timeout = Duration::from_secs(self.storage.settings.photo_timeout_seconds.into());
        let left = timeout.saturating_sub(self.last_touch.elapsed());
        let title_w = right - bar.left();
        if title_w > 200.0 * scale {
            let title = painter.layout_no_wrap("Votre message".into(), ui::display(38.0 * scale), Color32::WHITE);
            let th = title.size().y;
            painter.galley(Pos2::new(bar.left(), cy - th / 2.0 - 14.0 * scale), title, Color32::WHITE);
            let sub = format!("Gardé automatiquement dans {} s", left.as_secs() + 1);
            painter.text(Pos2::new(bar.left(), cy + th / 2.0 - 4.0 * scale), Align2::LEFT_CENTER, sub, ui::body(18.0 * scale), Color32::from_white_alpha(210));
        }
        match pressed {
            Some("gb-keep") => { self.keep_message(ctx); return; }
            Some("gb-replay") => { self.play_message(ctx); self.last_touch = Instant::now(); }
            Some("gb-redo") => {
                if let Some(message) = self.guestbook.as_mut().and_then(|g| { g.player = None; g.message.take() }) {
                    let _ = message.delete();
                    boothsoft::log::info(format!("Guest book: {} discarded by the guest", message.stem()));
                }
                if let Some(live) = self.live.as_mut() { live.clear(); }
                self.open_mic_check();
                return;
            }
            _ => {}
        }
        // A recorded message is never lost by inaction: kept after the timeout.
        if left.is_zero() { self.keep_message(ctx); return; }
        ctx.request_repaint_after(Duration::from_millis(1_000 - u64::from(left.subsec_millis()) % 1_000).min(Duration::from_millis(250)));
    }

    fn keep_message(&mut self, ctx: &egui::Context) {
        let thumb = self.guestbook.as_ref().and_then(|g| g.message.as_ref()).map(Message::thumbnail);
        if let Some(message) = self.guestbook.as_ref().and_then(|g| g.message.as_ref()) {
            boothsoft::log::info(format!("Guest book: {} kept ({:.1} s)", message.stem(), message.info.seconds));
        }
        self.show_thanks(0);
        self.thanks_message = Some(self.recipient());
        self.thanks_photo = thumb.and_then(|path| super::load_texture(ctx, "thanks-message", &path, 960, 720));
    }
}
