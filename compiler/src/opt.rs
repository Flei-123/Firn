//! Optimierer auf FIR: Konstantenfaltung und Entfernen toten Codes.
//!
//! SCHNITTSTELLE (fest):
//!   `pub fn optimize(m: &mut fir::Module) -> OptStats`
//! Regel: Die Optimierung darf das Programmverhalten NIE aendern. Die
//! Testsuite faehrt jedes Programm mit und ohne `--no-opt` und vergleicht.
//!
//! Durchgefuehrte Umformungen (alle verhaltenserhaltend):
//!  1. **Konstantenfaltung** ueber `Op::Bin`, `Op::Cmp`, `Op::Un` und
//!     `Op::Cast`, wenn alle Operanden `Op::Const` sind. Das Ergebnis wird mit
//!     `FTy::truncate` auf Breite und Vorzeichen des Ergebnistyps normalisiert
//!     und ersetzt die Instruktion durch `Op::Const` — die `Val`-Id bleibt
//!     gleich, alle Verwendungen bleiben gueltig.
//!     NICHT gefaltet werden: Division/Rest durch null, der Ueberlauffall
//!     `MIN / -1` bzw. `MIN % -1` (beides loest auf der CPU eine Ausnahme aus)
//!     und Verschiebungen mit einer Weite >= Bitbreite (auf x86 undefiniert).
//!  2. **Vereinfachung von `brcond`** mit konstanter Bedingung (oder gleichen
//!     Zielen) zu `br`. Erst dadurch entsteht unerreichbarer Code.
//!  3. **Toter Code**: unerreichbare Bloecke (Erreichbarkeit ab `bb0` ueber
//!     `Term::successors`) werden entfernt und die verbleibenden Bloecke
//!     luecklos neu nummeriert (Invariante `blocks[i].id == i` bleibt erhalten,
//!     alle Terminatoren werden umgeschrieben). Unbenutzte REINE Instruktionen
//!     (kein `store`/`call`/`syscall`/`copymem`) werden entfernt; `alloca` nur
//!     dann, wenn ihr Zeiger nirgends mehr verwendet wird.
//!
//! Es wird bis zum Fixpunkt iteriert, aber hoechstens `MAX_ROUNDS` mal, damit
//! der Optimierer unter keinen Umstaenden haengen bleibt.

use crate::fir::{BinOp, BlockId, CmpOp, FTy, Func, Module, Op, Term, UnOp, Val};
use std::collections::{HashMap, HashSet};

/// harte Obergrenze der Fixpunkt-Iterationen
const MAX_ROUNDS: u32 = 50;

#[derive(Clone, Copy, Debug, Default)]
pub struct OptStats {
    /// Anzahl zu Konstanten gefalteter Instruktionen
    pub folded: usize,
    /// entfernte Instruktionen (tot/unbenutzt und rein)
    pub removed_insts: usize,
    /// entfernte, unerreichbare Basisbloecke
    pub removed_blocks: usize,
}

pub fn optimize(m: &mut Module) -> OptStats {
    let mut st = OptStats::default();
    for f in m.funcs.iter_mut() {
        optimize_func(f, &mut st);
    }
    st
}

fn optimize_func(f: &mut Func, st: &mut OptStats) {
    let mut round = 0;
    loop {
        round += 1;
        let mut changed = false;
        changed |= fold_constants(f, st);
        changed |= simplify_terminators(f);
        changed |= remove_unreachable_blocks(f, st);
        changed |= remove_dead_insts(f, st);
        if !changed || round >= MAX_ROUNDS {
            break;
        }
    }
}

// ---------------------------------------------------------------- Faltung ---

/// Sammelt alle bekannten Konstantenwerte der Funktion.
fn const_map(f: &Func) -> HashMap<Val, i128> {
    let mut m = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
                m.insert(d, *c);
            }
        }
    }
    m
}

