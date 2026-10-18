//! Settings overlay (`SettingsPage.tsx`, `HotkeyRecorder.tsx`,
//! `FileAccessDiagnostics.tsx`).

use cdout_core::utils::config::ApprovalMode;
use eframe::egui::{
    self, Color32, Event, Key, Margin, Modifiers, RichText, ScrollArea, Stroke, TextEdit, Ui,
};

use super::theme::*;
use super::{icons, widgets};
use crate::state::{AppState, Msg, SettingsTab};

const TABS: [(SettingsTab, &str, &str); 5] = [
    (SettingsTab::General, "⌨", "General"),
    (SettingsTab::Providers, "🗝", "Providers"),
    (SettingsTab::Approval, "🛡", "Approval"),
    (SettingsTab::Skills, "★", "Skills"),
    (SettingsTab::Diagnostics, "🛠", "Diagnostics"),
];

pub fn show(ui: &mut Ui, state: &mut AppState, out: &mut Vec<Msg>) {
    if ui.input(|i| i.key_pressed(Key::Escape)) && !state.settings.recording_hotkey {
        out.push(Msg::CloseSettings);
    }
    egui::Panel::left("settings-nav")
        .exact_size(200.0)
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(GRAY_900)
                .inner_margin(Margin::same(12)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Settings").strong().color(GRAY_200));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, "🗙", "Close settings").clicked() {
                        out.push(Msg::CloseSettings);
                    }
                });
            });
            ui.add_space(8.0);
            for (tab, icon, label) in TABS {
                let active = state.settings.tab == tab;
                let r = ui.add(
                    egui::Button::new(RichText::new(format!("{icon}  {label}")).color(if active {
                        INDIGO_300
                    } else {
                        GRAY_400
                    }))
                    .fill(if active {
                        INDIGO_600.gamma_multiply(0.2)
                    } else {
                        Color32::TRANSPARENT
                    })
                    .min_size(egui::vec2(ui.available_width(), 30.0)),
                );
                let l = label.to_string();
                r.widget_info(move || {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::SelectableLabel,
                        true,
                        active,
                        l.clone(),
                    )
                });
                if r.clicked() {
                    out.push(Msg::SetSettingsTab(tab));
                }
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                if ui.add(widgets::secondary_button("Close")).clicked() {
                    out.push(Msg::CloseSettings);
                }
                if ui
                    .add(
                        widgets::primary_button("Save")
                            .min_size(egui::vec2(ui.available_width(), 28.0)),
                    )
                    .clicked()
                {
                    out.push(Msg::SaveSettings);
                }
            });
        });
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(GRAY_950)
                .inner_margin(Margin::same(24)),
        )
        .show(ui, |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(640.0));
                    match state.settings.tab {
                        SettingsTab::General => general(ui, state, out),
                        SettingsTab::Providers => providers(ui, state, out),
                        SettingsTab::Approval => approval(ui, state, out),
                        SettingsTab::Skills => skills(ui, state),
                        SettingsTab::Diagnostics => diagnostics(ui, state, out),
                    }
                });
        });
}

fn header(ui: &mut Ui, title: &str, description: &str) {
    ui.label(RichText::new(title).size(18.0).strong().color(GRAY_100));
    ui.label(RichText::new(description).size(12.0).color(GRAY_500));
    ui.add_space(12.0);
}

fn sub(ui: &mut Ui, title: &str, description: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(title).size(14.0).strong().color(GRAY_200));
    ui.label(RichText::new(description).size(11.0).color(GRAY_500));
    ui.add_space(6.0);
}

fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(GRAY_600));
}

// --- General: hotkey ---

const MOD_ORDER: [&str; 5] = ["Ctrl", "Alt", "Shift", "Cmd", "Super"];

