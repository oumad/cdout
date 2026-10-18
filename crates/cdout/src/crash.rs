//! Come back after a crash.
//!
//! A panic on the UI thread (the GPU renderer is the realistic source: a
//! lost device under memory pressure) takes the whole process down. The
//! panic hook therefore relaunches the executable before the process dies,
//! and the new instance restores the spotlight draft and says what
//! happened. A marker file rate-limits this so a crash on startup cannot
//! loop.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Set on the relaunched process. Read once at startup.
pub const RESTARTED_ENV: &str = "CDOUT_RESTARTED_AFTER_CRASH";
/// Never relaunch more often than this.
const MIN_INTERVAL: Duration = Duration::from_secs(60);
const MARKER: &str = "last_crash_restart";

fn marker_path() -> PathBuf {
    cdout_core::utils::paths::app_data_dir().join(MARKER)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Whether this process was started by [`relaunch_after_panic`].
pub fn was_restarted() -> bool {
    std::env::var_os(RESTARTED_ENV).is_some_and(|v| v == "1")
}

/// Decide, from the marker file's timestamp, whether a relaunch is allowed
/// now; writes the marker when it is. Pure apart from the file.
fn claim_restart_slot(marker: &std::path::Path, now: u64, min_interval: Duration) -> bool {
    let last = std::fs::read_to_string(marker)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    if now.saturating_sub(last) < min_interval.as_secs() {
        return false;
    }
    if let Some(dir) = marker.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(marker, now.to_string()).is_ok()
}

/// Spawn a fresh copy of this executable, detached, with the same
/// environment plus [`RESTARTED_ENV`]. Returns false when rate-limited or
/// the spawn failed. Called from the panic hook, so it must not panic.
pub fn relaunch_after_panic() -> bool {
    if !claim_restart_slot(&marker_path(), now_secs(), MIN_INTERVAL) {
        tracing::warn!(target: "cdout", "crash.restart skipped: a restart already happened within the last minute");
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.env(RESTARTED_ENV, "1")
        // The test-only crash trigger must not follow us into the new process.
        .env_remove("CDOUT_TEST_PANIC_MS")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // A new process group so the child is not killed with this one.
        cmd.creation_flags(0x0000_0200 | 0x0000_0008); // CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS
    }
    match cmd.spawn() {
        Ok(child) => {
            tracing::info!(target: "cdout", "crash.restart spawned pid={}", child.id());
            true
        }
        Err(e) => {
            tracing::error!(target: "cdout", "crash.restart failed: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_slot_is_rate_limited() {
        let dir = std::env::temp_dir().join(format!("cdout-crash-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let marker = dir.join(MARKER);
        let one_min = Duration::from_secs(60);
        assert!(
            claim_restart_slot(&marker, 1_000, one_min),
            "first crash: restart"
        );
        assert!(
            !claim_restart_slot(&marker, 1_030, one_min),
            "30 s later: no restart loop"
        );
        assert!(
            claim_restart_slot(&marker, 1_061, one_min),
            "after the interval: restart again"
        );
        assert_eq!(std::fs::read_to_string(&marker).unwrap().trim(), "1061");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn garbage_marker_counts_as_no_recent_restart() {
        let dir = std::env::temp_dir().join(format!("cdout-crash-test2-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let marker = dir.join(MARKER);
        std::fs::write(&marker, "not a number").unwrap();
        assert!(claim_restart_slot(&marker, 1_000, Duration::from_secs(60)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