fn fold_constants(f: &mut Func, st: &mut OptStats) -> bool {
    let mut consts = const_map(f);
    let mut changed = false;
    for bi in 0..f.blocks.len() {
        for ii in 0..f.blocks[bi].insts.len() {
            let (ty, op, dst) = {
                let i = &f.blocks[bi].insts[ii];
                (i.ty, i.op.clone(), i.dst)
            };
            let dst = match dst {
                Some(d) => d,
                None => continue,
            };
            let folded = match op {
                Op::Bin(bop, a, b) => match (consts.get(&a), consts.get(&b)) {
                    (Some(&x), Some(&y)) => fold_bin(ty, bop, x, y),
                    _ => None,
                },
                Op::Cmp { op, ty: oty, a, b } => match (consts.get(&a), consts.get(&b)) {
                    (Some(&x), Some(&y)) => Some(fold_cmp(oty, op, x, y)),
                    _ => None,
                },
                Op::Un(uop, a) => consts.get(&a).map(|&x| fold_un(ty, uop, x)),
                Op::Cast { src, from } => match consts.get(&src) {
                    Some(&x) => fold_cast(ty, from, x),
                    None => None,
                },
                _ => None,
            };
            if let Some(v) = folded {
                f.blocks[bi].insts[ii].op = Op::Const(v);
                consts.insert(dst, v);
                st.folded += 1;
                changed = true;
            }
        }
    }
    changed
}

fn fold_bin(ty: FTy, op: BinOp, a: i128, b: i128) -> Option<i128> {
    if ty == FTy::Void || ty.bits() == 0 {
        return None;
    }
    let a = ty.truncate(a);
    let b = ty.truncate(b);
    let bits = ty.bits() as i128;
    let min_signed: i128 = if ty.signed() { -(1i128 << (ty.bits() - 1)) } else { 0 };
    let r = match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => {
            if b == 0 || (ty.signed() && a == min_signed && b == -1) {
                return None;
            }
            a / b
        }
        BinOp::Rem => {
            if b == 0 || (ty.signed() && a == min_signed && b == -1) {
                return None;
            }
            a % b
        }
        BinOp::And => a & b,
        BinOp::Or => a | b,
        BinOp::Xor => a ^ b,
        BinOp::Shl => {
            if b < 0 || b >= bits {
                return None;
            }
            a << b
        }
        BinOp::Shr => {
            if b < 0 || b >= bits {
                return None;
            }
            // `a` ist bereits vorzeichenrichtig normalisiert: bei
            // vorzeichenlosen Typen nicht negativ (-> logische Verschiebung),
            // bei vorzeichenbehafteten arithmetisch.
            a >> b
        }
    };
    Some(ty.truncate(r))
}

fn fold_cmp(ty: FTy, op: CmpOp, a: i128, b: i128) -> i128 {
    let a = ty.truncate(a);
    let b = ty.truncate(b);
    let r = match op {
        CmpOp::Eq => a == b,
        CmpOp::Ne => a != b,
        CmpOp::Lt => a < b,
        CmpOp::Le => a <= b,
        CmpOp::Gt => a > b,
        CmpOp::Ge => a >= b,
    };
    if r {
        1
    } else {
        0
    }
}

fn fold_un(ty: FTy, op: UnOp, a: i128) -> i128 {
    let a = ty.truncate(a);
    match op {
        UnOp::Neg => ty.truncate(-a),
        UnOp::Not => {
            if ty == FTy::Bool {
                if a & 1 != 0 {
                    0
                } else {
                    1
                }
            } else {
                ty.truncate(!a)
            }
        }
    }
}

fn fold_cast(to: FTy, from: FTy, a: i128) -> Option<i128> {
    if to == FTy::Void || from == FTy::Void {
        return None;
    }
    // Ganzzahl -> bool ist keine reine Bitoperation (Vergleich mit 0 gegenueber
    // "unterstes Bit"); das ueberlaesst der Optimierer dem Backend.
    if to == FTy::Bool && from != FTy::Bool {
        return None;
    }
    Some(to.truncate(from.truncate(a)))
}

// ---------------------------------------------------------- Terminatoren ---

