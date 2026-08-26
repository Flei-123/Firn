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
//! **ROUND 96 — the type qualifier itself.** `secret[T]` is a type now
//! (`types::Type::Secret`), the marking spreads through expressions, and
//! `declassify(x)` is the one way back to a public type. The check in the
//! code generator (`f.constant_time && f.is_secret(cond)`) is fed at last:
//! `lower_expr` puts every value whose type is `secret[T]` into `f.secret`,
//! and `#[constant_time]` switches the check on — which is why `attrs.rs`
//! now lists the attribute as implemented.
//!
//! THE RULES, all of them in this file (SPEC §9.1):
//!
//! | written | answer |
//! |---|---|
//! | `if s { }`, `while s { }`, `s && t`, `s \|\| t` | refused — a jump |
//! | `table[s]` | refused — the address would land in the cache (`C4`) |
//! | `a / s`, `a % s` | refused — variable latency on real hardware |
//! | `a + s` (checked) | refused — the overflow test IS a jump; use `+%` |
//! | `a +\| s` (saturating) | refused — clamps with a jump (`jo`/`jc`) |
//! | `a << s` | refused — a secret shift AMOUNT is not constant time |
//! | `s as u8` | refused — that is declassification by the back door |
//! | `s ^ t`, `s & t`, `s \| t`, `~s`, `-s`, `s +% t`, `s << 3` | allowed |
//! | `s == t`, `s < t` | allowed, and yield `secret[bool]` |
//! | `u8` where `secret[u8]` is wanted | allowed — classification leaks nothing |
//! | `declassify(s)` | allowed, and visible in the source text |
//!
//! **Deliberately still not here:** `u128`/`i128` and `mul_wide` (SPEC §9.3,
//! `C5`). They are arithmetic, not a security property, and they are written
//! down as open in `ACCEPTANCE.md` rather than half built.
//!
//! `barrier(inout x)`/`secure_zero(inout buf)` from SPEC §9 need `inout`,
//! which stage 0 lacks; the stage 0 form takes the value, or a
//! pointer plus length. That too is written down in SPEC §14.1.

use crate::ast::{BinOp, Expr, TypeExpr};
use crate::diag::Span;
use crate::fir::{FTy, Op, Val};
use crate::lexer::TokKind;
use crate::lower::Lower;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::types::Type;

/// Names of the builtin primitives.
pub(crate) const SELECT: &str = "select";
pub(crate) const BARRIER: &str = "barrier";
pub(crate) const SECURE_ZERO: &str = "secure_zero";
/// **ROUND 96** — the one way out of `secret[T]` (SPEC §9.1).
pub(crate) const DECLASSIFY: &str = "declassify";
/// **ROUND 96** — the spelling of the type qualifier (SPEC §9.1).
pub(crate) const SECRET: &str = "secret";

/// Is this spelling the name of a builtin constant-time primitive?
pub(crate) fn is_ct_call(name: &str) -> bool {
    matches!(name, SELECT | BARRIER | SECURE_ZERO | DECLASSIFY)
}

/// May `select`/`barrier` work on a value of this type?
/// Scalar types only: integer, `bool`, pointer.
fn is_scalar(t: &Type) -> bool {
    let p = t.public();
    p.is_concrete_int() || *p == Type::Bool || p.is_ptr()
}

// --------------------------------------------------------------- Parse phase

/// `// HOOK ct` in `parser.rs::parse_type_inner` — `secret[T]` (SPEC §9.1).
///
/// Only the spelling `secret` followed by `[` is taken; a variable, a struct
/// or a function called `secret` is none of this function's business.
pub(crate) fn hook_type(p: &mut Parser, name: &str, sp: Span) -> Option<TypeExpr> {
    if name != SECRET || !p.at(&TokKind::LBracket) {
        return None;
    }
    p.bump();
    let inner = p.parse_type()?;
    let end = p.span();
    if !p.expect(TokKind::RBracket, "after the type in 'secret[T]'") {
        return None;
    }
    Some(TypeExpr::Secret {
        inner: Box::new(inner),
        span: Parser::join(sp, end),
    })
}

/// `// HOOK ct` in `lower.rs::lower_fn` — does this function carry
/// `#[constant_time]` (SPEC §9.2)?
pub(crate) fn has_constant_time(f: &crate::ast::FnDecl) -> bool {
    f.attrs.iter().any(|a| a.name == "constant_time")
}

// ----------------------------------------------------------------- Type phase