/// Port of `prettifyHotkey`.
pub fn prettify_hotkey(hotkey: &str, mac: bool) -> Vec<String> {
    hotkey
        .split('+')
        .map(|part| {
            let t = part.trim();
            if mac {
                match t {
                    "Cmd" => return "⌘".into(),
                    "Alt" => return "Opt".into(),
                    "Ctrl" => return "Ctrl".into(),
                    "Shift" => return "Shift".into(),
                    _ => {}
                }
            }
            match t {
                "CommandOrControl" | "CmdOrCtrl" => return if mac { "⌘" } else { "Ctrl" }.into(),
                "Super" => return if mac { "⌘" } else { "Win" }.into(),
                "ArrowUp" => return "Up".into(),
                "ArrowDown" => return "Down".into(),
                "ArrowLeft" => return "Left".into(),
                "ArrowRight" => return "Right".into(),
                _ => {}
            }
            if let Some(d) = t.strip_prefix("Digit") {
                return d.into();
            }
            if let Some(k) = t.strip_prefix("Key") {
                if k.len() == 1 {
                    return k.into();
                }
            }
            t.into()
        })
        .collect()
}

/// Port of `canonicalizeHotkey`: same chord, same string.
pub fn canonicalize_hotkey(hotkey: &str, mac: bool) -> String {
    let mut mods: Vec<&str> = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    for raw in hotkey.split('+') {
        let mut t = raw.trim();
        if t.is_empty() {
            continue;
        }
        t = match t {
            "Control" => "Ctrl",
            "Command" | "Meta" => "Cmd",
            "Option" => "Alt",
            "CommandOrControl" | "CmdOrCtrl" => {
                if mac {
                    "Cmd"
                } else {
                    "Ctrl"
                }
            }
            "Super" if mac => "Cmd",
            other => other,
        };
        if let Some(m) = MOD_ORDER.iter().find(|m| **m == t) {
            if !mods.contains(m) {
                mods.push(m);
            }
        } else if t.len() == 4 && t.starts_with("Key") && t.as_bytes()[3].is_ascii_uppercase() {
            keys.push(t[3..].to_string());
        } else if t.chars().count() == 1 {
            keys.push(t.to_uppercase());
        } else {
            keys.push(t.to_string());
        }
    }
    mods.sort_by_key(|m| MOD_ORDER.iter().position(|x| x == m));
    mods.into_iter()
        .map(String::from)
        .chain(keys)
        .collect::<Vec<_>>()
        .join("+")
}

fn key_code(key: Key) -> Option<String> {
    use Key::*;
    let name = match key {
        A | B | C | D | E | F | G | H | I | J | K | L | M | N | O | P | Q | R | S | T | U | V
        | W | X | Y | Z => key.name().to_uppercase(),
        Num0 | Num1 | Num2 | Num3 | Num4 | Num5 | Num6 | Num7 | Num8 | Num9 => {
            format!("Digit{}", key.name())
        }
        F1 | F2 | F3 | F4 | F5 | F6 | F7 | F8 | F9 | F10 | F11 | F12 | F13 | F14 | F15 | F16
        | F17 | F18 | F19 | F20 | F21 | F22 | F23 | F24 => key.name().to_string(),
        Space => "Space".into(),
        Enter => "Enter".into(),
        Tab => "Tab".into(),
        Backspace => "Backspace".into(),
        Delete => "Delete".into(),
        Insert => "Insert".into(),
        Home => "Home".into(),
        End => "End".into(),
        PageUp => "PageUp".into(),
        PageDown => "PageDown".into(),
        ArrowUp => "ArrowUp".into(),
        ArrowDown => "ArrowDown".into(),
        ArrowLeft => "ArrowLeft".into(),
        ArrowRight => "ArrowRight".into(),
        _ => return None,
    };
    Some(name)
}

