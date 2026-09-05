// SPDX-License-Identifier: GPL-2.0-only
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
//! | no global allocator, no runtime | `import std.*` rejected unless the
//! |   | module declares `profile kernel` itself (round 73) |
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
//! ## Round 73 — `import std.*` is no longer refused wholesale
//!
//! Up to round 72 EVERY `import std.*` was rejected. That was too coarse:
//! `Span::trim`, `find`, `compare`, the UTF-8 reader, `text_to_i64` and the
//! whole of `math` ask nobody for memory and make no system call. They fell
//! under the ban only because they stood in the same FILE as the functions
//! that call `mmap`. Round 73 pulled them out into `lib/std/core.fi`.
//!
//! **The rule now: a `std` module may be imported under `kernel` if it
//! declares `profile kernel` in its own first line.** Everything else stays
//! forbidden, with the message unchanged.
//!
//! That is deliberately **not** a name list in the compiler. Two reasons:
//!
//! 1. The declaration is a CLAIM by the module, and the claim is
//!    **checked**, not believed. Firn compiles whole programs: an imported
//!    module lands in the SAME compilation unit as the kernel that imports
//!    it, so `hook_check` below walks its functions as well. A `syscall`
//!    hidden in `lib/std/core.fi` is an error at the line where it stands
//!    (`tests/neg/core_kernel_syscall.fi`), and so are `gc class`,
//!    `#[unwinds]` and unmarked floating point. Nothing has to be
//!    maintained for that — the apparatus that already guards a kernel
//!    guards the library it imports.
//! 2. A new freestanding module needs no compiler change. Whoever writes
//!    one writes `profile kernel` into its first line and is done.
//!
//! What the claim does NOT cover, said plainly: a module may declare
//! `profile kernel` and be imported into an APP program, where nobody
//! checks it. That costs nothing — under `app` a `syscall` is allowed
//! anyway. The guarantee arises exactly where it is needed.
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
///
/// **ROUND ARM-FREESTANDING** adds the third source, and it is the weakest
/// of the three on purpose: a FREESTANDING TARGET (`--target=x86_64-none`,
/// `--target=aarch64-none`) turns the kernel profile on when neither the
/// command line nor the source has said anything. It cannot be otherwise —
/// every single thing the kernel profile forbids (system calls, the
/// standard library, `_start`, a collector) is forbidden by the ABSENCE OF
/// AN OPERATING SYSTEM, not by a word in line 1. The declaration in the
/// source keeps winning over it, so that a source which says `profile app`
/// is not silently turned into something else — that combination is a
/// contradiction and `hook_check` reports it as one.
pub fn define(prog: &Program, _unused: Option<()>) {
    if let Some(p) = FLAG.with(|f| f.get()) {
        ACTIVE.with(|a| a.set(p));
        return;
    }
    let p = match prog.profile.as_ref().map(|(n, _)| n.as_str()) {
        Some("kernel") => Profile::Kernel,
        Some(_) => Profile::App,
        // Nothing said at all: the target decides.
        None if crate::target::freestanding() => Profile::Kernel,
        None => Profile::App,
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
///
/// **Round 73:** a `std` module that declares `profile kernel` ITSELF is
/// admitted. `module_is_kernel` says whether the module being imported
/// carries that declaration; `modules.rs` reads it off the parsed module,
/// so the same source text decides in both compilers. The claim is checked
/// afterwards by `hook_check` — the module lies in the same compilation
/// unit as its importer (see the header of this file).
pub fn hook_import(dg: &mut Diags, path: &[String], span: Span, module_is_kernel: bool) {
    if !is_kernel() {
        return;
    }
    if path.first().map(|s| s.as_str()) != Some("std") {
        return;
    }
    if module_is_kernel {
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
    // ROUND ARM-FREESTANDING: `profile app` under a target that has no
    // operating system. Everything the `app` profile presumes -- `write`,
    // `mmap`, `exit_group`, a `_start` that a loader jumps to -- comes from
    // the operating system, and there is none. Reported here, once, at the
    // declaration itself rather than 200 lines later at the first `syscall`.
    if crate::target::freestanding() && !is_kernel() {
        let span = prog
            .profile
            .as_ref()
            .map(|(_, s)| *s)
            .unwrap_or_else(|| {
                prog.funcs.first().map(|f| f.span).unwrap_or(Span::in_file(0, 1, 1, 1))
            });
        dg.error_note(
            span,
            format!(
                "profile 'app' cannot be built for the target '{}'",
                crate::target::active().name()
            ),
            "the target name ends in '-none': there is no operating system under it, \
             and the app profile presupposes one (write, mmap, exit_group, a _start \
             that a loader jumps to) -- write 'profile kernel' or choose a target \
             with an operating system",
        );
        return;
    }
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
                if crate::types::canon_name(n) == "f64" || crate::types::canon_name(n) == "f32" {
                    self.fp(*s, if crate::types::canon_name(n) == "f32" { "the type f32" } else { "the type f64" });
                }
            }
            TypeExpr::Ptr { inner, .. } => self.ty(inner),
            TypeExpr::Array { elem, .. } => self.ty(elem),
            TypeExpr::Fn { params, ret, .. } => {
                for p in params {
                    self.ty(p);
                }
                if let Some(r) = ret {
                    self.ty(r);
                }
            }
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
            Stmt::Assign { target, value, .. }
            | Stmt::AssignOp { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            // ROUND 70: the step has no value expression.
            Stmt::Step { target, .. } => self.expr(target),
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
            // ROUND 71: `f32` falls under the same rule as `f64` —
            // floating point in the kernel profile only with #[allow_fp].
            ExprKind::Float(..) | ExprKind::FloatF32(_) => self.fp(e.span, "a floating point literal"),
            // ROUND 70: the text literal carries its array literal inside.
            ExprKind::Text(_, inner) => self.expr(inner),
            // Round 58: the body of a closure is checked like any other.
            ExprKind::Lambda(d) => {
                for p in &d.params {
                    self.ty(&p.ty);
                }
                if let Some(t) = &d.ret {
                    self.ty(t);
                }
                self.block(&d.body);
            }
            ExprKind::Syscall(args) => {
                // ROUND ARM-FREESTANDING: the same refusal, but it names the
                // reason the reader actually has. Somebody who wrote
                // `--target=aarch64-none` never typed the word "kernel"
                // anywhere and would have had to guess where that message
                // came from.
                let (what, why) = if crate::target::freestanding() {
                    (
                        format!(
                            "'syscall' does not exist on the target '{}'",
                            crate::target::active().name()
                        ),
                        "the target name ends in '-none': there is no operating system \
                         under it that could accept a system call -- a freestanding \
                         program reaches its machine through 'asm', MMIO and its own \
                         drivers",
                    )
                } else {
                    (
                        "'syscall' does not exist in profile 'kernel'".to_string(),
                        "under a freestanding kernel there is no operating system \
                         that could accept a system call",
                    )
                };
                self.dg.error_note(e.span, what, why);
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
