// SPDX-License-Identifier: MPL-2.0
use crate::fx::HashMap;

use crate::ast::TypeExpr;
use crate::mono::{renumber_block, renumber_expr, subst_block, subst_expr};
use crate::sema_generic::Instantiation;
use crate::sema_match::{match_info, push_fresh_match, put_match, take_fresh, take_match, MATCH_PREFIX};

pub(crate) fn clone_substituted(
    idx: usize,
    map: &HashMap<String, TypeExpr>,
    queue: &mut Vec<(String, Instantiation)>,
) -> String {
    let mut info = match match_info(idx) {
        Some(info) => info,
        None => return format!("{}{}", MATCH_PREFIX, idx),
    };
    subst_expr(&mut info.subject, map, queue);
    for arm in info.arms.iter_mut() {
        subst_block(&mut arm.body, map, queue);
    }
    format!("{}{}", MATCH_PREFIX, push_fresh_match(info))
}

pub(crate) fn renumber_fresh(idx: usize, next: &mut u32) {
    if !take_fresh(idx) {
        return;
    }
    let mut info = match take_match(idx) {
        Some(info) => info,
        None => return,
    };
    renumber_expr(&mut info.subject, next);
    for arm in info.arms.iter_mut() {
        renumber_block(&mut arm.body, next);
    }
    put_match(idx, info);
}
