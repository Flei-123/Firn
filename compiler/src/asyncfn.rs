// SPDX-License-Identifier: MPL-2.0
//! `async fn` / `.await` lowering (r1563, docs/ASYNC.md).
//!
//! The parser reads `async fn` as an ordinary function carrying the marker
//! attribute `__async`, and `x.await` as the field access `await`. At the end
//! of a module this pass cuts every async body at its top-level `.await`
//! statements into segments and emits a state machine as plain Firn source
//! text (frame struct, entry function, driver, one function per segment);
//! the user statements are moved into the segment functions as syntax trees.

use crate::asyncfn_hoist::Hoist;
use crate::ast::*;
use crate::diag::{Diags, Span};
use crate::fx::HashMap;

pub const MARKER: &str = "__async";
pub(crate) const AWAIT: &str = "await";

enum Bind {
    Let { name: String, mutable: bool },
    Assign(Expr),
    Discard,
    Return,
}

struct Await {
    bind: Bind,
    ty: String,
    expr: Expr,
}

struct Seg {
    stmts: Vec<Stmt>,
    wait: Option<Await>,
}

struct Local {
    name: String,
    ty: String,
    mutable: bool,
    seg: i64,
}

fn ty_text(t: &TypeExpr) -> String {
    match t {
        TypeExpr::Named(n, _) => n.clone(),
        TypeExpr::Ptr { mutable, inner, .. } => {
            format!("*{}{}", if *mutable { "mut " } else { "" }, ty_text(inner))
        }
        TypeExpr::Array { elem, len, .. } => format!("[{}; {}]", ty_text(elem), len),
        TypeExpr::Fn { params, ret, .. } => {
            let ps: Vec<String> = params.iter().map(ty_text).collect();
            match ret {
                Some(r) => format!("fn({}) -> {}", ps.join(", "), ty_text(r)),
                None => format!("fn({})", ps.join(", ")),
            }
        }
        TypeExpr::Tuple(items, _) => {
            let ps: Vec<String> = items.iter().map(ty_text).collect();
            format!("({})", ps.join(", "))
        }
    }
}

fn await_inner(e: &Expr) -> Option<&Expr> {
    match &e.kind {
        ExprKind::Field(inner, name, _) if name == AWAIT => Some(inner),
        _ => None,
    }
}

fn await_spans_in_block(b: &Block) -> Vec<Span> {
    let mut out = Vec::new();
    crate::refparam::walk_exprs(b, &mut |e| {
        if let ExprKind::Field(_, name, sp) = &e.kind {
            if name == AWAIT {
                out.push(*sp);
            }
        }
    });
    out
}

fn stmt_block(s: &Stmt) -> Block {
    Block { stmts: vec![s.clone()], span: s.span(), end: s.span() }
}

fn expr_block(e: &Expr) -> Block {
    stmt_block(&Stmt::Expr(e.clone()))
}

fn mentions(stmts: &[Stmt], extra: &[&Expr], name: &str) -> bool {
    let mut b = Block { stmts: stmts.to_vec(), span: Span::none(), end: Span::none() };
    for e in extra {
        b.stmts.push(Stmt::Expr((*e).clone()));
    }
    let mut hit = false;
    crate::refparam::walk_exprs(&b, &mut |e| {
        if let ExprKind::Ident(n) = &e.kind {
            if n == name {
                hit = true;
            }
        }
    });
    hit
}

struct Sigs {
    ret: HashMap<String, String>,
}

impl Sigs {
    fn new(funcs: &[FnDecl]) -> Sigs {
        let mut ret = HashMap::default();
        for f in funcs {
            let Some(r) = &f.ret else { continue };
            if f.attrs.iter().any(|a| a.name == MARKER) {
                ret.insert(f.name.clone(), ty_text(r));
                continue;
            }
            if let TypeExpr::Ptr { inner, .. } = r {
                if let TypeExpr::Named(n, _) = &**inner {
                    if let Some(t) = n.strip_prefix("Fut__") {
                        ret.insert(f.name.clone(), t.to_string());
                    }
                }
            }
        }
        Sigs { ret }
    }

