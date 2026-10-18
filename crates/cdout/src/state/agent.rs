//! The agent loop: starting steps, consuming their results, approvals,
//! auto-execution, stop. Port of `startAgent`, `runAgentStep`,
//! `approveCommand`, `rejectCommand`, `dismissCommand`, `answerQuestion`,
//! `dismissQuestion` and `stopAgent` from `src/App.tsx`.
//!
//! Deliberate differences from the React version (recorded in the plan's
//! execution log):
//! - Stop bumps the run generation instead of setting a `stopRequested`
//!   flag, so a stop pressed while no step is in flight cannot swallow the
//!   next step's result.
//! - Auto-executed command results carry the proposal's `tool_call_id`, like
//!   approved ones (React relied on the provider client's fallback).

use std::time::Instant;

use cdout_core::constants::ASK_USER_QUESTION_TOOL;
use cdout_core::features::agent::{AgentResponse, AgentStepResult, ToolProposal};
use cdout_core::features::loop_detector::LoopVerdict;
use cdout_core::llm::{FunctionCall, Message, ToolCall};
use cdout_core::tools::risk::{self, CommandRisk};
use cdout_core::utils::config::ApprovalMode;

use super::text;
use super::{
    AppState, Effect, ExecCtx, ExecKind, ExplorerPurpose, PendingCommand, PendingQuestion,
    SessionPurpose, StepCtx, AUTO_STEP_DELAY, FILE_LIST_THRESHOLD, MAX_AUTO_STEPS,
    MAX_CONSECUTIVE_FAILURES, SAVE_DEBOUNCE,
};

pub(super) fn user_msg(content: impl Into<String>) -> Message {
    Message {
        role: "user".into(),
        content: content.into(),
        tool_calls: None,
        synthetic: false,
    }
}

pub(super) fn synthetic_msg(content: impl Into<String>) -> Message {
    Message {
        synthetic: true,
        ..user_msg(content)
    }
}

fn tool_result(content: impl Into<String>, tool_call_id: Option<&str>, tool_name: &str) -> Message {
    Message {
        role: "tool".into(),
        content: content.into(),
        tool_calls: tool_call_id.map(|id| {
            vec![ToolCall {
                id: Some(id.to_string()),
                function: FunctionCall {
                    name: tool_name.to_string(),
                    arguments: serde_json::json!({}),
                },
            }]
        }),
        synthetic: false,
    }
}

/// Pick which proposal to surface: `ask_user_question` always wins.
pub fn select_proposal(proposals: &[ToolProposal]) -> Option<&ToolProposal> {
    proposals
        .iter()
        .find(|p| p.tool_name == ASK_USER_QUESTION_TOOL)
        .or_else(|| proposals.first())
}

/// Whether a command may run without a click. A `Dangerous` verdict
/// overrides every policy.
pub fn may_run_unattended(should_auto: bool, mode: ApprovalMode, risk: CommandRisk) -> bool {
    let permitted = should_auto
        || mode == ApprovalMode::Auto
        || (mode == ApprovalMode::ReadOnly && risk == CommandRisk::ReadOnly);
    permitted && risk != CommandRisk::Dangerous
}

impl AppState {
    // --- history bookkeeping ---

    /// Mark the conversation changed so it is saved after the debounce.
    pub(super) fn history_changed(&mut self, now: Instant) {
        if self.sessions.current.is_some() && !self.chat.history.is_empty() {
            self.sessions.save_due = Some(now + SAVE_DEBOUNCE);
        }
    }

    pub(super) fn push_history(&mut self, msg: Message, now: Instant) {
        self.chat.history.push(msg);
        self.history_changed(now);
    }

    pub(super) fn flush_save(&mut self, fx: &mut Vec<Effect>) {
        if self.sessions.save_due.take().is_none() {
            return;
        }
        if let Some(id) = self.sessions.current.clone() {
            if !self.chat.history.is_empty() {
                fx.push(Effect::SaveSession {
                    id,
                    messages: self.chat.history.clone(),
                });
            }
        }
    }

