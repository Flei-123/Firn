// SPDX-License-Identifier: GPL-2.0-only
//! **Round 58** — functions as first class values: function pointers and
//! closures (`docs/ROUND58.md`).
//!
//! ## The representation, binding for both compilers
//!
//! A value of type `fn(A, B) -> R` is **one machine word**: the address of a
//! FUNCTION RECORD.
//!
//! ```text
//! record:  [0]      address of the machine code
//!          [8+8*k]  the k-th captured value of a closure
//! ```
//!
//! * A named function taken as a value gets a record of exactly one word in
//!   `.rodata` (`.L__fnv.<symbol>`). It costs no allocation and works
//!   without the collector, under `profile kernel` and inside `#[no_gc]`.
//! * A closure that captures nothing gets the same kind of static record.
//! * A closure that captures something needs storage for the captured
//!   values. That storage is a GC object of a synthesised class
//!   (`gc.rs::declare_closure_class`), so the collector traces the captured
//!   `Gc[T]` pointers through the ordinary type table — no special path, no
//!   pinning, no external root.
//!
//! ## The call
//!
//! ```text
//! %c = load.ptr [%f]              ; the code address out of word 0
//! %r = calli.R  %c(a, b, %f)      ; the record goes in as the LAST argument
//! ```
//!
//! The record travels as the last argument, which is why a named function
//! needs no shim: System V lets the caller pass one argument too many, and a
//! function that does not know about it never reads it. A closure body is
//! translated as an ordinary function whose last parameter is the record —
//! that is where it reads its captured values from.
//!
//! **A direct call stays direct.** `add(1, 2)` is still `Op::Call` and thus
//! `call add`; the indirection arises only where the target really sits in a
//! value (proof: `tools/fnval/run.sh`).

use std::cell::RefCell;

/// Prefix of every function record in `.rodata`. It is file local (`.L`) and
/// contains a dot, so it can never collide with a symbol of the source text.
const RECORD_LABEL: &str = ".L__fnv.";

#[derive(Default)]
struct Registry {
    /// Symbol names for which a static record has to be emitted, in the
    /// order in which they were first needed (deterministic output).
    records: Vec<String>,
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
}

pub(crate) fn hook_reset() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
}

/// Registers a static record for the function `name` and yields its key.
pub(crate) fn record_of(name: &str) -> String {
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        if !reg.records.iter().any(|x| x == name) {
            reg.records.push(name.to_string());
        }
    });
    name.to_string()
}

pub(crate) fn has_records() -> bool {
    REG.with(|r| !r.borrow().records.is_empty())
}

/// Assembler name of a function record.
pub(crate) fn record_label(key: &str) -> String {
    format!("{}{}", RECORD_LABEL, crate::codegen_x86::label(key))
}

/// All static function records as one `.rodata` block.
pub(crate) fn records_asm() -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let recs: Vec<String> = REG.with(|r| r.borrow().records.clone());
    if recs.is_empty() {
        return out;
    }
    let _ = writeln!(out, "{}", crate::target::reloc_rodata());
    let _ = writeln!(out, "{}", crate::target::align(8));
    for k in recs {
        let sym = crate::codegen_x86::label(&k);
        let _ = writeln!(out, "{}{}:", RECORD_LABEL, sym);
        let _ = writeln!(out, "    .quad {}", sym);
    }
    out
}

// ------------------------------------------------------- Closures (round 58)

use crate::ast::{Expr, ExprKind, LambdaDecl, Param, TypeExpr};
use crate::diag::Span;
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::types::Type;

/// Name of the generated function behind a closure. The `#` makes it
/// impossible to collide with an identifier of the source text.
pub(crate) fn closure_fn(id: u32) -> String {
    format!("__closure#{}", id)
}

/// Name of the synthesised `gc class` that holds the captured values.
fn capture_class(id: u32) -> String {
    format!("__capture#{}", id)
}

/// The parameter through which a closure body reaches its record. It carries
/// a `#` as well, so no source text can name it.
pub(crate) const ENV_PARAM: &str = "__env#";

