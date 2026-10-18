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
use std::collections::HashMap;

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
        let b = p.block("of a lambda");
        p.no_struct_lit = saved;
        let r = ret.or(Some(TypeExpr::Named(RET_BLK.to_string(), start)));
        (b, r)
    } else {
        let e = p.expr();
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
    ret: Option<TypeExpr>,
    dg: &'a mut Diags,
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
    let mut sigs: HashMap<String, Vec<TypeExpr>> = HashMap::new();
    let mut msigs: HashMap<String, Vec<Vec<TypeExpr>>> = HashMap::new();
    for f in &prog.funcs {
        let tys: Vec<TypeExpr> = f.params.iter().map(|p| p.ty.clone()).collect();
        if f.params.first().map_or(false, |p| p.name == "self") {
            if let Some(i) = f.name.rfind("__") {
                msigs.entry(f.name[i + 2..].to_string()).or_default().push(tys.clone());
            }
        }
        sigs.insert(f.name.clone(), tys);
    }
    let mut fields: HashMap<String, HashMap<String, TypeExpr>> = HashMap::new();
    for s in &prog.structs {
        fields.insert(s.name.clone(), s.fields.iter().map(|(n, t, _)| (n.clone(), t.clone())).collect());
    }
    let mut cx = Cx { sigs: &sigs, msigs: &msigs, fields: &fields, ret: None, dg };
    for f in prog.funcs.iter_mut() {
        cx.ret = f.ret.clone();
        block(&mut cx, &mut f.body);
    }
    cx.ret = None;
    for c in prog.consts.iter_mut() {
        want(&mut cx, &mut c.value, None);
    }
    for s in prog.statics.iter_mut() {
        want(&mut cx, &mut s.value, None);
    }
}

fn block(cx: &mut Cx, b: &mut Block) {
    for s in b.stmts.iter_mut() {
        stmt(cx, s);
    }
}

fn stmt(cx: &mut Cx, s: &mut Stmt) {
    match s {
        Stmt::Let { ty, init, .. } => {
            let t = ty.clone();
            want(cx, init, t.as_ref());
        }
        Stmt::AssignOp { target, value, .. } | Stmt::Assign { target, value, .. } => {
            want(cx, target, None);
            want(cx, value, None);
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
        Stmt::For { start, end, body, .. } => {
            want(cx, start, None);
            want(cx, end, None);
            block(cx, body);
        }
        Stmt::Defer(inner, _, _) => stmt(cx, inner),
        Stmt::Expr(e) => want(cx, e, None),
        Stmt::Block(b) => block(cx, b),
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
    }
}

/// Visit `e`, knowing the type the context wants (if any).
fn want(cx: &mut Cx, e: &mut Expr, t: Option<&TypeExpr>) {
    match &mut e.kind {
        ExprKind::Call(n, args, _) if n == crate::errors::TRY_NAME && args.len() == 1 => {
            // `try gc |x| ..`: the lambda is the operand
            want(cx, &mut args[0], t);
        }
        ExprKind::Lambda(d) => resolve(cx, d, t, e.span),
        _ => walk(cx, e),
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
    block(cx, &mut d.body);
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
        ExprKind::Call(name, args, _) => {
            let tys: Option<Vec<TypeExpr>> = match crate::impls::method_name(name) {
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
            };
            for (i, a) in args.iter_mut().enumerate() {
                let t = tys.as_ref().and_then(|v| v.get(i));
                want(cx, a, t);
            }
        }
    }
}
