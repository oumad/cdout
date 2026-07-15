//! Persistent multi-session chat history.
//!
//! Sessions are JSON files under `%APPDATA%/cdout/sessions/<id>.json`.
//! One file per session keeps the format auditable (you can `cat` it),
//! sidebar-friendly (scan dir, parse metadata), and survives crashes — the
//! frontend debounces saves so the worst-case data loss is the last ~500ms of
//! a turn.
//!
//! The sidebar lists sessions ordered by `last_active_at` desc. Titles are
//! auto-generated from the first user prompt (first 60 chars, whitespace
//! normalised) and stay frozen unless the user explicitly renames.
//!
//! This module also FIXES the documented spotlight-resubmit race: each
//! spotlight submission creates a fresh session via `create_session`, so the
//! main window has no chance to render stale state from the previous
//! conversation while the new one initialises.

use crate::llm::Message;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const SESSIONS_DIR_NAME: &str = "sessions";
const TITLE_MAX_CHARS: usize = 60;

/// Lightweight metadata for sidebar rendering. Embedded in `Session` and
/// also returned standalone by `list_sessions` so we don't ship full message
/// histories to the renderer for every sidebar paint.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub last_active_at: i64,
    pub explorer_path: String,
    pub file_count: usize,
    pub model: String,
    pub message_count: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Session {
    #[serde(flatten)]
    pub meta: SessionMeta,
    pub messages: Vec<Message>,
}

// --- Storage layout ---

fn sessions_dir() -> Result<PathBuf, String> {
    let base = dirs::config_dir()
        .ok_or_else(|| "Failed to locate user config directory".to_string())?;
    let dir = base
        .join(crate::constants::APP_DATA_DIR_NAME)
        .join(SESSIONS_DIR_NAME);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create sessions directory: {}", e))?;
    Ok(dir)
}

fn session_path(id: &str) -> Result<PathBuf, String> {
    if !id_is_safe(id) {
        return Err(format!(
            "Refusing to use session id with unsafe characters: {:?}",
            id
        ));
    }
    Ok(sessions_dir()?.join(format!("{}.json", id)))
}

/// Defends `session_path` against path traversal. Session ids are generated
/// internally as `<millis>_<hex>` but we keep this gate so a future code path
/// can't accidentally write to `../config.json` or similar.
fn id_is_safe(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

// --- ID + title helpers ---

fn new_session_id() -> String {
    let ts = chrono::Utc::now().timestamp_millis();
    let suffix: u32 = rand::random();
    format!("{}_{:08x}", ts, suffix)
}

fn make_title(first_user_prompt: &str) -> String {
    let cleaned: String = first_user_prompt
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.is_empty() {
        return "(empty prompt)".to_string();
    }
    if cleaned.chars().count() > TITLE_MAX_CHARS {
        let truncated: String = cleaned.chars().take(TITLE_MAX_CHARS - 1).collect();
        format!("{}…", truncated)
    } else {
        cleaned
    }
}

// --- Public API ---

pub fn create_session(
    user_prompt: &str,
    explorer_path: &str,
    selected_files_count: usize,
    model: &str,
    initial_messages: Vec<Message>,
) -> Result<Session, String> {
    let now = chrono::Utc::now().timestamp_millis();
    let meta = SessionMeta {
        id: new_session_id(),
        title: make_title(user_prompt),
        created_at: now,
        last_active_at: now,
        explorer_path: explorer_path.to_string(),
        file_count: selected_files_count,
        model: model.to_string(),
        message_count: initial_messages.len(),
    };
    let session = Session {
        meta,
        messages: initial_messages,
    };
    write_session(&session)?;
    Ok(session)
}

pub fn write_session(session: &Session) -> Result<(), String> {
    let path = session_path(&session.meta.id)?;
    // Tee to a UNIQUE sibling file then rename — avoids a half-written session
    // if we crash mid-serialize. The tmp name is made unique per write so two
    // concurrent saves of the same session (debounced auto-save racing an
    // explicit flush) can't clobber each other's scratch file and commit an
    // interleaved/partial JSON. The rename is atomic on the same volume.
    let unique = std::process::id() ^ rand::random::<u32>();
    let tmp = path.with_extension(format!("json.tmp.{unique:08x}"));
    let json = serde_json::to_string_pretty(session)
        .map_err(|e| format!("Failed to serialize session: {}", e))?;
    if let Err(e) = fs::write(&tmp, json) {
        return Err(format!("Failed to write session file: {}", e));
    }
    if let Err(e) = fs::rename(&tmp, &path) {
        // Best-effort cleanup of our scratch file so a failed rename doesn't
        // litter the sessions directory.
        let _ = fs::remove_file(&tmp);
        return Err(format!("Failed to commit session file: {}", e));
    }
    Ok(())
}

pub fn save_messages(
    id: &str,
    messages: Vec<Message>,
    last_active_at: i64,
) -> Result<SessionMeta, String> {
    let path = session_path(id)?;
    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("Session {} not found: {}", id, e))?;
    let mut session: Session = serde_json::from_str(&raw)
        .map_err(|e| format!("Failed to parse session {}: {}", id, e))?;
    session.meta.last_active_at = last_active_at;
    session.meta.message_count = messages.len();
    session.messages = messages;
    write_session(&session)?;
    Ok(session.meta)
}