/// One captured value.
#[derive(Clone, Debug)]
pub(crate) struct Capture {
    pub(crate) name: String,
    pub(crate) ty: Type,
    /// Offset inside the record (word 0 is the code address).
    pub(crate) off: u64,
}

/// Everything the later stages need to know about one closure.
#[derive(Clone, Debug)]
pub(crate) struct ClosureInfo {
    pub(crate) captures: Vec<Capture>,
    /// Type tag and size of the capture class; both 0 without captures.
    pub(crate) tid: u64,
    pub(crate) size: u64,
    /// Struct index of the capture class, `usize::MAX` without captures.
    pub(crate) sidx: usize,
}

thread_local! {
    static LAMBDAS: RefCell<Vec<(u32, ClosureInfo)>> = const { RefCell::new(Vec::new()) };
    static NEXT_ID: RefCell<u32> = const { RefCell::new(0) };
}

pub(crate) fn closure_reset() {
    LAMBDAS.with(|r| r.borrow_mut().clear());
    NEXT_ID.with(|r| *r.borrow_mut() = 0);
}

fn next_id() -> u32 {
    NEXT_ID.with(|r| {
        let mut n = r.borrow_mut();
        let v = *n;
        *n += 1;
        v
    })
}

pub(crate) fn closure_info(id: u32) -> Option<ClosureInfo> {
    LAMBDAS.with(|r| r.borrow().iter().find(|(k, _)| *k == id).map(|(_, i)| i.clone()))
}

fn remember(id: u32, info: ClosureInfo) {
    LAMBDAS.with(|r| {
        let mut v = r.borrow_mut();
        match v.iter_mut().find(|(k, _)| *k == id) {
            Some(slot) => slot.1 = info,
            None => v.push((id, info)),
        }
    });
}

// ------------------------------------------------------------- Parser hook

/// `// HOOK fnval` in `parser::primary` — the closure literal.
///
/// `fn(` in an expression can be nothing else; `gc fn(` is the capturing
/// form. A DECLARATION always has a name after `fn`, so the two never
/// overlap.
pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr> {
    let heap = match p.kind() {
        TokKind::KwFn => false,
        TokKind::Ident(n) if n == "gc" => {
            matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::KwFn))
        }
        _ => return None,
    };
    if !heap && !matches!(p.kind(), TokKind::KwFn) {
        return None;
    }
    let start = p.span();
    if heap {
        p.bump();
    }
    p.bump(); // 'fn'
    if !p.expect(TokKind::LParen, "after 'fn' in a closure") {
        return Some(p.broken_expr(start));
    }
    let params: Vec<Param> = p.params();
    p.close(TokKind::RParen, "after the parameters of a closure");
    p.recovering = false;
    let ret = if p.eat(&TokKind::Arrow) {
        match p.parse_type() {
            Some(t) => Some(t),
            None => return Some(p.broken_expr(start)),
        }
    } else {
        None
    };
    if !p.at(&TokKind::LBrace) {
        p.error_here("expected '{' — the body of a closure");
        return Some(p.broken_expr(start));
    }
    // A closure body is an ordinary block; the `no_struct_lit` state of the
    // surrounding condition must not reach into it.
    let saved = p.no_struct_lit;
    p.no_struct_lit = false;
    let body = p.block("of a closure");
    p.no_struct_lit = saved;
    let span = Parser::join(start, body.span);
    let d = LambdaDecl { id: next_id(), heap, params, ret, body, span };
    Some(p.mk(span, ExprKind::Lambda(Box::new(d))))
}

// ---------------------------------------------------------------- Type check