/// `// HOOK ct` in `sema::resolve_ty_d` — what may stand inside `secret[…]`.
///
/// SPEC §9.1 says: a qualifier on integer and `bool` types. Not on floating
/// point (the machine's own instructions are not constant time there), not on
/// a pointer (the ADDRESS is what leaks, and it is public by nature), not on
/// an aggregate (`[secret[u8]; 32]` is the way to write a secret array) and
/// not on itself.
pub(crate) fn check_secret_ty(ck: &mut Checker, inner: Type, span: Span) -> Type {
    if inner.is_error() {
        return Type::Error;
    }
    if inner.is_secret() {
        ck.dg.error_note(
            span,
            "'secret[T]' cannot be nested",
            "one marking is the whole statement; 'secret[secret[u8]]' says nothing more than 'secret[u8]'",
        );
        return Type::Error;
    }
    if !(inner.is_concrete_int() || inner == Type::Bool) {
        ck.dg.error_note(
            span,
            format!(
                "'secret[T]' works on integer and bool types only, found {}",
                ck.tcx.name_of(&inner)
            ),
            "SPEC §9.1: an array of secrets is written '[secret[u8]; 32]', a pointer to one '*mut secret[u8]'",
        );
        return Type::Error;
    }
    Type::Secret(Box::new(inner))
}

/// `// HOOK ct` in `sema::check_cond` and in the `&&`/`||` arm of
/// `sema::binary` — a secret must not decide a branch (SPEC §9.1).
///
/// Yields `true` when it has reported something, so the caller can stop.
pub(crate) fn forbid_branch(ck: &mut Checker, t: &Type, span: Span, what: &str) -> bool {
    if !t.is_secret() {
        return false;
    }
    ck.dg.error_note(
        span,
        format!(
            "'{}' must not depend on a secret value, found {}",
            what,
            ck.tcx.name_of(t)
        ),
        "SPEC §9.1: 'select(cond, a, b)' chooses WITHOUT a jump; 'declassify(x)' takes the marking off where that is really meant",
    );
    true
}

/// `// HOOK ct` in `sema::index_type` — a secret index (SPEC §9.1, `C4`).
pub(crate) fn forbid_index(ck: &mut Checker, t: &Type, span: Span) -> bool {
    if !t.is_secret() {
        return false;
    }
    ck.dg.error_note(
        span,
        format!("an index must not be a secret value, found {}", ck.tcx.name_of(t)),
        "SPEC §9.1 (C4): the address of the access lands in the cache and betrays the value; read the WHOLE table and pick with 'select'",
    );
    true
}

/// `// HOOK ct` in `sema::binop_type` — `a op b` with at least one secret.
///
/// Everything refused here is refused because the MACHINE would branch or
/// would take a different amount of time. What survives goes on through the
/// ordinary rules with the markings taken off, and gets the marking back.
pub(crate) fn binop_type(
    ck: &mut Checker,
    op: BinOp,
    lt: &Type,
    rt: &Type,
    span: Span,
) -> Type {
    let refuse = |ck: &mut Checker, note: &str| {
        ck.dg.error_note(
            span,
            format!("operator '{}' is not allowed on a secret value", op.text()),
            note,
        );
        Type::Error
    };
    match op {
        // SPEC §9.1: division and remainder have a data dependent latency on
        // every processor this compiler emits for.
        BinOp::Div | BinOp::Rem => {
            return refuse(ck, "SPEC §9.1: the running time of a division depends on the operands; there is no constant time division")
        }
        // ROUND 96, and this one is the find of the round: at `dev`,
        // `dev-fast` and `release-safe` a `+` carries an overflow test, and
        // that test is a CONDITIONAL JUMP on a flag that comes out of secret
        // operands (`panic_rt.rs`). The explicit wrapping form has none.
        BinOp::Add | BinOp::Sub | BinOp::Mul => {
            return refuse(ck, "the checked form jumps on the overflow flag of a secret value; write '+%', '-%' or '*%' — wrapping is what crypto code means anyway")
        }
        // The saturating form clamps with `jo`/`jc` (codegen_x86::emit_wrap_sat).
        BinOp::AddSat | BinOp::SubSat | BinOp::MulSat => {
            return refuse(ck, "the saturating form clamps with a conditional jump; write '+%', '-%' or '*%'")
        }
        // The shifted VALUE may be secret; the AMOUNT may not. A variable
        // shift is not constant time on every processor, and the amount is
        // what would vary.
        BinOp::Shl | BinOp::Shr if rt.is_secret() => {
            ck.dg.error_note(
                span,
                format!(
                    "the shift amount of '{}' must not be a secret value, found {}",
                    op.text(),
                    ck.tcx.name_of(rt)
                ),
                "a variable shift is not constant time on every processor; the shifted VALUE may be secret",
            );
            return Type::Error;
        }
        _ => {}
    }
    let t = ck.binop_type_public(op, lt.public(), rt.public(), span);
    t.like_secret(true)
}

