// SPDX-License-Identifier: MPL-2.0
//! **r715 -- lambda sugar** (`docs/CLOSURES.md`, "Lambda sugar").
//!
//! ```text
//! sort_stable_by(v, |a, b| (*a).x < (*b).x)        // expression body
//! apply(|x| { return x + 1 })                      // block body
//! let f: fn(i32) -> i32 = |x| x * 2                // expected type from the let
//! let g = |x: i32, y: i32| -> i32 x + y            // fully typed, no context needed
//! try gc |x| x + n                                 // capturing form (heap record)
//! ```
//!
//! The parser (`hook_primary`) produces an ordinary `ExprKind::Lambda` whose
//! untyped parameters carry the marker type `__infer` and whose result is the
//! marker `__lam_expr` (expression body, one `Stmt::Expr`) or `__lam_blk`
//! (block body). `apply` runs after the monomorphization and the default
//! arguments (every callee has its final name) and BEFORE the type checker:
//! it finds the expected `fn(..) -> R` type -- parameter of a plain function
//! or method call, `let x: T`, `return`, struct literal field -- replaces
//! the markers and turns an expression body into `return <expr>`. Afterwards
//! the lambda is exactly `fn(x: T) -> R { .. }` and nothing behind this pass
//! knows about the sugar.
//!
//! A lambda without a usable expected type is an error with a hint.

use crate::ast::*;
use crate::diag::{Diags, Span};
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::fx::{HashMap, HashSet};

const INFER: &str = "__infer";
const RET_EXPR: &str = "__lam_expr";
const RET_BLK: &str = "__lam_blk";

fn is_marker_ret(t: &Option<TypeExpr>) -> Option<&'static str> {
    match t {
        Some(TypeExpr::Named(n, _)) if n == RET_EXPR => Some(RET_EXPR),
        Some(TypeExpr::Named(n, _)) if n == RET_BLK => Some(RET_BLK),
        _ => None,
    }
}

fn has_marker(d: &LambdaDecl) -> bool {
    is_marker_ret(&d.ret).is_some()
        || d.params.iter().any(|p| matches!(&p.ty, TypeExpr::Named(n, _) if n == INFER))
}

// ------------------------------------------------------------------ parser

fn starts_lambda(k: &TokKind) -> bool {
    matches!(k, TokKind::Pipe | TokKind::OrOr)
}

/// `// HOOK lambda` in `parser::primary`: `|a, b: T| [-> R] expr|{ block }`,
/// also `gc |..| ..`. A `|` at the START of an operand cannot be anything
/// else (bit-or is infix, `catch |e|` is parsed behind the operand).
pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr> {
    let heap = match p.kind() {
        k if starts_lambda(k) => false,
        TokKind::Ident(n) if n == "gc" => {
            if !matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(k) if starts_lambda(k)) {
                return None;
            }
            true
        }
        _ => return None,
    };
    let start = p.span();
    if heap {
        p.bump();
    }
    let mut params: Vec<Param> = Vec::new();
    if matches!(p.kind(), TokKind::OrOr) {
        p.bump(); // `||`: no parameters
    } else {
        p.bump(); // `|`
        while !matches!(p.kind(), TokKind::Pipe) {
            let (name, nsp) = match p.ident("as a lambda parameter") {
                Some(x) => x,
                None => return Some(p.broken_expr(start)),
            };
            let ty = if p.eat(&TokKind::Colon) {
                match p.parse_type() {
                    Some(t) => t,
                    None => return Some(p.broken_expr(start)),
                }
            } else {
                TypeExpr::Named(INFER.to_string(), nsp)
            };
            params.push(Param { name, ty, span: nsp, refk: 0, default: None });
            if !p.eat(&TokKind::Comma) {
                break;
            }
        }
        if !p.expect(TokKind::Pipe, "after the parameters of a lambda") {
            return Some(p.broken_expr(start));
        }
    }
    let mut ret: Option<TypeExpr> = None;
    if p.eat(&TokKind::Arrow) {
        match p.parse_type() {
            Some(t) => ret = Some(t),
            None => return Some(p.broken_expr(start)),
        }
    }
    let saved = p.no_struct_lit;
    let (body, ret) = if p.at(&TokKind::LBrace) {
        p.no_struct_lit = false;
        let saved_ret = std::mem::replace(&mut p.cur_ret, ret.clone());
        let b = p.block("of a lambda");
        p.cur_ret = saved_ret;
        p.no_struct_lit = saved;
        let r = ret.or(Some(TypeExpr::Named(RET_BLK.to_string(), start)));
        (b, r)
    } else {
        p.no_q += 1;
        let e = p.expr();
        p.no_q -= 1;
        let sp = e.span;
        let stmt = match ret {
            Some(_) => Stmt::Return { value: Some(e), span: sp },
            None => Stmt::Expr(e),
        };
        let b = Block { stmts: vec![stmt], span: sp, end: sp };
        let r = ret.or(Some(TypeExpr::Named(RET_EXPR.to_string(), start)));
        (b, r)
    };
    let span = Parser::join(start, body.span);
    let d = LambdaDecl { id: crate::fnval::next_lambda_id(), heap, params, ret, body, span };
    Some(p.mk(span, ExprKind::Lambda(Box::new(d))))
}

