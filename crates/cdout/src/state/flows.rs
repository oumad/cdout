//! Everything outside the agent loop proper: startup, the spotlight, file
//! manager context, models, sessions and settings.

use std::time::Instant;

use cdout_core::api::{MaskedApiKeys, ProviderStatus};
use cdout_core::features::explorer::ExplorerState;
use cdout_core::features::sessions::Session;

use super::{
    AppState, Effect, ExplorerPurpose, HideReason, SessionPurpose, StartupData, MODEL_POLL,
    NO_MODEL_SPOTLIGHT_ERROR, RESTARTED_NOTICE,
};

/// The model to switch to when the selected one is gone: the user's own
/// subscriptions, then cloud families they most likely meant, then local.
pub fn pick_replacement(models: &[String]) -> Option<String> {
    let find = |prefix: &str| models.iter().find(|m| m.starts_with(prefix)).cloned();
    // A subscription the user already pays for comes first.
    find("claude-code:")
        .or_else(|| find("codex:"))
        .or_else(|| find("anthropic/"))
        .or_else(|| find("anthropic:"))
        .or_else(|| find("openai/"))
        .or_else(|| find("google/"))
        .or_else(|| find("ollama:"))
        .or_else(|| models.first().cloned())
}

impl AppState {
    // --- startup ---

    pub(super) fn on_started(&mut self, now: Instant, fx: &mut Vec<Effect>) {
        // Parity with Tauri: the main window is visible at launch; the
        // spotlight waits for the hotkey.
        self.main.visible = true;
        self.main.focus_input = true;
        fx.push(Effect::ShowMain {
            center_on_cursor: false,
        });
        fx.push(Effect::LoadStartup);
        fx.push(Effect::LoadSelectedModel);
        self.fetch_models(fx);
        fx.push(Effect::FetchProviderStatus);
        fx.push(Effect::ListSessions);
        self.schedule_poll(now);
    }

    pub(super) fn on_startup_loaded(
        &mut self,
        data: StartupData,
        now: Instant,
        fx: &mut Vec<Effect>,
    ) {
        let s = &mut self.settings;
        s.ollama_url = data.ollama_url.clone();
        s.draft_ollama_url = data.ollama_url.clone();
        s.onboarding_url_draft = data.ollama_url;
        s.approval_mode = data.approval_mode;
        s.hotkey = data.hotkey;
        s.keys = data.keys;
        s.show_free = data.show_free;
        if data.skills.is_some() {
            s.skills = data.skills;
        }
        self.main.migration_banner = data.show_migration_banner;

        if !data.draft.is_empty() && self.spotlight.prompt.is_empty() {
            self.spotlight.prompt = data.draft.clone();
        }
        self.spotlight.saved_draft = data.draft;
        self.spotlight.draft_due = None;
        if data.restarted_after_crash {
            self.show_toast(RESTARTED_NOTICE, now);
            if !self.spotlight.prompt.is_empty() {
                // They were typing when it died: put the palette back.
                self.show_spotlight(fx);
            }
        }
    }

    // --- spotlight ---

    pub(super) fn on_hotkey(&mut self, fx: &mut Vec<Effect>) {
        tracing::info!(target: "cdout", "hotkey.pressed");
        self.show_spotlight(fx);
    }

    pub(super) fn show_spotlight(&mut self, fx: &mut Vec<Effect>) {
        self.spotlight.visible = true;
        self.spotlight.focus_input = true;
        self.spotlight.focused_once = false;
        self.spotlight.shown_at = None;
        fx.push(Effect::ShowSpotlight);
    }

    pub(super) fn on_spotlight_focus(&mut self, focused: bool, now: Instant, fx: &mut Vec<Effect>) {
        if focused {
            tracing::info!(target: "cdout", "spotlight.focused");
            self.spotlight.focused_once = true;
            self.spotlight.shown_at = Some(now);
            self.spotlight.focus_input = true;
            fx.push(Effect::FetchExplorer(ExplorerPurpose::Spotlight));
        } else if self.spotlight.visible && self.spotlight.focused_once {
            let grace_over = self
                .spotlight
                .shown_at
                .is_none_or(|t| now.duration_since(t) > super::BLUR_GRACE);
            if grace_over {
                self.hide_spotlight(HideReason::Blur, fx);
            }
        }
    }

