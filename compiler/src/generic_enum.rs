// SPDX-License-Identifier: MPL-2.0
use std::cell::RefCell;

use crate::ast::{ExprId, TypeExpr};
use crate::diag::Span;
use crate::fx::HashMap;
use crate::sema_generic::{Instantiation, TyParam};
use crate::sema_match::EnumDef;
use crate::types::Type;

#[derive(Clone, Debug)]
pub(crate) struct EnumTemplate {
    pub(crate) params: Vec<TyParam>,
    pub(crate) def: EnumDef,
}

thread_local! {
    static TEMPLATES: RefCell<HashMap<String, EnumTemplate>> = RefCell::new(HashMap::default());
    static HINT: RefCell<Option<Type>> = const { RefCell::new(None) };
    static FALLBACKS: RefCell<HashMap<ExprId, String>> = RefCell::new(HashMap::default());
    static SITE: RefCell<Option<ExprId>> = const { RefCell::new(None) };
}

pub(crate) fn reset() {
    TEMPLATES.with(|t| t.borrow_mut().clear());
    HINT.with(|h| *h.borrow_mut() = None);
    FALLBACKS.with(|f| f.borrow_mut().clear());
    SITE.with(|s| *s.borrow_mut() = None);
}

pub(crate) fn template(name: &str) -> Option<EnumTemplate> {
    TEMPLATES.with(|t| t.borrow().get(name).cloned())
}

pub(crate) fn add_template(name: &str, tpl: EnumTemplate) -> bool {
    TEMPLATES.with(|t| {
        let mut map = t.borrow_mut();
        if map.contains_key(name) {
            return false;
        }
        map.insert(name.to_string(), tpl);
        true
    })
}

pub(crate) fn set_hint(hint: Option<&Type>) {
    HINT.with(|h| *h.borrow_mut() = hint.cloned());
}

pub(crate) fn take_hint() -> Option<Type> {
    HINT.with(|h| h.borrow_mut().take())
}

pub(crate) fn set_site(id: ExprId) {
    SITE.with(|s| *s.borrow_mut() = Some(id));
}

pub(crate) fn take_fallback() -> Option<String> {
    let id = SITE.with(|s| s.borrow_mut().take())?;
    FALLBACKS.with(|f| f.borrow().get(&id).cloned())
}

pub(crate) fn infer_ctor(
    id: ExprId,
    at: Span,
    name: &str,
    strong: &[Option<TypeExpr>],
    weak: &[Option<TypeExpr>],
) -> bool {
    let Some((ename, vname)) = name.split_once("::") else { return false };
    if crate::sema_match::enum_by_name(ename).is_some() {
        return false;
    }
    let Some(tpl) = template(ename) else { return false };
    let Some(variant) = tpl.def.variant(vname) else { return false };
    if variant.field_tys.len() != strong.len() {
        return false;
    }
    let decls: Vec<&TypeExpr> = variant.field_tys.iter().collect();
    let map = crate::sema_generic::solve_params(&tpl.params, &decls, strong, weak);
    let Some(args) = crate::sema_generic::collect_args(&tpl.params, &map) else { return false };
    let mangled = crate::sema_generic::mangle(ename, &args);
    FALLBACKS.with(|f| f.borrow_mut().insert(id, mangled.clone()));
    if crate::sema_match::enum_by_name(&mangled).is_some() {
        return false;
    }
    crate::sema_generic::record_inst(
        &mangled,
        Instantiation { base: ename.to_string(), args, span: at, is_abstract: false, is_fn: false },
    );
    true
}
