//! Provider-grouped model picker (port of `ModelSelector.tsx`).

use cdout_core::llm::clients::cli;
use eframe::egui::{self, RichText, Ui};

use super::theme::*;
use crate::state::text::truncate_middle;
use crate::state::Msg;

pub struct Groups<'a> {
    pub subscriptions: Vec<&'a str>,
    pub local: Vec<&'a str>,
    pub anthropic_direct: Vec<&'a str>,
    pub openrouter: Vec<&'a str>,
}

pub fn group_models(models: &[String]) -> Groups<'_> {
    let mut g = Groups {
        subscriptions: Vec::new(),
        local: Vec::new(),
        anthropic_direct: Vec::new(),
        openrouter: Vec::new(),
    };
    for m in models {
        if cli::route(m).is_some() {
            g.subscriptions.push(m);
        } else if m.starts_with("ollama:") {
            g.local.push(m);
        } else if m.starts_with("anthropic:") {
            g.anthropic_direct.push(m);
        } else {
            g.openrouter.push(m);
        }
    }
    g
}

/// The visible name drops the prefix its group already conveys; a CLI
/// model is named after its CLI, since an alias like `sonnet` alone is vague.
pub fn display_name(slug: &str) -> String {
    if let Some((backend, model)) = cli::route(slug) {
        let model = if model == "default" {
            "default model"
        } else {
            model
        };
        return format!("{} · {}", backend.name(), model);
    }
    slug.strip_prefix("ollama:")
        .or_else(|| slug.strip_prefix("anthropic:"))
        .unwrap_or(slug)
        .to_string()
}

/// Picker with an accessible name of "Model". Emits [`Msg::SelectModel`].
pub fn show(
    ui: &mut Ui,
    id_salt: &str,
    selected: &str,
    models: &[String],
    connected: Option<bool>,
    max_chars: usize,
    out: &mut Vec<Msg>,
) {
    let selected_text = if models.is_empty() {
        match connected {
            None => "Checking...".to_string(),
            Some(_) => "No models — configure a provider in Settings".to_string(),
        }
    } else if selected.is_empty() {
        "Select a model".to_string()
    } else {
        truncate_middle(&display_name(selected), max_chars)
    };
    let color = if models.is_empty() && connected.is_some() {
        AMBER_400
    } else {
        GRAY_400
    };

    ui.add_enabled_ui(!models.is_empty(), |ui| {
        let combo = egui::ComboBox::from_id_salt(id_salt)
            .selected_text(RichText::new(selected_text).size(11.0).color(color))
            .width(0.0)
            .show_ui(ui, |ui| {
                let groups = group_models(models);
                for (title, items) in [
                    ("Your subscriptions", &groups.subscriptions),
                    ("Local (Ollama)", &groups.local),
                    ("Anthropic (direct · cached)", &groups.anthropic_direct),
                    ("OpenRouter", &groups.openrouter),
                ] {
                    if items.is_empty() {
                        continue;
                    }
                    ui.label(RichText::new(title).size(10.0).strong().color(GRAY_500));
                    for &m in items {
                        let text = truncate_middle(&display_name(m), max_chars.max(24));
                        if ui
                            .selectable_label(m == selected, text)
                            .on_hover_text(m)
                            .clicked()
                            && m != selected
                        {
                            out.push(Msg::SelectModel(m.to_string()));
                        }
                    }
                }
            });
        combo
            .response
            .on_hover_text(selected)
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, "Model"));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_by_prefix() {
        let models: Vec<String> = [
            "ollama:llama3",
            "anthropic:claude-x",
            "openai/gpt-5",
            "anthropic/claude-y",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let g = group_models(&models);
        assert!(g.subscriptions.is_empty());
        assert_eq!(g.local, vec!["ollama:llama3"]);
        assert_eq!(g.anthropic_direct, vec!["anthropic:claude-x"]);
        assert_eq!(g.openrouter, vec!["openai/gpt-5", "anthropic/claude-y"]);
    }

    #[test]
    fn display_drops_group_prefixes_only() {
        assert_eq!(display_name("ollama:llama3"), "llama3");
        assert_eq!(display_name("anthropic:claude-x"), "claude-x");
        assert_eq!(display_name("anthropic/claude-y"), "anthropic/claude-y");
    }

    #[test]
    fn cli_models_group_together_under_their_cli_name() {
        let models: Vec<String> = ["openai/gpt-5", "codex:default", "claude-code:sonnet"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let g = group_models(&models);
        assert_eq!(g.subscriptions, vec!["codex:default", "claude-code:sonnet"]);
        assert_eq!(g.openrouter, vec!["openai/gpt-5"]);
        assert_eq!(display_name("claude-code:sonnet"), "Claude Code · sonnet");
        assert_eq!(display_name("codex:default"), "Codex · default model");
        assert_eq!(display_name("codex:gpt-5.5"), "Codex · gpt-5.5");
    }
}
