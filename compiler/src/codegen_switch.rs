// SPDX-License-Identifier: GPL-2.0-only
//! Code generation for `Term::Switch` (SPEC §6.3, `P4`).
//!
//! INTERFACE (fixed, called by `codegen_x86.rs`):
//!   `pub(crate) fn emit_switch(e, f, fr, term) -> Result<(), String>`
//!
//! Two methods:
//!  * **jump table** — as soon as at least `MIN_TABLE_CASES` labels exist and
//!    the density `cases.len() * 100 / (max - min + 1)` reaches at least
//!    `MIN_DENSITY` percent. The table sits in `.rodata`, the jump is
//!    a single indirect `jmp qword ptr [...]`; outside of `[min, max]` control
//!    goes to `default`.
//!  * **comparison chain** — otherwise (few or widely scattered labels).
//!
//! Both methods behave alike; the optimizer changes nothing about that.

use crate::codegen_x86::{block_label, load_ext, reg, Emitter, Frame};
use crate::fir::{FTy, Func, Term};

/// Where does the value branched over come from?
///
/// **Round 51.** Formerly `emit_switch` could read the value only from the
/// frame. The register path (`regalloc.rs`) therefore had to write it there
/// first, although it already sat in a register:
///
/// ```text
/// mov %r12d,%r9d          ; state into a scratch register
/// mov %r9,%rax
/// mov %rax,-0x260(%rbp)   ; into the frame just for emit_switch
/// mov -0x260(%rbp),%eax   ; and right back out again
/// cmp $0x48,%eax
/// ```
///
/// In the tokenizer that is the state dispatch per character: 5.109.380
/// runs times two superfluous memory accesses.
pub(crate) enum ValueSource<'a> {
    /// Base path: the value lies in its frame slot.
    Frame(&'a Frame),
    /// Register path: the caller loads the value to `rax` itself,
    /// widened to the given width.
    ///
    /// **Guarantee of the caller:** the function ALWAYS emits at least one
    /// write to `eax`/`rax`. That is the ground on which the table below may
    /// do without `mov eax, eax`.
    Loaded(&'a dyn Fn(&mut Emitter, u32)),
}

/// Width at which the value is compared and indexed.
pub(crate) fn switch_bits(ty: FTy) -> u32 {
    if ty.bits() > 32 {
        64
    } else {
        32
    }
}

/// from this many cases onwards a table pays off
pub(crate) const MIN_TABLE_CASES: usize = 8;
/// minimum density as percent
pub(crate) const MIN_DENSITY: usize = 40;
/// Upper bound for the size of a table (entries), so that frugal programs
/// do not get much `.rodata` unintentionally.
const MAX_TABLE_ENTRIES: i128 = 65536;

pub(crate) fn emit_switch(
    e: &mut Emitter,
    f: &Func,
    q: ValueSource,
    term: &Term,
) -> Result<(), String> {
    let (val, ty, cases, default) = match term {
        Term::Switch { val, ty, cases, default } => (*val, *ty, cases, *default),
        _ => return Err("internal error: emit_switch without switch".to_string()),
    };
    // SPEC §9.1: branching over a secret value is forbidden.
    if f.constant_time && f.is_secret(val) {
        return Err(format!(
            "#[constant_time]: switch in '{}' depends on a secret value (%{})",
            f.name, val
        ));
    }
    if ty == FTy::Void {
        return Err("internal error: switch over void".to_string());
    }
    if cases.is_empty() {
        e.line(&format!("jmp {}", block_label(&f.name, default)));
        return Ok(());
    }
    let bits = switch_bits(ty);
    match q {
        ValueSource::Frame(fr) => load_ext(e, fr, "rax", val, ty, bits),
        ValueSource::Loaded(load) => load(e, bits),
    }

    if let Some((min, max)) = table_range(cases) {
        emit_table(e, f, cases, default, min, max, bits);
        return Ok(());
    }
    for (k, target) in cases.iter() {
        e.line(&format!("cmp {}, {}", reg("rax", bits), *k as i64));
        e.line(&format!("je {}", block_label(&f.name, *target)));
    }
    e.line(&format!("jmp {}", block_label(&f.name, default)));
    Ok(())
}

/// Does a jump table pay off? Yields `[min, max]` of the labels.
fn table_range(cases: &[(i128, crate::fir::BlockId)]) -> Option<(i128, i128)> {
    if cases.len() < MIN_TABLE_CASES {
        return None;
    }
    let min = cases.iter().map(|(k, _)| *k).min()?;
    let max = cases.iter().map(|(k, _)| *k).max()?;
    let extent = max - min + 1;
    if extent <= 0 || extent > MAX_TABLE_ENTRIES {
        return None;
    }
    let density = (cases.len() as i128) * 100 / extent;
    if density < MIN_DENSITY as i128 {
        return None;
    }
    Some((min, max))
}

fn emit_table(
    e: &mut Emitter,
    f: &Func,
    cases: &[(i128, crate::fir::BlockId)],
    default: crate::fir::BlockId,
    min: i128,
    max: i128,
    bits: u32,
) {
    let extent = max - min + 1;
    let label = table_label(e, &f.name);
    let dflt = block_label(&f.name, default);

    // index = value - min; outside of [0, width) control goes to default.
    if bits == 32 {
        if min != 0 {
            e.line(&format!("sub eax, {}", min as i64));
        }
        e.line(&format!("cmp eax, {}", (extent - 1) as i64));
        e.line(&format!("ja {}", dflt));
        // NO `mov eax, eax` (round 51): on x86-64 EVERY write to a 32-bit
        // register zeroes the upper 32 bits. Up to here rax has been
        // written exactly that way for sure — either by `load_ext`
        // (each of its branches writes `eax`: `mov`, `movzx`, `movsx`,
        // `movsxd`), by the guarantee of `ValueSource::Loaded`, or by the
        // `sub eax, min` right above. The index in rax is thereby
        // already zero extended.
    } else {
        if min != 0 {
            e.line(&format!("mov rcx, {}", min as i64));
            e.line("sub rax, rcx");
        }
        e.line(&format!("mov rcx, {}", (extent - 1) as i64));
        e.line("cmp rax, rcx");
        e.line(&format!("ja {}", dflt));
    }
    e.line(&format!("lea rdx, [rip + {}]", label));
    e.line("jmp qword ptr [rdx + rax*8]");

    // table in .rodata; missing labels point to the default branch.
    e.raw(".section .rodata");
    e.raw(".align 8");
    e.raw(&format!("{}:", label));
    let mut i = 0usize;
    let mut k = min;
    while k <= max {
        while i < cases.len() && cases[i].0 < k {
            i += 1;
        }
        if i < cases.len() && cases[i].0 == k {
            e.raw(&format!(".quad {}", block_label(&f.name, cases[i].1)));
        } else {
            e.raw(&format!(".quad {}", dflt));
        }
        k += 1;
    }
    e.raw(".text");
}

/// Unique label for a table inside the output.
fn table_label(e: &Emitter, fname: &str) -> String {
    let base = format!(".Ltbl_{}", fname);
    let n = e.out.matches(&format!("{}_", base)).count();
    format!("{}_{}", base, n)
}

#[cfg(test)]
mod tests {
    use crate::codegen_x86::emit;
    use crate::fir::{FTy, Func, Module, Op, Term};

    /// Few labels: comparison chain.
    #[test]
    fn switch_generated_compare_chain() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let v = f.push(0, FTy::I32, Op::Const(2));
        let b1 = f.add_block();
        let b2 = f.add_block();
        let bd = f.add_block();
        f.set_term(0, Term::Switch { val: v, ty: FTy::I32, cases: vec![(1, b1), (2, b2)], default: bd });
        let c1 = f.push(b1, FTy::I32, Op::Const(10));
        f.set_term(b1, Term::Ret(Some(c1)));
        let c2 = f.push(b2, FTy::I32, Op::Const(20));
        f.set_term(b2, Term::Ret(Some(c2)));
        let cd = f.push(bd, FTy::I32, Op::Const(30));
        f.set_term(bd, Term::Ret(Some(cd)));
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        assert!(asm.contains("je .Lmain__bb1"), "{}", asm);
        assert!(asm.contains("je .Lmain__bb2"), "{}", asm);
        assert!(asm.contains("jmp .Lmain__bb3"), "{}", asm);
        assert!(!asm.contains("jmp qword ptr"), "unexpected table:\n{}", asm);
    }

    /// Many dense labels: jump table in `.rodata` with an indirect jump.
    #[test]
    fn denser_switch_generated_jump_table() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let v = f.push(0, FTy::I32, Op::Const(3));
        let mut cases = Vec::new();
        for i in 0..12i128 {
            let b = f.add_block();
            let c = f.push(b, FTy::I32, Op::Const(i));
            f.set_term(b, Term::Ret(Some(c)));
            cases.push((i, b));
        }
        let bd = f.add_block();
        let cd = f.push(bd, FTy::I32, Op::Const(99));
        f.set_term(bd, Term::Ret(Some(cd)));
        f.set_term(0, Term::Switch { val: v, ty: FTy::I32, cases, default: bd });
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        assert!(asm.contains("jmp qword ptr [rdx + rax*8]"), "{}", asm);
        assert!(asm.contains(".section .rodata"), "{}", asm);
        assert!(asm.contains(".quad .Lmain__bb5"), "{}", asm);
        // no comparison chain any more
        assert!(!asm.contains("je .Lmain__bb5"), "{}", asm);
    }

    /// Widely scattered labels: no table (density too low).
    #[test]
    fn sparsamer_switch_stays_chain() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let v = f.push(0, FTy::I32, Op::Const(3));
        let mut cases = Vec::new();
        for i in 0..10i128 {
            let b = f.add_block();
            let c = f.push(b, FTy::I32, Op::Const(i));
            f.set_term(b, Term::Ret(Some(c)));
            cases.push((i * 1000, b));
        }
        let bd = f.add_block();
        let cd = f.push(bd, FTy::I32, Op::Const(99));
        f.set_term(bd, Term::Ret(Some(cd)));
        f.set_term(0, Term::Switch { val: v, ty: FTy::I32, cases, default: bd });
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        assert!(!asm.contains("jmp qword ptr"), "{}", asm);
        assert!(asm.contains("je .Lmain__bb1"), "{}", asm);
    }

    /// PROOF (SPEC §6.3, `P4`): the state machine with 32 states at
    /// `tests/230_state_machine.fi` gets a real jump table —
    /// one indirect jump through `.rodata`, no chain of 32 `cmp`.
    #[test]
    fn jump_table_at_30_states() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/230_state_machine.fi");
        let src = std::fs::read_to_string(path).expect("test program missing");
        let mut dg = crate::diag::Diags::new(path, &src);
        let toks = crate::lexer::lex(&src, &mut dg);
        let mut prog = crate::parser::parse(&toks, &mut dg);
        crate::mono::expand(&mut prog, &mut dg);
        let info = crate::sema::check(&prog, &mut dg).expect("type check");
        let mut m = crate::lower::lower(&prog, &info, &mut dg).expect("lowering");
        assert!(!dg.has_errors(), "{}", dg.render());
        crate::opt::optimize(&mut m);
        // ROUND 92: exactly what `main.rs` does between the optimizer and
        // the code generator -- a backend never sees a phi.
        crate::phi::eliminate(&mut m).expect("phi elimination");
        let asm = emit(&m).expect("codegen");
        assert!(asm.contains("jmp qword ptr ["), "no jump table:\n{}", asm);
        assert!(asm.contains(".section .rodata"), "table not in .rodata:\n{}", asm);
        let entries = asm.matches(".quad .Lmain__bb").count();
        assert!(entries >= 32, "only {} table entries", entries);
        // ROUND 72: `sum % 251 as i32` (line 48 of the source) is now a
        // CHECKED `%` -- the signed `MIN % -1` special case (SPEC section
        // 13, `L9`) adds its own two `cmp`s (`panic_rt.rs::emit_checked_div`)
        // on top of the switch's own bounds check and the loop condition,
        // none of which have anything to do with whether the match itself
        // became a jump table (checked above by `jmp qword ptr [`,
        // `.rodata` and >=32 table entries) -- raised from 4 to 6, not
        // loosened into meaninglessness: still far short of one `cmp` per
        // state, which is what a comparison CHAIN instead of a table would
        // produce here.
        let compare = asm.lines().filter(|l| l.trim().starts_with("cmp ")).count();
        assert!(compare <= 6, "{} comparisons instead of table:\n{}", compare, asm);
    }

    /// `select` must become a `cmov` — never a jump (SPEC §9.2).
    #[test]
    fn select_becomes_cmov_and_never_in_jump() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let c = f.push(0, FTy::Bool, Op::Const(1));
        let a = f.push(0, FTy::I32, Op::Const(7));
        let b = f.push(0, FTy::I32, Op::Const(9));
        let s = f.push(0, FTy::I32, Op::Select { cond: c, a, b });
        f.set_term(0, Term::Ret(Some(s)));
        let asm = emit(&Module { funcs: vec![f] }).expect("codegen");
        assert!(asm.contains("cmovnz"), "{}", asm);
        let body = asm.split("main:").nth(1).unwrap();
        for line in body.lines() {
            let l = line.trim();
            assert!(!(l.starts_with('j') && !l.starts_with("jmp")), "conditional jump: {}", l);
        }
    }
}
