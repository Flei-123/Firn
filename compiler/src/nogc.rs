// SPDX-License-Identifier: GPL-2.0-only
//! `#[no_gc]` — the guarantee for hot paths, SPEC §3.5.4.
//!
//! In a `#[no_gc]` function these are forbidden:
//!  1. GC allocation (and every call that can trigger a collection run),
//!  2. calling a function **without** `#[no_gc]`,
//!  3. writing a `Gc[T]`/`GcWeak[T]` pointer into a field or a field
//!     element (insertion barrier).
//!
//! The check is **transitive**: because every function called must carry
//! `#[no_gc]` itself, the guarantee holds for the whole call tree. Once the
//! chain breaks — across several levels or across a module boundary too —
//! there is an error with line and column.
//!
//! Wired up through the line `// HOOK nogc` in `sema::Checker::run`. The
//! GC specific queries (1 and 3) come from `gc.rs` — this file knows the GC
//! through those two functions only and is therefore buildable without it.
//! For the self tests at the end of the file both queries are bundled
//! in `Rules`, so that rules 1 and 3 are provable even without a built
//! collector; inside the compiler **always** `Rules::real()` runs.
//!
//! ## Scope, honestly stated
//!
//! * Checked are all calls in the body, **including** the body blocks of
//!   `match` cases (those do not sit in the AST but in the registry of
//!   `sema_match.rs`).
//! * Compiler internal call names (`__match#N`, `__try#`, `__catch#`,
//!   `Enum::Variant`) are no function calls and trigger no error; their
//!   arguments are searched nonetheless.
//! * Calls of a name that does not exist at all are reported by the type
//!   check itself — no second, confusing error for it here.
//! * Calls through function pointers do not exist at stage 0 (`ExprKind::Call`
//!   always carries a name), so there is no loophole here.
//!
//! This file belongs to the module `nogc` (PLAN.md, round "hardening test 2").

use std::collections::HashMap;

use crate::ast::{Block, Expr, ExprKind, FnDecl, Program, Stmt};
use crate::diag::Span;
use crate::sema::Checker;
use crate::types::Type;

/// The two GC queries this file needs. Inside the compiler always
/// `Rules::real()` (contract of `gc.rs`); the self tests substitute their own
/// predicates, so that rules 1 and 3 are checkable before the collector
/// stands.
#[derive(Clone, Copy)]
struct Rules {
    is_alloc: fn(&str) -> bool,
    is_gc_ref: fn(&Type) -> bool,
}

impl Rules {
    fn real() -> Rules {
        Rules {
            is_alloc: crate::gc::is_gc_alloc_call,
            is_gc_ref: crate::gc::is_gc_ref,
        }
    }
}

/// Does the function carry `#[no_gc]`?
pub(crate) fn has_no_gc(f: &FnDecl) -> bool {
    f.attrs.iter().any(|a| a.name == "no_gc")
}

/// Call name produced compiler internally (no function call in the source).
///
/// `__match#N` (sema_match.rs), `__try#`/`__catch#` (errors.rs) and
/// `Enum::Variant` (lower_match.rs) can never arise from an identifier
/// of the source text — they contain `#` or `::`.
fn is_interner_name(name: &str) -> bool {
    name.contains('#') || name.contains("::")
}

/// Write `helper__square` (module system, `modules.rs`) as `helper.square`
/// again — the message shall show the name that stands in the source.
fn readable(name: &str) -> String {
    if name.starts_with('_') || is_interner_name(name) {
        return name.to_string();
    }
    let parts: Vec<&str> = name.split("__").collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        return format!("{}.{}", parts[0], parts[1]);
    }
    name.to_string()
}

/// `// HOOK nogc` in `sema::Checker::run`: checks all `#[no_gc]` functions.
pub(crate) fn hook_check(ck: &mut Checker, prog: &Program) {
    let findings = collect_findings(prog, &ck.expr_types, Rules::real());
    for (span, msg, note) in findings {
        ck.dg.error_note(span, msg, note);
    }
}

