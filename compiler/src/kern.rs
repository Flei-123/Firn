//! **Runde 52 — freistehend: Inline-Assembler, MMIO, Interrupt-Einsprung.**
//!
//! Alles, was ein Kernel braucht und eine Anwendung nicht: die drei Stellen,
//! an denen Firn den Prozessor direkt anspricht. `profil.rs` setzt daneben die
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
//! __mmio_schreiben8(p, 65 as u8)
//! let z: u32 = __mmio_lesen32(p)
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
pub(crate) const MMIO_LESEN: [&str; 4] = [
    "__mmio_lesen8",
    "__mmio_lesen16",
    "__mmio_lesen32",
    "__mmio_lesen64",
];
pub(crate) const MMIO_SCHREIBEN: [&str; 4] = [
    "__mmio_schreiben8",
    "__mmio_schreiben16",
    "__mmio_schreiben32",
    "__mmio_schreiben64",
];

/// Breitenindex 0..3 eines MMIO-Namens, oder `None`.
fn mmio_breite(name: &str, schreiben: bool) -> Option<usize> {
    let tab = if schreiben { &MMIO_SCHREIBEN } else { &MMIO_LESEN };
    tab.iter().position(|n| *n == name)
}

/// Ganzzahltyp zur Breite (0..3 = u8/u16/u32/u64).
fn mmio_typ(i: usize) -> Type {
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
const GESPERRT: &[&str] = &[
    "rbx", "ebx", "bx", "bl",
    "rbp", "ebp", "bp", "bpl",
    "rsp", "esp", "sp", "spl",
    "r12", "r12d", "r12w", "r12b",
    "r13", "r13d", "r13w", "r13b",
    "r14", "r14d", "r14w", "r14b",
    "r15", "r15d", "r15w", "r15b",
];

/// 64-Bit-Stamm eines erlaubten Registernamens.
pub(crate) fn stamm(r: &str) -> Option<&'static str> {
    REGISTER.iter().find(|(n, _)| *n == r).map(|(_, s)| *s)
}

// ------------------------------------------------------------ Register ---

/// Ein `asm`-Block, so wie ihn der Parser gesehen hat. Die Eingabe-AUSDRUECKE
/// stehen nicht hier, sondern als Argumente des erzeugten Aufrufs — dadurch
/// laufen Monomorphisierung, `#[no_gc]`-Pruefung und `comptime` unveraendert
/// darueber hinweg (dieselbe Bauart wie `__match#N` in `sema_match.rs`).
#[derive(Clone, Debug)]
pub(crate) struct AsmBlock {
    pub(crate) vorlage: String,
    pub(crate) aus: Option<String>,
    pub(crate) ein_regs: Vec<String>,
    pub(crate) clobber: Vec<String>,
    pub(crate) span: Span,
}

thread_local! {
    static REG: RefCell<Vec<AsmBlock>> = const { RefCell::new(Vec::new()) };
}

fn anmelden(b: AsmBlock) -> usize {
    REG.with(|r| {
        let mut v = r.borrow_mut();
        v.push(b);
        v.len() - 1
    })
}

pub(crate) fn block_bei(i: usize) -> Option<AsmBlock> {
    REG.with(|r| r.borrow().get(i).cloned())
}

/// Anzahl angemeldeter `asm`-Bloecke (Selbsttests, `--stats`).
pub(crate) fn block_anzahl() -> usize {
    REG.with(|r| r.borrow().len())
}

/// Register leeren — nur fuer die Selbsttests, die mehrere Programme in
/// EINEM Prozess uebersetzen.
#[cfg(test)]
pub(crate) fn zuruecksetzen() {
    REG.with(|r| r.borrow_mut().clear());
}

/// Gehoert der Name zu einem `asm`-Block? Liefert die Nummer.
fn asm_nummer(name: &str) -> Option<usize> {
    name.strip_prefix(P_ASM)?.parse::<usize>().ok()
}

// ------------------------------------------------------------- Parser ---

/// Zeichenkettenliteral an Position `pos + off`?
fn ist_str_bei(p: &Parser, off: usize) -> bool {
    matches!(p.toks.get(p.pos + off).map(|t| &t.kind), Some(TokKind::Str(..)))
}

