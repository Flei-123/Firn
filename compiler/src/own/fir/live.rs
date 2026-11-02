use super::flow::{Borrows, CellVal, Flow};
use super::registry::Drop;
use super::index::{term_uses, Bits, Index};
use super::sites::Site;
use super::slots::Slots;
use crate::fir::{Func, Inst, Op, Val};
use crate::fx::HashMap;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Handle {
    Val(Val),
    Cell(usize),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Free {
    pub block: usize,
    pub pos: usize,
    pub handle: Handle,
    pub site: usize,
}

struct Items {
    val: HashMap<Val, usize>,
    cell: HashMap<usize, usize>,
    n: usize,
    valmask: Bits,
    touch: Vec<bool>,
}

struct Ctx<'a> {
    f: &'a Func,
    ix: &'a Index,
    sl: &'a Slots,
    seed: usize,
    site: usize,
    items: Items,
}

pub(crate) fn deaths(f: &Func, ix: &Index, sl: &Slots, fl: &mut Flow, br: &dyn Borrows) -> (Vec<Free>, Vec<bool>) {
    let mut out = Vec::new();
    let mut freed = vec![false; fl.sites.len()];
    for k in 0..fl.sites.len() {
        let seed = fl.seed_of_site(k);
        if fl.escaped.get(seed) || fl.ret_may.get(seed) {
            continue;
        }
        if let Some(v) = site_deaths(f, ix, sl, fl, br, k) {
            freed[k] = !v.is_empty();
            out.extend(v);
        }
    }
    (out, freed)
}

fn items_for(f: &Func, sl: &Slots, fl: &Flow, seed: usize, site: usize) -> Items {
    let mut val = HashMap::default();
    let mut n = 0;
    for (v, m) in fl.may.iter().enumerate() {
        if m.get(seed) {
            val.insert(v as Val, n);
            n += 1;
        }
    }
    let nv = n;
    let mut cell = HashMap::default();
    if let Site::Pair { cell_p, .. } = fl.sites[site] {
        cell.insert(cell_p, n);
        n += 1;
    }
    for b in f.blocks.iter() {
        for inst in b.insts.iter() {
            if let Op::Store { addr, val: sv } = &inst.op {
                if let Some(a) = sl.access(*addr, inst.ty) {
                    if fl.may[*sv as usize].get(seed) && !cell.contains_key(&a.cell) {
                        cell.insert(a.cell, n);
                        n += 1;
                    }
                }
            }
        }
    }
    let mut valmask = Bits::new(n);
    (0..nv).for_each(|i| valmask.set(i));
    let mut touch = vec![false; f.blocks.len()];
    let mut buf = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        buf.clear();
        term_uses(&b.term, &mut buf);
        let mut t = buf.iter().any(|v| val.contains_key(v));
        for (ii, inst) in b.insts.iter().enumerate() {
            if t {
                break;
            }
            buf.clear();
            inst.op.uses(&mut buf);
            t = inst.dst.map_or(false, |d| val.contains_key(&d)) || buf.iter().any(|v| val.contains_key(v)) || fl.site_at.contains_key(&(bi, ii));
            if !t {
                if let Op::Load { addr } | Op::Store { addr, .. } = &inst.op {
                    t = sl.access(*addr, inst.ty).map_or(false, |a| cell.contains_key(&a.cell));
                }
            }
        }
        touch[bi] = t;
    }
    Items { val, cell, n, valmask, touch }
}