/// Port of `chordFromEvent`: a chord string the hotkey parser accepts, or
/// why this key press can't be one.
pub fn chord_from_key(key: Key, m: Modifiers, mac: bool) -> Result<String, String> {
    let mut mods = Vec::new();
    if m.ctrl {
        mods.push("Ctrl");
    }
    if m.alt {
        mods.push("Alt");
    }
    if m.shift {
        mods.push("Shift");
    }
    if mac && m.mac_cmd {
        mods.push("Cmd");
    }
    if mods.is_empty() {
        return Err(
            "Needs at least one modifier — a bare key would be captured system-wide.".into(),
        );
    }
    let code =
        key_code(key).ok_or_else(|| "That key can't be used in a global hotkey.".to_string())?;
    let chord = mods
        .into_iter()
        .map(String::from)
        .chain([code])
        .collect::<Vec<_>>()
        .join("+");
    crate::hotkey::parse(&chord)?;
    Ok(chord)
}

fn general(ui: &mut Ui, state: &mut AppState, out: &mut Vec<Msg>) {
    let mac = state.platform.os == "macos";
    header(
        ui,
        "General",
        "The global hotkey that opens the spotlight prompt over any window.",
    );
    sub(
        ui,
        "Spotlight hotkey",
        "Press the combination you want. It takes effect immediately — no restart. If another app already owns it, the OS refuses and cdout keeps the previous one.",
    );

    let s = &mut state.settings;
    if s.recording_hotkey {
        let pressed = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => Some((*key, *modifiers)),
                _ => None,
            })
        });
        if let Some((key, mods)) = pressed {
            if key == Key::Escape {
                s.recording_hotkey = false;
                s.hotkey_error = None;
            } else {
                match chord_from_key(key, mods, mac) {
                    Ok(chord) => {
                        s.recording_hotkey = false;
                        if canonicalize_hotkey(&chord, mac) != canonicalize_hotkey(&s.hotkey, mac) {
                            out.push(Msg::ChangeHotkey(chord));
                        }
                    }
                    Err(e) => s.hotkey_error = Some(e),
                }
            }
        }
    }

    ui.horizontal(|ui| {
        let text = if s.recording_hotkey {
            "Press the keys… (Esc to cancel)".to_string()
        } else {
            format!("⌨  {}", prettify_hotkey(&s.hotkey, mac).join(" + "))
        };
        let b = egui::Button::new(RichText::new(text).color(if s.recording_hotkey {
            INDIGO_300
        } else {
            GRAY_200
        }))
        .fill(if s.recording_hotkey {
            INDIGO_600.gamma_multiply(0.2)
        } else {
            BLACK_40
        })
        .stroke(Stroke::new(
            1.0,
            if s.recording_hotkey {
                INDIGO_500
            } else {
                GRAY_700
            },
        ));
        let r = ui.add(b);
        r.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                "Change the spotlight hotkey",
            )
        });
        if r.clicked() {
            s.hotkey_error = None;
            s.recording_hotkey = !s.recording_hotkey;
        }
        let default = state.platform.default_hotkey;
        if canonicalize_hotkey(&s.hotkey, mac) != canonicalize_hotkey(default, mac)
            && ui.add(widgets::secondary_button("Reset")).clicked()
        {
            out.push(Msg::ChangeHotkey(default.to_string()));
        }
    });
    if let Some(e) = &s.hotkey_error {
        ui.label(RichText::new(e).size(11.0).color(RED_400));
    }
}

// --- Providers ---

