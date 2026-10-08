//! The hotkey palette (port of `SpotlightApp`). The window is exactly the
//! card: it is resized to the card's height every frame. On Windows the OS
//! rounds the window's corners; on macOS the window is transparent and the
//! card paints its own.

use eframe::egui::{self, Align, Color32, Key, Layout, Margin, RichText, Stroke, TextEdit, Ui};

use super::theme::*;
use super::{icons, model_select, widgets};
use crate::state::text::spotlight_files_line;
use crate::state::{AppState, HideReason, Msg};

pub const CARD_WIDTH: f32 = 560.0;
/// Initial size of the spotlight window, in points; the height then follows
/// the content.
pub const WINDOW_SIZE: [f32; 2] = [CARD_WIDTH, 196.0];
/// Whether the spotlight window is transparent (only where the platform
/// supports it through egui; see `app::spotlight_viewport_builder`).
pub const TRANSPARENT_WINDOW: bool = cfg!(target_os = "macos");

pub fn show(ui: &mut Ui, state: &mut AppState, out: &mut Vec<Msg>) {
    let mac = state.platform.os == "macos";
    let file_manager = state.platform.file_manager;

    // Keys first, so they work wherever focus is.
    let (enter, escape, modifier) = ui.input(|i| {
        (
            i.key_pressed(Key::Enter),
            i.key_pressed(Key::Escape),
            if mac {
                i.modifiers.mac_cmd
            } else {
                i.modifiers.ctrl
            },
        )
    });
    if escape {
        out.push(Msg::SpotlightHide(HideReason::Escape));
    }

    let (radius, stroke) = if TRANSPARENT_WINDOW {
        (
            CornerRadiusExt::XL,
            Stroke::new(1.0, GRAY_700.gamma_multiply(0.5)),
        )
    } else {
        // An opaque window: fill it all, so a resize never shows a gap; the
        // OS draws the rounded outline.
        ui.painter().rect_filled(ui.max_rect(), 0.0, GRAY_900);
        (egui::CornerRadius::ZERO, Stroke::NONE)
    };
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        let card = egui::Frame::new()
            .fill(GRAY_900)
            .stroke(stroke)
            .corner_radius(radius)
            .inner_margin(Margin::same(20))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    icons::icon(ui, 34.0, |p, r| icons::paint_bot(p, r, INDIGO_500.gamma_multiply(0.85)));
                    ui.add_space(4.0);

                    let input = TextEdit::singleline(&mut state.spotlight.prompt)
                        .hint_text(RichText::new("What would you like to do?").color(GRAY_600))
                        .font(egui::FontId::proportional(19.0))
                        .text_color(GRAY_200)
                        .desired_width(f32::INFINITY)
                        .margin(Margin::symmetric(14, 9))
                        .background_color(BLACK_40);
                    let resp = ui.add(input);
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Prompt")
                    });
                    if std::mem::take(&mut state.spotlight.focus_input) {
                        resp.request_focus();
                    }
                    if enter && (resp.lost_focus() || resp.has_focus()) {
                        out.push(Msg::SpotlightSubmit {
                            auto_approve: modifier,
                        });
                        resp.request_focus();
                    }

                    ui.add_space(6.0);
                    context_lines(ui, state, file_manager);
                    let modifier_label = if mac { "⌘" } else { "Ctrl" };
                    ui.label(
                        RichText::new(format!(
                            "Enter to run · {modifier_label}+Enter to run without approving each step"
                        ))
                        .size(11.0)
                        .color(GRAY_600),
                    );
                    ui.horizontal(|ui| {
                        // Centre the row by measuring nothing: a right-to-left
                        // half and a left-to-right half meet in the middle.
                        let w = ui.available_width();
                        ui.add_space((w / 2.0 - 150.0).max(0.0));
                        model_select::show(
                            ui,
                            "spotlight-model",
                            &state.models.selected,
                            &state.models.available,
                            state.models.connected,
                            32,
                            out,
                        );
                        if state.models.refreshing {
                            widgets::spinner(ui, 10.0);
                        } else if widgets::icon_button(ui, "⟳", "Refresh models").clicked() {
                            out.push(Msg::RefreshModels);
                        }
                        ui.label(RichText::new("· Esc to dismiss").size(11.0).color(GRAY_600));
                    });

                    if let Some(err) = &state.spotlight.error {
                        ui.add_space(6.0);
                        egui::Frame::new()
                            .fill(RED_950)
                            .stroke(Stroke::new(1.0, RED_600))
                            .corner_radius(RADIUS)
                            .inner_margin(Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(&err.message).size(12.0).color(RED_400));
                                    if widgets::icon_button(ui, "🗙", "Dismiss error").clicked() {
                                        out.push(Msg::DismissSpotlightError);
                                    }
                                });
                            });
                    }
                });
            });
        fit_window_height(ui, card.response.rect.height());
    });
}

/// Resize the window to the card's height (no-op when it already fits).
fn fit_window_height(ui: &Ui, card_height: f32) {
    let wanted = card_height.ceil();
    let current = ui
        .ctx()
        .input(|i| i.viewport().inner_rect.map(|r| r.height()));
    if current.is_some_and(|h| (h - wanted).abs() > 1.0) {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                CARD_WIDTH, wanted,
            )));
    }
}

fn context_lines(ui: &mut Ui, state: &AppState, file_manager: &str) {
    let small = |t: String, c: Color32| RichText::new(t).size(11.0).color(c);
    match state
        .spotlight
        .explorer
        .as_ref()
        .filter(|e| !e.path.is_empty())
    {
        Some(e) => {
            ui.label(small(format!("🗁 {}", e.path), GRAY_500))
                .on_hover_text(&e.path);
            if let Some((names, more)) = spotlight_files_line(&e.selected_files) {
                ui.horizontal(|ui| {
                    let w = ui.available_width();
                    ui.add_space((w / 2.0 - 140.0).max(0.0));
                    ui.label(small(format!("🗋 {names}"), GRAY_400))
                        .on_hover_text(e.selected_files.join("\n"));
                    if let Some(more) = more {
                        ui.label(small(more, GRAY_600));
                    }
                });
            }
        }
        None => {
            ui.label(small(
                format!("No {file_manager} window detected"),
                GRAY_600,
            ));
        }
    }
}

/// Extra radius presets not in egui.
struct CornerRadiusExt;
impl CornerRadiusExt {
    const XL: egui::CornerRadius = egui::CornerRadius::same(16);
}
