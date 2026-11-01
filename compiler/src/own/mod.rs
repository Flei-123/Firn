//! Automatic ownership (docs/OWNERSHIP.md): mode switch and runtime text.

pub(crate) mod borrow;
pub(crate) mod concat;
pub(crate) mod fir;
pub(crate) mod fresh;
pub(crate) mod reuse;
pub(crate) mod shape;
pub(crate) mod walk;

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mode {
    Off,
    On,
    Check,
    Strict,
}

static MODE: AtomicU8 = AtomicU8::new(1);

static EXPLICIT: AtomicBool = AtomicBool::new(false);

const RUNTIME: &str = include_str!("../../../lib/gc/own.fi");

pub(crate) const ALLOWED: &str = "off, on, check, strict";

const STRICT_SWITCH: (&str, &str) = ("const OWN_STRICT: u64 = 0\n", "const OWN_STRICT: u64 = 1\n");

const STRICT_GC_HOOKS: &[(&str, &str)] = &[("fn __gc_trace(obj: u64) {\n", "    __own_strict_trace(obj)\n")];

pub(crate) fn runtime_text() -> String {
    if mode() != Mode::Strict {
        return RUNTIME.to_string();
    }
    RUNTIME.replacen(STRICT_SWITCH.0, STRICT_SWITCH.1, 1)
}

pub(crate) fn patch_gc_runtime(s: &mut String) {
    if mode() != Mode::Strict {
        return;
    }
    for (anchor, insert) in STRICT_GC_HOOKS {
        if let Some(at) = s.find(anchor) {
            s.insert_str(at + anchor.len(), insert);
        }
    }
}

pub(crate) fn parse(s: &str) -> Option<Mode> {
    match s {
        "off" => Some(Mode::Off),
        "on" => Some(Mode::On),
        "check" => Some(Mode::Check),
        "strict" => Some(Mode::Strict),
        _ => None,
    }
}

pub(crate) fn set_mode(m: Mode) {
    let v = match m {
        Mode::Off => 0,
        Mode::On => 1,
        Mode::Check => 2,
        Mode::Strict => 3,
    };
    MODE.store(v, Ordering::Relaxed);
    EXPLICIT.store(true, Ordering::Relaxed);
}

pub(crate) fn mode_name() -> &'static str {
    match mode() {
        Mode::Off => "off",
        Mode::On => "on",
        Mode::Check => "check",
        Mode::Strict => "strict",
    }
}

pub(crate) fn apply_env() -> Result<(), String> {
    match std::env::var("FIRNC_OWNERSHIP") {
        Ok(v) if !v.is_empty() => match parse(&v) {
            Some(m) => {
                set_mode(m);
                Ok(())
            }
            None => Err(format!("unknown FIRNC_OWNERSHIP value '{}' (allowed: {})", v, ALLOWED)),
        },
        _ => Ok(()),
    }
}

fn default_applies() -> bool {
    EXPLICIT.load(Ordering::Relaxed)
        || (matches!(crate::target::active(), crate::target::Target::X86_64 | crate::target::Target::Aarch64)
            && !crate::prof::is_kernel())
}

pub(crate) fn mode() -> Mode {
    match MODE.load(Ordering::Relaxed) {
        1 if default_applies() => Mode::On,
        2 => Mode::Check,
        3 => Mode::Strict,
        _ => Mode::Off,
    }
}

pub(crate) fn enabled() -> bool {
    mode() != Mode::Off
}

#[cfg(test)]
mod tests {
    #[test]
    fn strict_anchors_exist() {
        let gc = include_str!("../../../lib/gc/gc.fi");
        for (anchor, _) in super::STRICT_GC_HOOKS {
            assert_eq!(gc.matches(anchor).count(), 1, "gc.fi anchor {:?}", anchor);
        }
        assert_eq!(super::RUNTIME.matches(super::STRICT_SWITCH.0).count(), 1);
        assert!(super::RUNTIME.contains("fn __own_strict_trace(obj: u64)"));
    }
}
