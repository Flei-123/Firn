// SPDX-License-Identifier: MPL-2.0
//! **ROUND REF4 -- `for x in array { ... }`** (element iteration).
//!
//! ```firn
//! var sum: i64 = 0
//! for x in values {        // values: [i64; 8]  (or a `&[i64; 8]` parameter)
//!     sum += x
//! }
//! ```
//!
//! The parser turns it into the counting loop the language already has:
//!
//! ```firn
//! for __each#L_C in 0 as usize..__each_len(values) {
//!     let x = values[__each#L_C]
//!     ...
//! }
//! ```
//!
//! `x` is an ordinary immutable `let`: a COPY of the element (so the element
//! type must be trivial -- a value with a `drop` cannot be moved out of an
//! array, and `moves.rs` says so). `__each_len(a)` is the one new intrinsic: the
//! length of an array (or of an array behind a reference parameter), a
//! constant of type `usize` -- nothing is computed at run time. Because the
//! array expression appears twice it must be a PLACE without calls: a name,
//! a field path, `*p`, or an index by a name / number. Anything else is a
//! syntax error that tells the author to name the array first.
//!
//! **Vec and str (r664, vspy-fnc).** The same desugar also walks a
//! `Vec[T]` (element copy, `T` scalar or plain struct) and a `str`/`Span`
//! (the bytes, `u8`; NOT code points: decode with `str.utf8` if needed).
//! The hidden loop index may index such a sequence -- and ONLY the hidden
//! index (`__each#..`): `v[i]` written by hand stays an error, because this
//! element access has no bounds check. The length is read once when the loop
//! starts; do not push to or clear the Vec inside the body.
//!
//! `continue` and `break` work as in every other `for` (the hidden counter is
//! raised in the step block of the loop).

use crate::ast::*;
use crate::diag::Span;
use crate::fir::{BinOp as FBin, FTy, Op, Val};
use crate::lower::Lower;
use crate::parser::{Parser, TuplePat};
use crate::sema::Checker;
use crate::types::Type;

/// The hidden intrinsic: the length of an array.
pub(crate) const LEN: &str = "__each_len";

/// Name prefix of the hidden loop counter (`__each#L_C`).
pub(crate) const HIDDEN: &str = "__each#";

/// Offsets of the two words every walkable sequence starts with: the data
/// pointer at 0 and the length at 8 (`Vec`: ptr, len, cap; `str`: p, n).
const PTR_OFF: u64 = 0;
const LEN_OFF: u64 = 8;

/// Is `t` a sequence that `for x in s` can walk besides an array: a `Vec[T]`
/// instance or a `str`-shaped view? Returns the struct index (also behind
/// one reference).
pub(crate) fn seq_struct(tcx: &crate::types::TypeCtx, t: &Type) -> Option<usize> {
    let t = match t {
        Type::Ptr { inner, .. } => &**inner,
        other => other,
    };
    let sidx = match t {
        Type::Struct(i) => *i,
        _ => return None,
    };
    let sd = tcx.structs.get(sidx)?;
    let word = |name: &str, off: u64| sd.field(name).map(|f| f.offset == off).unwrap_or(false);
    if crate::strtype::is_str_like(t) {
        return Some(sidx);
    }
    let vec_like = sd.name.starts_with("Vec__")
        && sd.fields.len() == 3
        && word("ptr", PTR_OFF)
        && word("len", LEN_OFF)
        && word("cap", 16);
    if vec_like {
        Some(sidx)
    } else {
        None
    }
}

/// Field names of the two words: `str` calls them `p`/`n`, `Vec` `ptr`/`len`.
fn word_names(tcx: &crate::types::TypeCtx, sidx: usize) -> (&'static str, &'static str) {
    if tcx.structs[sidx].field("ptr").is_some() {
        ("ptr", "len")
    } else {
        ("p", "n")
    }
}

