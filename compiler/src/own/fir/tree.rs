use super::borrow::Graph;
use super::flow::{Borrows, Flow, Mode, OTHER};
use super::prep::{Cache, Prep};
use super::registry::{self, Drop};
use super::sites::Site;
use super::summary::{analyzable, skipped, Summaries};
use crate::fir::{FTy, Func, Module, Op, Val};
use crate::fx::{HashMap, HashSet};

const MAX_ROUNDS: usize = 32;

pub(crate) fn enabled() -> bool {
    std::env::var_os("FIRN_OWN_NO_TREE").is_none()
}

pub(crate) fn compute(m: &Module, cache: &Cache, s: &mut Summaries) {
    if !enabled() {
        return;
    }
    let mut cand: Vec<usize> = (0..m.funcs.len()).filter(|&i| s.fresh_kind(&m.funcs[i].name) == Some(Drop::Block)).collect();
    if std::env::var_os("FIRN_OWN_TRACE").is_some() {
        eprintln!("ownership fir: tree candidates {:?}", cand.iter().map(|&i| &m.funcs[i].name).collect::<Vec<_>>());
    }
    if cand.is_empty() {
        return;
    }
    let names = |c: &[usize]| -> HashSet<String> { c.iter().map(|&i| m.funcs[i].name.clone()).collect() };
    s.tree = names(&cand);
    let mut stable = false;
    for _ in 0..MAX_ROUNDS {
        let keep = {
            let sr: &Summaries = s;
            crate::parfn::map_weighted(&cand, |&i| crate::parfn::weight(&m.funcs[i]), |&i| constructs_tree(&m.funcs[i], &cache.get(m, i), sr))
        };
        let next: Vec<usize> = cand.iter().zip(keep).filter(|(_, k)| *k).map(|(&i, _)| i).collect();
        stable = next.len() == cand.len();
        cand = next;
        s.tree = names(&cand);
        if stable {
            break;
        }
    }
    if !stable || s.tree.is_empty() {
        s.tree.clear();
        return;
    }
    deep(m, cache, s);
    if std::env::var_os("FIRN_OWN_TRACE").is_some() {
        eprintln!("ownership fir: tree constructors {:?}, deep borrows {:?}", s.tree, s.deep.iter().filter(|(_, v)| v.iter().any(|b| *b)).map(|(k, _)| k).collect::<Vec<_>>());
    }
}

fn deep(m: &Module, cache: &Cache, s: &mut Summaries) {
    let g = Graph::build(m);
    let tree_idx: Vec<usize> = (0..m.funcs.len()).filter(|&i| s.tree.contains(&m.funcs[i].name)).collect();
    let mut seen = vec![false; m.funcs.len()];
    let mut work: Vec<usize> = Vec::new();
    for &t in tree_idx.iter() {
        for &c in g.callers[t].iter() {
            for &d in g.callees[c].iter() {
                if !seen[d] {
                    seen[d] = true;
                    work.push(d);
                }
            }
        }
    }
    while let Some(i) = work.pop() {
        for &d in g.callees[i].iter() {
            if !seen[d] {
                seen[d] = true;
                work.push(d);
            }
        }
    }
    let cand: Vec<usize> = (0..m.funcs.len())
        .filter(|&i| seen[i] && analyzable(&m.funcs[i]) && !skipped(&m.funcs[i]) && s.shallow(i).iter().any(|b| *b))
        .collect();
    s.deep = cand.iter().map(|&i| (m.funcs[i].name.clone(), s.shallow(i))).collect();
    let mut stable = false;
    for _ in 0..MAX_ROUNDS {
        let res = {
            let sr: &Summaries = s;
            crate::parfn::map_weighted(&cand, |&i| crate::parfn::weight(&m.funcs[i]), |&i| deep_params(&m.funcs[i], &cache.get(m, i), sr))
        };
        let mut changed = false;
        for (&i, new) in cand.iter().zip(res) {
            let old = s.deep.get_mut(&m.funcs[i].name).expect("deep entry");
            for (o, n) in old.iter_mut().zip(new) {
                if *o && !n {
                    *o = false;
                    changed = true;
                }
            }
        }
        if !changed {
            stable = true;
            break;
        }
    }
    if !stable {
        s.deep.clear();
    }
}

fn deep_params(f: &Func, p: &Prep, s: &Summaries) -> Vec<bool> {
    let cur = s.deep.get(&f.name).cloned().unwrap_or_default();
    if !cur.iter().any(|b| *b) {
        return cur;
    }
    let fl = Flow::build_with(f, p, s, false, Mode::Deep, &[]);
    let narrow = (f.ret != FTy::Ptr).then(|| Flow::build_with(f, p, s, false, Mode::DeepPtr, &[]));
    let ret = narrow.as_ref().map_or(&fl.ret_may, |n| &n.ret_may);
    (0..f.params.len()).map(|i| cur.get(i).copied().unwrap_or(false) && !fl.escaped.get(1 + i) && !ret.get(1 + i)).collect()
}

fn site_defs_ok(f: &Func, fl: &Flow, k: usize, ok: &dyn Fn(&Op) -> bool) -> bool {
    let mut any = false;
    for (&(bi, ii), &kk) in fl.site_at.iter() {
        if kk != k {
            continue;
        }
        any = true;
        if !ok(&f.blocks[bi].insts[ii].op) {
            return false;
        }
    }
    any
}

fn tree_call(s: &Summaries, op: &Op) -> bool {
    matches!(op, Op::Call { name, .. } if s.tree(name))
}

