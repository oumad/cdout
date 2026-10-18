//! Headless tests of every screen with `egui_kittest`: find widgets through
//! the accessibility tree, click and type, then assert on the messages the
//! screen emitted and on pixel snapshots.
//!
//! Snapshots live in `tests/snapshots/`. Regenerate deliberately with
//! `UPDATE_SNAPSHOTS=true`; set `CDOUT_SKIP_SNAPSHOTS=1` where no GPU
//! adapter is available (they then return early).

use cdout::state::{
    AppState, HideReason, KillDialog, Msg, PendingCommand, PendingQuestion, SettingsTab, Toast,
};
use cdout::ui;
use cdout_core::api::{platform_info, ListSkillsResult, ProviderStatus};
use cdout_core::features::agent::{QuestionData, QuestionOption};
use cdout_core::features::explorer::ExplorerState;
use cdout_core::features::sessions::SessionMeta;
use cdout_core::features::skills::{Skill, SkillMetadata};
use cdout_core::llm::clients::cli::{Backend, CliStatus};
use cdout_core::llm::Message;
use cdout_core::tools::risk::CommandRisk;
use cdout_core::utils::config::ApprovalMode;
use eframe::egui::{self, Key, Modifiers};
use egui_commonmark::CommonMarkCache;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

/// Fixed clock for the sidebar's relative times.
const NOW_MS: i64 = 1_800_000_000_000;

struct T {
    s: AppState,
    out: Vec<Msg>,
    md: CommonMarkCache,
}

fn base_state() -> AppState {
    let mut s = AppState::new(platform_info(None));
    s.models.selected = "ollama:llama3.2".into();
    s.models.available = vec![
        "ollama:llama3.2".into(),
        "anthropic/claude-sonnet-4.6".into(),
    ];
    s.models.connected = Some(true);
    s
}

/// The wgpu-backed harnesses crash the process (access violation in the
/// driver) when many are created concurrently, so tests take turns.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn snapshots_enabled() -> bool {
    std::env::var("CDOUT_SKIP_SNAPSHOTS").map_or(true, |v| v != "1")
}

fn spotlight(s: AppState) -> Harness<'static, T> {
    Harness::builder()
        .with_size(egui::vec2(560.0, 230.0))
        .with_pixels_per_point(1.0)
        .build_ui_state(
            |ui, t: &mut T| {
                ui::theme::apply(ui.ctx());
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| ui::spotlight::show(ui, &mut t.s, &mut t.out));
            },
            T {
                s,
                out: Vec::new(),
                md: CommonMarkCache::default(),
            },
        )
}

fn main_window(s: AppState) -> Harness<'static, T> {
    Harness::builder()
        .with_size(egui::vec2(1100.0, 760.0))
        .with_pixels_per_point(1.0)
        .build_ui_state(
            |ui, t: &mut T| {
                ui::theme::apply(ui.ctx());
                let T { s, out, md } = t;
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(ui::theme::GRAY_950))
                    .show(ui, |ui| ui::main_window::show(ui, s, md, NOW_MS, out));
            },
            T {
                s,
                out: Vec::new(),
                md: CommonMarkCache::default(),
            },
        )
}

fn take(h: &mut Harness<'_, T>) -> Vec<Msg> {
    std::mem::take(&mut h.state_mut().out)
}

fn msg(role: &str, content: &str) -> Message {
    Message {
        role: role.into(),
        content: content.into(),
        tool_calls: None,
        synthetic: false,
    }
}

fn submit_modifier(s: &AppState) -> Modifiers {
    if s.platform.os == "macos" {
        Modifiers::MAC_CMD
    } else {
        Modifiers::CTRL
    }
}

// --- spotlight ---

