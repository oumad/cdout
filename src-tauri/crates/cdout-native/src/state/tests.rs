//! Branch-by-branch tests of the state machine. Each test drives
//! `AppState::update` with messages and asserts on state and effects; no
//! I/O happens.

use std::time::{Duration, Instant};

use cdout_core::api::platform_info;
use cdout_core::features::agent::{
    AgentResponse, AgentStepResult, QuestionData, QuestionOption, ToolProposal,
};
use cdout_core::features::explorer::ExplorerState;
use cdout_core::features::loop_detector::LoopVerdict;
use cdout_core::features::sessions::{Session, SessionMeta};
use cdout_core::llm::Message;
use cdout_core::tools::risk::CommandRisk;
use cdout_core::utils::config::ApprovalMode;

use super::agent::may_run_unattended;
use super::flows::pick_replacement;
use super::*;

const READ_ONLY_CMD: &str = "ffprobe -v error clip.mov";
const MUTATING_CMD: &str = "ffmpeg -i a.mov b.mp4";
const DANGEROUS_CMD: &str = "sudo rm -rf /";

fn t0() -> Instant {
    Instant::now()
}

fn ms(base: Instant, n: u64) -> Instant {
    base + Duration::from_millis(n)
}

fn state() -> AppState {
    let mut s = AppState::new(platform_info(None));
    s.models.selected = "ollama:test".into();
    s.models.available = vec!["ollama:test".into()];
    s.models.connected = Some(true);
    s.models.hydrated = true;
    s
}

fn explorer(files: &[&str]) -> ExplorerState {
    ExplorerState {
        path: "C:/work".into(),
        selected_files: files.iter().map(|f| f.to_string()).collect(),
    }
}

fn msg(role: &str, content: &str) -> Message {
    Message {
        role: role.into(),
        content: content.into(),
        tool_calls: None,
        synthetic: false,
    }
}

fn session(id: &str, messages: Vec<Message>) -> Session {
    Session {
        meta: SessionMeta {
            id: id.into(),
            title: "t".into(),
            created_at: 0,
            last_active_at: 0,
            explorer_path: "C:/saved".into(),
            file_count: 0,
            model: "ollama:saved".into(),
            message_count: messages.len(),
        },
        messages,
    }
}

fn step_result(response: AgentResponse, history: Vec<Message>) -> AgentStepResult {
    AgentStepResult {
        response,
        updated_history: history,
        loop_verdict: None,
    }
}

fn command(cmd: &str, id: &str) -> AgentResponse {
    AgentResponse::ToolProposals(vec![ToolProposal {
        tool_name: cdout_core::constants::TOOL_NAME.into(),
        command: cmd.into(),
        tool_call_id: Some(id.into()),
        question_data: None,
    }])
}

fn question() -> AgentResponse {
    AgentResponse::ToolProposals(vec![
        ToolProposal {
            tool_name: cdout_core::constants::TOOL_NAME.into(),
            command: READ_ONLY_CMD.into(),
            tool_call_id: Some("c1".into()),
            question_data: None,
        },
        ToolProposal {
            tool_name: "ask_user_question".into(),
            command: "Which format?".into(),
            tool_call_id: Some("q1".into()),
            question_data: Some(QuestionData {
                question: "Which format?".into(),
                header: None,
                options: vec![
                    QuestionOption {
                        label: "mp4".into(),
                        description: None,
                    },
                    QuestionOption {
                        label: "mov".into(),
                        description: None,
                    },
                ],
                multi_select: false,
            }),
        },
    ])
}

fn has(fx: &[Effect], pred: impl Fn(&Effect) -> bool) -> bool {
    fx.iter().any(pred)
}

fn run_step_of(fx: &[Effect]) -> Option<(u64, String, Vec<Message>, bool)> {
    fx.iter().find_map(|e| match e {
        Effect::RunStep {
            gen,
            model,
            history,
            drop_tools,
        } => Some((*gen, model.clone(), history.clone(), *drop_tools)),
        _ => None,
    })
}

/// A state with a live session and an in-flight step, ready for StepDone.
fn mid_conversation() -> (AppState, Instant) {
    let now = t0();
    let mut s = state();
    s.explorer = Some(explorer(&["a.mov"]));
    s.sessions.current = Some("s1".into());
    s.chat.history = vec![msg("system", "sys"), msg("user", "do it")];
    s.chat.input = "next".into();
    let fx = s.update(Msg::MainSubmit, now);
    assert!(run_step_of(&fx).is_some(), "{fx:?}");
    (s, now)
}

// --- spotlight ---

#[test]
fn hotkey_shows_spotlight_and_requests_focus() {
    let mut s = state();
    let fx = s.update(Msg::HotkeyPressed, t0());
    assert_eq!(fx, vec![Effect::ShowSpotlight]);
    assert!(s.spotlight.visible);
    assert!(s.spotlight.focus_input);
}

#[test]
fn spotlight_blur_respects_the_grace_period() {
    let now = t0();
    let mut s = state();
    s.update(Msg::HotkeyPressed, now);
    let fx = s.update(Msg::SpotlightFocusChanged(true), now);
    assert!(has(&fx, |e| *e
        == Effect::FetchExplorer(ExplorerPurpose::Spotlight)));

    let fx = s.update(Msg::SpotlightFocusChanged(false), ms(now, 100));
    assert!(fx.is_empty(), "blur inside grace must not hide: {fx:?}");
    assert!(s.spotlight.visible);

    let fx = s.update(Msg::SpotlightFocusChanged(false), ms(now, 400));
    assert_eq!(fx, vec![Effect::HideSpotlight(HideReason::Blur)]);
    assert!(!s.spotlight.visible);
}

#[test]
fn escape_hides_once() {
    let mut s = state();
    s.update(Msg::HotkeyPressed, t0());
    assert_eq!(
        s.update(Msg::SpotlightHide(HideReason::Escape), t0()),
        vec![Effect::HideSpotlight(HideReason::Escape)]
    );
    assert!(s
        .update(Msg::SpotlightHide(HideReason::Escape), t0())
        .is_empty());
}

