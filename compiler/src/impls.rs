//! `impl T { fn method(*mut self, …) }` — methods on struct types
//! (round 45).
//!
//! ## What happens here — and what explicitly does not
//!
//! A method is **no new kind of thing** in Firn. `impl` is a writing aid
//! with exactly two effects:
//!
//!  1. `impl T { fn m(*mut self, x: i32) … }` creates the ordinary function
//!     `T__m(self: *mut T, x: i32)` — nothing more. Afterwards it stands in
//!     `Program::funcs` like every other one and goes through the same type
//!     check, the same lowering, the same code generator.
//!  2. `x.m(a)` becomes the call of that function. Which function is meant
//!     is decided **by the static type of the receiver alone** — no vtable,
//!     no dynamic dispatch, no lookup at run time. Once the type is settled,
//!     the jump instruction is settled.
//!
//! Up to the type check the call carries the name `"method m"`. The space
//! makes it a name that can arise from no identifier of the source text —
//! the same build as `"gc C"` in `gc.rs`. `sema.rs` resolves it, and
//! `lower.rs` derives the very same resolution once more. Both compute from
//! the same material (receiver type + method name); a side table between
//! the phases deliberately does not exist, because it would have to be
//! dragged along in `firnc1` without being able to do anything that the
//! type does not say already.
//!
//! ## The receiver — why `*self` and not `&self`
//!
//! Firn has **no references**. It has pointers (`*T`, `*mut T`) and the
//! address operator `&x`. A `&self` would be a new term that appears
//! nowhere in the rest of the language. That is why the receiver is
//! written the way a parameter is written in Firn:
//!
//! ```text
//! impl Bytes {
//!     fn length(*self) -> usize        // self: *Bytes
//!     fn push(*mut self, v: u8)        // self: *mut Bytes
//!     fn head(self) -> u8              // self: Bytes   (copy)
//! }
//! ```
//!
//! At the call site **one** adaptation happens, namely the one you would
//! otherwise write by hand: if the method demands a pointer and the receiver
//! is present as a value, the compiler takes its address (`&x`). If it is
//! already present as a pointer, it is passed through. There is no more
//! automatism: no automatic dereferencing, no chain of `*`, no detours
//! through fields. Whoever wants a copy writes `(*p).m()`.
//!
//! That `*self` and `*mut self` admit the **same** receiver in the type
//! check is no sloppiness of this file but the rule of the language:
//! `sema::compatible` compares pointers without the mutability (`*T` and
//! `*mut T` can be used for each other). `*mut self` therefore says exactly
//! what `*mut T` says as a parameter type today — intent, not constraint.
//! Once that rule changes for parameters, it changes for the receiver
//! along with it by itself.
//!
//! ## Name resolution (SPEC addendum §12.6)
//!
//! * Methods and free functions live in **one** namespace, but under
//!   different names: the method `m` of `T` is called `T__m`. `m(x)`
//!   therefore never finds a method, and `x.m()` never finds a free
//!   function. Shadowing is thereby ruled out — there is nothing to shadow.
//! * In the module `str`, `T__m` becomes `str__T__m` while merging
//!   (`modules.rs`), and the type `T` becomes `str__T`. The resolution
//!   `struct name ++ "__" ++ method` still holds afterwards — it always
//!   computes with the name that the type carries at that point.
//! * Generic types (`Vec[T]`) have no methods during this round; see
//!   `docs/ROUND45.md`, section "deliberately left out".

use crate::ast::{Expr, ExprKind, FnDecl, Param, Program, TypeExpr, UnOp};
use crate::diag::Span;
use crate::lexer::TokKind;
use crate::parser::Parser;
use std::collections::HashMap;

use crate::sema::{Checker, FnSig, TypeInfo};
use crate::types::{Type, TypeCtx};

