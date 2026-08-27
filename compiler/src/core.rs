// SPDX-License-Identifier: GPL-2.0-only
//! **Round 52 — freestanding: inline assembler, MMIO, interrupt entry.**
//!
//! Everything a kernel needs and an application does not: the three places
//! at which Firn addresses the processor directly. Alongside it `prof.rs`
//! enforces the profile rules of `SPEC.md` §2, and `main.rs` produces a
//! freestanding ELF object file with `-c`.
//!
//! ## 1. Inline assembler
//!
//! ```firn
//! asm("cli")
//! asm("out dx, al", <asm_op>, <asm_op>)
//! let old: u64 = asm("rdtsc", out("rax"), clobber("rdx"))
//! ```
//!
//! Grammar (the parser hook below implements exactly that):
//!
//! ```ebnf
//! asm_expr = "asm" "(" str_lit { "," asm_op } ")" ;
//! asm_op   = KwIn      "(" str_lit ")" expression
//!          | "out"     "(" str_lit ")"
//!          | "clobber" "(" str_lit ")" ;
//! ```
//!
//! `asm` is **no keyword**: the parser spots the form only when the identifier
//! `asm` is followed immediately by `(` and a string literal. That keeps `asm`
//! usable as a plain name and leaves the token stream unchanged (the same
//! decision as with `size_of[T]`, `select`, `barrier`, `secure_zero`,
//! `__atomic_add`).
//!
//! **Volatile cannot be waived.** `fir::Op::Asm` counts as impure
//! (`is_pure() == false`), has no CSE key (`opt.rs::key`), cannot be hoisted
//! as loop invariant (`licm.rs`) and counts in `mem2reg.rs` as untouchable
//! and memory changing. That is the lesson of round 40 — there the optimizer
//! removed code it was not allowed to remove.
//!
//! **Register binding rather than placeholders.** Operands state their own
//! register; there is no `{0}` substitution. That is the minimal honest form:
//! the code generator puts `mov <reg>, <value>` in front of the block and
//! reads `out` afterwards. Allowed are exclusively the **caller-saved**
//! registers (`rax rcx rdx rsi rdi r8..r11` together with their narrow
//! names) — exactly those that any ordinary `call` destroys as well. That
//! way the register allocation needs no special rule: it treats `Op::Asm`
//! like a call. `rbx`, `rbp`, `rsp` and `r12`–`r15` are rejected, with a message.
//!
//! ## 2. MMIO
//!
//! ```firn
//! __mmio_write8(p, 65 as u8)
//! let z: u32 = __mmio_read32(p)
//! ```
//!
//! Eight builtin names (`8|16|32|64` × `read|write`). They become
//! `fir::Op::MmioLoad` / `Op::MmioStore` — a single machine instruction that
//! no pass may merge, move or remove. The `__` prefix is reserved (like
//! `__atomic_add`, round 47).
//!
//! ## 3. Interrupt entry points
//!
//! `#[interrupt] fn keyboard() { … }` — see `codegen_x86.rs`. Here stands
//! only the check: no parameters, no return value, not callable, kernel
//! profile only.

use std::cell::RefCell;

use crate::ast::{Expr, ExprKind};
use crate::diag::Span;
use crate::fir::{FTy, Op, Val};
use crate::lexer::TokKind;
use crate::lower::Lower;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::strings::LitValue;
use crate::types::Type;

// -------------------------------------------------------------- Labels ---

/// Reserved name prefix of the inline assembler. Firn identifiers cannot
/// contain a `$` — a collision with user code is ruled out.
const P_ASM: &str = "asm$";

/// The eight MMIO names. Order = width 8/16/32/64.
pub(crate) const MMIO_READ: [&str; 4] = [
    "__mmio_read8",
    "__mmio_read16",
    "__mmio_read32",
    "__mmio_read64",
];
pub(crate) const MMIO_WRITE: [&str; 4] = [
    "__mmio_write8",
    "__mmio_write16",
    "__mmio_write32",
    "__mmio_write64",
];

/// Width index 0..3 of an MMIO name, or `None`.
fn mmio_width(name: &str, write: bool) -> Option<usize> {
    let tab = if write { &MMIO_WRITE } else { &MMIO_READ };
    tab.iter().position(|n| *n == name)
}

/// Integer type per width (0..3 = u8/u16/u32/u64).
fn mmio_ty(i: usize) -> Type {
    match i {
        0 => Type::U8,
        1 => Type::U16,
        2 => Type::U32,
        _ => Type::U64,
    }
}