fn providers(ui: &mut Ui, state: &mut AppState, out: &mut Vec<Msg>) {
    header(
        ui,
        "Providers",
        "Configure where cdout sends model requests.",
    );
    sub(
        ui,
        "Subscriptions",
        "Agent CLIs signed in on this machine. cdout runs them with their own tools switched off, so your plan covers the usage and every command still waits for your approval.",
    );
    super::onboarding::subscription_rows(ui, state, out);
    let s = &mut state.settings;
    sub(
        ui,
        "Local",
        "Self-hosted models on your own machine. Stays airtight — prompts never leave the box.",
    );
    ui.label(
        RichText::new("Ollama Server URL")
            .size(12.0)
            .color(GRAY_300),
    );
    ui.add(
        TextEdit::singleline(&mut s.draft_ollama_url)
            .hint_text("http://localhost:11434")
            .desired_width(f32::INFINITY),
    )
    .widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Ollama Server URL")
    });
    hint(
        ui,
        "Defaults to http://localhost:11434. Ollama models are prefixed `ollama:` in the picker.",
    );

    sub(ui, "API keys", "Pay-per-use keys for hosted providers, stored locally in your config. Prompts go through openrouter.ai, or straight to Anthropic with an Anthropic key.");
    key_field(
        ui,
        "OpenRouter API Key",
        &mut s.openrouter_draft,
        s.keys.openrouter_preview.as_deref(),
        "sk-or-v1-...",
        "Primary cloud gateway. Single key, ~400 models including Claude Opus/Sonnet 4.7+, GPT-5, Gemini 3.",
        "https://openrouter.ai/keys",
        out,
    );
    key_field(
        ui,
        "Anthropic API Key (optional)",
        &mut s.anthropic_draft,
        s.keys.anthropic_preview.as_deref(),
        "sk-ant-api-...",
        "Enables the direct Anthropic path with prompt caching — OpenRouter's OpenAI-compat wire drops cache_control. Surfaces `anthropic:claude-opus-4-7` and friends in the picker.",
        "https://console.anthropic.com/settings/keys",
        out,
    );
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width(ui.available_width().min(480.0));
            ui.label(RichText::new("Show OpenRouter free-tier models").size(12.0).color(GRAY_300));
            hint(ui, "Off by default. Free models have documented FP4/Int4 quantization swaps that break non-Latin output, and ~20 req/min caps.");
        });
        let label = if s.show_free { "Showing" } else { "Hidden" };
        let r = ui.add(
            egui::Button::new(RichText::new(label).size(11.0).color(if s.show_free { INDIGO_300 } else { GRAY_400 }))
                .fill(if s.show_free { INDIGO_600.gamma_multiply(0.3) } else { GRAY_800 }),
        );
        r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Show free-tier models"));
        if r.clicked() {
            out.push(Msg::ToggleShowFree);
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn key_field(
    ui: &mut Ui,
    label: &str,
    draft: &mut String,
    preview: Option<&str>,
    placeholder: &str,
    hint_text: &str,
    url: &str,
    out: &mut Vec<Msg>,
) {
    ui.add_space(6.0);
    ui.label(RichText::new(label).size(12.0).color(GRAY_300));
    let ph = match preview {
        Some(p) => format!("Saved: {p}"),
        None => placeholder.to_string(),
    };
    let l = label.to_string();
    ui.add(
        TextEdit::singleline(draft)
            .password(true)
            .hint_text(ph)
            .desired_width(f32::INFINITY),
    )
    .widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, l.clone()));
    let h = match preview {
        Some(p) => format!("Current key on file: {p}. Type a new value to replace."),
        None => hint_text.to_string(),
    };
    hint(ui, &h);
    if widgets::link_label(
        ui,
        RichText::new("Get a key ⬈").size(11.0).color(INDIGO_300),
    )
    .clicked()
    {
        out.push(Msg::OpenUrl(url.to_string()));
    }
}

// --- Approval ---

const APPROVAL_OPTIONS: [(ApprovalMode, &str, &str, bool); 3] = [
    (ApprovalMode::Ask, "Ask every time", "Nothing runs until you approve it. The most cautious setting, and the most clicks.", false),
    (ApprovalMode::ReadOnly, "Auto-run read-only commands", "Probes that cannot change anything — reading a file's metadata, listing a folder, testing whether a path exists — run straight away. Anything that writes, moves or deletes still waits for you. A command the classifier does not recognise counts as writing, so the worst case is an extra prompt.", false),
    (ApprovalMode::Auto, "Auto-run everything", "The agent executes whatever it proposes, including commands that overwrite or delete files. Loop detection, the consecutive-failure pause and the step cap still apply, and recognisably destructive commands (rm -rf /, sudo, disk formatting) always stop for approval.", true),
];