/// Liest ein Zeichenkettenliteral als Rust-`String` (nur Oktettliterale).
fn str_lit(p: &mut Parser, wozu: &str) -> Option<(String, Span)> {
    let k = p.kind().clone();
    match k {
        TokKind::Str(_, LitValue::Octets(v)) => {
            let sp = p.bump();
            match String::from_utf8(v) {
                Ok(s) => Some((s, sp)),
                Err(_) => {
                    p.error_here(format!("{} muss gueltiges UTF-8 sein", wozu));
                    None
                }
            }
        }
        TokKind::Str(_, LitValue::Units(_)) => {
            p.error_here(format!("{} darf kein u\"…\"-literal sein", wozu));
            None
        }
        _ => {
            p.error_here(format!(
                "erwartet ein zeichenkettenliteral {}, gefunden '{}'",
                wozu,
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
    if !ist_str_bei(p, 2) {
        return None;
    }
    let start = p.bump(); // 'asm'
    p.bump(); // '('
    let (vorlage, vspan) = str_lit(p, "als vorlage von 'asm'")?;

    let mut aus: Option<String> = None;
    let mut ein_regs: Vec<String> = Vec::new();
    let mut ein: Vec<Expr> = Vec::new();
    let mut clobber: Vec<String> = Vec::new();

    while p.eat(&TokKind::Comma) {
        if p.at(&TokKind::RParen) || p.at_eof() {
            break;
        }
        let vor = p.pos;
        // `in` ist ein Schluesselwort (for-Schleife), `out`/`clobber` sind
        // gewoehnliche Bezeichner. Alle drei werden hier oertlich erkannt.
        let art = match p.kind().clone() {
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
                    "erwartet 'in', 'out' oder 'clobber' in einem asm-block, gefunden '{}'",
                    other.text()
                ));
                break;
            }
        };
        if !p.expect(TokKind::LParen, "nach dem operandenwort eines asm-blocks") {
            break;
        }
        let (reg, rspan) = match str_lit(p, "als registername in einem asm-block") {
            Some(x) => x,
            None => break,
        };
        if !p.close(TokKind::RParen, "nach dem registernamen eines asm-blocks") {
            break;
        }
        match art {
            "in" => {
                ein_regs.push(reg);
                ein.push(p.nested_expr());
            }
            "out" => {
                if aus.is_some() {
                    p.dg.error(
                        rspan,
                        "ein asm-block hat hoechstens ein 'out'-register".to_string(),
                    );
                }
                aus = Some(reg);
            }
            _ => clobber.push(reg),
        }
        if p.pos == vor {
            p.bump();
        }
    }
    let end = p.span();
    p.close(TokKind::RParen, "nach den operanden des asm-blocks");
    let span = Parser::join(start, end);
    let nr = anmelden(AsmBlock {
        vorlage,
        aus,
        ein_regs,
        clobber,
        span: Parser::join(start, vspan),
    });
    Some(p.mk(span, ExprKind::Call(format!("{}{}", P_ASM, nr), ein, start)))
}

// ------------------------------------------------------------ Typphase ---

/// Pruefung eines Registernamens. `wo` steht in der Meldung.
fn pruefe_reg(ck: &mut Checker, reg: &str, span: Span, wo: &str) -> bool {
    if reg == "memory" && wo == "clobber" {
        return true;
    }
    if stamm(reg).is_some() {
        return true;
    }
    if GESPERRT.contains(&reg) {
        ck.dg.error_note(
            span,
            format!("register '{}' ist im asm-block nicht erlaubt ({})", reg, wo),
            "erlaubt sind nur die caller-saved register rax rcx rdx rsi rdi r8..r11 \
             (samt ihren schmalen namen); rbx, rbp, rsp und r12-r15 tragen den rahmen \
             bzw. sind callee-saved",
        );
        return false;
    }
    ck.dg.error_note(
        span,
        format!("unbekannter registername '{}' im asm-block ({})", reg, wo),
        "erlaubt sind rax rcx rdx rsi rdi r8..r11 samt schmalen namen \
         (eax/ax/al, r8d/r8w/r8b, …); in der clobber-liste zusaetzlich 'memory'",
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
    if let Some(nr) = asm_nummer(name) {
        return Some(check_asm(ck, nr, args, espan));
    }
    if ck.fns.contains_key(name) {
        return None;
    }
    if let Some(w) = mmio_breite(name, false) {
        return Some(check_mmio_lesen(ck, name, w, args, nspan));
    }
    if let Some(w) = mmio_breite(name, true) {
        return Some(check_mmio_schreiben(ck, name, w, args, nspan));
    }
    None
}

fn check_asm(ck: &mut Checker, nr: usize, args: &[Expr], espan: Span) -> Type {
    let b = match block_bei(nr) {
        Some(b) => b,
        None => {
            ck.dg
                .error(espan, "interner fehler: unbekannter asm-block".to_string());
            return Type::Error;
        }
    };
    // SPEC §2: Inline-Assembler ist Kernel-Sache. Im `app`-Profil abgelehnt,
    // damit niemand versehentlich eine Anwendung an eine Architektur nagelt.
    crate::profil::hook_asm(ck, b.span);
    let mut gut = true;
    if let Some(r) = &b.aus {
        gut &= pruefe_reg(ck, r, b.span, "out");
    }
    for r in &b.ein_regs {
        gut &= pruefe_reg(ck, r, b.span, "in");
    }
    for r in &b.clobber {
        gut &= pruefe_reg(ck, r, b.span, "clobber");
    }
    // Jeder Eingabewert muss skalar sein (Ganzzahl, bool, Zeiger): in ein
    // Register passt nichts anderes.
    for (i, a) in args.iter().enumerate() {
        let t = ck.expr(a, Some(&Type::U64));
        if t.is_error() {
            gut = false;
            continue;
        }
        if !(t.is_concrete_int() || t == Type::Bool || t.is_ptr()) {
            let reg = b.ein_regs.get(i).map(|s| s.as_str()).unwrap_or("?");
            ck.dg.error_note(
                a.span,
                format!(
                    "der eingabeoperand fuer '{}' hat den typ {}, das passt in kein register",
                    reg,
                    ck.tcx.name_of(&t)
                ),
                "erlaubt sind ganzzahl-, bool- und zeigertypen",
            );
            gut = false;
        }
    }
    if !gut {
        return Type::Error;
    }
    if b.aus.is_some() {
        Type::U64
    } else {
        Type::Void
    }
}

fn check_mmio_lesen(
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
                "'{}' erwartet genau ein argument (die adresse), gefunden {}",
                name,
                args.len()
            ),
            "die form ist __mmio_lesen<breite>(p: *mut T) -> T",
        );
        return Type::Error;
    }
    let zt = mmio_typ(w);
    let pt = ck.expr(&args[0], Some(&Type::ptr(zt.clone(), true)));
    if !pt.is_error() && !pt.is_ptr() {
        ck.dg.error(
            args[0].span,
            format!(
                "'{}' erwartet einen zeiger, gefunden {}",
                name,
                ck.tcx.name_of(&pt)
            ),
        );
        return Type::Error;
    }
    zt
}

