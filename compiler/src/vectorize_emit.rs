// SPDX-License-Identifier: MPL-2.0

use crate::fir::{BinOp, BlockId, CmpOp, FTy, Func, Inst, Op, Term, Val};
use crate::fx::HashMap;
use crate::simd::SimdKind;
use crate::vectorize::{Plan, Step, UNROLL};

struct Em<'a> {
    f: &'a mut Func,
    cur: BlockId,
    memo: HashMap<Val, Val>,
    fails: Vec<BlockId>,
    h: BlockId,
}

impl<'a> Em<'a> {
    fn op(&mut self, ty: FTy, op: Op) -> Val {
        self.f.push(self.cur, ty, op)
    }

    fn konst(&mut self, ty: FTy, k: i128) -> Val {
        self.op(ty, Op::Const(k))
    }

    fn simd(&mut self, kind: SimdKind, args: Vec<Val>, imm: u8, ty: FTy) -> Val {
        self.op(ty, Op::Simd { kind, args, imm })
    }

    fn branch(&mut self, cond: Val, then_bb: BlockId, else_bb: BlockId) {
        self.f.blocks[self.cur as usize].term = Term::BrCond { cond, then_bb, else_bb };
        if else_bb == self.h || then_bb == self.h {
            self.fails.push(self.cur);
        }
    }

    fn jump(&mut self, to: BlockId) {
        self.f.blocks[self.cur as usize].term = Term::Br(to);
    }

    fn mat(&mut self, plan: &Plan, v: Val) -> Option<Val> {
        let c = plan.rep.get(&v).copied().unwrap_or(v);
        if let Some(&m) = self.memo.get(&c) {
            return Some(m);
        }
        let (pos, ii) = match plan.defs.get(&c) {
            Some(&d) => d,
            None => return Some(c),
        };
        let inst: Inst = self.f.blocks[plan.chain[pos]].insts[ii].clone();
        let out = match &inst.op {
            Op::Const(k) => self.konst(inst.ty, *k),
            Op::Load { addr } => {
                let a = self.mat(plan, *addr)?;
                self.op(inst.ty, Op::Load { addr: a })
            }
            Op::PtrAdd { base, off } => {
                let b = self.mat(plan, *base)?;
                let o = self.mat(plan, *off)?;
                self.op(inst.ty, Op::PtrAdd { base: b, off: o })
            }
            Op::Bin(op, a, b) => {
                let x = self.mat(plan, *a)?;
                let y = self.mat(plan, *b)?;
                self.op(inst.ty, Op::Bin(*op, x, y))
            }
            Op::Cast { src, from } => {
                let x = self.mat(plan, *src)?;
                self.op(inst.ty, Op::Cast { src: x, from: *from })
            }
            _ => return None,
        };
        self.memo.insert(c, out);
        Some(out)
    }

    fn splat(&mut self, plan: &Plan, v: Val) -> Option<Val> {
        let c = plan.rep.get(&v).copied().unwrap_or(v);
        let ty = self.f.val_types[c as usize];
        let lane = if let Some(k) = plan.consts.get(&c) {
            let m = if plan.ebits == 32 { (*k as u64) & 0xffff_ffff } else { *k as u64 };
            let w = if plan.ebits == 32 { m | (m << 32) } else { m };
            self.konst(FTy::U64, w as i128)
        } else {
            let x = self.mat(plan, c)?;
            let u = if ty == FTy::U64 { x } else { self.op(FTy::U64, Op::Cast { src: x, from: ty }) };
            if plan.ebits == 32 {
                let mask = self.konst(FTy::U64, 0xffff_ffff);
                let lo = self.op(FTy::U64, Op::Bin(BinOp::And, u, mask));
                let sh = self.konst(FTy::U64, 32);
                let hi = self.op(FTy::U64, Op::Bin(BinOp::Shl, lo, sh));
                self.op(FTy::U64, Op::Bin(BinOp::Or, lo, hi))
            } else {
                u
            }
        };
        Some(self.simd(SimdKind::FromU64, vec![lane, lane], 0, FTy::V128))
    }