pub fn load_session(id: &str) -> Result<Session, String> {
    let path = session_path(id)?;
    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("Session {} not found: {}", id, e))?;
    serde_json::from_str(&raw).map_err(|e| format!("Failed to parse session: {}", e))
}

/// List session metadata for the sidebar. Sorted by `last_active_at` desc so
/// most-recently-used appears first. Failures on individual files are logged
/// and skipped — one corrupt file shouldn't take the whole sidebar down.
pub fn list_sessions() -> Result<Vec<SessionMeta>, String> {
    let dir = sessions_dir()?;
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };
    let mut out: Vec<SessionMeta> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let raw = match fs::read_to_string(&path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[sessions] skip {}: read failed: {}", path.display(), e);
                continue;
            }
        };
        // Parse meta only — full message history is deferred to load_session.
        let value: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[sessions] skip {}: json parse failed: {}", path.display(), e);
                continue;
            }
        };
        let meta: SessionMeta = match serde_json::from_value(value.clone()) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[sessions] skip {}: meta parse failed: {}", path.display(), e);
                continue;
            }
        };
        out.push(meta);
    }
    out.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
    Ok(out)
}

pub fn delete_session(id: &str) -> Result<(), String> {
    let path = session_path(id)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Failed to delete session {}: {}", id, e)),
    }
}