#[test]
fn spotlight_enter_submits_what_was_typed() {
    let _serial = serial();
    let mut h = spotlight(base_state());
    h.run();
    h.get_by_label("Prompt").focus();
    h.run();
    h.get_by_label("Prompt").type_text("rename these");
    h.run();
    assert_eq!(h.state().s.spotlight.prompt, "rename these");
    h.key_press(Key::Enter);
    h.run();
    let out = take(&mut h);
    assert!(
        out.iter().any(|m| matches!(
            m,
            Msg::SpotlightSubmit {
                auto_approve: false
            }
        )),
        "{out:?}"
    );
}

#[test]
fn spotlight_modifier_enter_pre_authorises() {
    let _serial = serial();
    let s = base_state();
    let modifier = submit_modifier(&s);
    let mut h = spotlight(s);
    h.run();
    h.get_by_label("Prompt").focus();
    h.run();
    h.get_by_label("Prompt").type_text("compress");
    h.run();
    h.key_press_modifiers(modifier, Key::Enter);
    h.run();
    let out = take(&mut h);
    assert!(
        out.iter()
            .any(|m| matches!(m, Msg::SpotlightSubmit { auto_approve: true })),
        "{out:?}"
    );
}

#[test]
fn spotlight_escape_hides() {
    let _serial = serial();
    let mut h = spotlight(base_state());
    h.run();
    h.key_press(Key::Escape);
    h.run();
    let out = take(&mut h);
    assert!(out
        .iter()
        .any(|m| matches!(m, Msg::SpotlightHide(HideReason::Escape))));
}

#[test]
fn spotlight_shows_the_file_manager_context() {
    let _serial = serial();
    let mut s = base_state();
    s.spotlight.explorer = Some(ExplorerState {
        path: "C:/work".into(),
        selected_files: ["a.mov", "b.mov", "c.mov", "d.mov"]
            .iter()
            .map(|f| f.to_string())
            .collect(),
    });
    let mut h = spotlight(s);
    h.run_steps(3);
    h.get_by_label("🗁 C:/work");
    h.get_by_label("🗋 a.mov, b.mov +2 more");
    if snapshots_enabled() {
        h.snapshot("spotlight_with_context");
    }
}

#[test]
fn spotlight_long_paths_stay_inside_the_card() {
    let _serial = serial();
    let mut s = base_state();
    let dir = "C:/Users/someone/Videos/".to_string() + &"deeply/nested/".repeat(6);
    s.spotlight.explorer = Some(ExplorerState {
        path: dir.clone(),
        selected_files: (0..3)
            .map(|i| format!("{dir}Enregistrement 2026-10-18 08262{i} with a long name.mp4"))
            .collect(),
    });
    let mut h = spotlight(s);
    h.run_steps(3); // centred rows settle after a frame
    h.get_by_label_contains("Enregistrement 2026-10-18 082620 with a long name.mp4");
    for node in h.get_all_by(|n| n.bounding_box().is_some()) {
        let r = node.rect();
        assert!(
            r.max.x <= 560.5 && r.min.x >= -0.5,
            "{node:?} spans x={}..{} outside the 560pt card",
            r.min.x,
            r.max.x
        );
    }
    // The model row is centred: its left edge is well inside the card.
    let model = h.get_by_label("Model").rect();
    assert!(model.min.x > 60.0, "model row starts at x={}", model.min.x);
    assert!(model.max.x < 500.0, "model row ends at x={}", model.max.x);
}

#[test]
fn spotlight_without_a_window_says_so() {
    let _serial = serial();
    let s = base_state();
    let fm = s.platform.file_manager;
    let mut h = spotlight(s);
    h.run();
    h.get_by_label(&format!("No {fm} window detected"));
    h.get_by_label("Model");
    if snapshots_enabled() {
        h.snapshot("spotlight_idle");
    }
}