fn approval(ui: &mut Ui, state: &mut AppState, out: &mut Vec<Msg>) {
    header(ui, "Approval", "How much cdout may run without asking. Every command is still shown and editable before it runs when approval is required.");
    for (mode, label, detail, risky) in APPROVAL_OPTIONS {
        let active = state.settings.approval_mode == mode;
        let (fill, stroke) = match (active, risky) {
            (true, true) => (RED_950, RED_500.gamma_multiply(0.4)),
            (true, false) => (
                INDIGO_600.gamma_multiply(0.15),
                INDIGO_500.gamma_multiply(0.4),
            ),
            _ => (GRAY_800.gamma_multiply(0.4), GRAY_700),
        };
        let resp = egui::Frame::new()
            .fill(fill)
            .stroke(Stroke::new(1.0, stroke))
            .corner_radius(RADIUS)
            .inner_margin(Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if active { "◎" } else { "○" }).color(if active {
                            INDIGO_300
                        } else {
                            GRAY_600
                        }),
                    );
                    ui.label(RichText::new(label).strong().color(if active {
                        GRAY_100
                    } else {
                        GRAY_300
                    }));
                    if mode == ApprovalMode::ReadOnly {
                        widgets::badge(ui, "default", GRAY_400, GRAY_800);
                    }
                });
                ui.label(RichText::new(detail).size(11.0).color(GRAY_500));
            })
            .response;
        let r = ui.interact(resp.rect, ui.id().with(label), egui::Sense::click());
        r.widget_info(move || {
            egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, active, label)
        });
        if r.clicked() && !active {
            out.push(Msg::SetApprovalMode(mode));
        }
        ui.add_space(6.0);
    }
    let mod_key = if state.platform.os == "macos" {
        "⌘"
    } else {
        "Ctrl"
    };
    hint(ui, &format!("Per-task override: press {mod_key}+Enter in the spotlight to run one task unattended without changing this setting — that way you do not have to wait for the first proposal just to click All."));
}

// --- Skills ---

fn skills(ui: &mut Ui, state: &AppState) {
    header(ui, "Skills", "Capabilities injected into the agent's system prompt. Availability depends on whether required binaries are on PATH.");
    let Some(list) = &state.settings.skills else {
        hint(ui, "Loading…");
        return;
    };
    if list.skills.is_empty() {
        hint(ui, "No skills loaded.");
    }
    for skill in &list.skills {
        let (fill, stroke, name_color) = if skill.available {
            (
                EMERALD_600.gamma_multiply(0.12),
                EMERALD_500.gamma_multiply(0.3),
                Color32::from_rgb(0x6e, 0xe7, 0xb7),
            )
        } else {
            (GRAY_800.gamma_multiply(0.4), GRAY_700, GRAY_400)
        };
        widgets::card(fill, stroke).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_max_width(ui.available_width() - 160.0);
                    ui.label(
                        RichText::new(&skill.metadata.name)
                            .strong()
                            .color(name_color),
                    );
                    ui.label(
                        RichText::new(&skill.metadata.description)
                            .size(11.0)
                            .color(GRAY_500),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if skill.available {
                        icons::icon(ui, 14.0, |p, r| icons::paint_check(p, r, EMERALD_500));
                    } else {
                        ui.label(
                            RichText::new(format!("× Missing: {}", skill.missing_bins.join(", ")))
                                .size(11.0)
                                .color(GRAY_500),
                        );
                    }
                });
            });
        });
        ui.add_space(4.0);
    }
    if !list.quarantined.is_empty() {
        ui.add_space(10.0);
        ui.label(RichText::new("Quarantined").strong().color(RED_400));
        hint(
            ui,
            "These user skills matched a safety rule and are NOT given to the agent.",
        );
        for q in &list.quarantined {
            widgets::card(RED_950, RED_600.gamma_multiply(0.4)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(&q.metadata.name).strong().color(RED_400));
                ui.label(
                    RichText::new(&q.source_path)
                        .size(10.0)
                        .monospace()
                        .color(GRAY_500),
                );
                for hit in &q.safety_hits {
                    ui.label(
                        RichText::new(format!("• {}: \"{}\"", hit.rule, hit.snippet))
                            .size(11.0)
                            .color(GRAY_400),
                    );
                }
            });
            ui.add_space(4.0);
        }
    }
}