#[test]
fn empty_spotlight_prompt_does_nothing() {
    let mut s = state();
    s.update(Msg::HotkeyPressed, t0());
    s.spotlight.prompt = "   ".into();
    assert!(s
        .update(
            Msg::SpotlightSubmit {
                auto_approve: false
            },
            t0()
        )
        .is_empty());
}

#[test]
fn spotlight_without_a_model_shows_the_setup_hint() {
    let mut s = state();
    s.models.selected.clear();
    s.update(Msg::HotkeyPressed, t0());
    s.spotlight.prompt = "rename".into();
    let fx = s.update(
        Msg::SpotlightSubmit {
            auto_approve: false,
        },
        t0(),
    );
    assert!(fx.is_empty());
    assert_eq!(
        s.spotlight.error.as_ref().map(|t| t.message.as_str()),
        Some(NO_MODEL_SPOTLIGHT_ERROR)
    );
    assert_eq!(s.spotlight.prompt, "rename", "prompt kept for retry");
}

#[test]
fn spotlight_submit_runs_the_whole_handoff() {
    let now = t0();
    let mut s = state();
    s.chat.history = vec![msg("user", "old")];
    s.sessions.current = Some("old".into());
    s.sessions.save_due = Some(ms(now, 999));
    s.update(Msg::HotkeyPressed, now);
    s.spotlight.prompt = "compress these".into();
    let gen_before = s.chat.generation;

    let fx = s.update(Msg::SpotlightSubmit { auto_approve: true }, now);
    let gen = s.chat.generation;
    assert_eq!(gen, gen_before + 1);
    assert_eq!(fx[0], Effect::HideSpotlight(HideReason::Submit));
    assert_eq!(
        fx[1],
        Effect::ShowMain {
            center_on_cursor: true
        }
    );
    assert!(has(
        &fx,
        |e| matches!(e, Effect::SaveSession { id, .. } if id == "old")
    ));
    assert!(has(&fx, |e| *e == Effect::CancelStream));
    assert!(has(&fx, |e| *e
        == Effect::FetchExplorer(ExplorerPurpose::SpotlightTask {
            gen,
            prompt: "compress these".into(),
            model: "ollama:test".into(),
            auto_approve: true,
        })));
    assert!(s.spotlight.prompt.is_empty());
    assert!(s.chat.history.is_empty());
    assert!(s.chat.processing);

    let fx = s.update(
        Msg::ExplorerResult {
            purpose: ExplorerPurpose::SpotlightTask {
                gen,
                prompt: "compress these".into(),
                model: "ollama:test".into(),
                auto_approve: true,
            },
            result: Ok(explorer(&["a.mov", "b.mov"])),
        },
        now,
    );
    let create = fx
        .iter()
        .find(|e| matches!(e, Effect::CreateSession { .. }))
        .expect("session created");
    match create {
        Effect::CreateSession {
            prompt,
            path,
            files,
            model,
            ..
        } => {
            assert_eq!(prompt, "compress these");
            assert_eq!(path, "C:/work");
            assert_eq!(files.len(), 2);
            assert_eq!(model, "ollama:test");
        }
        _ => unreachable!(),
    }

    let fx = s.update(
        Msg::SessionCreated {
            purpose: SessionPurpose::SpotlightTask {
                gen,
                auto_approve: true,
                model: "ollama:test".into(),
            },
            result: Ok(session(
                "new",
                vec![msg("system", "s"), msg("user", "compress these")],
            )),
        },
        now,
    );
    assert_eq!(s.sessions.current.as_deref(), Some("new"));
    assert!(
        s.chat.auto_execute,
        "modifier submit pre-authorises the run"
    );
    let (g, model, history, drop) = run_step_of(&fx).expect("first step");
    assert_eq!(
        (g, model.as_str(), history.len(), drop),
        (gen, "ollama:test", 2, false)
    );
    assert!(s.chat.step.as_ref().unwrap().should_auto);
}

#[test]
fn stale_spotlight_explorer_result_is_ignored() {
    let mut s = state();
    s.update(Msg::HotkeyPressed, t0());
    s.spotlight.prompt = "x".into();
    s.update(
        Msg::SpotlightSubmit {
            auto_approve: false,
        },
        t0(),
    );
    let gen = s.chat.generation;
    s.update(Msg::NewChat, t0()); // disowns the in-flight handoff
    let fx = s.update(
        Msg::ExplorerResult {
            purpose: ExplorerPurpose::SpotlightTask {
                gen,
                prompt: "x".into(),
                model: "ollama:test".into(),
                auto_approve: false,
            },
            result: Ok(explorer(&[])),
        },
        t0(),
    );
    assert!(!has(&fx, |e| matches!(e, Effect::CreateSession { .. })));
}

// --- starting from the main window ---

#[test]
fn main_prompt_without_context_fetches_it_first() {
    let now = t0();
    let mut s = state();
    s.chat.input = "list files".into();
    let fx = s.update(Msg::MainSubmit, now);
    let gen = s.chat.generation;
    assert_eq!(
        fx,
        vec![Effect::FetchExplorer(ExplorerPurpose::MainStart {
            gen,
            prompt: "list files".into()
        })]
    );
    assert!(s.chat.input.is_empty());
    let fx = s.update(
        Msg::ExplorerResult {
            purpose: ExplorerPurpose::MainStart {
                gen,
                prompt: "list files".into(),
            },
            result: Ok(explorer(&[])),
        },
        now,
    );
    assert!(has(
        &fx,
        |e| matches!(e, Effect::CreateSession { prompt, .. } if prompt == "list files")
    ));
}