#[test]
fn spotlight_error_can_be_dismissed() {
    let _serial = serial();
    let mut s = base_state();
    s.spotlight.error = Some(Toast {
        message: "No model set up yet.".into(),
        expires_at: std::time::Instant::now() + std::time::Duration::from_secs(60),
    });
    let mut h = spotlight(s);
    h.run();
    h.get_by_label("No model set up yet.");
    h.get_by_label("Dismiss error").click();
    h.run();
    assert!(take(&mut h)
        .iter()
        .any(|m| matches!(m, Msg::DismissSpotlightError)));
}

#[test]
fn spotlight_refresh_models() {
    let _serial = serial();
    let mut h = spotlight(base_state());
    h.run();
    h.get_by_label("Refresh models").click();
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::RefreshModels)));
}

// --- main window: conversation ---

fn with_conversation(mut s: AppState) -> AppState {
    s.explorer = Some(ExplorerState {
        path: "C:/work".into(),
        selected_files: vec!["a.mov".into()],
    });
    s.sessions.current = Some("s1".into());
    s.sessions.list = vec![SessionMeta {
        id: "s1".into(),
        title: "Convert clips".into(),
        created_at: NOW_MS - 3_600_000,
        last_active_at: NOW_MS - 120_000,
        explorer_path: "C:/work".into(),
        file_count: 1,
        model: "ollama:llama3.2".into(),
        message_count: 3,
    }];
    s.chat.history = vec![
        msg("system", "You are cdout."),
        msg("user", "convert a.mov to mp4"),
        msg("assistant", "I'll convert it with **ffmpeg**."),
    ];
    s
}

#[test]
fn approval_card_buttons_emit_decisions() {
    let _serial = serial();
    let mut s = with_conversation(base_state());
    s.chat.pending_command = Some(PendingCommand {
        command: "ffmpeg -i a.mov a.mp4".into(),
        tool_call_id: Some("c1".into()),
        risk: Some(CommandRisk::Mutating),
        loop_reason: None,
    });
    let mut h = main_window(s);
    h.run();
    h.get_by_label("🖊 WRITES FILES");
    if snapshots_enabled() {
        h.snapshot("main_command_proposal");
    }

    h.get_by_label("⏵ Execute").click();
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(
        m,
        Msg::ApproveCommand {
            continue_auto: false
        }
    )));

    h.get_by_label("⏵⏵ All").click();
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(
        m,
        Msg::ApproveCommand {
            continue_auto: true
        }
    )));

    h.get_by_label("🗙 Dismiss").click();
    h.run();
    assert!(take(&mut h)
        .iter()
        .any(|m| matches!(m, Msg::DismissCommand)));
}

#[test]
fn reject_needs_feedback_before_sending() {
    let _serial = serial();
    let mut s = with_conversation(base_state());
    s.chat.pending_command = Some(PendingCommand {
        command: "ffmpeg -i a.mov a.mp4".into(),
        tool_call_id: Some("c1".into()),
        risk: Some(CommandRisk::Mutating),
        loop_reason: None,
    });
    let mut h = main_window(s);
    h.run();
    h.get_by_label("Reject with Feedback").click();
    h.run();
    assert!(h.state().s.chat.reject_open);
    assert!(take(&mut h).is_empty(), "opening the box sends nothing");

    h.get_by_label("Feedback").focus();
    h.run();
    h.get_by_label("Feedback").type_text("use mov instead");
    h.run();
    h.get_by_label("Send Feedback").click();
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::RejectCommand)));
    assert_eq!(h.state().s.chat.reject_feedback, "use mov instead");
}

#[test]
fn dangerous_commands_are_labelled() {
    let _serial = serial();
    let mut s = with_conversation(base_state());
    s.chat.pending_command = Some(PendingCommand {
        command: "sudo rm -rf /".into(),
        tool_call_id: None,
        risk: Some(CommandRisk::Dangerous),
        loop_reason: Some("same command 3x".into()),
    });
    let mut h = main_window(s);
    h.run();
    h.get_by_label("DESTRUCTIVE — READ IT");
    h.get_by_label("Loop detected: same command 3x");
}

