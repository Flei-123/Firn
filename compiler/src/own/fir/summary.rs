use super::flow::{Borrows, Flow};
use super::index::Index;
use super::registry;
use super::sites::Site;
use super::slots::Slots;
use crate::fir::{Func, Module, Op};
use crate::fx::{HashMap, HashSet};

pub(crate) const MAX_INSTS: usize = 40000;
pub(crate) const MAX_SITE_WORK: usize = 200_000;

pub(crate) struct Summaries {
    map: HashMap<String, Vec<bool>>,
    fresh: HashSet<String>,
}

impl Borrows for Summaries {
    fn borrows(&self, name: &str, arg: usize) -> bool {
        if registry::consumes(name) {
            return false;
        }
        if registry::borrows_all(name) {
            return true;
        }
        self.map.get(name).and_then(|v| v.get(arg).copied()).unwrap_or(false)
    }

    fn fresh(&self, name: &str) -> bool {
        self.fresh.contains(name)
    }
}

pub(crate) fn analyzable(f: &Func) -> bool {
    f.inst_count() <= MAX_INSTS && !f.blocks.is_empty()
}

pub(crate) fn skipped(f: &Func) -> bool {
    f.constant_time || f.interrupt || !analyzable(f) || f.name.starts_with("__gc_") || f.name.starts_with("__own_")
}

fn param_borrows(f: &Func, s: &Summaries) -> Vec<bool> {
    if f.params.is_empty() {
        return Vec::new();
    }
    let ix = Index::build(f);
    let sl = Slots::build(f, &ix);
    let fl = Flow::build(f, &ix, &sl, s, false);
    (0..f.params.len()).map(|i| !fl.escaped.get(1 + i) && !fl.ret_may.get(1 + i)).collect()
}

fn returns_fresh(f: &Func, s: &Summaries) -> bool {
    if f.ret.bytes() != 8 || skipped(f) {
        return false;
    }
    let ix = Index::build(f);
    let sl = Slots::build(f, &ix);
    let fl = Flow::build(f, &ix, &sl, s, true);
    if fl.ret_derived || fl.ret_may.count() == 0 {
        return false;
    }
    let ok = fl.ret_may.iter().all(|b| b > fl.nparams && fl.sites[b - 1 - fl.nparams] == Site::Value && !fl.escaped.get(b));
    ok
}

struct Graph {
    callees: Vec<Vec<usize>>,
    callers: Vec<Vec<usize>>,
}

impl Graph {
    fn build(m: &Module) -> Graph {
        let n = m.funcs.len();
        let idx: HashMap<String, usize> = m.funcs.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
        let mut callees: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut callers: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, f) in m.funcs.iter().enumerate() {
            for b in f.blocks.iter() {
                for inst in b.insts.iter() {
                    if let Op::Call { name, .. } = &inst.op {
                        if let Some(&j) = idx.get(name.as_str()) {
                            if !callees[i].contains(&j) {
                                callees[i].push(j);
                                callers[j].push(i);
                            }
                        }
                    }
                }
            }
        }
        Graph { callees, callers }
    }
}

pub(crate) struct Stats {
    pub borrow_evals: usize,
    pub fresh_evals: usize,
    pub waves: usize,
}

struct Borrow<'a> {
    m: &'a Module,
    g: &'a Graph,
    reach: Vec<bool>,
    budget: usize,
    exhausted: bool,
    evals: usize,
    waves: usize,
}

impl<'a> Borrow<'a> {
    fn extend(&mut self, s: &mut Summaries, roots: &[usize]) {
        let mut stack: Vec<usize> = Vec::new();
        let mut fresh_reach: Vec<usize> = Vec::new();
        for &r in roots {
            if !self.reach[r] {
                self.reach[r] = true;
                stack.push(r);
            }
        }
        while let Some(i) = stack.pop() {
            fresh_reach.push(i);
            for &j in self.g.callees[i].iter() {
                if !self.reach[j] {
                    self.reach[j] = true;
                    stack.push(j);
                }
            }
        }
        let n = self.m.funcs.len();
        let mut queued = vec![false; n];
        let mut work: Vec<usize> = Vec::new();
        for i in fresh_reach {
            if analyzable(&self.m.funcs[i]) {
                queued[i] = true;
                work.push(i);
            }
        }
        self.budget += 8 * work.len();
        while !work.is_empty() {
            if self.budget < work.len() {
                for v in s.map.values_mut() {
                    v.iter_mut().for_each(|b| *b = false);
                }
                s.fresh.clear();
                self.exhausted = true;
                return;
            }
            self.budget -= work.len();
            self.evals += work.len();
            self.waves += 1;
            let mut batch = std::mem::take(&mut work);
            batch.sort_unstable();
            batch.iter().for_each(|&i| queued[i] = false);
            let m = self.m;
            let res = {
                let sr: &Summaries = s;
                crate::parfn::map_slice(&batch, |&i| param_borrows(&m.funcs[i], sr))
            };
            for (&i, new) in batch.iter().zip(res) {
                let name = &self.m.funcs[i].name;
                if s.map.get(name) != Some(&new) {
                    s.map.insert(name.clone(), new);
                    for &c in self.g.callers[i].iter() {
                        if self.reach[c] && !queued[c] {
                            queued[c] = true;
                            work.push(c);
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn compute(m: &Module, base: &[usize]) -> (Summaries, Vec<usize>, Stats) {
    let n = m.funcs.len();
    let mut s = Summaries { map: HashMap::default(), fresh: HashSet::default() };
    for f in m.funcs.iter() {
        s.map.insert(f.name.clone(), vec![analyzable(f); f.params.len()]);
    }
    let g = Graph::build(m);
    let mut bw = Borrow { m, g: &g, reach: vec![false; n], budget: 1000, exhausted: false, evals: 0, waves: 0 };
    let mut fresh_evals = 0;
    bw.extend(&mut s, base);
    let stats = |bw: &Borrow, fe: usize| Stats { borrow_evals: bw.evals, fresh_evals: fe, waves: bw.waves };
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
        let cand: Vec<usize> = batch.into_iter().filter(|&i| !s.fresh.contains(&m.funcs[i].name)).collect();
        fresh_evals += cand.len();
        let res = {
            let sr: &Summaries = &s;
            crate::parfn::map_slice(&cand, |&i| returns_fresh(&m.funcs[i], sr))
        };
        let newly: Vec<usize> = cand.iter().zip(res).filter(|(_, ok)| *ok).map(|(&i, _)| i).collect();
        if newly.is_empty() {
            continue;
        }
        let mut new: Vec<usize> = Vec::new();
        for &i in newly.iter() {
            s.fresh.insert(m.funcs[i].name.clone());
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