/// The pass proper, without `Checker` — testable on its own that way.
fn collect_findings(
    prog: &Program,
    expr_types: &[Type],
    rules: Rules,
) -> Vec<(Span, String, String)> {
    let mut marked: HashMap<&str, bool> = HashMap::new();
    for f in &prog.funcs {
        // With names declared twice (a separate error of the type check)
        // the stricter entry counts: marked stays marked.
        let e = marked.entry(f.name.as_str()).or_insert(false);
        *e |= has_no_gc(f);
    }
    if !marked.values().any(|v| *v) {
        return Vec::new();
    }
    let mut p = NoGcChecker {
        expr_types,
        marked: &marked,
        rules,
        who: String::new(),
        depth: 0,
        out: Vec::new(),
    };
    for f in &prog.funcs {
        if !has_no_gc(f) {
            continue;
        }
        p.who = readable(&f.name);
        p.check_block(&f.body);
    }
    let mut out = p.out;
    out.sort_by_key(|(s, _, _)| (s.file, s.line, s.col));
    out.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    out
}

struct NoGcChecker<'a> {
    expr_types: &'a [Type],
    marked: &'a HashMap<&'a str, bool>,
    rules: Rules,
    /// Name of the `#[no_gc]` function checked right now (for the message).
    who: String,
    /// Nesting depth of the `match` body blocks (rip cord, see below).
    depth: u32,
    out: Vec<(Span, String, String)>,
}

/// Highest nesting of `match` cases that is looked into. The parser caps
/// the nesting at 200 anyway; this bound is the second safeguard against
/// a recursion explosion.
const MAX_DEPTH: u32 = 256;

impl<'a> NoGcChecker<'a> {
    fn ty_of(&self, e: &Expr) -> Type {
        self.expr_types
            .get(e.id as usize)
            .cloned()
            .unwrap_or(Type::Error)
    }

    fn report(&mut self, span: Span, msg: String, note: String) {
        self.out.push((span, msg, note));
    }

