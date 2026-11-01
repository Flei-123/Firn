use super::flow::{Borrows, Flow};
use super::index::Index;
use super::registry;
use super::slots::Slots;
use crate::fir::{Func, Module, Op};
use crate::fx::HashMap;

pub(crate) const MAX_INSTS: usize = 40000;
pub(crate) const MAX_SITE_WORK: usize = 200_000;

pub(crate) struct Summaries {
    map: HashMap<String, Vec<bool>>,
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
}

pub(crate) fn analyzable(f: &Func) -> bool {
    f.inst_count() <= MAX_INSTS && !f.blocks.is_empty()
}

fn param_borrows(f: &Func, s: &Summaries) -> Vec<bool> {
    if f.params.is_empty() {
        return Vec::new();
    }
    let ix = Index::build(f);
    let sl = Slots::build(f, &ix);
    let fl = Flow::build(f, &ix, &sl, s, false);
    (0..f.params.len()).map(|i| !fl.escaped.get(1 + i)).collect()
}

pub(crate) fn compute(m: &Module, roots: &[usize]) -> Summaries {
    let n = m.funcs.len();
    let mut s = Summaries { map: HashMap::default() };
    let mut idx: HashMap<&str, usize> = HashMap::default();
    for (i, f) in m.funcs.iter().enumerate() {
        idx.insert(f.name.as_str(), i);
        s.map.insert(f.name.clone(), vec![analyzable(f); f.params.len()]);
    }
    let mut callers: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut reach = vec![false; n];
    let mut stack: Vec<usize> = roots.to_vec();
    roots.iter().for_each(|&r| reach[r] = true);
    while let Some(i) = stack.pop() {
        for b in m.funcs[i].blocks.iter() {
            for inst in b.insts.iter() {
                if let Op::Call { name, .. } = &inst.op {
                    if let Some(&j) = idx.get(name.as_str()) {
                        if callers[j].last() != Some(&i) {
                            callers[j].push(i);
                        }
                        if !reach[j] {
                            reach[j] = true;
                            stack.push(j);
                        }
                    }
                }
            }
        }
    }
    let mut queued: Vec<bool> = (0..n).map(|i| reach[i] && analyzable(&m.funcs[i])).collect();
    let mut work: std::collections::VecDeque<usize> = (0..n).filter(|&i| queued[i]).collect();
    let mut budget = 8 * work.len() + 1000;
    while let Some(i) = work.pop_front() {
        queued[i] = false;
        if budget == 0 {
            for v in s.map.values_mut() {
                v.iter_mut().for_each(|b| *b = false);
            }
            break;
        }
        budget -= 1;
        let f = &m.funcs[i];
        let new = param_borrows(f, &s);
        if s.map.get(&f.name) != Some(&new) {
            s.map.insert(f.name.clone(), new);
            for &c in callers[i].iter() {
                if !queued[c] {
                    queued[c] = true;
                    work.push_back(c);
                }
            }
        }
    }
    s
}