fn mmio_fty(i: usize) -> FTy {
    match i {
        0 => FTy::U8,
        1 => FTy::U16,
        2 => FTy::U32,
        _ => FTy::U64,
    }
}

// -------------------------------------------------------- Register table ---

/// Allowed registers: the caller-saved set of System V, at all four widths.
/// The second entry is the 64-bit trunk — that is where the code generator
/// puts the input value (the narrow names are views onto it).
const REGISTER: &[(&str, &str)] = &[
    ("rax", "rax"), ("eax", "rax"), ("ax", "rax"), ("al", "rax"),
    ("rcx", "rcx"), ("ecx", "rcx"), ("cx", "rcx"), ("cl", "rcx"),
    ("rdx", "rdx"), ("edx", "rdx"), ("dx", "rdx"), ("dl", "rdx"),
    ("rsi", "rsi"), ("esi", "rsi"), ("si", "rsi"), ("sil", "rsi"),
    ("rdi", "rdi"), ("edi", "rdi"), ("di", "rdi"), ("dil", "rdi"),
    ("r8", "r8"), ("r8d", "r8"), ("r8w", "r8"), ("r8b", "r8"),
    ("r9", "r9"), ("r9d", "r9"), ("r9w", "r9"), ("r9b", "r9"),
    ("r10", "r10"), ("r10d", "r10"), ("r10w", "r10"), ("r10b", "r10"),
    ("r11", "r11"), ("r11d", "r11"), ("r11w", "r11"), ("r11b", "r11"),
];

/// Registers that do exist, but the inline assembler rejects them: they are
/// callee-saved or carry the frame. A separate list, so that the message can
/// say WHY (and not merely "unknown").
const LOCKED: &[&str] = &[
    "rbx", "ebx", "bx", "bl",
    "rbp", "ebp", "bp", "bpl",
    "rsp", "esp", "sp", "spl",
    "r12", "r12d", "r12w", "r12b",
    "r13", "r13d", "r13w", "r13b",
    "r14", "r14d", "r14w", "r14b",
    "r15", "r15d", "r15w", "r15b",
];

/// **ROUND ARM-FREESTANDING — the same table for A64.**
///
/// AAPCS64 calls x0-x17 corruptible across a call: x0-x7 carry arguments and
/// results, x8 is the indirect result register (and the system call number
/// on Linux), x9-x15 are scratch, x16/x17 are the linker's veneer registers.
/// All of them may be named here, at both widths (`x9` and `w9` are one
/// register, exactly as `rax` and `eax` are).
///
/// The second entry is again the 64-BIT TRUNK — that is the name the code
/// generator writes when it moves the operand in or out, so that `in("w0")`
/// and `in("x0")` reach the same place.
const REGISTER_A64: &[(&str, &str)] = &[
    ("x0", "x0"), ("w0", "x0"), ("x1", "x1"), ("w1", "x1"),
    ("x2", "x2"), ("w2", "x2"), ("x3", "x3"), ("w3", "x3"),
    ("x4", "x4"), ("w4", "x4"), ("x5", "x5"), ("w5", "x5"),
    ("x6", "x6"), ("w6", "x6"), ("x7", "x7"), ("w7", "x7"),
    ("x8", "x8"), ("w8", "x8"), ("x9", "x9"), ("w9", "x9"),
    ("x10", "x10"), ("w10", "x10"), ("x11", "x11"), ("w11", "x11"),
    ("x12", "x12"), ("w12", "x12"), ("x13", "x13"), ("w13", "x13"),
    ("x14", "x14"), ("w14", "x14"), ("x15", "x15"), ("w15", "x15"),
    ("x16", "x16"), ("w16", "x16"), ("x17", "x17"), ("w17", "x17"),
];

/// The A64 counterpart of `LOCKED`: registers that exist and are refused,
/// with a reason. x18 is the one that would be easy to get wrong — it is
/// neither scratch nor callee-saved, it belongs to the PLATFORM (thread
/// pointer areas on some systems), and AAPCS64 says a portable program must
/// not touch it. Writing it would break nothing under `qemu-aarch64` today
/// and something else on a real machine tomorrow, which is the worst kind of
/// error to allow.
const LOCKED_A64: &[&str] = &[
    "x18", "w18",
    "x19", "w19", "x20", "w20", "x21", "w21", "x22", "w22", "x23", "w23",
    "x24", "w24", "x25", "w25", "x26", "w26", "x27", "w27", "x28", "w28",
    "x29", "w29", "fp", "x30", "w30", "lr", "sp", "wsp", "xzr", "wzr",
];

