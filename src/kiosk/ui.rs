// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Guest-facing drawing kit: embedded fonts, palette, vector icons and touch buttons.
//!
//! Everything here is immediate-mode painting of a handful of shapes. Motion is
//! touch-driven only (see "motion" below): short transitions that start on a
//! tap or a screen change and stop by themselves, so an idle booth costs zero CPU.
use eframe::egui::{
    self, emath::Rot2, epaint::Shadow, Align2, Color32, CornerRadius, FontData, FontDefinitions,
    FontFamily, FontId, Mesh, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, TextureHandle,
    Vec2,
};
use boothsoft::theme;
use std::sync::Arc;

// ------------------------------------------------------------------ motion
//
// Touch UI rules: every tap answers within one frame, every change of state is
// shown by a short movement (≤ 250 ms), and nothing ever loops. All helpers
// request repaints only while their transition is running.

/// Standard duration of an entrance (screen, modal, photo), in seconds.
pub const ENTER: f32 = 0.24;

/// Fast start, soft landing.
pub fn ease_out(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

/// Progress 0 → 1 (eased) since `id` started being drawn. It restarts whenever
/// the element was not drawn on the previous frame: call it every frame the
/// element is visible, with no bookkeeping in the caller.
pub fn appear(ctx: &egui::Context, id: impl std::hash::Hash, secs: f32) -> f32 {
    let id = egui::Id::new(("appear", id));
    let now = ctx.input(|i| i.time);
    let frame = ctx.cumulative_pass_nr();
    let start = ctx.data_mut(|data| {
        let start = data.get_temp::<(f64, u64)>(id).filter(|(_, last)| frame <= last + 1).map_or(now, |(start, _)| start);
        data.insert_temp(id, (start, frame));
        start
    });
    let t = ((now - start) as f32 / secs).clamp(0.0, 1.0);
    if t < 1.0 { ctx.request_repaint(); }
    ease_out(t)
}

/// A value that glides to `target` (selection pills, rings, knobs).
pub fn glide(ctx: &egui::Context, id: impl std::hash::Hash, target: f32) -> f32 {
    ctx.animate_value_with_time(egui::Id::new(("glide", id)), target, 0.16)
}

/// 0 → 1 while a control is held down, back to 0 on release.
pub fn press(ctx: &egui::Context, id: egui::Id, pressed: bool) -> f32 {
    ctx.animate_bool_with_time(id.with("press"), pressed, 0.08)
}

// ------------------------------------------------------------------ fonts

/// Embedded OFL fonts (~185 KB). Glyphs are rasterised lazily by egui, so only
/// the sizes actually displayed occupy the font atlas.
static DM_SANS_BOLD: &[u8] = include_bytes!("../../assets/fonts/DMSans_700Bold.ttf");

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("fraunces".into(), Arc::new(FontData::from_static(
        include_bytes!("../../assets/fonts/Fraunces_700Bold.ttf"))));
    fonts.font_data.insert("dmsans".into(), Arc::new(FontData::from_static(
        include_bytes!("../../assets/fonts/DMSans_500Medium.ttf"))));
    fonts.font_data.insert("dmsans-bold".into(), Arc::new(FontData::from_static(DM_SANS_BOLD)));
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    if let Some(proportional) = fonts.families.get_mut(&FontFamily::Proportional) {
        proportional.insert(0, "dmsans".into());
    }
    let with_fallback = |first: &str| {
        let mut family = vec![first.to_owned()];
        family.extend(fallback.iter().cloned());
        family
    };
    fonts.families.insert(FontFamily::Name("display".into()), with_fallback("fraunces"));
    fonts.families.insert(FontFamily::Name("bold".into()), with_fallback("dmsans-bold"));
    ctx.set_fonts(fonts);
}

pub fn display(size: f32) -> FontId { FontId::new(size, FontFamily::Name("display".into())) }
pub fn bold(size: f32) -> FontId { FontId::new(size, FontFamily::Name("bold".into())) }
pub fn body(size: f32) -> FontId { FontId::proportional(size) }

/// Countdown numerals are ~180 px tall. Drawn as egui text, each would be
/// rasterised into the shared font atlas, which then doubles in size and stays
/// that way (+24 MB measured). Instead the *current* numeral alone is
/// rasterised into its own ~80 KB texture, at most once per second.
#[derive(Default)]
pub struct BigNumeral {
    key: Option<(String, u32)>,
    texture: Option<TextureHandle>,
}

