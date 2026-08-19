//! **Profile `kernel` und `app` (SPEC.md §2) — Runde 52.**
//!
//! Bis Runde 51 war `profile` eine Deklaration, die geparst und auf ihren
//! Namen geprueft wurde und **sonst nichts tat** (SPEC §14, Punkt 6). Diese
//! Datei macht sie wahr.
//!
//! ## Woher das Profil kommt
//!
//! 1. `--profile=kernel` bzw. `--profile=app` auf der Kommandozeile —
//!    erzwingt das Profil fuer die GANZE Uebersetzungseinheit (SPEC §2).
//! 2. sonst `profile kernel` / `profile app` in der ersten Zeile der
//!    Wurzeldatei.
//! 3. sonst `app`.
//!
//! ## Was `kernel` verbietet — und woran es der Compiler merkt
//!
//! | SPEC §2 sagt | hier geprueft |
//! |---|---|
//! | kein globaler Allokator, keine Laufzeit | `import std.*` abgelehnt |
//! | keine `Gc[T]` (Tracing-Sammler) | `gc class` abgelehnt |
//! | keine Abwicklung / `throw` | `#[unwinds]` abgelehnt |
//! | keine versteckte Allokation | ergibt sich aus beidem: die einzige vom
//! |   | Compiler selbst eingesetzte Allokation ist die des Sammlers |
//! | Gleitkomma nur mit `#[allow_fp]` | `f64` und Gleitkommaliterale |
//! | freistehend | `syscall` abgelehnt, kein `_start`, ELF-Objekt |
//!
//! `syscall` steht nicht in der Tabelle von SPEC §2, gehoert aber zwingend
//! dazu: unter einem freistehenden Kernel liegt kein Betriebssystem, das
//! einen Systemaufruf entgegennehmen koennte. Genau diese eine Regel macht
//! die gesamte Standardbibliothek im Kernel-Profil unbenutzbar — jede
//! Allokation dort geht ueber `mmap`, jede Ausgabe ueber `write`. Sie ist
//! damit die schaerfste der sechs.
//!
//! ## Wo die Pruefungen haengen
//!
//! * `modules.rs::build_program` — `import`-Regel (nur dort sind die
//!   Einbindungen JEDER Datei mit ihrer Position bekannt),
//! * `sema.rs::check_profile` — alles Uebrige,
//! * `kern.rs` — Inline-Assembler und `#[interrupt]`,
//! * `codegen_x86.rs` — kein `_start`, kein Laufzeitvorspann,
//! * `main.rs` — ELF-Objekt statt ausfuehrbarer Datei.

use std::cell::Cell;

use crate::ast::{Block, Expr, ExprKind, Program, Stmt, TypeExpr};
use crate::diag::{Diags, Span};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Profil {
    Kernel,
    App,
}

thread_local! {
    /// Was `--profile=` gesagt hat (`None` = nichts gesagt).
    static FLAGGE: Cell<Option<Profil>> = const { Cell::new(None) };
    /// Das aufgeloeste Profil dieser Uebersetzungseinheit.
    static AKTIV: Cell<Profil> = const { Cell::new(Profil::App) };
}

/// `--profile=<name>` auswerten. `Err` = unbekannter Name.
pub fn flagge_setzen(name: &str) -> Result<(), String> {
    let p = match name {
        "kernel" => Profil::Kernel,
        "app" => Profil::App,
        other => {
            return Err(format!(
                "unbekanntes profil '{}' (erlaubt: kernel, app)",
                other
            ))
        }
    };
    FLAGGE.with(|f| f.set(Some(p)));
    AKTIV.with(|a| a.set(p));
    Ok(())
}

