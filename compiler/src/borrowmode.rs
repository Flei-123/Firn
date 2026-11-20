use std::sync::atomic::{AtomicU8, Ordering};

pub(crate) const ALLOWED: &str = "off, on, strict";

const LEVEL_OFF: u8 = 0;
const LEVEL_ON: u8 = 1;
const LEVEL_STRICT: u8 = 2;

static LEVEL: AtomicU8 = AtomicU8::new(LEVEL_ON);

pub(crate) fn parse(v: &str) -> Option<u8> {
    match v {
        "off" => Some(LEVEL_OFF),
        "on" => Some(LEVEL_ON),
        "strict" => Some(LEVEL_STRICT),
        _ => None,
    }
}

pub(crate) fn set_level(level: u8) {
    LEVEL.store(level, Ordering::Relaxed);
}

pub(crate) fn enabled() -> bool {
    LEVEL.load(Ordering::Relaxed) >= LEVEL_ON
}

pub(crate) fn strict() -> bool {
    LEVEL.load(Ordering::Relaxed) >= LEVEL_STRICT
}
