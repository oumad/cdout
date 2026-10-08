//! Log to `<data dir>/logs/cdout-native.log` and stderr.
//!
//! The file writer is synchronous on purpose: end-to-end tests read the log
//! while the app runs and kill it afterwards, so nothing may sit in a buffer.
//! Event names (`app.start`, `hotkey.registered`, `spotlight.shown`, …) are a
//! contract with `scripts/native-smoke.ps1`; keep them stable.

use std::path::PathBuf;

use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::EnvFilter;

pub const LOG_FILE: &str = "cdout-native.log";

pub fn log_dir() -> PathBuf {
    cdout_core::utils::paths::app_data_dir().join("logs")
}

pub fn init() {
    let dir = log_dir();
    let _ = std::fs::create_dir_all(&dir);
    let file = tracing_appender::rolling::never(&dir, LOG_FILE);
    let filter =
        EnvFilter::try_from_env("CDOUT_LOG").unwrap_or_else(|_| EnvFilter::new("warn,cdout=info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_target(false)
        .with_writer(file.and(std::io::stderr))
        .try_init();

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(target: "cdout", "app.panic {}", panic_message(info));
        default_hook(info);
    }));
}

/// The panic payload and location as one line.
pub fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<non-string panic>".to_string());
    match info.location() {
        Some(loc) => format!("{payload} at {}:{}", loc.file(), loc.line()),
        None => payload,
    }
}
