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

struct Borrow<'a> {
    m: &'a Module,
    g: &'a Graph,
    reach: Vec<bool>,
    budget: usize,
    exhausted: bool,
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
        let mut work: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
        for i in fresh_reach {
            if analyzable(&self.m.funcs[i]) {
                queued[i] = true;
                work.push_back(i);
            }
        }
        self.budget += 8 * work.len();
        while let Some(i) = work.pop_front() {
            queued[i] = false;
            if self.budget == 0 {
                for v in s.map.values_mut() {
                    v.iter_mut().for_each(|b| *b = false);
                }
                s.fresh.clear();
                self.exhausted = true;
                return;
            }
            self.budget -= 1;
            let f = &self.m.funcs[i];
            let new = param_borrows(f, s);
            if s.map.get(&f.name) != Some(&new) {
                s.map.insert(f.name.clone(), new);
                for &c in self.g.callers[i].iter() {
                    if self.reach[c] && !queued[c] {
                        queued[c] = true;
                        work.push_back(c);
                    }
                }
            }
        }
    }
}

pub(crate) fn compute(m: &Module, base: &[usize]) -> (Summaries, Vec<usize>) {
    let n = m.funcs.len();
    let mut s = Summaries { map: HashMap::default(), fresh: HashSet::default() };
    for f in m.funcs.iter() {
        s.map.insert(f.name.clone(), vec![analyzable(f); f.params.len()]);
    }
    let g = Graph::build(m);
    let mut bw = Borrow { m, g: &g, reach: vec![false; n], budget: 1000, exhausted: false };
    bw.extend(&mut s, base);
    if bw.exhausted {
        return (s, base.to_vec());
    }
    let mut is_root = vec![false; n];
    base.iter().for_each(|&r| is_root[r] = true);
    let mut queued = is_root.clone();
    let mut work: std::collections::VecDeque<usize> = base.iter().copied().collect();
    let mut budget = 4 * n + 1000;
    while let Some(i) = work.pop_front() {
        queued[i] = false;
        if budget == 0 {
            break;
        }
        budget -= 1;
        let f = &m.funcs[i];
        if s.fresh.contains(&f.name) || !returns_fresh(f, &s) {
            continue;
        }
        s.fresh.insert(f.name.clone());
        let new: Vec<usize> = g.callers[i].iter().copied().filter(|&c| !is_root[c]).collect();
        new.iter().for_each(|&c| is_root[c] = true);
        bw.extend(&mut s, &new);
        if bw.exhausted {
            return (s, base.to_vec());
        }
        for &c in g.callers[i].iter() {
            if !queued[c] {
                queued[c] = true;
                work.push_back(c);
            }
        }
    }
    let roots = (0..n).filter(|&i| is_root[i]).collect();
    (s, roots)
}
