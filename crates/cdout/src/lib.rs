//! cdout as a native Rust desktop app. All backend behaviour comes from
//! `cdout_core`; this crate is UI only.
//!
//! Layers, each testable on its own:
//! - [`state`]: the pure state machine (unit tests, no I/O, injected time);
//! - [`ui`]: screens as functions of the state (headless `egui_kittest`);
//! - [`dispatch`] and [`app`]: effects, windows, tray and hotkey (exercised
//!   end to end by `scripts/native-smoke.ps1`).

pub mod app;
pub mod crash;
pub mod dispatch;
pub mod fonts;
pub mod hotkey;
pub mod logging;
pub mod platform;
pub mod state;
pub mod tray;
pub mod ui;

/// Run the app until the user quits from the tray.
pub fn run() {
    cdout_core::utils::config::migrate_legacy_data_dir();
    logging::init();
    tracing::info!(
        target: "cdout",
        "app.start version={} data_dir={} restarted={}",
        env!("CARGO_PKG_VERSION"),
        cdout_core::utils::paths::app_data_dir().display(),
        crash::was_restarted()
    );

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("cdout-worker")
        .build()
        .expect("tokio runtime");
    let handle = rt.handle().clone();
    let (tx, rx) = std::sync::mpsc::channel();

    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport: app::main_viewport_builder(),
        renderer: eframe::Renderer::Wgpu,
        // The main window and the spotlight survive restarts at their
        // defaults; nothing about egui's own memory is worth persisting.
        persist_window: false,
        ..Default::default()
    };
    {
        use eframe::{egui_wgpu, wgpu};
        let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
        // A utility that idles in the tray has no business waking the
        // discrete GPU.
        setup.power_preference = wgpu::PowerPreference::LowPower;
        // Windows: DirectX 12, not Vulkan. When another process (a local
        // LLM, a render) has filled the GPU's memory, Vulkan allocations
        // fail outright, wgpu marks the device lost, and egui's renderer
        // panics on the next frame; WDDM pages memory for DirectX 12 and
        // the app keeps running. Reproduced both ways with VRAM full.
        // `WGPU_BACKEND` still overrides, for experiments.
        #[cfg(target_os = "windows")]
        {
            setup.instance_descriptor.backends =
                wgpu::Backends::from_env().unwrap_or(wgpu::Backends::DX12);
        }
        options.wgpu_options.wgpu_setup = egui_wgpu::WgpuSetup::CreateNew(setup);
    }

    let result = eframe::run_native(
        "cdout",
        options,
        Box::new(move |cc| Ok(Box::new(app::CdoutApp::new(cc, handle, tx, rx)))),
    );
    if let Err(e) = result {
        tracing::error!(target: "cdout", "app.error {e}");
    }
    tracing::info!(target: "cdout", "app.exit");
    rt.shutdown_timeout(std::time::Duration::from_secs(2));
}