/// 64-bit trunk of an allowed register name.
///
/// **ROUND ARM-FREESTANDING:** the answer depends on the MACHINE. For
/// `Arch::X86_64` it is the same table lookup that stood here before,
/// character for character; A64 gets its own table above. There is no third
/// possibility and no fallback — a register name belongs to an instruction
/// set, and a compiler that accepted `rax` while generating A64 would be
/// lying to whoever wrote it.
pub(crate) fn stem(r: &str) -> Option<&'static str> {
    match crate::target::arch() {
        crate::target::Arch::X86_64 => REGISTER.iter().find(|(n, _)| *n == r).map(|(_, s)| *s),
        crate::target::Arch::Aarch64 => {
            REGISTER_A64.iter().find(|(n, _)| *n == r).map(|(_, s)| *s)
        }
    }
}

// ------------------------------------------------------------ Register ---

/// An `asm` block the way the parser saw it. The input EXPRESSIONS do not
/// stand here but as arguments of the produced call — that way
/// monomorphization, the `#[no_gc]` check and `comptime` run over it
/// unchanged (the same build as `__match#N` at `sema_match.rs`).
#[derive(Clone, Debug)]
pub(crate) struct AsmBlock {
    pub(crate) template: String,
    /// The VALUE form `out("rax")` without an expression — the result of the
    /// `asm(…)` expression. At most one, as since round 52.
    pub(crate) out: Option<String>,
    pub(crate) in_regs: Vec<String>,
    /// **ROUND 68** — the MEMORY outputs `out("rdx") p`, in source order.
    /// Each one writes its register into `*p` after the template has run.
    pub(crate) out_regs: Vec<String>,
    pub(crate) clobber: Vec<String>,
    pub(crate) span: Span,
}

thread_local! {
    static REG: RefCell<Vec<AsmBlock>> = const { RefCell::new(Vec::new()) };
}

fn register(b: AsmBlock) -> usize {
    REG.with(|r| {
        let mut v = r.borrow_mut();
        v.push(b);
        v.len() - 1
    })
}

pub(crate) fn block_at(i: usize) -> Option<AsmBlock> {
    REG.with(|r| r.borrow().get(i).cloned())
}

/// Count of registered `asm` blocks (self tests, `--stats`).
pub(crate) fn block_count() -> usize {
    REG.with(|r| r.borrow().len())
}

/// Clear the register — only for the self tests, which compile several
/// programs in ONE process.
#[cfg(test)]
pub(crate) fn reset() {
    REG.with(|r| r.borrow_mut().clear());
}

/// Does the name belong to an `asm` block? Yields the number.
fn asm_number(name: &str) -> Option<usize> {
    name.strip_prefix(P_ASM)?.parse::<usize>().ok()
}

// ------------------------------------------------------------- Parser ---

/// String literal at position `pos + off`?
fn is_str_at(p: &Parser, off: usize) -> bool {
    matches!(p.toks.get(p.pos + off).map(|t| &t.kind), Some(TokKind::Str(..)))
}

/// Reads a string literal as a Rust `String` (octet literals only).
fn str_lit(p: &mut Parser, what_for: &str) -> Option<(String, Span)> {
    let k = p.kind().clone();
    match k {
        TokKind::Str(_, LitValue::Octets(v)) => {
            let sp = p.bump();
            match String::from_utf8(v) {
                Ok(s) => Some((s, sp)),
                Err(_) => {
                    p.error_here(format!("{} must be valid UTF-8", what_for));
                    None
                }
            }
        }
        TokKind::Str(_, LitValue::Units(_)) => {
            p.error_here(format!("{} must not be a u\"…\" literal", what_for));
            None
        }
        _ => {
            p.error_here(format!(
                "expected a string literal {}, found '{}'",
                what_for,
                p.kind().text()
            ));
            None
        }
    }
}