// -------------------------------------------------------------------- pass

struct Cx<'a> {
    sigs: &'a HashMap<String, Vec<TypeExpr>>,
    msigs: &'a HashMap<String, Vec<Vec<TypeExpr>>>,
    fields: &'a HashMap<String, HashMap<String, TypeExpr>>,
    /// result type per function name (for `let x = f()`)
    rets: &'a HashMap<String, TypeExpr>,
    /// element type per instantiated `Vec__..` (r740: `v[i] = |x| ..`)
    elems: &'a HashMap<String, TypeExpr>,
    /// type per static / const (r742: `s.f = |x| ..` on a module static)
    globals: &'a HashMap<String, TypeExpr>,
    /// functions whose first parameter is `self` (r742: `Type.m` as a value)
    methods: &'a HashSet<String>,
    /// block scopes of the current function, innermost last (r742). `None` = a
    /// local whose type is not evident; it still SHADOWS an outer one.
    scopes: Vec<HashMap<String, Option<TypeExpr>>>,
    ret: Option<TypeExpr>,
    /// r742b: result types of instances inferred in this round (`apply_g(id, 4)`)
    inst_rets: HashMap<String, TypeExpr>,
    /// r742: an instance was recorded while inferring `apply(id, 4)` -> expand again
    pending: bool,
    /// r750: next free ExprId (bound method values build new expressions)
    next: u32,
    /// r751: untyped comprehensions of the current function: hidden variable -> (collection
    /// type, instance name of the `*_new` function) once the element type is inferred
    coll_res: HashMap<String, (TypeExpr, String)>,
    /// r751: comprehensions whose element type could not be inferred (error already reported)
    coll_failed: HashSet<String>,
    /// r751: element type per `Vec` instance created by an inferred comprehension
    new_elems: HashMap<String, TypeExpr>,
    dg: &'a mut Diags,
}

/// Looks `n` up from the innermost block outwards. `Some(None)` = a local of
/// unknown type, `None` = no local of that name.
fn lookup(cx: &Cx, n: &str) -> Option<Option<TypeExpr>> {
    cx.scopes.iter().rev().find_map(|s| s.get(n).cloned())
}

fn declare(cx: &mut Cx, n: &str, t: Option<TypeExpr>) {
    if let Some(s) = cx.scopes.last_mut() {
        s.insert(n.to_string(), t);
    }
}

fn strip(t: TypeExpr) -> TypeExpr {
    match t {
        TypeExpr::Ptr { inner, .. } => strip(*inner),
        t => t,
    }
}

fn mkx(cx: &mut Cx, span: Span, kind: ExprKind) -> Expr {
    let id = cx.next;
    cx.next += 1;
    Expr { id, span, kind }
}

/// r750: `x.m` (a method of `x`'s struct, no call) -> the closure
/// `gc |a0, ..| x.m(a0, ..)`; the record captures the receiver pointer `x`.
/// `None` = `e` is no bound method; `Some(None)` = error reported.
fn bound_method(cx: &mut Cx, e: &Expr) -> Option<Option<Expr>> {
    let (inner, m) = match &e.kind {
        ExprKind::Field(i, m, _) => (i, m.clone()),
        _ => return None,
    };
    let recv_t = expr_ty(cx, inner)?;
    let is_ptr = matches!(recv_t, TypeExpr::Ptr { .. });
    let s = match strip(recv_t) {
        TypeExpr::Named(s, _) => s,
        _ => return None,
    };
    if cx.fields.get(&s).map_or(false, |fs| fs.contains_key(&m)) {
        return None;
    }
    let f = format!("{}__{}", s, m);
    if !cx.methods.contains(&f) {
        return None;
    }
    let name = match &inner.kind {
        ExprKind::Ident(n) if !n.contains('.') => n.clone(),
        _ => {
            cx.dg.error_note(
                e.span,
                format!("method '{}' of '{}' used as a value: the receiver must be a plain variable", m, s),
                "bind the receiver first: `let r = <expr>` and write `try r.m`",
            );
            return Some(None);
        }
    };
    if !is_ptr {
        cx.dg.error_note(
            e.span,
            format!("method '{}' of '{}' used as a value: the receiver '{}' is not a pointer", m, s, name),
            "the closure captures one word; pass a pointer (`p: *mut T`) or a Gc[T], or write `Type.m` (receiver as first argument)",
        );
        return Some(None);
    }
    let ptys = cx.sigs.get(&f)?.clone();
    let ret = cx.rets.get(&f).cloned();
    let sp = e.span;
    let mut params: Vec<Param> = Vec::new();
    let mut args: Vec<Expr> = vec![(**inner).clone()];
    for (i, t) in ptys.iter().skip(1).enumerate() {
        let pn = format!("__bm_a{}", i);
        params.push(Param { name: pn.clone(), ty: t.clone(), span: sp, refk: 0, default: None });
        args.push(mkx(cx, sp, ExprKind::Ident(pn)));
    }
    let call = mkx(cx, sp, ExprKind::Call(format!("{}{}", crate::impls::P_CALL, m), args, sp));
    let stmt = match ret {
        Some(_) => Stmt::Return { value: Some(call), span: sp },
        None => Stmt::Expr(call),
    };
    let body = Block { stmts: vec![stmt], span: sp, end: sp };
    let d = LambdaDecl { id: crate::fnval::next_lambda_id(), heap: true, params, ret, body, span: sp };
    Some(Some(mkx(cx, sp, ExprKind::Lambda(Box::new(d)))))
}

