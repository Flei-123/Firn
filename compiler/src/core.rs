//! **Runde 52 — freistehend: Inline-Assembler, MMIO, Interrupt-Einsprung.**
//!
//! Alles, was ein Kernel braucht und eine Anwendung nicht: die drei Stellen,
//! an denen Firn den Prozessor direkt anspricht. `prof.rs` setzt daneben die
//! Profilregeln aus `SPEC.md` §2 durch, `main.rs` erzeugt mit `-c` eine
//! freistehende ELF-Objektdatei.
//!
//! ## 1. Inline-Assembler
//!
//! ```firn
//! asm("cli")
//! asm("out dx, al", in("dx") port, in("al") wert)
//! let alt: u64 = asm("rdtsc", out("rax"), clobber("rdx"))
//! ```
//!
//! Grammatik (der Parser-Hook unten setzt genau das um):
//!
//! ```ebnf
//! asm_ausdruck = "asm" "(" str_lit { "," asm_op } ")" ;
//! asm_op       = "in"      "(" str_lit ")" ausdruck
//!              | "out"     "(" str_lit ")"
//!              | "clobber" "(" str_lit ")" ;
//! ```
//!
//! `asm` ist **kein Schluesselwort**: der Parser erkennt die Form nur, wenn auf
//! den Bezeichner `asm` unmittelbar `(` und ein Zeichenkettenliteral folgen.
//! Damit bleibt `asm` als gewoehnlicher Name benutzbar und der Tokenstrom
//! aendert sich nicht (dieselbe Entscheidung wie bei `size_of[T]`, `select`,
//! `barrier`, `secure_zero`, `__atomar_addieren`).
//!
//! **Volatile ist nicht abwaehlbar.** `fir::Op::Asm` gilt als unrein
//! (`is_pure() == false`), hat keinen CSE-Schluessel (`opt.rs::key`), ist
//! nicht schleifeninvariant hebbar (`licm.rs`) und gilt in `mem2reg.rs` als
//! unantastbar und speicherveraendernd. Das ist die Lehre aus Runde 40 — dort
//! entfernte der Optimierer Code, den er nicht entfernen durfte.
//!
//! **Registerbindung statt Platzhalter.** Operanden nennen ihr Register
//! selbst; es gibt keine `{0}`-Ersetzung. Das ist die minimale ehrliche Form:
//! der Codegenerator legt vor dem Block `mov <reg>, <wert>` und liest danach
//! `out` aus. Erlaubt sind ausschliesslich die **caller-saved** Register
//! (`rax rcx rdx rsi rdi r8..r11` samt ihren schmalen Namen) — genau die, die
//! auch ein gewoehnlicher `call` zerstoert. Dadurch braucht die
//! Registerzuteilung keine Sonderregel: sie behandelt `Op::Asm` wie einen
//! Aufruf. `rbx`, `rbp`, `rsp` und `r12`–`r15` sind abgelehnt, mit Meldung.
//!
//! ## 2. MMIO
//!
//! ```firn
//! __mmio_write8(p, 65 as u8)
//! let z: u32 = __mmio_read32(p)
//! ```
//!
//! Acht eingebaute Namen (`8|16|32|64` × `lesen|schreiben`). Sie werden zu
//! `fir::Op::MmioLoad` / `Op::MmioStore` — eine einzige Maschineninstruktion,
//! die kein Durchgang zusammenlegen, verschieben oder entfernen darf. Der
//! `__`-Praefix ist reserviert (wie `__atomar_addieren`, Runde 47).
//!
//! ## 3. Interrupt-Einsprungpunkte
//!
//! `#[interrupt] fn tastatur() { … }` — siehe `codegen_x86.rs`. Hier steht nur
//! die Pruefung: keine Parameter, kein Rueckgabewert, nicht aufrufbar, nur im
//! Kernel-Profil.

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

// --------------------------------------------------------------- Namen ---

/// Reservierter Namenspraefix des Inline-Assemblers. Firn-Bezeichner koennen
/// `$` nicht enthalten — eine Kollision mit Nutzercode ist ausgeschlossen.
const P_ASM: &str = "asm$";

/// Die acht MMIO-Namen. Reihenfolge = Breite 8/16/32/64.
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

