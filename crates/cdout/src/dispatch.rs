//! Executes [`Effect`]s. Background work runs on the tokio runtime (or a
//! single serial I/O thread for session files) and reports back by sending a
//! [`Msg`] and waking the UI. Effects that must run on the UI thread (window
//! commands, hotkey registration) are handed back to the caller.

use std::sync::mpsc::Sender;
use std::sync::Arc;

use cdout_core::api;
use cdout_core::features::agent::ChunkSink;
use cdout_core::features::{explorer, loop_detector, sessions, skills};
use cdout_core::llm::StreamChunk;
use cdout_core::utils::{cancel, config};
use eframe::egui;

use crate::state::{Effect, Msg, StartupData};

/// Sends messages to the UI thread and wakes every window so the message is
/// processed promptly.
#[derive(Clone)]
pub struct MsgSender {
    tx: Sender<Msg>,
    ctx: egui::Context,
}

impl MsgSender {
    pub fn new(tx: Sender<Msg>, ctx: egui::Context) -> Self {
        Self { tx, ctx }
    }

    pub fn send(&self, msg: Msg) {
        if self.tx.send(msg).is_ok() {
            crate::app::wake_all(&self.ctx);
        }
    }
}

/// One thread that runs session-file jobs in submission order, so a save
/// can never land after a later save or a delete of the same session.
#[derive(Clone)]
struct IoQueue {
    tx: Sender<Box<dyn FnOnce() + Send>>,
}

impl IoQueue {
    fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<Box<dyn FnOnce() + Send>>();
        std::thread::Builder::new()
            .name("cdout-session-io".into())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    job();
                }
            })
            .expect("spawn session I/O thread");
        Self { tx }
    }

    fn run(&self, job: impl FnOnce() + Send + 'static) {
        let _ = self.tx.send(Box::new(job));
    }

    /// Block until every job queued so far has run (bounded wait).
    fn drain(&self, timeout: std::time::Duration) -> bool {
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        self.run(move || {
            let _ = done_tx.send(());
        });
        done_rx.recv_timeout(timeout).is_ok()
    }
}

#[derive(Clone)]
pub struct Dispatcher {
    rt: tokio::runtime::Handle,
    out: MsgSender,
    io: IoQueue,
}

impl Dispatcher {
    pub fn new(rt: tokio::runtime::Handle, out: MsgSender) -> Self {
        Self {
            rt,
            out,
            io: IoQueue::new(),
        }
    }

    pub fn sender(&self) -> &MsgSender {
        &self.out
    }

    /// Run background effects; return the ones the UI thread must handle.
    pub fn execute(&self, effects: Vec<Effect>) -> Vec<Effect> {
        let mut ui_effects = Vec::new();
        for effect in effects {
            if is_ui_effect(&effect) {
                ui_effects.push(effect);
            } else {
                self.run(effect);
            }
        }
        ui_effects
    }

    /// Wait for queued session and config writes to finish. Called on quit,
    /// so the last save is not lost when the process exits.
    pub fn drain_io(&self) {
        if !self.io.drain(std::time::Duration::from_secs(3)) {
            tracing::warn!(target: "cdout", "io.drain timed out");
        }
    }

    fn blocking<F>(&self, f: F)
    where
        F: FnOnce() -> Msg + Send + 'static,
    {
        let out = self.out.clone();
        self.rt.spawn_blocking(move || out.send(f()));
    }