/// The declared type of an assignment target / local, as far as it is
/// evident without a type check (r740). `None` = unknown.
fn expr_ty(cx: &Cx, e: &Expr) -> Option<TypeExpr> {
    match &e.kind {
        ExprKind::Ident(n) => match lookup(cx, n) {
            Some(t) => t,
            None => cx.globals.get(n).cloned(),
        },
        ExprKind::StructLit(n, _, _) => Some(TypeExpr::Named(n.clone(), e.span)),
        ExprKind::Cast(_, t) => Some(t.clone()),
        ExprKind::Unary(UnOp::AddrOf, a) => {
            let inner = expr_ty(cx, a)?;
            Some(TypeExpr::Ptr { mutable: true, inner: Box::new(inner), span: e.span })
        }
        // r742: `let x = obj.method(); x.f = |a| ..` -- the receiver type picks `Type__method`
        ExprKind::Call(n, args, _) if crate::impls::method_name(n).is_some() => {
            let m = crate::impls::method_name(n)?;
            let recv = strip(expr_ty(cx, args.first()?)?);
            match recv {
                TypeExpr::Named(s, _) => cx.rets.get(&format!("{}__{}", s, m)).cloned(),
                _ => None,
            }
        }
        // `try f()`: the payload of the error union
        ExprKind::Call(n, args, _) if n == crate::errors::TRY_NAME && args.len() == 1 => {
            match expr_ty(cx, &args[0])? {
                TypeExpr::Named(u, _) => crate::errors::pending_inner(&u).map(|x| x.1),
                _ => None,
            }
        }
        ExprKind::Call(n, _, _) => cx.rets.get(n).or_else(|| cx.inst_rets.get(n)).cloned(),
        ExprKind::Unary(UnOp::Deref, a) => match expr_ty(cx, a)? {
            TypeExpr::Ptr { inner, .. } => Some(*inner),
            _ => None,
        },
        ExprKind::Field(a, f, _) => match strip(expr_ty(cx, a)?) {
            TypeExpr::Named(s, _) => cx.fields.get(&s)?.get(f).cloned(),
            _ => None,
        },
        ExprKind::Index(a, _) => match strip(expr_ty(cx, a)?) {
            TypeExpr::Array { elem, .. } => Some(*elem),
            TypeExpr::Named(m, _) => cx.elems.get(&m).or_else(|| cx.new_elems.get(&m)).cloned(),
            _ => None,
        },
        _ => None,
    }
}

/// r751: the type of an element expression of a comprehension, as far as it is evident
/// without the type checker. (strong, weak): `weak` = a literal default (`1` -> i32);
/// a strong type of one operand wins over the weak default of the other.
fn val_ty(cx: &Cx, e: &Expr) -> (Option<TypeExpr>, Option<TypeExpr>) {
    let named = |n: &str| Some(TypeExpr::Named(n.to_string(), e.span));
    match &e.kind {
        ExprKind::Int(_) => (None, named("i32")),
        ExprKind::Float(..) => (None, named("f64")),
        ExprKind::FloatF32(_) => (named("f32"), None),
        ExprKind::Bool(_) => (named("bool"), None),
        ExprKind::Text(false, _) => (named(crate::strtype::NAME), None),
        ExprKind::Cast(_, t) => (Some(t.clone()), None),
        ExprKind::StructLit(n, _, _) if n == crate::ast::TUPLE_LIT => (None, None),
        ExprKind::Unary(UnOp::Neg, a) | ExprKind::Unary(UnOp::BitNot, a) => val_ty(cx, a),
        ExprKind::Unary(UnOp::Not, _) => (named("bool"), None),
        ExprKind::Binary(op, a, b) => match op {
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::LAnd | BinOp::LOr => {
                (named("bool"), None)
            }
            BinOp::Shl | BinOp::Shr => val_ty(cx, a),
            _ => {
                let (s1, w1) = val_ty(cx, a);
                let (s2, w2) = val_ty(cx, b);
                (s1.or(s2), w1.or(w2))
            }
        },
        ExprKind::IfElse(_, a, b) => {
            let (s1, w1) = val_ty(cx, a);
            let (s2, w2) = val_ty(cx, b);
            (s1.or(s2), w1.or(w2))
        }
        _ => (expr_ty(cx, e), None),
    }
}