/// Prefix of the method call not yet resolved in the AST.
/// The space makes it unreachable from the source text.
pub(crate) const P_CALL: &str = "method ";
/// Separator in the name of the method function: `Type__method`.
pub(crate) const SEP: &str = "__";

/// Is this a method call that is not resolved yet? Yields the
/// method name.
pub(crate) fn method_name(name: &str) -> Option<&str> {
    name.strip_prefix(P_CALL)
}

/// Name of the function behind `Type.method`.
pub(crate) fn fn_name(ty: &str, method: &str) -> String {
    format!("{}{}{}", ty, SEP, method)
}

// ------------------------------------------------------------------- Parser

/// `// HOOK impl` in `parser.rs::program` — `impl T { … }` and (round
/// 46) `impl Interface for T { … }`.
///
/// `impl` is NO keyword (the tokenizer does not know it) but an identifier
/// in a position where nothing else may stand — the same solution as
/// `gc class` (gc.rs). That keeps `impl` valid as a variable name. `for`
/// on the other hand is a keyword already (the loop) and therefore
/// unambiguous here.
pub(crate) fn hook_item(p: &mut Parser, prog: &mut Program) -> bool {
    if !matches!(p.kind(), TokKind::Ident(n) if n == "impl") {
        return false;
    }
    if !matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::Ident(_))) {
        return false;
    }
    match p.toks.get(p.pos + 2).map(|t| &t.kind) {
        Some(TokKind::LBrace) => {
            impl_decl(p, prog, false);
            true
        }
        Some(TokKind::KwFor) => {
            if !matches!(p.toks.get(p.pos + 3).map(|t| &t.kind), Some(TokKind::Ident(_))) {
                return false;
            }
            if !matches!(p.toks.get(p.pos + 4).map(|t| &t.kind), Some(TokKind::LBrace)) {
                return false;
            }
            impl_decl(p, prog, true);
            true
        }
        _ => false,
    }
}

fn impl_decl(p: &mut Parser, prog: &mut Program, is_for: bool) {
    let start = p.bump(); // 'impl'
    if !p.pending_attrs.is_empty() {
        let sp = p.pending_attrs[0].span;
        p.dg.error_note(
            sp,
            "no attribute is allowed before 'impl'".to_string(),
            "write the attribute before the individual method".to_string(),
        );
        p.pending_attrs.clear();
    }
    let (first, esp) = match p.ident("after 'impl'") {
        Some(x) => x,
        None => {
            p.recovering = false;
            p.sync_item();
            return;
        }
    };
    // HOOK iface: `impl Interface for T` (iface.rs, round 46). The block
    // creates THE SAME functions as `impl T` — the interface merely says
    // in addition what MUST stand in it.
    let (ty, tsp) = if is_for {
        p.bump(); // 'for'
        match p.ident("after 'for' in 'impl … for …'") {
            Some(x) => x,
            None => {
                p.recovering = false;
                p.sync_item();
                return;
            }
        }
    } else {
        (first.clone(), esp)
    };
    if is_for {
        crate::iface::remember_impl(first, ty.clone(), esp);
    }
    if !p.expect(TokKind::LBrace, "after the type name in 'impl'") {
        p.recovering = false;
        p.sync_item();
        return;
    }
    loop {
        while p.eat(&TokKind::Semi) {}
        if p.at(&TokKind::RBrace) || p.at_eof() {
            break;
        }
        if p.dg.is_full() {
            break;
        }
        let before = p.pos;
        if !p.at(&TokKind::KwFn) {
            p.error_here(format!(
                "expected 'fn' in an impl block, found '{}'",
                p.kind().text()
            ));
            p.recovering = false;
            break;
        }
        // One broken method aborts the WHOLE block. Otherwise the actual
        // message is followed by a cascade of consequential errors, and the
        // first one — the only one that explains anything — drowns in it.
        if !method(p, prog, &ty, tsp) {
            p.recovering = false;
            p.sync_item();
            return;
        }
        if p.pos == before {
            p.bump();
        }
    }
    p.close(TokKind::RBrace, "at the end of the impl block");
    p.recovering = false;
    let _ = start;
}