#[test]
fn main_prompt_context_failure_restores_the_input() {
    let mut s = state();
    s.chat.input = "list files".into();
    s.update(Msg::MainSubmit, t0());
    let gen = s.chat.generation;
    s.update(
        Msg::ExplorerResult {
            purpose: ExplorerPurpose::MainStart {
                gen,
                prompt: "list files".into(),
            },
            result: Err("COM failed".into()),
        },
        t0(),
    );
    assert_eq!(s.chat.input, "list files");
    assert!(!s.chat.processing);
    assert_eq!(s.toast.as_ref().unwrap().message, "COM failed");
}

#[test]
fn follow_up_prompt_appends_and_runs() {
    let (s, _) = mid_conversation();
    assert_eq!(s.chat.history.last().unwrap().content, "next");
    assert!(s.chat.processing);
    assert!(s.sessions.save_due.is_some());
}

#[test]
fn changed_selection_is_announced_before_the_prompt() {
    let now = t0();
    let mut s = state();
    s.explorer = Some(explorer(&["a.mov"]));
    s.sessions.current = Some("s1".into());
    s.chat.history = vec![msg("system", "sys")];
    s.chat.last_synced = Some(("C:/work".into(), vec!["a.mov".into()]));
    s.update(Msg::SyncExplorer, now);
    s.update(
        Msg::ExplorerResult {
            purpose: ExplorerPurpose::MainSync,
            result: Ok(explorer(&["a.mov", "b.mov"])),
        },
        now,
    );
    assert!(s.chat.sync_pending);
    s.chat.input = "go".into();
    s.update(Msg::MainSubmit, now);
    let n = s.chat.history.len();
    assert!(s.chat.history[n - 2]
        .content
        .starts_with("[CONTEXT UPDATE - Files have changed]"));
    assert!(s.chat.history[n - 2].content.contains("2. b.mov"));
    assert_eq!(s.chat.history[n - 1].content, "go");
    assert!(!s.chat.sync_pending);
}

#[test]
fn unchanged_selection_is_not_pending() {
    let mut s = state();
    s.chat.history = vec![msg("system", "sys")];
    s.chat.last_synced = Some(("C:/work".into(), vec!["a.mov".into()]));
    s.update(Msg::SyncExplorer, t0());
    s.update(
        Msg::ExplorerResult {
            purpose: ExplorerPurpose::MainSync,
            result: Ok(explorer(&["a.mov"])),
        },
        t0(),
    );
    assert!(!s.chat.sync_pending);
}

#[test]
fn busy_prompts_queue_and_drain_when_idle() {
    let (mut s, now) = mid_conversation();
    s.chat.input = "second".into();
    let fx = s.update(Msg::MainSubmit, now);
    assert!(run_step_of(&fx).is_none());
    assert_eq!(s.chat.queued, vec!["second".to_string()]);

    let gen = s.chat.generation;
    let mut hist = s.chat.history.clone();
    hist.push(msg("assistant", "Task Complete"));
    let fx = s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                AgentResponse::Text("Task Complete".into()),
                hist,
            )),
        },
        now,
    );
    assert!(s.chat.queued.is_empty());
    let (_, _, history, _) = run_step_of(&fx).expect("queued prompt started");
    assert_eq!(history.last().unwrap().content, "second");
}

#[test]
fn removing_a_queued_prompt() {
    let mut s = state();
    s.chat.queued = ["a", "b", "c"].iter().map(|x| x.to_string()).collect();
    s.chat.processing = true;
    s.update(Msg::RemoveQueued(1), t0());
    assert_eq!(s.chat.queued, vec!["a".to_string(), "c".to_string()]);
    s.update(Msg::RemoveQueued(9), t0());
    assert_eq!(s.chat.queued.len(), 2);
}

// --- streaming and results ---

#[test]
fn deltas_accumulate_only_for_the_current_generation() {
    let (mut s, now) = mid_conversation();
    let gen = s.chat.generation;
    s.update(
        Msg::StreamDelta {
            gen,
            text: "Hel".into(),
        },
        now,
    );
    s.update(
        Msg::StreamDelta {
            gen,
            text: "lo".into(),
        },
        now,
    );
    s.update(
        Msg::StreamDelta {
            gen: gen + 1,
            text: "stale".into(),
        },
        now,
    );
    assert_eq!(s.chat.streaming, "Hello");
}

#[test]
fn stale_step_results_are_ignored() {
    let (mut s, now) = mid_conversation();
    let gen = s.chat.generation;
    s.update(Msg::NewChat, now);
    let fx = s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                AgentResponse::Text("hi".into()),
                vec![msg("assistant", "hi")],
            )),
        },
        now,
    );
    assert!(fx.is_empty());
    assert!(s.chat.history.is_empty());
}

#[test]
fn step_error_toasts_and_stops() {
    let (mut s, now) = mid_conversation();
    let gen = s.chat.generation;
    s.update(
        Msg::StepDone {
            gen,
            result: Err("401".into()),
        },
        now,
    );
    assert!(!s.chat.processing);
    assert_eq!(s.toast.as_ref().unwrap().message, "LLM Error: 401");
}

#[test]
fn text_reply_ends_a_manual_run() {
    let (mut s, now) = mid_conversation();
    let gen = s.chat.generation;
    let fx = s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                AgentResponse::Text("I'll use ffmpeg".into()),
                vec![msg("assistant", "I'll use ffmpeg")],
            )),
        },
        now,
    );
    assert!(fx.is_empty());
    assert!(!s.chat.processing);
    assert!(s.should_show_continue());
}

#[test]
fn question_wins_over_a_sibling_command() {
    let (mut s, now) = mid_conversation();
    s.chat.auto_execute = true;
    s.chat.step.as_mut().unwrap().should_auto = true;
    let gen = s.chat.generation;
    let fx = s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(question(), vec![msg("assistant", "")])),
        },
        now,
    );
    assert!(fx.is_empty());
    let q = s.chat.pending_question.as_ref().unwrap();
    assert_eq!(q.data.options.len(), 2);
    assert_eq!(q.tool_call_id.as_deref(), Some("q1"));
    assert!(s.chat.pending_command.is_none());
    assert!(!s.chat.auto_execute, "a question ends the unattended run");
}

