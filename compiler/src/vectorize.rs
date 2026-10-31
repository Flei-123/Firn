// SPDX-License-Identifier: MPL-2.0

use crate::fir::{BinOp, CmpOp, FTy, Func, Op, Term, Val};
use crate::fx::{HashMap, HashSet};

pub(crate) const UNROLL: u32 = 2;
const MAX_CHAIN: usize = 4;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Cls {
    Inv,
    Iv(i128),
    Idx(i128, u64),
    Addr(Val, i128, u64),
    Vec,
    Acc,
    AccNext,
}

#[derive(Clone, Debug)]
pub(crate) struct Test {
    pub pos: usize,
    pub n: Val,
    pub e: i128,
    pub r: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Acc {
    pub phi: Val,
    pub init: Val,
    pub op: BinOp,
    pub ty: FTy,
}

#[derive(Clone, Debug)]
pub(crate) struct IvPhi {
    pub phi: Val,
    pub init: Val,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Key {
    pub base: Val,
    pub c: i128,
    pub e: u64,
}

#[derive(Clone, Debug)]
pub(crate) enum Step {
    Load { dst: Val, key: usize },
    Bin { dst: Val, op: BinOp, a: Val, b: Val },
    Store { key: usize, val: Val },
    AccUpd { acc: usize, x: Val },
}

pub(crate) struct Plan {
    pub h: usize,
    pub p: usize,
    pub chain: Vec<usize>,
    pub s: i128,
    pub ivs: Vec<IvPhi>,
    pub tests: Vec<Test>,
    pub accs: Vec<Acc>,
    pub steps: Vec<Step>,
    pub keys: Vec<Key>,
    pub store_keys: Vec<usize>,
    pub inv_loads: Vec<Val>,
    pub ebits: u32,
    pub rep: HashMap<Val, Val>,
    pub defs: HashMap<Val, (usize, usize)>,
    pub consts: HashMap<Val, i128>,
    pub cls: HashMap<Val, Cls>,
}

pub(crate) fn elem_bits(ty: FTy) -> Option<u32> {
    match ty {
        FTy::I64 | FTy::U64 => Some(64),
        FTy::I32 | FTy::U32 => Some(32),
        _ => None,
    }
}

pub(crate) fn acc_op(op: BinOp) -> bool {
    matches!(op, BinOp::Add | BinOp::Xor | BinOp::Or | BinOp::And)
}

pub fn run(f: &mut Func) -> usize {
    if crate::target::active() != crate::target::Target::X86_64 {
        return 0;
    }
    if std::env::var_os("FIRN_NO_VECTORIZE").is_some() {
        return 0;
    }
    let original = f.blocks.len();
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    let mut count = 0;
    let mut tried: HashSet<usize> = HashSet::default();
    loop {
        let preds = predecessors(f);
        let mut found = None;
        for h in 0..original.min(f.blocks.len()) {
            if tried.contains(&h) {
                continue;
            }
            tried.insert(h);
            if let Some(plan) = analyze(f, &preds, h) {
                found = Some(plan);
                break;
            }
        }
        match found {
            Some(plan) => {
                if crate::vectorize_emit::apply(f, &plan) {
                    count += 1;
                }
            }
            None => break,
        }
    }
    count
}

fn predecessors(f: &Func) -> Vec<Vec<usize>> {
    let nb = f.blocks.len();
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); nb];
    for (i, b) in f.blocks.iter().enumerate() {
        b.term.for_each_succ(|s| {
            let s = s as usize;
            if s < nb && !preds[s].contains(&i) {
                preds[s].push(i);
            }
        });
    }
    preds
}

fn const_map(f: &Func) -> HashMap<Val, i128> {
    let mut m = HashMap::default();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Op::Const(k), Some(d)) = (&i.op, i.dst) {
                m.insert(d, *k);
            }
        }
    }
    m
}

struct Chain {
    blocks: Vec<usize>,
    p: usize,
    tests: Vec<(usize, Val, bool)>,
}