/// One method: `fn name(<receiver>[, param…]) [-> T] { … }`.
/// `false` = aborted, the surrounding `impl` block is discarded.
fn method(p: &mut Parser, prog: &mut Program, ty: &str, tsp: Span) -> bool {
    let start = p.bump(); // 'fn'
    let name = match p.ident("after 'fn' in an impl block") {
        Some((n, _)) => n,
        None => {
            p.recovering = false;
            p.sync_item();
            return false;
        }
    };
    if p.at(&TokKind::LBracket) {
        p.error_here("a method cannot be generic in this stage");
        p.recovering = false;
        p.sync_item();
        return false;
    }
    if !p.expect(TokKind::LParen, "after the method name") {
        p.recovering = false;
        p.sync_item();
        return false;
    }
    let slf = match self_param(p, ty, tsp) {
        Some(x) => x,
        None => {
            p.recovering = false;
            p.sync_item();
            return false;
        }
    };
    let mut params = vec![slf];
    if p.eat(&TokKind::Comma) {
        params.extend(p.params());
    }
    p.close(TokKind::RParen, "after the parameter list");
    p.recovering = false;
    let ret = if p.eat(&TokKind::Arrow) {
        match p.parse_type() {
            Some(t) => Some(t),
            None => {
                p.recovering = false;
                p.sync_item();
                return false;
            }
        }
    } else {
        None
    };
    if !p.at(&TokKind::LBrace) {
        p.error_here(format!(
            "expected '{{' at the start of the method body, found '{}'",
            p.kind().text()
        ));
        p.recovering = false;
        p.sync_item();
        return false;
    }
    let body = p.block("at the start of the method body");
    p.recovering = false;
    let attrs = std::mem::take(&mut p.pending_attrs);
    prog.funcs.push(FnDecl {
        name: fn_name(ty, &name),
        params,
        ret,
        body,
        span: start,
        attrs,
    });
    true
}

/// The receiver: `self`, `*self` or `*mut self`.
///
/// FOR A `gc class` (round 46) the receiver carries the INTERNAL struct
/// name `"gc K"`. That turns `*self` into exactly `Gc[K]` — a `gc class`
/// value exists on the heap only, a pointer to it is the only way to touch
/// it (SPEC §3.5.1). Whether `K` is a class is recorded in the registry of
/// `gc.rs`; it is filled while parsing, which is why `gc class K` has to
/// stand BEFORE the `impl` block (docs/ROUND46.md §9). The internal name
/// is not renamed by `modules.rs` — rightly so: class names hold
/// program wide.
fn self_param(p: &mut Parser, ty: &str, tsp: Span) -> Option<Param> {
    let class = crate::gc::is_class(ty);
    let tname = if class {
        format!("gc {}", ty)
    } else {
        ty.to_string()
    };
    if matches!(p.kind(), TokKind::Ident(n) if n == "self") {
        let sp = p.bump();
        if class {
            p.dg.error_note(
                sp,
                format!("'{}' is a gc class: the receiver cannot be a copy", ty),
                "a 'gc class' value lives only on the GC heap: write '*self' or '*mut self'"
                    .to_string(),
            );
            return None;
        }
        return Some(Param {
            name: "self".to_string(),
            ty: TypeExpr::Named(tname, tsp),
            span: sp,
        });
    }
    if p.at(&TokKind::Star) {
        let star = p.span();
        if let Some((mutable, sp)) = ptr_self(p) {
            return Some(Param {
                name: "self".to_string(),
                ty: TypeExpr::Ptr {
                    mutable,
                    inner: Box::new(TypeExpr::Named(tname, tsp)),
                    span: Parser::join(star, sp),
                },
                span: sp,
            });
        }
    }
    p.error_here(
        "the first parameter of a method is the receiver: 'self', '*self' or '*mut self'",
    );
    None
}

