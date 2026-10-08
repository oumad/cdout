//! Type-checked from Windows with `cargo check --target aarch64-apple-darwin`;
//! runtime behaviour is unverified until it runs on a Mac.

use eframe::egui::{pos2, Rect};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSEvent, NSScreen, NSWindowCollectionBehavior};

/// Frame of the screen under the mouse, in top-left-origin points (the
/// space winit and egui use on macOS).
pub fn cursor_screen_rect_points() -> Option<Rect> {
    let mtm = MainThreadMarker::new()?;
    let mouse = NSEvent::mouseLocation();
    let screens = NSScreen::screens(mtm);
    // AppKit's origin is the bottom-left of the primary (first) screen.
    let primary_height = screens.firstObject()?.frame().size.height;
    for screen in screens.iter() {
        let f = screen.frame();
        let inside = mouse.x >= f.origin.x
            && mouse.x < f.origin.x + f.size.width
            && mouse.y >= f.origin.y
            && mouse.y < f.origin.y + f.size.height;
        if inside {
            let top = primary_height - (f.origin.y + f.size.height);
            return Some(Rect::from_min_size(
                pos2(f.origin.x as f32, top as f32),
                eframe::egui::vec2(f.size.width as f32, f.size.height as f32),
            ));
        }
    }
    None
}

/// Let the window titled `title` appear on whichever Space is active,
/// including over full-screen apps. Returns whether the window was found.
pub fn join_all_spaces(title: &str) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let app = NSApplication::sharedApplication(mtm);
    for window in app.windows().iter() {
        if window.title().to_string() == title {
            let behavior = window.collectionBehavior()
                | NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary;
            window.setCollectionBehavior(behavior);
            return true;
        }
    }
    false
}
