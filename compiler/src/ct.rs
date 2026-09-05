// SPDX-License-Identifier: GPL-2.0-only
//! Constant-time primitives (SPEC §9.2/§9.3) — `select`, `barrier`,
//! `secure_zero`.
//!
//! These three primitives are the part of SPEC §9 that does something solid
//! today WITHOUT the type qualifier `secret[T]`, and that the code generator
//! already knows (`fir::Op::Select`, `Op::Barrier`, `Op::SecureZero`):
//!
//! * `select(cond, a, b)` — data independent choice, becomes `cmov` at the
//!   backend. No pass may turn it into a branch (SPEC §9.2); `mem2reg` and
//!   `opt` therefore treat `Op::Select` as untouchable.
//! * `barrier(x)` — opaque barrier: hands `x` back unchanged, yet counts as
//!   impenetrable for every pass (no CSE, no constant folding across the
//!   barrier).
//! * `secure_zero(ptr, count)` — zeroes `count` bytes from `ptr` onwards and
//!   NEVER counts as dead code (SPEC §9.3, `C3`).
//!
//! **Deliberately not here yet:** `secret[T]` as a type qualifier, the
//! spreading of that marker through expressions, `declassify` and the effect
//! of `#[constant_time]`. As long as no `secret` values exist, the check in
//! the code generator (`f.constant_time && f.is_secret(cond)`) is present
//! but unfed — which is why `attrs.rs` keeps `#[constant_time]` listed as
//! *not implemented*, reporting a clean error. Recorded by
//! SPEC §14.1.
//!
//! `barrier(inout x)`/`secure_zero(inout buf)` from SPEC §9 need `inout`,
//! which stage 0 lacks; the stage 0 form takes the value, or a
//! pointer plus length. That too is written down in SPEC §14.1.

use crate::ast::Expr;
use crate::diag::Span;
use crate::fir::{FTy, Op, Val};
use crate::lower::Lower;
use crate::sema::Checker;
use crate::types::Type;

/// Names of the builtin primitives.
pub(crate) const SELECT: &str = "select";
pub(crate) const BARRIER: &str = "barrier";
pub(crate) const SECURE_ZERO: &str = "secure_zero";

/// Is this spelling the name of a builtin constant-time primitive?
pub(crate) fn is_ct_call(name: &str) -> bool {
    matches!(name, SELECT | BARRIER | SECURE_ZERO)
}

/// May `select`/`barrier` work on a value of this type?
/// Scalar types only: integer, `bool`, pointer.
fn is_scalar(t: &Type) -> bool {
    t.is_concrete_int() || *t == Type::Bool || t.is_ptr()
}

// ----------------------------------------------------------------- Type phase

/// Hook from `sema::call`. Yields `None` if the spelling is no primitive or
/// if the program contains a function of the same spelling — that one wins.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Option<Type> {
    if !is_ct_call(name) || ck.fns.contains_key(name) {
        return None;
    }
    Some(match name {
        SELECT => check_select(ck, args, nspan, espan),
        BARRIER => check_barrier(ck, args, nspan, espan),
        _ => check_secure_zero(ck, args, nspan, espan),
    })
}

/// Check the expected argument count; on a mismatch the arguments present
/// are still typed, so that no ExprId is left without a type.
fn digit_count(ck: &mut Checker, name: &str, args: &[Expr], should: usize, nspan: Span) -> bool {
    if args.len() == should {
        return true;
    }
    for a in args {
        ck.type_out_expr(a);
    }
    ck.dg.error_note(
        nspan,
        format!(
            "'{}' expects {} argument(s), found {}",
            name,
            should,
            args.len()
        ),
        match name {
            SELECT => "call: select(condition, a, b)",
            BARRIER => "call: barrier(value)",
            _ => "call: secure_zero(pointer, byte_count)",
        },
    );
    false
}