    /// Disown all in-flight work and clear transient conversation UI.
    pub(super) fn hard_reset(&mut self, fx: &mut Vec<Effect>) {
        self.chat.generation += 1;
        fx.push(Effect::CancelStream);
        fx.push(Effect::ResetLoopDetector);
        let c = &mut self.chat;
        c.streaming.clear();
        c.pending_command = None;
        c.pending_question = None;
        c.processing = false;
        c.executing = false;
        c.auto_execute = false;
        c.auto_step_count = 0;
        c.consecutive_failures = 0;
        c.step = None;
        c.exec = None;
        c.auto_step_at = None;
        c.queued.clear();
        c.expanded.clear();
        c.clear_card_ui();
    }

    // --- starting work ---

    pub(super) fn submit_or_queue(&mut self, now: Instant, fx: &mut Vec<Effect>) {
        let text = self.chat.input.trim().to_string();
        if text.is_empty() {
            return;
        }
        let c = &self.chat;
        let busy = c.processing
            || c.executing
            || !c.streaming.is_empty()
            || c.pending_command.is_some()
            || c.pending_question.is_some()
            || c.auto_step_at.is_some();
        if busy {
            self.chat.queued.push_back(text);
            self.chat.input.clear();
        } else {
            self.start_agent(text, true, now, fx);
        }
    }

    /// Port of `startAgent`. `from_input` clears the input box.
    pub(super) fn start_agent(
        &mut self,
        prompt: String,
        from_input: bool,
        now: Instant,
        fx: &mut Vec<Effect>,
    ) {
        let prompt = prompt.trim().to_string();
        if prompt.is_empty() {
            return;
        }
        if from_input {
            self.chat.input.clear();
        }
        let Some(explorer) = self.explorer.clone() else {
            // React silently ignored a prompt sent before the first Sync.
            // Fetch the context instead and start once it arrives.
            self.chat.processing = true;
            fx.push(Effect::FetchExplorer(ExplorerPurpose::MainStart {
                gen: self.chat.generation,
                prompt,
            }));
            return;
        };

        self.chat.processing = true;
        self.chat.auto_step_count = 0;
        self.chat.consecutive_failures = 0;
        self.chat.pending_command = None;
        self.chat.pending_question = None;
        fx.push(Effect::ResetLoopDetector);

        if self.chat.history.is_empty() {
            self.chat.last_synced = Some((explorer.path.clone(), explorer.selected_files.clone()));
            self.chat.sync_pending = false;
            fx.push(Effect::CreateSession {
                purpose: SessionPurpose::MainStart {
                    gen: self.chat.generation,
                },
                prompt,
                path: explorer.path,
                files: explorer.selected_files,
                model: self.models.selected.clone(),
            });
            return;
        }

        if self.chat.sync_pending {
            let update = if explorer.selected_files.len() > FILE_LIST_THRESHOLD {
                match cdout_core::api::write_file_list(&explorer.selected_files) {
                    Ok(list_path) => {
                        let hint = cdout_core::api::platform_info(Some(&list_path)).read_list_hint;
                        text::context_update_file(
                            &explorer.path,
                            explorer.selected_files.len(),
                            &list_path,
                            &hint,
                        )
                    }
                    Err(e) => {
                        self.chat.processing = false;
                        self.chat.input = prompt;
                        self.show_toast(e, now);
                        return;
                    }
                }
            } else {
                text::context_update_inline(&explorer.path, &explorer.selected_files)
            };
            self.push_history(user_msg(update), now);
            self.chat.last_synced = Some((explorer.path.clone(), explorer.selected_files.clone()));
            self.chat.sync_pending = false;
        }

        self.push_history(user_msg(prompt), now);
        let auto = self.chat.auto_execute;
        self.run_step(auto, None, fx);
    }

    /// Port of the first half of `runAgentStep`: kick off a streaming step
    /// over the current history.
    pub(super) fn run_step(
        &mut self,
        should_auto: bool,
        model_override: Option<String>,
        fx: &mut Vec<Effect>,
    ) {
        let model = model_override.unwrap_or_else(|| self.models.selected.clone());
        self.chat.streaming.clear();
        self.chat.processing = true;
        let drop_tools = std::mem::take(&mut self.chat.drop_tools_next);
        self.chat.step = Some(StepCtx {
            should_auto,
            model: model.clone(),
        });
        tracing::info!(target: "cdout", "agent.step.start gen={} auto={should_auto} drop_tools={drop_tools}", self.chat.generation);
        fx.push(Effect::RunStep {
            gen: self.chat.generation,
            model,
            history: self.chat.history.clone(),
            drop_tools,
        });
    }