    fn of_expr(&self, e: &Expr, declared: &HashMap<String, String>) -> Option<String> {
        match &e.kind {
            ExprKind::Call(name, _, _) => self.ret.get(name).cloned(),
            ExprKind::Ident(n) => {
                let t = declared.get(n)?;
                let t = t.strip_prefix("*mut ").unwrap_or(t);
                t.strip_prefix("Fut__").map(|s| s.to_string())
            }
            _ => None,
        }
    }
}

fn classify(
    s: Stmt,
    sigs: &Sigs,
    declared: &HashMap<String, String>,
    fn_ret: &str,
    dg: &mut Diags,
) -> Result<(Stmt, Option<Await>), ()> {
    let unsupported = |dg: &mut Diags, sp: Span| {
        dg.error_help(
            sp,
            "`.await` is not supported in this position",
            "use `let x: T = f().await`, `x = f().await`, `f().await` or `return f().await` as a top-level statement of the async fn",
        );
    };
    let infer_fail = |dg: &mut Diags, sp: Span| {
        dg.error_help(
            sp,
            "cannot infer the result type of this `.await`",
            "write the type: `let x: T = f().await`",
        );
    };
    let (bind, inner, ty): (Bind, Expr, Option<String>) = match s {
        Stmt::Let { name, mutable, ty, init, span } => match await_inner(&init) {
            Some(x) => {
                let t = ty.as_ref().map(ty_text).or_else(|| sigs.of_expr(x, declared));
                (Bind::Let { name, mutable }, x.clone(), t.or_else(|| {
                    infer_fail(dg, span);
                    None
                }))
            }
            None => {
                let st = Stmt::Let { name, mutable, ty, init, span };
                return plain(st, dg, &unsupported);
            }
        },
        Stmt::Assign { target, value, span } => match await_inner(&value) {
            Some(x) => {
                let known = match &target.kind {
                    ExprKind::Ident(n) => declared.get(n).cloned(),
                    _ => None,
                };
                let t = known.or_else(|| sigs.of_expr(x, declared));
                (Bind::Assign(target), x.clone(), t.or_else(|| {
                    infer_fail(dg, span);
                    None
                }))
            }
            None => return plain(Stmt::Assign { target, value, span }, dg, &unsupported),
        },
        Stmt::Expr(e) => match await_inner(&e) {
            Some(x) => {
                let t = sigs.of_expr(x, declared);
                (Bind::Discard, x.clone(), t.or_else(|| {
                    infer_fail(dg, e.span);
                    None
                }))
            }
            None => return plain(Stmt::Expr(e), dg, &unsupported),
        },
        Stmt::Return { value: Some(v), span } => match await_inner(&v) {
            Some(x) => (Bind::Return, x.clone(), Some(fn_ret.to_string())),
            None => return plain(Stmt::Return { value: Some(v), span }, dg, &unsupported),
        },
        other => return plain(other, dg, &unsupported),
    };
    let Some(ty) = ty else { return Err(()) };
    if !await_spans_in_block(&expr_block(&inner)).is_empty() {
        unsupported(dg, inner.span);
        return Err(());
    }
    Ok((Stmt::Block(Block { stmts: vec![], span: Span::none(), end: Span::none() }), Some(Await { bind, ty, expr: inner })))
}

fn plain(
    s: Stmt,
    dg: &mut Diags,
    unsupported: &dyn Fn(&mut Diags, Span),
) -> Result<(Stmt, Option<Await>), ()> {
    let spans = await_spans_in_block(&stmt_block(&s));
    if let Some(sp) = spans.first() {
        unsupported(dg, *sp);
        return Err(());
    }
    Ok((s, None))
}

