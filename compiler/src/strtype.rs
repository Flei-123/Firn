// SPDX-License-Identifier: GPL-2.0-only
//! **ROUND 70** — the language type `str`.
//!
//! ## What `str` is
//!
//! A `str` is a **view of octets that nobody may change any more**: two
//! machine words, `p: *mut u8` and `n: usize`, laid out exactly like
//! `str.Span` from `lib/std/str.fi`. That is deliberate and it is the whole
//! trick of this round — the string library of round 42 does not have to be
//! written a second time, `str` is simply the name the LANGUAGE gives to the
//! view the LIBRARY already had.
//!
//! Because of that:
//!
//! * a substring (`trim`, `part`, `ab`, `to`) costs **nothing**: the two
//!   words move, the octets stay where they are. No copy.
//! * `str` is **immutable**. There is no operation that writes into the
//!   octets behind a `str`; whoever wants to build text takes `Bytes`.
//! * a `str` is **layout compatible** with every struct of the shape
//!   `{ *mut u8, usize }` — that is exactly `str.Span`. The conversion in
//!   either direction is free (see `same_view`).
//!
//! ## Where the octets live
//!
//! | origin | storage | freed by |
//! |---|---|---|
//! | literal `"hello"` | the frame of the enclosing function | the frame |
//! | `a + b` | the **GC heap** (`__str_concat`) | the collector |
//! | `Span`/`Bytes` | wherever the buffer lies | its owner |
//!
//! The literal lands in the frame, not in `.rodata` — the same honest price
//! that `SPEC §14.1.str` has been naming since round 39 for `[u8; N]`.
//! Nothing changes about it here; a `str` literal simply puts the address of
//! that frame array into `p`.
//!
//! ## Why the collector, and when it is pulled in
//!
//! `a + b` produces octets that outlive both operands — they need an owner,
//! and the only owner in this language that nobody has to name is the
//! collector (SPEC §3.5). `__str_eq` and `__str_concat` therefore live in
//! `lib/gc/gc.fi`, and a program that works with `str` pulls the collector
//! runtime in automatically (`source_uses_str`).
//!
//! The trigger is read off the TOKENS, exactly like the one for `gc class`:
//! the type name `str` or a text literal next to `+`, `==`, `!=`. That is
//! deliberately conservative — `var t: [u8; 20] = "…"` and `asm("…")` do
//! **not** trigger, which is why the kernel and the freestanding profile see
//! nothing of this round.

use crate::ast::{BinOp, Expr, ExprKind};
use crate::diag::Span;
use crate::lexer::TokKind;
use crate::sema::Checker;
use crate::types::{Type, TypeCtx};
use std::cell::RefCell;

/// The name of the type in the source text — and the name of the struct in
/// `TypeCtx`. A program that declares a struct `str` itself therefore gets
/// the ordinary "already declared" message.
pub(crate) const NAME: &str = "str";

/// **ROUND 88** — the second spelling, out of the C# flavoured alias family
/// of round 70/71 (`types.rs::alias_of`). `string` IS `str`: the name is
/// folded onto `NAME` before the struct lookup (`sema.rs::resolve_ty`), so
/// there is only ever ONE type. It has to stand here as well, because the
/// trigger below reads the TOKENS — a program that only ever writes
/// `string` needs the collector just as much as one that writes `str`.
pub(crate) const NAME_ALIAS: &str = "string";

/// Content comparison of two `str` (`lib/gc/gc.fi`).
pub(crate) const FN_EQ: &str = "__str_eq";
/// Concatenation of two `str` in the GC heap (`lib/gc/gc.fi`).
pub(crate) const FN_CONCAT: &str = "__str_concat";

/// Name of the field holding the pointer — the same name `str.Span` uses.
pub(crate) const F_PTR: &str = "p";
/// Name of the field holding the length in octets.
pub(crate) const F_LEN: &str = "n";

#[derive(Default)]
struct Reg {
    /// Index of the builtin struct in `TypeCtx`.
    idx: Option<usize>,
    /// Indices of all structs shaped `{ *mut u8, usize }` — the views that
    /// `str` may be used for and vice versa.
    views: Vec<usize>,
}

thread_local! {
    static REG: RefCell<Reg> = RefCell::new(Reg::default());
}

/// Resets the registry (one per compilation, `parser::reset_hooks`).
pub(crate) fn hook_reset() {
    REG.with(|r| *r.borrow_mut() = Reg::default());
}