    // --- consuming results ---

    /// Port of the second half of `runAgentStep`.
    pub(super) fn on_step_done(
        &mut self,
        gen: u64,
        result: Result<AgentStepResult, String>,
        now: Instant,
        fx: &mut Vec<Effect>,
    ) {
        if gen != self.chat.generation {
            return; // a newer run owns the UI
        }
        let Some(ctx) = self.chat.step.take() else {
            return;
        };
        self.chat.streaming.clear();

        let result = match result {
            Ok(r) => r,
            Err(e) => {
                tracing::info!(target: "cdout", "agent.step.error {e}");
                self.chat.processing = false;
                self.show_toast(format!("LLM Error: {e}"), now);
                return;
            }
        };

        self.chat.history = result.updated_history;
        self.history_changed(now);

        let proposals: Vec<ToolProposal> = match &result.response {
            AgentResponse::ToolProposals(p) => p.clone(),
            AgentResponse::CommandProposal(cmd) => vec![ToolProposal {
                tool_name: self.platform.shell_tool_name.to_string(),
                command: cmd.clone(),
                tool_call_id: None,
                question_data: None,
            }],
            AgentResponse::Text(_) => Vec::new(),
        };
        let proposal = select_proposal(&proposals).cloned();

        // Branch 1: a question for the user — never auto-run.
        if let Some(p) = &proposal {
            if p.tool_name == ASK_USER_QUESTION_TOOL {
                if let Some(data) = &p.question_data {
                    tracing::info!(target: "cdout", "agent.step.done kind=question");
                    self.chat.pending_command = None;
                    self.chat.pending_question = Some(PendingQuestion {
                        data: data.clone(),
                        tool_call_id: p.tool_call_id.clone(),
                    });
                    self.chat.clear_card_ui();
                    self.chat.consecutive_failures = 0;
                    if ctx.should_auto {
                        self.chat.auto_execute = false;
                    }
                    self.chat.processing = false;
                    return;
                }
            }
        }

        // Branch 2: a shell command.
        if let Some(p) = proposal.filter(|p| !p.command.is_empty()) {
            tracing::info!(target: "cdout", "agent.step.done kind=command");
            let cmd = p.command.clone();
            self.chat.auto_step_count += 1;
            let risk = risk::classify(&cmd);

            if let Some(LoopVerdict::Block(reason) | LoopVerdict::Break(reason)) =
                &result.loop_verdict
            {
                if ctx.should_auto {
                    self.chat.auto_execute = false;
                }
                self.chat.processing = false;
                if matches!(result.loop_verdict, Some(LoopVerdict::Break(_))) {
                    self.chat.drop_tools_next = true;
                }
                self.push_history(synthetic_msg(text::loop_detected_note(reason)), now);
                self.chat.pending_command = Some(PendingCommand {
                    command: cmd,
                    tool_call_id: p.tool_call_id,
                    risk: Some(risk),
                    loop_reason: Some(reason.clone()),
                });
                self.chat.clear_card_ui();
                return;
            }

            let may_run = may_run_unattended(ctx.should_auto, self.settings.approval_mode, risk);
            if may_run && self.chat.auto_step_count < MAX_AUTO_STEPS {
                self.chat.executing = true;
                self.chat.processing = false;
                self.chat.exec = Some(ExecCtx {
                    kind: ExecKind::Auto,
                    should_auto: ctx.should_auto,
                    model: ctx.model,
                    command: cmd.clone(),
                    tool_call_id: p.tool_call_id,
                    risk: Some(risk),
                });
                tracing::info!(target: "cdout", "exec.start kind=auto");
                fx.push(Effect::Execute {
                    gen: self.chat.generation,
                    command: cmd,
                    cwd: self.cwd(),
                });
            } else {
                if ctx.should_auto {
                    self.chat.auto_execute = false;
                }
                self.chat.pending_command = Some(PendingCommand {
                    command: cmd,
                    tool_call_id: p.tool_call_id,
                    risk: Some(risk),
                    loop_reason: None,
                });
                self.chat.clear_card_ui();
                self.chat.processing = false;
            }
            return;
        }

        // Branch 3: plain text.
        tracing::info!(target: "cdout", "agent.step.done kind=text");
        self.chat.pending_command = None;
        self.chat.processing = false;
        if ctx.should_auto {
            let response_text = match &result.response {
                AgentResponse::Text(t) => t.as_str(),
                _ => "",
            };
            if !text::is_task_complete(response_text) && self.chat.auto_step_count < MAX_AUTO_STEPS
            {
                self.chat.auto_step_count += 1;
                self.chat.auto_step_at = Some(now + AUTO_STEP_DELAY);
                self.chat.step = Some(ctx); // remembered for the continuation
                return;
            }
        }
        self.chat.auto_execute = false;
    }

