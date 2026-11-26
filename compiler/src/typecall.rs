// SPDX-License-Identifier: MPL-2.0
use crate::ast::{Expr, ExprKind};
use crate::diag::Span;
use crate::fx::HashMap;
use crate::impls;
use crate::sema::FnSig;

pub(crate) fn target(
    fns: &HashMap<String, FnSig>,
    name: &str,
    args: &[Expr],
    is_value: impl Fn(&str) -> bool,
) -> Option<String> {
    let method = impls::method_name(name)?;
    let ExprKind::Ident(ty) = &args.first()?.kind else { return None };
    if is_value(ty) {
        return None;
    }
    let full = impls::fn_name(crate::types::canon_name(ty), method);
    fns.contains_key(&full).then_some(full)
}

const PRIMITIVES: [&str; 13] = ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "isize", "usize", "f32", "f64", "bool"];

pub(crate) struct Missing {
    pub(crate) span: Span,
    pub(crate) message: String,
    pub(crate) help: Option<String>,
}

pub(crate) fn missing(
    fns: &HashMap<String, FnSig>,
    name: &str,
    args: &[Expr],
    is_value: impl Fn(&str) -> bool,
    is_type: impl Fn(&str) -> bool,
) -> Option<Missing> {
    let method = impls::method_name(name)?;
    let first = args.first()?;
    let ExprKind::Ident(ty) = &first.kind else { return None };
    let canon = crate::types::canon_name(ty);
    if is_value(ty) || !(PRIMITIVES.contains(&canon) || is_type(canon)) {
        return None;
    }
    let prefix = impls::fn_name(canon, "");
    let own = fns.keys().filter_map(|k| k.strip_prefix(prefix.as_str()));
    Some(Missing {
        span: first.span,
        message: format!("type '{}' has no function '{}'", ty, method),
        help: crate::diag::nearest(method, own).map(|n| crate::diag::did_you_mean(&n)),
    })
}
