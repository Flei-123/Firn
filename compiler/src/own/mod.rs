//! Automatic ownership (docs/OWNERSHIP.md): mode switch and runtime text.

pub(crate) mod concat;

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mode {
    Off,
    On,
    Check,
}

static MODE: AtomicU8 = AtomicU8::new(0);

pub(crate) const RUNTIME: &str = include_str!("../../../lib/gc/own.fi");

pub(crate) fn parse(s: &str) -> Option<Mode> {
    match s {
        "off" => Some(Mode::Off),
        "on" => Some(Mode::On),
        "check" => Some(Mode::Check),
        _ => None,
    }
}

pub(crate) fn set_mode(m: Mode) {
    let v = match m {
        Mode::Off => 0,
        Mode::On => 1,
        Mode::Check => 2,
    };
    MODE.store(v, Ordering::Relaxed);
}

pub(crate) fn mode() -> Mode {
    match MODE.load(Ordering::Relaxed) {
        1 => Mode::On,
        2 => Mode::Check,
        _ => Mode::Off,
    }
}

pub(crate) fn enabled() -> bool {
    mode() != Mode::Off
}
