// SPDX-License-Identifier: MPL-2.0
use crate::fir::{BinOp, BlockId, FTy, Func, Inst, Op, Term, Val};
use crate::fx::HashMap;

const MAX_HOISTED: usize = 4;
const MAX_PHIS: usize = 2;

pub(crate) fn enabled() -> bool {
    std::env::var_os("FIRN_IFCONV").is_some()
}

fn speculatable(i: &Inst) -> bool {
    if i.ty.is_float() || i.ty == FTy::V128 {
        return false;
    }
    match &i.op {
        Op::Const(_) | Op::Cmp { .. } | Op::Un(..) | Op::Cast { .. } | Op::Select { .. } | Op::PtrAdd { .. } => true,
        Op::GlobalAddr { .. } => true,
        Op::Bin(op, ..) => !matches!(op, BinOp::Div | BinOp::Rem),
        _ => false,
    }
}

fn selectable(ty: FTy) -> bool {
    !ty.is_float() && ty != FTy::V128
}

struct Cfg {
    index: HashMap<BlockId, usize>,
    preds: HashMap<BlockId, Vec<BlockId>>,
}

impl Cfg {
    fn build(f: &Func) -> Cfg {
        let mut index = HashMap::default();
        let mut preds: HashMap<BlockId, Vec<BlockId>> = HashMap::default();
        for (i, b) in f.blocks.iter().enumerate() {
            index.insert(b.id, i);
        }
        for b in &f.blocks {
            for s in b.term.successors() {
                preds.entry(s).or_default().push(b.id);
            }
        }
        Cfg { index, preds }
    }

    fn pred_count(&self, b: BlockId) -> usize {
        self.preds.get(&b).map_or(0, |v| v.len())
    }
}

struct Arm {
    via: Option<usize>,
    edge_from: BlockId,
}

fn arm(f: &Func, cfg: &Cfg, p: BlockId, s: BlockId, join: BlockId) -> Option<Arm> {
    if s == join {
        return Some(Arm { via: None, edge_from: p });
    }
    let si = *cfg.index.get(&s)?;
    let b = &f.blocks[si];
    if cfg.pred_count(s) != 1 || b.phi_count() > 0 || !matches!(b.term, Term::Br(t) if t == join) {
        return None;
    }
    if !b.insts.iter().all(speculatable) {
        return None;
    }
    Some(Arm { via: Some(si), edge_from: s })
}

fn join_of(f: &Func, cfg: &Cfg, s: BlockId) -> BlockId {
    match cfg.index.get(&s).map(|&i| &f.blocks[i].term) {
        Some(Term::Br(t)) if cfg.pred_count(s) == 1 => *t,
        _ => s,
    }
}