fn find_chain(f: &Func, preds: &[Vec<usize>], h: usize) -> Option<Chain> {
    if preds[h].len() != 2 {
        return None;
    }
    let mut blocks = vec![h];
    let mut tests = Vec::new();
    let mut cur = h;
    let latch;
    loop {
        match &f.blocks[cur].term {
            Term::Br(t) => {
                let t = *t as usize;
                if t == h {
                    latch = cur;
                    break;
                }
                if blocks.contains(&t) || preds[t].len() != 1 || blocks.len() >= MAX_CHAIN {
                    return None;
                }
                blocks.push(t);
                cur = t;
            }
            Term::BrCond { cond, then_bb, else_bb } => {
                let (t, e) = (*then_bb as usize, *else_bb as usize);
                if t == h && !blocks.contains(&e) && e != h {
                    tests.push((cur, *cond, true));
                    latch = cur;
                    break;
                }
                if !blocks.contains(&t) && t != h && preds[t].len() == 1 && !blocks.contains(&e) && e != h && blocks.len() < MAX_CHAIN {
                    tests.push((cur, *cond, false));
                    blocks.push(t);
                    cur = t;
                } else {
                    return None;
                }
            }
            _ => return None,
        }
    }
    if tests.is_empty() {
        return None;
    }
    let p = *preds[h].iter().find(|&&x| x != latch)?;
    if blocks.contains(&p) || !preds[h].contains(&latch) {
        return None;
    }
    if !matches!(f.blocks[p].term, Term::Br(t) if t as usize == h) {
        return None;
    }
    Some(Chain { blocks, p, tests })
}