/// r751: the element type of the comprehension that pushes `args` through `__coll_add_<kind>`.
fn coll_elem_ty(cx: &Cx, kind: &str, args: &[Expr]) -> Option<TypeExpr> {
    let e = if kind == "dict" { args.get(2)? } else { args.get(1)? };
    let (s, w) = val_ty(cx, e);
    s.or(w)
}

fn is_coll_marker(t: &Option<TypeExpr>) -> bool {
    matches!(t, Some(TypeExpr::Named(n, _)) if n == crate::collsugar::INFER_COLL)
}

fn error_coll(cx: &mut Cx, sp: Span) {
    cx.dg.error_note(
        sp,
        "a comprehension needs a declared collection type",
        "the element type is not evident here: write `let xs: Vec[T] = [e for x in v]` (or `return [..]` in a function returning `Vec[T]`)",
    );
}

/// r751: `__coll_add_<kind>(&x, ..)` -- the element type is known now: name the instances
/// `vec_push[T]` / `Vec[T]` ..., record them (expanded by `mono::expand_late`) and tell the
/// block that owns `let x`.
fn coll_push(cx: &mut Cx, name: &mut String, args: &[Expr], sp: Span) {
    let kind = name[crate::collsugar::COLL_ADD.len()..].to_string();
    let target = match args.first().map(|a| &a.kind) {
        Some(ExprKind::Unary(UnOp::AddrOf, a)) => match &a.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return,
        },
        _ => return,
    };
    let t = match coll_elem_ty(cx, &kind, args) {
        Some(t) => t,
        None => {
            if cx.coll_failed.insert(target) {
                error_coll(cx, sp);
            }
            return;
        }
    };
    let (base, new_fn, add_fn) = match kind.as_str() {
        "dict" => ("Dict", "dict_new", "dict_set"),
        "set" => ("Set", "set_new", "set_put"),
        _ => ("Vec", "vec_new", "vec_push"),
    };
    let targs = vec![t.clone()];
    let sty = crate::sema_generic::mangle(base, &targs);
    let nfn = crate::sema_generic::mangle(new_fn, &targs);
    let afn = crate::sema_generic::mangle(add_fn, &targs);
    let rec = |m: &str, b: &str, is_fn: bool| {
        crate::sema_generic::record_inst(
            m,
            crate::sema_generic::Instantiation {
                base: b.to_string(),
                args: targs.clone(),
                span: sp,
                is_abstract: false,
                is_fn,
            },
        )
    };
    rec(&sty, base, false);
    rec(&nfn, new_fn, true);
    rec(&afn, add_fn, true);
    *name = afn;
    let cty = TypeExpr::Named(sty.clone(), sp);
    if base == "Vec" {
        cx.new_elems.insert(sty, t);
    }
    if let Some(sc) = cx.scopes.iter_mut().rev().find(|s| s.contains_key(&target)) {
        sc.insert(target.clone(), Some(cty.clone()));
    }
    cx.coll_res.insert(target, (cty, nfn));
    cx.pending = true;
}

/// r742b: `e` is the NAME of a generic fn used as a value (`id`, `mod.id`) -> its template name.
fn generic_fn_value(cx: &Cx, e: &Expr) -> Option<String> {
    let n = match &e.kind {
        ExprKind::Ident(n) => n,
        _ => return None,
    };
    if lookup(cx, n).is_some() || cx.globals.contains_key(n) {
        return None;
    }
    let base = match n.split_once('.') {
        Some((_, f)) if crate::sema_generic::is_generic_fn(f) => f.to_string(),
        _ => n.clone(),
    };
    if crate::sema_generic::is_generic_fn(&base) {
        Some(base)
    } else {
        None
    }
}