    fn lane_vec(&mut self, plan: &Plan, init: Val, ty: FTy, op: BinOp) -> Option<Val> {
        let ident: u64 = if op == BinOp::And { u64::MAX } else { 0 };
        let x = self.mat(plan, init)?;
        let u = if ty == FTy::U64 { x } else { self.op(FTy::U64, Op::Cast { src: x, from: ty }) };
        if plan.ebits == 32 {
            let mask = self.konst(FTy::U64, 0xffff_ffff);
            let lo = self.op(FTy::U64, Op::Bin(BinOp::And, u, mask));
            let hi_c = self.konst(FTy::U64, (ident & 0xffff_ffff_0000_0000) as i128);
            let first = self.op(FTy::U64, Op::Bin(BinOp::Or, lo, hi_c));
            let rest = self.konst(FTy::U64, ident as i128);
            Some(self.simd(SimdKind::FromU64, vec![first, rest], 0, FTy::V128))
        } else {
            let rest = self.konst(FTy::U64, ident as i128);
            Some(self.simd(SimdKind::FromU64, vec![u, rest], 0, FTy::V128))
        }
    }

    fn ident_vec(&mut self, op: BinOp) -> Val {
        if op == BinOp::And {
            let m = self.konst(FTy::U64, u64::MAX as i128);
            self.simd(SimdKind::FromU64, vec![m, m], 0, FTy::V128)
        } else {
            self.simd(SimdKind::Zero, vec![], 0, FTy::V128)
        }
    }
}

enum Cnd {
    Le(Val, Val),
    Eq(Val, Val),
}

fn any_of(em: &mut Em, alts: &[Cnd]) {
    let ok = em.f.add_block();
    for (i, c) in alts.iter().enumerate() {
        let cond = match c {
            Cnd::Le(a, b) => em.op(FTy::Bool, Op::Cmp { op: CmpOp::Le, ty: FTy::U64, a: *a, b: *b }),
            Cnd::Eq(a, b) => em.op(FTy::Bool, Op::Cmp { op: CmpOp::Eq, ty: FTy::U64, a: *a, b: *b }),
        };
        if i + 1 < alts.len() {
            let nx = em.f.add_block();
            em.branch(cond, ok, nx);
            em.cur = nx;
        } else {
            let h = em.h;
            em.branch(cond, ok, h);
        }
    }
    em.cur = ok;
}

fn simd_kind(op: BinOp, ebits: u32) -> Option<SimdKind> {
    Some(match (op, ebits) {
        (BinOp::Add, 64) => SimdKind::Add64,
        (BinOp::Add, 32) => SimdKind::Add32,
        (BinOp::Sub, 32) => SimdKind::Sub32,
        (BinOp::Xor, _) => SimdKind::Xor,
        (BinOp::And, _) => SimdKind::And,
        (BinOp::Or, _) => SimdKind::Or,
        _ => return None,
    })
}

