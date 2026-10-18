use eframe::egui::{pos2, Rect};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

/// Full bounds (not work area, matching the Tauri shell) of the monitor
/// under the cursor, in physical pixels.
pub fn cursor_monitor_rect_px() -> Option<Rect> {
    // SAFETY: plain Win32 queries writing into stack-allocated structs whose
    // sizes are set as the API requires; no handles are retained.
    unsafe {
        let mut pt = POINT::default();
        GetCursorPos(&mut pt).ok()?;
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        let r = info.rcMonitor;
        Some(Rect::from_min_max(
            pos2(r.left as f32, r.top as f32),
            pos2(r.right as f32, r.bottom as f32),
        ))
    }
}