/// r742b: the type of a call argument, for inferring the type parameters of a
/// generic callee. (strong, weak): `weak` = a literal default.
fn arg_ty(cx: &Cx, e: &Expr) -> (Option<TypeExpr>, Option<TypeExpr>) {
    let named = |n: &str| Some(TypeExpr::Named(n.to_string(), e.span));
    match &e.kind {
        ExprKind::Int(_) => (None, named("i32")),
        ExprKind::Float(..) => (None, named("f64")),
        ExprKind::FloatF32(_) => (named("f32"), None),
        ExprKind::Bool(_) => (named("bool"), None),
        ExprKind::Unary(UnOp::Neg, a) => arg_ty(cx, a),
        ExprKind::Ident(n) if lookup(cx, n).is_none() && !cx.globals.contains_key(n) => {
            // a plain function used as a value: its signature is the type
            match (cx.sigs.get(n), cx.rets.get(n)) {
                (Some(ps), r) => (
                    Some(TypeExpr::Fn { params: ps.clone(), ret: r.map(|r| Box::new(r.clone())), span: e.span }),
                    None,
                ),
                _ => (None, None),
            }
        }
        _ => (expr_ty(cx, e), None),
    }
}

fn ty_key(t: &TypeExpr) -> String {
    match t {
        TypeExpr::Named(n, _) => n.clone(),
        TypeExpr::Ptr { mutable, inner, .. } => format!("*{}{}", if *mutable { "mut " } else { "" }, ty_key(inner)),
        TypeExpr::Array { elem, len, .. } => format!("[{}; {}]", ty_key(elem), len),
        TypeExpr::Fn { params, ret, .. } => format!(
            "fn({}){}",
            params.iter().map(ty_key).collect::<Vec<_>>().join(","),
            ret.as_ref().map(|r| format!("->{}", ty_key(r))).unwrap_or_default()
        ),
        TypeExpr::Tuple(v, _) => format!("({})", v.iter().map(ty_key).collect::<Vec<_>>().join(",")),
    }
}

pub fn apply(prog: &mut Program, dg: &mut Diags) {
    let mut start = 0usize;
    loop {
        let before = prog.funcs.len();
        let pending = round(prog, dg, start);
        if !pending || dg.has_errors() {
            break;
        }
        // r742: instances of generic functions inferred from an expected `fn`
        // type (`apply(id, 4)`) are expanded now; the new bodies get their turn.
        crate::mono::expand_late(prog, dg);
        if prog.funcs.len() == before {
            break;
        }
        start = before;
    }
}

fn round(prog: &mut Program, dg: &mut Diags, start: usize) -> bool {
    let mut sigs: HashMap<String, Vec<TypeExpr>> = HashMap::default();
    let mut msigs: HashMap<String, Vec<Vec<TypeExpr>>> = HashMap::default();
    let mut methods: HashSet<String> = HashSet::default();
    for f in &prog.funcs {
        let tys: Vec<TypeExpr> = f.params.iter().map(|p| p.ty.clone()).collect();
        if f.params.first().map_or(false, |p| p.name == "self") {
            methods.insert(f.name.clone());
            if let Some(i) = f.name.rfind("__") {
                msigs.entry(f.name[i + 2..].to_string()).or_default().push(tys.clone());
            }
        }
        sigs.insert(f.name.clone(), tys);
    }
    let mut fields: HashMap<String, HashMap<String, TypeExpr>> = HashMap::default();
    for s in &prog.structs {
        fields.insert(s.name.clone(), s.fields.iter().map(|(n, t, _)| (n.clone(), t.clone())).collect());
    }
    let rets: HashMap<String, TypeExpr> =
        prog.funcs.iter().filter_map(|f| f.ret.clone().map(|r| (f.name.clone(), r))).collect();
    let mut elems: HashMap<String, TypeExpr> = HashMap::default();
    for (name, inst) in crate::sema_generic::instantiations() {
        if !inst.is_fn && !inst.is_abstract && inst.base == "Vec" && inst.args.len() == 1 {
            elems.insert(name, inst.args[0].clone());
        }
    }
    let mut globals: HashMap<String, TypeExpr> = HashMap::default();
    for c in &prog.consts {
        globals.insert(c.name.clone(), c.ty.clone());
    }
    for s in &prog.statics {
        globals.insert(s.name.clone(), s.ty.clone());
    }
    let mut cx = Cx {
        sigs: &sigs,
        msigs: &msigs,
        fields: &fields,
        rets: &rets,
        elems: &elems,
        globals: &globals,
        methods: &methods,
        scopes: Vec::new(),
        ret: None,
        inst_rets: HashMap::default(),
        pending: false,
        next: prog.expr_count,
        coll_res: HashMap::default(),
        coll_failed: HashSet::default(),
        new_elems: HashMap::default(),
        dg,
    };
    for f in prog.funcs.iter_mut().skip(start) {
        cx.ret = f.ret.clone();
        cx.coll_res.clear();
        cx.coll_failed.clear();
        cx.scopes = vec![f.params.iter().map(|p| (p.name.clone(), Some(p.ty.clone()))).collect()];
        block(&mut cx, &mut f.body);
    }
    cx.ret = None;
    cx.scopes = vec![HashMap::default()];
    if start == 0 {
        for c in prog.consts.iter_mut() {
            let t = c.ty.clone();
            want(&mut cx, &mut c.value, Some(&t));
        }
        for s in prog.statics.iter_mut() {
            let t = s.ty.clone();
            want(&mut cx, &mut s.value, Some(&t));
        }
    }
    let pending = cx.pending;
    prog.expr_count = cx.next;
    pending
}

