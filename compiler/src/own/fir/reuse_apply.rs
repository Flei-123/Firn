use super::reuse::{Alloc, Plan};
use crate::fir::{BinOp, CmpOp, FTy, Func, Inst, Op, Term, Val};
use crate::fx::{HashMap, HashSet};

const FLAGS_WITHOUT_MARK: i128 = 0xFFFF_FFFC;

pub(crate) fn apply(f: &mut Func, plans: &[Plan], allocs: &[Alloc], handles: &HashMap<usize, Val>, exits: &[Vec<(usize, usize)>], free_fn: &str) -> usize {
    let mut n = 0;
    for (p, ex) in plans.iter().zip(exits.iter()) {
        if let Some(&h) = handles.get(&p.free) {
            reuse_one(f, p, &allocs[p.alloc], h, ex, free_fn);
            n += 1;
        }
    }
    n
}

fn pos_of(f: &Func, id: u32) -> usize {
    f.blocks.iter().position(|b| b.id == id).unwrap_or(id as usize)
}

fn succs(t: &Term) -> Vec<u32> {
    match t {
        Term::Br(b) => vec![*b],
        Term::BrCond { then_bb, else_bb, .. } => vec![*then_bb, *else_bb],
        Term::Switch { cases, default, .. } => {
            let mut v: Vec<u32> = cases.iter().map(|c| c.1).collect();
            v.push(*default);
            v
        }
        _ => Vec::new(),
    }
}

fn preds_of(f: &Func, x: usize) -> Vec<usize> {
    let id = f.blocks[x].id;
    let mut out = Vec::new();
    for (i, b) in f.blocks.iter().enumerate() {
        if succs(&b.term).contains(&id) && !out.contains(&i) {
            out.push(i);
        }
    }
    out
}

fn konst(f: &mut Func, at: usize, ty: FTy, c: i128) -> Val {
    let v = f.new_val_pub(ty);
    f.blocks[at].insts.insert(0, Inst::new(Some(v), ty, Op::Const(c)));
    v
}

fn push(f: &mut Func, b: usize, ty: FTy, op: Op) -> Val {
    let v = f.new_val_pub(ty);
    f.blocks[b].insts.push(Inst::new(Some(v), ty, op));
    v
}

fn free_at_start(f: &mut Func, b: usize, h: Val, free_fn: &str) {
    let at = f.blocks[b].insts.iter().position(|i| !matches!(i.op, Op::Phi { .. })).unwrap_or(f.blocks[b].insts.len());
    f.blocks[b].insts.insert(at, Inst::new(None, FTy::Void, Op::Call { name: free_fn.to_string(), args: vec![h] }));
}

