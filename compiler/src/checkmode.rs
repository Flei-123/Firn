// SPDX-License-Identifier: GPL-2.0-only
//! **Round 72** — which build level checks integer arithmetic (SPEC §13,
//! `L9`)?
//!
//! `opt::Level` already exists and already has a variant named
//! `ReleaseSafe` — the promise was in the name from the day it was written.
//! What was missing is this: nothing anywhere ever ASKED the level whether
//! arithmetic should be checked, because `lower.rs` (where `+ - *` become
//! FIR) never saw the level at all — only `opt.rs`'s FIR-to-FIR passes did.
//! This file is the missing question, asked once per compilation and
//! answered the same way `prof.rs` answers "which profile": a `thread_local`
//! set at the top of `main.rs::run` before parsing starts, read wherever
//! `lower.rs` decides whether `a + b` becomes `Op::Bin` or
//! `Op::CheckedBin`.
//!
//! ## The rule (SPEC §13, updated by this round)
//!
//! | level | checks `+ - * /` and narrowing `as` |
//! |---|---|
//! | `dev` / `dev-fast` | **yes** |
//! | `release-safe` | **yes** — this is the bug this round fixes |
//! | `release-fast` | no (wraps; `+%`/`+\|` exist for when that is wanted) |
//!
//! `dev`/`dev-fast` checking was already true in spirit (SPEC §13 said
//! `--debug` checks, and there never was a `--debug` flag — `dev`/`dev-fast`
//! are the levels that stand for it, see `docs/ROUND72.md`). `release-safe`
//! checking was the one line that was FALSE until this round: the level
//! existed, ran every FIR optimization pass, and the arithmetic underneath
//! it was exactly as unchecked as `release-fast`.

use std::cell::Cell;

thread_local! {
    static CHECKED: Cell<bool> = const { Cell::new(true) };
}

/// Sets the mode for the rest of this compilation, from the resolved
/// `opt::Level` (`main.rs::run`, before parsing).
pub fn set_from_level(level: crate::opt::Level) {
    let checked = !matches!(level, crate::opt::Level::ReleaseFast);
    CHECKED.with(|c| c.set(checked));
}

/// Should `+ - * /` and a narrowing `as` be checked at THIS point in the
/// lowering? `lower.rs` asks this once per arithmetic expression.
pub fn is_checked() -> bool {
    CHECKED.with(|c| c.get())
}

/// Resets to the default (checked) — only between compilations of the SAME
/// process (module tests, `--package`, which never call `set_from_level`
/// for every one of several compiled units).
#[cfg(test)]
pub(crate) fn reset() {
    CHECKED.with(|c| c.set(true));
}