/// `// HOOK ct` in `sema::expr_inner`, arm `Cast` — `x as T` and the marking.
///
/// `Some(t)` means: this conversion has been decided here.
pub(crate) fn cast(ck: &mut Checker, src: &Type, dst: &Type, span: Span) -> Option<Type> {
    if !src.is_secret() && !dst.is_secret() {
        return None;
    }
    if src.is_secret() && !dst.is_secret() {
        ck.dg.error_note(
            span,
            format!(
                "a secret value cannot be converted to the public type {}",
                ck.tcx.name_of(dst)
            ),
            "SPEC §9.1: 'declassify(x)' is the only way out, and it is meant to stand out in the source text",
        );
        return Some(Type::Error);
    }
    // Public -> secret, or secret -> wider secret. Whether the two PUBLIC
    // types may be converted into each other at all is the ordinary question.
    if !(crate::sema::cast_kind(src.public()) && crate::sema::cast_kind(dst.public())) {
        ck.dg.error(
            span,
            format!(
                "conversion from {} to {} is not allowed",
                ck.tcx.name_of(src),
                ck.tcx.name_of(dst)
            ),
        );
        return Some(Type::Error);
    }
    Some(dst.clone())
}

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
        SECURE_ZERO => check_secure_zero(ck, args, nspan, espan),
        _ => check_declassify(ck, args, nspan, espan),
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
            SECURE_ZERO => "call: secure_zero(pointer, byte_count)",
            _ => "call: declassify(secret_value)",
        },
    );
    false
}

