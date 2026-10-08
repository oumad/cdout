//! Colours and visuals matching the React app's Tailwind palette:
//! gray-950 background, indigo for the user, emerald for the agent and
//! tools, amber for warnings, red for danger.

use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};

pub const GRAY_950: Color32 = Color32::from_rgb(0x03, 0x07, 0x12);
pub const GRAY_900: Color32 = Color32::from_rgb(0x11, 0x18, 0x27);
pub const GRAY_800: Color32 = Color32::from_rgb(0x1f, 0x29, 0x37);
pub const GRAY_700: Color32 = Color32::from_rgb(0x37, 0x41, 0x51);
pub const GRAY_600: Color32 = Color32::from_rgb(0x4b, 0x55, 0x63);
pub const GRAY_500: Color32 = Color32::from_rgb(0x6b, 0x72, 0x80);
pub const GRAY_400: Color32 = Color32::from_rgb(0x9c, 0xa3, 0xaf);
pub const GRAY_300: Color32 = Color32::from_rgb(0xd1, 0xd5, 0xdb);
pub const GRAY_200: Color32 = Color32::from_rgb(0xe5, 0xe7, 0xeb);
pub const GRAY_100: Color32 = Color32::from_rgb(0xf3, 0xf4, 0xf6);
pub const INDIGO_300: Color32 = Color32::from_rgb(0xa5, 0xb4, 0xfc);
pub const INDIGO_500: Color32 = Color32::from_rgb(0x63, 0x66, 0xf1);
pub const INDIGO_600: Color32 = Color32::from_rgb(0x4f, 0x46, 0xe5);
pub const INDIGO_900: Color32 = Color32::from_rgb(0x31, 0x2e, 0x81);
pub const EMERALD_500: Color32 = Color32::from_rgb(0x10, 0xb9, 0x81);
pub const EMERALD_600: Color32 = Color32::from_rgb(0x05, 0x96, 0x69);
pub const AMBER_300: Color32 = Color32::from_rgb(0xfc, 0xd3, 0x4d);
pub const AMBER_400: Color32 = Color32::from_rgb(0xfb, 0xbf, 0x24);
pub const AMBER_500: Color32 = Color32::from_rgb(0xf5, 0x9e, 0x0b);
pub const AMBER_600: Color32 = Color32::from_rgb(0xd9, 0x77, 0x06);
pub const AMBER_950: Color32 = Color32::from_rgb(0x2a, 0x1a, 0x05);
pub const RED_400: Color32 = Color32::from_rgb(0xf8, 0x71, 0x71);
pub const RED_500: Color32 = Color32::from_rgb(0xef, 0x44, 0x44);
pub const RED_600: Color32 = Color32::from_rgb(0xdc, 0x26, 0x26);
pub const RED_950: Color32 = Color32::from_rgb(0x2b, 0x0a, 0x0a);
pub const BLACK_40: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 102);

pub const RADIUS: CornerRadius = CornerRadius::same(6);
pub const RADIUS_LG: CornerRadius = CornerRadius::same(12);

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.panel_fill = GRAY_950;
    v.window_fill = GRAY_900;
    v.extreme_bg_color = Color32::from_rgb(0x0b, 0x0f, 0x19);
    v.faint_bg_color = GRAY_900;
    v.code_bg_color = Color32::from_rgb(0x0b, 0x0f, 0x19);
    v.hyperlink_color = INDIGO_300;
    v.override_text_color = Some(GRAY_200);
    v.selection.bg_fill = INDIGO_600;
    v.selection.stroke = Stroke::new(1.0, INDIGO_300);
    v.window_stroke = Stroke::new(1.0, GRAY_700);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, GRAY_800);
    v.widgets.inactive.weak_bg_fill = GRAY_800;
    v.widgets.inactive.bg_fill = GRAY_800;
    v.widgets.hovered.weak_bg_fill = GRAY_700;
    v.widgets.hovered.bg_fill = GRAY_700;
    v.widgets.active.weak_bg_fill = GRAY_600;
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.noninteractive,
    ] {
        w.corner_radius = RADIUS;
    }
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.interaction.selectable_labels = false;
    });
}
