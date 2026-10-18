//! Collapsible sessions list (`SessionsSidebar.tsx`).

use eframe::egui::{self, Color32, Key, Margin, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui};

use super::theme::*;
use super::widgets;
use crate::state::{AppState, Msg};

pub const WIDTH: f32 = 248.0;
pub const COLLAPSED_WIDTH: f32 = 44.0;

/// Port of `relativeTime`; both arguments in Unix milliseconds.
pub fn relative_time(ts_ms: i64, now_ms: i64) -> String {
    let s = ((now_ms - ts_ms) / 1000).max(0);
    if s < 60 {
        return "just now".into();
    }
    let m = s / 60;
    if m < 60 {
        return format!("{m}m ago");
    }
    let h = m / 60;
    if h < 24 {
        return format!("{h}h ago");
    }
    let d = h / 24;
    if d < 30 {
        return format!("{d}d ago");
    }
    let months = d / 30;
    if months < 12 {
        return format!("{months}mo ago");
    }
    format!("{}y ago", months / 12)
}

/// Last path component, accepting both separators.
pub fn folder_name(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .find(|s| !s.is_empty())
        .unwrap_or(path)
}

pub fn show(ui: &mut Ui, state: &mut AppState, now_ms: i64, out: &mut Vec<Msg>) {
    if state.main.sidebar_collapsed {
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            if widgets::icon_button(ui, "⏵", "Expand sidebar").clicked() {
                out.push(Msg::ToggleSidebar);
            }
            if widgets::icon_button(ui, "✚", "New chat").clicked() {
                out.push(Msg::NewChat);
            }
        });
        return;
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui
            .add(widgets::secondary_button("✚ New chat").min_size(egui::vec2(WIDTH - 60.0, 28.0)))
            .on_hover_text("Start a new chat")
            .clicked()
        {
            out.push(Msg::NewChat);
        }
        if widgets::icon_button(ui, "⏴", "Collapse sidebar").clicked() {
            out.push(Msg::ToggleSidebar);
        }
    });
    ui.add_space(6.0);

    if state.sessions.list.is_empty() {
        ui.label(
            RichText::new("No sessions yet. Trigger the spotlight hotkey to start one.")
                .size(11.0)
                .color(GRAY_600),
        );
        return;
    }

    let current = state.sessions.current.clone();
    let list = state.sessions.list.clone();
    ScrollArea::vertical()
        .id_salt("sessions")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for s in &list {
                ui.push_id(&s.id, |ui| {
                    row(
                        ui,
                        state,
                        s,
                        current.as_deref() == Some(s.id.as_str()),
                        now_ms,
                        out,
                    );
                });
            }
        });
}

fn row(
    ui: &mut Ui,
    state: &mut AppState,
    s: &cdout_core::features::sessions::SessionMeta,
    active: bool,
    now_ms: i64,
    out: &mut Vec<Msg>,
) {
    let (fill, stroke, title_color) = if active {
        (
            INDIGO_600.gamma_multiply(0.2),
            INDIGO_500.gamma_multiply(0.4),
            Color32::from_rgb(0xe0, 0xe7, 0xff),
        )
    } else {
        (Color32::TRANSPARENT, Color32::TRANSPARENT, GRAY_300)
    };
    let renaming = state
        .sessions
        .renaming
        .as_ref()
        .is_some_and(|(id, _)| id == &s.id);
    let confirming = state.sessions.confirm_delete.as_deref() == Some(s.id.as_str());

    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(RADIUS)
        .inner_margin(Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if renaming {
                let draft = &mut state.sessions.renaming.as_mut().unwrap().1;
                let r = ui.add(TextEdit::singleline(draft).desired_width(f32::INFINITY));
                r.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Session title")
                });
                r.request_focus();
                let (enter, esc) =
                    ui.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
                if esc {
                    state.sessions.renaming = None;
                } else if enter || r.lost_focus() {
                    out.push(Msg::RenameSession {
                        id: s.id.clone(),
                        title: draft.clone(),
                    });
                }
                return;
            }
            ui.horizontal(|ui| {
                let title_col = ui.vertical(|ui| {
                    ui.set_max_width(ui.available_width() - 40.0);
                    ui.add(
                        egui::Label::new(RichText::new(&s.title).size(12.0).color(title_color))
                            .truncate(),
                    )
                    .on_hover_text(&s.title);
                    let files = if s.file_count == 1 { "file" } else { "files" };
                    ui.label(
                        RichText::new(format!(
                            "🗁 {} · {}",
                            folder_name(&s.explorer_path),
                            relative_time(s.last_active_at, now_ms)
                        ))
                        .size(10.0)
                        .color(GRAY_500),
                    )
                    .on_hover_text(format!("{} · {} {files}", s.explorer_path, s.file_count));
                });
                // Only the title column selects the session, so the buttons
                // beside it keep their own clicks.
                let select = ui.interact(
                    title_col.response.rect,
                    ui.id().with("select"),
                    Sense::click(),
                );
                let title = s.title.clone();
                select.widget_info(move || {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        format!("Session: {title}"),
                    )
                });
                if select
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                    && !confirming
                {
                    out.push(Msg::SelectSession(s.id.clone()));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if widgets::icon_button(ui, "🗙", "Delete session").clicked() {
                        state.sessions.confirm_delete = Some(s.id.clone());
                    }
                    if widgets::icon_button(ui, "🖊", "Rename").clicked() {
                        state.sessions.renaming = Some((s.id.clone(), s.title.clone()));
                    }
                });
            });
            if confirming {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Delete this session?")
                            .size(11.0)
                            .color(RED_400),
                    );
                    if ui.add(widgets::danger_button("Delete")).clicked() {
                        out.push(Msg::DeleteSession(s.id.clone()));
                    }
                    if ui.add(widgets::secondary_button("Cancel")).clicked() {
                        state.sessions.confirm_delete = None;
                    }
                });
            }
        });
    ui.add_space(2.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_time_buckets() {
        let now = 10_000_000_000;
        assert_eq!(relative_time(now - 5_000, now), "just now");
        assert_eq!(relative_time(now + 5_000, now), "just now");
        assert_eq!(relative_time(now - 120_000, now), "2m ago");
        assert_eq!(relative_time(now - 3 * 3_600_000, now), "3h ago");
        assert_eq!(relative_time(now - 2 * 86_400_000, now), "2d ago");
        assert_eq!(relative_time(now - 65 * 86_400_000, now), "2mo ago");
        assert_eq!(relative_time(now - 800 * 86_400_000, now), "2y ago");
    }

    #[test]
    fn folder_name_handles_both_separators() {
        assert_eq!(folder_name("C:\\Users\\me\\Videos"), "Videos");
        assert_eq!(folder_name("/Users/me/Movies/"), "Movies");
        assert_eq!(folder_name(""), "");
    }
}
