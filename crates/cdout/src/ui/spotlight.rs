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
                    centered_row(ui, "spotlight-model-row", |ui| {
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

/// Resize the window to the card's height (no-op when it already fits),
/// moving it up or down by half the change so it stays centred where it
/// was shown instead of growing downwards.
fn fit_window_height(ui: &Ui, card_height: f32) {
    let wanted = card_height.ceil();
    let (inner, outer) = ui
        .ctx()
        .input(|i| (i.viewport().inner_rect, i.viewport().outer_rect));
    let Some(current) = inner.map(|r| r.height()) else {
        return;
    };
    let delta = wanted - current;
    if delta.abs() <= 1.0 {
        return;
    }
    tracing::info!(target: "cdout", "spotlight.resize from={current} to={wanted}");
    let ctx = ui.ctx();
    if let Some(outer) = outer {
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
            outer.min.x,
            (outer.min.y - delta / 2.0).round(),
        )));
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
        CARD_WIDTH, wanted,
    )));
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
            ui.add(egui::Label::new(small(format!("🗁 {}", e.path), GRAY_500)).truncate())
                .on_hover_text(&e.path);
            // File names, not paths: the folder is on the line above, and a
            // full path per file is wider than the card.
            let names: Vec<String> = e
                .selected_files
                .iter()
                .map(|f| file_name(f).to_string())
                .collect();
            if let Some((list, more)) = spotlight_files_line(&names) {
                let text = match more {
                    Some(more) => format!("🗋 {list}{more}"),
                    None => format!("🗋 {list}"),
                };
                ui.add(egui::Label::new(small(text, GRAY_400)).truncate())
                    .on_hover_text(e.selected_files.join("\n"));
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

/// Last path component, accepting both separators.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .find(|s| !s.is_empty())
        .unwrap_or(path)
}

/// A horizontal row centred in the available width. The row's width is
/// measured each frame and used to place it on the next one, so the first
/// frame after a size change is left-aligned and every later one is centred
/// (egui lays out in one pass, so centring needs last frame's width).
fn centered_row(ui: &mut Ui, id_salt: &str, add: impl FnOnce(&mut Ui)) {
    let id = ui.id().with(id_salt);
    let last_width: f32 = ui.ctx().data(|d| d.get_temp(id)).unwrap_or(0.0);
    let avail = ui.available_width();
    let pad = ((avail - last_width) / 2.0).max(0.0);
    let inner = ui.horizontal(|ui| {
        ui.add_space(pad);
        let start = ui.cursor().min.x;
        add(ui);
        ui.cursor().min.x - start
    });
    let width = inner.inner - ui.spacing().item_spacing.x;
    if (width - last_width).abs() > 0.5 {
        ui.ctx().data_mut(|d| d.insert_temp(id, width));
        ui.ctx().request_repaint();
    }
}

/// Extra radius presets not in egui.
struct CornerRadiusExt;
impl CornerRadiusExt {
    const XL: egui::CornerRadius = egui::CornerRadius::same(16);
}