/// Profil aus der Deklaration festlegen; die Kommandozeile gewinnt.
pub fn festlegen(prog: &Program, _unbenutzt: Option<()>) {
    if let Some(p) = FLAGGE.with(|f| f.get()) {
        AKTIV.with(|a| a.set(p));
        return;
    }
    let p = match prog.profile.as_ref().map(|(n, _)| n.as_str()) {
        Some("kernel") => Profil::Kernel,
        _ => Profil::App,
    };
    AKTIV.with(|a| a.set(p));
}

pub fn aktiv() -> Profil {
    AKTIV.with(|a| a.get())
}

pub fn ist_kernel() -> bool {
    aktiv() == Profil::Kernel
}

/// Name des aktiven Profils (Fehlermeldungen, `--stats`).
pub fn name() -> &'static str {
    match aktiv() {
        Profil::Kernel => "kernel",
        Profil::App => "app",
    }
}

/// Alles zuruecksetzen — nur fuer Selbsttests, die mehrere Programme in
/// EINEM Prozess uebersetzen.
#[cfg(test)]
pub(crate) fn zuruecksetzen() {
    FLAGGE.with(|f| f.set(None));
    AKTIV.with(|a| a.set(Profil::App));
}

// ------------------------------------------------------------- import ---

/// `// HOOK profil` in `modules.rs::build_program`.
///
/// Im Kernel-Profil ist die Standardbibliothek gesperrt: sie setzt einen
/// globalen Allokator (`mmap`) und Linux-Systemaufrufe voraus. Eigene Module
/// bleiben erlaubt — der Kernel besteht ja aus ihnen.
pub fn hook_import(dg: &mut Diags, pfad: &[String], span: Span) {
    if !ist_kernel() {
        return;
    }
    if pfad.first().map(|s| s.as_str()) != Some("std") {
        return;
    }
    dg.error_note(
        span,
        format!(
            "das modul '{}' gehoert zur standardbibliothek und ist im profil 'kernel' nicht verfuegbar",
            pfad.join(".")
        ),
        "SPEC §2: das kernel-profil hat keinen globalen allokator und keine laufzeit; \
         die standardbibliothek setzt beides voraus (mmap, write)",
    );
}

// ------------------------------------------------------------- sema ---

/// `// HOOK profil` in `sema::check_profile`. Prueft alles, was am AST der
/// zusammengefuehrten Uebersetzungseinheit sichtbar ist.
pub fn hook_check(dg: &mut Diags, prog: &Program) {
    if let Some((n, span)) = &prog.profile {
        if n != "kernel" && n != "app" {
            dg.error_note(
                *span,
                format!("unbekanntes profil '{}'", n),
                "erlaubt sind 'kernel' und 'app'",
            );
        }
    }
    festlegen(prog, None);
    if !ist_kernel() {
        return;
    }
    // 1. Tracing-Sammler
    if crate::gc::hat_klassen() {
        let span = prog
            .profile
            .as_ref()
            .map(|(_, s)| *s)
            .unwrap_or_else(|| prog.funcs.first().map(|f| f.span).unwrap_or(Span::in_file(0, 1, 1, 1)));
        dg.error_note(
            span,
            "'gc class' braucht den tracing-sammler; im profil 'kernel' gibt es keine GC-typen"
                .to_string(),
            "SPEC §2: Gc[T] ist im kernel-profil nicht verfuegbar — der sammler braucht \
             einen globalen heap, den ein freistehender kernel nicht hat",
        );
    }
    // 2. Funktionen: Abwicklung, Gleitkomma, Systemaufrufe
    for f in &prog.funcs {
        if f.attrs.iter().any(|a| a.name == "unwinds") {
            dg.error_note(
                f.span,
                format!(
                    "'{}' ist mit #[unwinds] gekennzeichnet; abwicklung ist im profil 'kernel' verboten",
                    f.name
                ),
                "SPEC §2: fehler laufen im kernel-profil ueber ergebnistypen (§5.1), \
                 nicht ueber abwicklung",
            );
        }
        let fp_erlaubt = f.attrs.iter().any(|a| a.name == "allow_fp");
        let mut w = Waechter { dg, fp_erlaubt, funktion: f.name.clone() };
        if let Some(t) = &f.ret {
            w.typ(t);
        }
        for p in &f.params {
            w.typ(&p.ty);
        }
        w.block(&f.body);
    }
    // 3. Konstanten und Strukturen
    for c in &prog.consts {
        let mut w = Waechter { dg, fp_erlaubt: false, funktion: c.name.clone() };
        w.typ(&c.ty);
        w.ausdruck(&c.value);
    }
    for s in &prog.structs {
        let fp_erlaubt = s.attrs.iter().any(|a| a.name == "allow_fp");
        let mut w = Waechter { dg, fp_erlaubt, funktion: s.name.clone() };
        for (_, te, _) in &s.fields {
            w.typ(te);
        }
    }
}

