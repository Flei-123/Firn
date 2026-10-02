// SPDX-License-Identifier: MPL-2.0
//! **ROUND OWN-1 -- the move checker** (SPEC 3.3, ROADMAP r18).
//!
//! A type is *non-trivial* when destroying a value of it means running code:
//! a struct with `fn drop(inout self)` in its `impl`, or a struct / array that
//! contains such a value. Every other type is trivial and is simply copied --
//! this pass never looks at it. A program without a single `drop` is not
//! touched at all.
//!
//! A non-trivial value has exactly one owner. These uses *move* it:
//!
//! * `let y = x`, `y = x`, `return x`
//! * `f(x)` -- a call argument, also the receiver of a method that takes
//!   `self` by value
//! * a struct field or an array element in a literal
//!
//! After a move the name is dead: reading it, taking `&x` / `inout x` or
//! moving it again is an error ("use of moved value"). Assigning a new value
//! (`x = fresh()`) revives it.
//!
//! The rules are deliberately conservative, as SPEC 3.3 asks ("conditional
//! moves are rejected conservatively instead of being quietly given a
//! run-time flag"):
//!
//! * a value moved in one branch of an `if` but not in the other (or not
//!   leaving through `return`/`break`/`continue`) is an error;
//! * a value declared outside a loop cannot be moved inside it;
//! * no partial moves: a field, an array element or `*p` of a non-trivial
//!   type cannot be moved out of its owner;
//! * a move inside `defer` is not supported yet.
//!
//! The pass also checks the shape of `drop`: `fn drop(inout self)`, no result.
//!
//! The result for the lowering (ROUND OWN-2, `lower.rs`): the set of
//! identifier expressions that move a variable, and per `if` which branches
//! leave. With them the lowering knows at every exit which locals still own
//! a value and calls their `drop` in reverse order of declaration.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diag::Span;
use crate::sema::{Checker, FnSig};
use crate::types::{Type, TypeCtx};

/// `// HOOK moves` in `sema::Checker::run`.
pub(crate) fn hook_check(ck: &mut Checker, prog: &Program) {
    let drops = drop_types(&ck.tcx, &ck.fns);
    // 1. the shape of every `drop`
    for f in &prog.funcs {
        if let Some(prefix) = f.name.strip_suffix("__drop") {
            let idx = match ck.tcx.by_name.get(prefix) {
                Some(i) => *i,
                None => continue,
            };
            let ok = match ck.fns.get(&f.name) {
                Some(FnSig { params, ret }) => {
                    *ret == Type::Void
                        && params.len() == 1
                        && matches!(&params[0], Type::Ptr { mutable: true, inner }
                            if matches!(**inner, Type::Struct(i) if i == idx))
                }
                None => true,
            };
            if !ok {
                ck.dg.error_note(
                    f.span,
                    format!("'drop' of '{}' has the wrong shape", prefix),
                    "a destructor is written 'fn drop(inout self)' and returns nothing",
                );
            }
        }
    }
    if drops.is_empty() {
        return;
    }
    // 2. the moves
    let mut errors: Vec<(Span, String, String)> = Vec::new();
    let mut moved: HashSet<ExprId> = HashSet::new();
    let mut if_leaves: HashMap<ExprId, (bool, bool)> = HashMap::new();
    for f in &prog.funcs {
        if f.extern_info.is_some() {
            continue;
        }
        let sig = ck.fns.get(&f.name);
        let mut w = Walk {
            tcx: &ck.tcx,
            fns: &ck.fns,
            tys: &ck.expr_types,
            drops: &drops,
            vars: Vec::new(),
            loop_depth: 0,
            in_defer: 0,
            errors: &mut errors,
            moved: &mut moved,
            if_leaves: &mut if_leaves,
        };
        for (i, p) in f.params.iter().enumerate() {
            let nt = sig
                .and_then(|s| s.params.get(i))
                .map(|t| w.nontrivial(t))
                .unwrap_or(false);
            w.declare(&p.name, nt);
        }
        w.block(&f.body);
    }
    errors.sort_by_key(|(s, _, _)| (s.file, s.line, s.col));
    errors.dedup_by(|a, b| a.0.line == b.0.line && a.0.col == b.0.col && a.1 == b.1);
    for (span, msg, note) in errors {
        ck.dg.error_note(span, msg, note);
    }
    ck.drops = drops;
    ck.moved = moved;
    ck.if_leaves = if_leaves;
}