#[test]
fn answering_a_question_feeds_a_correlated_tool_result() {
    let (mut s, now) = mid_conversation();
    let gen = s.chat.generation;
    s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(question(), vec![msg("assistant", "")])),
        },
        now,
    );
    let fx = s.update(Msg::AnswerQuestion("mp4".into()), now);
    let last = s.chat.history.last().unwrap();
    assert_eq!((last.role.as_str(), last.content.as_str()), ("tool", "mp4"));
    let calls = last.tool_calls.as_ref().unwrap();
    assert_eq!(calls[0].id.as_deref(), Some("q1"));
    assert_eq!(calls[0].function.name, "ask_user_question");
    assert!(run_step_of(&fx).is_some());
}

#[test]
fn dismissing_a_question_records_it_and_stops() {
    let (mut s, now) = mid_conversation();
    let gen = s.chat.generation;
    s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(question(), vec![msg("assistant", "")])),
        },
        now,
    );
    let fx = s.update(Msg::DismissQuestion, now);
    assert!(run_step_of(&fx).is_none());
    assert_eq!(
        s.chat.history.last().unwrap().content,
        text::QUESTION_DISMISSED
    );
    assert!(!s.chat.processing);
}

#[test]
fn approval_policy_matrix() {
    use ApprovalMode as M;
    use CommandRisk as R;
    assert!(!may_run_unattended(false, M::Ask, R::ReadOnly));
    assert!(may_run_unattended(false, M::ReadOnly, R::ReadOnly));
    assert!(!may_run_unattended(false, M::ReadOnly, R::Mutating));
    assert!(may_run_unattended(false, M::Auto, R::Mutating));
    assert!(may_run_unattended(true, M::Ask, R::Mutating));
    for auto in [false, true] {
        for mode in [M::Ask, M::ReadOnly, M::Auto] {
            assert!(!may_run_unattended(auto, mode, R::Dangerous));
        }
    }
}

#[test]
fn read_only_command_runs_unattended_in_read_only_mode() {
    let (mut s, now) = mid_conversation();
    s.settings.approval_mode = ApprovalMode::ReadOnly;
    let gen = s.chat.generation;
    let fx = s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                command(READ_ONLY_CMD, "c1"),
                vec![msg("assistant", "")],
            )),
        },
        now,
    );
    assert_eq!(
        fx,
        vec![Effect::Execute {
            gen,
            command: READ_ONLY_CMD.into(),
            cwd: Some("C:/work".into())
        }]
    );
    assert!(s.chat.executing && !s.chat.processing);

    let fx = s.update(
        Msg::ExecDone {
            gen,
            result: Ok("[Exit code: 0 — Success]\nok".into()),
        },
        now,
    );
    let tool = s.chat.history.last().unwrap();
    assert_eq!(tool.role, "tool");
    assert_eq!(
        tool.tool_calls.as_ref().unwrap()[0].id.as_deref(),
        Some("c1")
    );
    let (_, _, _, _) = run_step_of(&fx).expect("loop continues");
    assert!(!s.chat.executing);
}

#[test]
fn mutating_command_waits_for_approval() {
    let (mut s, now) = mid_conversation();
    s.settings.approval_mode = ApprovalMode::ReadOnly;
    let gen = s.chat.generation;
    let fx = s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                command(MUTATING_CMD, "c1"),
                vec![msg("assistant", "")],
            )),
        },
        now,
    );
    assert!(fx.is_empty());
    let p = s.chat.pending_command.as_ref().unwrap();
    assert_eq!(p.risk, Some(CommandRisk::Mutating));
    assert!(!s.chat.processing);
}

#[test]
fn dangerous_command_never_runs_unattended() {
    let (mut s, now) = mid_conversation();
    s.settings.approval_mode = ApprovalMode::Auto;
    s.chat.auto_execute = true;
    s.chat.step.as_mut().unwrap().should_auto = true;
    let gen = s.chat.generation;
    let fx = s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                command(DANGEROUS_CMD, "c1"),
                vec![msg("assistant", "")],
            )),
        },
        now,
    );
    assert!(fx.is_empty(), "{fx:?}");
    assert_eq!(
        s.chat.pending_command.as_ref().unwrap().risk,
        Some(CommandRisk::Dangerous)
    );
    assert!(!s.chat.auto_execute);
}

#[test]
fn approve_runs_the_edited_command_then_continues() {
    let (mut s, now) = mid_conversation();
    let gen = s.chat.generation;
    s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                command(MUTATING_CMD, "c1"),
                vec![msg("assistant", "")],
            )),
        },
        now,
    );
    s.chat.pending_command.as_mut().unwrap().command = "ffmpeg -i a.mov c.mp4".into();
    let fx = s.update(
        Msg::ApproveCommand {
            continue_auto: false,
        },
        now,
    );
    assert_eq!(
        fx,
        vec![Effect::Execute {
            gen,
            command: "ffmpeg -i a.mov c.mp4".into(),
            cwd: Some("C:/work".into())
        }]
    );
    assert!(
        s.update(
            Msg::ApproveCommand {
                continue_auto: false
            },
            now
        )
        .is_empty(),
        "no double run"
    );
    let fx = s.update(
        Msg::ExecDone {
            gen,
            result: Ok("done".into()),
        },
        now,
    );
    assert!(s.chat.pending_command.is_none());
    let (_, _, history, _) = run_step_of(&fx).unwrap();
    let tool = history.last().unwrap();
    assert_eq!(tool.content, "done");
    assert_eq!(
        tool.tool_calls.as_ref().unwrap()[0].id.as_deref(),
        Some("c1")
    );
    assert!(!s.chat.step.as_ref().unwrap().should_auto);
}