/// `// HOOK kern` in `parser::primary`.
///
/// Spots `asm ( "…" … )`. Only that form — `asm(x)` with a non-literal
/// stays an ordinary call of a function `asm`.
pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr> {
    match p.kind() {
        TokKind::Ident(n) if n == "asm" => {}
        _ => return None,
    }
    if !matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::LParen)) {
        return None;
    }
    if !is_str_at(p, 2) {
        return None;
    }
    let start = p.bump(); // 'asm'
    p.bump(); // '('
    let (template, vspan) = str_lit(p, "as template of 'asm'")?;

    let mut out: Option<String> = None;
    let mut in_regs: Vec<String> = Vec::new();
    let mut ins: Vec<Expr> = Vec::new();
    // ROUND 68: the memory outputs. They stand in the SAME argument list as
    // the inputs, behind them — `check_asm` and `lower_asm` split the list
    // at `in_regs.len()`.
    let mut out_regs: Vec<String> = Vec::new();
    let mut out_exprs: Vec<Expr> = Vec::new();
    let mut clobber: Vec<String> = Vec::new();

    while p.eat(&TokKind::Comma) {
        if p.at(&TokKind::RParen) || p.at_eof() {
            break;
        }
        let before = p.pos;
        // `in` is a keyword (the for loop), `out`/`clobber` are ordinary
        // identifiers. All three are spotted locally here.
        let kind = match p.kind().clone() {
            TokKind::KwIn => {
                p.bump();
                "in"
            }
            TokKind::Ident(n) if n == "out" => {
                p.bump();
                "out"
            }
            TokKind::Ident(n) if n == "clobber" => {
                p.bump();
                "clobber"
            }
            other => {
                p.error_here(format!(
                    "expected 'in', 'out' or 'clobber' in an asm block, found '{}'",
                    other.text()
                ));
                break;
            }
        };
        if !p.expect(TokKind::LParen, "after the operand word of an asm block") {
            break;
        }
        let (reg, rspan) = match str_lit(p, "as register name in an asm block") {
            Some(x) => x,
            None => break,
        };
        if !p.close(TokKind::RParen, "after the register name of an asm block") {
            break;
        }
        match kind {
            "in" => {
                in_regs.push(reg);
                ins.push(p.nested_expr());
            }
            "out" => {
                // ROUND 68: `out("rdx") p` — with an expression behind it
                // the register is written into `*p`; any number of those.
                // Without one it stays the VALUE of the `asm(…)` expression,
                // and of those there is still at most one.
                if p.at(&TokKind::Comma) || p.at(&TokKind::RParen) || p.at_eof() {
                    if out.is_some() {
                        p.dg.error_note(
                            rspan,
                            "an asm block has at most one 'out' register WITHOUT a target"
                                .to_string(),
                            "write 'out(\"rdx\") p' — then the register goes into '*p' \
                             and there may be any number of them",
                        );
                    }
                    out = Some(reg);
                } else {
                    out_regs.push(reg);
                    out_exprs.push(p.nested_expr());
                }
            }
            _ => clobber.push(reg),
        }
        if p.pos == before {
            p.bump();
        }
    }
    let end = p.span();
    p.close(TokKind::RParen, "after the operands of the asm block");
    let span = Parser::join(start, end);
    let nr = register(AsmBlock {
        template,
        out,
        in_regs,
        out_regs,
        clobber,
        span: Parser::join(start, vspan),
    });
    let mut args = ins;
    args.extend(out_exprs);
    Some(p.mk(span, ExprKind::Call(format!("{}{}", P_ASM, nr), args, start)))
}

// ---------------------------------------------------------- Type phase ---

/// Check of a register name. `wo` shows up in the message.
fn check_reg(ck: &mut Checker, reg: &str, span: Span, wo: &str) -> bool {
    if reg == "memory" && wo == "clobber" {
        return true;
    }
    if stem(reg).is_some() {
        return true;
    }
    // ROUND ARM-FREESTANDING: both messages name the register set of the
    // machine that is actually being compiled for. A message that offers
    // `rax` to somebody generating A64 is worse than no message.
    let a64 = crate::target::arch() == crate::target::Arch::Aarch64;
    let locked = if a64 { LOCKED_A64 } else { LOCKED };
    if locked.contains(&reg) {
        let why = if a64 {
            "allowed are only the corruptible registers x0-x17 (and their w names); \
             x18 belongs to the platform, x19-x28 are callee-saved, x29/x30 carry \
             the frame and the return address, sp is the stack pointer"
        } else {
            "allowed are only the caller-saved registers rax rcx rdx rsi rdi r8..r11 \
             (including their narrow names); rbx, rbp, rsp and r12-r15 carry the frame \
             or are callee-saved"
        };
        ck.dg.error_note(
            span,
            format!("register '{}' is not allowed in the asm block ({})", reg, wo),
            why,
        );
        return false;
    }
    let why = if a64 {
        "allowed are x0-x17 including their 32-bit names (w0..w17); \
         in the clobber list additionally 'memory'"
    } else {
        "allowed are rax rcx rdx rsi rdi r8..r11 including narrow names \
         (eax/ax/al, r8d/r8w/r8b, …); in the clobber list additionally 'memory'"
    };
    ck.dg.error_note(
        span,
        format!("unknown register name '{}' in the asm block ({})", reg, wo),
        why,
    );
    false
}

