//! **ROUND FIRN-ENV** — environment variables AT BUILD TIME, as a value of
//! the language.
//!
//! ## What for
//!
//! A branded build. In Rust that reads
//!
//! ```text
//! pub const NAME: &str = match option_env!("FV_BRAND_NAME") {
//!     Some(s) => s,
//!     None => "FreeViewer",
//! };
//! ```
//!
//! and in Firn it read, until this round, `tools/marke-einsetzen.py`: a
//! script that wrote the values into a COPY of the source text in `/tmp`
//! before translating. Its own comment said "Firn knows no `option_env!`, so
//! the build does it". This file is the answer: the build does not do it,
//! the language does.
//!
//! ## The two intrinsics
//!
//! | spelling | yields | if not set |
//! |---|---|---|
//! | `__env_or("FV_BRAND_NAME", "FreeViewer")` | `str` | the second argument |
//! | `__env_has("FV_BRAND_NAME")` | `bool` | `false` |
//!
//! They follow the shape the language already uses for `__v128_*`
//! (`simd.rs`): an intrinsic FUNCTION with a name nothing else can collide
//! with, not an operator and nothing implicit. Both arguments have to be
//! text literals — an intrinsic that took a computed name would need a
//! second, half interpreter, and `comptime` is where that belongs.
//!
//! ## Where it happens: in the PARSER
//!
//! The parser turns `__env_or(a, b)` into exactly the node a hand written
//! text literal produces (`ExprKind::Text` over the array literal of its
//! octets, `parser.rs::primary`). Everything after the parser therefore
//! sees no difference to a literal, and that answers three demands at once:
//!
//!   * the value works in `const`, in `static`, in an initialisation and in
//!     an interpolation, without a single one of those places learning a new
//!     case;
//!   * a program that RUNS never asks the environment again — the octets
//!     stand in the binary, exactly like a literal's;
//!   * `firnc1` can do the same thing at the same place
//!     (`lib/firnc1/parser.fi`), so `--emit=ast-canon` prints the same text
//!     on both sides (`tools/parser_compare.sh`).
//!
//! ## The limits, and why every one of them is here
//!
//! A translator that writes ARBITRARY environment into the binary is a way
//! to get a build machine's secrets into a program that gets shipped. Hence:
//!
//!  1. **Allow list.** A name is only read if it starts with a permitted
//!     prefix. Without any option that is `FIRN_` alone; the build adds its
//!     own with `--env-allow=<prefix>`. A name outside the list is an ERROR
//!     — always, whether the variable happens to be set or not. That
//!     matters: an error that depends on the environment would be a second
//!     way for two builds to differ.
//!  2. **Name shape.** `A-Z`, `0-9`, `_`, at most [`MAX_NAME`] octets. Not
//!     because lower case would be technically hard, but because
//!     `__env_or("path", …)` next to `PATH` is a trap.
//!  3. **Value.** At most [`MAX_VALUE`] octets, valid UTF-8, no control
//!     characters. Too long or not UTF-8 is an error and not a silent
//!     truncation: a brand name cut in half is worse than a build that
//!     stops.
//!  4. **Log.** `--env-log` prints every read with its value and where it
//!     came from. Without it nothing is printed — the round asked for "on
//!     request", and a line on stderr per build would land in every
//!     comparison of compiler output there is.
//!
//! ## The fixpoint
//!
//! `bin/firnc1.fi` uses neither intrinsic and has no allow list beyond the
//! default, so the environment cannot reach the self compilation at all:
//! stage 2 and stage 3 stay character identical whatever is set. That is
//! not luck, it is the reason the allow list is empty by default.

use std::cell::RefCell;

/// The spelling of the two intrinsics.
pub(crate) const FN_OR: &str = "__env_or";
pub(crate) const FN_HAS: &str = "__env_has";

