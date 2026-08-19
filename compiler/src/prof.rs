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
//! * `core.rs` — Inline-Assembler und `#[interrupt]`,
//! * `codegen_x86.rs` — kein `_start`, kein Laufzeitvorspann,
//! * `main.rs` — ELF-Objekt statt ausfuehrbarer Datei.

use std::cell::Cell;

use crate::ast::{Block, Expr, ExprKind, Program, Stmt, TypeExpr};
use crate::diag::{Diags, Span};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Profile {
    Kernel,
    App,
}

thread_local! {
    /// Was `--profile=` gesagt hat (`None` = nichts gesagt).
    static FLAG: Cell<Option<Profile>> = const { Cell::new(None) };
    /// Das aufgeloeste Profil dieser Uebersetzungseinheit.
    static ACTIVE: Cell<Profile> = const { Cell::new(Profile::App) };
}

/// `--profile=<name>` auswerten. `Err` = unbekannter Name.
pub fn flag_set(name: &str) -> Result<(), String> {
    let p = match name {
        "kernel" => Profile::Kernel,
        "app" => Profile::App,
        other => {
            return Err(format!(
                "unknown profile '{}' (allowed: kernel, app)",
                other
            ))
        }
    };
    FLAG.with(|f| f.set(Some(p)));
    ACTIVE.with(|a| a.set(p));
    Ok(())
}

/// Profil aus der Deklaration festlegen; die Kommandozeile gewinnt.
pub fn define(prog: &Program, _unused: Option<()>) {
    if let Some(p) = FLAG.with(|f| f.get()) {
        ACTIVE.with(|a| a.set(p));
        return;
    }
    let p = match prog.profile.as_ref().map(|(n, _)| n.as_str()) {
        Some("kernel") => Profile::Kernel,
        _ => Profile::App,
    };
    ACTIVE.with(|a| a.set(p));
}

pub fn active() -> Profile {
    ACTIVE.with(|a| a.get())
}

pub fn is_kernel() -> bool {
    active() == Profile::Kernel
}

/// Name des aktiven Profils (Fehlermeldungen, `--stats`).
pub fn name() -> &'static str {
    match active() {
        Profile::Kernel => "kernel",
        Profile::App => "app",
    }
}

/// Alles zuruecksetzen — nur fuer Selbsttests, die mehrere Programme in
/// EINEM Prozess uebersetzen.
#[cfg(test)]
pub(crate) fn reset() {
    FLAG.with(|f| f.set(None));
    ACTIVE.with(|a| a.set(Profile::App));
}

// ------------------------------------------------------------- import ---