    pub(super) fn hide_spotlight(&mut self, reason: HideReason, fx: &mut Vec<Effect>) {
        if !self.spotlight.visible {
            return;
        }
        self.spotlight.visible = false;
        fx.push(Effect::HideSpotlight(reason));
    }

    pub(super) fn on_spotlight_submit(
        &mut self,
        auto_approve: bool,
        now: Instant,
        fx: &mut Vec<Effect>,
    ) {
        if self.spotlight.prompt.trim().is_empty() {
            return;
        }
        if self.models.selected.is_empty() {
            self.show_spotlight_error(NO_MODEL_SPOTLIGHT_ERROR, now);
            return;
        }
        let prompt = std::mem::take(&mut self.spotlight.prompt);
        let model = self.models.selected.clone();
        tracing::info!(target: "cdout", "spotlight.submitted auto={auto_approve}");
        self.hide_spotlight(HideReason::Submit, fx);
        self.main.visible = true;
        fx.push(Effect::ShowMain {
            center_on_cursor: true,
        });
        // The draft served its purpose; don't restore it next launch.
        self.spotlight.saved_draft.clear();
        self.spotlight.draft_due = None;
        fx.push(Effect::SaveDraft(String::new()));

        // The main window's `SPOTLIGHT_SUBMITTED` handler.
        self.main.settings_open = false;
        self.flush_save(fx);
        self.hard_reset(fx);
        self.chat.history.clear();
        self.sessions.current = None;
        self.select_model(model.clone(), fx);
        self.chat.processing = true;
        fx.push(Effect::FetchExplorer(ExplorerPurpose::SpotlightTask {
            gen: self.chat.generation,
            prompt,
            model,
            auto_approve,
        }));
    }

    // --- file-manager context ---

    pub(super) fn on_explorer(
        &mut self,
        purpose: ExplorerPurpose,
        result: Result<ExplorerState, String>,
        now: Instant,
        fx: &mut Vec<Effect>,
    ) {
        match purpose {
            ExplorerPurpose::Spotlight => {
                // A failed query reads as "no window detected" in the UI.
                self.spotlight.explorer = result.ok();
            }
            ExplorerPurpose::MainSync => {
                self.explorer_loading = false;
                match result {
                    Ok(state) => {
                        if !self.chat.history.is_empty() {
                            let synced: &[String] = self
                                .chat
                                .last_synced
                                .as_ref()
                                .map(|(_, f)| f.as_slice())
                                .unwrap_or(&[]);
                            if state.selected_files != synced {
                                self.chat.sync_pending = true;
                            }
                        }
                        self.explorer = Some(state);
                    }
                    Err(e) => {
                        self.explorer = None;
                        self.show_toast(e, now);
                    }
                }
            }
            ExplorerPurpose::MainStart { gen, prompt } => {
                if gen != self.chat.generation {
                    return;
                }
                self.chat.processing = false;
                match result {
                    Ok(state) => {
                        self.explorer = Some(state);
                        self.start_agent(prompt, false, now, fx);
                    }
                    Err(e) => {
                        self.chat.input = prompt;
                        self.show_toast(e, now);
                    }
                }
            }
            ExplorerPurpose::SpotlightTask {
                gen,
                prompt,
                model,
                auto_approve,
            } => {
                if gen != self.chat.generation {
                    return;
                }
                match result {
                    Ok(state) => {
                        self.chat.last_synced =
                            Some((state.path.clone(), state.selected_files.clone()));
                        self.chat.sync_pending = false;
                        self.explorer = Some(state.clone());
                        fx.push(Effect::CreateSession {
                            purpose: SessionPurpose::SpotlightTask {
                                gen,
                                auto_approve,
                                model: model.clone(),
                            },
                            prompt,
                            path: state.path,
                            files: state.selected_files,
                            model,
                        });
                    }
                    Err(e) => {
                        self.chat.processing = false;
                        self.show_toast(e, now);
                    }
                }
            }
        }
    }

    // --- models ---