/// The element type of a sequence for the checker: `u8` for a `str`, the type
/// argument for a `Vec[T]`.
pub(crate) fn seq_elem(ck: &mut Checker, t: &Type) -> Option<Type> {
    let sidx = seq_struct(&ck.tcx, t)?;
    if crate::strtype::is_str_like(&Type::Struct(sidx)) {
        return Some(Type::U8);
    }
    let name = ck.tcx.structs[sidx].name.clone();
    let inst = crate::sema_generic::instantiation(&name)?;
    if inst.base != "Vec" || inst.args.len() != 1 {
        return None;
    }
    let te = inst.args[0].clone();
    let et = ck.resolve_ty(&te);
    if et.is_error() {
        None
    } else {
        Some(et)
    }
}

/// Address of the data of a sequence and its length (runtime loads).
pub(crate) fn seq_words(lo: &mut Lower, base: &Expr) -> Option<(Val, Val)> {
    let bt = lo.ty_of(base);
    let sidx = match seq_struct(&lo.info.tcx, &bt) {
        Some(i) => i,
        None => return lo.ice(base.span, "walking something that is not a sequence"),
    };
    let baddr = match &bt {
        Type::Ptr { .. } => lo.lower_expr(base)?,
        _ => lo.lower_addr(base)?,
    };
    let (pn, ln) = word_names(&lo.info.tcx, sidx);
    let pa = lo.field_addr(baddr, sidx, pn, base.span)?;
    let la = lo.field_addr(baddr, sidx, ln, base.span)?;
    let data = lo.load(FTy::Ptr, pa);
    let len = lo.load(FTy::U64, la);
    Some((data, len))
}

/// Is `e` a place that may be evaluated twice (no calls, no side effects)?
fn place_ok(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Ident(_) => true,
        ExprKind::Field(b, ..) => place_ok(b),
        ExprKind::Unary(UnOp::Deref, b) => place_ok(b),
        ExprKind::Index(b, i) => {
            place_ok(b) && matches!(i.kind, ExprKind::Ident(_) | ExprKind::Int(_))
        }
        _ => false,
    }
}

/// A copy of a place with FRESH expression ids (the type table is keyed by id).
fn reid(p: &mut Parser, e: &Expr) -> Expr {
    let kind = match &e.kind {
        ExprKind::Ident(n) => ExprKind::Ident(n.clone()),
        ExprKind::Int(v) => ExprKind::Int(*v),
        ExprKind::Field(b, f, sp) => ExprKind::Field(Box::new(reid(p, b)), f.clone(), *sp),
        ExprKind::Unary(op, b) => ExprKind::Unary(*op, Box::new(reid(p, b))),
        ExprKind::Index(b, i) => ExprKind::Index(Box::new(reid(p, b)), Box::new(reid(p, i))),
        other => other.clone(),
    };
    p.mk(e.span, kind)
}

/// `for name in arr { body }` -- called by `Parser::for_stmt` once the body is
/// parsed. Returns the desugared counting loop.
pub(crate) fn build(
    p: &mut Parser,
    name: String,
    name_span: Span,
    arr: Expr,
    body: Block,
    start: Span,
) -> Stmt {
    if !place_ok(&arr) {
        p.dg.error_note(
            arr.span,
            "'for ... in' needs an array variable",
            "name the array first (`let a = ...`) and loop over `a`: the loop reads it twice, so it \
             must be a name, a field path or `*p`, never a call",
        );
        return Stmt::Error(start);
    }
    let idx = format!("{}{}_{}", HIDDEN, start.line, start.col);
    let arr_len = reid(p, &arr);
    let len = p.mk(arr.span, ExprKind::Call(LEN.to_string(), vec![arr_len], arr.span));
    let lit = p.mk(arr.span, ExprKind::Int(0));
    let zero = p.mk(
        arr.span,
        ExprKind::Cast(Box::new(lit), TypeExpr::Named("usize".to_string(), arr.span)),
    );
    let base = reid(p, &arr);
    let at = p.mk(name_span, ExprKind::Ident(idx.clone()));
    let elem = p.mk(arr.span, ExprKind::Index(Box::new(base), Box::new(at)));
    let bind = Stmt::Let { name, mutable: false, ty: None, init: elem, span: name_span };
    let mut stmts = Vec::with_capacity(body.stmts.len() + 1);
    stmts.push(bind);
    stmts.extend(body.stmts);
    let body = Block { stmts, span: body.span, end: body.end };
    Stmt::For {
        name: idx,
        start: zero,
        end: len,
        body,
        inclusive: false,
        name_span,
        span: start,
    }
}