/// `// HOOK profil` in `modules.rs::build_program`.
///
/// Im Kernel-Profil ist die Standardbibliothek gesperrt: sie setzt einen
/// globalen Allokator (`mmap`) und Linux-Systemaufrufe voraus. Eigene Module
/// bleiben erlaubt — der Kernel besteht ja aus ihnen.
pub fn hook_import(dg: &mut Diags, path: &[String], span: Span) {
    if !is_kernel() {
        return;
    }
    if path.first().map(|s| s.as_str()) != Some("std") {
        return;
    }
    dg.error_note(
        span,
        format!(
            "the module '{}' belongs to the standard library and is not available in profile 'kernel'",
            path.join(".")
        ),
        "SPEC §2: the kernel profile has no global allocator and no runtime; \
         the standard library presupposes both (mmap, write)",
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
                format!("unknown profile '{}'", n),
                "allowed are 'kernel' and 'app'",
            );
        }
    }
    define(prog, None);
    if !is_kernel() {
        return;
    }
    // 1. Tracing-Sammler
    if crate::gc::has_classes() {
        let span = prog
            .profile
            .as_ref()
            .map(|(_, s)| *s)
            .unwrap_or_else(|| prog.funcs.first().map(|f| f.span).unwrap_or(Span::in_file(0, 1, 1, 1)));
        dg.error_note(
            span,
            "'gc class' needs the tracing collector; there are no GC types in profile 'kernel'"
                .to_string(),
            "SPEC §2: Gc[T] is not available in the kernel profile — the collector needs \
             a global heap, which a freestanding kernel does not have",
        );
    }
    // 2. Funktionen: Abwicklung, Gleitkomma, Systemaufrufe
    for f in &prog.funcs {
        if f.attrs.iter().any(|a| a.name == "unwinds") {
            dg.error_note(
                f.span,
                format!(
                    "'{}' is marked with #[unwinds]; unwinding is forbidden in profile 'kernel'",
                    f.name
                ),
                "SPEC §2: in the kernel profile errors run over result types (§5.1), \
                 not over unwinding",
            );
        }
        let fp_allowed = f.attrs.iter().any(|a| a.name == "allow_fp");
        let mut w = Guard { dg, fp_allowed, func: f.name.clone() };
        if let Some(t) = &f.ret {
            w.ty(t);
        }
        for p in &f.params {
            w.ty(&p.ty);
        }
        w.block(&f.body);
    }
    // 3. Konstanten und Strukturen
    for c in &prog.consts {
        let mut w = Guard { dg, fp_allowed: false, func: c.name.clone() };
        w.ty(&c.ty);
        w.expr(&c.value);
    }
    for s in &prog.structs {
        let fp_allowed = s.attrs.iter().any(|a| a.name == "allow_fp");
        let mut w = Guard { dg, fp_allowed, func: s.name.clone() };
        for (_, te, _) in &s.fields {
            w.ty(te);
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
/// Nur `#[interrupt]` bleibt dem Kernel-Profil vorbehalten (`core.rs`) — eine
/// Anwendung hat keine Unterbrechungsvektortabelle.
pub fn hook_asm(_ck: &mut crate::sema::Checker, _span: Span) {}

// ------------------------------------------------------------- Waechter ---

struct Guard<'a> {
    dg: &'a mut Diags,
    fp_allowed: bool,
    func: String,
}

impl Guard<'_> {
    fn fp(&mut self, span: Span, what: &str) {
        if self.fp_allowed {
            return;
        }
        self.dg.error_note(
            span,
            format!(
                "floating point ({}) is allowed in profile 'kernel' only with #[allow_fp] — '{}' does not have the attribute",
                what, self.func
            ),
            "SPEC §2: in the kernel the FPU/SSE registers belong to the interrupted thread; \
             whoever touches them must save their state himself",
        );
    }

    fn ty(&mut self, t: &TypeExpr) {
        match t {
            TypeExpr::Named(n, s) => {
                if n == "f64" {
                    self.fp(*s, "the type f64");
                }
            }
            TypeExpr::Ptr { inner, .. } => self.ty(inner),
            TypeExpr::Array { elem, .. } => self.ty(elem),
        }
    }

    fn block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let { ty, init, .. } => {
                if let Some(t) = ty {
                    self.ty(t);
                }
                self.expr(init);
            }
            Stmt::Assign { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            Stmt::If { cond, then, els, .. } => {
                self.expr(cond);
                self.block(then);
                if let Some(e) = els {
                    self.stmt(e);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.expr(cond);
                self.block(body);
            }
            Stmt::For { start, end, body, .. } => {
                self.expr(start);
                self.expr(end);
                self.block(body);
            }
            Stmt::Return { value, .. } => {
                if let Some(e) = value {
                    self.expr(e);
                }
            }
            Stmt::Defer(inner, _, _) => self.stmt(inner),
            Stmt::Expr(e) => self.expr(e),
            Stmt::Block(b) => self.block(b),
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Float(_) => self.fp(e.span, "a floating point literal"),
            ExprKind::Syscall(args) => {
                self.dg.error_note(
                    e.span,
                    "'syscall' does not exist in profile 'kernel'".to_string(),
                    "under a freestanding kernel there is no operating system \
                     that could accept a system call",
                );
                for a in args {
                    self.expr(a);
                }
            }
            ExprKind::Cast(a, t) => {
                self.expr(a);
                self.ty(t);
            }
            ExprKind::Unary(_, a) => self.expr(a),
            ExprKind::Binary(_, a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Field(a, ..) => self.expr(a),
            ExprKind::Index(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Call(_, args, _) | ExprKind::ArrayLit(args) => {
                for a in args {
                    self.expr(a);
                }
            }
            ExprKind::StructLit(_, fields, _) => {
                for (_, a, _) in fields {
                    self.expr(a);
                }
            }
            ExprKind::ArrayRepeat(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Ident(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_of(src: &str) -> String {
        reset();
        crate::core::reset();
        let mut dg = crate::diag::Diags::new("profile_test", src);
        let toks = crate::lexer::lex(src, &mut dg);
        let mut prog = crate::parser::parse(&toks, &mut dg);
        crate::mono::expand(&mut prog, &mut dg);
        let _ = crate::sema::check(&prog, &mut dg);
        dg.render()
    }

    #[test]
    fn kernel_forbids_syscall_and_names_es() {
        let t = error_of("profile kernel\nfn main() -> i32 { syscall(60, 0)\n return 0 }\n");
        assert!(t.contains("'syscall' does not exist in profile 'kernel'"), "{}", t);
    }

    #[test]
    fn kernel_forbids_float_without_attr() {
        let t = error_of("profile kernel\nfn f(x: f64) -> f64 { return x }\n");
        assert!(t.contains("floating point"), "{}", t);
        assert!(t.contains("#[allow_fp]"), "{}", t);
    }

    #[test]
    fn allow_fp_makes_float_again_possible() {
        let t = error_of("profile kernel\n#[allow_fp]\nfn f(x: f64) -> f64 { return x }\n");
        assert!(!t.contains("floating point"), "{}", t);
    }

    #[test]
    fn app_stays_untouched() {
        let t = error_of("profile app\nfn f(x: f64) -> f64 { return x }\nfn main() -> i32 { return 0 }\n");
        assert!(!t.contains("floating point"), "{}", t);
    }

    #[test]
    fn app_allowed_inline_assembler() {
        // Bewusste Entscheidung (siehe hook_asm): nur so ist die
        // volatile-Zusage in einem laufenden Programm pruefbar.
        let t = error_of("profile app\nfn f() { asm(\"nop\") }\nfn main() -> i32 { return 0 }\n");
        assert!(!t.contains("error"), "{}", t);
    }

    #[test]
    fn app_forbids_interrupt() {
        let t = error_of("profile app\n#[interrupt]\nfn ih() { asm(\"nop\") }\nfn main() -> i32 { return 0 }\n");
        assert!(t.contains("only in profile 'kernel'"), "{}", t);
    }
}
