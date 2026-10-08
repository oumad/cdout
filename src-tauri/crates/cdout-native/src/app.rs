//! The eframe application: two native windows over one shared state.
//!
//! - The **root viewport is the main window**. eframe 0.35 attaches AccessKit
//!   (screen readers, UI Automation) to the root viewport only, and the main
//!   window is where nearly all interaction happens. Closing it hides it.
//! - The **spotlight is a deferred child viewport**, shown by the global
//!   hotkey. Deferred, not immediate, so it repaints at full rate even while
//!   the hidden root is throttled.
//!
//! eframe calls `App::ui` only while the root window is visible, but
//! `App::logic` on every pass. So everything that must keep working with all
//! windows hidden (draining messages, the hotkey, keeping the spotlight
//! viewport registered) runs in `logic`.
//!
//! Both viewports run on the UI thread and share [`Shared`] through a mutex.

use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui::{self, ViewportBuilder, ViewportCommand, ViewportId};
use egui_commonmark::CommonMarkCache;

use crate::dispatch::{Dispatcher, MsgSender};
use crate::state::{AppState, Effect, HideReason, Msg};
use crate::{hotkey, platform, tray, ui};

pub const MAIN_SIZE: [f32; 2] = [1100.0, 760.0];
pub const MAIN_MIN_SIZE: [f32; 2] = [900.0, 600.0];
pub const SPOTLIGHT_TITLE: &str = "cdout spotlight";

pub fn spotlight_viewport_id() -> ViewportId {
    ViewportId::from_hash_of("cdout-spotlight")
}

/// Wake both windows so whichever runs next drains the message queue.
pub fn wake_all(ctx: &egui::Context) {
    ctx.request_repaint_of(ViewportId::ROOT);
    ctx.request_repaint_of(spotlight_viewport_id());
}

static WINDOW_ICON_PNG: &[u8] = include_bytes!("../../../icons/128x128.png");

pub fn window_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(WINDOW_ICON_PNG).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

/// The root (main) window's builder, used once at startup.
pub fn main_viewport_builder() -> ViewportBuilder {
    let mut b = ViewportBuilder::default()
        .with_title("cdout")
        .with_inner_size(MAIN_SIZE)
        .with_min_inner_size(MAIN_MIN_SIZE)
        .with_decorations(false)
        // eframe decides surface transparency for *every* window from the
        // root viewport's flag, so on macOS the root must ask for it or the
        // spotlight can't be transparent. The main window paints an opaque
        // panel over its whole surface, so it changes nothing visible there.
        .with_transparent(ui::spotlight::TRANSPARENT_WINDOW)
        .with_visible(false);
    if let Some(icon) = window_icon() {
        b = b.with_icon(Arc::new(icon));
    }
    b
}

/// The spotlight window is exactly the size of its card. On Windows the OS
/// rounds and outlines the corners of the undecorated window (Windows 11);
/// a transparent margin isn't possible there because egui's window builder
/// can't request `WS_EX_NOREDIRECTIONBITMAP`, so the margin would paint
/// white. On macOS the window is transparent and the card paints its own
/// rounded corners.
fn spotlight_viewport_builder(visible: bool) -> ViewportBuilder {
    ViewportBuilder::default()
        .with_title(SPOTLIGHT_TITLE)
        .with_inner_size(ui::spotlight::WINDOW_SIZE)
        .with_decorations(false)
        .with_transparent(ui::spotlight::TRANSPARENT_WINDOW)
        .with_always_on_top()
        .with_taskbar(false)
        .with_resizable(false)
        .with_visible(visible)
}

pub struct Shared {
    pub state: AppState,
    rx: Receiver<Msg>,
    dispatcher: Dispatcher,
    pub md_cache: CommonMarkCache,
    last_spotlight_focus: Option<bool>,
    spotlight_configured: bool,
}

impl Shared {
    /// Apply one message and run its effects.
    pub fn dispatch(&mut self, ctx: &egui::Context, msg: Msg) {
        let fx = self.state.update(msg, Instant::now());
        self.run_effects(ctx, fx);
    }

    fn run_effects(&mut self, ctx: &egui::Context, fx: Vec<Effect>) {
        for effect in self.dispatcher.execute(fx) {
            self.run_ui_effect(ctx, effect);
        }
    }

