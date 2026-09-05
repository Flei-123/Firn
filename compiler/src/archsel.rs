//! **ROUND ARM-FREESTANDING — `#[arch(...)]`, one function per machine.**
//!
//! ## The problem this solves, stated exactly
//!
//! Round 80 built a second code generator and measured it against the first
//! by RUNNING the same source text on both machines. Four programs could not
//! take part, and the reason was not a gap in the code generator:
//!
//! ```firn
//! let s: u64 = asm("add rax, rcx", out("rax"), in("rax") a, in("rcx") b)
//! ```
//!
//! `add rax, rcx` is x86 text. It is not "not ported yet" — it cannot be
//! ported, because it is not a Firn expression at all; it is a line for a
//! particular assembler, sitting inside a Firn program. A language that
//! offers an inline assembler owes its user a way to say **which machine a
//! piece of source belongs to**, and up to this round Firn had none. The
//! four cases were therefore stuck at NOT SUPPORTED for a reason that lay in
//! the LANGUAGE, not in `codegen_a64.rs`.
//!
//! ## The form
//!
//! ```firn
//! #[arch(x86_64)]
//! fn add2(a: u64, b: u64) -> u64 {
//!     return asm("add rax, rcx", out("rax"), in("rax") a, in("rcx") b)
//! }
//!
//! #[arch(aarch64)]
//! fn add2(a: u64, b: u64) -> u64 {
//!     return asm("add x9, x9, x10", out("x9"), in("x9") a, in("x10") b)
//! }
//! ```
//!
//! Both are written; **exactly one is compiled.** This file is the pass that
//! throws the other away, and it runs BEFORE the type checker — which is the
//! whole point, because the checker is where register names are validated
//! against the machine (`core.rs::check_reg`). A pass that ran later would
//! have to make the checker tolerate `rax` while generating A64, and that
//! would give up the one guarantee worth having.
//!
//! ## Why on the function and not on the statement
//!
//! A statement-level `arch { ... }` was the other candidate and was not
//! taken. Three reasons, in the order they mattered:
//!
//! 1. **Overload by name falls out for free.** Two functions of the same
//!    name are a duplicate-definition error in `sema.rs` — and by the time
//!    `sema.rs` looks, one of them is gone. There is nothing to teach the
//!    name resolution, nothing to teach the caller, and a call site reads
//!    `add2(a, b)` on both machines with no marker at all.
//! 2. **A block has no value.** `let s: u64 = asm(...)` is an EXPRESSION in
//!    three of the four cases. A statement-level selector would have forced
//!    every one of them through a mutable variable assigned in two branches
//!    — more source, and a different shape from the one being ported.
//! 3. **It is the smaller change.** A new `Stmt` variant has to be handled
//!    in every pass that walks statements (nine files), and every one of
//!    those is a place where a later round can forget it. An attribute is
//!    handled here and nowhere else.
//!
//! The price is honest and worth naming: what varies must be a whole
//! function. Two machines that share ten lines and differ in one still write
//! the ten lines twice, or factor the one line into a function of its own.
//! For inline assembler — the only thing that has ever needed this — the
//! second is what one wants anyway.
//!
//! ## What it is NOT
//!
//! It is not conditional compilation in general. There is no `#[arch]` on a
//! struct, a constant or a module, no `not(...)`, no `any(...)`, and no
//! nesting. Everything in this file is one `retain` over the function list.
//! When a later round needs more, it can have more; inventing it now would
//! mean inventing a syntax nobody has used yet.

use crate::ast::{FnDecl, Program};
use crate::diag::Diags;
use crate::target::Arch;

/// The attribute's name, in one place.
pub const ATTR: &str = "arch";

/// The names a machine may be called by in the attribute. Deliberately the
/// SAME words the `--target` names are built from, so that
/// `--target=aarch64-none` and `#[arch(aarch64)]` visibly belong together.
const NAMES: &[(&str, Arch)] = &[("x86_64", Arch::X86_64), ("aarch64", Arch::Aarch64)];

/// The name of the active machine as the attribute spells it.
pub fn active_name() -> &'static str {
    match crate::target::arch() {
        Arch::X86_64 => "x86_64",
        Arch::Aarch64 => "aarch64",
    }
}

fn parse(name: &str) -> Option<Arch> {
    NAMES.iter().find(|(n, _)| *n == name).map(|(_, a)| *a)
}

/// The `#[arch(...)]` of a function, if it carries one.
fn wanted(f: &FnDecl) -> Option<&crate::ast::Attr> {
    f.attrs.iter().find(|a| a.name == ATTR)
}

