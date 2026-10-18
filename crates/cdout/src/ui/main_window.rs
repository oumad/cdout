//! The main window (`MainApp` layout): title bar, sessions sidebar, toolbar,
//! context drawer, transcript, proposal cards and input.

use eframe::egui::{self, Align2, Color32, Margin, RichText, Stroke, Ui};
use egui_commonmark::CommonMarkCache;

use super::theme::*;
use super::{approval, chat, chat_input, icons, onboarding, settings, sidebar, title_bar, widgets};
use crate::state::{AppState, Msg};

pub fn show(
    ui: &mut Ui,
    state: &mut AppState,
    md: &mut CommonMarkCache,
    now_ms: i64,
    out: &mut Vec<Msg>,
) {
    let mac = state.platform.os == "macos";
    egui::Panel::top("title-bar")
        .exact_size(title_bar::HEIGHT)
        .frame(egui::Frame::NONE)
        .show(ui, |ui| title_bar::show(ui, mac, out));

    let sidebar_w = if state.main.sidebar_collapsed {
        sidebar::COLLAPSED_WIDTH
    } else {
        sidebar::WIDTH
    };
    egui::Panel::left("sessions-sidebar")
        .exact_size(sidebar_w)
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(GRAY_900.gamma_multiply(0.6))
                .stroke(Stroke::new(1.0, GRAY_800))
                .inner_margin(Margin::symmetric(6, 0)),
        )
        .show(ui, |ui| sidebar::show(ui, state, now_ms, out));

    if state.main.settings_open {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| settings::show(ui, state, out));
    } else {
        conversation(ui, state, md, out);
    }

    kill_dialog(ui, state, out);
    toast(ui, state, out);
}

fn conversation(ui: &mut Ui, state: &mut AppState, md: &mut CommonMarkCache, out: &mut Vec<Msg>) {
    egui::Panel::top("toolbar")
        .frame(egui::Frame::new().fill(GRAY_950))
        .show(ui, |ui| {
            if state.main.migration_banner {
                onboarding::migration_banner(ui, out);
            }
            toolbar(ui, state, out);
            if state.main.context_open {
                context_drawer(ui, state, out);
            }
        });

    egui::Panel::bottom("composer")
        .frame(
            egui::Frame::new()
                .fill(GRAY_950)
                .inner_margin(Margin::symmetric(12, 10)),
        )
        .show(ui, |ui| {
            approval::question_card(ui, state, out);
            if state.should_show_continue() {
                continue_banner(ui, out);
            }
            approval::command_card(ui, state, out);
            chat_input::queued(ui, state, out);
            ui.add_space(4.0);
            chat_input::show(ui, state, out);
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(GRAY_950)
                .inner_margin(Margin::symmetric(16, 0)),
        )
        .show(ui, |ui| {
            if state.chat.history.is_empty() {
                if onboarding::should_show(state) {
                    egui::ScrollArea::vertical()
                        .show(ui, |ui| onboarding::provider_setup(ui, state, out));
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() / 2.0 - 40.0).max(0.0));
                        icons::icon(ui, 44.0, |p, r| icons::paint_bot(p, r, GRAY_700));
                        ui.label(
                            RichText::new("Agent Ready")
                                .size(13.0)
                                .strong()
                                .color(GRAY_700),
                        );
                    });
                }
            } else {
                chat::show(ui, state, md, out);
            }
        });
}

fn toolbar(ui: &mut Ui, state: &AppState, out: &mut Vec<Msg>) {
    let fm = state.platform.file_manager;
    egui::Frame::new()
        .inner_margin(Margin::symmetric(10, 5))
        .stroke(Stroke::new(1.0, GRAY_800))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if state.chat.auto_execute {
                    let r = ui.add(
                        egui::Button::new(RichText::new("» Auto").size(12.0).color(AMBER_300))
                            .fill(AMBER_500.gamma_multiply(0.2))
                            .stroke(Stroke::new(1.0, AMBER_500.gamma_multiply(0.4))),
                    );
                    if r.on_hover_text("Running unattended — click to require approval again")
                        .clicked()
                    {
                        out.push(Msg::DisableAutoExecute);
                    }
                }
                let busy = state.explorer_loading || state.chat.processing || state.chat.executing;
                let (text, fill, fg) = match &state.explorer {
                    Some(e) => {
                        let label = format!("🗁 {}", e.selected_files.len());
                        if state.chat.sync_pending {
                            (label, AMBER_500.gamma_multiply(0.2), AMBER_400)
                        } else {
                            (label, GRAY_800, GRAY_300)
                        }
                    }
                    None => ("🗁 Sync".to_string(), INDIGO_600, Color32::WHITE),
                };
                if state.explorer_loading {
                    widgets::spinner(ui, 12.0);
                }
                let sync = ui.add_enabled(
                    !busy,
                    egui::Button::new(RichText::new(text).size(12.0).color(fg)).fill(fill),
                );
                sync.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Sync")
                });
                let hover = match &state.explorer {
                    Some(e) if !e.selected_files.is_empty() => {
                        let names: Vec<&str> = e
                            .selected_files
                            .iter()
                            .map(|f| f.rsplit(['/', '\\']).next().unwrap_or(f))
                            .collect();
                        let mut t = format!("Selected files:\n{}", names.join("\n"));
                        if state.chat.sync_pending {
                            t.push_str("\n\nContext will update with next message");
                        }
                        t
                    }
                    Some(_) => format!("Click to sync with {fm}"),
                    None => format!("Sync with {fm}"),
                };
                if sync.on_hover_text(hover).clicked() {
                    out.push(Msg::SyncExplorer);
                }
                if widgets::icon_toggle(ui, "☰", "Toggle Context Details", state.main.context_open)
                    .clicked()
                {
                    out.push(Msg::ToggleContext);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_toggle(ui, "⚙", "Settings", state.main.settings_open).clicked()
                    {
                        out.push(if state.main.settings_open {
                            Msg::CloseSettings
                        } else {
                            Msg::OpenSettings
                        });
                    }
                    if !state.chat.history.is_empty()
                        && widgets::icon_button(ui, "🗙", "New Chat").clicked()
                    {
                        out.push(Msg::NewChat);
                    }
                });
            });
        });
}