// --- Diagnostics ---

/// What the diagnostics card should say, from the raw debug payload.
#[derive(Debug, PartialEq, Eq)]
pub enum AccessVerdict {
    Loading,
    Denied,
    Reading { path: String, selected: Option<i64> },
    NoWindow,
}

pub fn access_verdict(
    mac: bool,
    loading: bool,
    debug: Option<&Result<serde_json::Value, String>>,
) -> AccessVerdict {
    if loading || debug.is_none() {
        return AccessVerdict::Loading;
    }
    let debug = debug.unwrap();
    let info = debug.as_ref().ok();
    let denied = mac
        && (debug.is_err()
            || info
                .and_then(|i| i.get("automation_authorized"))
                .and_then(|v| v.as_bool())
                == Some(false));
    if denied {
        return AccessVerdict::Denied;
    }
    match info
        .and_then(|i| i.get("resolved_path"))
        .and_then(|v| v.as_str())
    {
        Some(p) if !p.is_empty() => AccessVerdict::Reading {
            path: p.to_string(),
            selected: info
                .and_then(|i| i.get("selected_count"))
                .and_then(|v| v.as_i64()),
        },
        _ => AccessVerdict::NoWindow,
    }
}

fn diagnostics(ui: &mut Ui, state: &AppState, out: &mut Vec<Msg>) {
    let fm = state.platform.file_manager;
    header(
        ui,
        "Diagnostics",
        &format!("Why the agent can or cannot see your {fm} selection."),
    );
    let mac = state.platform.os == "macos";
    let verdict = access_verdict(
        mac,
        state.settings.explorer_debug_loading,
        state.settings.explorer_debug.as_ref(),
    );
    let (fill, stroke, title_color, title) = match &verdict {
        AccessVerdict::Loading => (
            GRAY_800.gamma_multiply(0.4),
            GRAY_700,
            GRAY_300,
            format!("Checking {fm} access…"),
        ),
        AccessVerdict::Denied => (
            RED_950,
            RED_500.gamma_multiply(0.3),
            RED_400,
            format!("cdout cannot read {fm}"),
        ),
        _ => (
            EMERALD_600.gamma_multiply(0.12),
            EMERALD_500.gamma_multiply(0.3),
            Color32::from_rgb(0x6e, 0xe7, 0xb7),
            format!("{fm} access is working"),
        ),
    };
    widgets::card(fill, stroke).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong().color(title_color));
        let detail = match &verdict {
            AccessVerdict::Loading => String::new(),
            AccessVerdict::Denied => format!("Automation permission was denied, so the agent never sees your open folder or selected files. Enable cdout › {fm} under Privacy & Security › Automation, then re-check."),
            AccessVerdict::Reading { path, selected } => match selected {
                Some(n) => format!("Reading {path} · {n} file(s) selected"),
                None => format!("Reading {path}"),
            },
            AccessVerdict::NoWindow => format!("No {fm} window is open right now, so there is nothing to read. Open one and re-check."),
        };
        if !detail.is_empty() {
            ui.label(RichText::new(detail).size(11.0).color(GRAY_400));
        }
        ui.horizontal(|ui| {
            if verdict == AccessVerdict::Denied && ui.add(widgets::secondary_button("Open Settings ⬈")).clicked() {
                out.push(Msg::OpenFileAccessSettings);
            }
            if ui
                .add_enabled(!state.settings.explorer_debug_loading, widgets::secondary_button("⟳ Re-check"))
                .clicked()
            {
                out.push(Msg::LoadExplorerDebug);
            }
        });
    });
    if let Some(debug) = &state.settings.explorer_debug {
        ui.add_space(6.0);
        egui::CollapsingHeader::new(RichText::new("Raw diagnostics").size(11.0).color(GRAY_500))
            .show(ui, |ui| {
                let text = match debug {
                    Ok(v) => serde_json::to_string_pretty(v).unwrap_or_default(),
                    Err(e) => e.clone(),
                };
                ui.label(RichText::new(text).monospace().size(10.5).color(GRAY_400));
            });
    }
    ui.add_space(10.0);
    hint(
        ui,
        &format!(
            "Shell: {} · tool: {}",
            state.platform.shell_name, state.platform.shell_tool_name
        ),
    );
    hint(ui, &format!("Platform: {}", state.platform.os_name));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prettify_matches_the_frontend() {
        assert_eq!(prettify_hotkey("Cmd+Alt+A", true), vec!["⌘", "Opt", "A"]);
        assert_eq!(
            prettify_hotkey("Ctrl+Alt+A", false),
            vec!["Ctrl", "Alt", "A"]
        );
        assert_eq!(
            prettify_hotkey("CommandOrControl+Digit1", false),
            vec!["Ctrl", "1"]
        );
        assert_eq!(prettify_hotkey("Super+KeyK", false), vec!["Win", "K"]);
        assert_eq!(prettify_hotkey("Ctrl+ArrowUp", false), vec!["Ctrl", "Up"]);
    }

    #[test]
    fn canonical_form_ignores_order_and_aliases() {
        assert_eq!(canonicalize_hotkey("Alt+Cmd+A", true), "Alt+Cmd+A");
        assert_eq!(
            canonicalize_hotkey("Cmd+Alt+A", true),
            canonicalize_hotkey("Alt+Command+a", true)
        );
        assert_eq!(
            canonicalize_hotkey("CmdOrCtrl+Shift+KeyK", false),
            "Ctrl+Shift+K"
        );
        assert_eq!(canonicalize_hotkey("Super+K", true), "Cmd+K");
        assert_eq!(canonicalize_hotkey("Super+K", false), "Super+K");
    }

    #[test]
    fn chords_need_a_modifier_and_a_supported_key() {
        let ctrl_alt = Modifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        };
        assert_eq!(
            chord_from_key(Key::A, ctrl_alt, false).unwrap(),
            "Ctrl+Alt+A"
        );
        assert_eq!(
            chord_from_key(
                Key::F9,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
                false
            )
            .unwrap(),
            "Shift+F9"
        );
        assert_eq!(
            chord_from_key(Key::Num5, ctrl_alt, false).unwrap(),
            "Ctrl+Alt+Digit5"
        );
        assert!(chord_from_key(Key::A, Modifiers::NONE, false)
            .unwrap_err()
            .starts_with("Needs at least one modifier"));
        let cmd = Modifiers {
            mac_cmd: true,
            alt: true,
            ..Default::default()
        };
        assert_eq!(chord_from_key(Key::A, cmd, true).unwrap(), "Alt+Cmd+A");
    }

    #[test]
    fn diagnostics_verdicts() {
        use serde_json::json;
        assert_eq!(access_verdict(false, true, None), AccessVerdict::Loading);
        let denied = Ok(json!({"automation_authorized": false}));
        assert_eq!(
            access_verdict(true, false, Some(&denied)),
            AccessVerdict::Denied
        );
        assert_eq!(
            access_verdict(true, false, Some(&Err("x".into()))),
            AccessVerdict::Denied
        );
        assert_eq!(
            access_verdict(false, false, Some(&Err("x".into()))),
            AccessVerdict::NoWindow
        );
        let reading = Ok(json!({"resolved_path": "/Users/me", "selected_count": 3}));
        assert_eq!(
            access_verdict(true, false, Some(&reading)),
            AccessVerdict::Reading {
                path: "/Users/me".into(),
                selected: Some(3)
            }
        );
    }
}
