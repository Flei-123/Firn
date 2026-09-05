// SPDX-License-Identifier: GPL-2.0-only
//! **Attribute register** — the single truth about which attributes exist,
//! where they may stand and which of them really do something at stage 0.
//!
//! Firn's specification leans on attributes in many places:
//! `#[must_consume]` (SPEC §3.3, §5.1), `#[no_gc]` (§3.5.4),
//! `#[constant_time]` (§9.2), `#[unwinds]` (§5.3), `#[packed]`/`#[align(n)]`
//! (§13), `#[layout(soa)]` (DESIGN_GOALS §8), `#[abi_stable]`/`#[frozen]`
//! (DESIGN_GOALS §4), `#[hot]` (DESIGN_GOALS §9).
//!
//! They arrive at very different points in time. To keep that from ending up
//! as a thicket of scattered string comparisons, they are gathered **here**
//! in one table — with target, state of implementation and purpose.
//! `--list-attrs` prints it.
//!
//! Rule of the project: whatever is not implemented reports a **clean
//! compiler error** with line and column — never a crash and never silent
//! ignoring. A silently dropped `#[constant_time]` would be the most
//! dangerous sort of error that can exist in this language.

/// Where a given attribute may be written.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    /// only before `fn`
    Func,
    /// only before `struct` (and later `enum`)
    Type,
    /// before both
    Both,
}

impl Target {
    fn text(self) -> &'static str {
        match self {
            Target::Func => "fn",
            Target::Type => "struct",
            Target::Both => "fn, struct",
        }
    }
    fn allowed_fn(self) -> bool {
        matches!(self, Target::Func | Target::Both)
    }
    fn allowed_ty(self) -> bool {
        matches!(self, Target::Type | Target::Both)
    }
}

pub struct AttrInfo {
    pub name: &'static str,
    pub target: Target,
    /// Number of expected arguments in brackets (0 = without brackets).
    pub args: usize,
    /// Does it really do something at stage 0?
    pub implemented: bool,
    pub what: &'static str,
}

/// Every attribute the language knows.
pub const ATTRS: &[AttrInfo] = &[
    AttrInfo {
        name: "must_consume",
        target: Target::Both,
        args: 0,
        implemented: true,
        what: "result must not be discarded (SPEC 3.3, 5.1)",
    },
    AttrInfo {
        name: "no_gc",
        target: Target::Func,
        args: 0,
        implemented: true,
        what: "no collection run in this call tree (SPEC 3.5.4)",
    },
    AttrInfo {
        name: "test",
        target: Target::Func,
        args: 0,
        implemented: true,
        what: "a test case: found and run with --test (ROUND 94, testrun.rs)",
    },
    AttrInfo {
        name: "interrupt",
        target: Target::Func,
        args: 0,
        implemented: true,
        what: "interrupt entry point: save all registers, iretq (SPEC 2)",
    },
    AttrInfo {
        name: "arch",
        target: Target::Func,
        args: 1,
        implemented: true,
        what: "this definition belongs to one machine: #[arch(x86_64)] / #[arch(aarch64)] (archsel.rs)",
    },
    AttrInfo {
        name: "link_name",
        target: Target::Func,
        args: 1,
        implemented: true,
        what: "explicit C link name for 'extern fn', e.g. #[link_name(exit)] (SPEC 14.5)",
    },
    AttrInfo {
        name: "export_c",
        target: Target::Func,
        args: 0,
        implemented: true,
        what: "make a Firn function callable under its bare name from C (SPEC 14.5)",
    },
    AttrInfo {
        name: "allow_escape",
        target: Target::Func,
        args: 0,
        implemented: true,
        what: "the address of a local may leave this frame (SPEC 3.6, round 79)",
    },
    AttrInfo {
        name: "panic_handler",
        target: Target::Func,
        args: 0,
        implemented: true,
        what: "this function ends every panic: fn(msg: *u8, len: u64, a: i64, b: i64, code: u64) (SPEC 13, round 89)",
    },
    AttrInfo {
        name: "allow_fp",
        target: Target::Both,
        args: 0,
        implemented: true,
        what: "allow floating point in profile 'kernel', FPU state (SPEC 2)",
    },
    AttrInfo {
        name: "constant_time",
        target: Target::Func,
        args: 0,
        implemented: false,
        what: "no jump on secret data, checked in the code generator (SPEC 9.2)",
    },
    AttrInfo {
        name: "unwinds",
        target: Target::Func,
        args: 0,
        implemented: false,
        what: "may raise or pass on 'throw' (SPEC 5.3)",
    },
    AttrInfo {
        name: "packed",
        target: Target::Type,
        args: 0,
        implemented: false,
        what: "arrange fields without padding bytes (SPEC 13)",
    },
    AttrInfo {
        name: "align",
        target: Target::Type,
        args: 1,
        implemented: false,
        what: "force alignment, e.g. #[align(64)] (SPEC 13)",
    },
    AttrInfo {
        name: "layout",
        target: Target::Type,
        args: 1,
        implemented: false,
        what: "choose the arrangement, e.g. #[layout(soa)] (DESIGN_GOALS 8)",
    },
    AttrInfo {
        name: "no_move",
        target: Target::Type,
        args: 0,
        implemented: false,
        what: "no longer movable after construction (DESIGN_GOALS 6)",
    },
    AttrInfo {
        name: "abi_stable",
        target: Target::Both,
        args: 1,
        implemented: false,
        what: "stable ABI across component boundaries (DESIGN_GOALS 4)",
    },
    AttrInfo {
        name: "frozen",
        target: Target::Type,
        args: 0,
        implemented: false,
        what: "layout frozen, in exchange embeddable again (DESIGN_GOALS 4)",
    },
    AttrInfo {
        name: "hot",
        target: Target::Func,
        args: 0,
        implemented: false,
        what: "exchangeable at run time (DESIGN_GOALS 9, no date)",
    },
];