/// Longest permitted variable NAME, in octets.
pub const MAX_NAME: usize = 64;
/// Longest permitted VALUE, in octets.
pub const MAX_VALUE: usize = 4096;
/// The one prefix that is permitted without `--env-allow`.
pub const DEFAULT_ALLOW: &str = "FIRN_";

/// One line of the manifest: which variable was read, with which value, and
/// did it come from the environment or from the written default?
#[derive(Clone)]
pub struct Note {
    pub name: String,
    pub value: String,
    pub from_env: bool,
    /// `__env_has` asked only WHETHER it is set; the manifest says so with
    /// `?` instead of `=`, so that a `false` cannot be read as a value.
    pub has: bool,
}

#[derive(Default)]
struct Config {
    /// Permitted prefixes. `configure` always puts [`DEFAULT_ALLOW`] first.
    allow: Vec<String>,
    log: bool,
    notes: Vec<Note>,
}

thread_local! {
    static CFG: RefCell<Config> = RefCell::new(Config {
        allow: vec![DEFAULT_ALLOW.to_string()],
        log: false,
        notes: Vec::new(),
    });
}

/// Sets the allow list and the log for the rest of this compilation
/// (`main.rs::run`, before the first file is parsed). The default prefix is
/// always in the list; `--env-allow` only ever adds.
pub fn configure(extra: &[String], log: bool) {
    CFG.with(|c| {
        let mut c = c.borrow_mut();
        c.allow.clear();
        c.allow.push(DEFAULT_ALLOW.to_string());
        for a in extra {
            // A comma separated list is allowed too, so that one option can
            // carry several prefixes: `--env-allow=FV_,OS_`.
            for part in a.split(',') {
                let part = part.trim();
                if !part.is_empty() && !c.allow.iter().any(|x| x == part) {
                    c.allow.push(part.to_string());
                }
            }
        }
        c.log = log;
        c.notes.clear();
    })
}

/// Only between two compilations within the SAME process (module tests).
#[cfg(test)]
pub(crate) fn reset() {
    configure(&[], false);
}

/// Is this the name of one of the two intrinsics?
pub(crate) fn is_env_call(name: &str) -> bool {
    name == FN_OR || name == FN_HAS
}

/// The permitted prefixes, for the message that names them.
fn allow_text() -> String {
    CFG.with(|c| c.borrow().allow.join(", "))
}

/// Checks the SHAPE of a name and its prefix. `Ok(())` means: this name may
/// be looked up. The answer does not depend on the environment.
pub fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("the name of an environment variable must not be empty".to_string());
    }
    if name.len() > MAX_NAME {
        return Err(format!(
            "the name of an environment variable is at most {} octets long, this one has {}",
            MAX_NAME,
            name.len()
        ));
    }
    for (i, b) in name.bytes().enumerate() {
        let ok = b.is_ascii_uppercase() || b == b'_' || (i > 0 && b.is_ascii_digit());
        if !ok {
            return Err(format!(
                "'{}' is no permitted name for an environment variable — allowed are 'A'-'Z', \
'0'-'9' and '_', and a digit not in first place",
                name
            ));
        }
    }
    let allowed = CFG.with(|c| {
        c.borrow()
            .allow
            .iter()
            .any(|p| !p.is_empty() && name.starts_with(p.as_str()))
    });
    if !allowed {
        return Err(format!(
            "the environment variable '{}' may not be read here (permitted prefixes: {}) — \
add '--env-allow=<prefix>' to the build if that is intended",
            name,
            allow_text()
        ));
    }
    Ok(())
}

/// Checks a VALUE that came out of the environment.
pub fn check_value(name: &str, v: &[u8]) -> Result<(), String> {
    if v.len() > MAX_VALUE {
        return Err(format!(
            "the value of '{}' is {} octets long, at most {} are permitted",
            name,
            v.len(),
            MAX_VALUE
        ));
    }
    for b in v {
        if *b < 0x20 || *b == 0x7F {
            return Err(format!(
                "the value of '{}' contains a control character (0x{:02X}); a build time \
constant has to be printable",
                name, b
            ));
        }
    }
    if std::str::from_utf8(v).is_err() {
        return Err(format!("the value of '{}' is no valid UTF-8", name));
    }
    Ok(())
}

