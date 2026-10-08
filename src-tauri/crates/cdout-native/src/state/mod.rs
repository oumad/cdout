//! The whole application state and the pure state machine that drives it.
//!
//! Nothing in this module touches egui, the network, the filesystem or a
//! window. User intents and async results both arrive as [`Msg`]s;
//! [`AppState::update`] mutates the state and returns [`Effect`]s, which the
//! dispatcher executes (spawning I/O, moving windows) and whose results come
//! back as more `Msg`s. Time is passed in, never read, so every timer-driven
//! rule (blur grace, save debounce, auto-step delay, toast expiry, polling)
//! is deterministic under test.
//!
//! The agent loop is a port of `MainApp` in the React frontend (`src/App.tsx`);
//! comments there explain the *why* of each rule and are not repeated here.

mod agent;
mod flows;
pub mod text;

#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::time::{Duration, Instant};

use cdout_core::api::{ListSkillsResult, MaskedApiKeys, PlatformInfo, ProviderStatus};
use cdout_core::features::agent::{AgentStepResult, QuestionData};
use cdout_core::features::explorer::ExplorerState;
use cdout_core::features::sessions::{Session, SessionMeta};
use cdout_core::llm::Message;
use cdout_core::tools::risk::CommandRisk;
use cdout_core::tools::shell::RunningCommand;
use cdout_core::utils::config::ApprovalMode;

// --- Tunables (mirrors `src/constants.ts`) ---

pub const BLUR_GRACE: Duration = Duration::from_millis(300);
pub const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);
pub const AUTO_STEP_DELAY: Duration = Duration::from_millis(500);
pub const TOAST_DURATION: Duration = Duration::from_millis(8000);
pub const MODEL_POLL: Duration = Duration::from_millis(5000);
pub const MAX_AUTO_STEPS: u32 = 10;
pub const MAX_CONSECUTIVE_FAILURES: u32 = 3;
pub const FILE_LIST_THRESHOLD: usize = 20;
pub const TOOL_OUTPUT_COLLAPSE_CHARS: usize = 200;
pub const MESSAGE_COLLAPSE_CHARS: usize = 500;

pub const NO_MODEL_SPOTLIGHT_ERROR: &str = "No model set up yet. Open cdout (from the tray) and pick a provider — local Ollama or a cloud key — to get started.";

// --- State ---

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub expires_at: Instant,
}

#[derive(Debug, Default)]
pub struct SpotlightState {
    pub visible: bool,
    pub prompt: String,
    /// When the window last gained focus; blur within [`BLUR_GRACE`] of this
    /// is ignored (the OS can bounce focus while a window is being shown).
    pub shown_at: Option<Instant>,
    pub explorer: Option<ExplorerState>,
    /// Ask the UI to focus the prompt on its next frame.
    pub focus_input: bool,
    pub error: Option<Toast>,
}

#[derive(Debug, Default)]
pub struct MainWindowState {
    pub visible: bool,
    pub settings_open: bool,
    pub sidebar_collapsed: bool,
    pub context_open: bool,
    pub focus_input: bool,
    pub migration_banner: bool,
    /// The banner's "also delete other tools' credentials?" choice is open.
    pub cleanup_confirm: bool,
}

#[derive(Debug, Default)]
pub struct ModelsState {
    pub selected: String,
    pub available: Vec<String>,
    /// `None` until the first fetch resolves.
    pub connected: Option<bool>,
    /// Whether the persisted selection has been read from disk yet.
    pub hydrated: bool,
    pub inflight: bool,
    pub refreshing: bool,
    pub next_poll: Option<Instant>,
}