    /// The delayed nudge after an unattended text reply that isn't done yet.
    pub(super) fn run_auto_continuation(&mut self, now: Instant, fx: &mut Vec<Effect>) {
        self.chat.auto_step_at = None;
        let Some(ctx) = self.chat.step.take() else {
            return;
        };
        self.push_history(synthetic_msg(text::AUTO_CONTINUE_NUDGE), now);
        self.run_step(true, Some(ctx.model), fx);
    }

    pub(super) fn on_exec_done(
        &mut self,
        gen: u64,
        result: Result<String, String>,
        now: Instant,
        fx: &mut Vec<Effect>,
    ) {
        if gen != self.chat.generation {
            return;
        }
        let Some(ctx) = self.chat.exec.take() else {
            return;
        };
        self.chat.executing = false;
        tracing::info!(target: "cdout", "exec.done ok={}", result.is_ok());
        let tool_name = self.platform.shell_tool_name;

        match (ctx.kind, result) {
            (ExecKind::Auto, Ok(output)) => {
                if text::output_reports_failure(&output) {
                    self.chat.consecutive_failures += 1;
                } else {
                    self.chat.consecutive_failures = 0;
                }
                self.push_history(
                    tool_result(output, ctx.tool_call_id.as_deref(), tool_name),
                    now,
                );
                if self.chat.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    self.chat.consecutive_failures = 0;
                    self.chat.auto_execute = false;
                    self.chat.processing = false;
                    self.push_history(
                        synthetic_msg(text::auto_paused_note(MAX_CONSECUTIVE_FAILURES)),
                        now,
                    );
                    return;
                }
                self.run_step(ctx.should_auto, Some(ctx.model), fx);
            }
            (ExecKind::Auto, Err(e)) => {
                tracing::warn!(target: "cdout", "exec.error {e}");
                self.chat.auto_execute = false;
                self.chat.pending_command = Some(PendingCommand {
                    command: ctx.command,
                    tool_call_id: ctx.tool_call_id,
                    risk: ctx.risk,
                    loop_reason: None,
                });
            }
            (ExecKind::Approved, Ok(output)) => {
                self.push_history(
                    tool_result(output, ctx.tool_call_id.as_deref(), tool_name),
                    now,
                );
                self.chat.pending_command = None;
                self.run_step(ctx.should_auto, None, fx);
            }
            (ExecKind::Approved, Err(e)) => {
                self.chat.processing = false;
                self.show_toast(format!("Command execution failed: {e}"), now);
            }
        }
    }

    fn cwd(&self) -> Option<String> {
        self.explorer
            .as_ref()
            .map(|e| e.path.clone())
            .filter(|p| !p.is_empty())
    }

    // --- user decisions ---

    pub(super) fn approve_command(&mut self, continue_auto: bool, fx: &mut Vec<Effect>) {
        if self.chat.executing {
            return;
        }
        let Some(pending) = self.chat.pending_command.clone() else {
            return;
        };
        let should_auto = continue_auto || self.chat.auto_execute;
        if continue_auto {
            self.chat.auto_execute = true;
        }
        self.chat.executing = true;
        self.chat.exec = Some(ExecCtx {
            kind: ExecKind::Approved,
            should_auto,
            model: self.models.selected.clone(),
            command: pending.command.clone(),
            tool_call_id: pending.tool_call_id,
            risk: pending.risk,
        });
        tracing::info!(target: "cdout", "exec.start kind=approved");
        fx.push(Effect::Execute {
            gen: self.chat.generation,
            command: pending.command,
            cwd: self.cwd(),
        });
    }

    pub(super) fn reject_command(&mut self, now: Instant, fx: &mut Vec<Effect>) {
        let Some(pending) = self.chat.pending_command.take() else {
            return;
        };
        let feedback = std::mem::take(&mut self.chat.reject_feedback);
        self.chat.clear_card_ui();
        let tool_name = self.platform.shell_tool_name;
        self.push_history(
            tool_result(
                text::command_rejected(&feedback),
                pending.tool_call_id.as_deref(),
                tool_name,
            ),
            now,
        );
        let auto = self.chat.auto_execute;
        self.run_step(auto, None, fx);
    }

    pub(super) fn dismiss_command(&mut self, now: Instant) {
        let Some(pending) = self.chat.pending_command.take() else {
            return;
        };
        if let Some(id) = pending.tool_call_id.as_deref() {
            let tool_name = self.platform.shell_tool_name;
            self.push_history(
                tool_result(text::COMMAND_DISMISSED, Some(id), tool_name),
                now,
            );
        }
        self.chat.processing = false;
    }

    pub(super) fn answer_question(&mut self, answer: String, now: Instant, fx: &mut Vec<Effect>) {
        let Some(q) = self.chat.pending_question.take() else {
            return;
        };
        self.chat.clear_card_ui();
        self.push_history(
            tool_result(answer, q.tool_call_id.as_deref(), ASK_USER_QUESTION_TOOL),
            now,
        );
        let auto = self.chat.auto_execute;
        self.run_step(auto, None, fx);
    }

    pub(super) fn dismiss_question(&mut self, now: Instant) {
        let Some(q) = self.chat.pending_question.take() else {
            return;
        };
        self.chat.clear_card_ui();
        self.push_history(
            tool_result(
                text::QUESTION_DISMISSED,
                q.tool_call_id.as_deref(),
                ASK_USER_QUESTION_TOOL,
            ),
            now,
        );
        self.chat.processing = false;
    }

    pub(super) fn stop_agent(&mut self, now: Instant, fx: &mut Vec<Effect>) {
        self.chat.generation += 1;
        self.chat.auto_execute = false;
        fx.push(Effect::CancelStream);
        let partial = std::mem::take(&mut self.chat.streaming);
        if !partial.trim().is_empty() {
            self.push_history(
                Message {
                    role: "assistant".into(),
                    content: partial,
                    tool_calls: None,
                    synthetic: false,
                },
                now,
            );
        }
        self.push_history(synthetic_msg(text::INTERRUPTED_NOTE), now);
        let c = &mut self.chat;
        c.processing = false;
        c.executing = false;
        c.pending_command = None;
        c.pending_question = None;
        c.step = None;
        c.exec = None;
        c.auto_step_at = None;
        fx.push(Effect::ResetLoopDetector);
        fx.push(Effect::QueryRunningCommand);
    }

    pub(super) fn continue_agent(&mut self, now: Instant, fx: &mut Vec<Effect>) {
        if !self.should_show_continue() {
            return;
        }
        let nudge = text::continue_nudge(self.platform.shell_tool_name, self.platform.shell_name);
        self.push_history(synthetic_msg(nudge), now);
        let auto = self.chat.auto_execute;
        self.run_step(auto, None, fx);
    }

    /// Port of `shouldShowContinue`: the model stopped on prose that is
    /// neither a completion nor a question.
    pub fn should_show_continue(&self) -> bool {
        let c = &self.chat;
        if c.processing
            || c.executing
            || !c.streaming.is_empty()
            || c.pending_command.is_some()
            || c.pending_question.is_some()
        {
            return false;
        }
        let Some(last) = c.history.last() else {
            return false;
        };
        if last.role != "assistant" {
            return false;
        }
        if last.tool_calls.as_ref().is_some_and(|t| !t.is_empty()) {
            return false;
        }
        if text::is_task_complete(&last.content) {
            return false;
        }
        !last.content.trim().ends_with('?')
    }
}
