mod overlay;

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Profile {
    Default,
    Certus,
}

pub(crate) const ALLOWED: &str = "default, certus";
pub(crate) const CARDON_OFF: u64 = 2552;

struct Module {
    text: &'static str,
}

const CERTUS: &[Module] = &[
    Module { text: include_str!("../../../lib/gc/ext/certus.fi") },
    Module { text: include_str!("../../../lib/gc/ext/cards.fi") },
    Module { text: include_str!("../../../lib/gc/ext/par.fi") },
];

static PROFILE: AtomicU8 = AtomicU8::new(0);
static CARDS: AtomicBool = AtomicBool::new(false);

pub(crate) fn parse(v: &str) -> Option<Profile> {
    match v {
        "default" => Some(Profile::Default),
        "certus" => Some(Profile::Certus),
        _ => None,
    }
}

pub(crate) fn set_profile(p: Profile) {
    PROFILE.store(p as u8, Ordering::Relaxed);
}

pub(crate) fn profile() -> Profile {
    match PROFILE.load(Ordering::Relaxed) {
        1 => Profile::Certus,
        _ => Profile::Default,
    }
}

pub(crate) fn active() -> bool {
    profile() != Profile::Default
}

pub(crate) fn set_cards(on: bool) {
    CARDS.store(on, Ordering::Relaxed);
    if on {
        set_profile(Profile::Certus);
    }
}

pub(crate) fn cards() -> bool {
    CARDS.load(Ordering::Relaxed)
}

pub(crate) fn apply_env() -> Result<(), String> {
    match std::env::var("FIRNC_GC_RUNTIME") {
        Ok(v) if !v.is_empty() => match parse(&v) {
            Some(p) => {
                set_profile(p);
                Ok(())
            }
            None => Err(format!("unknown FIRNC_GC_RUNTIME value '{}' (allowed: {})", v, ALLOWED)),
        },
        _ => Ok(()),
    }
}

fn modules() -> &'static [Module] {
    match profile() {
        Profile::Default => &[],
        Profile::Certus => CERTUS,
    }
}

pub(crate) fn runtime(core: &str) -> String {
    let mut s = core.to_string();
    for m in modules() {
        s = overlay::overlay(&s, m.text);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certus_chunk_header_holds_data_end() {
        let mut s = include_str!("../../../lib/gc/gc.fi").to_string();
        for m in CERTUS {
            s = overlay::overlay(&s, m.text);
        }
        let items = overlay::items(&s);
        let chunk_new = items.iter().find(|i| i.name == "__gc_chunk_new").expect("missing __gc_chunk_new");
        let body = &s[chunk_new.start..chunk_new.end];
        assert!(body.contains("*((p + 32) as *mut u64) = p + CHUNK_HEADER + block * count"), "chunk word 32 must be the data end");
        assert!(s.contains("a < __gc_ld64(ci +% 32)"), "the parallel block lookup reads the data end");
    }

    #[test]
    fn certus_overlay_keeps_core_and_adds_api() {
        let core = include_str!("../../../lib/gc/gc.fi");
        let mut s = core.to_string();
        for m in CERTUS {
            s = overlay::overlay(&s, m.text);
        }
        for f in ["fn gc_perm_seal(", "fn gc_set_step_hook(", "fn __gc_block_of_slow(", "fn gc_init("] {
            assert!(s.contains(f), "missing {}", f);
        }
        assert!(s.contains(&format!("const S_CARDON: u64 = {}", CARDON_OFF)));
        let names: Vec<&str> = overlay::items(&s).iter().map(|i| i.name).collect();
        let mut seen = std::collections::HashSet::new();
        for n in &names {
            assert!(seen.insert(*n), "duplicate item {}", n);
        }
        let mut offs = std::collections::HashMap::new();
        for line in s.lines() {
            if let Some(rest) = line.strip_prefix("const S_") {
                if let Some((name, v)) = rest.split_once(": u64 = ") {
                    if let Ok(n) = v.trim().parse::<u64>() {
                        if (2048..4096).contains(&n) {
                            assert!(offs.insert(n, name.to_string()).is_none(), "state offset {} used twice ({})", n, name);
                        }
                    }
                }
            }
        }
    }
}