fn simplify_terminators(f: &mut Func) -> bool {
    let consts = const_map(f);
    let mut changed = false;
    for b in f.blocks.iter_mut() {
        if let Term::BrCond { cond, then_bb, else_bb } = b.term {
            if then_bb == else_bb {
                b.term = Term::Br(then_bb);
                changed = true;
            } else if let Some(&c) = consts.get(&cond) {
                b.term = Term::Br(if c != 0 { then_bb } else { else_bb });
                changed = true;
            }
        } else if let Term::Switch { val, cases, default, .. } = &b.term {
            // Konstante Marke: direkt zum passenden Zweig springen.
            if let Some(&c) = consts.get(val) {
                let t = cases.iter().find(|(k, _)| *k == c).map(|(_, t)| *t).unwrap_or(*default);
                b.term = Term::Br(t);
                changed = true;
            } else if cases.iter().all(|(_, t)| *t == *default) {
                let d = *default;
                b.term = Term::Br(d);
                changed = true;
            }
        }
    }
    changed
}

// ------------------------------------------------------------- toter Code ---

fn collect_uses(f: &Func, blocks: &[usize]) -> HashSet<Val> {
    let mut used = HashSet::new();
    let mut buf = Vec::new();
    for &bi in blocks {
        let b = &f.blocks[bi];
        for i in &b.insts {
            buf.clear();
            i.op.uses(&mut buf);
            for v in buf.iter() {
                used.insert(*v);
            }
        }
        match &b.term {
            Term::BrCond { cond, .. } => {
                used.insert(*cond);
            }
            Term::Ret(Some(v)) => {
                used.insert(*v);
            }
            Term::Switch { val, .. } => {
                used.insert(*val);
            }
            Term::Br(_) | Term::Ret(None) | Term::Unset => {}
        }
    }
    used
}

fn remove_unreachable_blocks(f: &mut Func, st: &mut OptStats) -> bool {
    if f.blocks.is_empty() {
        return false;
    }
    let mut index_of: HashMap<BlockId, usize> = HashMap::new();
    for (i, b) in f.blocks.iter().enumerate() {
        index_of.insert(b.id, i);
    }
    // Erreichbarkeit ab dem Eintrittsblock
    let mut reachable = vec![false; f.blocks.len()];
    let mut stack = vec![0usize];
    reachable[0] = true;
    while let Some(bi) = stack.pop() {
        for s in f.blocks[bi].term.successors() {
            if let Some(&si) = index_of.get(&s) {
                if !reachable[si] {
                    reachable[si] = true;
                    stack.push(si);
                }
            }
        }
    }
    if reachable.iter().all(|&r| r) {
        return false;
    }

    // Sicherheitsnetz: Wird ein in einem unerreichbaren Block definierter Wert
    // noch aus erreichbarem Code heraus gelesen (das waere ein Verstoss gegen
    // die SSA-Dominanz), wird NICHTS entfernt — lieber toter Code als eine
    // baumelnde Val-Id.
    let live_idx: Vec<usize> = (0..f.blocks.len()).filter(|&i| reachable[i]).collect();
    let used = collect_uses(f, &live_idx);
    for (i, b) in f.blocks.iter().enumerate() {
        if reachable[i] {
            continue;
        }
        for inst in &b.insts {
            if let Some(d) = inst.dst {
                if used.contains(&d) {
                    return false;
                }
            }
        }
    }

    let removed_insts: usize =
        f.blocks.iter().enumerate().filter(|(i, _)| !reachable[*i]).map(|(_, b)| b.insts.len()).sum();
    let removed_blocks = reachable.iter().filter(|&&r| !r).count();

    // luecklos neu nummerieren, Reihenfolge bleibt erhalten
    let mut new_id: HashMap<BlockId, BlockId> = HashMap::new();
    let mut kept = Vec::with_capacity(live_idx.len());
    for (n, &i) in live_idx.iter().enumerate() {
        new_id.insert(f.blocks[i].id, n as BlockId);
        kept.push(f.blocks[i].clone());
    }
    for (n, b) in kept.iter_mut().enumerate() {
        b.id = n as BlockId;
        b.term = match &b.term {
            Term::Br(t) => Term::Br(new_id[t]),
            Term::BrCond { cond, then_bb, else_bb } => {
                Term::BrCond { cond: *cond, then_bb: new_id[then_bb], else_bb: new_id[else_bb] }
            }
            Term::Switch { val, ty, cases, default } => Term::Switch {
                val: *val,
                ty: *ty,
                cases: cases.iter().map(|(k, t)| (*k, new_id[t])).collect(),
                default: new_id[default],
            },
            other => other.clone(),
        };
    }
    f.blocks = kept;
    st.removed_blocks += removed_blocks;
    st.removed_insts += removed_insts;
    true
}