    /// New provider status. When it shows a sign-in started from cdout has
    /// finished, switch to that CLI's model: the user just asked for it.
    pub(super) fn on_provider_status(
        &mut self,
        status: ProviderStatus,
        now: Instant,
        fx: &mut Vec<Effect>,
    ) {
        if let Some((backend, _)) = self.sign_in {
            let done = status
                .subscriptions
                .iter()
                .any(|s| s.backend == backend && s.signed_in);
            if done {
                self.sign_in = None;
                if let Some(model) = backend.models().first() {
                    self.select_model(model.to_string(), fx);
                }
                self.models.inflight = false;
                self.fetch_models(fx);
                self.show_toast(
                    format!("Signed in to {}. cdout will use it now.", backend.name()),
                    now,
                );
            }
        }
        self.provider_status = Some(status);
    }

    pub(super) fn fetch_models(&mut self, fx: &mut Vec<Effect>) {
        if self.models.inflight {
            return;
        }
        self.models.inflight = true;
        fx.push(Effect::FetchModels);
    }

    pub(super) fn should_poll_models(&self) -> bool {
        self.models.connected == Some(false)
            || (self.models.connected == Some(true) && self.models.available.is_empty())
    }

    pub(super) fn schedule_poll(&mut self, now: Instant) {
        let poll = self.should_poll_models()
            || self.models.connected.is_none()
            || self.models.available.is_empty()
            || self.sign_in.is_some();
        self.models.next_poll = poll.then(|| now + MODEL_POLL);
    }

    pub(super) fn select_model(&mut self, model: String, fx: &mut Vec<Effect>) {
        if model.is_empty() {
            return;
        }
        if self.models.selected != model {
            self.models.selected = model.clone();
        }
        fx.push(Effect::PersistModel(model));
    }

    pub(super) fn on_models(
        &mut self,
        result: Result<Vec<String>, String>,
        now: Instant,
        fx: &mut Vec<Effect>,
    ) {
        self.models.inflight = false;
        self.models.refreshing = false;
        match result {
            Ok(models) => {
                self.models.available = models;
                self.models.connected = Some(true);
                if !self.models.available.is_empty() {
                    let current = self.models.selected.clone();
                    if current.is_empty() {
                        if let Some(next) = pick_replacement(&self.models.available) {
                            self.models.selected = next.clone();
                            if self.models.hydrated {
                                fx.push(Effect::PersistModel(next));
                            }
                        }
                    } else if !self.models.available.contains(&current) {
                        if let Some(next) = pick_replacement(&self.models.available) {
                            self.models.selected = next.clone();
                            self.show_toast(
                                format!("Model '{current}' is no longer available — switched to '{next}'. Open Settings if you want to pick a different one."),
                                now,
                            );
                            fx.push(Effect::PersistModel(next));
                        }
                    }
                }
            }
            Err(_) => {
                self.models.connected = Some(false);
                self.models.available.clear();
            }
        }
        self.schedule_poll(now);
    }

    pub(super) fn on_selected_model(&mut self, saved: Option<String>, fx: &mut Vec<Effect>) {
        self.models.hydrated = true;
        match saved.filter(|s| !s.is_empty()) {
            Some(saved) => self.models.selected = saved,
            None => {
                if !self.models.selected.is_empty() {
                    fx.push(Effect::PersistModel(self.models.selected.clone()));
                }
            }
        }
    }

    // --- sessions ---

    pub(super) fn on_session_created(
        &mut self,
        purpose: SessionPurpose,
        result: Result<Session, String>,
        now: Instant,
        fx: &mut Vec<Effect>,
    ) {
        let gen = match &purpose {
            SessionPurpose::MainStart { gen } | SessionPurpose::SpotlightTask { gen, .. } => *gen,
        };
        if gen != self.chat.generation {
            return;
        }
        let session = match result {
            Ok(s) => s,
            Err(e) => {
                self.chat.processing = false;
                self.show_toast(e, now);
                return;
            }
        };
        self.sessions.current = Some(session.meta.id.clone());
        self.chat.history = session.messages;
        fx.push(Effect::ListSessions);
        match purpose {
            SessionPurpose::MainStart { .. } => {
                let auto = self.chat.auto_execute;
                self.run_step(auto, None, fx);
            }
            SessionPurpose::SpotlightTask {
                auto_approve,
                model,
                ..
            } => {
                if auto_approve {
                    self.chat.auto_execute = true;
                }
                let auto = auto_approve || self.chat.auto_execute;
                self.run_step(auto, Some(model), fx);
            }
        }
    }