fn block(cx: &mut Cx, b: &mut Block) {
    // r742: block scoping -- a `let` inside `{ .. }` ends with the block and a
    // shadowing `let` of another type does not leak into the outer one
    cx.scopes.push(HashMap::default());
    for s in b.stmts.iter_mut() {
        stmt(cx, s);
    }
    // r751: the hidden variables of untyped comprehensions get their inferred type
    for s in b.stmts.iter_mut() {
        if let Stmt::Let { name, ty, init, span, .. } = s {
            if !is_coll_marker(ty) {
                continue;
            }
            match cx.coll_res.get(name.as_str()).cloned() {
                Some((cty, nfn)) => {
                    *ty = Some(cty);
                    if let ExprKind::Call(n, _, _) = &mut init.kind {
                        *n = nfn;
                    }
                }
                None => {
                    if !cx.coll_failed.contains(name.as_str()) {
                        let sp = *span;
                        error_coll(cx, sp);
                    }
                }
            }
        }
    }
    cx.scopes.pop();
}

fn stmt(cx: &mut Cx, s: &mut Stmt) {
    match s {
        Stmt::Let { name, ty, init, .. } if is_coll_marker(ty) => {
            // r751: the type arrives with the push inside the loops that follow
            let _ = init;
            declare(cx, name, None);
        }
        Stmt::Let { name, ty, init, .. } => {
            let t = ty.clone();
            want(cx, init, t.as_ref());
            let known = t.or_else(|| expr_ty(cx, init));
            declare(cx, name, known);
        }
        Stmt::AssignOp { target, value, .. } => {
            want(cx, target, None);
            want(cx, value, None);
        }
        Stmt::Assign { target, value, .. } => {
            // r740: `s.f = |x| ..` / `v[i] = gc |x| ..` / `f = |x| ..` -- the
            // expected type is the declared type of the target
            want(cx, target, None);
            let t = expr_ty(cx, target);
            want(cx, value, t.as_ref());
        }
        Stmt::Step { target, .. } => want(cx, target, None),
        Stmt::If { cond, then, els, .. } => {
            want(cx, cond, None);
            block(cx, then);
            if let Some(e) = els {
                stmt(cx, e);
            }
        }
        Stmt::While { cond, body, .. } => {
            want(cx, cond, None);
            block(cx, body);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                let t = cx.ret.clone();
                want(cx, v, t.as_ref());
            }
        }
        Stmt::For { name, start, end, body, .. } => {
            want(cx, start, None);
            want(cx, end, None);
            cx.scopes.push(HashMap::default());
            // r751: the loop variable has the type of its range (a literal range is i32)
            let (s1, w1) = val_ty(cx, start);
            let (s2, w2) = val_ty(cx, end);
            let vt = s1.or(s2).or(w1).or(w2);
            declare(cx, name, vt);
            block(cx, body);
            cx.scopes.pop();
        }
        Stmt::Defer(inner, _, _) => stmt(cx, inner),
        Stmt::Expr(e) => want(cx, e, None),
        Stmt::Block(b) => block(cx, b),
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
    }
}

/// Visit `e`, knowing the type the context wants (if any).
fn want(cx: &mut Cx, e: &mut Expr, t: Option<&TypeExpr>) {
    if value_rewrite(cx, e, t) {
        return;
    }
    match &mut e.kind {
        ExprKind::Call(n, args, _) if n == crate::errors::TRY_NAME && args.len() == 1 => {
            // `try gc |x| ..`: the lambda is the operand
            // r750: `try x.m` -- bound method value
            match bound_method(cx, &args[0]) {
                Some(Some(lam)) => args[0] = lam,
                Some(None) => return,
                None => {}
            }
            want(cx, &mut args[0], t);
        }
        ExprKind::Lambda(d) => resolve(cx, d, t, e.span),
        _ => walk(cx, e),
    }
}

