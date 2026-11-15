// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
use std::sync::{Arc, OnceLock, RwLock};
use crate::theme_config::LoadedTheme;
use eframe::egui::{self, Color32, RichText, Stroke, Vec2};

pub const SAND: Color32 = Color32::from_rgb(248, 246, 239);
pub const GREEN: Color32 = Color32::from_rgb(47, 88, 65);
pub const INK: Color32 = Color32::from_rgb(34, 52, 40);
pub const MUTED: Color32 = Color32::from_rgb(117, 130, 118);
pub const WHITE: Color32 = Color32::WHITE;
pub const TERRACOTTA: Color32 = Color32::from_rgb(186, 103, 76);

// Only the UI thread changes themes. The camera worker never needs the active theme.
// Holding an Arc lets widgets read the theme without allocating a new config.
static ACTIVE: OnceLock<RwLock<Arc<LoadedTheme>>> = OnceLock::new();

pub fn set_active(theme: LoadedTheme) {
    assert!(
        ACTIVE.set(RwLock::new(Arc::new(theme))).is_ok(),
        "Theme may only be initialized once"
    );
}

pub fn replace_active(theme: LoadedTheme) {
    if let Some(active) = ACTIVE.get() {
        *active.write().expect("Theme lock poisoned") = Arc::new(theme);
    }
}

pub fn active() -> Option<Arc<LoadedTheme>> {
    ACTIVE.get().map(|lock| {
        Arc::clone(&lock.read().expect("Theme lock poisoned"))
    })
}

fn parse_color(color: &str) -> Color32 {
    let parse = |start: usize| u8::from_str_radix(&color[start..start + 2], 16).unwrap_or(0);
    Color32::from_rgb(parse(1), parse(3), parse(5))
}

pub fn background() -> Color32 { active().map_or(SAND, |theme| parse_color(&theme.config.colors.background)) }
pub fn primary() -> Color32 { active().map_or(GREEN, |theme| parse_color(&theme.config.colors.primary)) }
pub fn accent() -> Color32 { active().map_or(TERRACOTTA, |theme| parse_color(&theme.config.colors.accent)) }
pub fn foreground() -> Color32 { active().map_or(INK, |theme| parse_color(&theme.config.colors.foreground)) }
pub fn surface() -> Color32 { active().map_or(WHITE, |theme| parse_color(&theme.config.colors.surface)) }
pub fn muted() -> Color32 { active().map_or(MUTED, |theme| parse_color(&theme.config.colors.muted)) }

pub fn install(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = background();
    style.visuals.window_fill = background();
    style.visuals.widgets.inactive.bg_fill = surface();
    style.visuals.widgets.hovered.bg_fill = background();
    style.visuals.widgets.active.bg_fill = background();
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(12);
    style.spacing.item_spacing = Vec2::new(16.0, 16.0);
    style.spacing.button_padding = Vec2::new(20.0, 14.0);
    style.visuals.selection.bg_fill = background();
    style.visuals.selection.stroke = Stroke::new(1.5_f32, primary());
    ctx.set_style(style);
}

pub fn subtitle(value: &str) -> RichText {
    RichText::new(value).size(19.0).color(muted())
}

pub fn section(value: &str) -> RichText {
    RichText::new(value).size(26.0).color(foreground()).strong()
}

pub fn small_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(label).size(14.0).color(foreground())).fill(background()).corner_radius(10))
}