fn constructs_tree(f: &Func, p: &Prep, s: &Summaries) -> bool {
    let why = reject(f, p, s);
    if let (Some(w), true) = (why, std::env::var_os("FIRN_OWN_TRACE").is_some()) {
        eprintln!("ownership fir: {} not a tree constructor: {}", f.name, w);
    }
    why.is_none()
}

fn reject(f: &Func, p: &Prep, s: &Summaries) -> Option<&'static str> {
    let fl = Flow::build(f, p, s, true);
    if fl.ret_derived || fl.ret_may.count() == 0 {
        return Some("r1");
    }
    let np = fl.nparams;
    let mut r: Vec<usize> = Vec::new();
    for b in fl.ret_may.iter() {
        if b <= np || fl.escaped.get(b) {
            return Some("r2");
        }
        let k = b - 1 - np;
        if fl.sites[k] != Site::Value || fl.drops[k] != Drop::Block {
            return Some("r3");
        }
        let own = |op: &Op| match op {
            Op::Call { name, .. } => registry::is_gc_alloc(name) || s.tree(name),
            Op::PtrAdd { .. } => true,
            _ => false,
        };
        if !site_defs_ok(f, &fl, k, &own) {
            return Some("r4");
        }
        r.push(b);
    }
    let fl = Flow::build_with(f, p, s, true, Mode::Plain, &r);
    if r.iter().any(|&b| fl.escaped.get(b) || !fl.ret_may.get(b)) {
        return Some("r5");
    }
    let child = |b: usize| b > np && fl.sites[b - 1 - np] == Site::Value && site_defs_ok(f, &fl, b - 1 - np, &|op| tree_call(s, op));
    let mut held: HashSet<usize> = r.iter().copied().collect();
    let mut stored: HashSet<usize> = HashSet::default();
    for &(bi, ii) in fl.absorbed.iter() {
        let inst = &f.blocks[bi].insts[ii];
        let Op::Store { val, .. } = &inst.op else { return Some("r6") };
        let v = &fl.may[*val as usize];
        let ptr = inst.ty == FTy::Ptr || f.val_ty(*val) == FTy::Ptr;
        if ptr {
            if fl.derived[*val as usize] {
                return Some("r7");
            }
            for b in v.iter() {
                if b == OTHER || !child(b) || fl.escaped.get(b) || fl.ret_may.get(b) || !stored.insert(b) {
                    return Some("r8");
                }
            }
        } else {
            for b in v.iter() {
                if b == OTHER {
                    continue;
                }
                if b > np || f.params.get(b - 1) == Some(&FTy::Ptr) {
                    return Some("r9");
                }
            }
        }
    }
    held.extend(stored.iter().copied());
    let touches = |v: Val| fl.may[v as usize].iter().any(|b| held.contains(&b));
    let mut uses: Vec<Val> = Vec::new();
    for b in f.blocks.iter() {
        for inst in b.insts.iter() {
            match &inst.op {
                Op::Load { addr } if p.sl.access(*addr, inst.ty).is_none() && touches(*addr) && !free_link(f, p, *addr) => return Some("r10"),
                Op::Call { name, args } if !registry::is_gc_alloc(name) && args.iter().any(|a| touches(*a)) => return Some("r11"),
                Op::CallIndirect { .. } => {
                    uses.clear();
                    inst.op.uses(&mut uses);
                    if uses.iter().any(|a| touches(*a)) {
                        return Some("r12");
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn free_link(f: &Func, p: &Prep, addr: Val) -> bool {
    matches!(p.ix.op(f, addr), Some(Op::PtrAdd { base, off }) if super::sites::fast_def(f, &p.ix, *base, *off))
}

pub(crate) fn mark(f: &Func, p: &Prep, fl: &mut Flow, s: &dyn Borrows) -> usize {
    let np = fl.nparams;
    let mut cands: Vec<usize> = Vec::new();
    for k in 0..fl.sites.len() {
        if fl.sites[k] == Site::Value && fl.drops[k] == Drop::Block {
            let ok = |op: &Op| matches!(op, Op::Call { name, .. } if s.tree(name));
            if site_defs_ok(f, fl, k, &ok) {
                cands.push(k);
            }
        }
    }
    if cands.is_empty() {
        return 0;
    }
    let mut bad: HashMap<usize, bool> = cands.iter().map(|&k| (1 + np + k, false)).collect();
    let mut uses: Vec<Val> = Vec::new();
    for b in f.blocks.iter() {
        for inst in b.insts.iter() {
            uses.clear();
            inst.op.uses(&mut uses);
            for (&seed, flag) in bad.iter_mut() {
                if *flag || !uses.iter().any(|u| fl.may[*u as usize].get(seed)) {
                    continue;
                }
                let carries = |v: &Val| fl.may[*v as usize].get(seed);
                *flag = match &inst.op {
                    Op::Phi { .. } | Op::Copy { .. } | Op::Cast { .. } | Op::Select { .. } | Op::Cmp { .. } => false,
                    Op::Call { name, args } => args.iter().enumerate().any(|(i, a)| carries(a) && !s.deep_borrows(name, i)),
                    Op::Store { addr, .. } => p.sl.access(*addr, inst.ty).is_none() || carries(addr),
                    Op::Load { addr } => p.sl.access(*addr, inst.ty).is_none(),
                    _ => true,
                };
            }
        }
    }
    let mut n = 0;
    for k in cands {
        if !bad[&(1 + np + k)] {
            fl.drops[k] = Drop::Tree;
            n += 1;
        }
    }
    if n > 0 && std::env::var_os("FIRN_OWN_TRACE").is_some() {
        eprintln!("ownership fir: {} tree drops {}", f.name, n);
    }
    n
}
