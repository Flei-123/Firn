use super::prep::Cache;
use super::summary::{analyzable, param_borrows, Summaries};
use crate::fir::{Module, Op};
use crate::fx::HashMap;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::{Condvar, Mutex};

pub(crate) struct Graph {
    pub callees: Vec<Vec<usize>>,
    pub callers: Vec<Vec<usize>>,
}

impl Graph {
    pub fn build(m: &Module) -> Graph {
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

const UNSEEN: u32 = u32::MAX;
const OPEN: u32 = u32::MAX - 1;

pub(crate) struct Borrow<'a> {
    m: &'a Module,
    g: &'a Graph,
    cache: &'a Cache,
    reach: Vec<bool>,
    height: Vec<u32>,
    weight: Vec<usize>,
    budget: usize,
    pub exhausted: bool,
    pub evals: usize,
    pub rounds: usize,
}

struct Queue {
    queued: Vec<bool>,
    running: Vec<bool>,
    blockers: Vec<u32>,
    ready: BinaryHeap<(Reverse<u32>, usize, Reverse<usize>)>,
    nrunning: usize,
    budget: usize,
    evals: usize,
    exhausted: bool,
}

struct Shared<'b> {
    reach: &'b [bool],
    height: &'b [u32],
    weight: &'b [usize],
    callers: &'b [Vec<usize>],
}

impl Shared<'_> {
    fn below(&self, j: usize, c: usize) -> bool {
        self.reach[c] && self.height[c] > self.height[j]
    }

    fn push(&self, q: &mut Queue, i: usize) {
        q.ready.push((Reverse(self.height[i]), self.weight[i], Reverse(i)));
    }

    fn enqueue(&self, q: &mut Queue, i: usize) {
        if q.queued[i] {
            return;
        }
        q.queued[i] = true;
        if q.running[i] {
            return;
        }
        for &c in self.callers[i].iter() {
            if self.below(i, c) {
                q.blockers[c] += 1;
            }
        }
        if q.blockers[i] == 0 {
            self.push(q, i);
        }
    }

    fn retire(&self, q: &mut Queue, i: usize) {
        for &c in self.callers[i].iter() {
            if self.below(i, c) {
                q.blockers[c] -= 1;
                if q.blockers[c] == 0 && q.queued[c] && !q.running[c] {
                    self.push(q, c);
                }
            }
        }
    }

    fn pop(&self, q: &mut Queue) -> Option<usize> {
        while let Some((_, _, Reverse(i))) = q.ready.pop() {
            if q.queued[i] && !q.running[i] && q.blockers[i] == 0 {
                return Some(i);
            }
        }
        None
    }
}

impl<'a> Borrow<'a> {
    pub fn new(m: &'a Module, g: &'a Graph, cache: &'a Cache) -> Borrow<'a> {
        let n = m.funcs.len();
        let weight = m.funcs.iter().map(crate::parfn::weight).collect();
        Borrow { m, g, cache, reach: vec![false; n], height: vec![UNSEEN; n], weight, budget: 1000, exhausted: false, evals: 0, rounds: 0 }
    }

    fn heights(&mut self, nodes: &[usize]) {
        let callees = &self.g.callees;
        let height = &mut self.height;
        for &r in nodes {
            if height[r] != UNSEEN {
                continue;
            }
            height[r] = OPEN;
            let mut stack: Vec<(usize, usize)> = vec![(r, 0)];
            while let Some(top) = stack.last_mut() {
                let (i, k) = *top;
                if k < callees[i].len() {
                    top.1 += 1;
                    let j = callees[i][k];
                    if height[j] == UNSEEN {
                        height[j] = OPEN;
                        stack.push((j, 0));
                    }
                } else {
                    height[i] = callees[i].iter().map(|&j| height[j]).filter(|&h| h < OPEN).map(|h| h + 1).max().unwrap_or(0);
                    stack.pop();
                }
            }
        }
    }

    pub fn extend(&mut self, s: &mut Summaries, roots: &[usize]) {
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
        self.heights(&fresh_reach);
        let items: Vec<usize> = fresh_reach.into_iter().filter(|&i| analyzable(&self.m.funcs[i])).collect();
        if items.is_empty() {
            return;
        }
        self.rounds += 1;
        self.budget += 8 * items.len();
        let n = self.m.funcs.len();
        let sh = Shared { reach: &self.reach, height: &self.height, weight: &self.weight, callers: &self.g.callers };
        let mut q = Queue {
            queued: vec![false; n],
            running: vec![false; n],
            blockers: vec![0; n],
            ready: BinaryHeap::new(),
            nrunning: 0,
            budget: self.budget,
            evals: 0,
            exhausted: false,
        };
        for &i in items.iter() {
            sh.enqueue(&mut q, i);
        }
        let lock = Mutex::new(q);
        let cv = Condvar::new();
        let (m, cache, sr) = (self.m, self.cache, &*s);
        let worker = || run_worker(&sh, &lock, &cv, m, cache, sr);
        let nthreads = crate::parfn::threads().min(items.len());
        if nthreads <= 1 {
            worker();
        } else {
            std::thread::scope(|sc| {
                let hs: Vec<_> = (0..nthreads).map(|_| sc.spawn(&worker)).collect();
                for h in hs {
                    if let Err(e) = h.join() {
                        std::panic::resume_unwind(e);
                    }
                }
            });
        }
        let q = lock.into_inner().unwrap_or_else(|e| e.into_inner());
        self.budget = q.budget;
        self.evals += q.evals;
        if q.exhausted {
            s.reset();
            self.exhausted = true;
        }
    }
}

fn run_worker(sh: &Shared, lock: &Mutex<Queue>, cv: &Condvar, m: &Module, cache: &Cache, s: &Summaries) {
    let mut q = lock.lock().unwrap();
    loop {
        if q.exhausted {
            return;
        }
        let Some(i) = sh.pop(&mut q) else {
            if q.nrunning == 0 {
                cv.notify_all();
                return;
            }
            q = cv.wait(q).unwrap();
            continue;
        };
        if q.budget == 0 {
            q.exhausted = true;
            cv.notify_all();
            return;
        }
        q.budget -= 1;
        q.evals += 1;
        q.queued[i] = false;
        q.running[i] = true;
        q.nrunning += 1;
        drop(q);
        let f = &m.funcs[i];
        let new = if f.params.is_empty() { Vec::new() } else { param_borrows(f, cache.get(m, i), s) };
        q = lock.lock().unwrap();
        if s.store(i, &new) {
            for &c in sh.callers[i].iter() {
                if sh.reach[c] {
                    sh.enqueue(&mut q, c);
                }
            }
        }
        q.running[i] = false;
        q.nrunning -= 1;
        if q.queued[i] {
            if q.blockers[i] == 0 {
                sh.push(&mut q, i);
            }
        } else {
            sh.retire(&mut q, i);
        }
        cv.notify_all();
    }
}