fn remove_dead_insts(f: &mut Func, st: &mut OptStats) -> bool {
    let mut changed = false;
    let mut round = 0;
    loop {
        round += 1;
        let all: Vec<usize> = (0..f.blocks.len()).collect();
        let used = collect_uses(f, &all);
        let mut removed = 0usize;
        for b in f.blocks.iter_mut() {
            let before = b.insts.len();
            b.insts.retain(|i| {
                if !i.op.is_pure() {
                    return true;
                }
                match i.dst {
                    Some(d) => used.contains(&d),
                    // reine Instruktion ohne Ergebnis: wirkungslos
                    None => false,
                }
            });
            removed += before - b.insts.len();
        }
        if removed == 0 {
            break;
        }
        st.removed_insts += removed;
        changed = true;
        if round >= MAX_ROUNDS {
            break;
        }
    }
    changed
}

// ------------------------------------------------------------------ Tests ---

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{Inst, Term};

    fn consts_in(f: &Func) -> Vec<i128> {
        let mut v = Vec::new();
        for b in &f.blocks {
            for i in &b.insts {
                if let Op::Const(c) = i.op {
                    v.push(c);
                }
            }
        }
        v
    }

    #[test]
    fn faltet_arithmetik_und_entfernt_zwischenwerte() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(20));
        let b = f.push(0, FTy::I32, Op::Const(2));
        let m = f.push(0, FTy::I32, Op::Bin(BinOp::Mul, a, b));
        let c = f.push(0, FTy::I32, Op::Const(2));
        let s = f.push(0, FTy::I32, Op::Bin(BinOp::Add, m, c));
        f.set_term(0, Term::Ret(Some(s)));
        let before = f.inst_count();
        let mut m0 = Module::new();
        m0.funcs.push(f);
        let st = optimize(&mut m0);
        let f = &m0.funcs[0];
        assert!(st.folded >= 2, "es muss gefaltet werden: {:?}", st);
        assert!(f.inst_count() < before, "{} -> {}", before, f.inst_count());
        assert_eq!(f.inst_count(), 1);
        assert_eq!(consts_in(f), vec![42]);
        assert!(matches!(f.blocks[0].term, Term::Ret(Some(_))));
    }

    #[test]
    fn division_durch_null_bleibt_stehen() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(7));
        let b = f.push(0, FTy::I32, Op::Const(0));
        let d = f.push(0, FTy::I32, Op::Bin(BinOp::Div, a, b));
        f.set_term(0, Term::Ret(Some(d)));
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        assert_eq!(st.folded, 0);
        assert_eq!(m.funcs[0].inst_count(), 3);
        assert!(matches!(m.funcs[0].blocks[0].insts[2].op, Op::Bin(BinOp::Div, _, _)));
    }

    #[test]
    fn zu_breite_verschiebung_bleibt_stehen() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(1));
        let b = f.push(0, FTy::I32, Op::Const(32));
        let s = f.push(0, FTy::I32, Op::Bin(BinOp::Shl, a, b));
        f.set_term(0, Term::Ret(Some(s)));
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        assert_eq!(st.folded, 0);
        assert!(matches!(m.funcs[0].blocks[0].insts[2].op, Op::Bin(BinOp::Shl, _, _)));
    }

    #[test]
    fn ueberlauf_wird_korrekt_zurechtgestutzt() {
        let mut f = Func::new("t", vec![], FTy::I8);
        let a = f.push(0, FTy::I8, Op::Const(100));
        let b = f.push(0, FTy::I8, Op::Const(100));
        let s = f.push(0, FTy::I8, Op::Bin(BinOp::Add, a, b));
        f.set_term(0, Term::Ret(Some(s)));
        let mut m = Module::new();
        m.funcs.push(f);
        optimize(&mut m);
        assert_eq!(consts_in(&m.funcs[0]), vec![-56]); // 200 mod 256 als i8
    }

    #[test]
    fn unsignierte_verschiebung_und_cast() {
        let mut f = Func::new("t", vec![], FTy::U64);
        let a = f.push(0, FTy::U8, Op::Const(200));
        let c = f.push(0, FTy::U64, Op::Cast { src: a, from: FTy::U8 });
        let sh = f.push(0, FTy::U64, Op::Const(1));
        let r = f.push(0, FTy::U64, Op::Bin(BinOp::Shr, c, sh));
        f.set_term(0, Term::Ret(Some(r)));
        let mut m = Module::new();
        m.funcs.push(f);
        optimize(&mut m);
        assert_eq!(consts_in(&m.funcs[0]), vec![100]);

        // vorzeichenbehaftete Verkuerzung/Erweiterung
        let mut f = Func::new("t2", vec![], FTy::I64);
        let a = f.push(0, FTy::I8, Op::Const(-1));
        let c = f.push(0, FTy::I64, Op::Cast { src: a, from: FTy::I8 });
        f.set_term(0, Term::Ret(Some(c)));
        let mut m = Module::new();
        m.funcs.push(f);
        optimize(&mut m);
        assert_eq!(consts_in(&m.funcs[0]), vec![-1]);

        // i8 -1 -> u32 = 4294967295
        let mut f = Func::new("t3", vec![], FTy::U32);
        let a = f.push(0, FTy::I8, Op::Const(-1));
        let c = f.push(0, FTy::U32, Op::Cast { src: a, from: FTy::I8 });
        f.set_term(0, Term::Ret(Some(c)));
        let mut m = Module::new();
        m.funcs.push(f);
        optimize(&mut m);
        assert_eq!(consts_in(&m.funcs[0]), vec![4294967295]);
    }

    #[test]
    fn vergleich_und_zweig_falten_unerreichbaren_block_weg() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let then_bb = f.add_block();
        let else_bb = f.add_block();
        let a = f.push(0, FTy::I32, Op::Const(3));
        let b = f.push(0, FTy::I32, Op::Const(4));
        let c = f.push(0, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I32, a, b });
        f.set_term(0, Term::BrCond { cond: c, then_bb, else_bb });
        let x = f.push(then_bb, FTy::I32, Op::Const(1));
        f.set_term(then_bb, Term::Ret(Some(x)));
        let y = f.push(else_bb, FTy::I32, Op::Const(2));
        f.set_term(else_bb, Term::Ret(Some(y)));
        let blocks_before = f.blocks.len();
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        let f = &m.funcs[0];
        assert_eq!(st.removed_blocks, 1);
        assert_eq!(f.blocks.len(), blocks_before - 1);
        // Block-Ids bleiben lueckenlos und passen zu ihrer Position
        for (i, b) in f.blocks.iter().enumerate() {
            assert_eq!(b.id, i as u32);
        }
        assert!(matches!(f.blocks[0].term, Term::Br(1)));
        assert_eq!(consts_in(f), vec![1]);
    }

    #[test]
    fn seiteneffekte_bleiben_erhalten() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(4, 4);
        let v = f.push(0, FTy::I32, Op::Const(5));
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: v });
        let n = f.push(0, FTy::I64, Op::Const(60));
        let arg = f.push(0, FTy::I64, Op::Const(0));
        let sc = f.push(0, FTy::I64, Op::Syscall { args: vec![n, arg] });
        let unused = f.push(0, FTy::I32, Op::Const(99));
        let _ = unused;
        let call = f.push(0, FTy::I32, Op::Call { name: "f".into(), args: vec![] });
        let _ = call;
        let ld = f.push(0, FTy::I32, Op::Load { addr: slot });
        f.set_term(0, Term::Ret(Some(ld)));
        let _ = sc;
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        let f = &m.funcs[0];
        // entfernt wird nur die unbenutzte Konstante 99
        assert_eq!(st.removed_insts, 1);
        let kinds: Vec<&str> = f.blocks[0]
            .insts
            .iter()
            .map(|i: &Inst| match &i.op {
                Op::Alloca { .. } => "alloca",
                Op::Const(_) => "const",
                Op::Store { .. } => "store",
                Op::Syscall { .. } => "syscall",
                Op::Call { .. } => "call",
                Op::Load { .. } => "load",
                _ => "?",
            })
            .collect();
        assert_eq!(kinds, vec!["alloca", "const", "store", "const", "const", "syscall", "call", "load"]);
    }

    #[test]
    fn unbenutzte_alloca_verschwindet_kettenweise() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(8, 8);
        let off = f.push(0, FTy::I64, Op::Const(4));
        let p = f.push(0, FTy::Ptr, Op::PtrAdd { base: slot, off });
        let _ld = f.push(0, FTy::I32, Op::Load { addr: p });
        let r = f.push(0, FTy::I32, Op::Const(0));
        f.set_term(0, Term::Ret(Some(r)));
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        assert_eq!(m.funcs[0].inst_count(), 1);
        assert_eq!(st.removed_insts, 4);
    }

    #[test]
    fn schleife_bleibt_unangetastet_und_terminiert() {
        // while (i < 10) { i = i + 1 }  — nichts davon ist konstant faltbar,
        // der Optimierer darf hier nichts entfernen und muss anhalten.
        let mut f = Func::new("t", vec![], FTy::I32);
        let head = f.add_block();
        let body = f.add_block();
        let exit = f.add_block();
        let slot = f.alloca(4, 4);
        let zero = f.push(0, FTy::I32, Op::Const(0));
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: zero });
        f.set_term(0, Term::Br(head));
        let i = f.push(head, FTy::I32, Op::Load { addr: slot });
        let ten = f.push(head, FTy::I32, Op::Const(10));
        let c = f.push(head, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I32, a: i, b: ten });
        f.set_term(head, Term::BrCond { cond: c, then_bb: body, else_bb: exit });
        let i2 = f.push(body, FTy::I32, Op::Load { addr: slot });
        let one = f.push(body, FTy::I32, Op::Const(1));
        let s = f.push(body, FTy::I32, Op::Bin(BinOp::Add, i2, one));
        f.push_void(body, FTy::I32, Op::Store { addr: slot, val: s });
        f.set_term(body, Term::Br(head));
        let r = f.push(exit, FTy::I32, Op::Load { addr: slot });
        f.set_term(exit, Term::Ret(Some(r)));
        let before = f.inst_count();
        let blocks = f.blocks.len();
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        assert_eq!(st.folded, 0);
        assert_eq!(st.removed_insts, 0);
        assert_eq!(st.removed_blocks, 0);
        assert_eq!(m.funcs[0].inst_count(), before);
        assert_eq!(m.funcs[0].blocks.len(), blocks);
    }

    #[test]
    fn kette_wird_bis_zum_fixpunkt_gefaltet() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let mut v = f.push(0, FTy::I32, Op::Const(1));
        for _ in 0..10 {
            let one = f.push(0, FTy::I32, Op::Const(1));
            v = f.push(0, FTy::I32, Op::Bin(BinOp::Add, v, one));
        }
        let neg = f.push(0, FTy::I32, Op::Un(UnOp::Neg, v));
        f.set_term(0, Term::Ret(Some(neg)));
        let mut m = Module::new();
        m.funcs.push(f);
        optimize(&mut m);
        assert_eq!(m.funcs[0].inst_count(), 1);
        assert_eq!(consts_in(&m.funcs[0]), vec![-11]);
    }
}