fn analyze(f: &Func, preds: &[Vec<usize>], h: usize) -> Option<Plan> {
    let ch = find_chain(f, preds, h)?;
    let consts = const_map(f);
    let npar = f.params.len();
    let mut defs: HashMap<Val, (usize, usize)> = HashMap::default();
    for (pos, &b) in ch.blocks.iter().enumerate() {
        for (ii, i) in f.blocks[b].insts.iter().enumerate() {
            if let Some(d) = i.dst {
                defs.insert(d, (pos, ii));
            }
            if pos > 0 && matches!(i.op, Op::Phi { .. }) {
                return None;
            }
        }
    }
    let latch = *ch.blocks.last().unwrap();
    let mut cls: HashMap<Val, Cls> = HashMap::default();
    let mut ivs: Vec<IvPhi> = Vec::new();
    let mut accs: Vec<Acc> = Vec::new();
    let mut phis: Vec<(Val, FTy, Val, Val)> = Vec::new();
    for i in &f.blocks[h].insts {
        if let Op::Phi { incoming } = &i.op {
            if incoming.len() != 2 {
                return None;
            }
            let init = incoming.iter().find(|(b, _)| *b as usize == ch.p)?.1;
            let back = incoming.iter().find(|(b, _)| *b as usize == latch)?.1;
            phis.push((i.dst?, i.ty, init, back));
        }
    }
    let mut s: Option<i128> = None;
    for &(phi, ty, init, back) in &phis {
        if ty != FTy::U64 {
            continue;
        }
        let k = match consts.get(&init) {
            Some(k) => *k,
            None => continue,
        };
        let is_inc = match defs.get(&back) {
            Some(&(pos, ii)) => match &f.blocks[ch.blocks[pos]].insts[ii].op {
                Op::Bin(BinOp::Add, a, b) => {
                    (*a == phi && consts.get(b) == Some(&1)) || (*b == phi && consts.get(a) == Some(&1))
                }
                _ => false,
            },
            None => false,
        };
        if !is_inc {
            continue;
        }
        let base = *s.get_or_insert(k);
        cls.insert(phi, Cls::Iv(k - base));
        ivs.push(IvPhi { phi, init });
    }
    let s = s?;
    let mut changed = true;
    while changed {
        changed = false;
        for &(phi, ty, init, back) in &phis {
            if cls.contains_key(&phi) || ty != FTy::U64 {
                continue;
            }
            if let (Some(Cls::Iv(cb)), Some(&k)) = (cls.get(&back).copied(), consts.get(&init)) {
                if k - s == cb - 1 {
                    cls.insert(phi, Cls::Iv(cb - 1));
                    ivs.push(IvPhi { phi, init });
                    changed = true;
                }
            }
        }
    }
    let mut acc_back: HashMap<Val, usize> = HashMap::default();
    for &(phi, ty, init, back) in &phis {
        if cls.contains_key(&phi) {
            continue;
        }
        elem_bits(ty)?;
        let (pos, ii) = *defs.get(&back)?;
        let op = match &f.blocks[ch.blocks[pos]].insts[ii].op {
            Op::Bin(op, a, b) if acc_op(*op) && (*a == phi || *b == phi) && *a != *b => *op,
            _ => return None,
        };
        if defs.contains_key(&init) {
            return None;
        }
        cls.insert(phi, Cls::Acc);
        acc_back.insert(back, accs.len());
        accs.push(Acc { phi, init, op, ty });
    }
    let mut counts: HashMap<Val, usize> = HashMap::default();
    let mut buf = Vec::new();
    for &b in &ch.blocks {
        for i in &f.blocks[b].insts {
            if matches!(i.op, Op::Phi { .. }) {
                continue;
            }
            buf.clear();
            i.op.uses(&mut buf);
            for u in &buf {
                *counts.entry(*u).or_insert(0) += 1;
            }
        }
    }
    let cond_set: HashMap<Val, (usize, bool)> = ch.tests.iter().map(|&(b, c, r)| (c, (b, r))).collect();
    let pos_of_block: HashMap<usize, usize> = ch.blocks.iter().enumerate().map(|(i, &b)| (b, i)).collect();

    let mut keys: Vec<Key> = Vec::new();
    let mut steps: Vec<Step> = Vec::new();
    let mut store_keys: Vec<usize> = Vec::new();
    let mut tests: Vec<Test> = Vec::new();
    let mut inv_loads: Vec<Val> = Vec::new();
    let mut rep: HashMap<Val, Val> = HashMap::default();
    let mut load_canon: HashMap<(Val, u32, bool), Val> = HashMap::default();
    let mut ebits: Option<u32> = None;
    let mut set_bits = |ty: FTy, ebits: &mut Option<u32>| -> Option<()> {
        let b = elem_bits(ty)?;
        if let Some(e) = *ebits {
            if e != b {
                return None;
            }
        }
        *ebits = Some(b);
        Some(())
    };
    let canon = |rep: &HashMap<Val, Val>, v: Val| -> Val { rep.get(&v).copied().unwrap_or(v) };
    let get = |cls: &HashMap<Val, Cls>, v: Val| -> Cls {
        if (v as usize) < npar {
            return Cls::Inv;
        }
        cls.get(&v).copied().unwrap_or(Cls::Inv)
    };
    for (pos, &b) in ch.blocks.iter().enumerate() {
        for i in &f.blocks[b].insts {
            let d = match i.dst {
                Some(d) => d,
                None => {
                    if let Op::Store { addr, val } = &i.op {
                        let ak = get(&cls, *addr);
                        let vc = get(&cls, *val);
                        let (base, c, e) = match ak {
                            Cls::Addr(b, c, e) => (b, c, e),
                            _ => return None,
                        };
                        if !matches!(vc, Cls::Vec | Cls::Inv) {
                            return None;
                        }
                        let vty = f.val_types.get(*val as usize).copied()?;
                        set_bits(vty, &mut ebits)?;
                        if i.ty != vty || u64::from(elem_bits(vty)? / 8) != e {
                            return None;
                        }
                        let k = intern(&mut keys, Key { base, c, e });
                        store_keys.push(k);
                        steps.push(Step::Store { key: k, val: *val });
                        continue;
                    }
                    return None;
                }
            };
            match &i.op {
                Op::Phi { .. } => {}
                Op::Const(_) => {
                    cls.insert(d, Cls::Inv);
                }
                Op::Cmp { op: CmpOp::Lt, ty: FTy::U64, a, b } if cond_set.contains_key(&d) => {
                    let e = match get(&cls, *a) {
                        Cls::Iv(e) => e,
                        _ => return None,
                    };
                    if get(&cls, *b) != Cls::Inv {
                        return None;
                    }
                    let (tb, r) = cond_set[&d];
                    if pos_of_block[&tb] != pos {
                        return None;
                    }
                    if let Some(&(npos, _)) = defs.get(b) {
                        if npos > pos {
                            return None;
                        }
                    }
                    let nn = canon(&rep, *b);
                    tests.push(Test { pos, n: nn, e, r });
                }
                Op::Bin(op, a, bb) => {
                    let (ca, cb) = (get(&cls, *a), get(&cls, *bb));
                    match (op, ca, cb) {
                        (_, Cls::Inv, Cls::Inv) if i.ty != FTy::V128 => {
                            cls.insert(d, Cls::Inv);
                        }
                        (BinOp::Add, Cls::Iv(c), Cls::Inv) | (BinOp::Add, Cls::Inv, Cls::Iv(c)) if i.ty == FTy::U64 => {
                            let o = if matches!(ca, Cls::Iv(_)) { bb } else { a };
                            let k = *consts.get(o)?;
                            cls.insert(d, Cls::Iv(c + k));
                        }
                        (BinOp::Sub, Cls::Iv(c), Cls::Inv) if i.ty == FTy::U64 => {
                            let k = *consts.get(bb)?;
                            cls.insert(d, Cls::Iv(c - k));
                        }
                        (BinOp::Mul, Cls::Iv(c), Cls::Inv) if i.ty == FTy::U64 => {
                            let k = *consts.get(bb)?;
                            if k <= 0 || k > 64 {
                                return None;
                            }
                            cls.insert(d, Cls::Idx(c, k as u64));
                        }
                        (_, Cls::Acc, x) | (_, x, Cls::Acc) if acc_back.contains_key(&d) => {
                            if x != Cls::Vec || !acc_op(*op) {
                                return None;
                            }
                            let xv = if ca == Cls::Acc { *bb } else { *a };
                            let ai = acc_back[&d];
                            if accs[ai].ty != i.ty || accs[ai].op != *op {
                                return None;
                            }
                            set_bits(i.ty, &mut ebits)?;
                            cls.insert(d, Cls::AccNext);
                            steps.push(Step::AccUpd { acc: ai, x: xv });
                        }
                        (BinOp::Add | BinOp::Xor | BinOp::And | BinOp::Or, x, y)
                            if matches!(x, Cls::Vec | Cls::Inv) && matches!(y, Cls::Vec | Cls::Inv) =>
                        {
                            set_bits(i.ty, &mut ebits)?;
                            cls.insert(d, Cls::Vec);
                            steps.push(Step::Bin { dst: d, op: *op, a: *a, b: *bb });
                        }
                        (BinOp::Sub, x, y)
                            if matches!(x, Cls::Vec | Cls::Inv)
                                && matches!(y, Cls::Vec | Cls::Inv)
                                && elem_bits(i.ty) == Some(32) =>
                        {
                            set_bits(i.ty, &mut ebits)?;
                            cls.insert(d, Cls::Vec);
                            steps.push(Step::Bin { dst: d, op: *op, a: *a, b: *bb });
                        }
                        _ => return None,
                    }
                }
                Op::PtrAdd { base, off } => match (get(&cls, *base), get(&cls, *off)) {
                    (Cls::Inv, Cls::Idx(c, e)) => {
                        cls.insert(d, Cls::Addr(canon(&rep, *base), c, e));
                    }
                    (Cls::Inv, Cls::Inv) => {
                        cls.insert(d, Cls::Inv);
                    }
                    _ => return None,
                },
                Op::Load { addr } => match get(&cls, *addr) {
                    Cls::Addr(base, c, e) => {
                        set_bits(i.ty, &mut ebits)?;
                        if u64::from(elem_bits(i.ty)? / 8) != e {
                            return None;
                        }
                        let k = intern(&mut keys, Key { base, c, e });
                        cls.insert(d, Cls::Vec);
                        steps.push(Step::Load { dst: d, key: k });
                    }
                    Cls::Inv => {
                        let ca = canon(&rep, *addr);
                        match load_canon.get(&(ca, i.ty.bits(), i.ty.signed())) {
                            Some(&r) => {
                                rep.insert(d, r);
                            }
                            None => {
                                load_canon.insert((ca, i.ty.bits(), i.ty.signed()), d);
                                inv_loads.push(d);
                            }
                        }
                        cls.insert(d, Cls::Inv);
                    }
                    _ => return None,
                },
                _ => return None,
            }
        }
    }
    let ebits = ebits?;
    if tests.len() != ch.tests.len() || steps.is_empty() || store_keys.is_empty() && accs.is_empty() {
        return None;
    }
    for (ai, a) in accs.iter().enumerate() {
        if elem_bits(a.ty)? != ebits {
            return None;
        }
        if counts.get(&a.phi).copied().unwrap_or(0) != 1 {
            return None;
        }
        let upd = steps.iter().find_map(|st| match st {
            Step::AccUpd { acc, .. } if *acc == ai => Some(()),
            _ => None,
        });
        upd?;
    }
    for (&back, _) in acc_back.iter() {
        if counts.get(&back).copied().unwrap_or(0) != 0 {
            return None;
        }
    }
    for &(phi, _, _, back) in &phis {
        if let Some(Cls::Acc) = cls.get(&phi) {
            if cls.get(&back) != Some(&Cls::AccNext) {
                return None;
            }
        }
    }
    for k in 0..keys.len() {
        for j in 0..keys.len() {
            if k != j && keys[k].base == keys[j].base && (store_keys.contains(&k) || store_keys.contains(&j)) {
                return None;
            }
        }
    }
    let mut seen = HashSet::default();
    for t in &tests {
        let thr = s + t.e - if t.r { 1 } else { 0 };
        if thr < 0 || thr > (1 << 31) {
            return None;
        }
        seen.insert(t.pos);
    }
    for iv in &ivs {
        if !consts.contains_key(&iv.init) {
            return None;
        }
    }
    Some(Plan {
        h,
        p: ch.p,
        chain: ch.blocks,
        s,
        ivs,
        tests,
        accs,
        steps,
        keys,
        store_keys,
        inv_loads,
        ebits,
        rep,
        defs,
        consts,
        cls,
    })
}

fn intern(keys: &mut Vec<Key>, k: Key) -> usize {
    if let Some(i) = keys.iter().position(|x| *x == k) {
        return i;
    }
    keys.push(k);
    keys.len() - 1
}
