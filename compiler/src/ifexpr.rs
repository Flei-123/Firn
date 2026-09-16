// SPDX-License-Identifier: MPL-2.0
//! **`if` AS AN EXPRESSION** -- `let x = if c { a } else { b }`.
//!
//! INTERFACE (fixed):
//!   `pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr>`   (parser)
//!
//! ## Why
//!
//! `SPEC.md` T1 records why this was missing: *"the result value needs a
//! merge of values (phi) in the lowering, which stage 0 does not have."*
//! That was a statement about the state back then, not a decision about the
//! language. Since round 92 real phi nodes exist (`fir.rs`), and the
//! lowering does not even need them directly here: like everywhere else it
//! makes an `alloca` place, writes into it in both branches and reads
//! afterwards. `mem2reg` turns that into a phi by itself.
//!
//! Set apart from this, `++`/`--` stays a pure statement. That is a real
//! design decision with a reason (SPEC 12.7) and not an open building site.
//!
//! ## How it looks
//!
//! ```firn
//! let bigger: i32 = if a > b { a } else { b }
//! let s: i32 = if n < 0 { -1 } else { if n > 0 { 1 } else { 0 } }
//! let t: i32 = if n < 0 { -1 } else if n > 0 { 1 } else { 0 }
//! ```
//!
//! ## What deliberately does NOT work
//!
//! * **`else` is mandatory.** Without it the expression would have no value
//!   in one of the two cases. That is not an oversight but the difference
//!   from the statement.
//! * **Exactly ONE expression per branch**, not a block with statements.
//!   That keeps the stance of the language that no control flow with side
//!   effects hides inside an expression. Whoever needs statements keeps
//!   using the `if` statement.
//! * Both branches have to produce the same type (checked in `sema.rs`).

use crate::ast::{Expr, ExprKind};
use crate::lexer::TokKind;
use crate::parser::Parser;

/// Hooks into `Parser::primary`. It only takes effect when an `if` really
/// stands in expression position here -- the `if` statement is caught in
/// `stmt()` before and never arrives here.
pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr> {
    if !p.at(&TokKind::KwIf) {
        return None;
    }
    Some(parse_if(p))
}

fn parse_if(p: &mut Parser) -> Expr {
    let start = p.bump(); // 'if'
    let cond = p.cond_expr();

    if !p.expect(TokKind::LBrace, "after the condition of an 'if' expression") {
        return p.broken_expr(start);
    }
    let then_v = p.expr();
    if !p.expect(TokKind::RBrace, "after the value of the 'if' branch") {
        return p.broken_expr(start);
    }

    if !p.at(&TokKind::KwElse) {
        p.dg.error_note(
            start,
            "an 'if' expression needs an 'else'".to_string(),
            "without 'else' there would be no value in one of the two cases; \
             as a statement 'if' works without it"
                .to_string(),
        );
        return p.broken_expr(start);
    }
    p.bump(); // 'else'

    // `else if ...` without braces, like the statement.
    let else_v = if p.at(&TokKind::KwIf) {
        parse_if(p)
    } else {
        if !p.expect(TokKind::LBrace, "after 'else' in an 'if' expression") {
            return p.broken_expr(start);
        }
        let e = p.expr();
        if !p.expect(TokKind::RBrace, "after the value of the 'else' branch") {
            return p.broken_expr(start);
        }
        e
    };

    let span = Parser::join(start, else_v.span);
    p.mk(span, ExprKind::IfElse(Box::new(cond), Box::new(then_v), Box::new(else_v)))
}