#[test]
fn run_all_turns_on_auto_execute() {
    let (mut s, now) = mid_conversation();
    let gen = s.chat.generation;
    s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                command(MUTATING_CMD, "c1"),
                vec![msg("assistant", "")],
            )),
        },
        now,
    );
    s.update(
        Msg::ApproveCommand {
            continue_auto: true,
        },
        now,
    );
    assert!(s.chat.auto_execute);
    s.update(
        Msg::ExecDone {
            gen,
            result: Ok("ok".into()),
        },
        now,
    );
    assert!(s.chat.step.as_ref().unwrap().should_auto);
}

#[test]
fn approved_exec_failure_keeps_the_proposal() {
    let (mut s, now) = mid_conversation();
    let gen = s.chat.generation;
    s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                command(MUTATING_CMD, "c1"),
                vec![msg("assistant", "")],
            )),
        },
        now,
    );
    s.update(
        Msg::ApproveCommand {
            continue_auto: false,
        },
        now,
    );
    s.update(
        Msg::ExecDone {
            gen,
            result: Err("bad input".into()),
        },
        now,
    );
    assert!(s.chat.pending_command.is_some());
    assert!(!s.chat.executing && !s.chat.processing);
    assert_eq!(
        s.toast.as_ref().unwrap().message,
        "Command execution failed: bad input"
    );
}

#[test]
fn reject_sends_feedback_as_the_tool_result() {
    let (mut s, now) = mid_conversation();
    let gen = s.chat.generation;
    s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                command(MUTATING_CMD, "c1"),
                vec![msg("assistant", "")],
            )),
        },
        now,
    );
    s.chat.reject_feedback = "use mov".into();
    let fx = s.update(Msg::RejectCommand, now);
    let (_, _, history, _) = run_step_of(&fx).unwrap();
    let last = history.last().unwrap();
    assert_eq!(last.content, "[User rejected the command] use mov");
    assert_eq!(
        last.tool_calls.as_ref().unwrap()[0].id.as_deref(),
        Some("c1")
    );
    assert!(s.chat.reject_feedback.is_empty());
}

#[test]
fn dismiss_answers_the_tool_use_and_stops() {
    let (mut s, now) = mid_conversation();
    let gen = s.chat.generation;
    s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                command(MUTATING_CMD, "c1"),
                vec![msg("assistant", "")],
            )),
        },
        now,
    );
    let fx = s.update(Msg::DismissCommand, now);
    assert!(fx.is_empty());
    assert_eq!(
        s.chat.history.last().unwrap().content,
        text::COMMAND_DISMISSED
    );
    assert!(s.chat.pending_command.is_none());
}

#[test]
fn loop_block_pauses_and_break_drops_tools_next_time() {
    let (mut s, now) = mid_conversation();
    s.chat.auto_execute = true;
    s.chat.step.as_mut().unwrap().should_auto = true;
    let gen = s.chat.generation;
    let mut r = step_result(command(READ_ONLY_CMD, "c1"), vec![msg("assistant", "")]);
    r.loop_verdict = Some(LoopVerdict::Break("same command 3x".into()));
    let fx = s.update(Msg::StepDone { gen, result: Ok(r) }, now);
    assert!(fx.is_empty());
    assert!(!s.chat.auto_execute);
    assert!(s.chat.drop_tools_next);
    let note = s.chat.history.last().unwrap();
    assert!(note.synthetic);
    assert!(note.content.starts_with("[Loop detected] same command 3x"));
    assert_eq!(
        s.chat
            .pending_command
            .as_ref()
            .unwrap()
            .loop_reason
            .as_deref(),
        Some("same command 3x")
    );

    let fx = s.update(Msg::RejectCommand, now);
    let (_, _, _, drop) = run_step_of(&fx).unwrap();
    assert!(
        drop,
        "the Break verdict withholds tools from the next request"
    );
    assert!(!s.chat.drop_tools_next, "consumed once");
}

#[test]
fn three_failures_in_a_row_pause_auto_execution() {
    let (mut s, now) = mid_conversation();
    s.settings.approval_mode = ApprovalMode::Auto;
    s.chat.auto_execute = true;
    s.chat.step.as_mut().unwrap().should_auto = true;
    for i in 0..3 {
        let gen = s.chat.generation;
        let fx = s.update(
            Msg::StepDone {
                gen,
                result: Ok(step_result(
                    command(MUTATING_CMD, &format!("c{i}")),
                    s.chat.history.clone(),
                )),
            },
            now,
        );
        assert!(
            has(&fx, |e| matches!(e, Effect::Execute { .. })),
            "iteration {i}"
        );
        let fx = s.update(
            Msg::ExecDone {
                gen,
                result: Ok("[Exit code: 1 — Failed]\nboom".into()),
            },
            now,
        );
        if i < 2 {
            assert!(run_step_of(&fx).is_some());
        } else {
            assert!(run_step_of(&fx).is_none());
        }
    }
    assert!(!s.chat.auto_execute);
    assert_eq!(s.chat.consecutive_failures, 0);
    assert!(s
        .chat
        .history
        .last()
        .unwrap()
        .content
        .starts_with("[Auto-execute paused] 3 commands failed in a row."));
}

#[test]
fn auto_steps_are_capped() {
    let (mut s, now) = mid_conversation();
    s.settings.approval_mode = ApprovalMode::Auto;
    s.chat.auto_step_count = MAX_AUTO_STEPS - 1;
    let gen = s.chat.generation;
    let fx = s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                command(MUTATING_CMD, "c"),
                vec![msg("assistant", "")],
            )),
        },
        now,
    );
    assert!(fx.is_empty(), "the 10th step waits for a click");
    assert!(s.chat.pending_command.is_some());
}