/// Declares the builtin struct. Called ONCE per compilation, at the very
/// beginning of the type check — before enums, interfaces and gc classes,
/// so that the index is the same in every program.
pub(crate) fn declare(tcx: &mut TypeCtx) {
    let idx = tcx.declare(NAME);
    tcx.set_fields(
        idx,
        vec![
            (F_PTR.to_string(), Type::ptr(Type::U8, true)),
            (F_LEN.to_string(), Type::Usize),
        ],
    );
    REG.with(|r| r.borrow_mut().idx = Some(idx));
}

/// Index of the builtin struct, if it has been declared.
pub(crate) fn index() -> Option<usize> {
    REG.with(|r| r.borrow().idx)
}

/// Is this type the builtin `str`?
pub(crate) fn is_str(t: &Type) -> bool {
    match (t, index()) {
        (Type::Struct(i), Some(k)) => *i == k,
        _ => false,
    }
}

/// Is this type a `str` OR a view of the same shape (`str.Span`)?
///
/// The operators of this round work on both — they are the same two words,
/// and the library hands out `Span` (`trim`, `part`, `find`). Nothing can
/// break through this either: `==` and `+` on a struct are an error today.
pub(crate) fn is_str_like(t: &Type) -> bool {
    is_str(t) || same_view(t, &ty())
}

/// The type `str`, or `Type::Error` when it has not been declared.
pub(crate) fn ty() -> Type {
    match index() {
        Some(i) => Type::Struct(i),
        None => Type::Error,
    }
}

/// Notes every struct of the shape `{ p: *mut u8, n: usize }`. Called after
/// the struct layout; from then on `same_view` can answer without a
/// `TypeCtx`.
pub(crate) fn remember_views(tcx: &TypeCtx) {
    let mut views = Vec::new();
    for (i, s) in tcx.structs.iter().enumerate() {
        if s.fields.len() != 2 {
            continue;
        }
        let a = &s.fields[0];
        let b = &s.fields[1];
        let ptr_ok = matches!(&a.ty, Type::Ptr { inner, .. } if **inner == Type::U8);
        if ptr_ok && a.offset == 0 && b.ty == Type::Usize && b.offset == 8 {
            views.push(i);
        }
    }
    REG.with(|r| r.borrow_mut().views = views);
}

/// **ROUND 88** — the names of the structs a `str` may take its methods
/// from: every registered view (`{ *mut u8, usize }` — that is `str.Span`),
/// in the order in which they were declared, so that the choice is settled
/// and does not depend on a hash table.
///
/// WHY THIS EXISTS. Up to round 87 `a.length()` and `a.starts_with("te")`
/// worked on a `str` and `a.part(0, 4)` did not — and the reason was an
/// ACCIDENT: the builtin type is called `str`, the module of the string
/// library is called `str` too, so `a.length()` found the FREE function
/// `str.length(s: Span)` under the very name (`str__length`) that the
/// method resolution builds. Everything for which the module happens to
/// have a free function of the same name looked like a method; `part` has
/// none (it is called `span_part` there) and therefore did not exist.
/// SPEC 8.1 promises the whole library on a `str`, so the resolution now
/// really asks `impl Span` as well.
pub(crate) fn view_names(tcx: &TypeCtx) -> Vec<String> {
    REG.with(|r| {
        r.borrow()
            .views
            .iter()
            .filter_map(|i| tcx.structs.get(*i).map(|s| s.name.clone()))
            .collect()
    })
}

/// May these two types be used for each other?
///
/// True exactly when ONE of them is the builtin `str` and the other is a
/// view of the same shape (`str.Span`). Two ordinary structs of that shape
/// stay separate from each other — only `str` gets this privilege, so that
/// the language type can reach the library without a conversion function.
pub(crate) fn same_view(a: &Type, b: &Type) -> bool {
    let (x, y) = match (a, b) {
        (Type::Struct(x), Type::Struct(y)) => (*x, *y),
        _ => return false,
    };
    if x == y {
        return false;
    }
    REG.with(|r| {
        let reg = r.borrow();
        let s = match reg.idx {
            Some(s) => s,
            None => return false,
        };
        (x == s && reg.views.contains(&y)) || (y == s && reg.views.contains(&x))
    })
}

// ---------------------------------------------------------------- the trigger