fn reuse_one(f: &mut Func, p: &Plan, a: &Alloc, h: Val, exits: &[(usize, usize)], free_fn: &str) {
    let entry = pos_of(f, f.entry());
    let ty = f.val_ty(h);
    let zero = konst(f, entry, ty, 0);
    let mut hv = h;
    let mut at_path: Vec<Val> = vec![h];
    let mut scope: Vec<usize> = vec![p.path[0]];
    for k in 1..p.path.len() {
        let (prev, x) = (p.path[k - 1], p.path[k]);
        let preds = preds_of(f, x);
        if preds.len() > 1 {
            let incoming = preds.iter().map(|&q| (f.blocks[q].id, if q == prev { hv } else { zero })).collect();
            let ph = f.new_val_pub(ty);
            f.blocks[x].insts.insert(0, Inst::new(Some(ph), ty, Op::Phi { incoming }));
            hv = ph;
            scope.clear();
        }
        scope.push(x);
        at_path.push(hv);
    }
    for &(k, y) in exits.iter() {
        if a.call || y != a.slow {
            free_at_start(f, y, at_path[k], free_fn);
        }
    }
    let head = a.head;
    let Some(idx) = f.blocks[head].insts.iter().position(|i| i.dst == Some(a.b0)) else { return };
    if a.call {
        if let Op::Call { name, args } = &mut f.blocks[head].insts[idx].op {
            *name = super::registry::REUSE_ALLOC.to_string();
            args.insert(0, hv);
        }
        return;
    }
    let h2 = f.add_block() as usize;
    let tail = f.blocks[head].insts.split_off(idx);
    let term = std::mem::replace(&mut f.blocks[head].term, Term::Unset);
    f.blocks[h2].insts = tail;
    f.blocks[h2].term = term;
    retarget_phis(f, h2, f.blocks[head].id, f.blocks[h2].id);
    let r1 = f.add_block() as usize;
    let r = f.add_block() as usize;
    let (r1_id, r_id, h2_id) = (f.blocks[r1].id, f.blocks[r].id, f.blocks[h2].id);
    let zero2 = f.new_val_pub(ty);
    f.blocks[head].insts.push(Inst::new(Some(zero2), ty, Op::Const(0)));
    let nz = push(f, head, FTy::Bool, Op::Cmp { op: CmpOp::Ne, ty, a: hv, b: zero2 });
    f.blocks[head].term = Term::BrCond { cond: nz, then_bb: r1_id, else_bb: h2_id };
    let c12 = push(f, r1, FTy::I64, Op::Const(-12));
    let fa = push(f, r1, FTy::Ptr, Op::PtrAdd { base: hv, off: c12 });
    let zw = push(f, r1, FTy::U32, Op::Load { addr: fa });
    let mk = push(f, r1, FTy::U32, Op::Const(FLAGS_WITHOUT_MARK));
    let m = push(f, r1, FTy::U32, Op::Bin(BinOp::And, zw, mk));
    let c0 = push(f, r1, FTy::U32, Op::Const(0));
    let ok = push(f, r1, FTy::Bool, Op::Cmp { op: CmpOp::Eq, ty: FTy::U32, a: m, b: c0 });
    f.blocks[r1].term = Term::BrCond { cond: ok, then_bb: r_id, else_bb: h2_id };
    let c16 = push(f, r, FTy::I64, Op::Const(-(crate::gc::BLOCK_HEADER as i128)));
    let rb = push(f, r, f.val_ty(a.b0), Op::PtrAdd { base: hv, off: c16 });
    clone_take(f, a, h2, r, rb);
    if preds_of(f, a.slow).iter().all(|q| *q == h2 || scope.contains(q)) {
        free_at_start(f, a.slow, hv, free_fn);
    }
}

fn retarget_phis(f: &mut Func, from: usize, old: u32, new: u32) {
    for s in succs(&f.blocks[from].term) {
        let sp = pos_of(f, s);
        for inst in f.blocks[sp].insts.iter_mut() {
            let Op::Phi { incoming } = &mut inst.op else { continue };
            for e in incoming.iter_mut() {
                if e.0 == old {
                    e.0 = new;
                }
            }
        }
    }
}

fn clone_take(f: &mut Func, a: &Alloc, h2: usize, r: usize, rb: Val) {
    let mut map: HashMap<Val, Val> = HashMap::default();
    map.insert(a.b0, rb);
    let h2_consts: HashMap<Val, (FTy, i128)> = f.blocks[h2]
        .insts
        .iter()
        .filter_map(|i| match (i.dst, &i.op) {
            (Some(d), Op::Const(c)) => Some((d, (i.ty, *c))),
            _ => None,
        })
        .collect();
    let insts = f.blocks[a.take].insts.clone();
    let mut done_consts: HashSet<Val> = HashSet::default();
    for inst in insts {
        if let Op::Store { val, .. } = &inst.op {
            if *val == a.next {
                continue;
            }
        }
        let mut op = inst.op.clone();
        if let Op::Store { val, .. } = &mut op {
            if *val == a.since_n {
                *val = a.since;
            }
        }
        let mut uses = Vec::new();
        op.uses(&mut uses);
        for u in uses {
            if let Some(&(ty, c)) = h2_consts.get(&u) {
                if done_consts.insert(u) {
                    let v = push(f, r, ty, Op::Const(c));
                    map.insert(u, v);
                }
            }
        }
        op.for_each_use_mut(|v| {
            if let Some(&m) = map.get(v) {
                *v = m;
            }
        });
        let dst = inst.dst.map(|d| {
            let n = f.new_val_pub(f.val_ty(d));
            map.insert(d, n);
            n
        });
        f.blocks[r].insts.push(Inst::like(dst, inst.ty, op, inst.loc));
    }
    let term = f.blocks[a.take].term.clone();
    f.blocks[r].term = term;
    let (t_id, r_id) = (f.blocks[a.take].id, f.blocks[r].id);
    for inst in f.blocks[a.done].insts.iter_mut() {
        let Op::Phi { incoming } = &mut inst.op else { continue };
        let add: Vec<(u32, Val)> = incoming.iter().filter(|e| e.0 == t_id).map(|e| (r_id, map.get(&e.1).copied().unwrap_or(e.1))).collect();
        incoming.extend(add);
    }
}
