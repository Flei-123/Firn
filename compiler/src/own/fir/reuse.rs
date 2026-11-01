use super::index::Index;
use super::live::Free;
use super::registry::{self, Drop};
use super::sites::{fast_def, Site};
use crate::fir::{Func, Op, Term, Val};
use crate::fx::{HashMap, HashSet};

const MAX_PATH: usize = 8;

pub(crate) struct Alloc {
    pub call: bool,
    pub class: usize,
    pub head: usize,
    pub b0: Val,
    pub take: usize,
    pub slow: usize,
    pub done: usize,
    pub next: Val,
    pub since_n: Val,
    pub since: Val,
}

pub(crate) struct Plan {
    pub free: usize,
    pub path: Vec<usize>,
    pub alloc: usize,
}

pub(crate) fn site_classes(f: &Func, ix: &Index, sites: &[Site], drops: &[Drop], site_at: &HashMap<(usize, usize), usize>) -> Vec<Option<usize>> {
    let mut seen: Vec<Option<Option<usize>>> = vec![None; sites.len()];
    for (&at, &k) in site_at.iter() {
        let c = if sites[k] == Site::Value && drops[k] == Drop::Block { def_class(f, ix, at) } else { None };
        seen[k] = Some(match seen[k] {
            None => c,
            Some(prev) if prev == c => c,
            Some(_) => None,
        });
    }
    seen.into_iter().map(|s| s.flatten()).collect()
}

fn def_class(f: &Func, ix: &Index, at: (usize, usize)) -> Option<usize> {
    match &f.blocks[at.0].insts.get(at.1)?.op {
        Op::Call { name, args } if registry::is_gc_alloc(name) => {
            let size = ix.const_of(f, *args.get(1)?)?;
            let c = crate::gc::class_for(u64::try_from(size).ok()?);
            (c < crate::gc::CLASS_BYTES.len()).then_some(c)
        }
        Op::PtrAdd { base, .. } => head_class(f, ix, *base),
        _ => None,
    }
}

fn head_class(f: &Func, ix: &Index, b0: Val) -> Option<usize> {
    let Some(Op::Load { addr }) = ix.op(f, b0) else { return None };
    let Some(Op::PtrAdd { off, .. }) = ix.op(f, *addr) else { return None };
    let c = ix.const_of(f, *off)? - crate::gc::ST_FREE as i128;
    usize::try_from(c / 8).ok()
}

fn has_call(insts: &[crate::fir::Inst]) -> bool {
    insts.iter().any(|i| matches!(i.op, Op::Call { .. } | Op::CallIndirect { .. }))
}

pub(crate) fn allocs(f: &Func, ix: &Index, with_calls: bool) -> Vec<Alloc> {
    let mut out = Vec::new();
    let pos: HashMap<u32, usize> = f.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    for (t, blk) in f.blocks.iter().enumerate() {
        for (ii, inst) in blk.insts.iter().enumerate() {
            if with_calls && matches!(&inst.op, Op::Call { name, .. } if registry::is_gc_alloc(name)) {
                if let (Some(d), Some(class)) = (inst.dst, def_class(f, ix, (t, ii))) {
                    out.push(Alloc { call: true, class, head: t, b0: d, take: t, slow: t, done: t, next: d, since_n: d, since: d });
                }
                continue;
            }
            let (Some(obj), Op::PtrAdd { base, off }) = (inst.dst, &inst.op) else { continue };
            if !fast_def(f, ix, *base, *off) {
                continue;
            }
            if let Some(a) = alloc_at(f, ix, &pos, t, obj, *base) {
                out.push(a);
            }
        }
    }
    let slows: HashSet<usize> = out.iter().filter(|a| !a.call).map(|a| a.slow).collect();
    out.retain(|a| !a.call || !slows.contains(&a.head));
    out
}

fn alloc_at(f: &Func, ix: &Index, pos: &HashMap<u32, usize>, t: usize, obj: Val, b0: Val) -> Option<Alloc> {
    let class = head_class(f, ix, b0)?;
    let (head, b0_at) = ix.def.get(b0 as usize).copied().flatten()?;
    if head == t || ix.preds[t] != [head] || has_call(&f.blocks[t].insts) {
        return None;
    }
    let tid = f.blocks[t].id;
    let slow = match f.blocks[head].term {
        Term::BrCond { then_bb, else_bb, .. } if then_bb == tid => *pos.get(&else_bb)?,
        Term::BrCond { then_bb, else_bb, .. } if else_bb == tid => *pos.get(&then_bb)?,
        _ => return None,
    };
    let Term::Br(d) = f.blocks[t].term else { return None };
    let done = *pos.get(&d)?;
    if ix.preds[done].len() < 2 || slow == t {
        return None;
    }
    let (mut next, mut since_n, mut since) = (None, None, None);
    for inst in f.blocks[t].insts.iter() {
        let Op::Store { addr, val } = &inst.op else { continue };
        if matches!(ix.op(f, *val), Some(Op::Load { addr: a }) if *a == obj) {
            next = Some(*val);
        }
        if is_since_addr(f, ix, *addr) {
            if let Some(Op::Bin(crate::fir::BinOp::Add, s, c)) = ix.op(f, *val) {
                if ix.const_of(f, *c).is_some() {
                    since_n = Some(*val);
                    since = Some(*s);
                }
            }
        }
    }
    let h_after: HashSet<Val> = f.blocks[head].insts[b0_at + 1..].iter().filter_map(|i| i.dst).collect();
    let mut ok = true;
    for inst in f.blocks[t].insts.iter() {
        let mut uses = Vec::new();
        inst.op.uses(&mut uses);
        for u in uses {
            if h_after.contains(&u) && !matches!(ix.op(f, u), Some(Op::Const(_))) {
                ok = false;
            }
        }
    }
    if !ok {
        return None;
    }
    Some(Alloc { call: false, class, head, b0, take: t, slow, done, next: next?, since_n: since_n?, since: since? })
}