/// `// HOOK kern` in `sema::call` — `asm$N(…)` and the eight MMIO names.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Option<Type> {
    if let Some(nr) = asm_number(name) {
        return Some(check_asm(ck, nr, args, espan));
    }
    if ck.fns.contains_key(name) {
        return None;
    }
    if let Some(w) = mmio_width(name, false) {
        return Some(check_mmio_read(ck, name, w, args, nspan));
    }
    if let Some(w) = mmio_width(name, true) {
        return Some(check_mmio_write(ck, name, w, args, nspan));
    }
    None
}

fn check_asm(ck: &mut Checker, nr: usize, args: &[Expr], espan: Span) -> Type {
    let b = match block_at(nr) {
        Some(b) => b,
        None => {
            ck.dg
                .error(espan, "internal error: unknown asm block".to_string());
            return Type::Error;
        }
    };
    // SPEC §2: the inline assembler is kernel business. Rejected under the
    // `app` profile, so nobody nails an application to one architecture.
    crate::prof::hook_asm(ck, b.span);
    let mut good = true;
    if let Some(r) = &b.out {
        good &= check_reg(ck, r, b.span, "out");
    }
    for r in &b.in_regs {
        good &= check_reg(ck, r, b.span, "in");
    }
    for r in &b.out_regs {
        good &= check_reg(ck, r, b.span, "out");
    }
    for r in &b.clobber {
        good &= check_reg(ck, r, b.span, "clobber");
    }
    // ROUND 68: two outputs must not name the SAME register — one of the two
    // results would be lost without a word being said.
    for (i, r) in b.out_regs.iter().enumerate() {
        let same_stem = |x: &String| stem(x) == stem(r) && stem(r).is_some();
        let twice = b.out_regs[..i].iter().any(same_stem)
            || b.out.as_ref().map(|o| same_stem(o)).unwrap_or(false);
        if twice {
            ck.dg.error_note(
                b.span,
                format!("the register '{}' is an output operand twice", r),
                "every output needs a register of its own; the second one would \
                 overwrite the first",
            );
            good = false;
        }
    }
    let n_in = b.in_regs.len();
    // Every input value must be scalar (integer, bool, pointer): nothing else
    // fits into a register.
    for (i, a) in args.iter().take(n_in).enumerate() {
        let t = ck.expr(a, Some(&Type::U64));
        if t.is_error() {
            good = false;
            continue;
        }
        if !(t.is_concrete_int() || t == Type::Bool || t.is_ptr()) {
            let reg = b.in_regs.get(i).map(|s| s.as_str()).unwrap_or("?");
            ck.dg.error_note(
                a.span,
                format!(
                    "the input operand for '{}' has type {}, that does not fit into a register",
                    reg,
                    ck.tcx.name_of(&t)
                ),
                "allowed are integer, bool and pointer types",
            );
            good = false;
        }
    }
    // ROUND 68: an output operand is an ADDRESS. What is written there is
    // always the WHOLE register, so the target has to be eight octets wide —
    // a narrower one would have its neighbours overwritten silently.
    for (i, a) in args.iter().skip(n_in).enumerate() {
        let reg = b.out_regs.get(i).map(|s| s.as_str()).unwrap_or("?");
        let t = ck.expr(a, Some(&Type::ptr(Type::U64, true)));
        if t.is_error() {
            good = false;
            continue;
        }
        let inner = match &t {
            Type::Ptr { inner, .. } => (**inner).clone(),
            _ => {
                ck.dg.error_note(
                    a.span,
                    format!(
                        "the output operand for '{}' has type {}, expected a pointer",
                        reg,
                        ck.tcx.name_of(&t)
                    ),
                    "write 'out(\"rdx\") &x' — the register goes into '*(&x)'",
                );
                good = false;
                continue;
            }
        };
        let ok = (inner.is_concrete_int() || inner.is_ptr())
            && ck.tcx.size_of(&inner) == 8;
        if !ok {
            ck.dg.error_note(
                a.span,
                format!(
                    "the output operand for '{}' points at {}, that is not eight octets wide",
                    reg,
                    ck.tcx.name_of(&inner)
                ),
                "an output operand always writes the whole register; allowed are \
                 u64, i64, usize, isize and pointer targets",
            );
            good = false;
        }
    }
    if !good {
        return Type::Error;
    }
    if b.out.is_some() {
        Type::U64
    } else {
        Type::Void
    }
}

