// SPDX-License-Identifier: MPL-2.0
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub const SUPPORTED: &[&str] = &["2026"];

static ACTIVE: AtomicUsize = AtomicUsize::new(SUPPORTED.len() - 1);
static EXPLICIT: AtomicBool = AtomicBool::new(false);

pub fn current() -> &'static str {
    SUPPORTED[SUPPORTED.len() - 1]
}

pub fn active() -> &'static str {
    SUPPORTED[ACTIVE.load(Ordering::Relaxed)]
}

pub fn check(name: &str) -> Result<usize, String> {
    match SUPPORTED.iter().position(|e| *e == name) {
        Some(i) => Ok(i),
        None => Err(format!("unknown edition '{}' (supported: {})", name, SUPPORTED.join(", "))),
    }
}

pub fn flag_set(name: &str) -> Result<(), String> {
    let i = check(name)?;
    ACTIVE.store(i, Ordering::Relaxed);
    EXPLICIT.store(true, Ordering::Relaxed);
    Ok(())
}

pub fn manifest_set(name: &str) {
    if name.is_empty() || EXPLICIT.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(i) = check(name) {
        ACTIVE.store(i, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_is_supported() {
        assert!(check(current()).is_ok());
    }

    #[test]
    fn unknown_edition_is_refused_with_the_supported_list() {
        let e = check("1999").unwrap_err();
        assert_eq!(e, "unknown edition '1999' (supported: 2026)");
        assert!(check("").is_err());
        assert!(check("2026x").is_err());
    }

    #[test]
    fn flag_wins_over_manifest() {
        assert!(flag_set("2026").is_ok());
        manifest_set("2026");
        assert_eq!(active(), "2026");
        assert!(flag_set("2999").is_err());
    }
}