fn check_select(ck: &mut Checker, args: &[Expr], nspan: Span, espan: Span) -> Type {
    if !digit_count(ck, SELECT, args, 3, nspan) {
        return Type::Error;
    }
    let ct = ck.expr(&args[0], Some(&Type::Bool));
    // ROUND 96: `select` is the ANSWER to a secret condition — this is the
    // one place where a `secret[bool]` may steer anything, and it steers a
    // `cmov`, not a jump (SPEC §9.1).
    if !ct.is_error() && *ct.public() != Type::Bool {
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
    // ROUND 96: the result is secret as soon as ANYTHING about the choice is
    // — the condition included. Otherwise `select(secret_cond, 0, 1)` would
    // hand out a public value that says which way the condition went.
    let secret = ct.is_secret() || ta.is_secret() || tb.is_secret();
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
    if ta.public() != tb.public() {
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
    ta.public().clone().like_secret(secret)
}

/// **ROUND 96** — `declassify(x)`: the ONE way out of `secret[T]`
/// (SPEC §9.1).
///
/// It is a call and not an operator on purpose: a review can grep for it.
/// In the IR it becomes a `Barrier` — the value itself does not change, but
/// no pass may reason across the point at which a secret became public
/// (constant folding through it would put key material into `.rodata`).
fn check_declassify(ck: &mut Checker, args: &[Expr], nspan: Span, _espan: Span) -> Type {
    if !digit_count(ck, DECLASSIFY, args, 1, nspan) {
        return Type::Error;
    }
    let t = ck.expr(&args[0], None);
    if t.is_error() {
        return Type::Error;
    }
    match t {
        Type::Secret(inner) => *inner,
        other => {
            ck.dg.error_note(
                args[0].span,
                format!(
                    "'declassify' expects a secret value, found {}",
                    ck.tcx.name_of(&other)
                ),
                "declassify(x) takes the marking off a 'secret[T]'; a public value does not carry one",
            );
            Type::Error
        }
    }
}

fn check_barrier(ck: &mut Checker, args: &[Expr], nspan: Span, _espan: Span) -> Type {
    if !digit_count(ck, BARRIER, args, 1, nspan) {
        return Type::Error;
    }
    let t = ck.expr(&args[0], None);
    if t.is_error() {
        return Type::Error;
    }
    // ROUND 96: a `barrier` on a secret hands back a secret — the marking is
    // exactly what must not fall off at an opaque point.
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
        // ROUND 96 — `declassify(x)`. The VALUE is unchanged; what this
        // instruction is for is the line the optimizer must not reason
        // across. Without the barrier a `declassify(secret_const)` would be
        // folded and the key would stand in `.rodata`; with it the value is
        // opaque to every pass (`fir::is_untouchable`).
        DECLASSIFY => {
            if args.len() != 1 {
                return lw.ice(span, "'declassify' with wrong argument count in lowering");
            }
            let ty = lw.fty_of(&args[0])?;
            let val = lw.lower_expr(&args[0])?;
            Some(Some(lw.push(ty, Op::Barrier { val })))
        }
        SECURE_ZERO => {
            if args.len() != 2 {
                return lw.ice(span, "'secure_zero' with wrong argument count in lowering");
            }
            let addr = lw.lower_expr(&args[0])?;
            let n = lw.lower_expr(&args[1])?;
            let size = align(lw, &args[1], n)?;
            lw.push_void(FTy::Void, Op::SecureZero { addr, size });
            Some(None)
        }
        _ => lw.ice(span, "unknown constant-time primitive in lowering"),
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

    /// **ROUND 96** — PROOF (ACCEPTANCE item 6, the second half): the
    /// assembly of a comparison of two `secret` values carries NO
    /// conditional jump. Held against the counter-check, which does.
    #[test]
    fn comparison_of_two_secrets_has_no_conditional_jump() {
        let asm = asm_of(
            "#[constant_time]\n\
             fn ct_eq(a: secret[u64], b: secret[u64]) -> secret[bool] { return a == b }\n\
             fn plain_eq(a: u64, b: u64) -> bool { if a == b { return true }\n return false }\n\
             fn main() -> i32 {\n\
                 var x: secret[u64] = 1\n\
                 var y: secret[u64] = 2\n\
                 if declassify(ct_eq(x, y)) { return 1 }\n\
                 if plain_eq(1, 2) { return 2 }\n\
                 return 0\n\
             }\n",
        );
        let jumps = |name: &str| -> usize {
            let body = asm.split(&format!("{}:\n", name)).nth(1).expect("function missing");
            let body = body.split("\n.globl").next().unwrap_or(body);
            body.lines()
                .map(str::trim)
                .filter(|l| l.starts_with('j') && !l.starts_with("jmp"))
                .count()
        };
        assert_eq!(jumps("_F0.ct_eq"), 0, "conditional jump on secret data:\n{}", asm);
        // Without this the test above would pass on an empty string too.
        assert!(jumps("_F0.plain_eq") > 0, "the counter-check does not branch:\n{}", asm);
    }

    /// **ROUND 96** — PROOF (SPEC §9.2): the check in the CODE GENERATOR is
    /// fed and it strikes.
    ///
    /// The type check refuses a branch on a secret long before this, which
    /// is why the FIR is built by hand here: this is the second line of
    /// defence, for a branch that only some later pass invents. Two
    /// counter-checks, because a check that always fires is no check: the
    /// same function without `#[constant_time]`, and with it but on a public
    /// condition, both have to translate.
    #[test]
    fn the_check_in_the_generator_strikes() {
        use crate::fir::{FTy, Func, Module, Op, Term};
        let build = |constant_time: bool, secret: bool| -> Result<String, String> {
            let mut f = Func::new("main", vec![], FTy::I32);
            f.constant_time = constant_time;
            let c = f.push(0, FTy::Bool, Op::Const(1));
            if secret {
                f.secret.insert(c);
            }
            let b1 = f.add_block();
            let b2 = f.add_block();
            f.set_term(0, Term::BrCond { cond: c, then_bb: b1, else_bb: b2 });
            let one = f.push(b1, FTy::I32, Op::Const(1));
            f.set_term(b1, Term::Ret(Some(one)));
            let zero = f.push(b2, FTy::I32, Op::Const(0));
            f.set_term(b2, Term::Ret(Some(zero)));
            let mut m = Module::new();
            m.funcs.push(f);
            crate::codegen_x86::emit(&m)
        };
        let err = build(true, true).expect_err("the branch on a secret was not caught");
        assert!(err.contains("#[constant_time]"), "{}", err);
        assert!(err.contains("secret value"), "{}", err);
        assert!(build(false, true).is_ok(), "without the attribute it has to translate");
        assert!(build(true, false).is_ok(), "on a public condition it has to translate");
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