fn check_select(ck: &mut Checker, args: &[Expr], nspan: Span, espan: Span) -> Type {
    if !digit_count(ck, SELECT, args, 3, nspan) {
        return Type::Error;
    }
    let ct = ck.expr(&args[0], Some(&Type::Bool));
    if !ct.is_error() && ct != Type::Bool {
        ck.dg.error(
            args[0].span,
            format!(
                "the condition of 'select' must be bool, found {}",
                ck.tcx.name_of(&ct)
            ),
        );
    }
    let ta = ck.expr(&args[1], None);
    let tb = ck.expr(&args[2], Some(&ta));
    if ta.is_error() || tb.is_error() {
        return Type::Error;
    }
    if !is_scalar(&ta) {
        ck.dg.error_note(
            args[1].span,
            format!(
                "'select' only works on scalar values, found {}",
                ck.tcx.name_of(&ta)
            ),
            "allowed are integer, bool and pointer types",
        );
        return Type::Error;
    }
    if ta != tb {
        ck.dg.error_note(
            espan,
            format!(
                "both branches of 'select' must have the same type, found {} and {}",
                ck.tcx.name_of(&ta),
                ck.tcx.name_of(&tb)
            ),
            "there is no implicit conversion; write e.g. 'x as i32'",
        );
        return Type::Error;
    }
    ta
}

fn check_barrier(ck: &mut Checker, args: &[Expr], nspan: Span, _espan: Span) -> Type {
    if !digit_count(ck, BARRIER, args, 1, nspan) {
        return Type::Error;
    }
    let t = ck.expr(&args[0], None);
    if t.is_error() {
        return Type::Error;
    }
    if !is_scalar(&t) {
        ck.dg.error_note(
            args[0].span,
            format!(
                "'barrier' only works on scalar values, found {}",
                ck.tcx.name_of(&t)
            ),
            "allowed are integer, bool and pointer types",
        );
        return Type::Error;
    }
    t
}

fn check_secure_zero(ck: &mut Checker, args: &[Expr], nspan: Span, _espan: Span) -> Type {
    if !digit_count(ck, SECURE_ZERO, args, 2, nspan) {
        return Type::Error;
    }
    let tp = ck.expr(&args[0], None);
    if !tp.is_error() && !tp.is_ptr() {
        ck.dg.error_note(
            args[0].span,
            format!(
                "the first argument of 'secure_zero' must be a pointer, found {}",
                ck.tcx.name_of(&tp)
            ),
            "call: secure_zero(pointer, byte_count)",
        );
    }
    let tn = ck.expr(&args[1], Some(&Type::Usize));
    if !tn.is_error() && !tn.is_concrete_int() {
        ck.dg.error(
            args[1].span,
            format!(
                "the byte count of 'secure_zero' must be an integer, found {}",
                ck.tcx.name_of(&tn)
            ),
        );
    }
    Type::Void
}

// ------------------------------------------------------------ Lowering phase

/// Hook from `lower::lower_call`. Produces the FIR instruction.
pub(crate) fn lower_ct_call(
    lw: &mut Lower,
    name: &str,
    args: &[Expr],
    span: Span,
) -> Option<Option<Val>> {
    match name {
        SELECT => {
            if args.len() != 3 {
                return lw.ice(span, "'select' with wrong argument count in lowering");
            }
            let ty = lw.fty_of(&args[1])?;
            let cond = lw.lower_expr(&args[0])?;
            let a = lw.lower_expr(&args[1])?;
            let b = lw.lower_expr(&args[2])?;
            Some(Some(lw.push(ty, Op::Select { cond, a, b })))
        }
        BARRIER => {
            if args.len() != 1 {
                return lw.ice(span, "'barrier' with wrong argument count in lowering");
            }
            let ty = lw.fty_of(&args[0])?;
            let val = lw.lower_expr(&args[0])?;
            Some(Some(lw.push(ty, Op::Barrier { val })))
        }
        _ => {
            if args.len() != 2 {
                return lw.ice(span, "'secure_zero' with wrong argument count in lowering");
            }
            let addr = lw.lower_expr(&args[0])?;
            let n = lw.lower_expr(&args[1])?;
            let size = align(lw, &args[1], n)?;
            lw.push_void(FTy::Void, Op::SecureZero { addr, size });
            Some(None)
        }
    }
}