    /// Drain queued messages, then fire due timers.
    fn pump(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            self.dispatch(ctx, msg);
        }
        self.dispatch(ctx, Msg::Tick);
        if let Some(at) = self.state.next_wakeup() {
            let delay = at.saturating_duration_since(Instant::now());
            ctx.request_repaint_after_for(delay, ViewportId::ROOT);
            ctx.request_repaint_after_for(delay, spotlight_viewport_id());
        }
    }

    fn run_ui_effect(&mut self, ctx: &egui::Context, effect: Effect) {
        let main = ViewportId::ROOT;
        let spot = spotlight_viewport_id();
        match effect {
            Effect::ShowSpotlight => {
                let size = ctx
                    .input_for(spot, |i| i.viewport().outer_rect.map(|r| r.size()))
                    .unwrap_or_else(|| egui::Vec2::from(ui::spotlight::WINDOW_SIZE));
                let ppp = ctx
                    .input_for(spot, |i| i.viewport().native_pixels_per_point)
                    .or_else(|| ctx.input_for(main, |i| i.viewport().native_pixels_per_point))
                    .unwrap_or(1.0);
                if let Some(monitor) = platform::cursor_monitor_rect(ppp) {
                    let pos = platform::center_in(monitor, size);
                    ctx.send_viewport_cmd_to(spot, ViewportCommand::OuterPosition(pos));
                }
                ctx.send_viewport_cmd_to(spot, ViewportCommand::Visible(true));
                ctx.send_viewport_cmd_to(spot, ViewportCommand::Focus);
                ctx.request_repaint_of(spot);
                if !self.spotlight_configured {
                    self.spotlight_configured =
                        platform::configure_spotlight_window(SPOTLIGHT_TITLE);
                }
                tracing::info!(target: "cdout", "spotlight.shown");
            }
            Effect::HideSpotlight(reason) => {
                ctx.send_viewport_cmd_to(spot, ViewportCommand::Visible(false));
                tracing::info!(target: "cdout", "spotlight.hidden reason={}", reason.as_str());
            }
            Effect::ShowMain { center_on_cursor } => {
                if center_on_cursor {
                    let ppp = ctx
                        .input_for(main, |i| i.viewport().native_pixels_per_point)
                        .unwrap_or(1.0);
                    let size = ctx
                        .input_for(main, |i| i.viewport().outer_rect.map(|r| r.size()))
                        .unwrap_or_else(|| egui::Vec2::from(MAIN_SIZE));
                    if let Some(monitor) = platform::cursor_monitor_rect(ppp) {
                        let pos = platform::center_in(monitor, size);
                        ctx.send_viewport_cmd_to(main, ViewportCommand::OuterPosition(pos));
                    }
                }
                ctx.send_viewport_cmd_to(main, ViewportCommand::Visible(true));
                ctx.send_viewport_cmd_to(main, ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd_to(main, ViewportCommand::Focus);
                ctx.request_repaint_of(main);
                tracing::info!(target: "cdout", "main.shown");
            }
            Effect::HideMain => {
                ctx.send_viewport_cmd_to(main, ViewportCommand::Visible(false));
                tracing::info!(target: "cdout", "main.hidden");
            }
            Effect::MinimizeMain => {
                ctx.send_viewport_cmd_to(main, ViewportCommand::Minimized(true))
            }
            Effect::ToggleMaximizeMain => {
                let maximized = ctx
                    .input_for(main, |i| i.viewport().maximized)
                    .unwrap_or(false);
                ctx.send_viewport_cmd_to(main, ViewportCommand::Maximized(!maximized));
            }
            Effect::Quit => {
                tracing::info!(target: "cdout", "app.quit");
                self.dispatcher.drain_io();
                ctx.send_viewport_cmd_to(main, ViewportCommand::Close);
            }
            Effect::ApplyHotkey { chord, persist } => {
                let result = hotkey::apply(&chord).and_then(|()| {
                    if persist {
                        cdout_core::utils::config::set_hotkey(chord.clone())
                    } else {
                        Ok(())
                    }
                });
                self.dispatcher
                    .sender()
                    .send(Msg::HotkeyApplied { chord, result });
            }
            other => unreachable!("not a UI effect: {other:?}"),
        }
    }
}

pub struct CdoutApp {
    shared: Arc<Mutex<Shared>>,
    _tray: Option<tray_icon::TrayIcon>,
}