#[test]
fn question_card_answers_with_the_picked_option() {
    let _serial = serial();
    let mut s = with_conversation(base_state());
    s.chat.pending_question = Some(PendingQuestion {
        data: QuestionData {
            question: "Which format?".into(),
            header: Some("Output".into()),
            options: vec![
                QuestionOption {
                    label: "mp4".into(),
                    description: Some("Smaller".into()),
                },
                QuestionOption {
                    label: "mov".into(),
                    description: None,
                },
            ],
            multi_select: false,
        },
        tool_call_id: Some("q1".into()),
    });
    let mut h = main_window(s);
    h.run();
    h.get_by_label("Which format?");
    if snapshots_enabled() {
        h.snapshot("main_question");
    }
    h.get_by_label("Answer").click();
    h.run();
    assert!(take(&mut h).is_empty(), "Answer is disabled until a pick");

    h.get_by_label("mp4").click();
    h.run();
    h.get_by_label("Answer").click();
    h.run();
    let out = take(&mut h);
    assert!(
        out.iter()
            .any(|m| matches!(m, Msg::AnswerQuestion(a) if a == "mp4")),
        "{out:?}"
    );
}

#[test]
fn custom_answer_replaces_the_options() {
    let _serial = serial();
    let mut s = with_conversation(base_state());
    s.chat.pending_question = Some(PendingQuestion {
        data: QuestionData {
            question: "Which format?".into(),
            header: None,
            options: vec![QuestionOption {
                label: "mp4".into(),
                description: None,
            }],
            multi_select: false,
        },
        tool_call_id: None,
    });
    let mut h = main_window(s);
    h.run();
    h.get_by_label("Other (type your own answer)").click();
    h.run();
    h.get_by_label("Your answer").focus();
    h.run();
    h.get_by_label("Your answer").type_text("gif");
    h.run();
    h.get_by_label("Answer").click();
    h.run();
    assert!(take(&mut h)
        .iter()
        .any(|m| matches!(m, Msg::AnswerQuestion(a) if a == "gif")));
}

#[test]
fn chat_input_sends_on_enter_and_stops_when_busy() {
    let _serial = serial();
    let mut h = main_window(with_conversation(base_state()));
    h.run();
    h.get_by_label("Message").focus();
    h.run();
    h.get_by_label("Message").type_text("next step");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::MainSubmit)));

    h.state_mut().s.chat.processing = true;
    h.run_steps(3); // the thinking spinner keeps repainting
    h.get_by_label("Stop").click();
    h.run_steps(3);
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::Stop)));
}

#[test]
fn transcript_renders_roles_and_internal_notes() {
    let _serial = serial();
    let mut s = with_conversation(base_state());
    s.chat.history.push(Message {
        synthetic: true,
        ..msg("user", "[Loop detected] stop and reassess")
    });
    s.chat
        .history
        .push(msg("tool", "[Exit code: 0 — Success]\nok"));
    let mut h = main_window(s);
    h.run();
    h.get_by_label("⚙ CDOUT INTERNAL");
    h.get_by_label("[Loop detected] stop and reassess");
    h.get_by_label("convert a.mov to mp4");
    h.get_by_label("[Exit code: 0 — Success]\nok");
    if snapshots_enabled() {
        h.snapshot("main_conversation");
    }
}

#[test]
fn long_messages_collapse_with_a_toggle() {
    let _serial = serial();
    let mut s = with_conversation(base_state());
    s.chat.history.push(msg("tool", &"x".repeat(400)));
    let idx = s.chat.history.len() - 1;
    let mut h = main_window(s);
    h.run();
    h.get_by_label("⏷ SHOW MORE").click();
    h.run();
    assert!(take(&mut h)
        .iter()
        .any(|m| matches!(m, Msg::ToggleExpand(i) if *i == idx)));
}

