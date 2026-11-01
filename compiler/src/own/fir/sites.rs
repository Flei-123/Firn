use super::flow::Borrows;
use super::index::Index;
use super::registry::{self, Drop, Shape};
use super::slots::Slots;
use crate::fir::{Func, Op, Val};
use crate::fx::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Site {
    Pair { cell_p: usize, cell_n: usize },
    Value,
}

pub(crate) struct Found {
    pub sites: Vec<Site>,
    pub site_at: HashMap<(usize, usize), usize>,
    pub drops: Vec<Drop>,
}

struct Def {
    val: Val,
    at: (usize, usize),
    slow: bool,
    drop: Drop,
}

pub(crate) fn find(f: &Func, ix: &Index, sl: &Slots, br: &dyn Borrows) -> Found {
    let mut sites = Vec::new();
    let mut site_at = HashMap::default();
    let mut defs: Vec<Def> = Vec::new();
    let mut drops: Vec<Drop> = Vec::new();
    for &bi in ix.rpo.iter() {
        for (ii, inst) in f.blocks[bi].insts.iter().enumerate() {
            match &inst.op {
                Op::Call { name, args } => match registry::producer(name) {
                    Some(p) if p.shape == Shape::SretPair => {
                        if let Some((cell_p, cell_n)) = args.first().and_then(|a| sl.producer_cells(*a)) {
                            site_at.insert((bi, ii), sites.len());
                            sites.push(Site::Pair { cell_p, cell_n });
                            drops.push(Drop::Block);
                        }
                    }
                    Some(p) if p.shape == Shape::SlotsOwner => {
                        if let Some((cell_p, cell_n)) = args.first().and_then(|a| sl.result_cells(*a)) {
                            site_at.insert((bi, ii), sites.len());
                            sites.push(Site::Pair { cell_p, cell_n });
                            drops.push(Drop::SlotsOwner);
                        }
                    }
                    _ => {
                        if let Some(d) = inst.dst {
                            if inst.ty.bytes() == 8 && (registry::is_gc_alloc(name) || br.fresh(name)) {
                                defs.push(Def { val: d, at: (bi, ii), slow: true, drop: Drop::Block });
                            }
                        }
                    }
                },
                Op::PtrAdd { base, off } => {
                    if let Some(d) = inst.dst {
                        if fast_def(f, ix, *base, *off) {
                            defs.push(Def { val: d, at: (bi, ii), slow: false, drop: Drop::Block });
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if defs.is_empty() {
        return Found { sites, site_at, drops };
    }
    let pos: HashMap<Val, usize> = defs.iter().enumerate().map(|(i, d)| (d.val, i)).collect();
    let mut uf: Vec<usize> = (0..defs.len()).collect();
    for &bi in ix.rpo.iter() {
        for inst in f.blocks[bi].insts.iter() {
            let Op::Phi { incoming } = &inst.op else { continue };
            let ids: Option<Vec<usize>> = incoming.iter().map(|(_, v)| pos.get(v).copied()).collect();
            let Some(ids) = ids else { continue };
            if ids.is_empty() || !ids.iter().any(|&i| defs[i].slow) {
                continue;
            }
            for w in ids.windows(2) {
                let (a, b) = (root(&mut uf, w[0]), root(&mut uf, w[1]));
                if a != b {
                    uf[b] = a;
                }
            }
        }
    }
    let mut cell_def: HashMap<usize, usize> = HashMap::default();
    for &bi in ix.rpo.iter() {
        for inst in f.blocks[bi].insts.iter() {
            let Op::Store { addr, val } = &inst.op else { continue };
            let (Some(&i), Some(a)) = (pos.get(val), sl.access(*addr, inst.ty)) else { continue };
            if !a.strong {
                continue;
            }
            let j = *cell_def.entry(a.cell).or_insert(i);
            let (x, y) = (root(&mut uf, j), root(&mut uf, i));
            if x != y {
                uf[y] = x;
            }
        }
    }
    let mut has_slow = vec![false; defs.len()];
    let mut all_owner = vec![true; defs.len()];
    for i in 0..defs.len() {
        let r = root(&mut uf, i);
        if defs[i].slow {
            has_slow[r] = true;
        }
        if defs[i].drop != Drop::SlotsOwner {
            all_owner[r] = false;
        }
    }
    let mut group_site: HashMap<usize, usize> = HashMap::default();
    for i in 0..defs.len() {
        let r = root(&mut uf, i);
        if !has_slow[r] {
            continue;
        }
        let k = *group_site.entry(r).or_insert_with(|| {
            sites.push(Site::Value);
            drops.push(if all_owner[r] { Drop::SlotsOwner } else { Drop::Block });
            sites.len() - 1
        });
        site_at.insert(defs[i].at, k);
    }
    Found { sites, site_at, drops }
}

fn root(uf: &mut [usize], mut i: usize) -> usize {
    while uf[i] != i {
        uf[i] = uf[uf[i]];
        i = uf[i];
    }
    i
}

fn fast_def(f: &Func, ix: &Index, base: Val, off: Val) -> bool {
    if ix.const_of(f, off) != Some(crate::gc::BLOCK_HEADER as i128) {
        return false;
    }
    let Some(Op::Load { addr }) = ix.op(f, base) else { return false };
    let Some(Op::PtrAdd { base: st, off: o }) = ix.op(f, *addr) else { return false };
    if !matches!(ix.op(f, *st), Some(Op::GcAddr { .. })) {
        return false;
    }
    let lo = crate::gc::ST_FREE as i128;
    let hi = lo + 8 * crate::gc::CLASS_BYTES.len() as i128;
    matches!(ix.const_of(f, *o), Some(c) if c >= lo && c < hi && (c - lo) % 8 == 0)
}
