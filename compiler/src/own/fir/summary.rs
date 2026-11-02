use super::borrow::{Borrow, Graph};
use super::flow::{Borrows, Flow};
use super::prep::{Cache, Prep};
use super::registry;
use super::sites::Site;
use crate::fir::{Func, Module};
use crate::fx::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) const MAX_INSTS: usize = 40000;
pub(crate) const MAX_SITE_WORK: usize = 200_000;

pub(crate) struct Summaries {
    idx: HashMap<String, usize>,
    params: Vec<Vec<AtomicBool>>,
    fresh: HashMap<String, registry::Drop>,
}

impl Summaries {
    fn new(m: &Module) -> Summaries {
        let idx = m.funcs.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
        let params = m.funcs.iter().map(|f| (0..f.params.len()).map(|_| AtomicBool::new(analyzable(f))).collect()).collect();
        Summaries { idx, params, fresh: HashMap::default() }
    }

    pub(super) fn store(&self, i: usize, new: &[bool]) -> bool {
        let mut changed = false;
        for (slot, &b) in self.params[i].iter().zip(new.iter()) {
            if slot.swap(b, Ordering::Relaxed) != b {
                changed = true;
            }
        }
        changed
    }

    pub(super) fn reset(&mut self) {
        for v in self.params.iter() {
            v.iter().for_each(|b| b.store(false, Ordering::Relaxed));
        }
        self.fresh.clear();
    }
}

impl Borrows for Summaries {
    fn borrows(&self, name: &str, arg: usize) -> bool {
        if registry::consumes(name) {
            return false;
        }
        if registry::borrows_all(name) {
            return true;
        }
        self.idx.get(name).and_then(|&i| self.params[i].get(arg)).map_or(false, |b| b.load(Ordering::Relaxed))
    }

    fn fresh(&self, name: &str) -> bool {
        self.fresh.contains_key(name)
    }

    fn fresh_drop(&self, name: &str) -> registry::Drop {
        self.fresh.get(name).copied().unwrap_or(registry::Drop::Block)
    }
}

pub(crate) fn analyzable(f: &Func) -> bool {
    f.inst_count() <= MAX_INSTS && !f.blocks.is_empty()
}

pub(crate) fn skipped(f: &Func) -> bool {
    f.constant_time || f.interrupt || !analyzable(f) || f.name.starts_with("__gc_") || f.name.starts_with("__own_")
}

pub(super) fn param_borrows(f: &Func, p: &Prep, s: &Summaries) -> Vec<bool> {
    let fl = Flow::build(f, p, s, false);
    (0..f.params.len()).map(|i| !fl.escaped.get(1 + i) && !fl.ret_may.get(1 + i)).collect()
}

fn returns_fresh(f: &Func, p: &Prep, s: &Summaries) -> Option<registry::Drop> {
    let fl = Flow::build(f, p, s, true);
    if fl.ret_derived || fl.ret_may.count() == 0 {
        return None;
    }
    let (mut all_value, mut all_owner) = (true, true);
    for b in fl.ret_may.iter() {
        if b <= fl.nparams || fl.escaped.get(b) {
            return None;
        }
        let k = b - 1 - fl.nparams;
        let owner = fl.drops[k] == registry::Drop::SlotsOwner;
        let value = fl.sites[k] == Site::Value;
        if !value && !owner {
            return None;
        }
        all_value &= value;
        all_owner &= owner;
    }
    if all_owner {
        Some(registry::Drop::SlotsOwner)
    } else if all_value {
        Some(registry::Drop::Block)
    } else {
        None
    }
}

fn fresh_candidate(f: &Func) -> bool {
    f.ret.bytes() == 8 && !skipped(f)
}

pub(crate) struct Stats {
    pub borrow_evals: usize,
    pub fresh_evals: usize,
    pub rounds: usize,
}

pub(crate) fn compute(m: &Module, base: &[usize], cache: &Cache) -> (Summaries, Vec<usize>, Stats) {
    let n = m.funcs.len();
    let mut s = Summaries::new(m);
    let g = Graph::build(m);
    let mut bw = Borrow::new(m, &g, cache);
    let mut fresh_evals = 0;
    bw.extend(&mut s, base);
    let stats = |bw: &Borrow, fe: usize| Stats { borrow_evals: bw.evals, fresh_evals: fe, rounds: bw.rounds };
    if bw.exhausted {
        return (s, base.to_vec(), stats(&bw, fresh_evals));
    }
    let mut is_root = vec![false; n];
    base.iter().for_each(|&r| is_root[r] = true);
    let mut queued = is_root.clone();
    let mut work: Vec<usize> = base.to_vec();
    let mut budget = 4 * n + 1000;
    while !work.is_empty() && budget > 0 {
        let mut batch = std::mem::take(&mut work);
        batch.sort_unstable();
        if batch.len() > budget {
            batch.split_off(budget);
        }
        budget -= batch.len();
        batch.iter().for_each(|&i| queued[i] = false);
        let cand: Vec<usize> = batch.into_iter().filter(|&i| !s.fresh.contains_key(&m.funcs[i].name)).collect();
        fresh_evals += cand.len();
        let res = {
            let sr: &Summaries = &s;
            crate::parfn::map_weighted(&cand, |&i| crate::parfn::weight(&m.funcs[i]), |&i| {
                let f = &m.funcs[i];
                if fresh_candidate(f) {
                    returns_fresh(f, cache.get(m, i), sr)
                } else {
                    None
                }
            })
        };
        let newly: Vec<(usize, registry::Drop)> = cand.iter().zip(res).filter_map(|(&i, d)| d.map(|d| (i, d))).collect();
        if newly.is_empty() {
            continue;
        }
        let mut new: Vec<usize> = Vec::new();
        let newly: Vec<usize> = newly
            .into_iter()
            .map(|(i, d)| {
                s.fresh.insert(m.funcs[i].name.clone(), d);
                i
            })
            .collect();
        for &i in newly.iter() {
            for &c in g.callers[i].iter() {
                if !is_root[c] {
                    is_root[c] = true;
                    new.push(c);
                }
            }
        }
        bw.extend(&mut s, &new);
        if bw.exhausted {
            return (s, base.to_vec(), stats(&bw, fresh_evals));
        }
        for &i in newly.iter() {
            for &c in g.callers[i].iter() {
                if !queued[c] {
                    queued[c] = true;
                    work.push(c);
                }
            }
        }
    }
    let roots = (0..n).filter(|&i| is_root[i]).collect();
    (s, roots, stats(&bw, fresh_evals))
}