/// Breitenindex 0..3 eines MMIO-Namens, oder `None`.
fn mmio_width(name: &str, write: bool) -> Option<usize> {
    let tab = if write { &MMIO_WRITE } else { &MMIO_READ };
    tab.iter().position(|n| *n == name)
}

/// Ganzzahltyp zur Breite (0..3 = u8/u16/u32/u64).
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

// ------------------------------------------------------- Registertabelle ---

/// Erlaubte Register: die caller-saved Menge von System V, in allen vier
/// Breiten. Der zweite Eintrag ist der 64-Bit-Stamm — in ihn legt der
/// Codegenerator den Eingabewert (die schmalen Namen sind Sichten darauf).
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

/// Register, die es zwar gibt, die der Inline-Assembler aber ablehnt: sie sind
/// callee-saved bzw. tragen den Rahmen. Getrennte Liste, damit die Meldung
/// sagen kann WARUM (und nicht nur „unbekannt").
const LOCKED: &[&str] = &[
    "rbx", "ebx", "bx", "bl",
    "rbp", "ebp", "bp", "bpl",
    "rsp", "esp", "sp", "spl",
    "r12", "r12d", "r12w", "r12b",
    "r13", "r13d", "r13w", "r13b",
    "r14", "r14d", "r14w", "r14b",
    "r15", "r15d", "r15w", "r15b",
];

/// 64-Bit-Stamm eines erlaubten Registernamens.
pub(crate) fn stem(r: &str) -> Option<&'static str> {
    REGISTER.iter().find(|(n, _)| *n == r).map(|(_, s)| *s)
}

// ------------------------------------------------------------ Register ---

/// Ein `asm`-Block, so wie ihn der Parser gesehen hat. Die Eingabe-AUSDRUECKE
/// stehen nicht hier, sondern als Argumente des erzeugten Aufrufs — dadurch
/// laufen Monomorphisierung, `#[no_gc]`-Pruefung und `comptime` unveraendert
/// darueber hinweg (dieselbe Bauart wie `__match#N` in `sema_match.rs`).
#[derive(Clone, Debug)]
pub(crate) struct AsmBlock {
    pub(crate) template: String,
    pub(crate) out: Option<String>,
    pub(crate) in_regs: Vec<String>,
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

/// Anzahl angemeldeter `asm`-Bloecke (Selbsttests, `--stats`).
pub(crate) fn block_count() -> usize {
    REG.with(|r| r.borrow().len())
}

/// Register leeren — nur fuer die Selbsttests, die mehrere Programme in
/// EINEM Prozess uebersetzen.
#[cfg(test)]
pub(crate) fn reset() {
    REG.with(|r| r.borrow_mut().clear());
}

/// Gehoert der Name zu einem `asm`-Block? Liefert die Nummer.
fn asm_number(name: &str) -> Option<usize> {
    name.strip_prefix(P_ASM)?.parse::<usize>().ok()
}

// ------------------------------------------------------------- Parser ---

/// Zeichenkettenliteral an Position `pos + off`?
fn is_str_at(p: &Parser, off: usize) -> bool {
    matches!(p.toks.get(p.pos + off).map(|t| &t.kind), Some(TokKind::Str(..)))
}

/// Liest ein Zeichenkettenliteral als Rust-`String` (nur Oktettliterale).
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
/// Erkennt `asm ( "…" … )`. Nur diese Form — `asm(x)` mit einem
/// Nicht-Literal bleibt ein gewoehnlicher Aufruf einer Funktion `asm`.
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
    let mut clobber: Vec<String> = Vec::new();

    while p.eat(&TokKind::Comma) {
        if p.at(&TokKind::RParen) || p.at_eof() {
            break;
        }
        let before = p.pos;
        // `in` ist ein Schluesselwort (for-Schleife), `out`/`clobber` sind
        // gewoehnliche Bezeichner. Alle drei werden hier oertlich erkannt.
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
                if out.is_some() {
                    p.dg.error(
                        rspan,
                        "an asm block has at most one 'out' register".to_string(),
                    );
                }
                out = Some(reg);
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
        clobber,
        span: Parser::join(start, vspan),
    });
    Some(p.mk(span, ExprKind::Call(format!("{}{}", P_ASM, nr), ins, start)))
}

// ------------------------------------------------------------ Typphase ---

