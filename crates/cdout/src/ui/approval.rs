//! The command approval card (`CommandApproval.tsx`) and the question card
//! (`QuestionApproval.tsx`).

use std::collections::BTreeSet;

use cdout_core::features::agent::QuestionData;
use cdout_core::tools::risk::CommandRisk;
use eframe::egui::{self, Color32, Margin, RichText, Stroke, TextEdit, Ui};

use super::theme::*;
use super::widgets;
use crate::state::{AppState, Msg};

pub fn command_card(ui: &mut Ui, state: &mut AppState, out: &mut Vec<Msg>) {
    let executing = state.chat.executing;
    let processing = state.chat.processing;
    let c = &mut state.chat;
    let Some(pending) = c.pending_command.as_mut() else {
        return;
    };
    let dangerous = pending.risk == Some(CommandRisk::Dangerous);
    let (accent, header_fg) = if dangerous {
        (RED_500, Color32::from_rgb(0xfc, 0xa5, 0xa5))
    } else {
        (EMERALD_500, Color32::from_rgb(0x34, 0xd3, 0x99))
    };

    egui::Frame::new()
        .fill(BLACK_40)
        .stroke(Stroke::new(1.0, accent.gamma_multiply(0.4)))
        .corner_radius(RADIUS)
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(">_ COMMAND PROPOSAL (EDITABLE)")
                        .size(10.0)
                        .strong()
                        .color(header_fg),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(risk) = pending.risk {
                        risk_badge(ui, risk);
                    }
                });
            });
            if let Some(reason) = &pending.loop_reason {
                ui.label(
                    RichText::new(format!("Loop detected: {reason}"))
                        .size(11.0)
                        .color(AMBER_400),
                );
            }
            let editor = TextEdit::multiline(&mut pending.command)
                .code_editor()
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .interactive(!executing);
            ui.add(editor).widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Command")
            });

            let busy = processing || executing;
            ui.horizontal(|ui| {
                let run_label = if executing {
                    "Running..."
                } else {
                    "⏵ Execute"
                };
                let run = ui.add_enabled(
                    !busy && !c.reject_open,
                    egui::Button::new(RichText::new(run_label).color(Color32::WHITE).strong())
                        .fill(EMERALD_600)
                        .min_size(egui::vec2(160.0, 26.0)),
                );
                if run.clicked() {
                    out.push(Msg::ApproveCommand {
                        continue_auto: false,
                    });
                }
                if ui
                    .add_enabled(!busy && !c.reject_open, widgets::primary_button("⏵⏵ All"))
                    .on_hover_text("Execute this and all following commands automatically")
                    .clicked()
                {
                    out.push(Msg::ApproveCommand {
                        continue_auto: true,
                    });
                }
                let reject_label = if c.reject_open {
                    "Send Feedback"
                } else {
                    "Reject with Feedback"
                };
                let reject_btn = if c.reject_open {
                    widgets::warn_button(reject_label)
                } else {
                    egui::Button::new(RichText::new(reject_label).color(AMBER_500))
                        .fill(AMBER_950)
                        .stroke(Stroke::new(1.0, AMBER_600.gamma_multiply(0.4)))
                };
                if ui.add_enabled(!busy, reject_btn).clicked() {
                    if c.reject_open && !c.reject_feedback.trim().is_empty() {
                        out.push(Msg::RejectCommand);
                    } else {
                        c.reject_open = !c.reject_open;
                    }
                }
                if ui
                    .add_enabled(!busy, widgets::secondary_button("🗙 Dismiss"))
                    .clicked()
                {
                    out.push(Msg::DismissCommand);
                }
            });
            if c.reject_open {
                ui.separator();
                let fb = TextEdit::multiline(&mut c.reject_feedback)
                    .hint_text("Tell the agent what to fix...")
                    .desired_rows(2)
                    .desired_width(f32::INFINITY);
                let r = ui.add(fb);
                r.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Feedback")
                });
            }
        });
}

pub fn risk_badge(ui: &mut Ui, risk: CommandRisk) {
    let (text, color) = match risk {
        CommandRisk::Dangerous => ("DESTRUCTIVE — READ IT", Color32::from_rgb(0xfc, 0xa5, 0xa5)),
        CommandRisk::Mutating => ("🖊 WRITES FILES", AMBER_400),
        CommandRisk::ReadOnly => ("READ-ONLY", GRAY_500),
    };
    ui.label(RichText::new(text).size(10.0).strong().color(color));
}

/// The answer a question card submits, or `None` when nothing is chosen.
pub fn compose_answer(
    data: &QuestionData,
    picks: &BTreeSet<usize>,
    custom_open: bool,
    custom: &str,
) -> Option<String> {
    if custom_open {
        let t = custom.trim();
        return (!t.is_empty()).then(|| t.to_string());
    }
    let labels: Vec<&str> = picks
        .iter()
        .filter_map(|&i| data.options.get(i).map(|o| o.label.as_str()))
        .collect();
    (!labels.is_empty()).then(|| labels.join(", "))
}