fn split(
    f: &mut FnDecl,
    sigs: &Sigs,
    ret: &str,
    hoist: &mut Hoist,
    dg: &mut Diags,
) -> Result<Vec<Seg>, ()> {
    let mut declared: HashMap<String, String> = HashMap::default();
    for p in &f.params {
        declared.insert(p.name.clone(), ty_text(&p.ty));
    }
    let mut segs = vec![Seg { stmts: Vec::new(), wait: None }];
    let mut ok = true;
    let stmts: Vec<Stmt> = std::mem::take(&mut f.body.stmts).into_iter().flat_map(|s| hoist.stmt(s)).collect();
    for s in stmts {
        if let Stmt::Let { name, ty: Some(t), .. } = &s {
            declared.insert(name.clone(), ty_text(t));
        }
        match classify(s, sigs, &declared, ret, dg) {
            Ok((st, None)) => segs.last_mut().unwrap().stmts.push(st),
            Ok((_, Some(w))) => {
                if let Bind::Let { name, .. } = &w.bind {
                    declared.insert(name.clone(), w.ty.clone());
                }
                segs.last_mut().unwrap().wait = Some(w);
                segs.push(Seg { stmts: Vec::new(), wait: None });
            }
            Err(()) => ok = false,
        }
    }
    if ok { Ok(segs) } else { Err(()) }
}

fn collect_locals(f: &FnDecl, segs: &[Seg], dg: &mut Diags) -> Result<Vec<Local>, ()> {
    let mut decls: Vec<(String, Option<String>, bool, i64, Span)> = Vec::new();
    for (k, seg) in segs.iter().enumerate() {
        if k > 0 {
            if let Some(Await { bind: Bind::Let { name, mutable }, ty, .. }) = &segs[k - 1].wait {
                decls.push((name.clone(), Some(ty.clone()), *mutable, k as i64, f.span));
            }
        }
        for s in &seg.stmts {
            if let Stmt::Let { name, mutable, ty, span, .. } = s {
                decls.push((name.clone(), ty.as_ref().map(ty_text), *mutable, k as i64, *span));
            }
        }
    }
    let mut out: Vec<Local> = Vec::new();
    for p in &f.params {
        out.push(Local { name: p.name.clone(), ty: ty_text(&p.ty), mutable: false, seg: -1 });
    }
    let mut ok = true;
    for (name, ty, mutable, seg, span) in decls {
        let used_later = (seg as usize + 1..segs.len()).any(|j| {
            let mut extra: Vec<&Expr> = Vec::new();
            if let Some(Await { bind: Bind::Assign(t), .. }) = &segs[j - 1].wait {
                extra.push(t);
            }
            if let Some(w) = &segs[j].wait {
                extra.push(&w.expr);
            }
            mentions(&segs[j].stmts, &extra, &name)
        });
        if !used_later {
            continue;
        }
        if out.iter().any(|l| l.name == name) {
            dg.error(span, format!("async fn '{}': '{}' is declared twice across an await", f.name, name));
            ok = false;
            continue;
        }
        match ty {
            Some(ty) => out.push(Local { name, ty, mutable, seg }),
            None => {
                dg.error_help(
                    span,
                    format!("'{}' is used after an `.await` and needs a type", name),
                    "write the type: `let x: T = ...`",
                );
                ok = false;
            }
        }
    }
    if ok { Ok(out) } else { Err(()) }
}

