use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) const ALLOWED: &str = "off, strict";

static STRICT: AtomicBool = AtomicBool::new(false);

pub(crate) fn parse(v: &str) -> Option<bool> {
    match v {
        "off" => Some(false),
        "strict" => Some(true),
        _ => None,
    }
}

pub(crate) fn set_strict(on: bool) {
    STRICT.store(on, Ordering::Relaxed);
}

pub(crate) fn enabled() -> bool {
    STRICT.load(Ordering::Relaxed)
}
