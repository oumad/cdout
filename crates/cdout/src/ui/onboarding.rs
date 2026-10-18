//! First-run provider setup and the one-shot banner about credential files
//! left by older versions.

use cdout_core::llm::clients::cli::CliStatus;
use eframe::egui::{self, Color32, Key, Margin, RichText, Stroke, TextEdit, Ui};

use super::theme::*;
use super::{icons, widgets};
use crate::state::{AppState, Msg};

/// Shown instead of the empty chat when no model is usable.
pub fn should_show(state: &AppState) -> bool {
    state.chat.history.is_empty()
        && state.models.connected.is_some()
        && state.models.available.is_empty()
}

pub fn provider_setup(ui: &mut Ui, state: &mut AppState, out: &mut Vec<Msg>) {
    let saved_url = state.settings.ollama_url.clone();
    ui.vertical_centered(|ui| {
        ui.add_space(24.0);
        icons::icon(ui, 32.0, |p, r| icons::paint_bot(p, r, INDIGO_500));
        ui.label(RichText::new("Set up a model to get started").size(18.0).strong().color(GRAY_100));
        ui.label(
            RichText::new("cdout needs a model to think. Any of these works, and you can switch anytime from the picker.")
                .size(12.0)
                .color(GRAY_500),
        );
        ui.add_space(14.0);
    });

    let status = state.provider_status.clone();
    widgets::card(GRAY_900, GRAY_800).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new("Use a subscription you already have").strong().color(GRAY_100));
        ui.label(RichText::new("NO API KEY").size(10.0).strong().color(EMERALD_500));
        ui.label(
            RichText::new("Signed in to Claude Code or Codex on this machine? cdout runs it for you and your plan covers the usage. Commands still wait for your approval.")
                .size(11.0)
                .color(GRAY_400),
        );
        ui.add_space(6.0);
        subscription_rows(ui, state, out);
        ui.add_space(4.0);
        if ui.add(widgets::secondary_button("⟳ Re-check")).clicked() {
            out.push(Msg::OnboardingRecheck);
        }
    });
    ui.add_space(10.0);

    let s = &mut state.settings;
    ui.columns(2, |cols| {
        // Local
        widgets::card(GRAY_900, GRAY_800).show(&mut cols[0], |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("🖥 Local").strong().color(GRAY_100));
            ui.label(RichText::new("PRIVATE · FREE").size(10.0).strong().color(EMERALD_500));
            ui.label(
                RichText::new("Run models with Ollama — on this machine or another one on your network. Prompts never leave your hardware.")
                    .size(11.0)
                    .color(GRAY_400),
            );
            ui.add_space(6.0);
            match &status {
                None => {
                    ui.horizontal(|ui| {
                        widgets::spinner(ui, 11.0);
                        ui.label(RichText::new("Checking for Ollama…").size(11.0).color(GRAY_500));
                    });
                }
                Some(st) if st.ollama_reachable => {
                    ui.label(RichText::new("Ollama is running").size(12.0).color(EMERALD_500));
                    ui.label(RichText::new("No models yet — pull one that supports tool calling, then re-check:").size(11.0).color(GRAY_400));
                    ui.label(RichText::new("ollama pull llama3.2").monospace().size(11.0).color(GRAY_300));
                }
                Some(_) => {
                    ui.label(RichText::new(format!("No Ollama at {saved_url}")).size(12.0).color(AMBER_400));
                    ui.label(
                        RichText::new("Install it here and re-check, or point cdout at another machine below.")
                            .size(11.0)
                            .color(GRAY_400),
                    );
                }
            }
            ui.add_space(6.0);
            ui.label(RichText::new("🖧 Already running on another machine?").size(11.0).color(GRAY_400));
            ui.horizontal(|ui| {
                let r = ui.add(
                    TextEdit::singleline(&mut s.onboarding_url_draft)
                        .hint_text("http://192.168.1.50:11434")
                        .desired_width(ui.available_width() - 90.0),
                );
                r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Ollama server URL"));
                let trimmed = s.onboarding_url_draft.trim().to_string();
                let can = !trimmed.is_empty() && trimmed != saved_url;
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                if (ui.add_enabled(can, widgets::secondary_button("Connect")).clicked() || enter) && can {
                    out.push(Msg::OnboardingSaveOllamaUrl(trimmed));
                }
            });
            ui.horizontal(|ui| {
                if ui.add(widgets::secondary_button("⟳ Re-check")).clicked() {
                    out.push(Msg::OnboardingRecheck);
                }
                let unreachable = status.as_ref().is_some_and(|s| !s.ollama_reachable);
                if unreachable
                    && widgets::link_label(ui, RichText::new("Get Ollama ⬈").size(11.0).color(INDIGO_300)).clicked()
                {
                    out.push(Msg::OpenUrl("https://ollama.com/download".into()));
                }
            });
        });

        // Cloud
        widgets::card(GRAY_900, GRAY_800).show(&mut cols[1], |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("API key").strong().color(GRAY_100));
            ui.label(RichText::new("PAY PER USE").size(10.0).strong().color(INDIGO_300));
            ui.label(
                RichText::new("An OpenRouter key reaches Claude, GPT, Gemini and more, billed per request. Paste it to start right away.")
                    .size(11.0)
                    .color(GRAY_400),
            );
            ui.add_space(6.0);
            let r = ui.add(
                TextEdit::singleline(&mut s.onboarding_key_draft)
                    .password(true)
                    .hint_text("sk-or-v1-...")
                    .desired_width(f32::INFINITY),
            );
            r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "OpenRouter API key"));
            let can = !s.onboarding_key_draft.trim().is_empty();
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if (ui
                .add_enabled(can, widgets::primary_button("Save key & start").min_size(egui::vec2(ui.available_width(), 28.0)))
                .clicked()
                || enter)
                && can
            {
                out.push(Msg::OnboardingSaveKey(s.onboarding_key_draft.clone()));
            }
            if widgets::link_label(ui, RichText::new("Get a key ⬈").size(11.0).color(INDIGO_300)).clicked() {
                out.push(Msg::OpenUrl("https://openrouter.ai/keys".into()));
            }
        });
    });
    ui.vertical_centered(|ui| {
        ui.add_space(10.0);
        if widgets::link_label(
            ui,
            RichText::new("⚙ More options (Anthropic direct, free models) in Settings")
                .size(11.0)
                .color(GRAY_500),
        )
        .clicked()
        {
            out.push(Msg::OpenSettings);
        }
    });
}