/// `// HOOK fnval` in `sema::expr_inner` — the closure literal.
pub(crate) fn check_lambda(ck: &mut Checker, d: &LambdaDecl) -> Type {
    let mut params = Vec::new();
    for p in &d.params {
        params.push(ck.resolve_ty(&p.ty));
    }
    let ret = match &d.ret {
        Some(t) => ck.resolve_ty(t),
        None => Type::Void,
    };
    let fty = Type::Fn { params: params.clone(), ret: Box::new(ret.clone()) };

    // The body is checked in a scope of its own. Everything it reads out of
    // the scopes BELOW is a capture — `Checker::capture_base` records that
    // at the places where a name really gets used.
    let base = ck.scopes.len();
    ck.scopes.push(std::collections::HashMap::new());
    ck.capture_frames.push((base, Vec::new()));
    for (p, t) in d.params.iter().zip(params.iter()) {
        ck.declare_var(&p.name, t.clone(), false, p.span);
    }
    let saved_ret = std::mem::replace(&mut ck.ret, ret.clone());
    ck.check_block(&d.body, true);
    ck.ret = saved_ret;
    ck.scopes.pop();
    let found = match ck.capture_frames.pop() {
        Some((_, v)) => v,
        None => Vec::new(),
    };
    if ret != Type::Void && !ret.is_error() && !crate::sema::block_returns(&d.body) {
        ck.dg.error_note(
            d.span,
            format!(
                "the closure reaches the end without 'return' (return type {})",
                ck.tcx.name_of(&ret)
            ),
            "every path has to end in a 'return'",
        );
    }

    // Captured values are COPIED into the record. That is why they have to
    // fit into one word — an aggregate would make the record a variable
    // sized thing that the collector could no longer describe.
    let mut caps: Vec<Capture> = Vec::new();
    let mut off: u64 = 8;
    for (name, ty) in found {
        let size = ck.tcx.size_of(&ty);
        if matches!(ty, Type::Array(..) | Type::Struct(_)) || size > 8 {
            ck.dg.error_note(
                d.span,
                format!(
                    "a closure cannot capture '{}' of type {} ({} octets)",
                    name,
                    ck.tcx.name_of(&ty),
                    size
                ),
                "captured are copies of at most one word; pass an aggregate through a pointer or a Gc[T]",
            );
            continue;
        }
        caps.push(Capture { name, ty, off });
        off += 8;
    }

    ck.fns.insert(closure_fn(d.id), closure_sig(ck, d));
    if caps.is_empty() {
        if d.heap {
            ck.dg.error_note(
                d.span,
                "this closure captures nothing and needs no GC record",
                "write it without the 'gc' in front",
            );
            return Type::Error;
        }
        remember(d.id, ClosureInfo { captures: caps, tid: 0, size: 0, sidx: usize::MAX });
        return fty;
    }
    if !d.heap {
        let names: Vec<String> = caps.iter().map(|c| format!("'{}'", c.name)).collect();
        ck.dg.error_note(
            d.span,
            format!(
                "this closure captures {} and therefore needs storage",
                names.join(", ")
            ),
            "write 'gc fn(…)' — the record then lies in the GC heap and the result is an AllocError!fn(…)",
        );
        return Type::Error;
    }
    let ctypes: Vec<Type> = caps.iter().map(|c| c.ty.clone()).collect();
    let (tid, size, sidx) = crate::gc::declare_capture_class(ck, &capture_class(d.id), &ctypes);
    remember(d.id, ClosureInfo { captures: caps, tid, size, sidx });
    match crate::errors::union_type(ck, crate::gc::ERR_SET, &fty) {
        Some(t) => t,
        None => Type::Error,
    }
}

/// Preview for `sema::probe_d` — the type WITHOUT checking the body.
pub(crate) fn probe_lambda(ck: &Checker, d: &LambdaDecl) -> Option<Type> {
    if d.heap {
        return None;
    }
    let mut params = Vec::new();
    for p in &d.params {
        params.push(ck.resolve_ty_quiet_pub(&p.ty)?);
    }
    let ret = match &d.ret {
        Some(t) => ck.resolve_ty_quiet_pub(t)?,
        None => Type::Void,
    };
    Some(Type::Fn { params, ret: Box::new(ret) })
}

/// The parameter list of the generated function: the closure parameters and
/// the record as the last one.
pub(crate) fn closure_params(d: &LambdaDecl) -> Vec<Param> {
    let mut ps = d.params.clone();
    ps.push(Param {
        name: ENV_PARAM.to_string(),
        ty: TypeExpr::Ptr {
            mutable: true,
            inner: Box::new(TypeExpr::Named("u8".to_string(), d.span)),
            span: d.span,
        },
        span: d.span,
    });
    ps
}

