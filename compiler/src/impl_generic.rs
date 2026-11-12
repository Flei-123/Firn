// SPDX-License-Identifier: MPL-2.0
use crate::ast::{FnDecl, TypeExpr};
use crate::diag::Span;
use crate::impls;
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::sema_generic::{self, FnTemplate, TyParam};

pub(crate) fn hook_item(p: &mut Parser) -> bool {
    let name = match p.toks.get(p.pos + 1).map(|t| &t.kind) {
        Some(TokKind::Ident(n)) => n.clone(),
        _ => return false,
    };
    if !sema_generic::is_generic_struct(&name) {
        return false;
    }
    impl_decl(p, &name);
    true
}

fn impl_decl(p: &mut Parser, base: &str) {
    p.bump();
    if !p.pending_attrs.is_empty() {
        let sp = p.pending_attrs[0].span;
        p.dg.error_note(
            sp,
            "no attribute is allowed before 'impl'".to_string(),
            "write the attribute before the individual method".to_string(),
        );
        p.pending_attrs.clear();
    }
    let (_, tsp) = match p.ident("after 'impl'") {
        Some(x) => x,
        None => return abort(p),
    };
    let params = match p.generic_params() {
        Some(x) => x,
        None => return abort(p),
    };
    let args: Vec<TypeExpr> = params
        .iter()
        .map(|t| TypeExpr::Named(t.name.clone(), tsp))
        .collect();
    sema_generic::enter_template();
    let self_ty = p.note_inst(base, args, tsp, false);
    if !p.expect(TokKind::LBrace, "after the type name in 'impl'") {
        sema_generic::leave_template();
        return abort(p);
    }
    let ok = methods(p, base, &self_ty, tsp, &params);
    sema_generic::leave_template();
    if !ok {
        return abort(p);
    }
    p.close(TokKind::RBrace, "at the end of the impl block");
    p.recovering = false;
}

fn abort(p: &mut Parser) {
    p.recovering = false;
    p.sync_item();
}

fn methods(p: &mut Parser, base: &str, self_ty: &str, tsp: Span, params: &[TyParam]) -> bool {
    loop {
        while p.eat(&TokKind::Semi) {}
        if p.at(&TokKind::RBrace) || p.at_eof() || p.dg.is_full() {
            return true;
        }
        let before = p.pos;
        if !p.at(&TokKind::KwFn) {
            p.error_here(format!(
                "expected 'fn' in an impl block, found '{}'",
                p.kind().text()
            ));
            return false;
        }
        let Some(decl) = impls::method(p, base, self_ty, tsp, false) else {
            return false;
        };
        register(p, base, params, decl);
        if p.pos == before {
            p.bump();
        }
    }
}

fn register(p: &mut Parser, base: &str, params: &[TyParam], decl: FnDecl) {
    let prefix = format!("{}__", base);
    let method = decl.name[prefix.len()..].to_string();
    let key = decl.name.clone();
    let span = decl.span;
    let tpl = FnTemplate { params: params.to_vec(), decl };
    if !sema_generic::add_method_template(base, &method, key, tpl) {
        p.dg.error(
            span,
            format!("method '{}' of '{}' is already declared", method, base),
        );
    }
}