/// r742: a function NAME used as a value.
/// * `id` (generic fn, no type arguments) where a `fn(..)` type is expected:
///   the type arguments come from that type (`apply(id, 4)`, `let f: fn(i32) -> i32 = id`,
///   struct field, `return`) -> the identifier becomes the instance name.
/// * `Type.m` (method of a struct, no call) -> the plain function `Type__m`, whose
///   first parameter is the receiver pointer.
/// * r750: a BOUND receiver `try x.m` (x a pointer variable) -> `try gc |a..| x.m(a..)`.
/// Returns true when `e` was rewritten or reported.
fn value_rewrite(cx: &mut Cx, e: &mut Expr, t: Option<&TypeExpr>) -> bool {
    match &e.kind {
        ExprKind::Ident(n) => {
            let n = n.clone();
            if lookup(cx, &n).is_some() || cx.globals.contains_key(&n) {
                return false;
            }
            // `mod.f` is read as one identifier when `mod` is an imported module
            let base = match n.split_once('.') {
                Some((_, f)) if crate::sema_generic::is_generic_fn(f) => f.to_string(),
                _ => n.clone(),
            };
            if !crate::sema_generic::is_generic_fn(&base) {
                return false;
            }
            let want_t = match t {
                Some(t @ TypeExpr::Fn { .. }) => t.clone(),
                _ => return false,
            };
            match crate::sema_generic::infer_fn_value(&base, &want_t, e.span) {
                Ok(m) => {
                    e.kind = ExprKind::Ident(m);
                    cx.pending = true;
                }
                Err(msg) => cx.dg.error(e.span, msg),
            }
            true
        }
        ExprKind::Field(inner, m, _) => {
            let m = m.clone();
            if let ExprKind::Ident(s) = &inner.kind {
                // `Type.m`: `Type` is a struct, no variable
                if lookup(cx, s).is_none() && !cx.globals.contains_key(s) {
                    let f = format!("{}__{}", s, m);
                    if cx.fields.contains_key(s) && cx.methods.contains(&f) {
                        e.kind = ExprKind::Ident(f);
                        return true;
                    }
                }
            }
            // `x.m` without a call: a method, no field (r750: bound method value)
            let payload = match t {
                Some(TypeExpr::Named(n, _)) => crate::errors::pending_inner(n).map(|x| x.1),
                _ => None,
            };
            match bound_method(cx, e) {
                Some(Some(lam)) => {
                    if payload.is_some() {
                        *e = lam;
                        walk(cx, e);
                    } else {
                        cx.dg.error_note(
                            e.span,
                            format!("method '{}' used as a value without a call needs `try`", m),
                            "a bound method value allocates a closure record (type `AllocError!fn(..)`): write `try x.m`",
                        );
                    }
                    return true;
                }
                Some(None) => return true,
                None => {}
            }
            false
        }
        _ => false,
    }
}

fn error_infer(cx: &mut Cx, sp: Span) {
    cx.dg.error_note(
        sp,
        "cannot infer the types of this lambda here",
        "use it where a `fn(..) -> R` type is known (argument of a function, `let f: fn(..) = ..`, `return`, struct field) or write the types: `|x: i32| -> i32 x + 1`",
    );
}

fn resolve(cx: &mut Cx, d: &mut LambdaDecl, t: Option<&TypeExpr>, sp: Span) {
    // `E!fn(..)` (the result of `gc |..| ..`): the payload is the function type
    let payload: Option<TypeExpr> = match t {
        Some(TypeExpr::Named(n, _)) => crate::errors::pending_inner(n).map(|x| x.1),
        _ => None,
    };
    let t: Option<&TypeExpr> = payload.as_ref().or(t);
    if has_marker(d) {
        match t {
            Some(TypeExpr::Fn { params, ret, .. }) => {
                if params.len() != d.params.len() {
                    cx.dg.error_note(
                        sp,
                        format!("this lambda takes {} parameter(s) but the expected function type has {}", d.params.len(), params.len()),
                        "the number of `|..|` names must match the `fn(..)` type",
                    );
                } else {
                    for (p, w) in d.params.iter_mut().zip(params.iter()) {
                        if matches!(&p.ty, TypeExpr::Named(n, _) if n == INFER) {
                            p.ty = w.clone();
                        }
                    }
                    match is_marker_ret(&d.ret) {
                        Some(m) => {
                            d.ret = ret.as_ref().map(|r| (**r).clone());
                            if m == RET_EXPR && d.ret.is_some() {
                                // expression body -> `return <expr>`
                                if let Some(Stmt::Expr(_)) = d.body.stmts.first() {
                                    if let Some(Stmt::Expr(ex)) = d.body.stmts.pop() {
                                        let esp = ex.span;
                                        d.body.stmts.push(Stmt::Return { value: Some(ex), span: esp });
                                    }
                                }
                            }
                        }
                        None => {}
                    }
                }
            }
            _ => error_infer(cx, sp),
        }
    }
    // the body: its own `return` is checked against the lambda result
    let saved = cx.ret.take();
    cx.ret = d.ret.clone();
    let ps: HashMap<String, Option<TypeExpr>> = d
        .params
        .iter()
        .map(|p| {
            let known = !matches!(&p.ty, TypeExpr::Named(n, _) if n == INFER);
            (p.name.clone(), if known { Some(p.ty.clone()) } else { None })
        })
        .collect();
    cx.scopes.push(ps);
    block(cx, &mut d.body);
    cx.scopes.pop();
    cx.ret = saved;
}