pub fn search(name: &str) -> Option<&'static AttrInfo> {
    ATTRS.iter().find(|a| a.name == name)
}

/// Does the attribute fit this target?
pub fn fits(a: &AttrInfo, on_func: bool) -> bool {
    if on_func {
        a.target.allowed_fn()
    } else {
        a.target.allowed_ty()
    }
}

/// Closest known spelling (Levenshtein distance <= 3), for suggestions.
pub fn proposal(name: &str) -> Option<&'static str> {
    let mut best: Option<(usize, &'static str)> = None;
    for a in ATTRS {
        let d = distance(name, a.name);
        if d <= 3 && best.map(|(bd, _)| d < bd).unwrap_or(true) {
            best = Some((d, a.name));
        }
    }
    best.map(|(_, n)| n)
}

fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut line: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut before = line[0];
        line[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let replace = before + usize::from(ca != cb);
            before = line[j + 1];
            line[j + 1] = replace.min(line[j] + 1).min(line[j + 1] + 1);
        }
    }
    line[b.len()]
}

/// The register as text (for `--list-attrs`).
pub fn attrs_text() -> String {
    let mut out = String::new();
    out.push_str("Attribute\n\n");
    out.push_str("NAME            TARGET       ARGS  STAGE 0     PURPOSE\n");
    for a in ATTRS {
        out.push_str(&format!(
            "{:<15} {:<12} {:<5} {:<11} {}\n",
            a.name,
            a.target.text(),
            a.args,
            if a.implemented { "implemented" } else { "error" },
            a.what
        ));
    }
    out.push_str(
        "\n'error' means: the attribute is known and planned, but is\n",
    );
    out.push_str(
        "rejected in stage 0 with a clear message instead of silently ignored.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_is_unique() {
        for (i, a) in ATTRS.iter().enumerate() {
            assert!(
                ATTRS.iter().skip(i + 1).all(|b| b.name != a.name),
                "duplicate attribute name: {}",
                a.name
            );
        }
    }

    #[test]
    fn only_must_consume_is_implemented() {
        // Once one more attribute is implemented, this test MUST be adjusted
        // — that forces README and SPEC to be dragged along.
        // State of round "hardening test 2": additionally #[no_gc] (SPEC 3.5.4,
        // checked by nogc.rs, test programs tests/54x_no_gc_*.fi and
        // tests/neg/nogc_*.fi).
        // Round 52: plus #[interrupt] and #[allow_fp] (SPEC 2, core.rs/prof.rs).
        // Round 79: plus #[allow_escape] -- the way out of the escape analysis
        // (escape.rs, docs/ROUND79.md 3).
        // Round 89: plus #[panic_handler] -- the program's own ending for
        // every run time abort (SPEC 13, panic_rt.rs, docs/ROUND89.md).
        // Round 94: plus #[test] -- the test runner finds its cases by it
        // (testrun.rs, lib/test/runner.fi, docs/ROUND94.md).
        // Round ARM-FREESTANDING: plus #[arch(...)] -- this definition
        // belongs to one machine, and the others are thrown away before the
        // type checker runs (archsel.rs, docs/ROUND-ARM-FREESTANDING.md 5).
        let u: Vec<&str> = ATTRS.iter().filter(|a| a.implemented).map(|a| a.name).collect();
        assert_eq!(
            u,
            vec![
                "must_consume",
                "no_gc",
                "test",
                "interrupt",
                "arch",
                "link_name",
                "export_c",
                "allow_escape",
                "panic_handler",
                "allow_fp"
            ]
        );
    }

    #[test]
    fn not_implemented_attribute_report_next_a_error() {
        // Counter-check to tests/neg/attr_not_implemented.fi: the remaining
        // attributes stay rejected, nothing is silently ignored.
        for name in ["constant_time", "unwinds", "packed", "align", "layout", "no_move", "hot"] {
            let a = search(name).expect(name);
            assert!(!a.implemented, "{} unexpectedly counts as implemented", name);
        }
    }

    #[test]
    fn proposal_finds_typo() {
        assert_eq!(proposal("must_consum"), Some("must_consume"));
        assert_eq!(proposal("no_gk"), Some("no_gc"));
        assert_eq!(proposal("voellig_anders_xyz"), None);
    }

    #[test]
    fn targets_match() {
        let mc = search("must_consume").expect("must_consume");
        assert!(fits(mc, true) && fits(mc, false));
        let p = search("packed").expect("packed");
        assert!(!fits(p, true) && fits(p, false));
        let ng = search("no_gc").expect("no_gc");
        assert!(fits(ng, true) && !fits(ng, false));
    }
}
