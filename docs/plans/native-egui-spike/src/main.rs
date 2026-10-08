use std::fs::OpenOptions;
use std::io::Write;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke};
use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

fn log(msg: &str) {
    let mut f = OpenOptions::new().create(true).append(true).open("spike.log").unwrap();
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
    writeln!(f, "{ms} {msg}").ok();
}

enum Ev { Hotkey(GlobalHotKeyEvent), Tray(String), Menu(String) }

struct App {
    _tray: TrayIcon,
    _hk: GlobalHotKeyManager,
    rx: mpsc::Receiver<Ev>,
    visible: bool,
    frames: u64,
    started: Instant,
    text: String,
    last_focus: Option<bool>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let icon = tray_icon::Icon::from_rgba(vec![255u8; 16 * 16 * 4], 16, 16).expect("icon");
        let menu = Menu::new();
        let quit = MenuItem::with_id("quit", "Quit", true, None);
        menu.append(&quit).expect("menu append");
        let tray = TrayIconBuilder::new()
            .with_tooltip("cdout spike")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()
            .expect("tray");
        log("tray created");

        let hk = GlobalHotKeyManager::new().expect("hk manager");
        let hotkey: HotKey = "ctrl+alt+shift+F9".parse().expect("parse hotkey");
        hk.register(hotkey).expect("register hotkey");
        log(&format!("hotkey registered id={}", hotkey.id()));

        let (tx, rx) = mpsc::channel::<Ev>();
        {
            let tx = tx.clone();
            let ctx = cc.egui_ctx.clone();
            GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
                let _ = tx.send(Ev::Hotkey(e));
                ctx.request_repaint();
            }));
        }
        {
            let tx = tx.clone();
            let ctx = cc.egui_ctx.clone();
            TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
                let _ = tx.send(Ev::Tray(format!("{e:?}")));
                ctx.request_repaint();
            }));
        }
        {
            let ctx = cc.egui_ctx.clone();
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
                let _ = tx.send(Ev::Menu(e.id.0.clone()));
                ctx.request_repaint();
            }));
        }
        log("handlers installed");
        Self { _tray: tray, _hk: hk, rx, visible: true, frames: 0, started: Instant::now(), text: String::new(), last_focus: None }
    }

    fn set_visible(&mut self, ctx: &egui::Context, v: bool) {
        self.visible = v;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(v));
        if v { ctx.send_viewport_cmd(egui::ViewportCommand::Focus); }
        log(&format!("set_visible {v}"));
    }
}

impl eframe::App for App {
    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        self.frames += 1;
        if self.frames == 1 { log("first frame"); }
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Ev::Hotkey(e) => {
                    log(&format!("hotkey event id={} state={:?}", e.id, e.state));
                    if e.state == HotKeyState::Pressed { let v = !self.visible; self.set_visible(ctx, v); }
                }
                Ev::Tray(s) => log(&format!("tray event {s}")),
                Ev::Menu(id) => { log(&format!("menu event {id}")); if id == "quit" { ctx.send_viewport_cmd(egui::ViewportCommand::Close); } }
            }
        }
        let focused = ctx.input(|i| i.viewport().focused);
        if focused != self.last_focus { log(&format!("focus -> {focused:?}")); self.last_focus = focused; }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { self.set_visible(ctx, false); }

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(root, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(17, 24, 39))
                .stroke(Stroke::new(1.0, Color32::from_rgb(55, 65, 81)))
                .corner_radius(CornerRadius::same(16))
                .inner_margin(Margin::same(24))
                .show(ui, |ui| {
                    ui.set_min_size(egui::vec2(550.0, 210.0));
                    ui.heading(egui::RichText::new("cdout spike").color(Color32::from_rgb(229, 231, 235)));
                    let r = ui.add(egui::TextEdit::singleline(&mut self.text).hint_text("Ask cdout… 🚀 émoji ✓").desired_width(f32::INFINITY));
                    if self.frames == 1 { r.request_focus(); }
                    ui.label(format!("frames={} focused={:?} visible={}", self.frames, focused, self.visible));
                    if ui.button("Hide").clicked() { self.set_visible(ctx, false); }
                });
        });

        if self.started.elapsed() > Duration::from_secs(150) {
            log("auto-exit");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

fn main() -> eframe::Result {
    let _ = std::fs::remove_file("spike.log");
    log("main start");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([600.0, 260.0])
            .with_position([300.0, 300.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native("cdout-spike", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
