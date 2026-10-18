//! Small building blocks shared by the screens.

use eframe::egui::{
    self, Button, Color32, CornerRadius, Response, RichText, Sense, Stroke, Ui, WidgetInfo,
    WidgetType,
};

use super::theme::*;

/// A frameless icon button whose accessible name is `label`, so tests (and
/// screen readers) can find it by meaning rather than by glyph.
pub fn icon_button(ui: &mut Ui, icon: &str, label: &str) -> Response {
    let r = ui
        .add(Button::new(RichText::new(icon).size(15.0).color(GRAY_400)).frame(false))
        .on_hover_text(label);
    let label = label.to_string();
    r.widget_info(move || WidgetInfo::labeled(WidgetType::Button, true, label.clone()));
    r
}

/// Same, but rendered as "active" (toggled on).
pub fn icon_toggle(ui: &mut Ui, icon: &str, label: &str, on: bool) -> Response {
    let color = if on { GRAY_100 } else { GRAY_500 };
    let mut b = Button::new(RichText::new(icon).size(15.0).color(color));
    if on {
        b = b.fill(GRAY_800);
    } else {
        b = b.frame(false);
    }
    let r = ui.add(b).on_hover_text(label);
    let label = label.to_string();
    r.widget_info(move || WidgetInfo::labeled(WidgetType::Button, true, label.clone()));
    r
}

pub fn primary_button(text: impl Into<String>) -> Button<'static> {
    Button::new(RichText::new(text.into()).color(Color32::WHITE).strong())
        .fill(INDIGO_600)
        .corner_radius(RADIUS)
}

pub fn danger_button(text: impl Into<String>) -> Button<'static> {
    Button::new(RichText::new(text.into()).color(Color32::WHITE).strong())
        .fill(RED_600)
        .corner_radius(RADIUS)
}

pub fn warn_button(text: impl Into<String>) -> Button<'static> {
    Button::new(RichText::new(text.into()).color(Color32::WHITE).strong())
        .fill(AMBER_600)
        .corner_radius(RADIUS)
}

pub fn secondary_button(text: impl Into<String>) -> Button<'static> {
    Button::new(RichText::new(text.into()).color(GRAY_300))
        .fill(GRAY_800)
        .stroke(Stroke::new(1.0, GRAY_700))
        .corner_radius(RADIUS)
}

pub fn card(fill: Color32, stroke: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(RADIUS)
        .inner_margin(egui::Margin::same(12))
}

pub fn section_label(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(10.0)
            .strong()
            .color(GRAY_500),
    );
}

/// A pill-shaped tag, e.g. a risk badge.
pub fn badge(ui: &mut Ui, text: &str, fg: Color32, bg: Color32) -> Response {
    egui::Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(255))
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).color(fg).strong())
        })
        .response
}

/// A text label that can be clicked, with a pointer cursor.
pub fn link_label(ui: &mut Ui, text: RichText) -> Response {
    ui.add(egui::Label::new(text).sense(Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A small spinner (painter-drawn, so it needs no font glyph).
pub fn spinner(ui: &mut Ui, size: f32) -> Response {
    ui.add(egui::Spinner::new().size(size).color(GRAY_400))
}
