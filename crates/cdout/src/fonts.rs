//! System fonts. The bundled egui fonts render app chrome, but model output
//! routinely contains arrows, check marks and emoji they lack. At startup we
//! add the platform UI font (so text looks native, like the Tauri app's
//! system font stack) and symbol/emoji fonts as fallbacks. Any file that is
//! missing is skipped; the bundled fonts remain the last resort.
//!
//! Not used by headless tests, whose snapshots must not depend on what is
//! installed.

use std::sync::Arc;

use eframe::egui::{FontData, FontDefinitions, FontFamily};

/// (name, path, use as the primary proportional font?)
#[cfg(target_os = "windows")]
const CANDIDATES: &[(&str, &str, bool)] = &[
    ("segoe-ui", r"C:\Windows\Fonts\segoeui.ttf", true),
    ("segoe-ui-symbol", r"C:\Windows\Fonts\seguisym.ttf", false),
    ("segoe-ui-emoji", r"C:\Windows\Fonts\seguiemj.ttf", false),
];

#[cfg(target_os = "macos")]
const CANDIDATES: &[(&str, &str, bool)] = &[
    (
        "helvetica-neue",
        "/System/Library/Fonts/HelveticaNeue.ttc",
        true,
    ),
    (
        "apple-symbols",
        "/System/Library/Fonts/Apple Symbols.ttf",
        false,
    ),
    (
        "arial-unicode",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        false,
    ),
];

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const CANDIDATES: &[(&str, &str, bool)] = &[];

/// Bundled fonts plus whichever system fonts exist. Returns the definitions
/// and the names that were loaded (for the log).
pub fn definitions() -> (FontDefinitions, Vec<&'static str>) {
    let mut defs = FontDefinitions::default();
    let mut loaded = Vec::new();
    let mut primary_inserted = 0;
    for &(name, path, primary) in CANDIDATES {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        defs.font_data
            .insert(name.to_string(), Arc::new(FontData::from_owned(bytes)));
        let proportional = defs.families.entry(FontFamily::Proportional).or_default();
        if primary {
            proportional.insert(primary_inserted, name.to_string());
            primary_inserted += 1;
        } else {
            proportional.push(name.to_string());
            defs.families
                .entry(FontFamily::Monospace)
                .or_default()
                .push(name.to_string());
        }
        loaded.push(name);
    }
    (defs, loaded)
}