#[test]
fn long_unbroken_text_stays_inside_the_window() {
    let _serial = serial();
    // A path or hash with no spaces must wrap, not push the row off-screen.
    let token = "C:/very/long/path/".repeat(30);
    let mut s = with_conversation(base_state());
    s.chat.history.push(msg("user", &token));
    s.chat.history.push(msg("tool", &token));
    let mut h = main_window(s);
    h.run();
    for node in h.get_all_by(|n| n.bounding_box().is_some()) {
        let r = node.rect();
        assert!(
            r.max.x <= 1100.5,
            "{node:?} ends at x={} beyond the 1100pt window",
            r.max.x
        );
    }
}

#[test]
fn continue_is_offered_for_stalled_prose() {
    let _serial = serial();
    let mut h = main_window(with_conversation(base_state()));
    h.run();
    h.get_by_label("⟳ Continue").click();
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::ContinueAgent)));
}

#[test]
fn kill_dialog_offers_both_choices() {
    let _serial = serial();
    let mut s = with_conversation(base_state());
    s.chat.kill_dialog = Some(KillDialog {
        pid: 4242,
        command: "ffmpeg -i big.mov out.mp4".into(),
    });
    let mut h = main_window(s);
    h.run();
    h.get_by_label("PID: 4242");
    h.get_by_label("Kill process").click();
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::ConfirmKill)));
    h.get_by_label("Keep running").click();
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::KeepRunning)));
}

#[test]
fn toast_shows_and_dismisses() {
    let _serial = serial();
    let mut s = with_conversation(base_state());
    s.toast = Some(Toast {
        message: "LLM Error: 401".into(),
        expires_at: std::time::Instant::now() + std::time::Duration::from_secs(60),
    });
    let mut h = main_window(s);
    h.run();
    h.get_by_label("LLM Error: 401");
    h.get_by_label("Dismiss error").click();
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::DismissToast)));
}

#[test]
fn toolbar_sync_context_and_auto_badge() {
    let _serial = serial();
    let mut s = with_conversation(base_state());
    s.chat.auto_execute = true;
    let mut h = main_window(s);
    h.run();
    h.get_by_label("» Auto").click();
    h.run();
    assert!(take(&mut h)
        .iter()
        .any(|m| matches!(m, Msg::DisableAutoExecute)));
    h.get_by_label("Sync").click();
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::SyncExplorer)));
    h.get_by_label("Toggle Context Details").click();
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::ToggleContext)));
    h.get_by_label("New Chat").click();
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::NewChat)));
}

#[test]
fn context_drawer_lists_the_selection() {
    let _serial = serial();
    let mut s = with_conversation(base_state());
    s.main.context_open = true;
    let mut h = main_window(s);
    h.run();
    h.get_by_label("C:/work");
    h.get_by_label("🗋 a.mov");
}

// --- main window: sidebar ---

#[test]
fn sidebar_selects_renames_and_deletes() {
    let _serial = serial();
    let mut s = with_conversation(base_state());
    s.sessions.list.push(SessionMeta {
        id: "s2".into(),
        title: "Older chat".into(),
        created_at: NOW_MS - 90_000_000,
        last_active_at: NOW_MS - 86_400_000 * 2,
        explorer_path: "D:\\footage".into(),
        file_count: 4,
        model: "ollama:llama3.2".into(),
        message_count: 5,
    });
    let mut h = main_window(s);
    h.run();
    h.get_by_label("🗁 footage · 2d ago");
    h.get_by_label("Session: Older chat").click();
    h.run();
    assert!(take(&mut h)
        .iter()
        .any(|m| matches!(m, Msg::SelectSession(id) if id == "s2")));

    h.get_all_by_label("Rename").next().unwrap().click();
    h.run();
    assert_eq!(
        h.state().s.sessions.renaming.as_ref().map(|r| r.0.as_str()),
        Some("s1")
    );
    h.get_by_label("Session title").type_text(" (v2)");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(
        m,
        Msg::RenameSession { id, title } if id == "s1" && title == "Convert clips (v2)"
    )));

    h.state_mut().s.sessions.renaming = None;
    h.run();
    h.get_all_by_label("Delete session").next().unwrap().click();
    h.run();
    h.get_by_label("Delete this session?");
    h.get_by_label("Delete").click();
    h.run();
    assert!(take(&mut h)
        .iter()
        .any(|m| matches!(m, Msg::DeleteSession(id) if id == "s1")));
}