impl<'a> Ctx<'a> {
    fn back(&self, fl: &Flow, inst: &Inst, at: (usize, usize), live: &mut Bits, u: &mut Vec<Val>) {
        if let Some(d) = inst.dst {
            if let Some(&i) = self.items.val.get(&d) {
                live.clear(i);
            }
        }
        match &inst.op {
            Op::Store { addr, .. } => {
                if let Some(a) = self.sl.access(*addr, inst.ty) {
                    if a.strong {
                        if let Some(&i) = self.items.cell.get(&a.cell) {
                            live.clear(i);
                        }
                    }
                }
            }
            Op::Load { addr } => {
                if let Some(a) = self.sl.access(*addr, inst.ty) {
                    if let Some(&i) = self.items.cell.get(&a.cell) {
                        live.set(i);
                    }
                }
            }
            Op::Call { .. } => {
                if let Some(&k) = fl.site_at.get(&at) {
                    if let Site::Pair { cell_p, cell_n } = fl.sites[k] {
                        for c in [cell_p, cell_n] {
                            if let Some(&i) = self.items.cell.get(&c) {
                                live.clear(i);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        if matches!(inst.op, Op::Phi { .. }) {
            return;
        }
        u.clear();
        inst.op.uses(u);
        for v in u.iter() {
            if let Some(&i) = self.items.val.get(v) {
                live.set(i);
            }
        }
    }

    fn carried(&self, live: &Bits, cells: &[CellVal]) -> bool {
        if live.intersects(&self.items.valmask) {
            return true;
        }
        self.items.cell.iter().any(|(&c, &i)| live.get(i) && cells[c].may.get(self.seed))
    }

    fn must_cell(&self, cells: &[CellVal], c: usize) -> bool {
        let x = &cells[c];
        !x.derived && x.may.count() == 1 && x.may.get(self.seed)
    }

    fn live_in(&self, fl: &Flow, bi: usize, out: &Bits) -> Bits {
        let mut live = out.clone();
        if !self.items.touch[bi] {
            return live;
        }
        let b = &self.f.blocks[bi];
        let mut u = Vec::new();
        term_uses(&b.term, &mut u);
        for v in u {
            if let Some(&i) = self.items.val.get(&v) {
                live.set(i);
            }
        }
        let mut buf = Vec::new();
        for (ii, inst) in b.insts.iter().enumerate().rev() {
            self.back(fl, inst, (bi, ii), &mut live, &mut buf);
        }
        live
    }

    fn live_out(&self, bi: usize, ins: &[Bits]) -> Bits {
        let mut out = Bits::new(self.items.n);
        let me = self.f.blocks[bi].id;
        for &s in self.ix.succs[bi].iter() {
            out.union(&ins[s]);
            for inst in self.f.blocks[s].insts.iter() {
                if let Op::Phi { incoming } = &inst.op {
                    for (p, v) in incoming.iter() {
                        if *p == me {
                            if let Some(&i) = self.items.val.get(v) {
                                out.set(i);
                            }
                        }
                    }
                } else {
                    break;
                }
            }
        }
        out
    }
}

fn site_deaths(f: &Func, ix: &Index, sl: &Slots, fl: &mut Flow, br: &dyn Borrows, site: usize) -> Option<Vec<Free>> {
    let seed = fl.seed_of_site(site);
    let items = items_for(f, sl, fl, seed, site);
    let cx = Ctx { f, ix, sl, seed, site, items };
    let nb = f.blocks.len();
    let mut ins = vec![Bits::new(cx.items.n); nb];
    let mut outs = vec![Bits::new(cx.items.n); nb];
    let mut rounds = 0;
    loop {
        rounds += 1;
        let mut ch = false;
        for &bi in ix.rpo.iter().rev() {
            let o = cx.live_out(bi, &ins);
            let i = cx.live_in(fl, bi, &o);
            if i != ins[bi] || o != outs[bi] {
                ins[bi] = i;
                outs[bi] = o;
                ch = true;
            }
        }
        if !ch {
            break;
        }
        if rounds > 500 {
            return None;
        }
    }
    let mut frees = Vec::new();
    let mut ends: Vec<Option<Vec<CellVal>>> = vec![None; nb];
    let mut buf = Vec::new();
    for &bi in ix.rpo.iter() {
        if !cx.items.touch[bi] {
            continue;
        }
        let cin = fl.cin[bi].clone()?;
        let b = &f.blocks[bi];
        let mut states: Vec<Vec<CellVal>> = Vec::with_capacity(b.insts.len() + 1);
        let mut cur = cin;
        states.push(cur.clone());
        for (ii, inst) in b.insts.iter().enumerate() {
            fl.transfer(f, sl, br, &mut cur, inst, (bi, ii), false);
            states.push(cur.clone());
        }
        ends[bi] = Some(cur);
        let mut live = outs[bi].clone();
        let mut u = Vec::new();
        term_uses(&b.term, &mut u);
        for v in u {
            if let Some(&i) = cx.items.val.get(&v) {
                live.set(i);
            }
        }
        let pc = b.phi_count();
        for ii in (pc..b.insts.len()).rev() {
            let inst = &b.insts[ii];
            let after = cx.carried(&live, &states[ii + 1]);
            let mut before_live = live.clone();
            cx.back(fl, inst, (bi, ii), &mut before_live, &mut buf);
            let before = cx.carried(&before_live, &states[ii]);
            let is_site = fl.site_at.get(&(bi, ii)) == Some(&site);
            if is_site && before {
                let old = self_append_old(&cx, fl, inst)?;
                let mut rest = live.clone();
                if let Some(&c) = cx.items.cell.get(&cell_p_of(fl, site)?) {
                    rest.clear(c);
                }
                if cx.carried(&rest, &states[ii + 1]) {
                    return None;
                }
                frees.push(Free { block: bi, pos: ii + 1, handle: Handle::Val(old), site: cx.site });
                live = before_live;
                continue;
            }
            let defines = is_site || inst.dst.map_or(false, |d| cx.items.val.contains_key(&d));
            if (before || defines) && !after {
                if let Some(h) = handle(&cx, fl, inst, is_site, &states[ii + 1]) {
                    frees.push(Free { block: bi, pos: ii + 1, handle: h, site: cx.site });
                }
            }
            live = before_live;
        }
    }
    for &bi in ix.rpo.iter() {
        let b = &f.blocks[bi];
        if ix.preds[bi].len() != 1 || b.has_phi() {
            continue;
        }
        let p = ix.preds[bi][0];
        let (Some(pend), Some(cin)) = (ends[p].as_ref().or(fl.cin[p].as_ref()), fl.cin[bi].as_ref()) else { continue };
        if !cx.carried(&outs[p], pend) || cx.carried(&ins[bi], cin) {
            continue;
        }
        let mut h = None;
        for (&v, &i) in cx.items.val.iter() {
            if outs[p].get(i) && fl.must(v) == Some(seed) {
                h = Some(Handle::Val(v));
                break;
            }
        }
        if h.is_none() && cell_handles(fl, cx.site) {
            for (&c, &i) in cx.items.cell.iter() {
                if outs[p].get(i) && cx.must_cell(pend, c) {
                    h = Some(Handle::Cell(c));
                    break;
                }
            }
        }
        if let Some(h) = h {
            frees.push(Free { block: bi, pos: 0, handle: h, site: cx.site });
        }
    }
    Some(frees)
}

fn handle(cx: &Ctx, fl: &Flow, inst: &Inst, is_site: bool, after: &[CellVal]) -> Option<Handle> {
    if is_site {
        if let Site::Pair { cell_p, .. } = fl.sites[cx.site] {
            return (cell_handles(fl, cx.site) && cx.must_cell(after, cell_p)).then_some(Handle::Cell(cell_p));
        }
    }
    let mut u = Vec::new();
    inst.op.uses(&mut u);
    if let Some(d) = inst.dst {
        u.push(d);
    }
    if let Some(v) = u.iter().copied().find(|v| fl.must(*v) == Some(cx.seed)) {
        return Some(Handle::Val(v));
    }
    u.into_iter().find_map(|v| base_of(cx, fl, v)).map(Handle::Val)
}

fn base_of(cx: &Ctx, fl: &Flow, mut v: Val) -> Option<Val> {
    for _ in 0..8 {
        match cx.ix.op(cx.f, v)? {
            Op::PtrAdd { base, .. } => v = *base,
            Op::Load { addr } if fl.owner_bits.get(cx.seed) => v = fl.zero_add.get(addr).copied().unwrap_or(*addr),
            _ => return None,
        }
        if fl.must(v) == Some(cx.seed) {
            return Some(v);
        }
    }
    None
}

fn cell_p_of(fl: &Flow, site: usize) -> Option<usize> {
    match fl.sites[site] {
        Site::Pair { cell_p, .. } => Some(cell_p),
        Site::Value => None,
    }
}

fn self_append_old(cx: &Ctx, fl: &Flow, inst: &Inst) -> Option<Val> {
    if std::env::var_os("FIRN_OWN_NO_APPEND").is_some() {
        return None;
    }
    let Op::Call { name, args } = &inst.op else { return None };
    if !super::append::source(name) || args.len() < 5 {
        return None;
    }
    let Site::Pair { cell_p, cell_n } = fl.sites[cx.site] else { return None };
    let cell = |v: Val| match cx.ix.op(cx.f, v) {
        Some(Op::Load { addr }) => cx.sl.access(*addr, cx.f.val_ty(v)).map(|a| a.cell),
        _ => None,
    };
    if cell(args[1]) != Some(cell_p) || cell(args[2]) != Some(cell_n) || fl.must(args[1]) != Some(cx.seed) {
        return None;
    }
    Some(args[1])
}

fn cell_handles(fl: &Flow, site: usize) -> bool {
    fl.drops[site] != Drop::SlotsOwner
}