impl CdoutApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        rt: tokio::runtime::Handle,
        tx: std::sync::mpsc::Sender<Msg>,
        rx: Receiver<Msg>,
    ) -> Self {
        let ctx = cc.egui_ctx.clone();
        if let Some(rs) = &cc.wgpu_render_state {
            let info = rs.adapter.get_info();
            tracing::info!(target: "cdout", "gpu.adapter backend={:?} name={}", info.backend, info.name);
        }
        let (fonts, loaded) = crate::fonts::definitions();
        ctx.set_fonts(fonts);
        tracing::info!(target: "cdout", "fonts.loaded {}", loaded.join(","));
        ui::theme::apply(&ctx);

        let sender = MsgSender::new(tx, ctx.clone());
        let dispatcher = Dispatcher::new(rt, sender.clone());

        let tray = match tray::create(sender.clone()) {
            Ok(t) => Some(t),
            Err(e) => {
                tracing::error!(target: "cdout", "tray.error {e}");
                None
            }
        };

        let state = AppState::new(cdout_core::api::platform_info(None));
        let mut shared = Shared {
            state,
            rx,
            dispatcher,
            md_cache: CommonMarkCache::default(),
            last_spotlight_focus: None,
            spotlight_configured: false,
        };

        match hotkey::init() {
            Ok(()) => {
                let hotkey_out = sender.clone();
                hotkey::set_handler(move || hotkey_out.send(Msg::HotkeyPressed));
                register_startup_hotkey();
            }
            Err(e) => tracing::error!(target: "cdout", "hotkey.error manager: {e}"),
        }

        shared.dispatch(&ctx, Msg::Started);
        Self {
            shared: Arc::new(Mutex::new(shared)),
            _tray: tray,
        }
    }
}

/// Register the configured hotkey (or `CDOUT_HOTKEY`), falling back to the
/// built-in default so a bad config can't leave the spotlight unreachable.
fn register_startup_hotkey() {
    let configured = std::env::var(hotkey::HOTKEY_ENV)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(cdout_core::utils::config::get_hotkey);
    if let Err(e) = hotkey::apply(&configured) {
        let default = cdout_core::utils::config::DEFAULT_HOTKEY;
        tracing::warn!(target: "cdout", "hotkey.error {e}; falling back to '{default}'");
        if configured != default {
            let _ = hotkey::apply(default);
        }
    }
}

fn spotlight_pass(ui: &mut egui::Ui, shared: &Mutex<Shared>) {
    let ctx = ui.ctx().clone();
    let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
    let sh = &mut *guard;
    sh.pump(&ctx);

    // The spotlight is never closed, only hidden (Alt+F4 included).
    if ctx.input(|i| i.viewport().close_requested()) {
        ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        sh.dispatch(&ctx, Msg::SpotlightHide(HideReason::Escape));
    }
    let focused = ctx.input(|i| i.viewport().focused);
    if focused != sh.last_spotlight_focus {
        sh.last_spotlight_focus = focused;
        if let Some(f) = focused {
            sh.dispatch(&ctx, Msg::SpotlightFocusChanged(f));
        }
    }

    let mut out = Vec::new();
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE)
        .show(ui, |ui| ui::spotlight::show(ui, &mut sh.state, &mut out));
    for msg in out {
        sh.dispatch(&ctx, msg);
    }
}

impl eframe::App for CdoutApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // The spotlight needs a transparent clear; the main window paints an
        // opaque panel over the whole surface anyway.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let spotlight_visible;
        {
            let mut guard = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            let sh = &mut *guard;
            sh.pump(ctx);
            // Closing the main window (title bar, Alt+F4, Cmd+W) hides it to
            // the tray; only "Quit cdout" exits.
            if ctx.input(|i| i.viewport().close_requested()) && !sh.state.quitting {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                sh.dispatch(ctx, Msg::CloseMain);
            }
            spotlight_visible = sh.state.spotlight.visible;
        }
        let shared = self.shared.clone();
        ctx.show_viewport_deferred(
            spotlight_viewport_id(),
            spotlight_viewport_builder(spotlight_visible),
            move |ui, _class| spotlight_pass(ui, &shared),
        );
    }

    /// Draws the main window; only called while it is visible.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut guard = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let sh = &mut *guard;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let mut out = Vec::new();
        {
            let Shared {
                state, md_cache, ..
            } = sh;
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(ui::theme::GRAY_950))
                .show(ui, |ui| {
                    ui::main_window::show(ui, state, md_cache, now_ms, &mut out)
                });
        }
        for msg in out {
            sh.dispatch(&ctx, msg);
        }
    }
}