pub(crate) fn apply(f: &mut Func, plan: &Plan) -> bool {
    let lanes = 128 / plan.ebits;
    let chunk = (lanes * UNROLL) as u64;
    let shift = chunk.trailing_zeros() as i128;
    let nb_before = f.blocks.len();
    let nv_before = f.val_types.len();
    let h = plan.h as BlockId;
    let first = f.add_block();
    let mut em = Em { f, cur: first, memo: HashMap::default(), fails: Vec::new(), h };
    let ok = (|| -> Option<(BlockId, Val)> {
        let mut lim: Option<Val> = None;
        for t in &plan.tests {
            let n = em.mat(plan, t.n)?;
            let thr_k = plan.s + t.e - if t.r { 1 } else { 0 };
            let thr = em.konst(t.ty, thr_k);
            let cond = em.op(FTy::Bool, Op::Cmp { op: CmpOp::Gt, ty: t.ty, a: n, b: thr });
            let dn = em.op(t.ty, Op::Bin(BinOp::Sub, n, thr));
            let d = if t.ty == FTy::U64 { dn } else { em.op(FTy::U64, Op::Cast { src: dn, from: t.ty }) };
            lim = Some(match lim {
                None => d,
                Some(l) => {
                    let lt = em.op(FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::U64, a: d, b: l });
                    em.op(FTy::U64, Op::Select { cond: lt, a: d, b: l })
                }
            });
            let next = em.f.add_block();
            em.branch(cond, next, h);
            em.cur = next;
        }
        let lim = lim?;
        let one = em.konst(FTy::U64, 1);
        let lm1 = em.op(FTy::U64, Op::Bin(BinOp::Sub, lim, one));
        let sh = em.konst(FTy::U64, shift);
        let vi = em.op(FTy::U64, Op::Bin(BinOp::Shr, lm1, sh));
        let zero = em.konst(FTy::U64, 0);
        let nz = em.op(FTy::Bool, Op::Cmp { op: CmpOp::Ne, ty: FTy::U64, a: vi, b: zero });
        let pc2 = em.f.add_block();
        em.branch(nz, pc2, h);
        em.cur = pc2;
        let cov = em.op(FTy::U64, Op::Bin(BinOp::Shl, vi, sh));

        let mut pinit: Vec<Val> = Vec::new();
        for k in &plan.keys {
            let b = em.mat(plan, k.base)?;
            let off = em.konst(FTy::I64, (plan.s + k.c) * k.e as i128);
            pinit.push(em.op(FTy::Ptr, Op::PtrAdd { base: b, off }));
        }
        let mut lo: Vec<Val> = Vec::new();
        let mut hi: Vec<Val> = Vec::new();
        for (i, k) in plan.keys.iter().enumerate() {
            let l = em.op(FTy::U64, Op::Cast { src: pinit[i], from: FTy::Ptr });
            let ek = em.konst(FTy::U64, k.e as i128);
            let span = em.op(FTy::U64, Op::Bin(BinOp::Mul, cov, ek));
            let hh = em.op(FTy::U64, Op::Bin(BinOp::Add, l, span));
            lo.push(l);
            hi.push(hh);
        }
        for (si, &sk) in plan.store_keys.iter().enumerate() {
            if plan.store_keys[..si].contains(&sk) {
                continue;
            }
            for ok in 0..plan.keys.len() {
                if ok == sk || (plan.store_keys.contains(&ok) && ok < sk) {
                    continue;
                }
                let mut alts = Vec::new();
                if plan.keys[sk].e == plan.keys[ok].e {
                    alts.push(Cnd::Eq(lo[sk], lo[ok]));
                }
                alts.push(Cnd::Le(hi[sk], lo[ok]));
                alts.push(Cnd::Le(hi[ok], lo[sk]));
                any_of(&mut em, &alts);
            }
            for &il in &plan.inv_loads {
                let sz = (em.f.val_types[il as usize].bits() / 8).max(1) as i128;
                let (pos, ii) = *plan.defs.get(&il)?;
                let addr = match &em.f.blocks[plan.chain[pos]].insts[ii].op {
                    Op::Load { addr } => *addr,
                    _ => return None,
                };
                let a = em.mat(plan, addr)?;
                let au = em.op(FTy::U64, Op::Cast { src: a, from: FTy::Ptr });
                let szc = em.konst(FTy::U64, sz);
                let ae = em.op(FTy::U64, Op::Bin(BinOp::Add, au, szc));
                any_of(&mut em, &[Cnd::Le(ae, lo[sk]), Cnd::Le(hi[sk], au)]);
            }
        }

        let mut splats: HashMap<Val, Val> = HashMap::default();
        let mut need_splat = |v: Val, em: &mut Em, splats: &mut HashMap<Val, Val>| -> Option<()> {
            let c = plan.rep.get(&v).copied().unwrap_or(v);
            if plan.cls.get(&c) == Some(&crate::vectorize::Cls::Vec) {
                return Some(());
            }
            if !splats.contains_key(&c) {
                let s = em.splat(plan, c)?;
                splats.insert(c, s);
            }
            Some(())
        };
        for st in &plan.steps {
            match st {
                Step::Bin { a, b, .. } => {
                    need_splat(*a, &mut em, &mut splats)?;
                    need_splat(*b, &mut em, &mut splats)?;
                }
                Step::Store { val, .. } => need_splat(*val, &mut em, &mut splats)?,
                _ => {}
            }
        }
        let mut acc_inits: Vec<Val> = Vec::new();
        let mut acc_idents: Vec<Val> = Vec::new();
        for a in &plan.accs {
            acc_inits.push(em.lane_vec(plan, a.init, a.ty, a.op)?);
            acc_idents.push(em.ident_vec(a.op));
        }
        let pre = em.cur;
        let vl = em.f.add_block();
        em.jump(vl);
        em.cur = vl;
        let cnt = em.f.new_val_pub(FTy::U64);
        let mut pphi: Vec<Val> = Vec::new();
        for _ in &plan.keys {
            pphi.push(em.f.new_val_pub(FTy::Ptr));
        }
        let mut aphi: Vec<Vec<Val>> = Vec::new();
        for _ in &plan.accs {
            aphi.push((0..UNROLL).map(|_| em.f.new_val_pub(FTy::V128)).collect());
        }
        let mut anext: Vec<Vec<Val>> = vec![Vec::new(); plan.accs.len()];
        let mut vmap: HashMap<(Val, u32), Val> = HashMap::default();
        let forward = std::env::var_os("FIRN_NO_VEC_FORWARD").is_none();
        let mut known: HashMap<(usize, u32), Val> = HashMap::default();
        for j in 0..UNROLL {
            for st in &plan.steps {
                match st {
                    Step::Load { dst, key } => {
                        if let Some(&v) = known.get(&(*key, j)) {
                            vmap.insert((*dst, j), v);
                            continue;
                        }
                        let ptr = if j == 0 {
                            pphi[*key]
                        } else {
                            let o = em.konst(FTy::I64, (j * 16) as i128);
                            em.op(FTy::Ptr, Op::PtrAdd { base: pphi[*key], off: o })
                        };
                        let v = em.simd(SimdKind::Load, vec![ptr], 0, FTy::V128);
                        vmap.insert((*dst, j), v);
                        if forward {
                            known.insert((*key, j), v);
                        }
                    }
                    Step::Bin { dst, op, a, b } => {
                        let ty = em.f.val_types[*dst as usize];
                        let _ = ty;
                        let va = operand(&vmap, &splats, plan, *a, j)?;
                        let vb = operand(&vmap, &splats, plan, *b, j)?;
                        let kind = simd_kind(*op, plan.ebits)?;
                        let v = em.simd(kind, vec![va, vb], 0, FTy::V128);
                        vmap.insert((*dst, j), v);
                    }
                    Step::Store { key, val } => {
                        let ptr = if j == 0 {
                            pphi[*key]
                        } else {
                            let o = em.konst(FTy::I64, (j * 16) as i128);
                            em.op(FTy::Ptr, Op::PtrAdd { base: pphi[*key], off: o })
                        };
                        let v = operand(&vmap, &splats, plan, *val, j)?;
                        if forward {
                            let e = plan.keys[*key].e;
                            known.retain(|&(k, kj), _| kj != j || k == *key || plan.keys[k].e != e);
                            known.insert((*key, j), v);
                        }
                        let loc = em.f.loc_stamp;
                        em.f.blocks[vl as usize].insts.push(Inst {
                            dst: None,
                            ty: FTy::Void,
                            op: Op::Simd { kind: SimdKind::Store, args: vec![ptr, v], imm: 0 },
                            loc,
                        });
                    }
                    Step::AccUpd { acc, x } => {
                        let vx = operand(&vmap, &splats, plan, *x, j)?;
                        let kind = simd_kind(plan.accs[*acc].op, plan.ebits)?;
                        let n = em.simd(kind, vec![aphi[*acc][j as usize], vx], 0, FTy::V128);
                        anext[*acc].push(n);
                    }
                }
            }
        }
        let mut pnext: Vec<Val> = Vec::new();
        for (i, k) in plan.keys.iter().enumerate() {
            let o = em.konst(FTy::I64, (chunk * k.e) as i128);
            pnext.push(em.op(FTy::Ptr, Op::PtrAdd { base: pphi[i], off: o }));
        }
        let one = em.konst(FTy::U64, 1);
        let cnt_next = em.op(FTy::U64, Op::Bin(BinOp::Sub, cnt, one));
        let zero = em.konst(FTy::U64, 0);
        let again = em.op(FTy::Bool, Op::Cmp { op: CmpOp::Ne, ty: FTy::U64, a: cnt_next, b: zero });
        let vx = em.f.add_block();
        em.f.blocks[vl as usize].term = Term::BrCond { cond: again, then_bb: vl, else_bb: vx };
        let mut phis: Vec<Inst> = Vec::new();
        phis.push(Inst::new(Some(cnt), FTy::U64, Op::Phi { incoming: vec![(pre, vi), (vl, cnt_next)] }));
        for (i, p) in pphi.iter().enumerate() {
            phis.push(Inst::new(Some(*p), FTy::Ptr, Op::Phi { incoming: vec![(pre, pinit[i]), (vl, pnext[i])] }));
        }
        for (ai, ps) in aphi.iter().enumerate() {
            for (j, p) in ps.iter().enumerate() {
                let init = if j == 0 { acc_inits[ai] } else { acc_idents[ai] };
                phis.push(Inst::new(Some(*p), FTy::V128, Op::Phi { incoming: vec![(pre, init), (vl, anext[ai][j])] }));
            }
        }
        let body = std::mem::take(&mut em.f.blocks[vl as usize].insts);
        em.f.blocks[vl as usize].insts = phis.into_iter().chain(body).collect();

        em.cur = vx;
        em.jump(h);
        let mut new_vals: HashMap<Val, Val> = HashMap::default();
        for iv in &plan.ivs {
            let init = em.mat(plan, iv.init)?;
            let step = if plan.ity == FTy::U64 { cov } else { em.op(plan.ity, Op::Cast { src: cov, from: FTy::U64 }) };
            let nv = em.op(plan.ity, Op::Bin(BinOp::Add, init, step));
            new_vals.insert(iv.phi, nv);
        }
        for (ai, a) in plan.accs.iter().enumerate() {
            let kind = simd_kind(a.op, plan.ebits)?;
            let mut v = anext[ai][0];
            for &x in &anext[ai][1..] {
                v = em.simd(kind, vec![v, x], 0, FTy::V128);
            }
            let (lane_n, lane_k, lty) =
                if plan.ebits == 64 { (2u8, SimdKind::GetU64, FTy::U64) } else { (4u8, SimdKind::GetU32, FTy::U32) };
            let mut r = em.simd(lane_k, vec![v], 0, lty);
            for l in 1..lane_n {
                let x = em.simd(lane_k, vec![v], l, lty);
                r = em.op(lty, Op::Bin(a.op, r, x));
            }
            if lty != a.ty {
                r = em.op(a.ty, Op::Cast { src: r, from: lty });
            }
            new_vals.insert(a.phi, r);
        }
        Some((vx, 0)).map(|(b, _)| {
            let mut fails = std::mem::take(&mut em.fails);
            fails.sort();
            fails.dedup();
            let p = plan.p as BlockId;
            let phi_ids: Vec<usize> = (0..em.f.blocks[plan.h].insts.len())
                .filter(|&i| matches!(em.f.blocks[plan.h].insts[i].op, Op::Phi { .. }))
                .collect();
            for i in phi_ids {
                let dst = em.f.blocks[plan.h].insts[i].dst;
                if let Op::Phi { incoming } = &mut em.f.blocks[plan.h].insts[i].op {
                    let pos = incoming.iter().position(|(bb, _)| *bb == p);
                    if let Some(pos) = pos {
                        let (_, init) = incoming.remove(pos);
                        for fb in &fails {
                            incoming.push((*fb, init));
                        }
                        if let Some(nv) = dst.and_then(|d| new_vals.get(&d)) {
                            incoming.push((b, *nv));
                        }
                    }
                }
            }
            (b, 0)
        })
    })();
    if ok.is_none() {
        em.f.blocks.truncate(nb_before);
        em.f.val_types.truncate(nv_before);
        let p = plan.p;
        em.f.blocks[p].term = Term::Br(h);
        return false;
    }
    let p = plan.p;
    em.f.blocks[p].term = Term::Br(first);
    true
}

fn operand(
    vmap: &HashMap<(Val, u32), Val>,
    splats: &HashMap<Val, Val>,
    plan: &Plan,
    v: Val,
    j: u32,
) -> Option<Val> {
    let c = plan.rep.get(&v).copied().unwrap_or(v);
    if let Some(&x) = vmap.get(&(v, j)) {
        return Some(x);
    }
    splats.get(&c).copied()
}
