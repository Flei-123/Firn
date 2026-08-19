//! **LICM** — schleifeninvariante Berechnungen aus der Schleife ziehen.
//!
//! SCHNITTSTELLE (fest):
//!   `pub(crate) fn hoist_loop_invariants(f: &mut Func) -> usize`
//!
//! ## Warum dieser Durchgang
//!
//! Gemessen an `bench/firn/matmul.fi` (Faktor 6,26× gegen Rust, der schlechteste
//! Wert der Suite): die innerste Schleife rechnet
//!
//! ```text
//! s = s + ld32(a, r * n + k) * ld32(b, k * n + cc)
//! ```
//!
//! und `r * n` hängt von keiner Schleifenvariablen ab. Vor diesem Durchgang
//! stand im erzeugten Code je Iteration ein `imul` dafür — 240 Mal je Zeile,
//! 240 × 240 × 3 Mal je Lauf. LLVM zieht das heraus, Firn tat es nicht.
//!
//! ## Was hochgezogen wird
//!
//! Eine Instruktion wandert in den **Vorkopf**, wenn alles davon gilt:
//!
//! * Sie ist **rein**: `const`, `bin`, `cmp`, `un`, `cast`, `ptradd`. Kein
//!   `load` (ohne Aliasanalyse nicht beweisbar), kein `store`, `call`,
//!   `syscall`, `copymem`, `alloca`, `gcaddr`.
//! * Sie kann **nicht fallen**: `div`/`rem` sind ausgeschlossen, weil eine
//!   Division durch null auf der CPU eine Ausnahme auslöst. Ein Hochziehen
//!   würde sie auch dann auslösen, wenn die Schleife nie läuft. Verschiebungen
//!   sind erlaubt: eine zu große Weite ist auf x86 undefiniert, aber keine
//!   Falle, und der Wert wäre in der Schleife derselbe.
//! * **Kein Operand wird in der Schleife definiert** (oder er wurde selbst
//!   schon hochgezogen — deshalb die Fixpunktschleife).
//! * Weder Ergebnis noch Operand ist ein `secret`-Wert (SPEC §9.2), und die
//!   Instruktion ist nicht `select`/`barrier`/`secure_zero`.
//!
//! ## Sicherheit
//!
//! Der Vorkopf **dominiert** die ganze Schleife; damit ist jede hochgezogene
//! Definition an jeder bisherigen Verwendungsstelle gültig. Die `Val`-Id bleibt
//! gleich, es wird nichts umgeschrieben — nur die Position ändert sich.
//!
//! Eine Instruktion aus einem Block, der **nicht** bei jedem Durchlauf
//! ausgeführt wird (etwa in einem `if` im Schleifenrumpf), wird nach dem
//! Hochziehen unbedingt ausgeführt. Für fallenfreie, reine Rechnungen ist das
//! verhaltenserhaltend — im schlechtesten Fall rechnet der Vorkopf etwas, das
//! niemand liest. Genau deshalb ist die Fallenfreiheit oben Bedingung und nicht
//! Kür.
//!
//! Verschachtelte Schleifen brauchen hier keine Sonderbehandlung: `opt.rs`
//! iteriert bis zum Fixpunkt, und was aus der inneren Schleife in deren Vorkopf
//! gewandert ist, liegt danach im Rumpf der äußeren und wandert in der nächsten
//! Runde weiter. In `matmul` erreicht `r * n` so den Kopf der `cc`-Schleife.

use crate::fir::{BinOp, Func, Inst, Op, Term, Val};
use std::collections::HashSet;