#[test]
fn unattended_prose_gets_one_delayed_nudge() {
    let (mut s, now) = mid_conversation();
    s.chat.auto_execute = true;
    s.chat.step.as_mut().unwrap().should_auto = true;
    let gen = s.chat.generation;
    let fx = s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                AgentResponse::Text("Now I will convert".into()),
                vec![msg("assistant", "Now I will convert")],
            )),
        },
        now,
    );
    assert!(fx.is_empty());
    assert!(!s.chat.is_idle(), "the pending nudge keeps the loop busy");
    assert_eq!(s.next_wakeup(), Some(ms(now, 500)).min(s.sessions.save_due));

    assert!(run_step_of(&s.update(Msg::Tick, ms(now, 499))).is_none());
    let fx = s.update(Msg::Tick, ms(now, 500));
    let (_, _, history, _) = run_step_of(&fx).expect("continuation");
    let nudge = history.last().unwrap();
    assert!(nudge.synthetic);
    assert_eq!(nudge.content, text::AUTO_CONTINUE_NUDGE);
}

#[test]
fn unattended_completion_ends_the_run() {
    let (mut s, now) = mid_conversation();
    s.chat.auto_execute = true;
    s.chat.step.as_mut().unwrap().should_auto = true;
    let gen = s.chat.generation;
    s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                AgentResponse::Text("Task Complete".into()),
                vec![msg("assistant", "Task Complete")],
            )),
        },
        now,
    );
    assert!(s.chat.auto_step_at.is_none());
    assert!(!s.chat.auto_execute);
}

#[test]
fn legacy_command_proposal_uses_the_platform_tool_name() {
    let (mut s, now) = mid_conversation();
    s.settings.approval_mode = ApprovalMode::Ask;
    let gen = s.chat.generation;
    s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                AgentResponse::CommandProposal(MUTATING_CMD.into()),
                vec![msg("assistant", "")],
            )),
        },
        now,
    );
    let p = s.chat.pending_command.as_ref().unwrap();
    assert_eq!(p.command, MUTATING_CMD);
    assert_eq!(p.tool_call_id, None);
}

// --- stop ---

#[test]
fn stop_keeps_partial_text_and_disowns_the_stream() {
    let (mut s, now) = mid_conversation();
    let gen = s.chat.generation;
    s.update(
        Msg::StreamDelta {
            gen,
            text: "Partial ans".into(),
        },
        now,
    );
    let fx = s.update(Msg::Stop, now);
    assert!(has(&fx, |e| *e == Effect::CancelStream));
    assert!(has(&fx, |e| *e == Effect::QueryRunningCommand));
    let n = s.chat.history.len();
    assert_eq!(s.chat.history[n - 2].role, "assistant");
    assert_eq!(s.chat.history[n - 2].content, "Partial ans");
    assert_eq!(s.chat.history[n - 1].content, text::INTERRUPTED_NOTE);
    assert!(s.chat.is_idle());

    let fx = s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(AgentResponse::Text("late".into()), vec![])),
        },
        now,
    );
    assert!(fx.is_empty());
    assert_eq!(s.chat.history.len(), n, "late result ignored");
}

#[test]
fn stop_while_idle_does_not_swallow_the_next_step() {
    let (mut s, now) = mid_conversation();
    let gen = s.chat.generation;
    s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                AgentResponse::Text("hi".into()),
                vec![msg("assistant", "hi")],
            )),
        },
        now,
    );
    s.update(Msg::Stop, now);
    s.chat.input = "again".into();
    s.update(Msg::MainSubmit, now);
    let gen = s.chat.generation;
    s.update(
        Msg::StepDone {
            gen,
            result: Ok(step_result(
                AgentResponse::Text("second answer".into()),
                vec![msg("assistant", "second answer")],
            )),
        },
        now,
    );
    assert_eq!(s.chat.history.last().unwrap().content, "second answer");
}

#[test]
fn running_command_after_stop_opens_the_kill_dialog() {
    let mut s = state();
    s.update(
        Msg::RunningCommandLoaded(Some(cdout_core::tools::shell::RunningCommand {
            pid: 42,
            command_preview: "ffmpeg ...".into(),
        })),
        t0(),
    );
    assert_eq!(s.chat.kill_dialog.as_ref().unwrap().pid, 42);
    assert_eq!(
        s.update(Msg::ConfirmKill, t0()),
        vec![Effect::KillRunningCommand]
    );
    assert!(s.chat.kill_dialog.is_none());
    assert!(s.update(Msg::ConfirmKill, t0()).is_empty());
}

// --- continue ---

#[test]
fn continue_is_offered_only_for_stalled_prose() {
    let mut s = state();
    s.chat.history = vec![msg("assistant", "I will run ffmpeg next")];
    assert!(s.should_show_continue());
    s.chat.history = vec![msg("assistant", "Which format do you want?")];
    assert!(!s.should_show_continue());
    s.chat.history = vec![msg("assistant", "All done.")];
    assert!(!s.should_show_continue());
    s.chat.history = vec![msg("user", "hi")];
    assert!(!s.should_show_continue());
    s.chat.history = vec![msg("assistant", "I will run ffmpeg next")];
    s.chat.processing = true;
    assert!(!s.should_show_continue());
}

#[test]
fn continue_appends_the_platform_nudge() {
    let mut s = state();
    s.chat.history = vec![msg("assistant", "I will run ffmpeg next")];
    let fx = s.update(Msg::ContinueAgent, t0());
    let (_, _, history, _) = run_step_of(&fx).unwrap();
    let nudge = history.last().unwrap();
    assert!(nudge.synthetic);
    assert!(nudge.content.contains(cdout_core::constants::TOOL_NAME));
}

// --- sessions ---

#[test]
fn history_changes_are_saved_after_the_debounce() {
    let (mut s, now) = mid_conversation();
    let due = s.sessions.save_due.unwrap();
    assert_eq!(due, ms(now, 500));
    assert!(!has(&s.update(Msg::Tick, ms(now, 499)), |e| matches!(
        e,
        Effect::SaveSession { .. }
    )));
    let fx = s.update(Msg::Tick, ms(now, 500));
    assert!(has(
        &fx,
        |e| matches!(e, Effect::SaveSession { id, messages } if id == "s1" && messages.len() == 3)
    ));
    assert!(s.sessions.save_due.is_none());
}

