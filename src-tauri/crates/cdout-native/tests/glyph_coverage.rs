//! Every non-ASCII character in the UI sources must be renderable by egui's
//! bundled fonts, so app chrome looks the same on every OS whatever fonts are
//! installed. (Model output is different: system fallback fonts handle it.)

use eframe::egui;

const SOURCES: &[(&str, &str)] = &[
    ("app.rs", include_str!("../src/app.rs")),
    ("ui/approval.rs", include_str!("../src/ui/approval.rs")),
    ("ui/chat.rs", include_str!("../src/ui/chat.rs")),
    ("ui/chat_input.rs", include_str!("../src/ui/chat_input.rs")),
    ("ui/icons.rs", include_str!("../src/ui/icons.rs")),
    (
        "ui/main_window.rs",
        include_str!("../src/ui/main_window.rs"),
    ),
    (
        "ui/model_select.rs",
        include_str!("../src/ui/model_select.rs"),
    ),
    ("ui/onboarding.rs", include_str!("../src/ui/onboarding.rs")),
    ("ui/settings.rs", include_str!("../src/ui/settings.rs")),
    ("ui/sidebar.rs", include_str!("../src/ui/sidebar.rs")),
    ("ui/spotlight.rs", include_str!("../src/ui/spotlight.rs")),
    ("ui/title_bar.rs", include_str!("../src/ui/title_bar.rs")),
    ("ui/widgets.rs", include_str!("../src/ui/widgets.rs")),
    ("state/text.rs", include_str!("../src/state/text.rs")),
    ("state/mod.rs", include_str!("../src/state/mod.rs")),
];

#[test]
fn ui_glyphs_are_covered_by_bundled_fonts() {
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(Default::default(), |_| {});
    let font = egui::FontId::proportional(14.0);
    let mut missing = Vec::new();
    for (file, src) in SOURCES {
        for (n, line) in src.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue; // comments are never rendered
            }
            for ch in line.chars().filter(|c| !c.is_ascii()) {
                let ok = ctx.fonts_mut(|f| f.has_glyph(&font, ch));
                if !ok {
                    missing.push(format!("{file}:{} '{ch}' U+{:04X}", n + 1, ch as u32));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "glyphs without bundled-font coverage:\n{}",
        missing.join("\n")
    );
}