fn try_block(f: &mut Func, cfg: &Cfg, pi: usize) -> bool {
    let p = f.blocks[pi].id;
    let (cond, t, e) = match f.blocks[pi].term {
        Term::BrCond { cond, then_bb, else_bb } if then_bb != else_bb => (cond, then_bb, else_bb),
        _ => return false,
    };
    let jt = join_of(f, cfg, t);
    let je = join_of(f, cfg, e);
    let join = if jt == je {
        jt
    } else if jt == e {
        e
    } else if je == t {
        t
    } else {
        return false;
    };
    if join == p {
        return false;
    }
    let (at, ae) = match (arm(f, cfg, p, t, join), arm(f, cfg, p, e, join)) {
        (Some(a), Some(b)) => (a, b),
        _ => return false,
    };
    if at.via.is_none() && ae.via.is_none() {
        return false;
    }
    let hoisted: usize = [&at, &ae].iter().filter_map(|a| a.via).map(|i| f.blocks[i].insts.len()).sum();
    if hoisted > MAX_HOISTED {
        return false;
    }
    let ji = match cfg.index.get(&join) {
        Some(&i) => i,
        None => return false,
    };
    let nphi = f.blocks[ji].phi_count();
    if nphi == 0 || nphi > MAX_PHIS {
        return false;
    }
    let mut picks: Vec<(usize, Val, Val, FTy)> = Vec::new();
    for k in 0..nphi {
        let inst = &f.blocks[ji].insts[k];
        let inc = match &inst.op {
            Op::Phi { incoming } => incoming,
            _ => return false,
        };
        if !selectable(inst.ty) {
            return false;
        }
        let vt = inc.iter().find(|(b, _)| *b == at.edge_from).map(|x| x.1);
        let ve = inc.iter().find(|(b, _)| *b == ae.edge_from).map(|x| x.1);
        match (vt, ve) {
            (Some(a), Some(b)) => picks.push((k, a, b, inst.ty)),
            _ => return false,
        }
    }
    for a in [&at, &ae] {
        if let Some(si) = a.via {
            let moved = std::mem::take(&mut f.blocks[si].insts);
            f.blocks[pi].insts.extend(moved);
            f.blocks[si].term = Term::Ret(None);
        }
    }
    let loc = f.blocks[ji].insts[0].loc;
    for (k, vt, ve, ty) in picks {
        let v = if vt == ve {
            vt
        } else {
            let d = f.new_val_pub(ty);
            f.blocks[pi].insts.push(Inst { dst: Some(d), ty, op: Op::Select { cond, a: vt, b: ve }, loc });
            d
        };
        if let Op::Phi { incoming } = &mut f.blocks[ji].insts[k].op {
            incoming.retain(|(b, _)| *b != at.edge_from && *b != ae.edge_from);
            incoming.push((p, v));
        }
    }
    f.blocks[pi].term = Term::Br(join);
    true
}

pub(crate) fn run(f: &mut Func) -> usize {
    let mut n = 0usize;
    loop {
        let cfg = Cfg::build(f);
        let mut changed = false;
        for pi in (0..f.blocks.len()).rev() {
            if try_block(f, &cfg, pi) {
                changed = true;
                n += 1;
                break;
            }
        }
        if !changed {
            return n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::CmpOp;

    fn clamp_func() -> Func {
        let mut f = Func::new("t", vec![FTy::I32], FTy::I32);
        for _ in 0..5 {
            f.add_block();
        }
        let x = f.param_val(0);
        let lo = f.push(0, FTy::I32, Op::Const(0));
        let hi = f.push(0, FTy::I32, Op::Const(100));
        let c1 = f.push(0, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I32, a: x, b: lo });
        f.set_term(0, Term::BrCond { cond: c1, then_bb: 1, else_bb: 2 });
        f.set_term(1, Term::Br(5));
        let c2 = f.push(2, FTy::Bool, Op::Cmp { op: CmpOp::Gt, ty: FTy::I32, a: x, b: hi });
        f.set_term(2, Term::BrCond { cond: c2, then_bb: 3, else_bb: 4 });
        f.set_term(3, Term::Br(5));
        f.set_term(4, Term::Br(5));
        let r = f.push(5, FTy::I32, Op::Phi { incoming: vec![(1, lo), (3, hi), (4, x)] });
        f.set_term(5, Term::Ret(Some(r)));
        f
    }

    #[test]
    fn clamp_becomes_two_selects() {
        let mut f = clamp_func();
        assert_eq!(run(&mut f), 2);
        assert!(matches!(f.blocks[0].term, Term::Br(5)));
        let sel = f.blocks[0].insts.iter().filter(|i| matches!(i.op, Op::Select { .. })).count();
        assert_eq!(sel, 2);
        match &f.blocks[5].insts[0].op {
            Op::Phi { incoming } => assert_eq!(incoming.len(), 1),
            o => panic!("{:?}", o),
        }
    }

    #[test]
    fn arm_with_a_store_is_left_alone() {
        let mut f = clamp_func();
        let a = f.push(3, FTy::Ptr, Op::Alloca { size: 8, align: 8 });
        f.push_void(3, FTy::Void, Op::Store { addr: a, val: f.param_val(0) });
        assert_eq!(run(&mut f), 0);
        assert!(matches!(f.blocks[2].term, Term::BrCond { .. }));
    }
}
