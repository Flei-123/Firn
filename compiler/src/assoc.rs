use std::cell::RefCell;
use crate::fx::HashMap;

use crate::ast::TypeExpr;
use crate::diag::Span;
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::types::Type;

pub(crate) const PATH_SEP: &str = "::";
const SELF_PREFIX: &str = "Self::";

struct ImplItem {
    iface: String,
    ty: String,
    name: String,
    value: TypeExpr,
    span: Span,
}

#[derive(Default)]
struct Registry {
    impl_ty: Option<String>,
    check_ty: Option<String>,
    declared: HashMap<String, Vec<String>>,
    items: Vec<ImplItem>,
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
}

pub(crate) fn reset() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
}

pub(crate) fn split(name: &str) -> Option<(&str, &str)> {
    name.split_once(PATH_SEP)
}

pub(crate) fn is_self_path(name: &str) -> bool {
    name.starts_with(SELF_PREFIX)
}

pub(crate) fn set_impl_ty(ty: Option<&str>) {
    REG.with(|r| r.borrow_mut().impl_ty = ty.map(|t| t.to_string()));
}

pub(crate) fn set_check_ty(ty: &str) {
    REG.with(|r| r.borrow_mut().check_ty = Some(ty.to_string()));
}

pub(crate) fn hook_type(p: &mut Parser, name: &str, sp: Span) -> Option<TypeExpr> {
    let next = |i: usize| p.toks.get(p.pos + i).map(|t| t.kind.clone());
    if next(0) != Some(TokKind::Colon) || next(1) != Some(TokKind::Colon) {
        return None;
    }
    let Some(TokKind::Ident(assoc)) = next(2) else {
        return None;
    };
    p.bump();
    p.bump();
    let end = p.bump();
    let base = match (name, REG.with(|r| r.borrow().impl_ty.clone())) {
        ("Self", Some(ty)) => ty,
        _ => name.to_string(),
    };
    Some(TypeExpr::Named(format!("{}{}{}", base, PATH_SEP, assoc), Parser::join(sp, end)))
}

pub(crate) fn at_type_item(p: &Parser) -> bool {
    matches!(p.kind(), TokKind::Ident(w) if w == "type")
        && matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::Ident(_)))
}

pub(crate) fn interface_item(p: &mut Parser, iface: &str, names: &mut Vec<String>) -> bool {
    p.bump();
    let Some((name, sp)) = p.ident("after 'type' in an interface") else {
        return false;
    };
    if names.contains(&name) {
        p.dg.error(sp, format!("the interface '{}' already has the associated type '{}'", iface, name));
    }
    names.push(name);
    p.eat(&TokKind::Semi);
    true
}

pub(crate) fn declare(iface: &str, names: Vec<String>) {
    REG.with(|r| {
        r.borrow_mut().declared.insert(iface.to_string(), names);
    });
}

pub(crate) fn impl_item(p: &mut Parser, iface: &str, ty: &str, is_for: bool) -> bool {
    let start = p.bump();
    let Some((name, sp)) = p.ident("after 'type' in an impl block") else {
        return false;
    };
    if !is_for {
        p.dg.error_note(
            Parser::join(start, sp),
            "an associated type belongs in 'impl Interface for Type'",
            "an inherent impl block has no interface to declare it",
        );
    }
    if !p.expect(TokKind::Assign, "after the name of the associated type") {
        return false;
    }
    let Some(value) = p.parse_type() else {
        return false;
    };
    p.eat(&TokKind::Semi);
    REG.with(|r| {
        r.borrow_mut().items.push(ImplItem {
            iface: iface.to_string(),
            ty: ty.to_string(),
            name,
            value,
            span: sp,
        })
    });
    true
}

pub(crate) fn lookup(ty: &str, name: &str) -> Option<TypeExpr> {
    REG.with(|r| {
        r.borrow()
            .items
            .iter()
            .find(|i| i.ty == ty && i.name == name)
            .map(|i| i.value.clone())
    })
}

pub(crate) fn subst_path(n: &str, sp: Span, map: &HashMap<String, TypeExpr>) -> Option<TypeExpr> {
    let (base, assoc) = split(n)?;
    let Some(TypeExpr::Named(concrete, _)) = map.get(base) else {
        return None;
    };
    let concrete = crate::types::canon_name(concrete);
    match lookup(concrete, assoc) {
        Some(t) => Some(t),
        None => Some(TypeExpr::Named(format!("{}{}{}", concrete, PATH_SEP, assoc), sp)),
    }
}

pub(crate) fn resolve_self_path(ck: &mut Checker, n: &str, sp: Span) -> Type {
    let ty = REG.with(|r| r.borrow().check_ty.clone()).unwrap_or_default();
    let assoc = &n[SELF_PREFIX.len()..];
    resolve_entry(ck, &ty, assoc, sp)
}

pub(crate) fn hook_resolve_ty(ck: &mut Checker, n: &str, sp: Span) -> Option<Type> {
    let (base, assoc) = split(n)?;
    Some(resolve_entry(ck, base, assoc, sp))
}

fn resolve_entry(ck: &mut Checker, ty: &str, assoc: &str, sp: Span) -> Type {
    match lookup(ty, assoc) {
        Some(te) => ck.resolve_ty(&te),
        None => {
            ck.dg.error_note(
                sp,
                format!("the type '{}' has no associated type '{}'", ty, assoc),
                format!("declare it in 'impl Interface for {}' with 'type {} = ..;'", ty, assoc),
            );
            Type::Error
        }
    }
}

pub(crate) fn check_impl_items(ck: &mut Checker, iface: &str, ty: &str, span: Span) {
    let declared = REG.with(|r| r.borrow().declared.get(iface).cloned().unwrap_or_default());
    for name in &declared {
        let present = REG.with(|r| {
            r.borrow().items.iter().any(|i| i.iface == iface && i.ty == ty && &i.name == name)
        });
        if !present {
            ck.dg.error_note(
                span,
                format!("'{}' does not define the associated type '{}.{}'", ty, iface, name),
                format!("'type {} = ..;' is expected in the block", name),
            );
        }
    }
    let extra: Vec<(String, Span)> = REG.with(|r| {
        r.borrow()
            .items
            .iter()
            .filter(|i| i.iface == iface && i.ty == ty && !declared.contains(&i.name))
            .map(|i| (i.name.clone(), i.span))
            .collect()
    });
    for (name, sp) in extra {
        ck.dg.error(sp, format!("the interface '{}' has no associated type '{}'", iface, name));
    }
}

pub(crate) fn replace_self(name: &str, self_name: &str) -> Option<String> {
    name.strip_prefix(SELF_PREFIX)
        .map(|assoc| format!("{}{}{}", self_name, PATH_SEP, assoc))
}