/// Consumes `*self` or `*mut self` and yields (mutable, position of
/// `self`). If no `self` follows, nothing is consumed.
pub(crate) fn ptr_self(p: &mut Parser) -> Option<(bool, Span)> {
    let with_mut = matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::KwMut));
    let idx = if with_mut { p.pos + 2 } else { p.pos + 1 };
    if !matches!(p.toks.get(idx).map(|t| &t.kind), Some(TokKind::Ident(n)) if n == "self") {
        return None;
    }
    p.bump(); // '*'
    if with_mut {
        p.bump(); // 'mut'
    }
    Some((with_mut, p.bump())) // 'self'
}

/// `// HOOK impl` in `parser.rs::postfix` — `x.m(args)`.
///
/// The field name has already been read; if a bracket follows now, this is
/// a method call and not a field access. A qualified module access
/// (`module.function(..)`) never arrives here: `Parser::qualify` already
/// turned that into ONE name in `primary`.
pub(crate) fn hook_method_call(
    p: &mut Parser,
    base: &Expr,
    name: &str,
    nsp: Span,
) -> Option<Expr> {
    // The bracket has to stand on the SAME line. Without that question
    //     let g: usize = (*p).field
    //     (*p).field = 0
    // would be a method call `(*p).field((*p))` — the line break ends the
    // statement (SPEC §10), and exactly that is what `cont` checks.
    if !p.cont() || !p.at(&TokKind::LParen) {
        return None;
    }
    p.bump(); // '('
    let (args, end) = p.call_args("after the argument list of a method call");
    let mut all = Vec::with_capacity(args.len() + 1);
    all.push(base.clone());
    all.extend(args);
    let span = Parser::join(base.span, end);
    Some(p.mk(span, ExprKind::Call(format!("{}{}", P_CALL, name), all, nsp)))
}

// --------------------------------------------------------------- Type check

/// Structure behind a receiver type: `(index, already present as a pointer)`.
/// Only for the question "is that a `dyn I`?" — otherwise `receiver_prefix`.
fn receiver_structure(tcx: &TypeCtx, t: &Type) -> Option<(usize, bool)> {
    match t {
        Type::Struct(i) if tcx.structs.get(*i).is_some() => Some((*i, false)),
        Type::Ptr { inner, .. } => match &**inner {
            Type::Struct(i) if tcx.structs.get(*i).is_some() => Some((*i, true)),
            _ => None,
        },
        _ => None,
    }
}

/// The name under which the methods of this receiver stand, and whether it
/// is already present as a pointer: `(prefix, is_pointer)`.
///
/// Since round 50 that includes a BASE TYPE as well (`impl Ord for i32`
/// creates `i32__less`). An untyped integer literal explicitly does not
/// belong to it: `1.m()` would have no settled type, and nobody could say
/// which `impl` block was meant.
fn receiver_prefix(tcx: &TypeCtx, t: &Type) -> Option<(String, bool)> {
    fn name(tcx: &TypeCtx, t: &Type) -> Option<String> {
        match t {
            Type::Struct(i) if tcx.structs.get(*i).is_some() => {
                Some(crate::iface::method_prefix(tcx, *i))
            }
            other => crate::iface::base_ty_name(other).map(|s| s.to_string()),
        }
    }
    match t {
        Type::Ptr { inner, .. } => name(tcx, inner).map(|n| (n, true)),
        other => name(tcx, other).map(|n| (n, false)),
    }
}

/// Interface behind a receiver type, if it is a `dyn I`.
/// (Round 46; `sema::probe` asks here too.)
pub(crate) fn dyn_interface(tcx: &TypeCtx, t: &Type) -> Option<String> {
    let (i, _) = receiver_structure(tcx, t)?;
    let name = &tcx.structs.get(i)?.name;
    crate::iface::interface_of(name).map(|s| s.to_string())
}