/// One line per agent CLI: signed in, installed but signed out (with a Sign
/// in button that opens the CLI's own login), or not installed (with an
/// install link). Before the first check answers, a plain note; no spinner,
/// since a failed check never answers.
pub fn subscription_rows(ui: &mut Ui, state: &AppState, out: &mut Vec<Msg>) {
    let statuses: Option<&[CliStatus]> = state
        .provider_status
        .as_ref()
        .map(|p| p.subscriptions.as_slice());
    let signing_in = state.sign_in.map(|(b, _)| b);
    let Some(statuses) = statuses else {
        ui.label(
            RichText::new("Looking for Claude Code and Codex…")
                .size(11.0)
                .color(GRAY_500),
        );
        return;
    };
    for st in statuses {
        ui.horizontal_wrapped(|ui| {
            let (color, state) = if st.signed_in {
                (EMERALD_500, format!("signed in · {}", st.detail))
            } else if st.installed {
                (AMBER_400, "installed, not signed in".to_string())
            } else {
                (GRAY_600, "not installed".to_string())
            };
            let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
            ui.painter().circle_filled(dot.center(), 3.5, color);
            ui.label(RichText::new(st.name).size(12.0).strong().color(GRAY_200));
            ui.label(RichText::new(state).size(11.0).color(color));
            if st.installed && !st.signed_in && signing_in == Some(st.backend) {
                ui.label(
                    RichText::new(
                        "Finish signing in in the window that opened; cdout notices on its own.",
                    )
                    .size(11.0)
                    .color(GRAY_400),
                );
            } else if st.installed && !st.signed_in {
                let r = ui.add(widgets::primary_button("Sign in"));
                r.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        format!("Sign in to {}", st.name),
                    )
                });
                if r.clicked() {
                    out.push(Msg::SignInCli(st.backend));
                }
                ui.label(RichText::new("or run").size(11.0).color(GRAY_500));
                ui.label(
                    RichText::new(st.sign_in_command)
                        .monospace()
                        .size(11.0)
                        .color(GRAY_300),
                );
            } else if !st.installed
                && widgets::link_label(
                    ui,
                    RichText::new(format!("Get {} ⬈", st.name))
                        .size(11.0)
                        .color(INDIGO_300),
                )
                .clicked()
            {
                out.push(Msg::OpenUrl(st.install_url.to_string()));
            }
        });
    }
}

pub fn migration_banner(ui: &mut Ui, out: &mut Vec<Msg>) {
    egui::Frame::new()
        .fill(AMBER_950)
        .stroke(Stroke::new(1.0, AMBER_500.gamma_multiply(0.4)))
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_max_width(ui.available_width() - 30.0);
                    ui.label(RichText::new("Old credentials found").strong().color(AMBER_300));
                    ui.label(
                        RichText::new("An earlier version of cdout left API keys or a login file on disk that it no longer uses. Your providers are set up in Settings › Providers.")
                            .size(11.0)
                            .color(Color32::from_rgb(0xfd, 0xe6, 0x8a)),
                    );
                    ui.horizontal(|ui| {
                        if ui.add(widgets::warn_button("Open Settings")).clicked() {
                            out.push(Msg::OpenSettingsFromBanner);
                        }
                        if ui.add(widgets::secondary_button("🗑 Remove the old file")).clicked() {
                            out.push(Msg::CleanupLegacy);
                        }
                    });
                });
                if widgets::icon_button(ui, "🗙", "Dismiss banner").clicked() {
                    out.push(Msg::DismissBanner);
                }
            });
        });
}