#[test]
fn sidebar_collapses() {
    let _serial = serial();
    let mut h = main_window(with_conversation(base_state()));
    h.run();
    h.get_by_label("Collapse sidebar").click();
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::ToggleSidebar)));
}

// --- onboarding and banner ---

fn cli_status(backend: Backend, installed: bool, signed_in: bool) -> CliStatus {
    CliStatus {
        backend,
        name: backend.name(),
        installed,
        signed_in,
        detail: if signed_in {
            "Max plan".into()
        } else {
            "Not signed in".into()
        },
        sign_in_command: backend.sign_in_command(),
        install_url: backend.install_url(),
    }
}

#[test]
fn settings_list_signed_in_subscriptions() {
    let _serial = serial();
    let mut s = settings_state(SettingsTab::Providers);
    s.provider_status = Some(ProviderStatus {
        subscriptions: vec![
            cli_status(Backend::ClaudeCode, true, true),
            cli_status(Backend::Codex, true, false),
        ],
        any_usable: true,
        ..Default::default()
    });
    let mut h = main_window(s);
    h.run();
    h.get_by_label("Subscriptions");
    h.get_by_label("signed in · Max plan");
    h.get_by_label("codex login");
}

#[test]
fn onboarding_appears_when_nothing_is_usable() {
    let _serial = serial();
    let mut s = AppState::new(platform_info(None));
    s.models.connected = Some(true);
    s.settings.ollama_url = "http://localhost:11434".into();
    s.provider_status = Some(ProviderStatus {
        ollama_reachable: false,
        subscriptions: vec![
            cli_status(Backend::ClaudeCode, true, false),
            cli_status(Backend::Codex, false, false),
        ],
        ..Default::default()
    });
    let mut h = main_window(s);
    h.run();
    h.get_by_label("Set up a model to get started");
    h.get_by_label("No Ollama at http://localhost:11434");
    h.get_by_label("Use a subscription you already have");
    h.get_by_label("installed, not signed in");
    h.get_by_label("Sign in to Claude Code").click();
    h.run();
    assert!(take(&mut h)
        .iter()
        .any(|m| matches!(m, Msg::SignInCli(Backend::ClaudeCode))));
    h.get_by_label("not installed");
    h.get_by_label("Get Codex ⬈").click();
    h.run();
    assert!(take(&mut h)
        .iter()
        .any(|m| matches!(m, Msg::OpenUrl(u) if u == Backend::Codex.install_url())));
    if snapshots_enabled() {
        h.snapshot("main_onboarding");
    }
    h.get_by_label("OpenRouter API key").focus();
    h.run();
    h.get_by_label("OpenRouter API key").type_text("sk-or-1");
    h.run();
    h.get_by_label("Save key & start").click();
    h.run();
    assert!(take(&mut h)
        .iter()
        .any(|m| matches!(m, Msg::OnboardingSaveKey(k) if k == "sk-or-1")));
}

#[test]
fn migration_banner_actions() {
    let _serial = serial();
    let mut s = with_conversation(base_state());
    s.main.migration_banner = true;
    let mut h = main_window(s);
    h.run();
    h.get_by_label("Old credentials found");
    h.get_by_label("🗑 Remove the old file").click();
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::CleanupLegacy)));
    h.get_by_label("Dismiss banner").click();
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::DismissBanner)));
}

// --- settings ---