/// Resolution: target function and whether the receiver is passed as an
/// ADDRESS. ONE place, three users — `sema::probe` (type hint),
/// `sema::call` (check) and `lower::lower_call` (call) all compute with
/// this, so that they cannot drift apart.
pub(crate) fn target_of(
    tcx: &TypeCtx,
    fns: &HashMap<String, FnSig>,
    method: &str,
    recv: &Type,
) -> Option<(String, bool)> {
    let (prefix, is_ptr) = receiver_prefix(tcx, recv)?;
    let full = fn_name(&prefix, method);
    let sig = fns.get(&full)?;
    let will_ptr = sig.params.first().map(|t| t.is_ptr()).unwrap_or(false);
    Some((full, will_ptr && !is_ptr))
}

/// The same for the lowering, which holds the finished `TypeInfo`.
pub(crate) fn target(info: &TypeInfo, method: &str, recv: &Type) -> Option<(String, bool)> {
    target_of(&info.tcx, &info.fns, method, recv)
}

/// **ROUND 68** — a FIELD of the receiver that holds a FUNCTION VALUE.
///
/// `c.hook(a, b)` is not a method call: `hook` is a field of `Ctx` whose
/// type is `fn(A, B) -> R` (round 58). Up to round 67 it had to be loaded
/// into a local first — `let h = c.hook; h(a, b)` (docs/ROUND63.md, gap 6).
///
/// The lookup runs **after** the method lookup, never before: a method of
/// the same name keeps winning, so no existing program changes its meaning.
/// Yields the structure index, the parameter types and the result type.
pub(crate) fn field_fn(
    tcx: &TypeCtx,
    recv: &Type,
    name: &str,
) -> Option<(usize, Vec<Type>, Type)> {
    let sidx = match recv {
        Type::Struct(i) => *i,
        // a pointer receiver, and with it `Gc[T]` — a gc class is a struct
        Type::Ptr { inner, .. } => match **inner {
            Type::Struct(i) => i,
            _ => return None,
        },
        _ => return None,
    };
    let f = tcx.structs.get(sidx)?.field(name)?;
    match &f.ty {
        Type::Fn { params, ret } => Some((sidx, params.clone(), (**ret).clone())),
        _ => None,
    }
}

/// Can the address of this expression be taken?
/// The same set that `&x` allows and that `lower::lower_addr` masters.
fn is_slot(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Ident(_) | ExprKind::Field(..) | ExprKind::Index(..) => true,
        ExprKind::Unary(op, _) => matches!(op, UnOp::Deref),
        _ => false,
    }
}