fn gen_source(name: &str, params: &[Param], ret: &str, segs: &[Seg], locals: &[Local]) -> String {
    let fr = format!("__af_{}_fr", name);
    let run = format!("__af_{}_run", name);
    let m = segs.len() - 1;
    let mut s = String::new();
    s.push_str(&format!("struct {} {{\n    __state: i32,\n    __fut: *mut Fut[{}],\n    __slot: {},\n", fr, ret, ret));
    for k in 0..m {
        let t = &segs[k].wait.as_ref().unwrap().ty;
        s.push_str(&format!("    __s{}: *mut Fut[{}],\n", k, t));
    }
    for l in locals {
        s.push_str(&format!("    {}: {},\n", l.name, l.ty));
    }
    s.push_str("}\n");
    let plist: Vec<String> = params.iter().map(|p| format!("{}: {}", p.name, ty_text(&p.ty))).collect();
    s.push_str(&format!("fn {}({}) -> *mut Fut[{}] {{\n", name, plist.join(", "), ret));
    s.push_str(&format!("    let __fr: *mut {} = fut_frame_new[{}]()\n", fr, fr));
    s.push_str(&format!("    let __f: *mut Fut[{}] = fut_new[{}]()\n    (*__fr).__fut = __f\n", ret, ret));
    for p in params {
        s.push_str(&format!("    (*__fr).{} = {}\n", p.name, p.name));
    }
    s.push_str(&format!("    {}(__fr as u64)\n    return __f\n}}\n", run));
    s.push_str(&format!("fn {}(ud: u64) {{\n    let __fr: *mut {} = ud as *mut {}\n    while true {{\n", run, fr, fr));
    s.push_str(&format!("        let __st: i32 = (*__fr).__state\n        var __r: {} = (*__fr).__slot\n", ret));
    for k in 0..=m {
        s.push_str(&format!("        if __st == {} {{\n            __r = __af_{}_s{}(__fr)\n", k, name, k));
        s.push_str(&format!(
            "            if (*__fr).__state == __st {{\n                let __f: *mut Fut[{}] = (*__fr).__fut\n                fut_frame_free[{}](__fr)\n                fut_set[{}](__f, __r)\n                return\n            }}\n",
            ret, fr, ret
        ));
        if k < m {
            let t = &segs[k].wait.as_ref().unwrap().ty;
            s.push_str(&format!(
                "            if !fut_ready[{}]((*__fr).__s{}) {{\n                fut_wait[{}]((*__fr).__s{}, {}, ud)\n                return\n            }}\n",
                t, k, t, k, run
            ));
        }
        s.push_str("            continue\n        }\n");
    }
    s.push_str("    }\n}\n");
    for k in 0..=m {
        s.push_str(&format!("fn __af_{}_s{}(__fr: *mut {}) -> {} {{\n", name, k, fr, ret));
        for l in locals.iter().filter(|l| l.seg < k as i64) {
            let kw = if l.mutable { "var" } else { "let" };
            s.push_str(&format!("    {} {}: {} = (*__fr).{}\n", kw, l.name, l.ty, l.name));
        }
        if k > 0 {
            let prev = segs[k - 1].wait.as_ref().unwrap();
            let take = format!("fut_take[{}]((*__fr).__s{})", prev.ty, k - 1);
            match &prev.bind {
                Bind::Let { name, mutable } => {
                    let kw = if *mutable { "var" } else { "let" };
                    s.push_str(&format!("    {} {}: {} = {}\n", kw, name, prev.ty, take));
                }
                Bind::Assign(_) => {
                    s.push_str(&format!("    let __bv: {} = {}\n    __splice_b{}()\n", prev.ty, take, k));
                }
                Bind::Discard => s.push_str(&format!("    fut_discard[{}]((*__fr).__s{})\n", prev.ty, k - 1)),
                Bind::Return => s.push_str(&format!("    return {}\n", take)),
            }
        }
        s.push_str(&format!("    __splice_{}()\n", k));
        if k < m {
            for l in locals.iter().filter(|l| l.seg <= k as i64) {
                s.push_str(&format!("    (*__fr).{} = {}\n", l.name, l.name));
            }
            s.push_str(&format!("    (*__fr).__s{} = __x{}\n    (*__fr).__state = {}\n    return (*__fr).__slot\n", k, k, k + 1));
        }
        s.push_str("}\n");
    }
    s
}

fn marker_of(s: &Stmt) -> Option<&str> {
    match s {
        Stmt::Expr(Expr { kind: ExprKind::Call(n, args, _), .. }) if args.is_empty() && n.starts_with("__splice_") => Some(n),
        _ => None,
    }
}

fn fresh_ident(prog: &mut Program, name: &str, span: Span) -> Expr {
    let id = prog.expr_count;
    prog.expr_count += 1;
    Expr { id, span, kind: ExprKind::Ident(name.to_string()) }
}