impl BigNumeral {
    pub fn get(&mut self, ctx: &egui::Context, text: &str, px: f32) -> Option<&TextureHandle> {
        let px = px.round().clamp(8.0, 512.0) as u32;
        if self.key.as_ref().is_none_or(|(t, p)| t != text || *p != px) {
            self.texture = rasterize(ctx, text, px);
            self.key = Some((text.to_owned(), px));
        }
        self.texture.as_ref()
    }

    pub fn clear(&mut self) {
        self.key = None;
        self.texture = None;
    }
}

fn rasterize(ctx: &egui::Context, text: &str, px: u32) -> Option<TextureHandle> {
    use ab_glyph::{point, Font, FontRef, ScaleFont};
    let font = FontRef::try_from_slice(DM_SANS_BOLD).ok()?;
    let scaled = font.as_scaled(px as f32);
    let mut caret = 0.0;
    let mut glyphs = Vec::with_capacity(text.len());
    for character in text.chars() {
        let id = font.glyph_id(character);
        glyphs.push(id.with_scale_and_position(px as f32, point(caret, scaled.ascent())));
        caret += scaled.h_advance(id);
    }
    let width = caret.ceil() as usize + 2;
    let height = (scaled.ascent() - scaled.descent()).ceil() as usize + 2;
    let mut coverage = vec![0u8; width * height];
    for glyph in glyphs {
        let Some(outline) = font.outline_glyph(glyph) else { continue; };
        let bounds = outline.px_bounds();
        outline.draw(|x, y, value| {
            let (x, y) = (x as i64 + bounds.min.x as i64, y as i64 + bounds.min.y as i64);
            if x >= 0 && y >= 0 && (x as usize) < width && (y as usize) < height {
                let cell = &mut coverage[y as usize * width + x as usize];
                *cell = (*cell).max((value.clamp(0.0, 1.0) * 255.0) as u8);
            }
        });
    }
    let rgba: Vec<u8> = coverage.iter().flat_map(|a| [255, 255, 255, *a]).collect();
    let image = egui::ColorImage::from_rgba_unmultiplied([width, height], &rgba);
    Some(ctx.load_texture("big-numeral", image, egui::TextureOptions::LINEAR))
}

/// Draw a white numeral texture centred on `center`, with a soft drop shadow.
pub fn numeral(painter: &Painter, texture: &TextureHandle, center: Pos2, scale: f32) {
    let rect = Rect::from_center_size(center, texture.size_vec2() * scale);
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    painter.image(texture.id(), rect.translate(Vec2::new(0.0, 4.0)), uv, Color32::from_black_alpha(90));
    painter.image(texture.id(), rect, uv, Color32::WHITE);
}

// ------------------------------------------------------------------ palette

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub fg: Color32,
    pub primary: Color32,
    pub accent: Color32,
    pub surface: Color32,
    pub on_accent: Color32,
}

impl Palette {
    pub fn current() -> Self {
        let accent = theme::accent();
        Self {
            bg: theme::background(), fg: theme::foreground(), primary: theme::primary(), accent,
            surface: theme::surface(), on_accent: readable_on(accent),
        }
    }
    pub fn dark_background(&self) -> bool { luminance(self.bg) < 0.35 }
}

fn luminance(color: Color32) -> f32 {
    (0.2126 * f32::from(color.r()) + 0.7152 * f32::from(color.g()) + 0.0722 * f32::from(color.b())) / 255.0
}

pub fn readable_on(color: Color32) -> Color32 {
    if luminance(color) > 0.62 { Color32::from_rgb(28, 28, 30) } else { Color32::WHITE }
}

pub fn alpha(color: Color32, value: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), value)
}

pub fn shade(color: Color32, factor: f32) -> Color32 {
    let f = |c: u8| (f32::from(c) * factor).clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(f(color.r()), f(color.g()), f(color.b()), color.a())
}

// ------------------------------------------------------------------ primitives