/// Toggle option `idx`; single-select questions keep at most one pick.
pub fn toggle_pick(picks: &mut BTreeSet<usize>, idx: usize, multi: bool) {
    let was = picks.contains(&idx);
    if !multi {
        picks.clear();
    }
    if was {
        picks.remove(&idx);
    } else {
        picks.insert(idx);
    }
}

pub fn question_card(ui: &mut Ui, state: &mut AppState, out: &mut Vec<Msg>) {
    let disabled = state.chat.processing;
    let c = &mut state.chat;
    let Some(q) = c.pending_question.as_ref() else {
        return;
    };
    let data = q.data.clone();
    egui::Frame::new()
        .fill(BLACK_40)
        .stroke(Stroke::new(1.0, INDIGO_500.gamma_multiply(0.3)))
        .corner_radius(RADIUS)
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let header = data
                .header
                .clone()
                .unwrap_or_else(|| "Quick question".into());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("? {}", header.to_uppercase()))
                        .size(10.0)
                        .strong()
                        .color(INDIGO_300),
                );
                if data.multi_select {
                    ui.label(
                        RichText::new("(pick one or more)")
                            .size(10.0)
                            .color(INDIGO_300),
                    );
                }
            });
            ui.label(RichText::new(&data.question).color(GRAY_200));
            ui.add_space(4.0);
            for (i, opt) in data.options.iter().enumerate() {
                let picked = c.question_picks.contains(&i);
                let mark = match (data.multi_select, picked) {
                    (true, true) => "■",
                    (true, false) => "☐",
                    (false, true) => "◎",
                    (false, false) => "○",
                };
                let mut text = format!("{mark}  {}", opt.label);
                if let Some(d) = &opt.description {
                    text.push_str(&format!("\n     {d}"));
                }
                let fill = if picked {
                    INDIGO_600.gamma_multiply(0.25)
                } else {
                    GRAY_900
                };
                let r = ui.add_enabled(
                    !disabled && !c.question_custom_open,
                    egui::Button::new(RichText::new(text).color(GRAY_200))
                        .fill(fill)
                        .stroke(Stroke::new(1.0, if picked { INDIGO_500 } else { GRAY_700 }))
                        .min_size(egui::vec2(ui.available_width(), 30.0)),
                );
                let label = opt.label.clone();
                r.widget_info(move || {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::RadioButton,
                        true,
                        picked,
                        label.clone(),
                    )
                });
                if r.clicked() {
                    toggle_pick(&mut c.question_picks, i, data.multi_select);
                }
            }
            let other = ui.add_enabled(
                !disabled,
                egui::Button::new(RichText::new("Other (type your own answer)").color(
                    if c.question_custom_open {
                        AMBER_300
                    } else {
                        GRAY_500
                    },
                ))
                .fill(if c.question_custom_open {
                    AMBER_950
                } else {
                    GRAY_900
                })
                .min_size(egui::vec2(ui.available_width(), 28.0)),
            );
            if other.clicked() {
                c.question_custom_open = !c.question_custom_open;
                if c.question_custom_open {
                    c.question_picks.clear();
                }
            }
            if c.question_custom_open {
                ui.add(
                    TextEdit::multiline(&mut c.question_answer)
                        .hint_text("Type your answer...")
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                )
                .widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Your answer")
                });
            }
            let answer = compose_answer(
                &data,
                &c.question_picks,
                c.question_custom_open,
                &c.question_answer,
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !disabled && answer.is_some(),
                        widgets::primary_button("Answer"),
                    )
                    .clicked()
                {
                    if let Some(a) = answer.clone() {
                        out.push(Msg::AnswerQuestion(a));
                    }
                }
                if ui
                    .add_enabled(!disabled, widgets::secondary_button("🗙 Dismiss"))
                    .clicked()
                {
                    out.push(Msg::DismissQuestion);
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use cdout_core::features::agent::QuestionOption;

    fn q(multi: bool) -> QuestionData {
        QuestionData {
            question: "Which?".into(),
            header: None,
            options: ["mp4", "mov", "webm"]
                .iter()
                .map(|l| QuestionOption {
                    label: l.to_string(),
                    description: None,
                })
                .collect(),
            multi_select: multi,
        }
    }

    #[test]
    fn single_select_keeps_one_pick() {
        let mut p = BTreeSet::new();
        toggle_pick(&mut p, 0, false);
        toggle_pick(&mut p, 2, false);
        assert_eq!(p.iter().copied().collect::<Vec<_>>(), vec![2]);
        toggle_pick(&mut p, 2, false);
        assert!(p.is_empty());
    }

    #[test]
    fn multi_select_joins_labels_in_option_order() {
        let mut p = BTreeSet::new();
        toggle_pick(&mut p, 2, true);
        toggle_pick(&mut p, 0, true);
        assert_eq!(
            compose_answer(&q(true), &p, false, ""),
            Some("mp4, webm".into())
        );
    }

    #[test]
    fn custom_answer_wins_and_must_be_non_empty() {
        let p: BTreeSet<usize> = [0].into_iter().collect();
        assert_eq!(compose_answer(&q(false), &p, true, "  "), None);
        assert_eq!(
            compose_answer(&q(false), &p, true, " gif "),
            Some("gif".into())
        );
        assert_eq!(compose_answer(&q(false), &BTreeSet::new(), false, ""), None);
    }
}