    fn run(&self, effect: Effect) {
        let out = self.out.clone();
        match effect {
            // Synchronous on purpose: these must take effect before any
            // effect after them (a new step resets the cancel flag).
            Effect::CancelStream => cancel::request(),
            Effect::ResetLoopDetector => loop_detector::reset_global(),

            Effect::LoadStartup => self.blocking(|| Msg::StartupLoaded(load_startup())),
            Effect::FetchExplorer(purpose) => self.blocking(move || Msg::ExplorerResult {
                purpose,
                result: explorer::get_active_explorer_info(),
            }),
            Effect::FetchModels => {
                self.rt.spawn(async move {
                    out.send(Msg::ModelsLoaded(api::available_models().await));
                });
            }
            Effect::LoadSelectedModel => {
                self.blocking(|| Msg::SelectedModelLoaded(config::get_selected_model()))
            }
            Effect::PersistModel(model) => {
                self.io.run(move || {
                    if let Err(e) = config::set_selected_model(model) {
                        tracing::warn!(target: "cdout", "config.model.error {e}");
                    }
                });
            }
            Effect::FetchProviderStatus => {
                self.rt.spawn(async move {
                    out.send(Msg::ProviderStatusLoaded(api::provider_status().await));
                });
            }
            Effect::ListSessions => self.io.run(move || {
                out.send(Msg::SessionsListed(sessions::list_sessions()));
            }),
            Effect::CreateSession {
                purpose,
                prompt,
                path,
                files,
                model,
            } => self.io.run(move || {
                let loaded = skills::load_all_skills_embedded();
                let result = api::create_session(&prompt, &path, &files, &model, &loaded);
                out.send(Msg::SessionCreated { purpose, result });
            }),
            Effect::LoadSession(id) => self.io.run(move || {
                out.send(Msg::SessionLoaded(sessions::load_session(&id)));
            }),
            Effect::SaveSession { id, messages } => self.io.run(move || {
                out.send(Msg::SessionSaved(api::save_session_messages(&id, messages)));
            }),
            Effect::DeleteSession(id) => self.io.run(move || {
                let result = sessions::delete_session(&id);
                out.send(Msg::SessionDeleted { id, result });
            }),
            Effect::RenameSession { id, title } => self.io.run(move || {
                out.send(Msg::SessionRenamed(sessions::rename_session(&id, &title)));
            }),
            Effect::RunStep {
                gen,
                model,
                history,
                drop_tools,
            } => {
                self.rt.spawn(async move {
                    let delta_out = out.clone();
                    let sink: Arc<dyn ChunkSink> = Arc::new(move |chunk: StreamChunk| {
                        if let StreamChunk::TextDelta { text } = chunk {
                            tracing::info!(target: "cdout", "agent.stream.delta gen={gen}");
                            delta_out.send(Msg::StreamDelta { gen, text });
                        }
                    });
                    let result = api::run_agent_step(model, history, drop_tools, sink).await;
                    out.send(Msg::StepDone { gen, result });
                });
            }
            Effect::Execute { gen, command, cwd } => self.blocking(move || Msg::ExecDone {
                gen,
                result: api::execute_shell_command(&command, cwd.as_deref()),
            }),
            Effect::QueryRunningCommand => {
                self.blocking(|| Msg::RunningCommandLoaded(cdout_core::tools::shell::get_running()))
            }
            Effect::KillRunningCommand => {
                self.blocking(|| Msg::KillResult(cdout_core::tools::shell::kill_running()))
            }
            Effect::PersistApprovalMode(mode) => self.io.run(move || {
                out.send(Msg::ApprovalModeSaved(config::set_approval_mode(mode)));
            }),
            Effect::SaveSettings {
                ollama_url,
                openrouter_key,
                anthropic_key,
            } => self.io.run(move || {
                let result = (|| {
                    api::set_ollama_url(&ollama_url)?;
                    if let Some(k) = openrouter_key {
                        api::set_openrouter_key(&k)?;
                    }
                    if let Some(k) = anthropic_key {
                        api::set_anthropic_key(&k)?;
                    }
                    Ok(api::masked_api_keys())
                })();
                out.send(Msg::SettingsSaved(result));
            }),
            Effect::SetShowFree(enabled) => self.io.run(move || {
                let result = config::set_show_free_openrouter_models(enabled).map(|()| enabled);
                out.send(Msg::ShowFreeSaved(result));
            }),
            Effect::SetDisclosureAck => self.io.run(move || {
                if let Err(e) = config::set_openrouter_disclosure_ack(true) {
                    tracing::warn!(target: "cdout", "config.ack.error {e}");
                }
            }),
            Effect::CleanupLegacy => {
                self.blocking(|| Msg::LegacyCleaned(api::cleanup_legacy_credentials()))
            }
            Effect::LoadSkills => self.blocking(|| {
                Msg::SkillsLoaded(api::list_skills(skills::load_all_skills_embedded()))
            }),
            Effect::SaveOpenrouterKey(key) => self.io.run(move || {
                let result = api::set_openrouter_key(&key).map(|()| api::masked_api_keys());
                out.send(Msg::OnboardingKeySaved(result));
            }),
            Effect::SaveOllamaUrl(url) => self.io.run(move || {
                let result = api::set_ollama_url(&url);
                out.send(Msg::OnboardingUrlSaved { url, result });
            }),
            Effect::OpenUrl(url) => {
                if let Err(e) = open::that_detached(&url) {
                    out.send(Msg::EffectFailed(format!("Could not open {url}: {e}")));
                }
            }
            Effect::OpenCliSignIn(backend) => {
                if let Err(e) = api::open_cli_sign_in(backend) {
                    out.send(Msg::EffectFailed(e));
                }
            }
            Effect::OpenFileAccessSettings => {
                if let Err(e) = api::open_file_access_settings() {
                    out.send(Msg::EffectFailed(e));
                }
            }
            Effect::SaveDraft(text) => self.io.run(move || save_draft(&text)),
            Effect::LoadExplorerDebug => self.blocking(|| {
                Msg::ExplorerDebugLoaded(
                    explorer::get_explorer_debug_info()
                        .and_then(|info| serde_json::to_value(&info).map_err(|e| e.to_string())),
                )
            }),

            ui @ (Effect::ShowSpotlight
            | Effect::HideSpotlight(_)
            | Effect::ShowMain { .. }
            | Effect::HideMain
            | Effect::MinimizeMain
            | Effect::ToggleMaximizeMain
            | Effect::Quit
            | Effect::ApplyHotkey { .. }) => {
                unreachable!("UI effect routed to the dispatcher: {ui:?}")
            }
        }
    }
}