fn lower_one(prog: &mut Program, mut f: FnDecl, sigs: &Sigs, dg: &mut Diags) {
    let span = f.span;
    let attrs: Vec<Attr> = f.attrs.iter().filter(|a| a.name != MARKER).cloned().collect();
    let Some(ret_ty) = f.ret.clone() else {
        dg.error_help(span, format!("async fn '{}' needs a result type", f.name), "write `-> T` (use `-> bool` when there is nothing to return)");
        return;
    };
    if f.params.iter().any(|p| p.refk != 0 || p.default.is_some()) {
        dg.error(span, format!("async fn '{}': `&`, `inout` and default parameters are not supported", f.name));
        return;
    }
    let ret = ty_text(&ret_ty);
    let mut hoist = Hoist::new(prog.expr_count);
    let split_result = split(&mut f, sigs, &ret, &mut hoist, dg);
    prog.expr_count = hoist.next_id();
    let Ok(mut segs) = split_result else { return };
    let Ok(locals) = collect_locals(&f, &segs, dg) else { return };
    let mut text = String::new();
    for imp in &prog.imports {
        text.push_str(&format!("import {}\n", imp.path.join(".")));
    }
    text.push_str(&gen_source(&f.name, &f.params, &ret, &segs, &locals));
    let name = format!("<async {}>", f.name);
    let file = dg.add_file(&name, &text);
    crate::dwarf::add_file(&name);
    let toks = crate::lexer::lex_file(&text, file, dg);
    let mut gen = crate::parser::parse_module(&toks, dg, file, prog.expr_count);
    prog.expr_count = gen.expr_count;
    let mut table: HashMap<String, Vec<Stmt>> = HashMap::default();
    let m = segs.len() - 1;
    for k in 0..=m {
        table.insert(format!("__splice_{}", k), std::mem::take(&mut segs[k].stmts));
        if k > 0 {
            let target = match segs[k - 1].wait.as_mut() {
                Some(Await { bind: Bind::Assign(t), .. }) => Some(t.clone()),
                _ => None,
            };
            if let Some(t) = target {
                let value = fresh_ident(prog, "__bv", t.span);
                table.insert(format!("__splice_b{}", k), vec![Stmt::Assign { target: t.clone(), value, span: t.span }]);
            }
        }
    }
    let mut exprs: HashMap<String, Expr> = HashMap::default();
    for k in 0..m {
        if let Some(w) = segs[k].wait.take() {
            exprs.insert(format!("__x{}", k), w.expr);
        }
    }
    for g in gen.funcs.iter_mut() {
        let old = std::mem::take(&mut g.body.stmts);
        for s in old {
            if let Some(key) = marker_of(&s) {
                if let Some(list) = table.remove(key) {
                    g.body.stmts.extend(list);
                    continue;
                }
            }
            match s {
                Stmt::Assign { target, value: Expr { kind: ExprKind::Ident(n), .. }, span } if exprs.contains_key(&n) => {
                    let value = exprs.remove(&n).unwrap();
                    g.body.stmts.push(Stmt::Assign { target, value, span });
                }
                other => g.body.stmts.push(other),
            }
        }
        if g.name == f.name {
            g.attrs = attrs.clone();
            g.span = span;
        }
    }
    prog.funcs.extend(gen.funcs);
    prog.structs.extend(gen.structs);
}

pub fn expand(prog: &mut Program, dg: &mut Diags) {
    let mut misuse: Vec<Span> = Vec::new();
    for f in &prog.funcs {
        if f.attrs.iter().any(|a| a.name == MARKER) {
            continue;
        }
        misuse.extend(await_spans_in_block(&f.body));
    }
    for sp in misuse {
        dg.error_help(sp, "`.await` outside of an `async fn`", "mark the enclosing function `async fn`");
    }
    if !prog.funcs.iter().any(|f| f.attrs.iter().any(|a| a.name == MARKER)) {
        return;
    }
    let sigs = Sigs::new(&prog.funcs);
    let mut asyncs: Vec<FnDecl> = Vec::new();
    let mut rest: Vec<FnDecl> = Vec::new();
    for f in std::mem::take(&mut prog.funcs) {
        if f.attrs.iter().any(|a| a.name == MARKER) {
            asyncs.push(f);
        } else {
            rest.push(f);
        }
    }
    prog.funcs = rest;
    for f in asyncs {
        lower_one(prog, f, &sigs, dg);
    }
}
