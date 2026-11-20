// SPDX-License-Identifier: MPL-2.0
use std::cell::RefCell;

use crate::fx::HashMap;
use crate::sema_generic::TyParam;
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
}

pub(crate) fn reset() {
    TEMPLATES.with(|t| t.borrow_mut().clear());
    HINT.with(|h| *h.borrow_mut() = None);
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
