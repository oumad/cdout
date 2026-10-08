//! The global spotlight hotkey.
//!
//! The OS registration lives on the UI thread (the Windows manager owns a
//! window handle and is not `Send`), so it is kept in a thread-local. The
//! event handler, which may run elsewhere, only compares ids against an
//! atomic.
//!
//! Chords use the same syntax as the Tauri shell's config (`Ctrl+Alt+A`,
//! `Cmd+Alt+A`, `CommandOrControl+Shift+K`): both parse with
//! `global_hotkey::hotkey::HotKey`, so one config file serves both shells.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

/// Overrides the configured chord for this run (development and tests).
pub const HOTKEY_ENV: &str = "CDOUT_HOTKEY";

static CURRENT_ID: AtomicU32 = AtomicU32::new(0);

struct Registry {
    manager: GlobalHotKeyManager,
    current: Option<HotKey>,
}

thread_local! {
    static REGISTRY: RefCell<Option<Registry>> = const { RefCell::new(None) };
}

pub fn parse(chord: &str) -> Result<HotKey, String> {
    chord
        .parse::<HotKey>()
        .map_err(|e| format!("Invalid hotkey '{chord}': {e}"))
}

/// Create the OS hotkey manager. Call once, on the UI thread.
pub fn init() -> Result<(), String> {
    let manager = GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
    REGISTRY.with(|r| {
        *r.borrow_mut() = Some(Registry {
            manager,
            current: None,
        })
    });
    Ok(())
}

/// Point the hotkey at `chord`, replacing the current one. On failure the
/// previous chord stays registered, so a rejected change never leaves the
/// user without a hotkey.
pub fn apply(chord: &str) -> Result<(), String> {
    let hotkey = parse(chord)?;
    REGISTRY.with(|r| {
        let mut r = r.borrow_mut();
        let reg = r.as_mut().ok_or("hotkey manager not initialised")?;
        if reg.current == Some(hotkey) {
            return Ok(());
        }
        let previous = reg.current.take();
        if let Some(prev) = previous {
            let _ = reg.manager.unregister(prev);
        }
        match reg.manager.register(hotkey) {
            Ok(()) => {
                reg.current = Some(hotkey);
                CURRENT_ID.store(hotkey.id(), Ordering::SeqCst);
                tracing::info!(target: "cdout", "hotkey.registered chord={chord}");
                Ok(())
            }
            Err(e) => {
                if let Some(prev) = previous {
                    if reg.manager.register(prev).is_ok() {
                        reg.current = Some(prev);
                    }
                }
                let msg = format!(
                    "Could not register '{chord}': {e}. Another application may already be using that combination."
                );
                tracing::warn!(target: "cdout", "hotkey.error {msg}");
                Err(msg)
            }
        }
    })
}

/// Route presses of the registered chord to `on_press`.
pub fn set_handler(on_press: impl Fn() + Send + Sync + 'static) {
    GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
        if e.state == HotKeyState::Pressed && e.id == CURRENT_ID.load(Ordering::SeqCst) {
            on_press();
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hotkey_parses() {
        let chord = cdout_core::utils::config::DEFAULT_HOTKEY;
        parse(chord).unwrap_or_else(|e| panic!("{e}"));
    }

    #[test]
    fn tauri_style_chords_parse() {
        for chord in [
            "Ctrl+Alt+A",
            "Cmd+Alt+A",
            "CommandOrControl+Shift+K",
            "ctrl+alt+shift+F9",
            "Super+Space",
        ] {
            parse(chord).unwrap_or_else(|e| panic!("{chord}: {e}"));
        }
        assert_eq!(parse("Ctrl+Alt+A").unwrap(), parse("Alt+Ctrl+A").unwrap());
        assert!(parse("Ctrl+").is_err());
        assert!(parse("NotAKey").is_err());
    }
}