fn walk(cx: &mut Cx, e: &mut Expr) {
    match &mut e.kind {
        ExprKind::Float(..) | ExprKind::FloatF32(_) | ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Ident(_) => {}
        ExprKind::Unary(_, a) | ExprKind::Cast(a, _) | ExprKind::Field(a, _, _) => want(cx, a, None),
        ExprKind::Text(_, a) => want(cx, a, None),
        ExprKind::Named(_, a, _) => want(cx, a, None),
        ExprKind::Binary(_, a, b) | ExprKind::Index(a, b) | ExprKind::ArrayRepeat(a, b) => {
            want(cx, a, None);
            want(cx, b, None);
        }
        ExprKind::IfElse(c, a, b) => {
            want(cx, c, None);
            want(cx, a, None);
            want(cx, b, None);
        }
        ExprKind::Syscall(xs) | ExprKind::ArrayLit(xs) => {
            for x in xs.iter_mut() {
                want(cx, x, None);
            }
        }
        ExprKind::StructLit(name, fs, _) => {
            let ftys = cx.fields.get(name.as_str());
            let tys: Vec<Option<TypeExpr>> =
                fs.iter().map(|(fname, _, _)| ftys.and_then(|m| m.get(fname)).cloned()).collect();
            for ((_, fe, _), t) in fs.iter_mut().zip(tys.iter()) {
                want(cx, fe, t.as_ref());
            }
        }
        ExprKind::Lambda(d) => {
            let sp = e.span;
            resolve(cx, d, None, sp);
        }
        ExprKind::Call(name, args, _) if name.starts_with(crate::collsugar::COLL_ADD) => {
            for a in args.iter_mut() {
                want(cx, a, None);
            }
            let sp = e.span;
            coll_push(cx, name, args, sp);
        }
        ExprKind::Call(name, args, _) => {
            // r742b: `apply_g(id, 4)` -- a generic callee with a generic fn as a value
            // argument: the type arguments come from the other arguments.
            let mut inferred: Option<Vec<TypeExpr>> = None;
            if crate::sema_generic::is_generic_fn(name)
                && crate::impls::method_name(name).is_none()
                && args.iter().any(|a| generic_fn_value(cx, a).is_some())
            {
                let tys: Vec<(Option<TypeExpr>, Option<TypeExpr>)> = args.iter().map(|a| arg_ty(cx, a)).collect();
                let strong: Vec<Option<TypeExpr>> = tys.iter().map(|t| t.0.clone()).collect();
                let weak: Vec<Option<TypeExpr>> = tys.iter().map(|t| t.1.clone()).collect();
                match crate::sema_generic::infer_call(name, &strong, &weak, e.span) {
                    Ok((m, ptys, ret)) => {
                        if let Some(r) = ret {
                            cx.inst_rets.insert(m.clone(), r);
                        }
                        *name = m;
                        cx.pending = true;
                        inferred = Some(ptys);
                    }
                    Err(msg) => cx.dg.error(e.span, msg),
                }
            }
            let tys: Option<Vec<TypeExpr>> = if inferred.is_some() { inferred } else { match crate::impls::method_name(name) {
                Some(m) => cx.msigs.get(m).and_then(|v| {
                    let c: Vec<&Vec<TypeExpr>> = v.iter().filter(|s| s.len() == args.len()).collect();
                    let first = c.first()?;
                    // all candidates must agree on every fn-typed position
                    for i in 0..args.len() {
                        let k = ty_key(&first[i]);
                        if c.iter().any(|s| ty_key(&s[i]) != k) {
                            let mut out: Vec<TypeExpr> = (*first).clone();
                            out[i] = TypeExpr::Named(INFER.to_string(), e.span);
                            return Some(out);
                        }
                    }
                    Some((*first).clone())
                }),
                None => cx.sigs.get(name.as_str()).cloned(),
            } };
            for (i, a) in args.iter_mut().enumerate() {
                let t = tys.as_ref().and_then(|v| v.get(i));
                want(cx, a, t);
            }
        }
    }
}