pub fn soft_shadow(painter: &Painter, rect: Rect, radius: u8, blur: u8, offset_y: i8, strength: u8) {
    painter.add(Shadow { offset: [0, offset_y], blur, spread: 0, color: Color32::from_black_alpha(strength) }
        .as_shape(rect, CornerRadius::same(radius)));
}

/// Vertical gradient as a 4-vertex mesh (no texture, no allocation per pixel).
pub fn gradient(painter: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(mesh));
}

/// UV rectangle that crops a texture to fill `target` ("object-fit: cover").
pub fn cover_uv(source: Vec2, target: Vec2) -> Rect {
    if source.x <= 0.0 || source.y <= 0.0 || target.x <= 0.0 || target.y <= 0.0 {
        return Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    }
    let source_ratio = source.x / source.y;
    let target_ratio = target.x / target.y;
    if source_ratio > target_ratio {
        let u = (1.0 - target_ratio / source_ratio) / 2.0;
        Rect::from_min_max(Pos2::new(u, 0.0), Pos2::new(1.0 - u, 1.0))
    } else {
        let v = (1.0 - source_ratio / target_ratio) / 2.0;
        Rect::from_min_max(Pos2::new(0.0, v), Pos2::new(1.0, 1.0 - v))
    }
}

/// Draw a texture filling `rect`. Mirroring is a UV flip: free on the GPU.
pub fn image_cover(painter: &Painter, texture: &TextureHandle, rect: Rect, mirror: bool, tint: Color32) {
    let mut uv = cover_uv(texture.size_vec2(), rect.size());
    if mirror { uv = Rect::from_min_max(Pos2::new(uv.max.x, uv.min.y), Pos2::new(uv.min.x, uv.max.y)); }
    painter.image(texture.id(), rect, uv, tint);
}

/// Largest rect of the texture's aspect ratio that fits inside `area`, centred.
pub fn fit(size: Vec2, area: Rect) -> Rect {
    if size.x <= 0.0 || size.y <= 0.0 { return area; }
    let scale = (area.width() / size.x).min(area.height() / size.y);
    Rect::from_center_size(area.center(), size * scale)
}

fn arc_points(center: Pos2, radius: f32, start: f32, end: f32, segments: usize) -> Vec<Pos2> {
    (0..=segments).map(|i| {
        let a = start + (end - start) * i as f32 / segments as f32;
        center + Vec2::angled(a) * radius
    }).collect()
}

pub fn arc(painter: &Painter, center: Pos2, radius: f32, start: f32, end: f32, stroke: Stroke) {
    let segments = ((end - start).abs() * radius / 6.0).clamp(8.0, 96.0) as usize;
    painter.add(Shape::line(arc_points(center, radius, start, end, segments), stroke));
}

/// Text with a soft dark halo, legible on any camera image.
pub fn shadowed_text(painter: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32) -> Rect {
    painter.text(pos + Vec2::new(0.0, 2.0), align, text, font.clone(), Color32::from_black_alpha(110));
    painter.text(pos, align, text, font, color)
}

// ------------------------------------------------------------------ icons

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon { Camera, Retake, Print, Arrow, Check, Close, Heart, Mic, Play }