/// Zieht schleifeninvariante Instruktionen in den Vorkopf. Liefert die Anzahl.
pub(crate) fn hoist_loop_invariants(f: &mut Func) -> usize {
    let n = f.blocks.len();
    if n < 2 {
        return 0;
    }
    let preds = crate::mem2reg::preds(f);
    let dom = crate::mem2reg::dominators(f);
    let mut moved = 0;

    // Rückwärtskanten: b -> h, wobei h den Block b dominiert.
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (b, blk) in f.blocks.iter().enumerate() {
        for s in blk.term.successors() {
            let h = s as usize;
            if h < n && dom[b][h] {
                edges.push((h, b));
            }
        }
    }
    if edges.is_empty() {
        return 0;
    }

    // Innerste Schleifen zuerst: kleinerer Rumpf = weiter innen.
    let mut loops: Vec<(usize, HashSet<usize>)> = Vec::new();
    for (h, b) in edges {
        loops.push((h, natural_loop(h, b, &preds)));
    }
    loops.sort_by_key(|(_, body)| body.len());

    for (head, body) in loops {
        let preheader = match preheader_of(f, head, &body, &preds) {
            Some(p) => p,
            None => continue,
        };
        moved += hoist_out(f, head, &body, preheader);
    }
    moved
}

/// Rumpf der natürlichen Schleife zur Rückwärtskante `back -> head`:
/// `head` plus alles, was `back` erreicht, ohne `head` zu passieren.
fn natural_loop(head: usize, back: usize, preds: &[Vec<usize>]) -> HashSet<usize> {
    let mut body = HashSet::new();
    body.insert(head);
    let mut stack = Vec::new();
    if back != head {
        body.insert(back);
        stack.push(back);
    }
    while let Some(b) = stack.pop() {
        for &p in &preds[b] {
            if body.insert(p) {
                stack.push(p);
            }
        }
    }
    body
}

/// Der eine Vorgänger des Kopfes außerhalb der Schleife — und nur, wenn er mit
/// einem schlichten `br` dorthin springt. Gibt es mehrere Eintritte, wird die
/// Schleife übersprungen: einen Vorkopf einzuziehen würde die Blocknummern
/// verschieben, und das ist diesen Durchgang nicht wert.
fn preheader_of(f: &Func, head: usize, body: &HashSet<usize>, preds: &[Vec<usize>]) -> Option<usize> {
    let mut outer = preds[head].iter().copied().filter(|p| !body.contains(p));
    let p = outer.next()?;
    if outer.next().is_some() {
        return None;
    }
    match f.blocks[p].term {
        Term::Br(t) if t as usize == head => Some(p),
        _ => None,
    }
}

/// Darf diese Instruktion überhaupt bewegt werden? (Reinheit + Fallenfreiheit)
fn hoistable_op(op: &Op) -> bool {
    if crate::mem2reg::is_untouchable(op) {
        return false;
    }
    match op {
        // Division und Rest können eine CPU-Ausnahme auslösen — niemals
        // unbedingt ausführen, nur weil der Wert invariant ist.
        Op::Bin(BinOp::Div, _, _) | Op::Bin(BinOp::Rem, _, _) => false,
        Op::Const(_)
        | Op::Bin(..)
        | Op::Cmp { .. }
        | Op::Un(..)
        | Op::Cast { .. }
        | Op::PtrAdd { .. } => true,
        _ => false,
    }
}