#[derive(Debug, Default)]
pub struct SessionsState {
    pub list: Vec<SessionMeta>,
    pub current: Option<String>,
    pub save_due: Option<Instant>,
    /// Inline rename in progress: (session id, draft title).
    pub renaming: Option<(String, String)>,
    pub confirm_delete: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCommand {
    pub command: String,
    pub tool_call_id: Option<String>,
    pub risk: Option<CommandRisk>,
    /// Set when the loop detector paused this proposal.
    pub loop_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingQuestion {
    pub data: QuestionData,
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KillDialog {
    pub pid: u32,
    pub command: String,
}

/// Context captured when an agent step is started, consumed when it finishes.
#[derive(Debug, Clone)]
pub struct StepCtx {
    pub should_auto: bool,
    pub model: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecKind {
    /// Ran unattended by policy; failures count toward the auto-pause.
    Auto,
    /// Ran because the user clicked Run / Run all.
    Approved,
}

#[derive(Debug, Clone)]
pub struct ExecCtx {
    pub kind: ExecKind,
    pub should_auto: bool,
    pub model: String,
    pub command: String,
    pub tool_call_id: Option<String>,
    pub risk: Option<CommandRisk>,
}

#[derive(Debug, Default)]
pub struct ChatState {
    pub history: Vec<Message>,
    pub streaming: String,
    pub processing: bool,
    pub executing: bool,
    pub pending_command: Option<PendingCommand>,
    pub pending_question: Option<PendingQuestion>,
    /// This run's opt-in to unattended execution ("Run all" or a modifier
    /// submit). Distinct from the persisted approval mode.
    pub auto_execute: bool,
    pub auto_step_count: u32,
    pub consecutive_failures: u32,
    pub drop_tools_next: bool,
    /// Bumped whenever in-flight work must be disowned (stop, new chat,
    /// session switch, spotlight submit). Results tagged with an older
    /// generation are ignored.
    pub generation: u64,
    pub step: Option<StepCtx>,
    pub exec: Option<ExecCtx>,
    /// A delayed auto-continuation is scheduled; the loop is not idle.
    pub auto_step_at: Option<Instant>,
    pub queued: VecDeque<String>,
    pub kill_dialog: Option<KillDialog>,
    pub expanded: HashSet<usize>,
    pub input: String,
    /// Approval card: the "Reject with feedback" box is open.
    pub reject_open: bool,
    pub reject_feedback: String,
    /// Question card: picked option indices, the "Other" toggle and text.
    pub question_picks: BTreeSet<usize>,
    pub question_custom_open: bool,
    pub question_answer: String,
    pub last_synced: Option<(String, Vec<String>)>,
    pub sync_pending: bool,
}

impl ChatState {
    /// Reset the approval and question cards' local UI state.
    pub fn clear_card_ui(&mut self) {
        self.reject_open = false;
        self.reject_feedback.clear();
        self.question_picks.clear();
        self.question_custom_open = false;
        self.question_answer.clear();
    }

    pub fn is_idle(&self) -> bool {
        !self.processing
            && !self.executing
            && self.streaming.is_empty()
            && self.pending_command.is_none()
            && self.pending_question.is_none()
            && self.auto_step_at.is_none()
    }

    /// The input shows Stop instead of Send while this is true.
    pub fn is_busy(&self) -> bool {
        (self.processing || self.executing || !self.streaming.is_empty())
            && self.pending_command.is_none()
            && self.pending_question.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    General,
    Providers,
    Approval,
    Skills,
    Diagnostics,
}

#[derive(Debug, Default)]
pub struct SettingsState {
    pub tab: SettingsTab,
    pub ollama_url: String,
    pub draft_ollama_url: String,
    pub openrouter_draft: String,
    pub anthropic_draft: String,
    pub keys: MaskedApiKeys,
    pub show_free: bool,
    pub approval_mode: ApprovalMode,
    pub hotkey: String,
    pub hotkey_error: Option<String>,
    pub recording_hotkey: bool,
    pub skills: Option<ListSkillsResult>,
    /// File-access diagnostics: the raw debug payload, or the error.
    pub explorer_debug: Option<Result<serde_json::Value, String>>,
    pub explorer_debug_loading: bool,
    pub onboarding_key_draft: String,
    pub onboarding_url_draft: String,
}

#[derive(Debug)]
pub struct AppState {
    pub platform: PlatformInfo,
    pub spotlight: SpotlightState,
    pub main: MainWindowState,
    pub models: ModelsState,
    pub sessions: SessionsState,
    pub chat: ChatState,
    pub settings: SettingsState,
    /// File-manager context for the main window's conversation.
    pub explorer: Option<ExplorerState>,
    pub explorer_loading: bool,
    pub provider_status: Option<ProviderStatus>,
    pub toast: Option<Toast>,
    pub quitting: bool,
}

// --- Messages ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HideReason {
    Escape,
    Blur,
    Submit,
}

impl HideReason {
    pub fn as_str(self) -> &'static str {
        match self {
            HideReason::Escape => "escape",
            HideReason::Blur => "blur",
            HideReason::Submit => "submit",
        }
    }
}

/// Why a file-manager query was made, so its result lands in the right place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplorerPurpose {
    /// The spotlight's own context line.
    Spotlight,
    /// The main window's Sync button.
    MainSync,
    /// A main-window prompt that needs context before it can start.
    MainStart { gen: u64, prompt: String },
    /// A spotlight submission being turned into a fresh session.
    SpotlightTask {
        gen: u64,
        prompt: String,
        model: String,
        auto_approve: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionPurpose {
    MainStart {
        gen: u64,
    },
    SpotlightTask {
        gen: u64,
        auto_approve: bool,
        model: String,
    },
}

/// Everything loaded once at startup (and partially re-read by Settings).
#[derive(Debug, Clone, Default)]
pub struct StartupData {
    pub ollama_url: String,
    pub approval_mode: ApprovalMode,
    pub hotkey: String,
    pub keys: MaskedApiKeys,
    pub show_free: bool,
    pub show_migration_banner: bool,
    pub skills: Option<ListSkillsResult>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    // Lifecycle
    Started,
    Tick,

    // Spotlight
    HotkeyPressed,
    SpotlightFocusChanged(bool),
    SpotlightHide(HideReason),
    SpotlightSubmit {
        auto_approve: bool,
    },
    SelectModel(String),
    RefreshModels,

    // Main window: chat
    MainSubmit,
    RemoveQueued(usize),
    Stop,
    ApproveCommand {
        continue_auto: bool,
    },
    RejectCommand,
    DismissCommand,
    AnswerQuestion(String),
    DismissQuestion,
    ConfirmKill,
    KeepRunning,
    ContinueAgent,
    DisableAutoExecute,
    ToggleExpand(usize),
    SyncExplorer,
    ToggleContext,

    // Main window: sessions
    NewChat,
    SelectSession(String),
    DeleteSession(String),
    RenameSession {
        id: String,
        title: String,
    },
    ToggleSidebar,

    // Main window: settings and onboarding
    OpenSettings,
    CloseSettings,
    SaveSettings,
    SetSettingsTab(SettingsTab),
    SetApprovalMode(ApprovalMode),
    ToggleShowFree,
    ChangeHotkey(String),
    DismissBanner,
    OpenSettingsFromBanner,
    CleanupLegacy {
        third_party: bool,
    },
    OnboardingSaveKey(String),
    OnboardingSaveOllamaUrl(String),
    OnboardingRecheck,
    OpenFileAccessSettings,
    LoadExplorerDebug,
    OpenUrl(String),

    // Main window: chrome
    CloseMain,
    MinimizeMain,
    ToggleMaximizeMain,
    DismissToast,
    DismissSpotlightError,

    // Tray
    TrayShowMain,
    TrayQuit,

    // Async results
    StartupLoaded(StartupData),
    ExplorerResult {
        purpose: ExplorerPurpose,
        result: Result<ExplorerState, String>,
    },
    ModelsLoaded(Result<Vec<String>, String>),
    SelectedModelLoaded(Option<String>),
    ProviderStatusLoaded(Result<ProviderStatus, String>),
    SessionsListed(Result<Vec<SessionMeta>, String>),
    SessionCreated {
        purpose: SessionPurpose,
        result: Result<Session, String>,
    },
    SessionLoaded(Result<Session, String>),
    SessionSaved(Result<SessionMeta, String>),
    SessionDeleted {
        id: String,
        result: Result<(), String>,
    },
    SessionRenamed(Result<SessionMeta, String>),
    StreamDelta {
        gen: u64,
        text: String,
    },
    StepDone {
        gen: u64,
        result: Result<AgentStepResult, String>,
    },
    ExecDone {
        gen: u64,
        result: Result<String, String>,
    },
    RunningCommandLoaded(Option<RunningCommand>),
    KillResult(Result<(), String>),
    SettingsSaved(Result<MaskedApiKeys, String>),
    ShowFreeSaved(Result<bool, String>),
    ApprovalModeSaved(Result<(), String>),
    HotkeyApplied {
        chord: String,
        result: Result<(), String>,
    },
    LegacyCleaned(Result<(), String>),
    OnboardingKeySaved(Result<MaskedApiKeys, String>),
    OnboardingUrlSaved {
        url: String,
        result: Result<(), String>,
    },
    SkillsLoaded(ListSkillsResult),
    ExplorerDebugLoaded(Result<serde_json::Value, String>),
    EffectFailed(String),
}

// --- Effects ---

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    // Windows and process (main thread)
    ShowSpotlight,
    HideSpotlight(HideReason),
    ShowMain {
        center_on_cursor: bool,
    },
    HideMain,
    MinimizeMain,
    ToggleMaximizeMain,
    Quit,
    ApplyHotkey {
        chord: String,
        persist: bool,
    },

    // Immediate, synchronous backend calls (must run before later effects)
    CancelStream,
    ResetLoopDetector,

    // Background work
    LoadStartup,
    FetchExplorer(ExplorerPurpose),
    FetchModels,
    LoadSelectedModel,
    PersistModel(String),
    FetchProviderStatus,
    ListSessions,
    CreateSession {
        purpose: SessionPurpose,
        prompt: String,
        path: String,
        files: Vec<String>,
        model: String,
    },
    LoadSession(String),
    SaveSession {
        id: String,
        messages: Vec<Message>,
    },
    DeleteSession(String),
    RenameSession {
        id: String,
        title: String,
    },
    RunStep {
        gen: u64,
        model: String,
        history: Vec<Message>,
        drop_tools: bool,
    },
    Execute {
        gen: u64,
        command: String,
        cwd: Option<String>,
    },
    QueryRunningCommand,
    KillRunningCommand,
    PersistApprovalMode(ApprovalMode),
    SaveSettings {
        ollama_url: String,
        openrouter_key: Option<String>,
        anthropic_key: Option<String>,
    },
    SetShowFree(bool),
    SetDisclosureAck,
    CleanupLegacy {
        third_party: bool,
    },
    LoadSkills,
    SaveOpenrouterKey(String),
    SaveOllamaUrl(String),
    OpenUrl(String),
    OpenFileAccessSettings,
    LoadExplorerDebug,
}

// --- Construction and the update entry point ---

impl AppState {
    pub fn new(platform: PlatformInfo) -> Self {
        Self {
            platform,
            spotlight: SpotlightState::default(),
            main: MainWindowState::default(),
            models: ModelsState::default(),
            sessions: SessionsState::default(),
            chat: ChatState::default(),
            settings: SettingsState::default(),
            explorer: None,
            explorer_loading: false,
            provider_status: None,
            toast: None,
            quitting: false,
        }
    }

    /// Apply one message. Returns the effects to execute, in order.
    pub fn update(&mut self, msg: Msg, now: Instant) -> Vec<Effect> {
        let mut fx = Vec::new();
        self.handle(msg, now, &mut fx);
        self.after_update(now, &mut fx);
        fx
    }

    fn handle(&mut self, msg: Msg, now: Instant, fx: &mut Vec<Effect>) {
        match msg {
            Msg::Started => self.on_started(now, fx),
            Msg::Tick => self.on_tick(now, fx),

            Msg::HotkeyPressed => self.on_hotkey(fx),
            Msg::SpotlightFocusChanged(focused) => self.on_spotlight_focus(focused, now, fx),
            Msg::SpotlightHide(reason) => self.hide_spotlight(reason, fx),
            Msg::SpotlightSubmit { auto_approve } => {
                self.on_spotlight_submit(auto_approve, now, fx)
            }
            Msg::SelectModel(model) => self.select_model(model, fx),
            Msg::RefreshModels => {
                self.models.refreshing = true;
                self.fetch_models(fx);
            }

            Msg::MainSubmit => self.submit_or_queue(now, fx),
            Msg::RemoveQueued(i) => {
                if i < self.chat.queued.len() {
                    self.chat.queued.remove(i);
                }
            }
            Msg::Stop => self.stop_agent(now, fx),
            Msg::ApproveCommand { continue_auto } => self.approve_command(continue_auto, fx),
            Msg::RejectCommand => self.reject_command(now, fx),
            Msg::DismissCommand => self.dismiss_command(now),
            Msg::AnswerQuestion(answer) => self.answer_question(answer, now, fx),
            Msg::DismissQuestion => self.dismiss_question(now),
            Msg::ConfirmKill => {
                if self.chat.kill_dialog.take().is_some() {
                    fx.push(Effect::KillRunningCommand);
                }
            }
            Msg::KeepRunning => self.chat.kill_dialog = None,
            Msg::ContinueAgent => self.continue_agent(now, fx),
            Msg::DisableAutoExecute => self.chat.auto_execute = false,
            Msg::ToggleExpand(i) => {
                if !self.chat.expanded.remove(&i) {
                    self.chat.expanded.insert(i);
                }
            }
            Msg::SyncExplorer => {
                if !(self.explorer_loading || self.chat.processing || self.chat.executing) {
                    self.explorer_loading = true;
                    fx.push(Effect::FetchExplorer(ExplorerPurpose::MainSync));
                }
            }
            Msg::ToggleContext => self.main.context_open = !self.main.context_open,

            Msg::NewChat => self.new_chat(fx),
            Msg::SelectSession(id) => self.select_session(id, fx),
            Msg::DeleteSession(id) => {
                self.sessions.confirm_delete = None;
                fx.push(Effect::DeleteSession(id));
            }
            Msg::RenameSession { id, title } => {
                self.sessions.renaming = None;
                let title = title.trim().to_string();
                if !title.is_empty() {
                    fx.push(Effect::RenameSession { id, title });
                }
            }
            Msg::ToggleSidebar => self.main.sidebar_collapsed = !self.main.sidebar_collapsed,

            Msg::OpenSettings => self.open_settings(fx),
            Msg::CloseSettings => self.main.settings_open = false,
            Msg::SaveSettings => self.save_settings(fx),
            Msg::SetSettingsTab(tab) => {
                self.settings.tab = tab;
                if tab == SettingsTab::Diagnostics {
                    self.settings.explorer_debug_loading = true;
                    fx.push(Effect::LoadExplorerDebug);
                }
            }
            Msg::SetApprovalMode(mode) => {
                self.settings.approval_mode = mode;
                fx.push(Effect::PersistApprovalMode(mode));
            }
            Msg::ToggleShowFree => fx.push(Effect::SetShowFree(!self.settings.show_free)),
            Msg::ChangeHotkey(chord) => {
                self.settings.hotkey_error = None;
                fx.push(Effect::ApplyHotkey {
                    chord,
                    persist: true,
                });
            }
            Msg::DismissBanner => self.dismiss_banner(fx),
            Msg::OpenSettingsFromBanner => {
                self.dismiss_banner(fx);
                self.open_settings(fx);
            }
            Msg::CleanupLegacy { third_party } => {
                self.main.cleanup_confirm = false;
                fx.push(Effect::CleanupLegacy { third_party });
            }
            Msg::OnboardingSaveKey(key) => {
                if key.trim().is_empty() {
                    self.show_toast("API key cannot be empty", now);
                } else {
                    fx.push(Effect::SaveOpenrouterKey(key.trim().to_string()));
                }
            }
            Msg::OnboardingSaveOllamaUrl(url) => {
                fx.push(Effect::SaveOllamaUrl(url.trim().to_string()))
            }
            Msg::OnboardingRecheck => {
                self.fetch_models(fx);
                fx.push(Effect::FetchProviderStatus);
            }
            Msg::OpenFileAccessSettings => fx.push(Effect::OpenFileAccessSettings),
            Msg::LoadExplorerDebug => {
                self.settings.explorer_debug_loading = true;
                fx.push(Effect::LoadExplorerDebug);
            }
            Msg::OpenUrl(url) => fx.push(Effect::OpenUrl(url)),

            Msg::CloseMain => {
                self.main.visible = false;
                self.flush_save(fx);
                fx.push(Effect::HideMain);
            }
            Msg::MinimizeMain => fx.push(Effect::MinimizeMain),
            Msg::ToggleMaximizeMain => fx.push(Effect::ToggleMaximizeMain),
            Msg::DismissToast => self.toast = None,
            Msg::DismissSpotlightError => self.spotlight.error = None,

            Msg::TrayShowMain => {
                self.main.visible = true;
                fx.push(Effect::ShowMain {
                    center_on_cursor: false,
                });
            }
            Msg::TrayQuit => {
                self.flush_save(fx);
                self.quitting = true;
                fx.push(Effect::Quit);
            }

            Msg::StartupLoaded(data) => self.on_startup_loaded(data),
            Msg::ExplorerResult { purpose, result } => self.on_explorer(purpose, result, now, fx),
            Msg::ModelsLoaded(result) => self.on_models(result, now, fx),
            Msg::SelectedModelLoaded(saved) => self.on_selected_model(saved, fx),
            Msg::ProviderStatusLoaded(result) => {
                if let Ok(status) = result {
                    self.provider_status = Some(status);
                }
            }
            Msg::SessionsListed(result) => match result {
                Ok(list) => self.sessions.list = list,
                Err(e) => tracing::warn!(target: "cdout", "sessions.list.error {e}"),
            },
            Msg::SessionCreated { purpose, result } => {
                self.on_session_created(purpose, result, now, fx)
            }
            Msg::SessionLoaded(result) => self.on_session_loaded(result, now, fx),
            Msg::SessionSaved(result) => match result {
                Ok(_) => fx.push(Effect::ListSessions),
                Err(e) => tracing::warn!(target: "cdout", "sessions.save.error {e}"),
            },
            Msg::SessionDeleted { id, result } => self.on_session_deleted(id, result, now, fx),
            Msg::SessionRenamed(result) => match result {
                Ok(_) => fx.push(Effect::ListSessions),
                Err(e) => self.show_toast(format!("Failed to rename session: {e}"), now),
            },
            Msg::StreamDelta { gen, text } => {
                if gen == self.chat.generation && self.chat.step.is_some() {
                    self.chat.streaming.push_str(&text);
                }
            }
            Msg::StepDone { gen, result } => self.on_step_done(gen, result, now, fx),
            Msg::ExecDone { gen, result } => self.on_exec_done(gen, result, now, fx),
            Msg::RunningCommandLoaded(running) => {
                if let Some(r) = running {
                    self.chat.kill_dialog = Some(KillDialog {
                        pid: r.pid,
                        command: r.command_preview,
                    });
                }
            }
            Msg::KillResult(result) => {
                if let Err(e) = result {
                    self.show_toast(format!("Failed to kill process: {e}"), now);
                }
            }
            Msg::SettingsSaved(result) => self.on_settings_saved(result, now, fx),
            Msg::ShowFreeSaved(result) => match result {
                Ok(enabled) => {
                    self.settings.show_free = enabled;
                    self.fetch_models(fx);
                }
                Err(e) => self.show_toast(format!("Failed to toggle free-tier models: {e}"), now),
            },
            Msg::ApprovalModeSaved(result) => {
                if let Err(e) = result {
                    self.show_toast(e, now);
                }
            }
            Msg::HotkeyApplied { chord, result } => match result {
                Ok(()) => {
                    self.settings.hotkey = chord;
                    self.settings.hotkey_error = None;
                    self.settings.recording_hotkey = false;
                }
                Err(e) => self.settings.hotkey_error = Some(e),
            },
            Msg::LegacyCleaned(result) => match result {
                Ok(()) => self.dismiss_banner(fx),
                Err(e) => self.show_toast(e, now),
            },
            Msg::OnboardingKeySaved(result) => match result {
                Ok(keys) => {
                    self.settings.keys = keys;
                    self.settings.onboarding_key_draft.clear();
                    self.fetch_models(fx);
                    fx.push(Effect::FetchProviderStatus);
                }
                Err(e) => self.show_toast(e, now),
            },
            Msg::OnboardingUrlSaved { url, result } => match result {
                Ok(()) => {
                    self.settings.ollama_url = url.clone();
                    self.settings.draft_ollama_url = url;
                    self.models.connected = None;
                    self.fetch_models(fx);
                    fx.push(Effect::FetchProviderStatus);
                }
                Err(e) => self.show_toast(e, now),
            },
            Msg::SkillsLoaded(skills) => self.settings.skills = Some(skills),
            Msg::ExplorerDebugLoaded(result) => {
                self.settings.explorer_debug_loading = false;
                self.settings.explorer_debug = Some(result);
            }
            Msg::EffectFailed(e) => self.show_toast(e, now),
        }
    }

    /// Rules that run after every message: drain the prompt queue when the
    /// loop goes idle.
    fn after_update(&mut self, now: Instant, fx: &mut Vec<Effect>) {
        if self.chat.is_idle() {
            if let Some(next) = self.chat.queued.pop_front() {
                self.start_agent(next, false, now, fx);
            }
        }
    }

    pub fn show_toast(&mut self, message: impl Into<String>, now: Instant) {
        let message = message.into();
        tracing::info!(target: "cdout", "toast {message}");
        self.toast = Some(Toast {
            message,
            expires_at: now + TOAST_DURATION,
        });
    }

    fn show_spotlight_error(&mut self, message: impl Into<String>, now: Instant) {
        self.spotlight.error = Some(Toast {
            message: message.into(),
            expires_at: now + TOAST_DURATION,
        });
    }

    /// Earliest instant at which [`Msg::Tick`] has work to do, so the UI can
    /// schedule a repaint for it instead of polling.
    pub fn next_wakeup(&self) -> Option<Instant> {
        [
            self.sessions.save_due,
            self.chat.auto_step_at,
            self.toast.as_ref().map(|t| t.expires_at),
            self.spotlight.error.as_ref().map(|t| t.expires_at),
            self.models.next_poll,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn on_tick(&mut self, now: Instant, fx: &mut Vec<Effect>) {
        if self.toast.as_ref().is_some_and(|t| now >= t.expires_at) {
            self.toast = None;
        }
        if self
            .spotlight
            .error
            .as_ref()
            .is_some_and(|t| now >= t.expires_at)
        {
            self.spotlight.error = None;
        }
        if self.sessions.save_due.is_some_and(|due| now >= due) {
            self.flush_save(fx);
        }
        if self.chat.auto_step_at.is_some_and(|at| now >= at) {
            self.run_auto_continuation(now, fx);
        }
        if self.models.next_poll.is_some_and(|at| now >= at) {
            self.models.next_poll = None;
            if self.should_poll_models() {
                self.fetch_models(fx);
            }
            if self.models.available.is_empty() {
                fx.push(Effect::FetchProviderStatus);
            }
            self.schedule_poll(now);
        }
    }
}