fn check_mmio_schreiben(
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
                "'{}' erwartet genau zwei argumente (adresse, wert), gefunden {}",
                name,
                args.len()
            ),
            "die form ist __mmio_schreiben<breite>(p: *mut T, wert: T)",
        );
        return Type::Error;
    }
    let zt = mmio_typ(w);
    let pt = ck.expr(&args[0], Some(&Type::ptr(zt.clone(), true)));
    if !pt.is_error() && !pt.is_ptr() {
        ck.dg.error(
            args[0].span,
            format!(
                "'{}' erwartet als erstes argument einen zeiger, gefunden {}",
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
                "'{}' schreibt {}, gefunden {}",
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
    if let Some(nr) = asm_nummer(name) {
        return Some(lower_asm(lw, nr, args, span));
    }
    if lw.info.fns.contains_key(name) {
        return None;
    }
    if let Some(w) = mmio_breite(name, false) {
        if args.len() != 1 {
            return Some(lw.ice(span, "mmio-lesen mit falscher stellenzahl"));
        }
        let a = match lw.lower_expr(&args[0]) {
            Some(v) => v,
            None => return Some(None),
        };
        return Some(Some(Some(lw.push(mmio_fty(w), Op::MmioLoad { addr: a }))));
    }
    if let Some(w) = mmio_breite(name, true) {
        if args.len() != 2 {
            return Some(lw.ice(span, "mmio-schreiben mit falscher stellenzahl"));
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
    let b = match block_bei(nr) {
        Some(b) => b,
        None => return lw.ice(span, "unbekannter asm-block im lowering"),
    };
    // Eingabewerte in Quellreihenfolge auswerten, danach auf 64 Bit bringen:
    // in ein Register geht immer das ganze Wort.
    let mut ein: Vec<Val> = Vec::with_capacity(args.len());
    for a in args {
        let v = lw.lower_expr(a)?;
        let from = lw.fty_of(a)?;
        let v = if from == FTy::U64 || from == FTy::I64 || from == FTy::Ptr {
            v
        } else {
            lw.push(FTy::U64, Op::Cast { src: v, from })
        };
        ein.push(v);
    }
    let ty = if b.aus.is_some() { FTy::U64 } else { FTy::Void };
    let op = Op::Asm {
        vorlage: b.vorlage.clone(),
        aus: b.aus.clone(),
        ein_regs: b.ein_regs.clone(),
        ein,
        clobber: b.clobber.clone(),
    };
    if b.aus.is_some() {
        Some(Some(lw.push(ty, op)))
    } else {
        lw.push_void(ty, op);
        Some(None)
    }
}

// ---------------------------------------------------------- #[interrupt] ---

/// Traegt die Funktion `#[interrupt]`?
pub(crate) fn hat_interrupt(f: &crate::ast::FnDecl) -> bool {
    f.attrs.iter().any(|a| a.name == "interrupt")
}

/// `// HOOK kern` in `sema::run`: Form der `#[interrupt]`-Funktionen pruefen
/// und sicherstellen, dass niemand sie ruft.
pub(crate) fn check_interrupts(ck: &mut Checker, prog: &crate::ast::Program) {
    let mut namen: Vec<String> = Vec::new();
    for f in &prog.funcs {
        if !hat_interrupt(f) {
            continue;
        }
        namen.push(f.name.clone());
        if !crate::profil::ist_kernel() {
            ck.dg.error_note(
                f.span,
                format!(
                    "'{}' ist mit #[interrupt] gekennzeichnet, das gibt es nur im profil 'kernel'",
                    f.name
                ),
                "schreibe 'profile kernel' in die erste zeile oder uebersetze mit --profile=kernel",
            );
        }
        if !f.params.is_empty() {
            ck.dg.error_note(
                f.span,
                format!(
                    "eine #[interrupt]-funktion hat keine parameter, '{}' hat {}",
                    f.name,
                    f.params.len()
                ),
                "der prozessor legt den unterbrechungsrahmen selbst auf den stapel; \
                 es gibt keine argumente",
            );
        }
        if f.ret.is_some() {
            ck.dg.error_note(
                f.span,
                format!(
                    "eine #[interrupt]-funktion liefert keinen wert, '{}' hat einen rueckgabetyp",
                    f.name
                ),
                "sie endet mit 'iretq', nicht mit 'ret' — es gibt niemanden, der einen \
                 wert entgegennehmen koennte",
            );
        }
    }
    if namen.is_empty() {
        return;
    }
    // Ein Aufruf mit `call` wuerde in einem `iretq` enden und den Stapel
    // zerlegen. Also verboten — mit Zeile und Spalte.
    for f in &prog.funcs {
        besuche_aufrufe(ck, &f.body, &namen);
    }
}

fn besuche_aufrufe(ck: &mut Checker, b: &crate::ast::Block, namen: &[String]) {
    use crate::ast::Stmt;
    for s in &b.stmts {
        match s {
            Stmt::Let { init, .. } => besuche_expr(ck, init, namen),
            Stmt::Assign { target, value, .. } => {
                besuche_expr(ck, target, namen);
                besuche_expr(ck, value, namen);
            }
            Stmt::If { cond, then, els, .. } => {
                besuche_expr(ck, cond, namen);
                besuche_aufrufe(ck, then, namen);
                if let Some(e) = els {
                    besuche_aufrufe(ck, &crate::ast::Block { stmts: vec![(**e).clone()], span: b.span }, namen);
                }
            }
            Stmt::While { cond, body, .. } => {
                besuche_expr(ck, cond, namen);
                besuche_aufrufe(ck, body, namen);
            }
            Stmt::For { start, end, body, .. } => {
                besuche_expr(ck, start, namen);
                besuche_expr(ck, end, namen);
                besuche_aufrufe(ck, body, namen);
            }
            Stmt::Return { value, .. } => {
                if let Some(e) = value {
                    besuche_expr(ck, e, namen);
                }
            }
            Stmt::Defer(inner, _, _) => besuche_aufrufe(
                ck,
                &crate::ast::Block { stmts: vec![(**inner).clone()], span: b.span },
                namen,
            ),
            Stmt::Expr(e) => besuche_expr(ck, e, namen),
            Stmt::Block(inner) => besuche_aufrufe(ck, inner, namen),
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
        }
    }
}

fn besuche_expr(ck: &mut Checker, e: &Expr, namen: &[String]) {
    match &e.kind {
        ExprKind::Call(n, args, nspan) => {
            if namen.iter().any(|x| x == n) {
                ck.dg.error_note(
                    *nspan,
                    format!("'{}' ist ein interrupt-einsprungpunkt und kann nicht aufgerufen werden", n),
                    "sie endet mit 'iretq' und erwartet den unterbrechungsrahmen des \
                     prozessors auf dem stapel; nur die IDT darf auf sie zeigen",
                );
            }
            for a in args {
                besuche_expr(ck, a, namen);
            }
        }
        ExprKind::Unary(_, a) => besuche_expr(ck, a, namen),
        ExprKind::Binary(_, a, b) => {
            besuche_expr(ck, a, namen);
            besuche_expr(ck, b, namen);
        }
        ExprKind::Field(a, ..) => besuche_expr(ck, a, namen),
        ExprKind::Index(a, b) => {
            besuche_expr(ck, a, namen);
            besuche_expr(ck, b, namen);
        }
        ExprKind::Syscall(args) | ExprKind::ArrayLit(args) => {
            for a in args {
                besuche_expr(ck, a, namen);
            }
        }
        ExprKind::Cast(a, _) => besuche_expr(ck, a, namen),
        ExprKind::StructLit(_, felder, _) => {
            for (_, a, _) in felder {
                besuche_expr(ck, a, namen);
            }
        }
        ExprKind::ArrayRepeat(a, b) => {
            besuche_expr(ck, a, namen);
            besuche_expr(ck, b, namen);
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

    fn baue(src: &str) -> (String, String) {
        zuruecksetzen();
        crate::profil::zuruecksetzen();
        let mut dg = crate::diag::Diags::new("kern_test", src);
        let toks = crate::lexer::lex(src, &mut dg);
        let mut prog = crate::parser::parse(&toks, &mut dg);
        crate::mono::expand(&mut prog, &mut dg);
        crate::profil::festlegen(&prog, None);
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
    fn asm_bleibt_trotz_unbenutztem_ergebnis_stehen() {
        // DIE FALLE AUS RUNDE 40: das Ergebnis wird nie gelesen. Ein
        // Optimierer, der `Op::Asm` fuer rein haelt, wirft die Zeile weg.
        let (asm, _) = baue(
            "profile kernel\nfn f() { let _x: u64 = asm(\"rdtsc\", out(\"rax\"), clobber(\"rdx\")) }\n",
        );
        assert!(asm.contains("rdtsc"), "asm-block verschwunden:\n{}", asm);
    }

    #[test]
    fn zwei_gleiche_asm_bloecke_werden_nicht_zusammengelegt() {
        let (asm, _) = baue(
            "profile kernel\nfn f() { asm(\"cli\")\n asm(\"cli\") }\n",
        );
        assert_eq!(asm.matches("cli").count(), 2, "CSE hat zugeschlagen:\n{}", asm);
    }

    #[test]
    fn mmio_zugriffe_werden_nicht_zusammengelegt() {
        let (asm, _) = baue(
            "profile kernel\nfn f(p: *mut u32) -> u32 { let a: u32 = __mmio_lesen32(p)\n let b: u32 = __mmio_lesen32(p)\n return a + b }\n",
        );
        assert_eq!(
            asm.matches("dword ptr [rcx]").count(),
            2,
            "zwei MMIO-Lasten wurden zu einer:\n{}",
            asm
        );
    }

    #[test]
    fn gesperrtes_register_wird_benannt() {
        let (_, fehler) = baue("profile kernel\nfn f() { asm(\"nop\", clobber(\"rbx\")) }\n");
        assert!(fehler.contains("rbx"), "{}", fehler);
        assert!(fehler.contains("callee-saved"), "{}", fehler);
    }

    #[test]
    fn interrupt_kann_nicht_gerufen_werden() {
        let (_, fehler) = baue(
            "profile kernel\n#[interrupt]\nfn ih() { asm(\"nop\") }\nfn f() { ih() }\n",
        );
        assert!(fehler.contains("interrupt-einsprungpunkt"), "{}", fehler);
    }

    #[test]
    fn interrupt_endet_mit_iretq() {
        let (asm, fehler) = baue("profile kernel\n#[interrupt]\nfn ih() { asm(\"nop\") }\n");
        assert!(!fehler.contains("error"), "{}", fehler);
        assert!(asm.contains("iretq"), "{}", asm);
        assert!(asm.contains("push r15"), "{}", asm);
    }
}