/// Vector icons drawn in a `size` x `size` box. No font glyph dependency.
pub fn icon(painter: &Painter, center: Pos2, size: f32, color: Color32, which: Icon) {
    let w = (size * 0.1).max(2.0);
    let stroke = Stroke::new(w, color);
    let s = size / 2.0;
    let p = |x: f32, y: f32| center + Vec2::new(x * s, y * s);
    match which {
        Icon::Camera => {
            let body = Rect::from_center_size(p(0.0, 0.12), Vec2::new(size * 0.92, size * 0.66));
            painter.rect_stroke(body, CornerRadius::same((size * 0.14) as u8), stroke, StrokeKind::Middle);
            painter.add(Shape::line(vec![p(-0.36, -0.21), p(-0.22, -0.46), p(0.22, -0.46), p(0.36, -0.21)], stroke));
            painter.circle_stroke(p(0.0, 0.14), size * 0.19, stroke);
        }
        Icon::Retake => {
            let r = s * 0.72;
            arc(painter, center, r, -1.9, 3.6, stroke);
            let tip = center + Vec2::angled(-1.9) * r;
            let tangent = Vec2::angled(-1.9 - std::f32::consts::FRAC_PI_2);
            let normal = Vec2::angled(-1.9);
            painter.add(Shape::convex_polygon(vec![
                tip + tangent * (s * 0.42),
                tip + normal * (s * 0.3),
                tip - normal * (s * 0.3),
            ], color, Stroke::NONE));
        }
        Icon::Print => {
            painter.rect_stroke(Rect::from_min_max(p(-0.5, -0.9), p(0.5, -0.3)), CornerRadius::same(2), stroke, StrokeKind::Middle);
            painter.rect_stroke(Rect::from_min_max(p(-0.92, -0.3), p(0.92, 0.45)), CornerRadius::same((size * 0.1) as u8), stroke, StrokeKind::Middle);
            painter.rect_filled(Rect::from_min_max(p(-0.5, 0.15), p(0.5, 0.92)), CornerRadius::same(2), color);
            painter.circle_filled(p(0.62, -0.05), w * 0.7, color);
        }
        Icon::Arrow => {
            painter.line_segment([p(-0.75, 0.0), p(0.7, 0.0)], stroke);
            painter.add(Shape::line(vec![p(0.15, -0.55), p(0.72, 0.0), p(0.15, 0.55)], stroke));
        }
        Icon::Check => {
            painter.add(Shape::line(vec![p(-0.7, 0.05), p(-0.2, 0.55), p(0.75, -0.5)], Stroke::new(w * 1.3, color)));
        }
        Icon::Close => {
            painter.line_segment([p(-0.55, -0.55), p(0.55, 0.55)], stroke);
            painter.line_segment([p(0.55, -0.55), p(-0.55, 0.55)], stroke);
        }
        Icon::Heart => {
            let mut points = Vec::with_capacity(48);
            for i in 0..48 {
                let t = i as f32 / 48.0 * std::f32::consts::TAU;
                let x = 16.0 * t.sin().powi(3);
                let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                points.push(center + Vec2::new(x, -y - 2.0) * (size / 34.0));
            }
            painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
        }
        Icon::Mic => {
            let capsule = Rect::from_min_max(p(-0.26, -0.9), p(0.26, 0.2));
            painter.rect_filled(capsule, CornerRadius::same((s * 0.26) as u8), color);
            arc(painter, p(0.0, -0.08), s * 0.48, 0.0, std::f32::consts::PI, stroke);
            painter.line_segment([p(0.0, 0.4), p(0.0, 0.72)], stroke);
            painter.line_segment([p(-0.3, 0.74), p(0.3, 0.74)], stroke);
        }
        Icon::Play => {
            painter.add(Shape::convex_polygon(vec![p(-0.42, -0.62), p(0.62, 0.0), p(-0.42, 0.62)], color, Stroke::NONE));
        }
    }
}

// ------------------------------------------------------------------ buttons

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Filled accent call-to-action.
    Primary,
    /// Translucent dark glass, for use over the camera image.
    Glass,
}

/// Large touch target. Pressed state is a 3 % shrink + darker fill: it is
/// rendered by the repaint the touch already triggers, so it costs nothing extra.
pub fn button(ui: &mut egui::Ui, rect: Rect, id: &str, label: &str, icon_kind: Option<Icon>, kind: Kind, pal: &Palette) -> bool {
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    let pressed = response.is_pointer_button_down_on();
    let rect = rect.shrink2(rect.size() * 0.025 * press(ui.ctx(), response.id, pressed));
    let painter = ui.painter();
    let radius = CornerRadius::same((rect.height() / 2.0).min(255.0) as u8);
    let (fill, text, outline) = match kind {
        Kind::Primary => (if pressed { shade(pal.accent, 0.88) } else { pal.accent }, pal.on_accent, Stroke::NONE),
        Kind::Glass => (Color32::from_black_alpha(if pressed { 150 } else { 105 }), Color32::WHITE, Stroke::new(1.0_f32, Color32::from_white_alpha(45))),
    };
    if kind == Kind::Primary && !pressed {
        soft_shadow(painter, rect, radius.nw, 22, 8, 55);
    }
    painter.rect(rect, radius, fill, outline, StrokeKind::Inside);
    let font = bold((rect.height() * 0.3).clamp(15.0, 28.0));
    let galley = painter.layout_no_wrap(label.to_owned(), font, text);
    let icon_size = rect.height() * 0.34;
    let gap = if icon_kind.is_some() && !label.is_empty() { rect.height() * 0.18 } else { 0.0 };
    let content = galley.size().x + if icon_kind.is_some() { icon_size + gap } else { 0.0 };
    let mut x = rect.center().x - content / 2.0;
    // Forward arrows read after the label ("C'est parti →"); other icons lead.
    let trailing = icon_kind == Some(Icon::Arrow) && !label.is_empty();
    if let Some(which) = icon_kind.filter(|_| !trailing) {
        icon(painter, Pos2::new(x + icon_size / 2.0, rect.center().y), icon_size, text, which);
        x += icon_size + gap;
    }
    let text_width = galley.size().x;
    painter.galley(Pos2::new(x, rect.center().y - galley.size().y / 2.0), galley, text);
    if trailing {
        icon(painter, Pos2::new(x + text_width + gap + icon_size / 2.0, rect.center().y), icon_size, text, Icon::Arrow);
    }
    response.clicked()
}