fn check_mmio_read(
    ck: &mut Checker,
    name: &str,
    w: usize,
    args: &[Expr],
    nspan: Span,
) -> Type {
    if args.len() != 1 {
        for a in args {
            ck.type_out_expr(a);
        }
        ck.dg.error_note(
            nspan,
            format!(
                "'{}' expects exactly one argument (the address), found {}",
                name,
                args.len()
            ),
            "the form is __mmio_read<width>(p: *mut T) -> T",
        );
        return Type::Error;
    }
    let zt = mmio_ty(w);
    let pt = ck.expr(&args[0], Some(&Type::ptr(zt.clone(), true)));
    if !pt.is_error() && !pt.is_ptr() {
        ck.dg.error(
            args[0].span,
            format!(
                "'{}' expects a pointer, found {}",
                name,
                ck.tcx.name_of(&pt)
            ),
        );
        return Type::Error;
    }
    zt
}

fn check_mmio_write(
    ck: &mut Checker,
    name: &str,
    w: usize,
    args: &[Expr],
    nspan: Span,
) -> Type {
    if args.len() != 2 {
        for a in args {
            ck.type_out_expr(a);
        }
        ck.dg.error_note(
            nspan,
            format!(
                "'{}' expects exactly two arguments (address, value), found {}",
                name,
                args.len()
            ),
            "the form is __mmio_write<width>(p: *mut T, value: T)",
        );
        return Type::Error;
    }
    let zt = mmio_ty(w);
    let pt = ck.expr(&args[0], Some(&Type::ptr(zt.clone(), true)));
    if !pt.is_error() && !pt.is_ptr() {
        ck.dg.error(
            args[0].span,
            format!(
                "'{}' expects a pointer as first argument, found {}",
                name,
                ck.tcx.name_of(&pt)
            ),
        );
        return Type::Error;
    }
    let wt = ck.expr(&args[1], Some(&zt));
    if !wt.is_error() && wt != zt && wt != Type::UntypedInt {
        ck.dg.error(
            args[1].span,
            format!(
                "'{}' writes {}, found {}",
                name,
                ck.tcx.name_of(&zt),
                ck.tcx.name_of(&wt)
            ),
        );
        return Type::Error;
    }
    Type::Void
}

// ------------------------------------------------------ Lowering phase ---

/// `// HOOK kern` in `lower::lower_call`.
#[allow(clippy::option_option)]
pub(crate) fn lower_hook(
    lw: &mut Lower,
    name: &str,
    args: &[Expr],
    span: Span,
) -> Option<Option<Option<Val>>> {
    if let Some(nr) = asm_number(name) {
        return Some(lower_asm(lw, nr, args, span));
    }
    if lw.info.fns.contains_key(name) {
        return None;
    }
    if let Some(w) = mmio_width(name, false) {
        if args.len() != 1 {
            return Some(lw.ice(span, "mmio read with wrong arity"));
        }
        let a = match lw.lower_expr(&args[0]) {
            Some(v) => v,
            None => return Some(None),
        };
        return Some(Some(Some(lw.push(mmio_fty(w), Op::MmioLoad { addr: a }))));
    }
    if let Some(w) = mmio_width(name, true) {
        if args.len() != 2 {
            return Some(lw.ice(span, "mmio write with wrong arity"));
        }
        let (a, v) = match (lw.lower_expr(&args[0]), lw.lower_expr(&args[1])) {
            (Some(a), Some(v)) => (a, v),
            _ => return Some(None),
        };
        lw.push_void(mmio_fty(w), Op::MmioStore { addr: a, val: v });
        return Some(Some(None));
    }
    None
}

fn lower_asm(
    lw: &mut Lower,
    nr: usize,
    args: &[Expr],
    span: Span,
) -> Option<Option<Val>> {
    let b = match block_at(nr) {
        Some(b) => b,
        None => return lw.ice(span, "unknown asm block in lowering"),
    };
    // Evaluate the input values in source order, then bring them to 64 bits:
    // what goes into a register is always the whole word.
    let n_in = b.in_regs.len();
    let mut ins: Vec<Val> = Vec::with_capacity(n_in);
    for a in args.iter().take(n_in) {
        let v = lw.lower_expr(a)?;
        let from = lw.fty_of(a)?;
        let v = if from == FTy::U64 || from == FTy::I64 || from == FTy::Ptr {
            v
        } else {
            lw.push(FTy::U64, Op::Cast { src: v, from })
        };
        ins.push(v);
    }
    // ROUND 68: the output operands are ADDRESSES — evaluated in source
    // order as well, after the inputs.
    let mut outs: Vec<Val> = Vec::with_capacity(b.out_regs.len());
    for a in args.iter().skip(n_in) {
        outs.push(lw.lower_expr(a)?);
    }
    let ty = if b.out.is_some() { FTy::U64 } else { FTy::Void };
    let op = Op::Asm {
        template: b.template.clone(),
        out: b.out.clone(),
        in_regs: b.in_regs.clone(),
        ins,
        out_regs: b.out_regs.clone(),
        outs,
        clobber: b.clobber.clone(),
    };
    if b.out.is_some() {
        Some(Some(lw.push(ty, op)))
    } else {
        lw.push_void(ty, op);
        Some(None)
    }
}