/// Does destroying a value of this type run code? A struct with a `drop`,
/// or a struct / array that contains one.
pub(crate) fn nontrivial_in(tcx: &TypeCtx, drops: &HashSet<usize>, t: &Type, depth: u32) -> bool {
    if depth > 32 {
        return false;
    }
    match t {
        Type::Struct(i) => {
            if drops.contains(i) {
                return true;
            }
            match tcx.structs.get(*i) {
                Some(s) => s.fields.iter().any(|f| nontrivial_in(tcx, drops, &f.ty, depth + 1)),
                None => false,
            }
        }
        Type::Array(inner, _) => nontrivial_in(tcx, drops, inner, depth + 1),
        _ => false,
    }
}

/// The structs that have a `drop` of their own.
fn drop_types(tcx: &TypeCtx, fns: &HashMap<String, FnSig>) -> HashSet<usize> {
    let mut out = HashSet::new();
    for (i, s) in tcx.structs.iter().enumerate() {
        let prefix = s.name.strip_prefix("gc ").unwrap_or(&s.name);
        if fns.contains_key(&format!("{}__drop", prefix)) {
            out.insert(i);
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum St {
    Live,
    /// Moved at this place.
    Moved(Span),
}

struct Var {
    name: String,
    nt: bool,
    state: St,
    /// Loop nesting where it was declared.
    loop_depth: u32,
}

struct Walk<'a> {
    tcx: &'a TypeCtx,
    fns: &'a HashMap<String, FnSig>,
    tys: &'a [Type],
    drops: &'a HashSet<usize>,
    vars: Vec<Var>,
    loop_depth: u32,
    in_defer: u32,
    errors: &'a mut Vec<(Span, String, String)>,
    /// Out: identifier expressions that move a variable.
    moved: &'a mut HashSet<ExprId>,
    /// Out: per `if` (key: id of the condition) which branches leave.
    if_leaves: &'a mut HashMap<ExprId, (bool, bool)>,
}

impl<'a> Walk<'a> {
    fn declare(&mut self, name: &str, nt: bool) {
        self.vars.push(Var {
            name: name.to_string(),
            nt,
            state: St::Live,
            loop_depth: self.loop_depth,
        });
    }

    fn lookup(&self, name: &str) -> Option<usize> {
        self.vars.iter().rposition(|v| v.name == name)
    }

    fn err(&mut self, span: Span, msg: String, note: &str) {
        self.errors.push((span, msg, note.to_string()));
    }

    /// Does destroying a value of this type run code?
    fn nontrivial(&self, t: &Type) -> bool {
        self.nontrivial_d(t, 0)
    }

    fn nontrivial_d(&self, t: &Type, depth: u32) -> bool {
        nontrivial_in(self.tcx, self.drops, t, depth)
    }

    fn ty(&self, e: &Expr) -> Option<&'a Type> {
        self.tys.get(e.id as usize)
    }

    fn is_nt(&self, e: &Expr) -> bool {
        match self.ty(e) {
            Some(t) => self.nontrivial(t),
            None => false,
        }
    }

    // ------------------------------------------------------------ statements

    /// Returns true when the block surely leaves (return/break/continue).
    fn block(&mut self, b: &Block) -> bool {
        let mark = self.vars.len();
        let mut leaves = false;
        for s in &b.stmts {
            if self.stmt(s) {
                leaves = true;
            }
        }
        self.vars.truncate(mark);
        leaves
    }

    fn states(&self, n: usize) -> Vec<St> {
        self.vars.iter().take(n).map(|v| v.state).collect()
    }

    fn set_states(&mut self, s: &[St]) {
        for (v, st) in self.vars.iter_mut().zip(s.iter()) {
            v.state = *st;
        }
    }

    fn stmt(&mut self, s: &Stmt) -> bool {
        match s {
            Stmt::Let { name, init, .. } => {
                self.consume(init);
                let nt = self.is_nt(init);
                self.declare(name, nt);
                false
            }
            Stmt::Assign { target, value, .. } => {
                self.consume(value);
                if let ExprKind::Ident(n) = &target.kind {
                    if let Some(i) = self.lookup(n) {
                        // a new value revives the name
                        self.vars[i].state = St::Live;
                        return false;
                    }
                }
                self.read(target);
                false
            }
            Stmt::AssignOp { target, value, .. } => {
                self.read(value);
                self.read(target);
                false
            }
            Stmt::Step { target, .. } => {
                self.read(target);
                false
            }
            Stmt::If { cond, then, els, span } => {
                self.read(cond);
                let n = self.vars.len();
                let before = self.states(n);
                let then_leaves = self.block(then);
                let after_then = self.states(n);
                self.set_states(&before);
                let else_leaves = match els {
                    Some(e) => self.stmt(e),
                    None => false,
                };
                self.if_leaves.insert(cond.id, (then_leaves, else_leaves));
                let after_else = self.states(n);
                self.merge(*span, n, &before, then_leaves, &after_then, else_leaves, &after_else);
                then_leaves && else_leaves
            }
            Stmt::While { cond, body, .. } => {
                self.loop_depth += 1;
                self.read(cond);
                self.block(body);
                self.loop_depth -= 1;
                false
            }
            Stmt::For { name, start, end, body, .. } => {
                self.read(start);
                self.read(end);
                self.loop_depth += 1;
                let mark = self.vars.len();
                self.declare(name, false);
                self.block(body);
                self.vars.truncate(mark);
                self.loop_depth -= 1;
                false
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.consume(v);
                }
                true
            }
            Stmt::Break(_) | Stmt::Continue(_) => true,
            Stmt::Defer(inner, _, _) => {
                self.in_defer += 1;
                self.stmt(inner);
                self.in_defer -= 1;
                false
            }
            Stmt::Expr(e) => {
                if matches!(e.kind, ExprKind::Call(..)) && self.is_nt(e) {
                    self.err(
                        e.span,
                        "the result of this call owns a value that is thrown away".to_string(),
                        "bind it with 'let' (it is dropped at the end of the block) or pass it on",
                    );
                }
                self.consume(e);
                false
            }
            Stmt::Block(b) => self.block(b),
            Stmt::Error(_) => false,
        }
    }

    /// The state after an `if`: what the branches that do not leave agree on.
    #[allow(clippy::too_many_arguments)]
    fn merge(
        &mut self,
        span: Span,
        n: usize,
        before: &[St],
        then_leaves: bool,
        a: &[St],
        else_leaves: bool,
        b: &[St],
    ) {
        if then_leaves && else_leaves {
            self.set_states(before);
            return;
        }
        if then_leaves {
            self.set_states(b);
            return;
        }
        if else_leaves {
            self.set_states(a);
            return;
        }
        let mut out: Vec<St> = Vec::with_capacity(n);
        for i in 0..n {
            let (x, y) = (a[i], b[i]);
            match (x, y) {
                (St::Live, St::Live) => out.push(St::Live),
                (St::Moved(s), St::Moved(_)) => out.push(St::Moved(s)),
                (St::Moved(s), St::Live) | (St::Live, St::Moved(s)) => {
                    if self.vars[i].nt {
                        let name = self.vars[i].name.clone();
                        self.err(
                            span,
                            format!("'{}' is moved in only one branch", name),
                            "move it in every branch (or leave through return/break/continue \
                             in the other): a conditional move would need a run-time flag",
                        );
                    }
                    out.push(St::Moved(s));
                }
            }
        }
        self.set_states(&out);
    }

    // ----------------------------------------------------------- expressions

    /// The value of `e` is handed over: a non-trivial place is MOVED.
    fn consume(&mut self, e: &Expr) {
        if !self.is_nt(e) {
            self.read(e);
            return;
        }
        match &e.kind {
            ExprKind::Ident(n) => {
                if let Some(i) = self.lookup(n) {
                    self.use_var(i, e.span, n);
                    if self.in_defer > 0 {
                        self.err(
                            e.span,
                            format!("'{}' is moved inside 'defer'", n),
                            "moving a value in a deferred statement is not supported yet",
                        );
                    } else if self.vars[i].loop_depth < self.loop_depth {
                        self.err(
                            e.span,
                            format!("'{}' is moved inside a loop", n),
                            "it would be used again in the next round; move it before the loop \
                             or create it inside",
                        );
                    }
                    if self.vars[i].nt {
                        self.vars[i].state = St::Moved(e.span);
                        self.moved.insert(e.id);
                    }
                }
            }
            ExprKind::Field(b, ..) => {
                self.err(
                    e.span,
                    "cannot move a field out of its owner".to_string(),
                    "partial moves are not supported: move the whole value or pass '&' / 'inout'",
                );
                self.read(b);
            }
            ExprKind::Index(b, i) => {
                self.err(
                    e.span,
                    "cannot move an element out of an array".to_string(),
                    "pass '&' / 'inout' of the element instead",
                );
                self.read(b);
                self.read(i);
            }
            ExprKind::Unary(UnOp::Deref, b) => {
                self.err(
                    e.span,
                    "cannot move out of memory behind a pointer".to_string(),
                    "the pointer does not own the value",
                );
                self.read(b);
            }
            ExprKind::IfElse(c, a, b) => {
                self.read(c);
                let n = self.vars.len();
                let before = self.states(n);
                self.consume(a);
                let sa = self.states(n);
                self.set_states(&before);
                self.consume(b);
                let sb = self.states(n);
                self.merge(e.span, n, &before, false, &sa, false, &sb);
            }
            _ => self.read(e),
        }
    }

    fn use_var(&mut self, i: usize, span: Span, name: &str) {
        if let St::Moved(at) = self.vars[i].state {
            self.err(
                span,
                format!("use of moved value '{}'", name),
                &format!("it was moved at line {}:{} and not given a new value", at.line, at.col),
            );
        }
    }

    /// `e` is looked at, not handed over.
    fn read(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Ident(n) => {
                if let Some(i) = self.lookup(n) {
                    self.use_var(i, e.span, n);
                }
            }
            ExprKind::Field(b, ..) => self.read(b),
            ExprKind::Index(b, i) => {
                self.read(b);
                self.read(i);
            }
            ExprKind::Unary(_, a) | ExprKind::Cast(a, _) | ExprKind::Text(_, a) => self.read(a),
            ExprKind::Binary(_, a, b) => {
                self.read(a);
                self.read(b);
            }
            ExprKind::ArrayRepeat(a, b) => {
                self.consume(a);
                self.read(b);
            }
            ExprKind::IfElse(c, a, b) => {
                self.read(c);
                let n = self.vars.len();
                let before = self.states(n);
                self.read(a);
                let sa = self.states(n);
                self.set_states(&before);
                self.read(b);
                let sb = self.states(n);
                self.merge(e.span, n, &before, false, &sa, false, &sb);
            }
            ExprKind::Call(name, args, _) if name == crate::foreach::LEN => {
                // `for x in array`: the length is only looked at.
                for a in args {
                    self.read(a);
                }
            }
            ExprKind::Call(name, args, _) => {
                let by_address_recv = match crate::impls::method_name(name) {
                    Some(m) => {
                        let recv = args.first().and_then(|a| self.ty(a));
                        match recv {
                            Some(t) => crate::impls::target_of(self.tcx, self.fns, m, t)
                                .map(|(_, addr)| addr)
                                .unwrap_or(false),
                            None => false,
                        }
                    }
                    None => false,
                };
                for (i, a) in args.iter().enumerate() {
                    if i == 0 && by_address_recv {
                        self.read(a);
                    } else {
                        self.consume(a);
                    }
                }
            }
            ExprKind::Syscall(args) | ExprKind::ArrayLit(args) => {
                for a in args {
                    self.consume(a);
                }
            }
            ExprKind::StructLit(_, fields, _) => {
                for (_, a, _) in fields {
                    self.consume(a);
                }
            }
            ExprKind::Lambda(d) => {
                // A closure body is a function of its own: its captures are
                // words, its own locals are checked from scratch.
                let mut inner = Walk {
                    tcx: self.tcx,
                    fns: self.fns,
                    tys: self.tys,
                    drops: self.drops,
                    vars: Vec::new(),
                    loop_depth: 0,
                    in_defer: 0,
                    errors: &mut *self.errors,
                    moved: &mut *self.moved,
                    if_leaves: &mut *self.if_leaves,
                };
                for p in &d.params {
                    inner.declare(&p.name, false);
                }
                inner.block(&d.body);
            }
            ExprKind::Float(..)
            | ExprKind::FloatF32(_)
            | ExprKind::Int(_)
            | ExprKind::Bool(_) => {}
        }
    }
}