#[test]
fn nothing_is_saved_without_a_session() {
    let mut s = state();
    s.chat.history = vec![msg("user", "x")];
    s.history_changed(t0());
    assert!(s.sessions.save_due.is_none());
}

#[test]
fn selecting_a_session_flushes_resets_and_loads() {
    let (mut s, now) = mid_conversation();
    let fx = s.update(Msg::SelectSession("other".into()), now);
    assert!(matches!(fx[0], Effect::SaveSession { .. }));
    assert!(has(&fx, |e| *e == Effect::LoadSession("other".into())));
    assert!(!s.chat.processing);

    let fx = s.update(
        Msg::SessionLoaded(Ok(session("other", vec![msg("user", "old q")]))),
        now,
    );
    assert_eq!(s.sessions.current.as_deref(), Some("other"));
    assert_eq!(s.chat.history.len(), 1);
    assert_eq!(s.models.selected, "ollama:saved");
    assert!(has(&fx, |e| *e == Effect::PersistModel("ollama:saved".into())));
    assert_eq!(s.explorer.as_ref().unwrap().path, "C:/saved");
    assert!(s.sessions.save_due.is_none(), "viewing does not re-save");
    assert!(s.update(Msg::SelectSession("other".into()), now).is_empty());
}

#[test]
fn deleting_the_current_session_clears_the_view() {
    let (mut s, now) = mid_conversation();
    s.update(Msg::DeleteSession("s1".into()), now);
    let fx = s.update(
        Msg::SessionDeleted {
            id: "s1".into(),
            result: Ok(()),
        },
        now,
    );
    assert!(s.chat.history.is_empty());
    assert!(s.sessions.current.is_none());
    assert!(s.explorer.is_none());
    assert!(has(&fx, |e| *e == Effect::ListSessions));
}

#[test]
fn rename_trims_and_ignores_blank_titles() {
    let mut s = state();
    assert_eq!(
        s.update(
            Msg::RenameSession {
                id: "a".into(),
                title: "  New  ".into()
            },
            t0()
        ),
        vec![Effect::RenameSession {
            id: "a".into(),
            title: "New".into()
        }]
    );
    assert!(s
        .update(
            Msg::RenameSession {
                id: "a".into(),
                title: "   ".into()
            },
            t0()
        )
        .is_empty());
}

#[test]
fn new_chat_clears_everything() {
    let (mut s, now) = mid_conversation();
    s.chat.queued.push_back("q".into());
    let fx = s.update(Msg::NewChat, now);
    assert!(matches!(fx[0], Effect::SaveSession { .. }));
    assert!(s.chat.history.is_empty());
    assert!(s.chat.queued.is_empty());
    assert!(s.sessions.current.is_none());
    assert!(s.main.focus_input);
}

// --- models ---

#[test]
fn replacement_prefers_cloud_families() {
    let v = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(
        pick_replacement(&v(&["ollama:a", "google/g", "anthropic:x", "anthropic/y"])),
        Some("anthropic/y".into())
    );
    assert_eq!(
        pick_replacement(&v(&["ollama:a", "openai/o"])),
        Some("openai/o".into())
    );
    assert_eq!(
        pick_replacement(&v(&["zzz", "ollama:a"])),
        Some("ollama:a".into())
    );
    assert_eq!(pick_replacement(&v(&["zzz"])), Some("zzz".into()));
    assert_eq!(pick_replacement(&[]), None);
}

#[test]
fn stale_selection_is_swapped_with_a_toast() {
    let mut s = state();
    s.models.selected = "claude:old".into();
    let fx = s.update(
        Msg::ModelsLoaded(Ok(vec!["ollama:a".into(), "anthropic/x".into()])),
        t0(),
    );
    assert_eq!(s.models.selected, "anthropic/x");
    assert!(has(&fx, |e| *e == Effect::PersistModel("anthropic/x".into())));
    assert!(s
        .toast
        .as_ref()
        .unwrap()
        .message
        .contains("'claude:old' is no longer available"));
}

#[test]
fn auto_pick_before_hydration_is_not_persisted() {
    let mut s = AppState::new(platform_info(None));
    let fx = s.update(Msg::ModelsLoaded(Ok(vec!["ollama:a".into()])), t0());
    assert_eq!(s.models.selected, "ollama:a");
    assert!(!has(&fx, |e| matches!(e, Effect::PersistModel(_))));
    let fx = s.update(Msg::SelectedModelLoaded(None), t0());
    assert_eq!(fx, vec![Effect::PersistModel("ollama:a".into())]);

    let mut s = AppState::new(platform_info(None));
    s.update(Msg::ModelsLoaded(Ok(vec!["ollama:a".into()])), t0());
    let fx = s.update(Msg::SelectedModelLoaded(Some("anthropic/x".into())), t0());
    assert!(fx.is_empty());
    assert_eq!(s.models.selected, "anthropic/x", "saved choice wins");
}

#[test]
fn model_fetches_do_not_overlap() {
    let mut s = AppState::new(platform_info(None));
    assert_eq!(
        s.update(Msg::RefreshModels, t0()),
        vec![Effect::FetchModels]
    );
    assert!(s.update(Msg::RefreshModels, t0()).is_empty());
    s.update(Msg::ModelsLoaded(Ok(vec![])), t0());
    assert_eq!(
        s.update(Msg::RefreshModels, t0()),
        vec![Effect::FetchModels]
    );
}