/// Signature of the generated function for `sema::TypeInfo::fns`.
pub(crate) fn closure_sig(ck: &Checker, d: &LambdaDecl) -> crate::sema::FnSig {
    let mut params: Vec<Type> = d.params.iter().map(|p| ck.resolve_ty_quiet_pub(&p.ty).unwrap_or(Type::Error)).collect();
    params.push(Type::ptr(Type::U8, true));
    let ret = match &d.ret {
        Some(t) => ck.resolve_ty_quiet_pub(t).unwrap_or(Type::Error),
        None => Type::Void,
    };
    crate::sema::FnSig { params, ret }
}

/// Collects every closure literal of a function body — `lower.rs` turns each
/// of them into a function of its own.
pub(crate) fn collect(prog: &crate::ast::Program) -> Vec<LambdaDecl> {
    let mut out = Vec::new();
    for f in &prog.funcs {
        walk_block(&f.body, &mut out);
    }
    out
}

fn walk_block(b: &crate::ast::Block, out: &mut Vec<LambdaDecl>) {
    for s in &b.stmts {
        walk_stmt(s, out);
    }
}

fn walk_stmt(s: &crate::ast::Stmt, out: &mut Vec<LambdaDecl>) {
    use crate::ast::Stmt;
    match s {
        Stmt::Let { init, .. } => walk_expr(init, out),
        Stmt::Assign { target, value, .. } | Stmt::AssignOp { target, value, .. } => {
            walk_expr(target, out);
            walk_expr(value, out);
        }
        // ROUND 70: the step has no value expression.
        Stmt::Step { target, .. } => walk_expr(target, out),
        Stmt::If { cond, then, els, .. } => {
            walk_expr(cond, out);
            walk_block(then, out);
            if let Some(e) = els {
                walk_stmt(e, out);
            }
        }
        Stmt::While { cond, body, .. } => {
            walk_expr(cond, out);
            walk_block(body, out);
        }
        Stmt::For { start, end, body, .. } => {
            walk_expr(start, out);
            walk_expr(end, out);
            walk_block(body, out);
        }
        Stmt::Return { value, .. } => {
            if let Some(e) = value {
                walk_expr(e, out);
            }
        }
        Stmt::Defer(inner, _, _) => walk_stmt(inner, out),
        Stmt::Expr(e) => walk_expr(e, out),
        Stmt::Block(b) => walk_block(b, out),
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
    }
}

fn walk_expr(e: &Expr, out: &mut Vec<LambdaDecl>) {
    match &e.kind {
        ExprKind::FloatF32(_) => {}
        ExprKind::Lambda(d) => {
            walk_block(&d.body, out);
            out.push((**d).clone());
        }
        // ROUND 70: the text literal carries its array literal inside.
        ExprKind::Text(_, inner) => walk_expr(inner, out),
        ExprKind::Unary(_, i) => walk_expr(i, out),
        ExprKind::Binary(_, a, b) => {
            walk_expr(a, out);
            walk_expr(b, out);
        }
        ExprKind::Field(b, _, _) => walk_expr(b, out),
        ExprKind::Index(b, i) => {
            walk_expr(b, out);
            walk_expr(i, out);
        }
        ExprKind::Call(_, args, _) | ExprKind::Syscall(args) | ExprKind::ArrayLit(args) => {
            for a in args {
                walk_expr(a, out);
            }
        }
        ExprKind::Cast(i, _) => walk_expr(i, out),
        ExprKind::StructLit(_, fields, _) => {
            for (_, fe, _) in fields {
                walk_expr(fe, out);
            }
        }
        ExprKind::ArrayRepeat(a, b) => {
            walk_expr(a, out);
            walk_expr(b, out);
        }
        ExprKind::Int(_) | ExprKind::Float(..) | ExprKind::Bool(_) | ExprKind::Ident(_) => {}
    }
}