/// r666 / r708 -- `for (i, x) in enumerate(v)`, `for (x, y) in zip(a, b)` and
/// `for (a, b) in vec_of_tuples`.
///
/// ```firn
/// for __each#L_C in 0 as usize..__each_len(v) {
///     let i = __each#L_C        // usize
///     let x = v[__each#L_C]     // x may be a nested pattern: `(i, (a, b))`
///     ...
/// }
/// ```
/// `zip` walks to the SHORTER length (`if la < lb { la } else { lb }`). Without
/// `enumerate`/`zip` the whole pattern is the pattern of the element (a
/// tuple): `let __tup#k#n = v[__each#L_C]` then one `let` per name. Every
/// sequence must be a place without calls, as with `for x in v`. The hidden
/// bindings are made BEFORE the body is parsed (the `__tup#` numbers run in
/// source order, like firnc1).
pub(crate) fn build_pair(p: &mut Parser, pat: TuplePat, src: Expr, start: Span) -> Stmt {
    let (fname, fargs) = match &src.kind {
        ExprKind::Call(n, a, _) => (n.clone(), a.clone()),
        _ => (String::new(), Vec::new()),
    };
    let parts: Vec<TuplePat> = match &pat {
        TuplePat::Tuple(e, _) => e.clone(),
        _ => Vec::new(),
    };
    let is_enum = fname == "enumerate" && fargs.len() == 1 && parts.len() == 2;
    let is_zip = fname == "zip" && fargs.len() == 2 && parts.len() == 2;
    // the sequences that are read more than once
    let seqs: Vec<Expr> = if is_enum || is_zip { fargs.clone() } else { vec![src.clone()] };
    let mut ok = true;
    for a in &seqs {
        if !place_ok(a) {
            ok = false;
            p.dg.error_note(
                a.span,
                "'for ... in' needs a sequence variable",
                "name the sequence first (`let a = ...`): the loop reads it more than once, so it \
                 must be a name, a field path or `*p`, never a call",
            );
        }
    }
    if !ok {
        let _ = p.for_pair_body();
        return Stmt::Error(start);
    }
    let sp = src.span;
    let idx = format!("{}{}_{}", HIDDEN, start.line, start.col);
    let len_of = |p: &mut Parser, a: &Expr| {
        let r = reid(p, a);
        p.mk(sp, ExprKind::Call(LEN.to_string(), vec![r], sp))
    };
    let len = if is_zip {
        let la = len_of(p, &seqs[0]);
        let lb = len_of(p, &seqs[1]);
        let la2 = len_of(p, &seqs[0]);
        let lb2 = len_of(p, &seqs[1]);
        let cond = p.mk(sp, ExprKind::Binary(BinOp::Lt, Box::new(la), Box::new(lb)));
        p.mk(sp, ExprKind::IfElse(Box::new(cond), Box::new(la2), Box::new(lb2)))
    } else {
        len_of(p, &seqs[0])
    };
    let lit = p.mk(sp, ExprKind::Int(0));
    let zero = p.mk(sp, ExprKind::Cast(Box::new(lit), TypeExpr::Named("usize".to_string(), sp)));
    let mut stmts: Vec<Stmt> = Vec::new();
    let elem_at = |p: &mut Parser, k: usize| {
        let base = reid(p, &seqs[k]);
        let i2 = p.mk(sp, ExprKind::Ident(idx.clone()));
        p.mk(sp, ExprKind::Index(Box::new(base), Box::new(i2)))
    };
    if is_enum {
        let init = p.mk(sp, ExprKind::Ident(idx.clone()));
        p.bind_pat(&parts[0], init, &mut stmts);
        let elem = elem_at(p, 0);
        p.bind_pat(&parts[1], elem, &mut stmts);
    } else if is_zip {
        for k in 0..2 {
            let elem = elem_at(p, k);
            p.bind_pat(&parts[k], elem, &mut stmts);
        }
    } else {
        let elem = elem_at(p, 0);
        p.bind_pat(&pat, elem, &mut stmts);
    }
    let body = p.for_pair_body();
    stmts.extend(body.stmts);
    let body = Block { stmts, span: body.span, end: body.end };
    Stmt::For {
        name: idx,
        start: zero,
        end: len,
        body,
        inclusive: false,
        name_span: sp,
        span: start,
    }
}