fn settings_state(tab: SettingsTab) -> AppState {
    let mut s = with_conversation(base_state());
    s.main.settings_open = true;
    s.settings.tab = tab;
    s.settings.hotkey = "Ctrl+Alt+A".into();
    s.settings.draft_ollama_url = "http://localhost:11434".into();
    s.settings.keys.openrouter_set = true;
    s.settings.keys.openrouter_preview = Some("…3fa1".into());
    s.settings.skills = Some(ListSkillsResult {
        skills: vec![
            Skill {
                metadata: SkillMetadata {
                    name: "ffmpeg".into(),
                    description: "Video conversion".into(),
                    requires: None,
                },
                body: String::new(),
                source_path: "bundled:ffmpeg.md".into(),
                available: true,
                missing_bins: vec![],
            },
            Skill {
                metadata: SkillMetadata {
                    name: "oiiotool".into(),
                    description: "Image pipelines".into(),
                    requires: None,
                },
                body: String::new(),
                source_path: "bundled:oiiotool.md".into(),
                available: false,
                missing_bins: vec!["oiiotool".into()],
            },
        ],
        quarantined: vec![],
    });
    s
}

#[test]
fn settings_tabs_switch() {
    let _serial = serial();
    let mut h = main_window(settings_state(SettingsTab::General));
    h.run();
    h.get_by_label("Change the spotlight hotkey");
    if snapshots_enabled() {
        h.snapshot("settings_general");
    }
    h.get_by_label("Approval").click();
    h.run();
    assert!(take(&mut h)
        .iter()
        .any(|m| matches!(m, Msg::SetSettingsTab(SettingsTab::Approval))));
}

#[test]
fn settings_approval_modes() {
    let _serial = serial();
    let mut h = main_window(settings_state(SettingsTab::Approval));
    h.run();
    if snapshots_enabled() {
        h.snapshot("settings_approval");
    }
    h.get_by_role_and_label(egui::accesskit::Role::RadioButton, "Auto-run everything")
        .click();
    h.run();
    assert!(take(&mut h)
        .iter()
        .any(|m| matches!(m, Msg::SetApprovalMode(ApprovalMode::Auto))));
}

#[test]
fn settings_providers_show_masked_keys_only() {
    let _serial = serial();
    let mut h = main_window(settings_state(SettingsTab::Providers));
    h.run();
    h.get_by_label("Current key on file: …3fa1. Type a new value to replace.");
    if snapshots_enabled() {
        h.snapshot("settings_providers");
    }
    h.get_by_label("Show free-tier models").click();
    h.run();
    assert!(take(&mut h)
        .iter()
        .any(|m| matches!(m, Msg::ToggleShowFree)));
    h.get_by_label("Save").click();
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::SaveSettings)));
}

#[test]
fn settings_skills_list_availability() {
    let _serial = serial();
    let mut h = main_window(settings_state(SettingsTab::Skills));
    h.run();
    h.get_by_label("ffmpeg");
    h.get_by_label("× Missing: oiiotool");
    if snapshots_enabled() {
        h.snapshot("settings_skills");
    }
}

#[test]
fn settings_hotkey_recorder_records_a_chord() {
    let _serial = serial();
    let mut h = main_window(settings_state(SettingsTab::General));
    h.run();
    h.get_by_label("Change the spotlight hotkey").click();
    h.run();
    assert!(h.state().s.settings.recording_hotkey);
    h.key_press_modifiers(
        Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        },
        Key::K,
    );
    h.run();
    let out = take(&mut h);
    assert!(
        out.iter()
            .any(|m| matches!(m, Msg::ChangeHotkey(c) if c == "Ctrl+Shift+K")),
        "{out:?}"
    );
    assert!(!h.state().s.settings.recording_hotkey);
}

#[test]
fn settings_escape_closes() {
    let _serial = serial();
    let mut h = main_window(settings_state(SettingsTab::General));
    h.run();
    h.key_press(Key::Escape);
    h.run();
    assert!(take(&mut h).iter().any(|m| matches!(m, Msg::CloseSettings)));
}
