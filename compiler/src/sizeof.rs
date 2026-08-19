//! **`size_of[T]()`** — die Größe eines Typs in Bytes, zur Übersetzungszeit.
//!
//! SCHNITTSTELLE (fest):
//!   `pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr>`  (Parser)
//!   `pub(crate) fn hook_call(ck, name, args, span) -> Option<Type>` (Typprüfer)
//!   `pub(crate) fn wert(name: &str) -> Option<i128>`               (Lowering)
//!
//! ## Wozu
//!
//! `docs/SELBSTHOSTING.md` §4 führt `Vec[T]` als zweitgrößten Blocker auf dem
//! Weg zu Stufe 1. Ein Feld **fester** Größe geht seit Runde 2
//! (`tests/211_generic_struct.fi`), ein **wachsendes** nicht: dafür muss die
//! Adresse des `i`-ten Elements ausgerechnet werden, und das braucht die
//! Elementgröße.
//!
//! ## Wie es aussieht
//!
//! ```firn
//! let n: usize = size_of[i32]()      // 4
//! let m: usize = size_of[Punkt]()    // Layout des Structs
//! ```
//!
//! ## Wie es gebaut ist
//!
//! Wie `gc_null[C]()` (siehe `gc.rs`): der Parser erkennt die Form direkt und
//! verpackt sie als **Aufruf mit einem reservierten Namen**, in dem der
//! Typtext steckt. Der Typprüfer löst den Typ auf, rechnet die Größe aus und
//! merkt sie sich; das Lowering setzt eine Konstante ein. Zur Laufzeit bleibt
//! **nichts** davon übrig.
//!
//! Der Weg über den Namen ist Absicht: `size_of` ist damit kein Schlüsselwort
//! und kollidiert mit keinem Bezeichner, den jemand schon benutzt.

use crate::ast::{Expr, ExprKind, TypeExpr};
use crate::diag::Span;
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::types::Type;
use std::cell::RefCell;
use std::collections::HashMap;

/// Reservierter Namenspräfix. Firn-Bezeichner können `$` nicht enthalten,
/// deshalb ist eine Kollision mit Nutzercode ausgeschlossen.
const P_SIZE: &str = "size_of$";

thread_local! {
    /// Name -> Größe. Gefüllt vom Typprüfer, gelesen vom Lowering.
    static WERTE: RefCell<HashMap<String, i128>> = RefCell::new(HashMap::new());
}

/// Setzt die Tabelle zurück (eine je Übersetzung, `parser::reset_hooks`).
pub(crate) fn hook_reset() {
    WERTE.with(|w| w.borrow_mut().clear());
}

/// `// HOOK sizeof` in `parser.rs::primary` — `size_of[T]()`.
pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr> {
    match p.kind() {
        TokKind::Ident(n) if n == "size_of" => {}
        _ => return None,
    }
    if !matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::LBracket)) {
        return None;
    }
    let start = p.bump(); // 'size_of'
    p.bump(); // '['
    // BEWUSST NUR EIN TYPNAME, kein voller Typausdruck: `size_of[i32]`,
    // `size_of[Punkt]`. Wer die Groesse eines zusammengesetzten Typs braucht,
    // gibt ihm einen Namen — das ist ohnehin lesbarer als `size_of[*mut u8]`.
    let (ty_name, _) = p.ident("after 'size_of['")?;
    if !p.expect(TokKind::RBracket, "after the type argument of 'size_of'") {
        return None;
    }
    if !p.expect(TokKind::LParen, "after the type argument of 'size_of'") {
        return None;
    }
    let end = match p.kind() {
        TokKind::RParen => p.bump(),
        _ => {
            p.error_here("'size_of' takes no arguments".to_string());
            return None;
        }
    };
    let span = Parser::join(start, end);
    // Der Typtext wandert in den Namen; aufgeloest wird er im Typpruefer,
    // der die Struct-Tabelle kennt.
    Some(p.mk(span, ExprKind::Call(format!("{}{}", P_SIZE, ty_name), Vec::new(), start)))
}

/// `// HOOK sizeof` in `sema::call`.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    span: Span,
) -> Option<Type> {
    let ty_text = name.strip_prefix(P_SIZE)?;
    if !args.is_empty() {
        ck.dg.error(span, "'size_of' takes no arguments".to_string());
        return Some(Type::Error);
    }
    let te = TypeExpr::Named(ty_text.to_string(), span);
    let t = ck.resolve_ty(&te);
    if t.is_error() {
        return Some(Type::Error);
    }
    if matches!(t, Type::Void) {
        ck.dg.error(span, "'size_of[void]' is not meaningful".to_string());
        return Some(Type::Error);
    }
    let size = ck.tcx.size_of(&t) as i128;
    WERTE.with(|w| w.borrow_mut().insert(name.to_string(), size));
    Some(Type::Usize)
}

/// Die im Typprüfer ermittelte Größe — für das Lowering.
pub(crate) fn value(name: &str) -> Option<i128> {
    if !name.starts_with(P_SIZE) {
        return None;
    }
    WERTE.with(|w| w.borrow().get(name).copied())
}
