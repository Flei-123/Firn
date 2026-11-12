// SPDX-License-Identifier: MPL-2.0
use crate::ast::{FnDecl, Program, TypeExpr};
use crate::fx::HashSet;
use crate::iface::{self, SELF_TYPE};
use crate::impls;
use crate::mono;
use crate::parser::Parser;

pub(crate) fn add_defaults(
    p: &mut Parser,
    prog: &mut Program,
    iface_name: &str,
    ty: &str,
    defined: &HashSet<String>,
) {
    let self_name = if crate::gc::is_class(ty) {
        format!("gc {}", ty)
    } else {
        ty.to_string()
    };
    for (method, template) in iface::default_methods(iface_name) {
        let name = impls::fn_name(ty, &method);
        if defined.contains(&name) {
            continue;
        }
        prog.funcs.push(instantiate(p, &template, &name, &self_name));
    }
}

fn instantiate(p: &mut Parser, template: &FnDecl, name: &str, self_name: &str) -> FnDecl {
    let mut decl = template.clone();
    decl.name = name.to_string();
    for param in decl.params.iter_mut() {
        param.ty = replace_self(&param.ty, self_name);
    }
    if let Some(ret) = decl.ret.as_mut() {
        *ret = replace_self(ret, self_name);
    }
    mono::renumber_block(&mut decl.body, &mut p.next_id);
    decl
}

fn replace_self(te: &TypeExpr, self_name: &str) -> TypeExpr {
    match te {
        TypeExpr::Named(n, sp) if n == SELF_TYPE => TypeExpr::Named(self_name.to_string(), *sp),
        TypeExpr::Named(..) => te.clone(),
        TypeExpr::Ptr { mutable, inner, span } => TypeExpr::Ptr {
            mutable: *mutable,
            inner: Box::new(replace_self(inner, self_name)),
            span: *span,
        },
        TypeExpr::Array { elem, len, span } => TypeExpr::Array {
            elem: Box::new(replace_self(elem, self_name)),
            len: len.clone(),
            span: *span,
        },
        TypeExpr::Fn { params, ret, span } => TypeExpr::Fn {
            params: params.iter().map(|t| replace_self(t, self_name)).collect(),
            ret: ret.as_ref().map(|t| Box::new(replace_self(t, self_name))),
            span: *span,
        },
        TypeExpr::Tuple(elems, sp) => {
            TypeExpr::Tuple(elems.iter().map(|t| replace_self(t, self_name)).collect(), *sp)
        }
    }
}
