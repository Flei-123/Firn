//! Automatic ownership (docs/OWNERSHIP.md): mode switch and runtime text.

pub(crate) mod borrow;
pub(crate) mod concat;
pub(crate) mod fresh;
pub(crate) mod reuse;
pub(crate) mod shape;
pub(crate) mod walk;

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

pub(crate) fn mode_name() -> &'static str {
    match mode() {
        Mode::Off => "off",
        Mode::On => "on",
        Mode::Check => "check",
    }
}

pub(crate) fn apply_env() -> Result<(), String> {
    match std::env::var("FIRNC_OWNERSHIP") {
        Ok(v) if !v.is_empty() => match parse(&v) {
            Some(m) => {
                set_mode(m);
                Ok(())
            }
            None => Err(format!("unknown FIRNC_OWNERSHIP value '{}' (allowed: off, on, check)", v)),
        },
        _ => Ok(()),
    }
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