/// `// HOOK foreach` in `sema::Checker::call` -- the type of `__each_len(a)`.
pub(crate) fn hook_call(ck: &mut Checker, name: &str, args: &[Expr], espan: Span) -> Option<Type> {
    if name != LEN {
        return None;
    }
    if args.len() != 1 {
        return Some(Type::Error);
    }
    let t = ck.expr(&args[0], None);
    if seq_struct(&ck.tcx, &t).is_some() {
        return Some(Type::Usize);
    }
    let arr = match &t {
        Type::Array(..) => true,
        Type::Ptr { inner, .. } => matches!(**inner, Type::Array(..)),
        Type::Error => return Some(Type::Error),
        _ => false,
    };
    if !arr {
        ck.dg.error_note(
            args[0].span,
            format!("'for ... in' needs an array, found {}", ck.tcx.name_of(&t)),
            "the element loop walks a fixed size array `[T; N]`; for other sequences use an index loop",
        );
        let _ = espan;
        return Some(Type::Error);
    }
    Some(Type::Usize)
}

/// The length behind the type of the array expression.
fn length_of(t: &Type) -> Option<u64> {
    match t {
        Type::Array(_, n) => Some(*n),
        Type::Ptr { inner, .. } => match &**inner {
            Type::Array(_, n) => Some(*n),
            _ => None,
        },
        _ => None,
    }
}

/// Hook from `lower::lower_call`: the length is a constant.
pub(crate) fn lower_call(lo: &mut Lower, args: &[Expr], span: Span) -> Option<Option<Val>> {
    if args.len() != 1 {
        return lo.ice(span, "__each_len with wrong arity");
    }
    if seq_struct(&lo.info.tcx, &lo.ty_of(&args[0])).is_some() {
        let (_, len) = seq_words(lo, &args[0])?;
        return Some(Some(len));
    }
    let n = match length_of(&lo.ty_of(&args[0])) {
        Some(n) => n,
        None => return lo.ice(span, "__each_len of something that is not an array"),
    };
    Some(Some(lo.constant(FTy::U64, n as i128)))
}

// ------------------------------------------------------------ Vec slice (r728)

/// Name of the slice method (the sugar `v[a:b]` of the parser).
const SLICE: &str = "slice";