/// All methods of a type, alphabetically — for the error message.
fn methods_of(ck: &Checker, prefix: &str) -> Vec<String> {
    let prefix = format!("{}{}", prefix, SEP);
    let mut out: Vec<String> = ck
        .fns
        .keys()
        .filter_map(|k| k.strip_prefix(&prefix).map(|s| s.to_string()))
        .filter(|s| !s.is_empty())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// `// HOOK impl` in `sema::Checker::call` — resolves `x.m(args)`.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Option<Type> {
    let method = method_name(name)?.to_string();
    // The parser always puts the receiver down as the first argument.
    let recv = match args.first() {
        Some(e) => e,
        None => return Some(Type::Error),
    };
    let et = ck.expr(recv, None);
    if et.is_error() {
        for a in &args[1..] {
            ck.type_out_expr(a);
        }
        return Some(Type::Error);
    }
    let (prefix, is_ptr) = match receiver_prefix(&ck.tcx, &et) {
        Some(x) => x,
        None => {
            for a in &args[1..] {
                ck.type_out_expr(a);
            }
            ck.dg.error_note(
                nspan,
                format!(
                    "method '{}' on a value of type {} — this type cannot have methods",
                    method,
                    ck.tcx.name_of(&et)
                ),
                "a method is declared with 'impl Type { fn … }' for a struct or primitive type"
                    .to_string(),
            );
            return Some(Type::Error);
        }
    };
    // HOOK iface: `f.m(args)` on a `dyn I` — DYNAMIC DISPATCH. Which function
    // runs is settled at run time in the method table; the check is made
    // against the interface (iface.rs, round 46).
    if let Some((sidx, _)) = receiver_structure(&ck.tcx, &et) {
        let sname = ck.tcx.structs[sidx].name.clone();
        if let Some(iname) = crate::iface::interface_of(&sname) {
            let iname = iname.to_string();
            return Some(crate::iface::hook_method(
                ck, &iname, &method, args, &et, is_ptr, nspan, espan,
            ));
        }
    }
    let sname = prefix.clone();
    let full = fn_name(&prefix, &method);
    let sig = match ck.fns.get(&full) {
        Some(s) => s.clone(),
        None => {
            // HOOK fnval (ROUND 68): no method of that name — but perhaps a
            // FIELD holding a function value. `c.hook(a, b)` then calls
            // through that value; the receiver is only the place the value
            // is read from and is NOT passed on.
            if let Some((_, params, ret)) = field_fn(&ck.tcx, &et, &method) {
                let display = format!("{}.{}", prefix, method);
                let shown = ck.tcx.name_of(&Type::Fn {
                    params: params.clone(),
                    ret: Box::new(ret.clone()),
                });
                let found = args.len().saturating_sub(1);
                if found != params.len() {
                    ck.dg.error(
                        espan,
                        format!(
                            "the function value '{}' of type {} expects {} argument(s), found {}",
                            display,
                            shown,
                            params.len(),
                            found
                        ),
                    );
                }
                for (i, a) in args[1..].iter().enumerate() {
                    match params.get(i) {
                        Some(p) => ck.check_argument(&display, i + 1, a, p),
                        None => ck.type_out_expr(a),
                    }
                }
                return Some(ret);
            }
            for a in &args[1..] {
                ck.type_out_expr(a);
            }
            let present = methods_of(ck, &prefix);
            let note = if present.is_empty() {
                format!("no 'impl' block is declared for '{}'", sname)
            } else {
                format!("'{}' has: {}", sname, present.join(", "))
            };
            ck.dg.error_note(
                nspan,
                format!("type '{}' has no method '{}'", sname, method),
                note,
            );
            return Some(Type::Error);
        }
    };
    let display = format!("{}.{}", prefix, method);
    // Adapt the receiver — the only automatism at the call site.
    match sig.params.first() {
        Some(t) if t.is_ptr() && !is_ptr => {
            if !is_slot(recv) {
                ck.dg.error_note(
                    recv.span,
                    format!(
                        "the receiver of '{}' needs an address, this expression has none",
                        display
                    ),
                    "bind it to a variable and call the method on it".to_string(),
                );
            }
        }
        Some(t) if !t.is_ptr() && is_ptr => {
            ck.dg.error_note(
                recv.span,
                format!(
                    "'{}' expects the receiver as a value, found {}",
                    display,
                    ck.tcx.name_of(&et)
                ),
                format!("write (*x).{}(…) if the copy is meant", method),
            );
        }
        _ => {}
    }
    // The remaining arguments — counted WITHOUT the receiver.
    let expected = sig.params.len().saturating_sub(1);
    let found = args.len().saturating_sub(1);
    if found != expected {
        ck.dg.error(
            espan,
            format!(
                "method '{}' expects {} argument(s), found {}",
                display, expected, found
            ),
        );
    }
    for (i, a) in args[1..].iter().enumerate() {
        match sig.params.get(i + 1) {
            Some(p) => ck.check_argument(&display, i + 1, a, p),
            None => ck.type_out_expr(a),
        }
    }
    Some(sig.ret)
}