/// Width a `button` needs for its label at the given height.
pub fn button_width(ui: &egui::Ui, label: &str, height: f32, with_icon: bool) -> f32 {
    let font = bold((height * 0.3).clamp(15.0, 28.0));
    let text = ui.painter().layout_no_wrap(label.to_owned(), font, Color32::WHITE).size().x;
    text + height * 1.1 + if with_icon { height * 0.52 } else { 0.0 }
}

/// Round camera shutter: white ring, accent core, camera glyph.
pub fn shutter(ui: &mut egui::Ui, center: Pos2, radius: f32, pal: &Palette) -> bool {
    shutter_with(ui, center, radius, pal, Icon::Camera)
}

pub fn shutter_with(ui: &mut egui::Ui, center: Pos2, radius: f32, pal: &Palette, which: Icon) -> bool {
    let rect = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
    let response = ui.interact(rect, ui.id().with("shutter"), Sense::click());
    let pressed = response.is_pointer_button_down_on();
    let r = radius * (1.0 - 0.08 * press(ui.ctx(), response.id, pressed));
    let painter = ui.painter();
    // Ring + core only: no blurred shadow or translucent disc to blend each frame.
    painter.circle_stroke(center, r - 3.0, Stroke::new(6.0_f32, Color32::WHITE));
    painter.circle_filled(center, r * 0.76, if pressed { shade(pal.accent, 0.88) } else { pal.accent });
    icon(painter, center, r * 0.62, pal.on_accent, which);
    response.clicked()
}

/// Rotate a textured quad around its centre (used for the tilted photo print).
pub fn rotated_image(painter: &Painter, texture: &TextureHandle, rect: Rect, angle: f32) {
    let mut mesh = Mesh::with_texture(texture.id());
    mesh.add_rect_with_uv(rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
    mesh.rotate(Rot2::from_angle(angle), rect.center());
    painter.add(Shape::mesh(mesh));
}

pub fn rotated_rect(painter: &Painter, rect: Rect, angle: f32, color: Color32) {
    let rot = Rot2::from_angle(angle);
    let c = rect.center();
    let points = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()]
        .iter().map(|p| c + rot * (*p - c)).collect();
    painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
}

/// Letter-spaced uppercase label ("PHOTOBOOTH"), done with thin spaces.
pub fn eyebrow(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 4);
    for (i, c) in text.to_uppercase().chars().enumerate() {
        if i > 0 { out.push('\u{2009}'); }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contrast_picks_dark_text_on_light_colors() {
        assert_eq!(readable_on(Color32::from_rgb(255, 200, 87)), Color32::from_rgb(28, 28, 30));
        assert_eq!(readable_on(Color32::from_rgb(46, 77, 58)), Color32::WHITE);
    }
    #[test]
    fn fit_keeps_aspect_ratio() {
        let r = fit(Vec2::new(1800.0, 1200.0), Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 900.0)));
        assert!((r.width() / r.height() - 1.5).abs() < 0.001);
        assert!((r.width() - 900.0).abs() < 0.01);
    }
    #[test]
    fn eyebrow_spaces_letters() {
        assert_eq!(eyebrow("ab"), "A\u{2009}B");
    }
}