fn hoist_out(f: &mut Func, head: usize, body: &HashSet<usize>, preheader: usize) -> usize {
    let mut moved = 0;
    let mut buf: Vec<Val> = Vec::new();
    loop {
        // 1. Welche Werte entstehen in der Schleife?
        let mut in_loop: HashSet<Val> = HashSet::new();
        for &b in body {
            for i in &f.blocks[b].insts {
                if let Some(d) = i.dst {
                    in_loop.insert(d);
                }
            }
        }
        // 2. Erste bewegbare Instruktion suchen (in Blockreihenfolge).
        let mut hit: Option<(usize, usize)> = None;
        'search: for &b in {
            let mut v: Vec<usize> = body.iter().copied().collect();
            v.sort_unstable();
            &v.clone()
        } {
            for (ix, i) in f.blocks[b].insts.iter().enumerate() {
                if !hoistable_op(&i.op) {
                    continue;
                }
                let d = match i.dst {
                    Some(d) => d,
                    None => continue,
                };
                if f.is_secret(d) {
                    continue;
                }
                buf.clear();
                i.op.uses(&mut buf);
                if buf.iter().any(|v| in_loop.contains(v) || f.is_secret(*v)) {
                    continue;
                }
                // Der Kopf selbst darf seine Bedingung behalten: eine
                // Instruktion, die der Terminator des Kopfes braucht, ist zwar
                // hebbar, aber der Gewinn ist null. Wir heben sie trotzdem —
                // sie ist invariant, also ist auch die Bedingung invariant.
                let _ = head;
                hit = Some((b, ix));
                break 'search;
            }
        }
        let (b, ix) = match hit {
            Some(x) => x,
            None => break,
        };
        // 3. Verschieben: ans Ende des Vorkopfs, vor dessen Terminator.
        let inst: Inst = f.blocks[b].insts.remove(ix);
        f.blocks[preheader].insts.push(inst);
        moved += 1;
        if moved > 10_000 {
            break; // harte Bremse, kann nicht vorkommen
        }
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{BinOp, Block, FTy, Func, Inst, Op, Term};

    /// `bb0: br bb1` · `bb1: cmp/brcond` · `bb2: %x = mul p0,p1 ; br bb1`
    fn loop_with_invariant_multiplication() -> Func {
        let mut f = Func::new("t", vec![FTy::U64, FTy::U64], FTy::U64);
        // %0, %1 sind Parameter
        let c = f.new_val_pub(FTy::U64);
        let m = f.new_val_pub(FTy::U64);
        f.blocks = vec![
            Block { id: 0, insts: vec![], term: Term::Br(1) },
            Block {
                id: 1,
                insts: vec![Inst {
                    dst: Some(c),
                    ty: FTy::Bool,
                    op: Op::Cmp { op: crate::fir::CmpOp::Lt, ty: FTy::U64, a: 0, b: 1 },
                }],
                term: Term::BrCond { cond: c, then_bb: 2, else_bb: 3 },
            },
            Block {
                id: 2,
                insts: vec![Inst { dst: Some(m), ty: FTy::U64, op: Op::Bin(BinOp::Mul, 0, 1) }],
                term: Term::Br(1),
            },
            Block { id: 3, insts: vec![], term: Term::Ret(Some(0)) },
        ];
        f
    }

    #[test]
    fn invariant_multiplication_moves_in_the_preheader() {
        let mut f = loop_with_invariant_multiplication();
        let n = hoist_loop_invariants(&mut f);
        assert!(n >= 1, "nichts hochgezogen");
        assert!(
            f.blocks[2].insts.is_empty(),
            "die Multiplikation steht noch im Rumpf: {:?}",
            f.blocks[2].insts
        );
        assert!(
            f.blocks[0].insts.iter().any(|i| matches!(i.op, Op::Bin(BinOp::Mul, 0, 1))),
            "die Multiplikation ist nicht im Vorkopf gelandet"
        );
    }

    #[test]
    fn division_stays_in_the_loop() {
        let mut f = loop_with_invariant_multiplication();
        f.blocks[2].insts[0].op = Op::Bin(BinOp::Div, 0, 1);
        hoist_loop_invariants(&mut f);
        assert_eq!(
            f.blocks[2].insts.len(),
            1,
            "eine Division darf NIE unbedingt ausgefuehrt werden (Division durch null)"
        );
    }

    #[test]
    fn dependent_value_stays_inside() {
        let mut f = loop_with_invariant_multiplication();
        // %m haengt von einem load ab -> nicht invariant
        let l = f.new_val_pub(FTy::U64);
        f.blocks[2].insts.insert(
            0,
            Inst { dst: Some(l), ty: FTy::U64, op: Op::Load { addr: 0 } },
        );
        f.blocks[2].insts[1].op = Op::Bin(BinOp::Mul, l, 1);
        hoist_loop_invariants(&mut f);
        assert_eq!(f.blocks[2].insts.len(), 2, "nichts haette wandern duerfen");
    }
}