    pub(super) fn new_chat(&mut self, fx: &mut Vec<Effect>) {
        self.flush_save(fx);
        self.hard_reset(fx);
        self.chat.history.clear();
        self.sessions.current = None;
        self.chat.last_synced = None;
        self.chat.sync_pending = false;
        self.main.focus_input = true;
        fx.push(Effect::ListSessions);
    }

    pub(super) fn select_session(&mut self, id: String, fx: &mut Vec<Effect>) {
        if self.sessions.current.as_deref() == Some(id.as_str()) {
            return;
        }
        self.flush_save(fx);
        self.hard_reset(fx);
        fx.push(Effect::LoadSession(id));
    }

    pub(super) fn on_session_loaded(
        &mut self,
        result: Result<Session, String>,
        now: Instant,
        fx: &mut Vec<Effect>,
    ) {
        match result {
            Ok(session) => {
                self.sessions.current = Some(session.meta.id.clone());
                // Not marked dirty: merely viewing a session must not bump
                // its last-active time (React re-saved it here).
                self.chat.history = session.messages;
                self.select_model(session.meta.model.clone(), fx);
                let path = session.meta.explorer_path;
                self.explorer = Some(ExplorerState {
                    path: path.clone(),
                    selected_files: Vec::new(),
                });
                self.chat.last_synced = Some((path, Vec::new()));
                self.chat.sync_pending = false;
            }
            Err(e) => self.show_toast(format!("Failed to load session: {e}"), now),
        }
    }

    pub(super) fn on_session_deleted(
        &mut self,
        id: String,
        result: Result<(), String>,
        now: Instant,
        fx: &mut Vec<Effect>,
    ) {
        match result {
            Ok(()) => {
                if self.sessions.current.as_deref() == Some(id.as_str()) {
                    self.sessions.current = None;
                    self.sessions.save_due = None;
                    self.chat.history.clear();
                    self.explorer = None;
                    self.hard_reset(fx);
                }
                fx.push(Effect::ListSessions);
            }
            Err(e) => self.show_toast(format!("Failed to delete session: {e}"), now),
        }
    }

    // --- settings ---

    pub(super) fn open_settings(&mut self, fx: &mut Vec<Effect>) {
        self.settings.draft_ollama_url = self.settings.ollama_url.clone();
        self.settings.hotkey_error = None;
        self.main.settings_open = true;
        self.fetch_models(fx);
        fx.push(Effect::FetchProviderStatus);
        fx.push(Effect::LoadSkills);
    }

    pub(super) fn save_settings(&mut self, fx: &mut Vec<Effect>) {
        let s = &self.settings;
        let non_empty = |v: &str| Some(v.trim().to_string()).filter(|v| !v.is_empty());
        fx.push(Effect::SaveSettings {
            ollama_url: s.draft_ollama_url.clone(),
            openrouter_key: non_empty(&s.openrouter_draft),
            anthropic_key: non_empty(&s.anthropic_draft),
        });
    }

    pub(super) fn on_settings_saved(
        &mut self,
        result: Result<MaskedApiKeys, String>,
        now: Instant,
        fx: &mut Vec<Effect>,
    ) {
        match result {
            Ok(keys) => {
                self.settings.ollama_url = self.settings.draft_ollama_url.clone();
                self.models.connected = None;
                self.settings.openrouter_draft.clear();
                self.settings.anthropic_draft.clear();
                self.settings.keys = keys;
                self.main.settings_open = false;
                self.fetch_models(fx);
            }
            Err(e) => self.show_toast(format!("Failed to save settings: {e}"), now),
        }
    }

    pub(super) fn dismiss_banner(&mut self, fx: &mut Vec<Effect>) {
        self.main.migration_banner = false;
        fx.push(Effect::SetDisclosureAck);
    }
}