pub fn rename_session(id: &str, new_title: &str) -> Result<SessionMeta, String> {
    let path = session_path(id)?;
    let raw = fs::read_to_string(&path)
        .map_err(|e| format!("Session {} not found: {}", id, e))?;
    let mut session: Session = serde_json::from_str(&raw)
        .map_err(|e| format!("Failed to parse session {}: {}", id, e))?;
    let cleaned = make_title(new_title);
    if cleaned == "(empty prompt)" {
        return Err("Title cannot be empty".to_string());
    }
    session.meta.title = cleaned;
    write_session(&session)?;
    Ok(session.meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::Message;
    use std::sync::Mutex;
    use std::sync::OnceLock;

    // All tests touch the same on-disk dir. Serialize to avoid interleaving.
    fn serial_lock() -> std::sync::MutexGuard<'static, ()> {
        static L: OnceLock<Mutex<()>> = OnceLock::new();
        L.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner())
    }

    fn cleanup() {
        if let Ok(dir) = sessions_dir() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }

    #[test]
    fn make_title_normalises_whitespace_and_truncates() {
        let s = make_title("  rename   my  photos  by    date");
        assert_eq!(s, "rename my photos by date");
    }

    #[test]
    fn make_title_truncates_with_ellipsis() {
        let long = "a".repeat(200);
        let s = make_title(&long);
        let chars: Vec<char> = s.chars().collect();
        assert!(chars.len() <= TITLE_MAX_CHARS);
        assert!(chars.last() == Some(&'…'));
    }

    #[test]
    fn make_title_handles_empty_input() {
        assert_eq!(make_title(""), "(empty prompt)");
        assert_eq!(make_title("   \n\t  "), "(empty prompt)");
    }

    #[test]
    fn make_title_preserves_unicode() {
        let s = make_title("café renommer");
        assert!(s.contains("café"));
    }

    #[test]
    fn id_is_safe_rejects_traversal() {
        assert!(!id_is_safe("../etc/passwd"));
        assert!(!id_is_safe("foo/bar"));
        assert!(!id_is_safe("foo bar"));
        assert!(!id_is_safe(""));
        assert!(id_is_safe("1717862400000_a3f2b1c4"));
        assert!(id_is_safe("test-session-1"));
    }

    #[test]
    fn create_load_roundtrip() {
        let _g = serial_lock();
        cleanup();
        let msgs = vec![Message {
            role: "user".to_string(),
            content: "rename photos".to_string(),
            ..Default::default()
        }];
        let s = create_session("rename photos", "C:/Users/x", 3, "anthropic/claude-opus-4.7", msgs)
            .unwrap();
        let loaded = load_session(&s.meta.id).unwrap();
        assert_eq!(loaded.meta.id, s.meta.id);
        assert_eq!(loaded.meta.title, "rename photos");
        assert_eq!(loaded.meta.file_count, 3);
        assert_eq!(loaded.messages.len(), 1);
        cleanup();
    }

    #[test]
    fn list_sessions_orders_by_last_active_desc() {
        let _g = serial_lock();
        cleanup();
        let a = create_session("first", "C:/x", 0, "m", vec![]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = create_session("second", "C:/x", 0, "m", vec![]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let c = create_session("third", "C:/x", 0, "m", vec![]).unwrap();
        let list = list_sessions().unwrap();
        let order: Vec<&str> = list.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(order[0], c.meta.id);
        assert_eq!(order[1], b.meta.id);
        assert_eq!(order[2], a.meta.id);
        cleanup();
    }

    #[test]
    fn save_messages_updates_meta_and_persists() {
        let _g = serial_lock();
        cleanup();
        let s = create_session("hi", "C:/x", 0, "m", vec![]).unwrap();
        let msgs = vec![
            Message {
                role: "user".to_string(),
                content: "hi".to_string(),
                ..Default::default()
            },
            Message {
                role: "assistant".to_string(),
                content: "hello".to_string(),
                ..Default::default()
            },
        ];
        let later_ts = chrono::Utc::now().timestamp_millis() + 1000;
        let updated = save_messages(&s.meta.id, msgs.clone(), later_ts).unwrap();
        assert_eq!(updated.message_count, 2);
        assert_eq!(updated.last_active_at, later_ts);
        let loaded = load_session(&s.meta.id).unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.meta.last_active_at, later_ts);
        cleanup();
    }

    #[test]
    fn delete_session_is_idempotent() {
        let _g = serial_lock();
        cleanup();
        let s = create_session("hi", "C:/x", 0, "m", vec![]).unwrap();
        delete_session(&s.meta.id).unwrap();
        // second delete should NOT error (Idempotent / Not-Found is OK)
        delete_session(&s.meta.id).unwrap();
        assert!(load_session(&s.meta.id).is_err());
        cleanup();
    }

    #[test]
    fn list_sessions_skips_corrupt_files() {
        let _g = serial_lock();
        cleanup();
        let good = create_session("good", "C:/x", 0, "m", vec![]).unwrap();
        // Write a junk file to the sessions dir.
        let dir = sessions_dir().unwrap();
        fs::write(dir.join("garbage.json"), b"{ not json").unwrap();
        let list = list_sessions().unwrap();
        // Only the good session should appear.
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, good.meta.id);
        cleanup();
    }

    #[test]
    fn rename_session_updates_title() {
        let _g = serial_lock();
        cleanup();
        let s = create_session("original title", "C:/x", 0, "m", vec![]).unwrap();
        let meta = rename_session(&s.meta.id, "renamed").unwrap();
        assert_eq!(meta.title, "renamed");
        let loaded = load_session(&s.meta.id).unwrap();
        assert_eq!(loaded.meta.title, "renamed");
        cleanup();
    }

    #[test]
    fn rename_session_rejects_empty_title() {
        let _g = serial_lock();
        cleanup();
        let s = create_session("orig", "C:/x", 0, "m", vec![]).unwrap();
        assert!(rename_session(&s.meta.id, "  ").is_err());
        cleanup();
    }
}
