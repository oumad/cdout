//! Prompt box at the bottom of the main window (`ChatInput.tsx`): Enter
//! sends (or queues while busy), Stop replaces Send while the agent works.

use eframe::egui::{self, Color32, Key, Margin, RichText, Stroke, TextEdit, Ui};

use super::theme::*;
use super::{model_select, widgets};
use crate::state::{AppState, Msg};

pub fn show(ui: &mut Ui, state: &mut AppState, out: &mut Vec<Msg>) {
    let busy = state.chat.is_busy();
    let has_pending = state.chat.pending_command.is_some();
    let placeholder = if has_pending {
        "Approve command above..."
    } else if busy {
        "Agent is working… you can type while it runs"
    } else {
        "Ask me anything..."
    };

    egui::Frame::new()
        .fill(GRAY_900)
        .stroke(Stroke::new(1.0, GRAY_700))
        .corner_radius(RADIUS_LG)
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let input = TextEdit::singleline(&mut state.chat.input)
                .hint_text(RichText::new(placeholder).color(GRAY_600))
                .frame(egui::Frame::NONE)
                .desired_width(f32::INFINITY)
                .font(egui::FontId::proportional(14.0));
            let r = ui.add(input);
            r.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Message")
            });
            if std::mem::take(&mut state.main.focus_input) {
                r.request_focus();
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                out.push(Msg::MainSubmit);
                r.request_focus();
            }

            ui.horizontal(|ui| {
                model_select::show(
                    ui,
                    "main-model",
                    &state.models.selected,
                    &state.models.available,
                    state.models.connected,
                    32,
                    out,
                );
                if state.models.refreshing {
                    widgets::spinner(ui, 12.0);
                } else if widgets::icon_button(ui, "⟳", "Refresh models").clicked() {
                    out.push(Msg::RefreshModels);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if busy {
                        let stop = ui.add(
                            egui::Button::new(RichText::new("■").color(Color32::WHITE))
                                .fill(RED_600)
                                .corner_radius(RADIUS),
                        );
                        stop.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Stop")
                        });
                        if stop.on_hover_text("Stop").clicked() {
                            out.push(Msg::Stop);
                        }
                    } else {
                        let can_send = !state.chat.input.trim().is_empty();
                        let send = ui.add_enabled(
                            can_send,
                            egui::Button::new(RichText::new("⏶").color(Color32::WHITE).strong())
                                .fill(INDIGO_600)
                                .corner_radius(RADIUS),
                        );
                        send.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Send")
                        });
                        if send.on_hover_text("Send").clicked() {
                            out.push(Msg::MainSubmit);
                        }
                    }
                });
            });
        });
}

/// Chips for prompts waiting behind the running task.
pub fn queued(ui: &mut Ui, state: &AppState, out: &mut Vec<Msg>) {
    if state.chat.queued.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        widgets::section_label(ui, "Queued");
        for (i, q) in state.chat.queued.iter().enumerate() {
            egui::Frame::new()
                .fill(INDIGO_900.gamma_multiply(0.3))
                .stroke(Stroke::new(1.0, INDIGO_500.gamma_multiply(0.3)))
                .corner_radius(egui::CornerRadius::same(255))
                .inner_margin(Margin::symmetric(10, 2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(RichText::new(q).size(11.0).color(INDIGO_300))
                                .truncate(),
                        )
                        .on_hover_text(q);
                        if widgets::icon_button(ui, "🗙", "Remove from queue").clicked() {
                            out.push(Msg::RemoveQueued(i));
                        }
                    });
                });
        }
    });
}
