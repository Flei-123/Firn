//! **Profiles `kernel` and `app` (SPEC.md §2) — round 52.**
//!
//! Up to round 51 `profile` was a declaration that was parsed, checked for
//! its name and did **nothing else** (SPEC §14, point 6). This file makes
//! it come true.
//!
//! ## Where the profile comes from
//!
//! 1. `--profile=kernel` or `--profile=app` on the command line —
//!    forces the profile for the WHOLE compilation unit (SPEC §2).
//! 2. otherwise `profile kernel` / `profile app` on the first line of the
//!    root file.
//! 3. otherwise `app`.
//!
//! ## What `kernel` forbids — and how the compiler notices
//!
//! | SPEC §2 says | checked here |
//! |---|---|
//! | no global allocator, no runtime | `import std.*` rejected |
//! | no `Gc[T]` (tracing collector) | `gc class` rejected |
//! | no unwinding / `throw` | `#[unwinds]` rejected |
//! | no hidden allocation | follows from both: the only allocation the
//! |   | compiler itself puts there is that of the collector |
//! | floating point only with `#[allow_fp]` | `f64` and float literals |
//! | freestanding | `syscall` rejected, no `_start`, ELF object |
//!
//! `syscall` does not appear in the table of SPEC §2, but belongs there
//! inevitably: below a freestanding kernel there is no operating system
//! that could accept a system call. That single rule renders the whole
//! standard library unusable under the kernel profile — every allocation
//! there goes through `mmap`, every output through `write`. It is thereby
//! the sharpest of the six.
//!
//! ## Where the checks hang
//!
//! * `modules.rs::build_program` — the `import` rule (only there are the
//!   inclusions of EVERY file known with their position),
//! * `sema.rs::check_profile` — all the rest,
//! * `core.rs` — inline assembler and `#[interrupt]`,
//! * `codegen_x86.rs` — no `_start`, no runtime prologue,
//! * `main.rs` — ELF object rather than executable file.

use std::cell::Cell;

use crate::ast::{Block, Expr, ExprKind, Program, Stmt, TypeExpr};
use crate::diag::{Diags, Span};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Profile {
    Kernel,
    App,
}

thread_local! {
    /// What `--profile=` said (`None` = nothing said).
    static FLAG: Cell<Option<Profile>> = const { Cell::new(None) };
    /// The resolved profile of this compilation unit.
    static ACTIVE: Cell<Profile> = const { Cell::new(Profile::App) };
}

/// Evaluate the `--profile=<name>` flag. `Err` = unknown name.
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

/// Fix the profile from the declaration; the command line wins.
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

/// Name of the active profile (error messages, `--stats`).
pub fn name() -> &'static str {
    match active() {
        Profile::Kernel => "kernel",
        Profile::App => "app",
    }
}

/// Reset everything — only for self tests, which compile several programs
/// in ONE process.
#[cfg(test)]
pub(crate) fn reset() {
    FLAG.with(|f| f.set(None));
    ACTIVE.with(|a| a.set(Profile::App));
}

// ------------------------------------------------------------- import ---

/// `// HOOK profil` in `modules.rs::build_program`.
///
/// Under the kernel profile the standard library is barred: it presumes a
/// global allocator (`mmap`) and Linux system calls. Modules of your own
/// stay allowed — the kernel is made of them, after all.
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

/// `// HOOK profil` in `sema::check_profile`. Checks everything visible
/// in the AST of the merged compilation unit.
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
    // 1. tracing collector
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
    // 2. functions: unwinding, floating point, system calls
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
    // 3. constants and structures
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

/// Inline assembler and MMIO exist under BOTH profiles.
///
/// That is a deliberate decision and no sloppiness: both are escape
/// hatches to the machine, and even an application needs them now and then
/// (`rdtsc`, `cpuid`, a device mapped through `/dev/mem`). The price — the
/// code is nailed to x86-64 — stands in the source text, where anybody
/// sees it. The gain is provability: only that way can the volatile
/// guarantees be checked in a program that REALLY RUNS
/// (`tests/85x_*.fi`), rather than in the generated assembler text alone.
///
/// Only `#[interrupt]` stays reserved for the kernel profile (`core.rs`) —
/// an application has no interrupt vector table.
pub fn hook_asm(_ck: &mut crate::sema::Checker, _span: Span) {}

// --------------------------------------------------------------- Guards ---

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
        // Deliberate decision (see hook_asm): only that way is the
        // volatile guarantee checkable in a running program.
        let t = error_of("profile app\nfn f() { asm(\"nop\") }\nfn main() -> i32 { return 0 }\n");
        assert!(!t.contains("error"), "{}", t);
    }

    #[test]
    fn app_forbids_interrupt() {
        let t = error_of("profile app\n#[interrupt]\nfn ih() { asm(\"nop\") }\nfn main() -> i32 { return 0 }\n");
        assert!(t.contains("only in profile 'kernel'"), "{}", t);
    }
}