#[test]
fn polling_runs_only_while_nothing_is_usable() {
    let now = t0();
    let mut s = AppState::new(platform_info(None));
    s.update(Msg::ModelsLoaded(Ok(vec![])), now);
    assert_eq!(s.models.next_poll, Some(ms(now, 5000)));
    let fx = s.update(Msg::Tick, ms(now, 5000));
    assert!(has(&fx, |e| *e == Effect::FetchModels));
    assert!(has(&fx, |e| *e == Effect::FetchProviderStatus));

    let later = ms(now, 6000);
    s.update(Msg::ModelsLoaded(Ok(vec!["ollama:a".into()])), later);
    assert_eq!(s.models.next_poll, None);
}

// --- settings ---

#[test]
fn settings_save_sends_only_entered_keys() {
    let mut s = state();
    s.update(Msg::OpenSettings, t0());
    s.settings.draft_ollama_url = "http://box:11434".into();
    s.settings.openrouter_draft = " sk-or-1 ".into();
    let fx = s.update(Msg::SaveSettings, t0());
    assert_eq!(
        fx,
        vec![Effect::SaveSettings {
            ollama_url: "http://box:11434".into(),
            openrouter_key: Some("sk-or-1".into()),
            anthropic_key: None
        }]
    );
    let keys = cdout_core::api::MaskedApiKeys {
        openrouter_set: true,
        openrouter_preview: Some("…or-1".into()),
        ..Default::default()
    };
    s.update(Msg::SettingsSaved(Ok(keys)), t0());
    assert!(!s.main.settings_open);
    assert!(
        s.settings.openrouter_draft.is_empty(),
        "plaintext not retained"
    );
    assert_eq!(s.settings.ollama_url, "http://box:11434");
}

#[test]
fn approval_mode_applies_immediately() {
    let mut s = state();
    let fx = s.update(Msg::SetApprovalMode(ApprovalMode::Auto), t0());
    assert_eq!(s.settings.approval_mode, ApprovalMode::Auto);
    assert_eq!(fx, vec![Effect::PersistApprovalMode(ApprovalMode::Auto)]);
}

#[test]
fn hotkey_change_only_sticks_once_applied() {
    let mut s = state();
    s.settings.hotkey = "Ctrl+Alt+A".into();
    let fx = s.update(Msg::ChangeHotkey("Ctrl+Shift+K".into()), t0());
    assert_eq!(
        fx,
        vec![Effect::ApplyHotkey {
            chord: "Ctrl+Shift+K".into(),
            persist: true
        }]
    );
    s.update(
        Msg::HotkeyApplied {
            chord: "Ctrl+Shift+K".into(),
            result: Err("taken".into()),
        },
        t0(),
    );
    assert_eq!(s.settings.hotkey, "Ctrl+Alt+A");
    assert_eq!(s.settings.hotkey_error.as_deref(), Some("taken"));
    s.update(
        Msg::HotkeyApplied {
            chord: "Ctrl+Shift+K".into(),
            result: Ok(()),
        },
        t0(),
    );
    assert_eq!(s.settings.hotkey, "Ctrl+Shift+K");
    assert!(s.settings.hotkey_error.is_none());
}

#[test]
fn banner_dismissal_is_persisted() {
    let mut s = state();
    s.main.migration_banner = true;
    let fx = s.update(Msg::OpenSettingsFromBanner, t0());
    assert!(!s.main.migration_banner);
    assert!(s.main.settings_open);
    assert_eq!(fx[0], Effect::SetDisclosureAck);
}

// --- chrome ---

#[test]
fn toasts_expire() {
    let now = t0();
    let mut s = state();
    s.show_toast("x", now);
    s.update(Msg::Tick, ms(now, 7999));
    assert!(s.toast.is_some());
    s.update(Msg::Tick, ms(now, 8000));
    assert!(s.toast.is_none());
}

#[test]
fn closing_main_hides_it_and_flushes() {
    let (mut s, now) = mid_conversation();
    let fx = s.update(Msg::CloseMain, now);
    assert!(matches!(fx[0], Effect::SaveSession { .. }));
    assert_eq!(fx[1], Effect::HideMain);
    assert!(!s.main.visible);
}

#[test]
fn quit_flushes_before_exiting() {
    let (mut s, now) = mid_conversation();
    let fx = s.update(Msg::TrayQuit, now);
    assert!(matches!(fx[0], Effect::SaveSession { .. }));
    assert_eq!(fx[1], Effect::Quit);
}

#[test]
fn dock_activation_reopens_only_when_nothing_is_visible() {
    let mut s = state();
    s.main.visible = false;
    assert_eq!(
        s.update(Msg::AppActivated, t0()),
        vec![Effect::ShowMain {
            center_on_cursor: false
        }]
    );
    assert!(s.main.visible);
    assert!(
        s.update(Msg::AppActivated, t0()).is_empty(),
        "already visible"
    );

    let mut s = state();
    s.main.visible = false;
    s.update(Msg::HotkeyPressed, t0());
    assert!(
        s.update(Msg::AppActivated, t0()).is_empty(),
        "activation caused by showing the spotlight"
    );
}

#[test]
fn hotkey_startup_failure_is_visible() {
    let mut s = state();
    s.update(
        Msg::HotkeyStartupFailed("Could not register 'Ctrl+Alt+A'.".into()),
        t0(),
    );
    assert!(s
        .toast
        .as_ref()
        .unwrap()
        .message
        .contains("Settings › General"));
    assert_eq!(
        s.settings.hotkey_error.as_deref(),
        Some("Could not register 'Ctrl+Alt+A'.")
    );
}

#[test]
fn startup_shows_main_and_loads_everything() {
    let mut s = AppState::new(platform_info(None));
    let fx = s.update(Msg::Started, t0());
    for e in [
        Effect::ShowMain {
            center_on_cursor: false,
        },
        Effect::LoadStartup,
        Effect::LoadSelectedModel,
        Effect::FetchModels,
        Effect::FetchProviderStatus,
        Effect::ListSessions,
    ] {
        assert!(fx.contains(&e), "missing {e:?} in {fx:?}");
    }
    assert!(s.main.visible);
}