// ---------------------------------------------------------- #[interrupt] ---

/// Does the function carry `#[interrupt]`?
pub(crate) fn has_interrupt(f: &crate::ast::FnDecl) -> bool {
    f.attrs.iter().any(|a| a.name == "interrupt")
}

/// `// HOOK kern` in `sema::run`: check the form of the `#[interrupt]`
/// functions and make sure that nobody calls them.
pub(crate) fn check_interrupts(ck: &mut Checker, prog: &crate::ast::Program) {
    let mut names: Vec<String> = Vec::new();
    for f in &prog.funcs {
        if !has_interrupt(f) {
            continue;
        }
        names.push(f.name.clone());
        if !crate::prof::is_kernel() {
            ck.dg.error_note(
                f.span,
                format!(
                    "'{}' is marked with #[interrupt], which exists only in profile 'kernel'",
                    f.name
                ),
                "write 'profile kernel' in the first line or compile with --profile=kernel",
            );
        }
        if !f.params.is_empty() {
            ck.dg.error_note(
                f.span,
                format!(
                    "an #[interrupt] function has no parameters, '{}' has {}",
                    f.name,
                    f.params.len()
                ),
                "the processor puts the interrupt frame on the stack itself; \
                 there are no arguments",
            );
        }
        if f.ret.is_some() {
            ck.dg.error_note(
                f.span,
                format!(
                    "an #[interrupt] function yields no value, '{}' has a return type",
                    f.name
                ),
                "it ends with 'iretq', not with 'ret' — there is nobody who could \
                 accept a value",
            );
        }
    }
    if names.is_empty() {
        return;
    }
    // A call through `call` would end at an `iretq` and take the stack
    // apart. So forbidden — with line and column.
    for f in &prog.funcs {
        visit_calls(ck, &f.body, &names);
    }
}

fn visit_calls(ck: &mut Checker, b: &crate::ast::Block, names: &[String]) {
    use crate::ast::Stmt;
    for s in &b.stmts {
        match s {
            Stmt::Let { init, .. } => visit_expr(ck, init, names),
            Stmt::Assign { target, value, .. }
            | Stmt::AssignOp { target, value, .. } => {
                visit_expr(ck, target, names);
                visit_expr(ck, value, names);
            }
            // ROUND 70: the step has no value expression.
            Stmt::Step { target, .. } => visit_expr(ck, target, names),
            Stmt::If { cond, then, els, .. } => {
                visit_expr(ck, cond, names);
                visit_calls(ck, then, names);
                if let Some(e) = els {
                    visit_calls(ck, &crate::ast::Block { stmts: vec![(**e).clone()], span: b.span, end: b.end }, names);
                }
            }
            Stmt::While { cond, body, .. } => {
                visit_expr(ck, cond, names);
                visit_calls(ck, body, names);
            }
            Stmt::For { start, end, body, .. } => {
                visit_expr(ck, start, names);
                visit_expr(ck, end, names);
                visit_calls(ck, body, names);
            }
            Stmt::Return { value, .. } => {
                if let Some(e) = value {
                    visit_expr(ck, e, names);
                }
            }
            Stmt::Defer(inner, _, _) => visit_calls(
                ck,
                &crate::ast::Block { stmts: vec![(**inner).clone()], span: b.span, end: b.end },
                names,
            ),
            Stmt::Expr(e) => visit_expr(ck, e, names),
            Stmt::Block(inner) => visit_calls(ck, inner, names),
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
        }
    }
}

