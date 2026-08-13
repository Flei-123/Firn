//! Typpruefer (SPEC §12).
//!
//! SCHNITTSTELLE (fest):
//!   `pub fn check(prog: &ast::Program, dg: &mut Diags) -> Option<TypeInfo>`
//! Bei Erfolg gilt die ZUSICHERUNG: fuer JEDE vergebene `ExprId` steht in
//! `TypeInfo::expr_types` ein konkreter Typ (nie `Type::UntypedInt`, nie
//! `Type::Error`). Darauf verlaesst sich `lower.rs`.

use std::collections::HashMap;

use crate::ast::Program;
use crate::diag::{Diags, Span};
use crate::types::{Type, TypeCtx};

#[derive(Clone, Debug)]
pub struct FnSig {
    pub params: Vec<Type>,
    pub ret: Type,
}

#[derive(Clone, Debug, Default)]
pub struct TypeInfo {
    /// Struct-Tabelle inklusive berechnetem Layout.
    pub tcx: TypeCtx,
    /// Typ jedes Ausdrucks, indiziert mit `ExprId`.
    pub expr_types: Vec<Type>,
    /// Ausgewertete `const`-Deklarationen: Name -> (Typ, Wert).
    pub consts: HashMap<String, (Type, i128)>,
    /// Signaturen aller Funktionen.
    pub fns: HashMap<String, FnSig>,
}

impl TypeInfo {
    pub fn expr_ty(&self, id: crate::ast::ExprId) -> &Type {
        self.expr_types.get(id as usize).unwrap_or(&Type::Error)
    }
}

/// STUB — wird von Modul "sema" implementiert.
pub fn check(prog: &Program, dg: &mut Diags) -> Option<TypeInfo> {
    let _ = prog;
    dg.error(Span::none(), "Typpruefer ist in diesem Baustand noch nicht implementiert");
    None
}