/// Throw away every function that belongs to another machine.
///
/// Called from `main.rs` directly before `mono::expand`, i.e. after the
/// modules are merged, after `comptime` has appended what it generated and
/// after the `--test` runner has been built — so a generated function may
/// carry the attribute too — and before anything at all has looked at a
/// type or a register name.
pub fn select(prog: &mut Program, dg: &mut Diags) {
    let here = crate::target::arch();
    // First the names, so that a typo is an error and not a silently
    // dropped function. `#[arch(arm64)]` would otherwise remove the
    // function on BOTH machines and the program would fail at the call
    // site, a hundred lines away from the cause.
    for f in &prog.funcs {
        if let Some(a) = wanted(f) {
            let n = a.args.first().map(|s| s.as_str()).unwrap_or("");
            if parse(n).is_none() {
                dg.error_note(
                    a.span,
                    format!("unknown machine '{}' in #[arch(...)]", n),
                    format!(
                        "known are {} -- the same words the --target names are built from",
                        NAMES.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
                    ),
                );
            }
        }
    }
    if dg.has_errors() {
        return;
    }
    // Which names survive at all? Collected BEFORE the throwing away,
    // because afterwards there is nothing left to ask.
    let mut survives: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for f in &prog.funcs {
        match wanted(f) {
            None => {
                survives.insert(f.name.as_str());
            }
            Some(a) => {
                if parse(a.args.first().map(|s| s.as_str()).unwrap_or("")) == Some(here) {
                    survives.insert(f.name.as_str());
                }
            }
        }
    }
    // A name whose every definition belongs to another machine. Said once
    // per NAME -- three variants for three other machines would otherwise
    // produce three copies of the same sentence.
    let mut said: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for f in &prog.funcs {
        let a = match wanted(f) {
            Some(a) => a,
            None => continue,
        };
        if survives.contains(f.name.as_str()) || !said.insert(f.name.as_str()) {
            continue;
        }
        dg.error_note(
            a.span,
            format!(
                "'{}' has no implementation for the machine '{}'",
                f.name,
                active_name()
            ),
            format!(
                "every definition of '{}' carries an #[arch(...)] for another machine; \
                 write one for '{}' as well, or leave the attribute off the version \
                 that fits every machine",
                f.name,
                active_name()
            ),
        );
    }
    if dg.has_errors() {
        return;
    }
    prog.funcs.retain(|f| match wanted(f) {
        None => true,
        Some(a) => parse(a.args.first().map(|s| s.as_str()).unwrap_or("")) == Some(here),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(src: &str) -> (Vec<String>, String) {
        crate::prof::reset();
        crate::core::reset();
        let mut dg = Diags::new("archsel_test", src);
        let toks = crate::lexer::lex(src, &mut dg);
        let mut prog = crate::parser::parse(&toks, &mut dg);
        select(&mut prog, &mut dg);
        (prog.funcs.iter().map(|f| f.name.clone()).collect(), dg.render())
    }

    const SRC: &str = "#[arch(x86_64)]\nfn only_here() -> i32 { return 1 }\n\
                       #[arch(aarch64)]\nfn only_here() -> i32 { return 2 }\n\
                       fn both() -> i32 { return 3 }\n\
                       fn main() -> i32 { return only_here() + both() }\n";

    #[test]
    fn exactly_one_variant_survives_on_each_machine() {
        crate::target::reset();
        let (names, err) = build(SRC);
        assert!(!err.contains("error"), "{}", err);
        assert_eq!(names.iter().filter(|n| *n == "only_here").count(), 1);
        assert!(names.contains(&"both".to_string()));

        crate::target::set(crate::target::Target::Aarch64);
        let (names, err) = build(SRC);
        assert!(!err.contains("error"), "{}", err);
        assert_eq!(names.iter().filter(|n| *n == "only_here").count(), 1);
        crate::target::reset();
    }

    #[test]
    fn a_freestanding_target_is_still_its_arch() {
        // The two axes of `target.rs` really are independent: `-none`
        // changes what lies underneath, not which instructions exist.
        crate::target::set(crate::target::Target::Aarch64None);
        assert_eq!(active_name(), "aarch64");
        crate::target::set(crate::target::Target::X86_64None);
        assert_eq!(active_name(), "x86_64");
        crate::target::reset();
    }

    #[test]
    fn a_typo_is_an_error_and_not_a_silent_removal() {
        crate::target::reset();
        let (_, err) = build("#[arch(arm64)]\nfn f() -> i32 { return 1 }\nfn main() -> i32 { return f() }\n");
        assert!(err.contains("unknown machine 'arm64'"), "{}", err);
    }

    #[test]
    fn a_name_that_exists_only_for_the_other_machine_says_so() {
        crate::target::reset();
        let (_, err) = build(
            "#[arch(aarch64)]\nfn f() -> i32 { return 1 }\nfn main() -> i32 { return f() }\n",
        );
        assert!(err.contains("no implementation for the machine 'x86_64'"), "{}", err);
    }
}