/// Inline-Assembler und MMIO gibt es in BEIDEN Profilen.
///
/// Das ist eine bewusste Entscheidung und keine Nachlaessigkeit: beides ist
/// eine Fluchtluke zur Maschine, und die braucht auch eine Anwendung
/// gelegentlich (`rdtsc`, `cpuid`, ein per `/dev/mem` eingeblendetes Geraet).
/// Der Preis — der Code ist an x86-64 genagelt — steht im Quelltext, wo ihn
/// jeder sieht. Der Gewinn ist Nachweisbarkeit: nur so lassen sich die
/// volatile-Zusagen in einem Programm pruefen, das WIRKLICH LAEUFT
/// (`tests/85x_*.fi`), statt nur im erzeugten Assemblertext.
///
/// Nur `#[interrupt]` bleibt dem Kernel-Profil vorbehalten (`kern.rs`) — eine
/// Anwendung hat keine Unterbrechungsvektortabelle.
pub fn hook_asm(_ck: &mut crate::sema::Checker, _span: Span) {}

// ------------------------------------------------------------- Waechter ---

struct Waechter<'a> {
    dg: &'a mut Diags,
    fp_erlaubt: bool,
    funktion: String,
}

impl Waechter<'_> {
    fn fp(&mut self, span: Span, was: &str) {
        if self.fp_erlaubt {
            return;
        }
        self.dg.error_note(
            span,
            format!(
                "gleitkomma ({}) ist im profil 'kernel' nur mit #[allow_fp] erlaubt — '{}' hat das attribut nicht",
                was, self.funktion
            ),
            "SPEC §2: die FPU/SSE-register gehoeren im kernel dem unterbrochenen faden; \
             wer sie anfasst, muss ihren zustand selbst retten",
        );
    }

    fn typ(&mut self, t: &TypeExpr) {
        match t {
            TypeExpr::Named(n, s) => {
                if n == "f64" {
                    self.fp(*s, "der typ f64");
                }
            }
            TypeExpr::Ptr { inner, .. } => self.typ(inner),
            TypeExpr::Array { elem, .. } => self.typ(elem),
        }
    }

    fn block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.anweisung(s);
        }
    }

    fn anweisung(&mut self, s: &Stmt) {
        match s {
            Stmt::Let { ty, init, .. } => {
                if let Some(t) = ty {
                    self.typ(t);
                }
                self.ausdruck(init);
            }
            Stmt::Assign { target, value, .. } => {
                self.ausdruck(target);
                self.ausdruck(value);
            }
            Stmt::If { cond, then, els, .. } => {
                self.ausdruck(cond);
                self.block(then);
                if let Some(e) = els {
                    self.anweisung(e);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.ausdruck(cond);
                self.block(body);
            }
            Stmt::For { start, end, body, .. } => {
                self.ausdruck(start);
                self.ausdruck(end);
                self.block(body);
            }
            Stmt::Return { value, .. } => {
                if let Some(e) = value {
                    self.ausdruck(e);
                }
            }
            Stmt::Defer(inner, _, _) => self.anweisung(inner),
            Stmt::Expr(e) => self.ausdruck(e),
            Stmt::Block(b) => self.block(b),
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
        }
    }

    fn ausdruck(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Float(_) => self.fp(e.span, "ein gleitkommaliteral"),
            ExprKind::Syscall(args) => {
                self.dg.error_note(
                    e.span,
                    "'syscall' gibt es im profil 'kernel' nicht".to_string(),
                    "unter einem freistehenden kernel liegt kein betriebssystem, \
                     das einen systemaufruf entgegennehmen koennte",
                );
                for a in args {
                    self.ausdruck(a);
                }
            }
            ExprKind::Cast(a, t) => {
                self.ausdruck(a);
                self.typ(t);
            }
            ExprKind::Unary(_, a) => self.ausdruck(a),
            ExprKind::Binary(_, a, b) => {
                self.ausdruck(a);
                self.ausdruck(b);
            }
            ExprKind::Field(a, ..) => self.ausdruck(a),
            ExprKind::Index(a, b) => {
                self.ausdruck(a);
                self.ausdruck(b);
            }
            ExprKind::Call(_, args, _) | ExprKind::ArrayLit(args) => {
                for a in args {
                    self.ausdruck(a);
                }
            }
            ExprKind::StructLit(_, felder, _) => {
                for (_, a, _) in felder {
                    self.ausdruck(a);
                }
            }
            ExprKind::ArrayRepeat(a, b) => {
                self.ausdruck(a);
                self.ausdruck(b);
            }
            ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Ident(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fehler_von(src: &str) -> String {
        zuruecksetzen();
        crate::kern::zuruecksetzen();
        let mut dg = crate::diag::Diags::new("profil_test", src);
        let toks = crate::lexer::lex(src, &mut dg);
        let mut prog = crate::parser::parse(&toks, &mut dg);
        crate::mono::expand(&mut prog, &mut dg);
        let _ = crate::sema::check(&prog, &mut dg);
        dg.render()
    }

    #[test]
    fn kernel_verbietet_syscall_und_nennt_es() {
        let t = fehler_von("profile kernel\nfn main() -> i32 { syscall(60, 0)\n return 0 }\n");
        assert!(t.contains("'syscall' gibt es im profil 'kernel' nicht"), "{}", t);
    }

    #[test]
    fn kernel_verbietet_gleitkomma_ohne_attribut() {
        let t = fehler_von("profile kernel\nfn f(x: f64) -> f64 { return x }\n");
        assert!(t.contains("gleitkomma"), "{}", t);
        assert!(t.contains("#[allow_fp]"), "{}", t);
    }

    #[test]
    fn allow_fp_macht_gleitkomma_wieder_moeglich() {
        let t = fehler_von("profile kernel\n#[allow_fp]\nfn f(x: f64) -> f64 { return x }\n");
        assert!(!t.contains("gleitkomma"), "{}", t);
    }

    #[test]
    fn app_bleibt_unberuehrt() {
        let t = fehler_von("profile app\nfn f(x: f64) -> f64 { return x }\nfn main() -> i32 { return 0 }\n");
        assert!(!t.contains("gleitkomma"), "{}", t);
    }

    #[test]
    fn app_erlaubt_inline_assembler() {
        // Bewusste Entscheidung (siehe hook_asm): nur so ist die
        // volatile-Zusage in einem laufenden Programm pruefbar.
        let t = fehler_von("profile app\nfn f() { asm(\"nop\") }\nfn main() -> i32 { return 0 }\n");
        assert!(!t.contains("error"), "{}", t);
    }

    #[test]
    fn app_verbietet_interrupt() {
        let t = fehler_von("profile app\n#[interrupt]\nfn ih() { asm(\"nop\") }\nfn main() -> i32 { return 0 }\n");
        assert!(t.contains("nur im profil 'kernel'"), "{}", t);
    }
}
