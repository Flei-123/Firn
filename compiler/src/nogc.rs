//! `#[no_gc]` — die Garantie fuer heisse Pfade, SPEC §3.5.4.
//!
//! In einer `#[no_gc]`-Funktion sind verboten:
//!  1. GC-Allokation,
//!  2. Aufruf einer Funktion **ohne** `#[no_gc]`,
//!  3. Schreiben in ein `Gc[T]`-Feld (Einfuegebarriere).
//!
//! Die Pruefung ist **transitiv**: weil jede aufgerufene Funktion selbst
//! `#[no_gc]` tragen muss, gilt die Zusage fuer den ganzen Aufrufbaum. Bricht
//! die Kette, gibt es einen Fehler mit Zeile und Spalte.
//!
//! Angebunden ueber die Zeile `// HOOK nogc` in `sema::Checker::run`. Die
//! GC-spezifischen Abfragen (1 und 3) kommen aus `gc.rs` — diese Datei kennt
//! den GC nur ueber diese drei Funktionen und ist damit unabhaengig davon
//! baubar.
//!
//! Diese Datei gehoert dem Modul `nogc` (PLAN.md, Runde „Haertetest 2").

use std::collections::HashMap;

use crate::ast::{Block, Expr, ExprKind, FnDecl, Program, Stmt};
use crate::diag::Span;
use crate::sema::Checker;

/// Traegt die Funktion `#[no_gc]`?
pub(crate) fn hat_no_gc(f: &FnDecl) -> bool {
    f.attrs.iter().any(|a| a.name == "no_gc")
}

/// `// HOOK nogc` in `sema::Checker::run`: prueft alle `#[no_gc]`-Funktionen.
pub(crate) fn hook_check(ck: &mut Checker, prog: &Program) {
    let mut markiert: HashMap<&str, bool> = HashMap::new();
    for f in &prog.funcs {
        markiert.insert(f.name.as_str(), hat_no_gc(f));
    }
    if !markiert.values().any(|v| *v) {
        return;
    }
    let mut befunde: Vec<(Span, String)> = Vec::new();
    for f in &prog.funcs {
        if !hat_no_gc(f) {
            continue;
        }
        pruefe_block(ck, &f.body, &f.name, &markiert, &mut befunde);
    }
    for (span, msg) in befunde {
        ck.dg.error(span, msg);
    }
}

fn pruefe_block(
    ck: &Checker,
    b: &Block,
    wer: &str,
    markiert: &HashMap<&str, bool>,
    out: &mut Vec<(Span, String)>,
) {
    for s in &b.stmts {
        pruefe_stmt(ck, s, wer, markiert, out);
    }
}

fn pruefe_stmt(
    ck: &Checker,
    s: &Stmt,
    wer: &str,
    markiert: &HashMap<&str, bool>,
    out: &mut Vec<(Span, String)>,
) {
    match s {
        Stmt::Let { init, .. } => pruefe_expr(ck, init, wer, markiert, out),
        Stmt::Assign { target, value, .. } => {
            // Regel 3: Schreiben in ein Gc[T]-Feld.
            if let ExprKind::Field(_, name, sp) = &target.kind {
                let ty = ck
                    .expr_types
                    .get(target.id as usize)
                    .cloned()
                    .unwrap_or(crate::types::Type::Error);
                if crate::gc::ist_gc_zeiger(&ty) {
                    out.push((
                        *sp,
                        format!(
                            "'{wer}' ist #[no_gc], schreibt aber in das GC-Feld '{name}' \
                             (SPEC 3.5.4: keine Einfuegebarriere in einem #[no_gc]-Aufrufbaum)"
                        ),
                    ));
                }
            }
            pruefe_expr(ck, target, wer, markiert, out);
            pruefe_expr(ck, value, wer, markiert, out);
        }
        Stmt::If { cond, then, els, .. } => {
            pruefe_expr(ck, cond, wer, markiert, out);
            pruefe_block(ck, then, wer, markiert, out);
            if let Some(e) = els {
                pruefe_stmt(ck, e, wer, markiert, out);
            }
        }
        Stmt::While { cond, body, .. } => {
            pruefe_expr(ck, cond, wer, markiert, out);
            pruefe_block(ck, body, wer, markiert, out);
        }
        Stmt::For { start, end, body, .. } => {
            pruefe_expr(ck, start, wer, markiert, out);
            pruefe_expr(ck, end, wer, markiert, out);
            pruefe_block(ck, body, wer, markiert, out);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                pruefe_expr(ck, v, wer, markiert, out);
            }
        }
        Stmt::Expr(e) => pruefe_expr(ck, e, wer, markiert, out),
        Stmt::Block(b) => pruefe_block(ck, b, wer, markiert, out),
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
    }
}

fn pruefe_expr(
    ck: &Checker,
    e: &Expr,
    wer: &str,
    markiert: &HashMap<&str, bool>,
    out: &mut Vec<(Span, String)>,
) {
    match &e.kind {
        ExprKind::Call(name, args, sp) => {
            if crate::gc::ist_gc_alloc_aufruf(name) {
                // Regel 1: GC-Allokation (kann einen Sammellauf ausloesen).
                out.push((
                    *sp,
                    format!(
                        "'{wer}' ist #[no_gc], alloziert aber ueber '{name}' auf dem GC-Heap \
                         (SPEC 3.5.4)"
                    ),
                ));
            } else if let Some(false) = markiert.get(name.as_str()) {
                // Regel 2: Aufruf ohne #[no_gc] — bricht die Kette transitiv.
                out.push((
                    *sp,
                    format!(
                        "'{wer}' ist #[no_gc], ruft aber '{name}' ohne #[no_gc] \
                         (SPEC 3.5.4: die Zusage gilt transitiv fuer den ganzen Aufrufbaum)"
                    ),
                ));
            }
            for a in args {
                pruefe_expr(ck, a, wer, markiert, out);
            }
        }
        ExprKind::Unary(_, a) => pruefe_expr(ck, a, wer, markiert, out),
        ExprKind::Binary(_, a, b) => {
            pruefe_expr(ck, a, wer, markiert, out);
            pruefe_expr(ck, b, wer, markiert, out);
        }
        ExprKind::Field(b, _, _) => pruefe_expr(ck, b, wer, markiert, out),
        ExprKind::Index(b, i) => {
            pruefe_expr(ck, b, wer, markiert, out);
            pruefe_expr(ck, i, wer, markiert, out);
        }
        ExprKind::Syscall(args) | ExprKind::ArrayLit(args) => {
            for a in args {
                pruefe_expr(ck, a, wer, markiert, out);
            }
        }
        ExprKind::Cast(a, _) => pruefe_expr(ck, a, wer, markiert, out),
        ExprKind::StructLit(_, felder, _) => {
            for (_, v, _) in felder {
                pruefe_expr(ck, v, wer, markiert, out);
            }
        }
        ExprKind::ArrayRepeat(v, n) => {
            pruefe_expr(ck, v, wer, markiert, out);
            pruefe_expr(ck, n, wer, markiert, out);
        }
        ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Ident(_) => {}
    }
}