/// The byte count of `secure_zero` is needed as `u64`; narrower integers
/// are widened (sign correct per source type).
fn align(lw: &mut Lower, arg: &Expr, v: Val) -> Option<Val> {
    let from = lw.fty_of(arg)?;
    if from == FTy::U64 || from == FTy::I64 {
        return Some(v);
    }
    Some(lw.push(FTy::U64, Op::Cast { src: v, from }))
}

#[cfg(test)]
mod tests {
    /// Compiles source text down to the assembler — exactly the path `firnc`
    /// takes (with the optimizer).
    fn asm_of(src: &str) -> String {
        let mut dg = crate::diag::Diags::new("ct_test", src);
        let toks = crate::lexer::lex(src, &mut dg);
        let mut prog = crate::parser::parse(&toks, &mut dg);
        crate::mono::expand(&mut prog, &mut dg);
        let info = crate::sema::check(&prog, &mut dg).expect("type check");
        let mut m = crate::lower::lower(&prog, &info, &mut dg).expect("lowering");
        assert!(!dg.has_errors(), "{}", dg.render());
        crate::opt::optimize(&mut m);
        crate::codegen_x86::emit(&m).expect("codegen")
    }

    /// PROOF (SPEC §9.2): `select` becomes a `cmov` — and the function built
    /// from nothing but that `select` grows NO conditional jump.
    #[test]
    fn select_becomes_cmov_and_never_in_jump() {
        let asm = asm_of(
            "fn choose(b: bool, a: i32, c: i32) -> i32 { return select(b, a, c) }\n\
             fn main() -> i32 { return choose(true, 1 as i32, 2 as i32) }\n",
        );
        assert!(asm.contains("cmov"), "no cmov:\n{}", asm);
        let body = asm.split("choose:").nth(1).expect("function missing");
        let body = body.split("\nmain:").next().unwrap_or(body);
        for line in body.lines() {
            let l = line.trim();
            assert!(
                !(l.starts_with('j') && !l.starts_with("jmp")),
                "conditional jump in 'select': {}\n{}",
                l,
                asm
            );
        }
    }

    /// PROOF (SPEC §9.3, `C3`): `secure_zero` stays, although the buffer is
    /// never read afterwards — the optimizer must not drop it as a dead
    /// access.
    #[test]
    fn secure_zero_survives_the_optimizer() {
        let asm = asm_of(
            "fn main() -> i32 {\n\
                 var buf: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8]\n\
                 secure_zero(&buf[0], 8 as usize)\n\
                 return 0\n\
             }\n",
        );
        assert!(asm.contains("rep stosb"), "secure_zero removed:\n{}", asm);
    }

    /// PROOF (SPEC §9.2): `barrier` survives constant folding — the value is
    /// NOT replaced by the constant.
    #[test]
    fn barrier_stays_opaque() {
        let asm = asm_of("fn main() -> i32 { let a: i32 = barrier(7 as i32)\n return a }\n");
        let body = asm.split("main:").nth(1).expect("main is missing");
        assert!(
            !body.contains("mov rax, 7") && !body.contains("mov eax, 7"),
            "barrier optimized away:\n{}",
            asm
        );
    }

    /// A function of your own that is spelled like a primitive wins —
    /// otherwise the new primitive could silently change the meaning of a
    /// program.
    #[test]
    fn own_func_shadowed_the_primitive() {
        let asm = asm_of(
            "fn barrier(x: i32) -> i32 { return x + 1 as i32 }\n\
             fn main() -> i32 { return barrier(1 as i32) }\n",
        );
        assert!(asm.contains("barrier:"), "own function missing:\n{}", asm);
    }
}