fn is_since_addr(f: &Func, ix: &Index, a: Val) -> bool {
    let Some(Op::PtrAdd { base, off }) = ix.op(f, a) else { return false };
    matches!(ix.op(f, *base), Some(Op::GcAddr { .. })) && ix.const_of(f, *off) == Some(crate::gc::ST_SINCE as i128)
}

pub(crate) fn plan(f: &Func, ix: &Index, frees: &[Free], class: &[Option<usize>], allocs: &[Alloc]) -> Vec<Plan> {
    let mut used: HashSet<Val> = HashSet::default();
    let mut plans = Vec::new();
    for (i, fr) in frees.iter().enumerate() {
        let Some(c) = class.get(fr.site).copied().flatten() else { continue };
        let b = fr.block;
        let rest = &f.blocks[b].insts[fr.pos.min(f.blocks[b].insts.len())..];
        let here = allocs.iter().position(|a| a.head == b && a.class == c && !used.contains(&a.b0) && def_pos(ix, a.b0) >= fr.pos);
        let found = if let Some(k) = here {
            let upto = &f.blocks[b].insts[fr.pos.min(f.blocks[b].insts.len())..def_pos(ix, allocs[k].b0)];
            (!has_call(upto)).then(|| (vec![b], k))
        } else if has_call(rest) {
            None
        } else {
            let mut path = vec![b];
            let mut seen: HashSet<usize> = HashSet::default();
            seen.insert(b);
            search(f, ix, allocs, &used, c, &mut path, &mut seen)
        };
        let Some((path, k)) = found else { continue };
        used.insert(allocs[k].b0);
        plans.push(Plan { free: i, path, alloc: k });
    }
    let heads: HashSet<usize> = plans.iter().map(|p| allocs[p.alloc].head).collect();
    plans.retain(|p| p.path[..p.path.len() - 1].iter().all(|x| !heads.contains(x)) && simple_preds(ix, &p.path) && exits_ok(ix, &p.path, &allocs[p.alloc]));
    plans
}

pub(crate) fn side_exits(ix: &Index, path: &[usize]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for k in 0..path.len().saturating_sub(1) {
        for &y in ix.succs[path[k]].iter() {
            if y != path[k + 1] && !out.contains(&(k, y)) {
                out.push((k, y));
            }
        }
    }
    out
}

fn exits_ok(ix: &Index, path: &[usize], a: &Alloc) -> bool {
    side_exits(ix, path).iter().all(|&(k, y)| (!a.call && y == a.slow) || ix.preds[y] == [path[k]])
}

fn def_pos(ix: &Index, v: Val) -> usize {
    ix.def[v as usize].map(|d| d.1).unwrap_or(0)
}

fn simple_preds(ix: &Index, path: &[usize]) -> bool {
    path.windows(2).all(|w| ix.preds[w[1]].iter().filter(|&&p| p == w[0]).count() == 1)
}

fn search(f: &Func, ix: &Index, allocs: &[Alloc], used: &HashSet<Val>, c: usize, path: &mut Vec<usize>, seen: &mut HashSet<usize>) -> Option<(Vec<usize>, usize)> {
    if path.len() > MAX_PATH {
        return None;
    }
    let last = *path.last()?;
    for &x in ix.succs[last].iter() {
        if !seen.insert(x) {
            continue;
        }
        if let Some(k) = allocs.iter().position(|a| a.head == x && a.class == c && !used.contains(&a.b0)) {
            if !has_call(&f.blocks[x].insts[..def_pos(ix, allocs[k].b0)]) {
                let mut p = path.clone();
                p.push(x);
                return Some((p, k));
            }
        }
        if has_call(&f.blocks[x].insts) {
            continue;
        }
        path.push(x);
        if let Some(r) = search(f, ix, allocs, used, c, path, seen) {
            return Some(r);
        }
        path.pop();
    }
    None
}