/// Pruefung eines Registernamens. `wo` steht in der Meldung.
fn check_reg(ck: &mut Checker, reg: &str, span: Span, wo: &str) -> bool {
    if reg == "memory" && wo == "clobber" {
        return true;
    }
    if stem(reg).is_some() {
        return true;
    }
    if LOCKED.contains(&reg) {
        ck.dg.error_note(
            span,
            format!("register '{}' is not allowed in the asm block ({})", reg, wo),
            "allowed are only the caller-saved registers rax rcx rdx rsi rdi r8..r11 \
             (including their narrow names); rbx, rbp, rsp and r12-r15 carry the frame \
             or are callee-saved",
        );
        return false;
    }
    ck.dg.error_note(
        span,
        format!("unknown register name '{}' in the asm block ({})", reg, wo),
        "allowed are rax rcx rdx rsi rdi r8..r11 including narrow names \
         (eax/ax/al, r8d/r8w/r8b, …); in the clobber list additionally 'memory'",
    );
    false
}

/// `// HOOK kern` in `sema::call` — `asm$N(…)` und die acht MMIO-Namen.
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
    // SPEC §2: Inline-Assembler ist Kernel-Sache. Im `app`-Profil abgelehnt,
    // damit niemand versehentlich eine Anwendung an eine Architektur nagelt.
    crate::prof::hook_asm(ck, b.span);
    let mut good = true;
    if let Some(r) = &b.out {
        good &= check_reg(ck, r, b.span, "out");
    }
    for r in &b.in_regs {
        good &= check_reg(ck, r, b.span, "in");
    }
    for r in &b.clobber {
        good &= check_reg(ck, r, b.span, "clobber");
    }
    // Jeder Eingabewert muss skalar sein (Ganzzahl, bool, Zeiger): in ein
    // Register passt nichts anderes.
    for (i, a) in args.iter().enumerate() {
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

// ---------------------------------------------------------- Lowerphase ---

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
    // Eingabewerte in Quellreihenfolge auswerten, danach auf 64 Bit bringen:
    // in ein Register geht immer das ganze Wort.
    let mut ins: Vec<Val> = Vec::with_capacity(args.len());
    for a in args {
        let v = lw.lower_expr(a)?;
        let from = lw.fty_of(a)?;
        let v = if from == FTy::U64 || from == FTy::I64 || from == FTy::Ptr {
            v
        } else {
            lw.push(FTy::U64, Op::Cast { src: v, from })
        };
        ins.push(v);
    }
    let ty = if b.out.is_some() { FTy::U64 } else { FTy::Void };
    let op = Op::Asm {
        template: b.template.clone(),
        out: b.out.clone(),
        in_regs: b.in_regs.clone(),
        ins,
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

/// Traegt die Funktion `#[interrupt]`?
pub(crate) fn has_interrupt(f: &crate::ast::FnDecl) -> bool {
    f.attrs.iter().any(|a| a.name == "interrupt")
}

/// `// HOOK kern` in `sema::run`: Form der `#[interrupt]`-Funktionen pruefen
/// und sicherstellen, dass niemand sie ruft.
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
    // Ein Aufruf mit `call` wuerde in einem `iretq` enden und den Stapel
    // zerlegen. Also verboten — mit Zeile und Spalte.
    for f in &prog.funcs {
        visit_calls(ck, &f.body, &names);
    }
}

fn visit_calls(ck: &mut Checker, b: &crate::ast::Block, names: &[String]) {
    use crate::ast::Stmt;
    for s in &b.stmts {
        match s {
            Stmt::Let { init, .. } => visit_expr(ck, init, names),
            Stmt::Assign { target, value, .. } => {
                visit_expr(ck, target, names);
                visit_expr(ck, value, names);
            }
            Stmt::If { cond, then, els, .. } => {
                visit_expr(ck, cond, names);
                visit_calls(ck, then, names);
                if let Some(e) = els {
                    visit_calls(ck, &crate::ast::Block { stmts: vec![(**e).clone()], span: b.span }, names);
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
                &crate::ast::Block { stmts: vec![(**inner).clone()], span: b.span },
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
        ExprKind::Float(_)
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
        // DIE FALLE AUS RUNDE 40: das Ergebnis wird nie gelesen. Ein
        // Optimierer, der `Op::Asm` fuer rein haelt, wirft die Zeile weg.
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
