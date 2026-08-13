//! Lowering AST -> FIR.
//!
//! SCHNITTSTELLE (fest):
//!   `pub fn lower(prog: &ast::Program, info: &sema::TypeInfo, dg: &mut Diags)
//!        -> Option<fir::Module>`
//! Zusicherung an das Backend: jeder Block hat einen echten Terminator
//! (kein `Term::Unset`), alle `alloca` stehen im Eintrittsblock.

use crate::ast::Program;
use crate::diag::{Diags, Span};
use crate::fir::Module;
use crate::sema::TypeInfo;

/// STUB — wird von Modul "fir-lowering" implementiert.
pub fn lower(prog: &Program, info: &TypeInfo, dg: &mut Diags) -> Option<Module> {
    let _ = (prog, info);
    dg.error(Span::none(), "Lowering ist in diesem Baustand noch nicht implementiert");
    None
}
