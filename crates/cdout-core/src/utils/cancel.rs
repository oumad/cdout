use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static CANCELLED: OnceLock<AtomicBool> = OnceLock::new();

fn flag() -> &'static AtomicBool {
    CANCELLED.get_or_init(|| AtomicBool::new(false))
}

pub fn reset() {
    flag().store(false, Ordering::Relaxed);
}

pub fn request() {
    flag().store(true, Ordering::Relaxed);
}

pub fn is_cancelled() -> bool {
    flag().load(Ordering::Relaxed)
}