    fn check_block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.check_stmt(s);
        }
    }

    fn check_stmt(&mut self, s: &Stmt) {
        match s {
            // The deferred body runs in the same frame and is subject to
            // the same rules.
            Stmt::Defer(inner, _, _) => self.check_stmt(inner),
            Stmt::Let { init, .. } => self.check_expr(init),
            Stmt::Assign { target, value, span }
            | Stmt::AssignOp { target, value, span, .. } => {
                self.check_write_target(target, *span);
                self.check_expr(target);
                self.check_expr(value);
            }
            // ROUND 70: the step writes into the target as well.
            Stmt::Step { target, span, .. } => {
                self.check_write_target(target, *span);
                self.check_expr(target);
            }
            Stmt::If { cond, then, els, .. } => {
                self.check_expr(cond);
                self.check_block(then);
                if let Some(e) = els {
                    self.check_stmt(e);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.check_expr(cond);
                self.check_block(body);
            }
            Stmt::For { start, end, body, .. } => {
                self.check_expr(start);
                self.check_expr(end);
                self.check_block(body);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.check_expr(v);
                }
            }
            Stmt::Expr(e) => self.check_expr(e),
            Stmt::Block(b) => self.check_block(b),
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
        }
    }

    /// Rule 3: writing a GC pointer into a field (or into a field
    /// element). A plain assignment to a local variable
    /// (`ExprKind::Ident`) sits on the stack, needs no insertion barrier
    /// and is allowed.
    fn check_write_target(&mut self, target: &Expr, fallback: Span) {
        let ty = self.ty_of(target);
        if !(self.rules.is_gc_ref)(&ty) {
            return;
        }
        let (what, sp) = match &target.kind {
            ExprKind::Field(_, name, sp) => (format!("the GC field '{}'", name), *sp),
            ExprKind::Index(b, _) => match &b.kind {
                ExprKind::Ident(_) => return, // local field on the stack
                _ => ("a GC element in memory".to_string(), target.span),
            },
            ExprKind::Unary(_, _) => ("a GC field behind a pointer".to_string(), target.span),
            _ => return,
        };
        let sp = if sp == Span::none() { fallback } else { sp };
        let who = self.who.clone();
        self.report(
            sp,
            format!("'{who}' is #[no_gc], but writes into {what}"),
            "SPEC 3.5.4: writing a Gc pointer into the heap needs the insertion \
             barrier and is forbidden in a #[no_gc] call tree"
                .to_string(),
        );
    }

    fn check_expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::FloatF32(_) => {}
            // ROUND 70: the text literal carries its array literal inside.
            ExprKind::Text(_, inner) => self.check_expr(inner),
            // Round 58: `gc fn(…)` allocates — inside `#[no_gc]` that has to
            // strike, and so has everything the body does.
            ExprKind::Lambda(d) => {
                if d.heap {
                    let who = self.who.clone();
                    self.report(
                        d.span,
                        format!(
                            "'{who}' is #[no_gc], but allocates on the GC heap via 'gc fn(…)'"
                        ),
                        "SPEC 3.5.4: no collection run may be triggered in a #[no_gc] \
                 call tree"
                            .to_string(),
                    );
                }
                self.check_block(&d.body);
            }
            ExprKind::Call(name, args, sp) => {
                self.check_call(name, *sp, e.span);
                for a in args {
                    self.check_expr(a);
                }
                // `match` appears as the call `__match#N` in the AST; the
                // body blocks of the cases sit in the registry of
                // sema_match.rs. Without this descent every state machine
                // would be a blind spot.
                self.check_match_cases(name);
            }
            ExprKind::Unary(_, a) => self.check_expr(a),
            ExprKind::Binary(_, a, b) => {
                self.check_expr(a);
                self.check_expr(b);
            }
            ExprKind::Field(b, _, _) => self.check_expr(b),
            ExprKind::Index(b, i) => {
                self.check_expr(b);
                self.check_expr(i);
            }
            ExprKind::Syscall(args) | ExprKind::ArrayLit(args) => {
                for a in args {
                    self.check_expr(a);
                }
            }
            ExprKind::Cast(a, _) => self.check_expr(a),
            ExprKind::StructLit(_, fields, _) => {
                for (_, v, _) in fields {
                    self.check_expr(v);
                }
            }
            ExprKind::ArrayRepeat(v, n) => {
                self.check_expr(v);
                self.check_expr(n);
            }
            ExprKind::Int(_) | ExprKind::Float(..) | ExprKind::Bool(_) | ExprKind::Ident(_) => {}
        }
    }

    fn check_call(&mut self, name: &str, sp: Span, fallback: Span) {
        let sp = if sp == Span::none() { fallback } else { sp };
        let who = self.who.clone();
        if (self.rules.is_alloc)(name) {
            // Rule 1: GC allocation — can trigger a collection run.
            self.report(
                sp,
                format!(
                    "'{who}' is #[no_gc], but allocates on the GC heap via '{}'",
                    readable(name)
                ),
                "SPEC 3.5.4: no collection run may be triggered in a #[no_gc] \
                 call tree"
                    .to_string(),
            );
            return;
        }
        // HOOK impl: up to the type check `x.m(..)` appears in the tree as
        // `"method m"` — only the type checker knows which function is meant.
        // This check runs without a type table of the receivers, so the case
        // is rejected EXPLICITLY rather than passed over silently: a hole
        // in a guarantee would be worse than a missing convenience
        // (round 45, impls.rs).
        if let Some(m) = crate::impls::method_name(name) {
            self.report(
                sp,
                format!("'{who}' is #[no_gc], but calls the method '{m}'"),
                "SPEC 3.5.4: the promise holds transitively — which function stands behind a \
                 method call is decided by the receiver type; call the function \
                 directly here (Type__method) or give up #[no_gc]"
                    .to_string(),
            );
            return;
        }
        if is_interner_name(name) {
            return;
        }
        match self.marked.get(name) {
            Some(false) => {
                // Rule 2: call without #[no_gc] — breaks the chain.
                self.report(
                    sp,
                    format!(
                        "'{who}' is #[no_gc], but calls '{}' without #[no_gc]",
                        readable(name)
                    ),
                    format!(
                        "SPEC 3.5.4: the promise holds transitively for the whole call tree — \
                         write #[no_gc] before '{}' or do not call it here",
                        readable(name)
                    ),
                );
            }
            // marked: fine. Unknown: the type check reports the unknown
            // name itself, no second error here.
            Some(true) | None => {}
        }
    }

    fn check_match_cases(&mut self, name: &str) {
        let idx = match name
            .strip_prefix(crate::sema_match::MATCH_PREFIX)
            .and_then(|s| s.parse::<usize>().ok())
        {
            Some(i) => i,
            None => return,
        };
        let mi = match crate::sema_match::match_info(idx) {
            Some(m) => m,
            None => return,
        };
        if self.depth >= MAX_DEPTH {
            return;
        }
        self.depth += 1;
        self.check_expr(&mi.subject);
        for arm in &mi.arms {
            self.check_block(&arm.body);
        }
        self.depth -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Attr, ExprId};

    // -------------------------------------------------------- AST by hand

    fn span(line: u32, col: u32) -> Span {
        Span { file: 0, line, col, len: 1 }
    }

    struct Build {
        next: ExprId,
    }

    impl Build {
        fn new() -> Build {
            Build { next: 0 }
        }
        fn expr(&mut self, sp: Span, kind: ExprKind) -> Expr {
            let id = self.next;
            self.next += 1;
            Expr { id, span: sp, kind }
        }
        fn call(&mut self, name: &str, sp: Span) -> Expr {
            self.expr(sp, ExprKind::Call(name.to_string(), Vec::new(), sp))
        }
        fn ident(&mut self, name: &str, sp: Span) -> Expr {
            self.expr(sp, ExprKind::Ident(name.to_string()))
        }
        fn field(&mut self, base: Expr, name: &str, sp: Span) -> Expr {
            self.expr(sp, ExprKind::Field(Box::new(base), name.to_string(), sp))
        }
    }

    fn fndecl(name: &str, no_gc: bool, stmts: Vec<Stmt>) -> FnDecl {
        FnDecl {
            name: name.to_string(),
            params: Vec::new(),
            ret: None,
            body: Block { stmts, span: span(1, 1), end: span(1, 1) },
            span: span(1, 1),
            attrs: if no_gc {
                vec![Attr { name: "no_gc".to_string(), args: Vec::new(), span: span(1, 1) }]
            } else {
                Vec::new()
            },
            extern_info: None,
        }
    }

    fn program(funcs: Vec<FnDecl>, expr_count: u32) -> Program {
        Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs,
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            expr_count,
            comptime_blocks: Vec::new(),
        }
    }

    /// Predicates for the self tests: `gc_new` allocates, `Gc[T]` is
    /// represented by `Type::Ptr`.
    fn test_rules() -> Rules {
        fn alloc(n: &str) -> bool {
            n == "gc_new"
        }
        fn ptr(t: &Type) -> bool {
            matches!(t, Type::Ptr { .. })
        }
        Rules { is_alloc: alloc, is_gc_ref: ptr }
    }

    // -------------------------------------------------------------- Rules

    #[test]
    fn regel1_gc_allocation_is_forbidden() {
        let mut b = Build::new();
        let call = b.call("gc_new", span(7, 12));
        let n = b.next;
        let prog = program(vec![fndecl("hot", true, vec![Stmt::Expr(call)])], n);
        let findings = collect_findings(&prog, &vec![Type::I32; n as usize], test_rules());
        assert_eq!(findings.len(), 1, "{:?}", findings);
        assert_eq!((findings[0].0.line, findings[0].0.col), (7, 12));
        assert!(findings[0].1.contains("allocates"), "{}", findings[0].1);
    }

    #[test]
    fn regel2_call_without_no_gc_is_forbidden() {
        let mut b = Build::new();
        let call = b.call("cold", span(9, 5));
        let n = b.next;
        let prog = program(
            vec![
                fndecl("hot", true, vec![Stmt::Expr(call)]),
                fndecl("cold", false, Vec::new()),
            ],
            n,
        );
        let findings = collect_findings(&prog, &vec![Type::I32; n as usize], test_rules());
        assert_eq!(findings.len(), 1, "{:?}", findings);
        assert_eq!((findings[0].0.line, findings[0].0.col), (9, 5));
        assert!(findings[0].1.contains("without #[no_gc]"), "{}", findings[0].1);
    }

    #[test]
    fn regel2_marked_call_is_allowed() {
        let mut b = Build::new();
        let call = b.call("also_hot", span(9, 5));
        let n = b.next;
        let prog = program(
            vec![
                fndecl("hot", true, vec![Stmt::Expr(call)]),
                fndecl("also_hot", true, Vec::new()),
            ],
            n,
        );
        assert!(collect_findings(&prog, &vec![Type::I32; n as usize], test_rules()).is_empty());
    }

    #[test]
    fn regel3_write_in_gc_field_is_forbidden() {
        let mut b = Build::new();
        let base = b.ident("node", span(4, 5));
        let target = b.field(base, "parent", span(4, 12));
        let value = b.ident("other", span(4, 26));
        let n = b.next;
        let target_id = target.id as usize;
        let stmt = Stmt::Assign { target: target, value: value, span: span(4, 5) };
        let prog = program(vec![fndecl("hot", true, vec![stmt])], n);
        let mut types = vec![Type::I32; n as usize];
        types[target_id] = Type::Ptr { mutable: true, inner: Box::new(Type::I32) };
        let findings = collect_findings(&prog, &types, test_rules());
        assert_eq!(findings.len(), 1, "{:?}", findings);
        assert_eq!((findings[0].0.line, findings[0].0.col), (4, 12));
        assert!(findings[0].1.contains("GC field 'parent'"), "{}", findings[0].1);
    }

    #[test]
    fn regel3_assign_an_local_mutable_is_allowed() {
        let mut b = Build::new();
        let target = b.ident("x", span(4, 5));
        let value = b.ident("y", span(4, 9));
        let n = b.next;
        let target_id = target.id as usize;
        let stmt = Stmt::Assign { target: target, value: value, span: span(4, 5) };
        let prog = program(vec![fndecl("hot", true, vec![stmt])], n);
        let mut types = vec![Type::I32; n as usize];
        types[target_id] = Type::Ptr { mutable: true, inner: Box::new(Type::I32) };
        assert!(collect_findings(&prog, &types, test_rules()).is_empty());
    }

    #[test]
    fn unmarked_func_becomes_not_checked() {
        let mut b = Build::new();
        let call = b.call("gc_new", span(3, 3));
        let n = b.next;
        let prog = program(vec![fndecl("cold", false, vec![Stmt::Expr(call)])], n);
        assert!(collect_findings(&prog, &vec![Type::I32; n as usize], test_rules()).is_empty());
    }

    #[test]
    fn internal_names_solve_no_error_out() {
        let mut b = Build::new();
        let m = b.call("__match#0", span(3, 3));
        let t = b.call("__try#", span(4, 3));
        let c = b.call("Color::Red", span(5, 3));
        let u = b.call("does_not_exist", span(6, 3));
        let n = b.next;
        let prog = program(
            vec![fndecl(
                "hot",
                true,
                vec![Stmt::Expr(m), Stmt::Expr(t), Stmt::Expr(c), Stmt::Expr(u)],
            )],
            n,
        );
        assert!(collect_findings(&prog, &vec![Type::I32; n as usize], test_rules()).is_empty());
    }

    #[test]
    fn module_name_becomes_readable_reported() {
        let mut b = Build::new();
        let call = b.call("helper__square", span(11, 12));
        let n = b.next;
        let prog = program(
            vec![
                fndecl("hot", true, vec![Stmt::Expr(call)]),
                fndecl("helper__square", false, Vec::new()),
            ],
            n,
        );
        let findings = collect_findings(&prog, &vec![Type::I32; n as usize], test_rules());
        assert_eq!(findings.len(), 1, "{:?}", findings);
        assert!(findings[0].1.contains("'helper.square'"), "{}", findings[0].1);
    }

    #[test]
    fn depth_nesting_becomes_reaches() {
        // The violation sits in a chain of if inside while inside if.
        let mut b = Build::new();
        let call = b.call("cold", span(20, 9));
        let cond1 = b.expr(span(10, 1), ExprKind::Bool(true));
        let cond2 = b.expr(span(11, 1), ExprKind::Bool(true));
        let inner = Stmt::If {
            cond: cond2,
            then: Block { stmts: vec![Stmt::Expr(call)], span: span(11, 1), end: span(11, 1) },
            els: None,
            span: span(11, 1),
        };
        let mid = Stmt::While {
            cond: cond1,
            body: Block { stmts: vec![inner], span: span(10, 1), end: span(10, 1) },
            span: span(10, 1),
        };
        let n = b.next;
        let prog = program(
            vec![
                fndecl("hot", true, vec![Stmt::Block(Block { stmts: vec![mid], span: span(9, 1), end: span(9, 1) })]),
                fndecl("cold", false, Vec::new()),
            ],
            n,
        );
        let findings = collect_findings(&prog, &vec![Type::I32; n as usize], test_rules());
        assert_eq!(findings.len(), 1, "{:?}", findings);
        assert_eq!((findings[0].0.line, findings[0].0.col), (20, 9));
    }

    #[test]
    fn has_no_gc_recognizes_the_attr() {
        assert!(has_no_gc(&fndecl("a", true, Vec::new())));
        assert!(!has_no_gc(&fndecl("a", false, Vec::new())));
    }

    #[test]
    fn real_rules_are_the_out_gc_rs() {
        // The contract: the compiler asks gc.rs exclusively.
        let r = Rules::real();
        assert_eq!((r.is_alloc)("main"), crate::gc::is_gc_alloc_call("main"));
        assert_eq!((r.is_gc_ref)(&Type::I32), crate::gc::is_gc_ref(&Type::I32));
    }
}
