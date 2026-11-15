// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Burst mode ("rafale"): one countdown, several quick photos, then a picker
//! where guests choose which photos to print and how many copies of each.
//!
//! Resource notes: burst photos are encoded one by one by the photo worker
//! while the next pose is shot; the picker keeps only small thumbnails
//! (`BURST_THUMB_EDGE`) plus one full-size texture for the selected photo,
//! decoded from disk when the selection changes.
use super::*;

/// One prompt per photo of a burst, so every shot gets a different pose.
const POSES: [&str; 10] = [
    "Souriez !", "Changez de pose !", "Grimace !", "Tous ensemble !", "Un câlin !",
    "Regard de star !", "Fou rire !", "Bisou !", "Pose de super-héros !", "Encore une !",
];

impl Kiosk {
    /// Live view between burst photos: shot counter, pose prompt, progress dots.
    pub(super) fn render_burst_shooting(&mut self, ui: &mut egui::Ui, rect: Rect, scale: f32, pal: &Palette, taken: u8, total: u8) {
        let gpu = self.draw_live(ui, rect, self.live_shade(rect, scale, false, 0.0));
        let painter = ui.painter().clone();
        self.paint_print_guide(&painter, rect, scale, gpu);
        let waiting_next = self.burst.as_ref().is_some_and(|b| b.next_at.is_some());
        let current = if waiting_next { taken + 1 } else { taken }.min(total);
        let pose = if current == total { "Dernière !" } else { POSES[usize::from(current - 1) % POSES.len()] };

        let counter = format!("{current} / {total}");
        let galley = painter.layout_no_wrap(counter, ui::display(54.0 * scale), Color32::WHITE);
        let badge = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 86.0 * scale), galley.size() + Vec2::new(60.0, 22.0) * scale);
        painter.rect_filled(badge, CornerRadius::same(255), Color32::from_black_alpha(130));
        painter.galley(badge.center() - galley.size() / 2.0, galley, Color32::WHITE);

        // Each new pose prompt pops up from below.
        let pop = ui::appear(ui.ctx(), ("burst-pose", current), 0.22);
        let galley = painter.layout_no_wrap(pose.to_owned(), ui::bold(34.0 * scale), pal.on_accent);
        let pill = Rect::from_center_size(Pos2::new(rect.center().x, rect.bottom() - 150.0 * scale + (1.0 - pop) * 34.0 * scale), galley.size() + Vec2::new(64.0, 26.0) * scale);
        painter.rect_filled(pill, CornerRadius::same(255), pal.accent);
        painter.galley(pill.center() - galley.size() / 2.0, galley, pal.on_accent);

        let dot = 9.0 * scale;
        let gap = 14.0 * scale;
        let width = f32::from(total) * (2.0 * dot) + f32::from(total - 1) * gap;
        let y = rect.bottom() - 82.0 * scale;
        for i in 0..total {
            let center = Pos2::new(rect.center().x - width / 2.0 + dot + f32::from(i) * (2.0 * dot + gap), y);
            if i < taken {
                painter.circle_filled(center, dot, Color32::WHITE);
            } else {
                painter.circle_stroke(center, dot - 1.0, Stroke::new(2.0_f32 * scale, Color32::from_white_alpha(170)));
            }
        }
    }

    /// Full-size texture of the selected burst photo (decoded once per selection).
    fn ensure_big(&mut self, ctx: &egui::Context) {
        let Some(burst) = self.burst.as_mut() else { return; };
        if burst.big.as_ref().is_some_and(|(index, _)| *index == burst.selected) { return; }
        let Some(shot) = burst.shots.get(burst.selected) else { return; };
        // Drop the previous one first: never two full-size textures at once.
        burst.big = None;
        if let Ok(image) = boothsoft::image_io::read_print_jpeg(&shot.framed) {
            burst.big = Some((burst.selected, texture(ctx, "burst-big", &image)));
        }
    }

    /// Copies the event can still print in total (`u32::MAX` without quota).
    fn burst_budget(&self) -> u32 {
        if !self.storage.settings.print_enabled { return 0; }
        self.prints_left().map_or(u32::MAX, |(left, _)| left)
    }

    pub(super) fn render_pick(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.ensure_big(&ctx);
        let rect = ui.max_rect();
        let pal = self.palette();
        let scale = (rect.height() / 800.0).clamp(0.8, 1.6);
        let painter = ui.painter().clone();
        match self.review_backdrop.as_ref() {
            Some(backdrop) => ui::image_cover(&painter, backdrop, rect, false, Color32::WHITE),
            None => { painter.rect_filled(rect, 0.0, ui::shade(pal.primary, 0.35)); }
        }
        painter.rect_filled(rect, 0.0, Color32::from_black_alpha(165));

        let printing = self.storage.settings.print_enabled && self.copies_available() > 0;
        let budget = self.burst_budget();
        let max_each = self.storage.settings.max_copies.clamp(1, 10);
        let Some(burst) = self.burst.as_ref() else { self.finish_session(); return; };
        let count = burst.shots.len();
        let selected = burst.selected.min(count.saturating_sub(1));
        let total_copies = burst.copies();
        let current_copies = burst.shots.get(selected).map_or(0, |s| s.copies);
        // Cheap handle clones (Arc) so `self` stays free while drawing.
        let big = burst.big.as_ref().map(|(_, texture)| texture.clone());
        let thumbs: Vec<(TextureHandle, u8)> = burst.shots.iter().map(|s| (s.thumb.clone(), s.copies)).collect();
        let confirm = burst.confirm;
        let (drag_dx, slide_dir) = (burst.drag_dx, burst.slide_dir);
        // A newly selected photo slides in from the side it came from.
        let arrive = ui::appear(ui.ctx(), ("pick-photo", selected), ui::ENTER);

        // Header.
        let margin = 48.0 * scale;
        let title = if printing { "Choisissez vos photos" } else { "Vos photos" };
        painter.text(Pos2::new(rect.left() + margin, rect.top() + 34.0 * scale), Align2::LEFT_TOP, title, ui::display(38.0 * scale), Color32::WHITE);
        let hint = if printing { "Faites défiler, puis choisissez le nombre de tirages de chaque photo." } else { "Faites défiler pour revoir la rafale." };
        painter.text(Pos2::new(rect.left() + margin, rect.top() + 86.0 * scale), Align2::LEFT_TOP, hint, ui::body(18.0 * scale), Color32::from_white_alpha(200));

        // Layout: big photo, copies stepper, filmstrip, action bar.
        let bar_h = 104.0 * scale;
        let strip_h = 96.0 * scale;
        let stepper_h = if printing { 76.0 * scale } else { 0.0 };
        let big_area = Rect::from_min_max(
            Pos2::new(rect.left() + 130.0 * scale, rect.top() + 126.0 * scale),
            Pos2::new(rect.right() - 130.0 * scale, rect.bottom() - bar_h - strip_h - stepper_h - 20.0 * scale));

        let mut select: Option<usize> = None;
        if let Some(big) = big.as_ref() {
            let photo = ui::fit(big.size_vec2(), big_area)
                .translate(Vec2::new(drag_dx + slide_dir * (1.0 - arrive) * 220.0 * scale, 0.0));
            let paper = photo.expand(6.0 * scale);
            ui::soft_shadow(&painter, paper, 4, 36, 14, 120);
            painter.rect_filled(paper, CornerRadius::same(4), Color32::WHITE);
            painter.image(big.id(), photo, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            if current_copies > 0 {
                let label = if current_copies == 1 { "1 tirage".to_owned() } else { format!("{current_copies} tirages") };
                let galley = painter.layout_no_wrap(label, ui::bold(17.0 * scale), pal.on_accent);
                let chip = Rect::from_min_size(paper.left_top() + Vec2::new(14.0, 14.0) * scale, galley.size() + Vec2::new(46.0, 14.0) * scale);
                painter.rect_filled(chip, CornerRadius::same(255), pal.accent);
                ui::icon(&painter, Pos2::new(chip.left() + 20.0 * scale, chip.center().y), 15.0 * scale, pal.on_accent, Icon::Print);
                painter.galley(Pos2::new(chip.left() + 34.0 * scale, chip.center().y - galley.size().y / 2.0), galley, pal.on_accent);
            }
            // Swipe left / right on the photo.
            let swipe = ui.interact(paper, ui.id().with("pick-swipe"), egui::Sense::drag());
            // The photo follows the finger; past 60 px it flips to the neighbour.
            let state = self.burst.as_mut().expect("burst");
            if swipe.drag_started() { state.swipe_from = swipe.interact_pointer_pos().map(|p| p.x); state.drag_dx = 0.0; }
            if swipe.dragged() {
                let at_edge = (state.drag_dx > 0.0 && selected == 0) || (state.drag_dx < 0.0 && selected + 1 >= count);
                // Rubber band at both ends of the carousel.
                state.drag_dx += swipe.drag_delta().x * if at_edge { 0.3 } else { 1.0 };
            }
            if swipe.drag_stopped() {
                let moved = state.drag_dx;
                state.swipe_from = None;
                state.drag_dx = 0.0;
                if moved > 60.0 * scale && selected > 0 { select = Some(selected - 1); }
                if moved < -60.0 * scale && selected + 1 < count { select = Some(selected + 1); }
            }
        }
        // Carousel arrows.
        let arrow = 64.0 * scale;
        let prev = Rect::from_center_size(Pos2::new(rect.left() + 64.0 * scale, big_area.center().y), Vec2::splat(arrow));
        let next = Rect::from_center_size(Pos2::new(rect.right() - 64.0 * scale, big_area.center().y), Vec2::splat(arrow));
        if selected > 0 && arrow_button(ui, &painter, prev, "pick-prev", true) { select = Some(selected - 1); }
        if selected + 1 < count && arrow_button(ui, &painter, next, "pick-next", false) { select = Some(selected + 1); }

        // Copies of the selected photo: [−] N tirages [+].
        let mut delta: i8 = 0;
        if printing {
            let cy = big_area.bottom() + 20.0 * scale + stepper_h / 2.0;
            let size = 56.0 * scale;
            let minus = Rect::from_center_size(Pos2::new(rect.center().x - 150.0 * scale, cy), Vec2::splat(size));
            let plus = Rect::from_center_size(Pos2::new(rect.center().x + 150.0 * scale, cy), Vec2::splat(size));
            let can_add = current_copies < max_each && total_copies < budget;
            if round_step(ui, &painter, minus, "pick-minus", false, current_copies > 0, &pal) { delta = -1; }
            if round_step(ui, &painter, plus, "pick-plus", true, can_add, &pal) { delta = 1; }
            let label = match current_copies { 0 => "Pas imprimée".to_owned(), 1 => "1 tirage".to_owned(), n => format!("{n} tirages") };
            // The count hops when it changes.
            let hop = ui::appear(ui.ctx(), ("pick-copies", selected, current_copies), 0.18);
            painter.text(Pos2::new(rect.center().x, cy - (1.0 - hop) * 10.0 * scale), Align2::CENTER_CENTER, label, ui::bold(26.0 * scale), Color32::WHITE);
        }

        // Filmstrip of every photo with its copies badge.
        let thumb_h = strip_h - 16.0 * scale;
        let widths: Vec<f32> = thumbs.iter().map(|(thumb, _)| {
            let size = thumb.size_vec2();
            thumb_h * size.x / size.y.max(1.0)
        }).collect();
        let gap = 12.0 * scale;
        let full: f32 = widths.iter().sum::<f32>() + gap * count.saturating_sub(1) as f32;
        let fit = ((rect.width() - 2.0 * margin) / full.max(1.0)).min(1.0);
        let mut x = rect.center().x - full * fit / 2.0;
        let strip_top = rect.bottom() - bar_h - strip_h + 8.0 * scale;
        for (index, ((thumb_tex, copies), width)) in thumbs.iter().zip(&widths).enumerate() {
            let thumb = Rect::from_min_size(Pos2::new(x, strip_top), Vec2::new(width * fit, thumb_h * fit));
            x += (width + gap) * fit;
            let active = index == selected;
            painter.image(thumb_tex.id(), thumb, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                if active || *copies > 0 { Color32::WHITE } else { Color32::from_gray(170) });
            if active {
                // The selection ring glides along the filmstrip.
                let ring_x = ui::glide(ui.ctx(), "pick-ring-x", thumb.left());
                let ring = Rect::from_min_size(Pos2::new(ring_x, thumb.top()), thumb.size());
                painter.rect_stroke(ring.expand(3.0), CornerRadius::same(4), Stroke::new(3.5_f32 * scale, Color32::WHITE), egui::StrokeKind::Outside);
            }
            if *copies > 0 {
                let center = Pos2::new(thumb.right() - 4.0 * scale, thumb.top() + 4.0 * scale);
                painter.circle_filled(center, 16.0 * scale, pal.accent);
                painter.text(center, Align2::CENTER_CENTER, format!("×{copies}"), ui::bold(15.0 * scale), pal.on_accent);
            }
            if ui.interact(thumb, ui.id().with(("pick-thumb", index)), egui::Sense::click()).clicked() { select = Some(index); }
        }

        // Action bar.
        let height = 72.0 * scale;
        let cy = rect.bottom() - bar_h / 2.0;
        let labels = theme::active().map(|t| t.config.ui.clone());
        let finish = labels.as_ref().map_or_else(|| "Terminer".to_owned(), |l| l.finish_label.clone());
        let print_label = match total_copies { 1 => "Imprimer 1 tirage".to_owned(), n => format!("Imprimer {n} tirages") };
        let mut right = rect.right() - margin;
        let mut print_pressed = false;
        if printing && total_copies > 0 {
            let w = ui::button_width(ui, &print_label, height, true).max(280.0 * scale);
            let r = Rect::from_min_size(Pos2::new(right - w, cy - height / 2.0), Vec2::new(w, height));
            print_pressed = ui::button(ui, r, "pick-print", &print_label, Some(Icon::Print), Kind::Primary, &pal);
            right -= w + 14.0 * scale;
        }
        let kind = if printing && total_copies > 0 { Kind::Glass } else { Kind::Primary };
        let w = ui::button_width(ui, &finish, height, true);
        let r = Rect::from_min_size(Pos2::new(right - w, cy - height / 2.0), Vec2::new(w, height));
        let finish_pressed = ui::button(ui, r, "pick-finish", &finish, Some(Icon::Check), kind, &pal);
        let timeout = Duration::from_secs(self.storage.settings.photo_timeout_seconds.into());
        let left = timeout.saturating_sub(self.last_touch.elapsed());
        let status = if left <= REVIEW_HINT { format!("Retour à l’appareil photo dans {} s", left.as_secs() + 1) }
            else if printing { format!("{count} photos · {total_copies} tirage(s) choisi(s)") }
            else { format!("{count} photos dans l’album de l’événement") };
        painter.text(Pos2::new(rect.left() + margin, cy), Align2::LEFT_CENTER, status, ui::body(18.0 * scale), Color32::from_white_alpha(210));

        if confirm {
            self.render_pick_confirm(ui, rect, scale, &pal, total_copies);
            return;
        }
        let burst = self.burst.as_mut().expect("burst");
        if let Some(index) = select {
            burst.slide_dir = if index > selected { 1.0 } else { -1.0 };
            burst.selected = index;
            self.last_touch = Instant::now();
        }
        if delta != 0 {
            if let Some(shot) = burst.shots.get_mut(selected) {
                shot.copies = if delta > 0 { shot.copies.saturating_add(1) } else { shot.copies.saturating_sub(1) };
            }
            self.last_touch = Instant::now();
        }
        if print_pressed {
            if total_copies > u32::from(Self::CONFIRM_ABOVE) { burst.confirm = true; } else { self.print_burst(); }
        } else if finish_pressed {
            self.show_thanks_burst(0);
        }
    }

    fn render_pick_confirm(&mut self, ui: &mut egui::Ui, rect: Rect, scale: f32, pal: &Palette, total: u32) {
        let painter = ui.painter().clone();
        let backdrop = ui.interact(rect, ui.id().with("pick-confirm-bg"), egui::Sense::click());
        let enter = ui::appear(ui.ctx(), "pick-confirm", 0.2);
        painter.rect_filled(rect, 0.0, Color32::from_black_alpha((150.0 * enter) as u8));
        let card = Rect::from_center_size(rect.center() + Vec2::new(0.0, (1.0 - enter) * 60.0 * scale), Vec2::new(600.0, 360.0) * scale);
        ui::soft_shadow(&painter, card, 28, 40, 16, 110);
        painter.rect_filled(card, CornerRadius::same((28.0 * scale) as u8), pal.surface);
        let ink = ui::readable_on(pal.surface);
        let icon_c = Pos2::new(card.center().x, card.top() + 76.0 * scale);
        painter.circle_filled(icon_c, 38.0 * scale, ui::alpha(pal.accent, 40));
        ui::icon(&painter, icon_c, 38.0 * scale, pal.accent, Icon::Print);
        painter.text(Pos2::new(card.center().x, card.top() + 134.0 * scale), Align2::CENTER_TOP,
            format!("Imprimer {total} tirages ?"), ui::display(34.0 * scale), ink);
        painter.text(Pos2::new(card.center().x, card.top() + 182.0 * scale), Align2::CENTER_TOP,
            "On vérifie avant de lancer l’impression.", ui::body(19.0 * scale), ui::alpha(ink, 170));
        let height = 72.0 * scale;
        let by = card.bottom() - 36.0 * scale - height;
        let yes = format!("Oui, imprimer {total}");
        let yes_w = ui::button_width(ui, &yes, height, true);
        let back_w = ui::button_width(ui, "Modifier", height, false);
        let both = yes_w + back_w + 16.0 * scale;
        let back = Rect::from_min_size(Pos2::new(card.center().x - both / 2.0, by), Vec2::new(back_w, height));
        let ok = Rect::from_min_size(Pos2::new(back.right() + 16.0 * scale, by), Vec2::new(yes_w, height));
        let outside = backdrop.clicked() && backdrop.interact_pointer_pos().is_some_and(|p| !card.contains(p));
        if ui::button(ui, back, "pick-back", "Modifier", None, Kind::Glass, pal) || outside {
            if let Some(burst) = self.burst.as_mut() { burst.confirm = false; }
        } else if ui::button(ui, ok, "pick-yes", &yes, Some(Icon::Print), Kind::Primary, pal) {
            self.print_burst();
        }
    }

    /// Queue every chosen copy (one quota reservation each), then thank the guests.
    fn print_burst(&mut self) {
        let Some(burst) = self.burst.as_ref() else { return; };
        let orders: Vec<(PathBuf, u8)> = burst.shots.iter().filter(|s| s.copies > 0).map(|s| (s.framed.clone(), s.copies)).collect();
        let mut queued: u16 = 0;
        'orders: for (path, copies) in orders {
            for _ in 0..copies {
                match self.submit_print(path.clone()) {
                    Ok(()) => queued += 1,
                    Err(error) => { self.status = error; break 'orders; }
                }
            }
        }
        if queued == 0 {
            if let Some(burst) = self.burst.as_mut() { burst.confirm = false; }
            self.notice = Some(("Impression indisponible pour le moment : vos photos sont bien enregistrées.".into(), Instant::now()));
        } else {
            self.show_thanks_burst(queued);
        }
    }

    /// Thank-you screen showing the first printed photo (or the selected one).
    fn show_thanks_burst(&mut self, copies: u16) {
        let ctx_photo = self.burst.as_mut().and_then(|burst| {
            let index = burst.shots.iter().position(|s| s.copies > 0).unwrap_or(burst.selected);
            match burst.big.take() {
                Some((big_index, texture)) if big_index == index => Some(texture),
                _ => burst.shots.get(index).map(|s| s.thumb.clone()),
            }
        });
        self.review = ctx_photo;
        self.show_thanks(copies);
    }
}