/// Every span of a closure literal in a body (`mono.rs` refuses those).
pub(crate) fn spans_in(b: &crate::ast::Block) -> Vec<Span> {
    let mut ls = Vec::new();
    walk_block(b, &mut ls);
    ls.iter().map(|d| d.span).collect()
}

/// Round 58: the captures of the generated function `__closure#N`.
pub(crate) fn captures_of_fn(name: &str) -> Option<Vec<Capture>> {
    let id: u32 = name.strip_prefix("__closure#")?.parse().ok()?;
    let info = closure_info(id)?;
    if info.captures.is_empty() {
        return None;
    }
    Some(info.captures)
}

// ------------------------------------------------------------------ Lowering

/// `// HOOK fnval` in `lower::write_into_inner` — `gc fn(…)`.
///
/// The same shape as `gc C{ … }` (gc_lower.rs): allocate, on failure write
/// `AllocError::OutOfMemory` into the union, on success write the record
/// into the union FIRST (so that the pointer immediately has a root on the
/// stack) and only then fill in the code address and the captured values.
pub(crate) fn lower_closure(
    lo: &mut crate::lower::Lower,
    dest: crate::fir::Val,
    d: &LambdaDecl,
    ty: &Type,
    span: Span,
) -> Option<()> {
    use crate::fir::{CmpOp, FTy, Op, Term};
    let info = match closure_info(d.id) {
        Some(i) => i,
        None => return lo.ice(span, "closure without registration"),
    };
    let union = match crate::errors::union_of(ty) {
        Some(u) => u,
        None => return lo.ice(span, "closure without error union"),
    };
    let code = match crate::errors::variant_code(crate::gc::ERR_SET, "OutOfMemory") {
        Some(c) => c,
        None => return lo.ice(span, "AllocError::OutOfMemory is missing"),
    };
    let tidv = lo.constant(FTy::U64, info.tid as i128);
    let sizev = lo.constant(FTy::U64, info.size as i128);
    let p = lo.push(
        FTy::Ptr,
        Op::Call { name: crate::gc::FN_ALLOC.to_string(), args: vec![tidv, sizev] },
    );
    let null = lo.constant(FTy::Ptr, 0);
    let ok = lo.push(FTy::Bool, Op::Cmp { op: CmpOp::Ne, ty: FTy::Ptr, a: p, b: null });
    let ok_bb = lo.new_block();
    let fail_bb = lo.new_block();
    let join = lo.new_block();
    lo.set_term(Term::BrCond { cond: ok, then_bb: ok_bb, else_bb: fail_bb });

    lo.cur = fail_bb;
    let c = lo.constant(FTy::U32, code);
    lo.store(FTy::U32, dest, c);
    let va = lo.field_addr_at(dest, union.val_off);
    let z = lo.constant(FTy::Ptr, 0);
    lo.store(FTy::Ptr, va, z);
    lo.set_term(Term::Br(join));

    lo.cur = ok_bb;
    let zero = lo.constant(FTy::U32, 0);
    lo.store(FTy::U32, dest, zero);
    let va = lo.field_addr_at(dest, union.val_off);
    lo.store(FTy::Ptr, va, p);
    // Word 0: the code address of the generated function.
    let key = record_of(&closure_fn(d.id));
    let rec = lo.push(FTy::Ptr, Op::FnRef { name: key });
    let cp = lo.load(FTy::Ptr, rec);
    lo.store(FTy::Ptr, p, cp);
    // The captured values, in the order of the class fields.
    for cap in &info.captures {
        let slot = match lo.lookup(&cap.name) {
            Some(s) => s,
            None => return lo.ice(span, "captured value without a slot"),
        };
        let ft = match crate::lower::scalar_fty_pub(&cap.ty) {
            Some(f) => f,
            None => return lo.ice(span, "captured value with a non-scalar type"),
        };
        let v = lo.load(ft, slot);
        let a = lo.field_addr_at(p, cap.off);
        lo.store(ft, a, v);
    }
    lo.set_term(Term::Br(join));
    lo.cur = join;
    Some(())
}