fn context_drawer(ui: &mut Ui, state: &AppState, out: &mut Vec<Msg>) {
    let busy = state.explorer_loading || state.chat.processing || state.chat.executing;
    egui::Frame::new()
        .fill(GRAY_900)
        .stroke(Stroke::new(1.0, GRAY_800))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            match &state.explorer {
                Some(e) => {
                    ui.horizontal(|ui| {
                        widgets::section_label(ui, "Current Directory");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(!busy, widgets::secondary_button("⟳ Sync"))
                                .on_hover_text("Refresh Context")
                                .clicked()
                            {
                                out.push(Msg::SyncExplorer);
                            }
                        });
                    });
                    ui.label(
                        RichText::new(&e.path)
                            .monospace()
                            .size(11.0)
                            .color(GRAY_300),
                    );
                    ui.add_space(6.0);
                    widgets::section_label(
                        ui,
                        &format!("Selected Files ({})", e.selected_files.len()),
                    );
                    if e.selected_files.is_empty() {
                        ui.label(
                            RichText::new("No selection")
                                .italics()
                                .size(11.0)
                                .color(GRAY_600),
                        );
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(96.0)
                            .show(ui, |ui| {
                                for f in &e.selected_files {
                                    ui.label(
                                        RichText::new(format!("🗋 {f}")).size(11.0).color(GRAY_400),
                                    );
                                }
                            });
                    }
                }
                None => {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("No active context found.")
                                .size(12.0)
                                .color(GRAY_500),
                        );
                        if ui
                            .add_enabled(!busy, widgets::primary_button(">_ Sync Context"))
                            .clicked()
                        {
                            out.push(Msg::SyncExplorer);
                        }
                    });
                }
            }
        });
}

fn continue_banner(ui: &mut Ui, out: &mut Vec<Msg>) {
    egui::Frame::new()
        .fill(AMBER_950)
        .stroke(Stroke::new(1.0, AMBER_500.gamma_multiply(0.3)))
        .corner_radius(RADIUS)
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("The agent stopped without acting. If this isn't a final answer, push it to emit the command.")
                        .size(11.0)
                        .color(AMBER_300),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(widgets::warn_button("⟳ Continue"))
                        .on_hover_text("Inject a 'now emit the code' nudge and re-run the agent")
                        .clicked()
                    {
                        out.push(Msg::ContinueAgent);
                    }
                });
            });
        });
    ui.add_space(6.0);
}

fn kill_dialog(ui: &mut Ui, state: &AppState, out: &mut Vec<Msg>) {
    let Some(k) = &state.chat.kill_dialog else {
        return;
    };
    egui::Window::new("A command is still running")
        .id(egui::Id::new("kill-dialog"))
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .default_width(420.0)
        .show(ui.ctx(), |ui| {
            ui.label(
                RichText::new("The LLM stream was stopped, but a command is still running on your system. Do you want to kill it?")
                    .size(12.0)
                    .color(GRAY_400),
            );
            widgets::card(BLACK_40, GRAY_800).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(&k.command).monospace().size(11.0).color(GRAY_300));
                ui.label(RichText::new(format!("PID: {}", k.pid)).size(10.0).color(GRAY_600));
            });
            ui.horizontal(|ui| {
                if ui.add(widgets::danger_button("Kill process")).clicked() {
                    out.push(Msg::ConfirmKill);
                }
                if ui.add(widgets::secondary_button("Keep running")).clicked() {
                    out.push(Msg::KeepRunning);
                }
            });
        });
}

fn toast(ui: &mut Ui, state: &AppState, out: &mut Vec<Msg>) {
    let Some(t) = &state.toast else {
        return;
    };
    egui::Area::new(egui::Id::new("toast"))
        .anchor(Align2::CENTER_BOTTOM, egui::vec2(0.0, -96.0))
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(RED_950)
                .stroke(Stroke::new(1.0, RED_600))
                .corner_radius(RADIUS)
                .inner_margin(Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(560.0));
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&t.message).size(12.0).color(RED_400));
                        if widgets::icon_button(ui, "🗙", "Dismiss error").clicked() {
                            out.push(Msg::DismissToast);
                        }
                    });
                });
        });
}
