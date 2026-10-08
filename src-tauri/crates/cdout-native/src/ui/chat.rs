//! Conversation transcript (port of `ChatMessage.tsx` and the streaming /
//! thinking rows of `MainApp`).

use cdout_core::llm::Message;
use eframe::egui::{
    self, Align, Color32, CornerRadius, Layout, Margin, RichText, ScrollArea, Stroke, Ui,
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use super::theme::*;
use super::{icons, widgets};
use crate::state::{AppState, Msg, MESSAGE_COLLAPSE_CHARS, TOOL_OUTPUT_COLLAPSE_CHARS};

const AVATAR: f32 = 22.0;

pub fn show(ui: &mut Ui, state: &AppState, md: &mut CommonMarkCache, out: &mut Vec<Msg>) {
    ScrollArea::vertical()
        .id_salt("transcript")
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.add_space(12.0);
            let width = ui.available_width();
            for (idx, msg) in state.chat.history.iter().enumerate() {
                ui.push_id(idx, |ui| {
                    ui.set_max_width(width);
                    message(ui, idx, msg, state.chat.expanded.contains(&idx), md, out);
                });
                ui.add_space(14.0);
            }
            if !state.chat.streaming.is_empty() {
                streaming(ui, &state.chat.streaming, md);
                ui.add_space(14.0);
            }
            let c = &state.chat;
            if (c.processing || c.executing)
                && c.pending_command.is_none()
                && c.streaming.is_empty()
            {
                thinking(ui, c.executing);
            }
            ui.add_space(8.0);
        });
}

#[derive(Clone, Copy)]
enum Avatar {
    Bot,
    User,
    Tool,
}

/// A fixed-size avatar square. Allocates exactly `AVATAR` points so it can
/// never stretch across the row.
fn avatar(ui: &mut Ui, kind: Avatar, fg: Color32, bg: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(AVATAR, AVATAR), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(4), bg);
    let inner = rect.shrink(4.0);
    match kind {
        Avatar::Bot => icons::paint_bot(p, inner, fg),
        Avatar::User => icons::paint_user(p, inner, fg),
        Avatar::Tool => {
            p.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                ">_",
                egui::FontId::monospace(10.0),
                fg,
            );
        }
    }
}

fn message(
    ui: &mut Ui,
    idx: usize,
    msg: &Message,
    expanded: bool,
    md: &mut CommonMarkCache,
    out: &mut Vec<Msg>,
) {
    if msg.synthetic {
        synthetic_note(ui, &msg.content);
        return;
    }
    match msg.role.as_str() {
        "system" => system_context(ui, idx, &msg.content),
        "user" => {
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                avatar(ui, Avatar::User, Color32::WHITE, INDIGO_600);
                // Cap the bubble at 85% of the row; inside, the label sizes
                // to its text and wraps at the cap.
                let cap = ui.available_width() * 0.85;
                ui.scope(|ui| {
                    ui.set_max_width(cap);
                    egui::Frame::new()
                        .fill(Color32::from_rgb(0x1e, 0x1e, 0x2e))
                        .stroke(Stroke::new(1.0, INDIGO_500.gamma_multiply(0.15)))
                        .corner_radius(RADIUS)
                        .inner_margin(Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&msg.content)
                                        .color(Color32::from_rgb(0xe0, 0xe7, 0xff)),
                                )
                                .wrap(),
                            );
                        });
                });
            });
        }
        "tool" => {
            ui.horizontal_top(|ui| {
                avatar(ui, Avatar::Tool, EMERALD_500, GRAY_800);
                tool_output(ui, idx, &msg.content, expanded, out);
            });
        }
        _ => {
            ui.horizontal_top(|ui| {
                avatar(ui, Avatar::Bot, Color32::WHITE, EMERALD_600);
                ui.vertical(|ui| {
                    ui.set_max_width(ui.available_width() * 0.92);
                    let long = msg.content.chars().count() > MESSAGE_COLLAPSE_CHARS;
                    if long && !expanded {
                        egui::ScrollArea::vertical()
                            .id_salt(("collapsed", idx))
                            .max_height(192.0)
                            .scroll_source(egui::scroll_area::ScrollSource::NONE)
                            .show(ui, |ui| markdown(ui, idx, &msg.content, md));
                    } else {
                        markdown(ui, idx, &msg.content, md);
                    }
                    if long {
                        expand_toggle(ui, idx, expanded, out);
                    }
                });
            });
        }
    }
}

fn markdown(ui: &mut Ui, idx: usize, text: &str, md: &mut CommonMarkCache) {
    ui.push_id(("md", idx), |ui| {
        CommonMarkViewer::new().show(ui, md, text);
    });
}