thread_local! {
    /// Element type per `Vec__..` struct, noted by the type check for the
    /// lowering of `v[a:b]` (the lowering cannot resolve type expressions).
    static SLICE_ELEM: std::cell::RefCell<std::collections::HashMap<String, Type>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

pub(crate) fn slice_reset() {
    SLICE_ELEM.with(|m| m.borrow_mut().clear());
}

/// Type check of `v[a:b]` on a Vec: checks the two bounds as `i64`, notes the
/// element type for the lowering, yields the Vec type itself (a view).
pub(crate) fn check_vec_slice(ck: &mut Checker, recv_t: Type, args: &[Expr]) -> Type {
    for a in &args[1..] {
        let _ = ck.expr(a, Some(&Type::I64));
    }
    let sidx = match seq_struct(&ck.tcx, &recv_t) {
        Some(i) => i,
        None => return Type::Error,
    };
    if let Some(et) = seq_elem(ck, &recv_t) {
        let name = ck.tcx.structs[sidx].name.clone();
        SLICE_ELEM.with(|m| m.borrow_mut().insert(name, et));
    }
    Type::Struct(sidx)
}

/// r728: is `recv.slice(a, b)` on a `Vec[T]` WITHOUT a method of that name?
/// `Vec[T]` has no methods, so the sugar `v[a:b]` is resolved here: the
/// result is a non-owning VIEW (`Vec[T]` with `cap = 0`, same element
/// storage, Python bound rules: negative from the end, clamped). `str` and
/// `Span` keep their own `slice` method (strtype / lib).
pub(crate) fn is_vec_slice(tcx: &crate::types::TypeCtx, fns: &std::collections::HashMap<String, crate::sema::FnSig>, name: &str, recv: &Type, nargs: usize) -> bool {
    if crate::impls::method_name(name) != Some(SLICE) || nargs != 3 {
        return false;
    }
    let sidx = match seq_struct(tcx, recv) {
        Some(i) => i,
        None => return false,
    };
    if !tcx.structs[sidx].name.starts_with("Vec__") {
        return false;
    }
    crate::impls::target_of(tcx, fns, SLICE, recv).is_none()
}

/// Clamps the Python bound `x` (I64) to `0..=n` (I64) without branches.
fn clamp_bound(lo: &mut Lower, x: Val, n: Val) -> Val {
    let c63 = lo.constant(FTy::I64, 63);
    let neg = lo.push(FTy::I64, Op::Bin(FBin::Shr, x, c63));
    let add = lo.push(FTy::I64, Op::Bin(FBin::And, n, neg));
    let x1 = lo.push(FTy::I64, Op::Bin(FBin::Add, x, add));
    let m1 = lo.push(FTy::I64, Op::Bin(FBin::Shr, x1, c63));
    let ones = lo.constant(FTy::I64, -1);
    let nm1 = lo.push(FTy::I64, Op::Bin(FBin::Xor, m1, ones));
    let x2 = lo.push(FTy::I64, Op::Bin(FBin::And, x1, nm1));
    let d = lo.push(FTy::I64, Op::Bin(FBin::Sub, n, x2));
    let m2 = lo.push(FTy::I64, Op::Bin(FBin::Shr, d, c63));
    let dm = lo.push(FTy::I64, Op::Bin(FBin::And, d, m2));
    lo.push(FTy::I64, Op::Bin(FBin::Add, x2, dm))
}

/// Lowers `v[a:b]` on a `Vec[T]` into a view written to `dest`.
pub(crate) fn lower_vec_slice(
    lo: &mut Lower,
    args: &[Expr],
    dest: Option<Val>,
    span: Span,
) -> Option<Option<Val>> {
    let recv = &args[0];
    let bt = lo.ty_of(recv);
    let sidx = seq_struct(&lo.info.tcx, &bt)?;
    let ename = lo.info.tcx.structs[sidx].name.clone();
    let et = SLICE_ELEM.with(|m| m.borrow().get(&ename).cloned());
    let esz = match et {
        Some(t) => lo.info.tcx.size_of(&t).max(1),
        None => return lo.ice(span, "slice: element type not recorded by the type check"),
    };
    let (data, len) = seq_words(lo, recv)?;
    let a0 = lo.lower_expr(&args[1])?;
    let b0 = lo.lower_expr(&args[2])?;
    let n = lo.push(FTy::I64, Op::Cast { src: len, from: FTy::U64 });
    let a = clamp_bound(lo, a0, n);
    let b = clamp_bound(lo, b0, n);
    let cnt0 = lo.push(FTy::I64, Op::Bin(FBin::Sub, b, a));
    let c63 = lo.constant(FTy::I64, 63);
    let mc = lo.push(FTy::I64, Op::Bin(FBin::Shr, cnt0, c63));
    let ones = lo.constant(FTy::I64, -1);
    let nmc = lo.push(FTy::I64, Op::Bin(FBin::Xor, mc, ones));
    let cnt = lo.push(FTy::U64, Op::Bin(FBin::And, cnt0, nmc));
    let au = lo.push(FTy::U64, Op::Cast { src: a, from: FTy::I64 });
    let ptr = lo.elem_addr(data, esz, au, FTy::U64);
    let d = match dest {
        Some(d) => d,
        None => lo.alloca(24, 8),
    };
    let (pn, ln) = word_names(&lo.info.tcx, sidx);
    let pa = lo.field_addr(d, sidx, pn, span)?;
    let la = lo.field_addr(d, sidx, ln, span)?;
    let ca = lo.field_addr(d, sidx, "cap", span)?;
    lo.store(FTy::Ptr, pa, ptr);
    lo.store(FTy::U64, la, cnt);
    let zero = lo.constant(FTy::U64, 0);
    lo.store(FTy::U64, ca, zero);
    Some(None)
}