fn visit_expr(ck: &mut Checker, e: &Expr, names: &[String]) {
    match &e.kind {
        ExprKind::FloatF32(_) => {}
        // Round 58: a closure body is code like any other.
        ExprKind::Lambda(d) => visit_calls(ck, &d.body, names),
        // ROUND 70: the text literal carries its array literal inside.
        ExprKind::Text(_, inner) => visit_expr(ck, inner, names),
        ExprKind::Call(n, args, nspan) => {
            if names.iter().any(|x| x == n) {
                ck.dg.error_note(
                    *nspan,
                    format!("'{}' is an interrupt entry point and cannot be called", n),
                    "it ends with 'iretq' and expects the interrupt frame of the \
                     processor on the stack; only the IDT may point to it",
                );
            }
            for a in args {
                visit_expr(ck, a, names);
            }
        }
        ExprKind::Unary(_, a) => visit_expr(ck, a, names),
        ExprKind::Binary(_, a, b) => {
            visit_expr(ck, a, names);
            visit_expr(ck, b, names);
        }
        ExprKind::Field(a, ..) => visit_expr(ck, a, names),
        ExprKind::Index(a, b) => {
            visit_expr(ck, a, names);
            visit_expr(ck, b, names);
        }
        ExprKind::Syscall(args) | ExprKind::ArrayLit(args) => {
            for a in args {
                visit_expr(ck, a, names);
            }
        }
        ExprKind::Cast(a, _) => visit_expr(ck, a, names),
        ExprKind::StructLit(_, fields, _) => {
            for (_, a, _) in fields {
                visit_expr(ck, a, names);
            }
        }
        ExprKind::ArrayRepeat(a, b) => {
            visit_expr(ck, a, names);
            visit_expr(ck, b, names);
        }
        ExprKind::Float(..)
        | ExprKind::Int(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(src: &str) -> (String, String) {
        reset();
        crate::prof::reset();
        let mut dg = crate::diag::Diags::new("core_test", src);
        let toks = crate::lexer::lex(src, &mut dg);
        let mut prog = crate::parser::parse(&toks, &mut dg);
        crate::mono::expand(&mut prog, &mut dg);
        crate::prof::define(&prog, None);
        let info = match crate::sema::check(&prog, &mut dg) {
            Some(i) => i,
            None => return (String::new(), dg.render()),
        };
        let mut m = match crate::lower::lower(&prog, &info, &mut dg) {
            Some(m) => m,
            None => return (String::new(), dg.render()),
        };
        crate::opt::optimize(&mut m);
        (crate::codegen_x86::emit(&m).unwrap_or_default(), dg.render())
    }

    #[test]
    fn asm_stays_despite_unused_result() {
        // THE TRAP OF ROUND 40: the result is never read. Any optimizer
        // that holds `Op::Asm` for pure throws the line away.
        let (asm, _) = build(
            "profile kernel\nfn f() { let _x: u64 = asm(\"rdtsc\", out(\"rax\"), clobber(\"rdx\")) }\n",
        );
        assert!(asm.contains("rdtsc"), "asm block vanished:\n{}", asm);
    }

    #[test]
    fn two_same_asm_blocks_become_not_merged() {
        let (asm, _) = build(
            "profile kernel\nfn f() { asm(\"cli\")\n asm(\"cli\") }\n",
        );
        assert_eq!(asm.matches("cli").count(), 2, "CSE struck:\n{}", asm);
    }

    #[test]
    fn mmio_accesses_become_not_merged() {
        let (asm, _) = build(
            "profile kernel\nfn f(p: *mut u32) -> u32 { let a: u32 = __mmio_read32(p)\n let b: u32 = __mmio_read32(p)\n return a + b }\n",
        );
        assert_eq!(
            asm.matches("dword ptr [rcx]").count(),
            2,
            "two MMIO loads became one:\n{}",
            asm
        );
    }

    #[test]
    fn locked_register_becomes_named() {
        let (_, err) = build("profile kernel\nfn f() { asm(\"nop\", clobber(\"rbx\")) }\n");
        assert!(err.contains("rbx"), "{}", err);
        assert!(err.contains("callee-saved"), "{}", err);
    }

    #[test]
    fn interrupt_can_not_called_become() {
        let (_, err) = build(
            "profile kernel\n#[interrupt]\nfn ih() { asm(\"nop\") }\nfn f() { ih() }\n",
        );
        assert!(err.contains("interrupt entry point"), "{}", err);
    }

    #[test]
    fn interrupt_ends_with_iretq() {
        let (asm, err) = build("profile kernel\n#[interrupt]\nfn ih() { asm(\"nop\") }\n");
        assert!(!err.contains("error"), "{}", err);
        assert!(asm.contains("iretq"), "{}", asm);
        assert!(asm.contains("push r15"), "{}", asm);
    }
}
