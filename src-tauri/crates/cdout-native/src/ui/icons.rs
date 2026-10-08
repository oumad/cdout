//! Icons. Glyph icons use only characters that egui's bundled fonts cover
//! (enforced by `tests/glyph_coverage.rs`, which scans every UI source file),
//! so chrome renders identically on every OS whatever fonts are installed.
//! The few icons with no covered glyph are drawn with the painter.

use eframe::egui::{self, pos2, vec2, Color32, Painter, Rect, Response, Sense, Stroke, Ui};

pub const FOLDER: &str = "🗁";
pub const FILE: &str = "🗋";
pub const SETTINGS: &str = "⚙";
pub const CLOSE: &str = "🗙";
pub const MINIMIZE: &str = "🗕";
pub const MAXIMIZE: &str = "🗖";
pub const REFRESH: &str = "⟳";
pub const NEW: &str = "✚";
pub const EXPAND: &str = "⏵";
pub const COLLAPSE: &str = "⏴";
pub const MORE: &str = "⏷";
pub const LESS: &str = "⏶";
pub const STOP: &str = "■";
pub const PLAY: &str = "⏵";
pub const SEND: &str = "⏶";
pub const EDIT: &str = "🖊";
pub const TRASH: &str = "🗑";
pub const EXTERNAL: &str = "⬈";
pub const KEY: &str = "🗝";
pub const SHIELD: &str = "🛡";
pub const STAR: &str = "★";
pub const TOOLS: &str = "🛠";
pub const KEYBOARD: &str = "⌨";
pub const COMPUTER: &str = "🖥";
pub const NETWORK: &str = "🖧";
pub const RADIO_ON: &str = "◎";
pub const RADIO_OFF: &str = "○";
pub const BOX_ON: &str = "■";
pub const BOX_OFF: &str = "☐";

/// Draw the agent's robot head in `rect`.
pub fn paint_bot(p: &Painter, rect: Rect, color: Color32) {
    let s = rect.width().min(rect.height());
    let c = rect.center();
    let head = Rect::from_center_size(c + vec2(0.0, s * 0.08), vec2(s * 0.72, s * 0.56));
    let stroke = Stroke::new((s * 0.08).max(1.0), color);
    p.rect_stroke(head, s * 0.12, stroke, egui::StrokeKind::Middle);
    let eye_y = head.center().y - s * 0.02;
    for dx in [-0.16, 0.16] {
        p.circle_filled(pos2(c.x + s * dx, eye_y), s * 0.07, color);
    }
    let top = head.top();
    p.line_segment([pos2(c.x, top), pos2(c.x, top - s * 0.14)], stroke);
    p.circle_filled(pos2(c.x, top - s * 0.17), s * 0.06, color);
}

/// Draw a head-and-shoulders user glyph in `rect`.
pub fn paint_user(p: &Painter, rect: Rect, color: Color32) {
    let s = rect.width().min(rect.height());
    let c = rect.center();
    p.circle_filled(pos2(c.x, c.y - s * 0.14), s * 0.18, color);
    let body = Rect::from_center_size(pos2(c.x, c.y + s * 0.26), vec2(s * 0.6, s * 0.3));
    p.rect_filled(
        body,
        egui::CornerRadius {
            nw: 200,
            ne: 200,
            sw: 0,
            se: 0,
        },
        color,
    );
}

/// Draw a check mark in `rect`.
pub fn paint_check(p: &Painter, rect: Rect, color: Color32) {
    let s = rect.width().min(rect.height());
    let c = rect.center();
    let stroke = Stroke::new((s * 0.14).max(1.5), color);
    let a = pos2(c.x - s * 0.3, c.y);
    let b = pos2(c.x - s * 0.08, c.y + s * 0.22);
    let d = pos2(c.x + s * 0.32, c.y - s * 0.24);
    p.line_segment([a, b], stroke);
    p.line_segment([b, d], stroke);
}

/// A painted icon occupying `size` points in the layout.
pub fn icon(ui: &mut Ui, size: f32, paint: impl FnOnce(&Painter, Rect)) -> Response {
    let (rect, r) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    paint(ui.painter(), rect);
    r
}
