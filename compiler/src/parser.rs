//! Handgeschriebener, rekursiv absteigender Parser (kein Generator).
//!
//! SCHNITTSTELLE (fest):
//!   `pub fn parse(toks: &[Token], dg: &mut Diags) -> ast::Program`
//! Der Parser vergibt fortlaufende `ExprId`s ab 0 und setzt
//! `Program::expr_count`. Er MUSS sich nach einem Fehler auf Anweisungs-
//! bzw. Item-Ebene erholen und weitere Fehler melden.

use crate::ast::Program;
use crate::diag::{Diags, Span};
use crate::lexer::Token;

/// STUB — wird von Modul "frontend" implementiert.
pub fn parse(toks: &[Token], dg: &mut Diags) -> Program {
    let _ = toks;
    dg.error(Span::none(), "Parser ist in diesem Baustand noch nicht implementiert");
    Program::default()
}
