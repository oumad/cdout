//! OS-specific window helpers. Everything here is small and isolated so the
//! rest of the app stays `cfg`-free; each function has a portable fallback.

use eframe::egui::{pos2, Pos2, Rect, Vec2};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// Top-left position that centres a window of `size` inside `monitor`.
/// Both in the same coordinate space (egui points).
pub fn center_in(monitor: Rect, size: Vec2) -> Pos2 {
    pos2(
        (monitor.min.x + (monitor.width() - size.x) / 2.0).round(),
        (monitor.min.y + (monitor.height() - size.y) / 2.0).round(),
    )
}

/// Bounds of the monitor under the mouse cursor, in egui points for a
/// window whose native scale is `native_ppp` (egui converts positions back to
/// physical pixels with that factor). `None` when the OS can't say.
pub fn cursor_monitor_rect(native_ppp: f32) -> Option<Rect> {
    #[cfg(target_os = "windows")]
    {
        windows::cursor_monitor_rect_px().map(|r| {
            let s = native_ppp.max(0.1);
            Rect::from_min_max(
                pos2(r.min.x / s, r.min.y / s),
                pos2(r.max.x / s, r.max.y / s),
            )
        })
    }
    #[cfg(target_os = "macos")]
    {
        let _ = native_ppp;
        macos::cursor_screen_rect_points()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = native_ppp;
        None
    }
}

/// Call `f` whenever the app becomes active. On macOS this is how a dock-icon
/// click reaches us when every window is hidden (winit doesn't surface
/// "reopen"). Elsewhere activation always goes through a window, so this is
/// a no-op.
pub fn on_app_activated(f: impl Fn() + Send + Sync + 'static) {
    #[cfg(target_os = "macos")]
    macos::on_app_activated(f);
    #[cfg(not(target_os = "macos"))]
    let _ = f;
}

/// One-time tweaks to the spotlight's native window, found by its title: on
/// macOS, appear on every Space (a palette that opens on the Space you left
/// is useless). Returns false when the window doesn't exist yet, so the
/// caller can retry on the next show.
pub fn configure_spotlight_window(title: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::join_all_spaces(title)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = title;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::vec2;

    #[test]
    fn centers_on_a_primary_monitor() {
        let m = Rect::from_min_size(pos2(0.0, 0.0), vec2(1920.0, 1080.0));
        assert_eq!(center_in(m, vec2(600.0, 260.0)), pos2(660.0, 410.0));
    }

    #[test]
    fn centers_on_a_monitor_left_of_the_primary() {
        let m = Rect::from_min_size(pos2(-2560.0, -200.0), vec2(2560.0, 1440.0));
        assert_eq!(center_in(m, vec2(1100.0, 760.0)), pos2(-1830.0, 140.0));
    }

    #[test]
    fn window_larger_than_monitor_overhangs_evenly() {
        let m = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        assert_eq!(center_in(m, vec2(1000.0, 800.0)), pos2(-100.0, -100.0));
    }

    #[test]
    fn cursor_monitor_is_sane_when_reported() {
        if let Some(r) = cursor_monitor_rect(1.0) {
            assert!(r.width() > 100.0 && r.height() > 100.0, "{r:?}");
        }
    }
}
