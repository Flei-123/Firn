// SPDX-License-Identifier: MPL-2.0
use crate::ast::{Expr, ExprKind};
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