/// The whole lookup: name checked, environment asked, value checked.
///
/// `Ok(None)` means "permitted, but not set" — that is where the written
/// default comes in, and it is NOT an error.
pub fn lookup(name: &str) -> Result<Option<Vec<u8>>, String> {
    check_name(name)?;
    let raw = match std::env::var_os(name) {
        Some(v) => v,
        None => return Ok(None),
    };
    // `into_encoded_bytes` and not `into_string`: what an octet sequence is
    // has to be decided by one rule (`check_value`), not by two.
    let bytes = raw.into_encoded_bytes();
    check_value(name, &bytes)?;
    Ok(Some(bytes))
}

/// Records one read of `__env_or` for the manifest of `--env-log`.
pub fn note(name: &str, value: &[u8], from_env: bool) {
    CFG.with(|c| {
        c.borrow_mut().notes.push(Note {
            name: name.to_string(),
            value: String::from_utf8_lossy(value).into_owned(),
            from_env,
            has: false,
        })
    })
}

/// Records one read of `__env_has`.
pub fn note_has(name: &str, set: bool) {
    CFG.with(|c| {
        c.borrow_mut().notes.push(Note {
            name: name.to_string(),
            value: if set { "true".to_string() } else { "false".to_string() },
            from_env: set,
            has: true,
        })
    })
}

/// Was `--env-log` given?
pub fn log_wanted() -> bool {
    CFG.with(|c| c.borrow().log)
}

/// The manifest: every read, in the order it happened. Empty when nothing
/// was read — a build that asked for nothing says so with one line.
pub fn manifest() -> String {
    CFG.with(|c| {
        let c = c.borrow();
        if c.notes.is_empty() {
            return "env: no build time environment variable was read\n".to_string();
        }
        let mut s = String::new();
        for n in &c.notes {
            if n.has {
                s.push_str(&format!("env: {} ? {}\n", n.name, n.value));
            } else {
                s.push_str(&format!(
                    "env: {} = \"{}\" ({})\n",
                    n.name,
                    n.value,
                    if n.from_env { "environment" } else { "default" }
                ));
            }
        }
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_allow_is_only_firn() {
        reset();
        assert!(check_name("FIRN_X").is_ok());
        assert!(check_name("FV_BRAND_NAME").is_err());
        assert!(check_name("PATH").is_err());
    }

    #[test]
    fn env_allow_opens_exactly_one_prefix() {
        configure(&["FV_".to_string()], false);
        assert!(check_name("FV_BRAND_NAME").is_ok());
        assert!(check_name("FW_BRAND_NAME").is_err());
        reset();
    }

    #[test]
    fn a_comma_separated_list_counts_as_several() {
        configure(&["FV_,OS_".to_string()], false);
        assert!(check_name("FV_A").is_ok());
        assert!(check_name("OS_A").is_ok());
        reset();
    }

    #[test]
    fn shape_of_the_name() {
        configure(&["X".to_string()], false);
        assert!(check_name("X_A9").is_ok());
        assert!(check_name("x_a").is_err(), "lower case");
        assert!(check_name("X-A").is_err(), "hyphen");
        assert!(check_name(&"X".repeat(MAX_NAME + 1)).is_err(), "too long");
        reset();
    }

    #[test]
    fn control_characters_and_length_of_the_value() {
        assert!(check_value("A", b"FreeViewer").is_ok());
        assert!(check_value("A", b"a\nb").is_err());
        assert!(check_value("A", b"a\0b").is_err());
        assert!(check_value("A", &vec![b'x'; MAX_VALUE + 1]).is_err());
        assert!(check_value("A", "Öffentlich".as_bytes()).is_ok());
        assert!(check_value("A", &[0xFFu8, 0xFE]).is_err(), "not UTF-8");
    }
}