fn is_ui_effect(e: &Effect) -> bool {
    matches!(
        e,
        Effect::ShowSpotlight
            | Effect::HideSpotlight(_)
            | Effect::ShowMain { .. }
            | Effect::HideMain
            | Effect::MinimizeMain
            | Effect::ToggleMaximizeMain
            | Effect::Quit
            | Effect::ApplyHotkey { .. }
    )
}

const DRAFT_FILE: &str = "spotlight_draft.txt";

fn draft_path() -> std::path::PathBuf {
    cdout_core::utils::paths::app_data_dir().join(DRAFT_FILE)
}

fn save_draft(text: &str) {
    let path = draft_path();
    let result = if text.is_empty() {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        std::fs::write(&path, text)
    };
    if let Err(e) = result {
        tracing::warn!(target: "cdout", "draft.save.error {e}");
    }
}

fn load_startup() -> StartupData {
    let has_legacy = api::has_legacy_credentials().unwrap_or(false);
    StartupData {
        draft: std::fs::read_to_string(draft_path()).unwrap_or_default(),
        restarted_after_crash: crate::crash::was_restarted(),
        ollama_url: config::get_ollama_url(),
        approval_mode: config::get_approval_mode(),
        hotkey: config::get_hotkey(),
        keys: api::masked_api_keys(),
        show_free: config::get_show_free_openrouter_models(),
        show_migration_banner: has_legacy && !config::get_openrouter_disclosure_ack(),
        skills: Some(api::list_skills(skills::load_all_skills_embedded())),
    }
}