/// Does this token stream work with `str`? (`modules.rs::gc_runtime`)
///
/// Two signals, both purely syntactic:
///
///  1. the identifier `str` — since round 88 `string` too — NOT next to a `.` — that is the TYPE name
///     (`let s: str`, `-> str`, `str { … }`). `import std.str` and
///     `str.trim(x)` are excluded by the dot.
///  2. a text literal directly next to `+`, `==` or `!=` — the two
///     operators of this round that need the runtime.
///
/// What deliberately does NOT trigger: `var t: [u8; 20] = "…"` (the
/// literal as an array, the form the whole existing source text uses) and
/// `asm("…")`. That is why the kernel pulls in no collector.
pub(crate) fn source_uses_str(toks: &[crate::lexer::Token]) -> bool {
    for (i, t) in toks.iter().enumerate() {
        if let TokKind::Ident(n) = &t.kind {
            if n == NAME || n == NAME_ALIAS {
                let before_dot = i > 0 && matches!(toks[i - 1].kind, TokKind::Dot);
                let after_dot = matches!(toks.get(i + 1).map(|x| &x.kind), Some(TokKind::Dot));
                if !before_dot && !after_dot {
                    return true;
                }
            }
        }
        if matches!(t.kind, TokKind::Str(..)) {
            let touches = |k: Option<&TokKind>| {
                matches!(k, Some(TokKind::Plus) | Some(TokKind::EqEq) | Some(TokKind::NotEq))
            };
            if i > 0 && touches(Some(&toks[i - 1].kind)) {
                return true;
            }
            if touches(toks.get(i + 1).map(|x| &x.kind)) {
                return true;
            }
        }
    }
    false
}

// ------------------------------------------------------------- the type check

/// Number of elements of the array literal behind a text literal.
pub(crate) fn literal_len(inner: &Expr) -> u64 {
    match &inner.kind {
        ExprKind::ArrayLit(xs) => xs.len() as u64,
        _ => 0,
    }
}

/// `// HOOK str` in `sema::expr_inner` — a text literal.
///
/// THE RULE, and it is the whole compatibility story of this round:
/// **the context decides**. Where an array type is wanted, the literal is
/// the array literal it has always been — with the same messages and the
/// same length check. Everywhere else it is a `str`.
///
/// Nothing can break through that: today a text literal WITHOUT an array
/// context is an error ("the type of the array literal cannot be inferred"),
/// so there is no program whose meaning changes.
pub(crate) fn check_text(
    ck: &mut Checker,
    e: &Expr,
    wide: bool,
    inner: &Expr,
    hint: Option<&Type>,
) -> Type {
    let wants_array = matches!(hint, Some(Type::Array(..)));
    if wants_array {
        if literal_len(inner) == 0 {
            ck.dg.error_note(
                e.span,
                "empty string literal".to_string(),
                "an array needs at least one element; write an array of the desired length, e.g. '[0 as u8; 8]'",
            );
            return Type::Error;
        }
        return ck.expr(inner, hint);
    }
    if wide {
        ck.dg.error_note(
            e.span,
            "a u\"…\" literal has no str type".to_string(),
            "str holds octets; give the array type, e.g. 'var t: [u16; 4] = u\"code\"'",
        );
        return Type::Error;
    }
    let n = literal_len(inner);
    if n > 0 {
        // The octets keep their array type — the lowering writes them into
        // the frame with exactly the code an array literal has always got.
        let at = Type::Array(Box::new(Type::U8), n);
        ck.expr(inner, Some(&at));
    }
    let t = ty();
    if t.is_error() {
        ck.dg.error(e.span, "the type 'str' is not available here");
    }
    t
}

/// `// HOOK str` in `sema::binary` — `==`, `!=` and `+` on `str`.
///
/// Returns `None` when neither side is a `str`; then the ordinary rules
/// apply unchanged.
pub(crate) fn hook_binary(
    ck: &mut Checker,
    op: BinOp,
    lt: &Type,
    rt: &Type,
    span: Span,
) -> Option<Type> {
    if !is_str_like(lt) && !is_str_like(rt) {
        return None;
    }
    match op {
        BinOp::Eq | BinOp::Ne => {
            need(ck, FN_EQ, op.text(), span);
            Some(Type::Bool)
        }
        BinOp::Add => {
            need(ck, FN_CONCAT, op.text(), span);
            Some(ty())
        }
        _ => {
            ck.dg.error_note(
                span,
                format!("operator '{}' is not defined for the type str", op.text()),
                "str knows '==', '!=' and '+'; for everything else use the functions of std.str",
            );
            Some(Type::Error)
        }
    }
}

/// The runtime function has to be part of the program. It comes with the
/// collector; whoever gets here without it gets a message that says what to
/// do about it.
fn need(ck: &mut Checker, fname: &str, op: &str, span: Span) {
    if ck.fns.contains_key(fname) {
        return;
    }
    ck.dg.error_note(
        span,
        format!("the operator '{}' on str needs the collector runtime", op),
        "the compiler reads that off the tokens; write the type down once, e.g. 'let s: str = …', then it pulls the runtime in",
    );
}
