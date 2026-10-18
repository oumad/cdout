//! Custom title bar for the undecorated main window (`TitleBar.tsx`).
//! Dragging the empty area moves the window; double-click toggles maximise;
//! close hides to the tray.

use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Ui, ViewportCommand};

use super::theme::*;
use super::widgets;
use crate::state::Msg;

pub const HEIGHT: f32 = 32.0;

pub fn show(ui: &mut Ui, mac: bool, out: &mut Vec<Msg>) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), HEIGHT), Sense::hover());
    // The drag area is the whole bar; buttons drawn on top take precedence.
    let drag = ui.interact(rect, ui.id().with("title-drag"), Sense::click_and_drag());
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if drag.double_clicked() {
        out.push(Msg::ToggleMaximizeMain);
    }
    ui.painter().rect_filled(rect, 0.0, GRAY_950);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0, GRAY_800),
    );

    let mut bar = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(10.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    if mac {
        mac_dots(&mut bar, out);
        bar.add_space(8.0);
    }
    bar.label(RichText::new(">_").size(12.0).color(INDIGO_500));
    bar.label(RichText::new("cdout").size(12.0).strong().color(GRAY_400));
    if !mac {
        bar.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::icon_button(ui, "🗙", "Close").clicked() {
                out.push(Msg::CloseMain);
            }
            if widgets::icon_button(ui, "🗖", "Maximize").clicked() {
                out.push(Msg::ToggleMaximizeMain);
            }
            if widgets::icon_button(ui, "🗕", "Minimize").clicked() {
                out.push(Msg::MinimizeMain);
            }
        });
    }
}

fn mac_dots(ui: &mut Ui, out: &mut Vec<Msg>) {
    let dot = |ui: &mut Ui, color: Color32, label: &str| {
        let (rect, r) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::click());
        ui.painter().circle_filled(rect.center(), 6.0, color);
        let label = label.to_string();
        r.widget_info(move || {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone())
        });
        r
    };
    if dot(ui, Color32::from_rgb(0xff, 0x5f, 0x57), "Close").clicked() {
        out.push(Msg::CloseMain);
    }
    if dot(ui, Color32::from_rgb(0xfe, 0xbc, 0x2e), "Minimize").clicked() {
        out.push(Msg::MinimizeMain);
    }
    if dot(ui, Color32::from_rgb(0x28, 0xc8, 0x40), "Zoom").clicked() {
        out.push(Msg::ToggleMaximizeMain);
    }
}