fn arrow_button(ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect, id: &str, left: bool) -> bool {
    let response = ui.interact(rect, ui.id().with(id), egui::Sense::click());
    let fill = Color32::from_black_alpha(if response.is_pointer_button_down_on() { 170 } else { 110 });
    painter.circle(rect.center(), rect.width() / 2.0, fill, Stroke::new(1.0_f32, Color32::from_white_alpha(50)));
    let c = rect.center();
    let a = rect.width() * 0.14;
    let dir = if left { 1.0 } else { -1.0 };
    let stroke = Stroke::new(4.0_f32, Color32::WHITE);
    painter.line_segment([c + Vec2::new(dir * a * 0.6, -a * 1.2), c + Vec2::new(-dir * a * 0.6, 0.0)], stroke);
    painter.line_segment([c + Vec2::new(-dir * a * 0.6, 0.0), c + Vec2::new(dir * a * 0.6, a * 1.2)], stroke);
    response.clicked()
}

fn round_step(ui: &mut egui::Ui, painter: &egui::Painter, rect: Rect, id: &str, plus: bool, enabled: bool, pal: &Palette) -> bool {
    let response = ui.interact(rect, ui.id().with(id), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let fill = if !enabled { Color32::from_white_alpha(25) }
        else if response.is_pointer_button_down_on() { ui::shade(pal.accent, 0.85) } else { pal.accent };
    painter.circle_filled(rect.center(), rect.width() / 2.0, fill);
    let glyph = if enabled { pal.on_accent } else { Color32::from_white_alpha(90) };
    let arm = rect.width() * 0.22;
    let stroke = Stroke::new(5.0_f32, glyph);
    let c = rect.center();
    painter.line_segment([c - Vec2::new(arm, 0.0), c + Vec2::new(arm, 0.0)], stroke);
    if plus { painter.line_segment([c - Vec2::new(0.0, arm), c + Vec2::new(0.0, arm)], stroke); }
    enabled && response.clicked()
}