fn expand_toggle(ui: &mut Ui, idx: usize, expanded: bool, out: &mut Vec<Msg>) {
    let text = if expanded {
        "⏶ SHOW LESS"
    } else {
        "⏷ SHOW MORE"
    };
    if widgets::link_label(ui, RichText::new(text).size(10.0).strong().color(GRAY_500)).clicked() {
        out.push(Msg::ToggleExpand(idx));
    }
}

fn tool_output(ui: &mut Ui, idx: usize, content: &str, expanded: bool, out: &mut Vec<Msg>) {
    egui::Frame::new()
        .fill(BLACK_40)
        .stroke(Stroke::new(1.0, GRAY_800))
        .corner_radius(RADIUS)
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            // The row around us is horizontal; lay out (and wrap) vertically.
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                let long = content.chars().count() > TOOL_OUTPUT_COLLAPSE_CHARS;
                let text = egui::Label::new(
                    RichText::new(content)
                        .monospace()
                        .size(11.5)
                        .color(Color32::from_rgb(0x4a, 0xde, 0x80)),
                )
                .wrap();
                if long && !expanded {
                    egui::ScrollArea::vertical()
                        .id_salt(("tool", idx))
                        .max_height(128.0)
                        .scroll_source(egui::scroll_area::ScrollSource::NONE)
                        .show(ui, |ui: &mut Ui| ui.add(text));
                } else {
                    ui.add(text);
                }
                if long {
                    expand_toggle(ui, idx, expanded, out);
                }
            });
        });
}

/// A card centred in the transcript, as wide as the transcript up to 672pt
/// (React's `max-w-2xl w-full`).
fn centered_card(ui: &mut Ui, frame: egui::Frame, add: impl FnOnce(&mut Ui)) {
    let width = ui.available_width().min(672.0);
    let margin = ((ui.available_width() - width) / 2.0).max(0.0);
    ui.horizontal(|ui| {
        ui.add_space(margin);
        ui.allocate_ui_with_layout(egui::vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
            frame.show(ui, |ui| {
                ui.set_width(ui.available_width());
                add(ui);
            });
        });
    });
}

fn synthetic_note(ui: &mut Ui, content: &str) {
    let frame = egui::Frame::new()
        .fill(GRAY_900.gamma_multiply(0.5))
        .stroke(Stroke::new(1.0, GRAY_800))
        .corner_radius(RADIUS)
        .inner_margin(Margin::symmetric(12, 8));
    centered_card(ui, frame, |ui| {
        ui.label(
            RichText::new("⚙ CDOUT INTERNAL")
                .size(9.0)
                .strong()
                .color(GRAY_600),
        );
        ui.add(
            egui::Label::new(RichText::new(content).size(11.0).italics().color(GRAY_500)).wrap(),
        );
    });
}

fn system_context(ui: &mut Ui, idx: usize, content: &str) {
    let frame = egui::Frame::new()
        .fill(GRAY_900)
        .stroke(Stroke::new(1.0, GRAY_800))
        .corner_radius(RADIUS)
        .inner_margin(Margin::symmetric(12, 6));
    centered_card(ui, frame, |ui| {
        egui::CollapsingHeader::new(RichText::new("› System Context").size(12.0).color(GRAY_500))
            .id_salt(("system", idx))
            .default_open(false)
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(content)
                            .monospace()
                            .size(11.0)
                            .color(GRAY_400),
                    )
                    .wrap(),
                );
            });
    });
}

fn streaming(ui: &mut Ui, text: &str, md: &mut CommonMarkCache) {
    ui.horizontal_top(|ui| {
        avatar(ui, Avatar::Bot, Color32::WHITE, EMERALD_600);
        ui.vertical(|ui| {
            ui.set_max_width(ui.available_width() * 0.92);
            ui.push_id("streaming", |ui| {
                CommonMarkViewer::new().show(ui, md, text);
            });
            ui.label(RichText::new("|").strong().color(EMERALD_500));
        });
    });
}

fn thinking(ui: &mut Ui, executing: bool) {
    ui.horizontal(|ui| {
        let (kind, fg) = if executing {
            (Avatar::Tool, INDIGO_500)
        } else {
            (Avatar::Bot, EMERALD_500)
        };
        avatar(ui, kind, fg, fg.gamma_multiply(0.2));
        ui.add(egui::Spinner::new().size(12.0).color(GRAY_500));
        let label = if executing {
            "Executing command..."
        } else {
            "Thinking..."
        };
        ui.label(RichText::new(label).size(12.0).color(GRAY_500));
    });
}
