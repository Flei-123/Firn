use super::index::{Bits, Index};
use crate::fir::{BlockId, Func};
use crate::fx::HashMap;

pub(crate) fn enabled() -> bool {
    std::env::var_os("FIRN_OWN_NO_CARRIED").is_none()
}

pub(crate) struct Labels {
    pub nd: usize,
    pub def_at: HashMap<(usize, usize), usize>,
    age: HashMap<(usize, usize), Bits>,
    bpos: HashMap<BlockId, usize>,
}

impl Labels {
    pub fn build(f: &Func, ix: &Index, site_at: &HashMap<(usize, usize), usize>) -> Option<Labels> {
        if !enabled() || site_at.is_empty() {
            return None;
        }
        let nb = f.blocks.len();
        let mut order = vec![usize::MAX; nb];
        for (k, &b) in ix.rpo.iter().enumerate() {
            order[b] = k;
        }
        let mut back = Vec::new();
        for &s in ix.rpo.iter() {
            for &t in ix.succs[s].iter() {
                if order[t] != usize::MAX && order[t] <= order[s] {
                    back.push((s, t));
                }
            }
        }
        if back.is_empty() {
            return None;
        }
        let entry = *ix.rpo.first()?;
        let mut avoid: HashMap<usize, Vec<bool>> = HashMap::default();
        for &(s, t) in back.iter() {
            let seen = avoid.entry(t).or_insert_with(|| reach_avoiding(ix, entry, t));
            if seen[s] {
                return None;
            }
        }
        let mut keys: Vec<(usize, usize)> = site_at.keys().copied().collect();
        keys.sort_unstable();
        let nd = keys.len();
        let def_at: HashMap<(usize, usize), usize> = keys.iter().enumerate().map(|(d, &at)| (at, d)).collect();
        let mut age: HashMap<(usize, usize), Bits> = HashMap::default();
        for &(s, t) in back.iter() {
            let body = natural_loop(ix, s, t);
            let mut m = Bits::new(2 * nd);
            for (d, at) in keys.iter().enumerate() {
                if body[at.0] {
                    m.set(d);
                }
            }
            if m.count() > 0 {
                age.insert((s, t), m);
            }
        }
        if age.is_empty() {
            return None;
        }
        let bpos = f.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
        Some(Labels { nd, def_at, age, bpos })
    }

    pub fn width(&self) -> usize {
        2 * self.nd
    }

    pub fn cur(&self, at: (usize, usize)) -> Option<usize> {
        self.def_at.get(&at).copied()
    }

    pub fn block_of(&self, id: BlockId) -> Option<usize> {
        self.bpos.get(&id).copied()
    }

    pub fn ages(&self, from: usize, to: usize) -> bool {
        self.age.contains_key(&(from, to))
    }

    pub fn age(&self, from: usize, to: usize, bits: &mut Bits) {
        let Some(m) = self.age.get(&(from, to)) else { return };
        for d in m.iter() {
            if bits.get(d) {
                bits.clear(d);
                bits.set(self.nd + d);
            }
        }
    }

    pub fn twins(&self, lab: &Bits) -> Bits {
        let mut t = lab.clone();
        for l in lab.iter() {
            t.set(l % self.nd);
            t.set(self.nd + l % self.nd);
        }
        t
    }
}

fn reach_avoiding(ix: &Index, entry: usize, t: usize) -> Vec<bool> {
    let mut seen = vec![false; ix.succs.len()];
    if entry == t {
        return seen;
    }
    let mut stack = vec![entry];
    seen[entry] = true;
    while let Some(b) = stack.pop() {
        for &s in ix.succs[b].iter() {
            if s != t && !seen[s] {
                seen[s] = true;
                stack.push(s);
            }
        }
    }
    seen
}

fn natural_loop(ix: &Index, s: usize, t: usize) -> Vec<bool> {
    let mut body = vec![false; ix.preds.len()];
    body[t] = true;
    let mut stack = Vec::new();
    if !body[s] {
        body[s] = true;
        stack.push(s);
    }
    while let Some(b) = stack.pop() {
        for &p in ix.preds[b].iter() {
            if !body[p] {
                body[p] = true;
                stack.push(p);
            }
        }
    }
    body
}

pub(crate) struct Class {
    pub lab: Bits,
    pub defs: Vec<(usize, usize)>,
}

pub(crate) fn classes(lb: &Labels, gens: impl Iterator<Item = Bits>) -> Vec<Class> {
    let n = lb.width();
    let mut uf: Vec<usize> = (0..n).collect();
    let mut used = vec![false; n];
    for g in gens {
        let mut first = None;
        for l in g.iter().filter(|&l| l < n) {
            used[l] = true;
            match first {
                None => first = Some(l),
                Some(a) => {
                    let (x, y) = (root(&mut uf, a), root(&mut uf, l));
                    if x != y {
                        uf[y] = x;
                    }
                }
            }
        }
    }
    let mut by_root: HashMap<usize, usize> = HashMap::default();
    let mut out: Vec<Class> = Vec::new();
    for l in 0..n {
        if !used[l] {
            continue;
        }
        let r = root(&mut uf, l);
        let k = *by_root.entry(r).or_insert_with(|| {
            out.push(Class { lab: Bits::new(n), defs: Vec::new() });
            out.len() - 1
        });
        out[k].lab.set(l);
    }
    let mut keys: Vec<(&(usize, usize), &usize)> = lb.def_at.iter().collect();
    keys.sort_unstable();
    for c in out.iter_mut() {
        for (&at, &d) in keys.iter() {
            if c.lab.get(d) || c.lab.get(lb.nd + d) {
                c.defs.push(at);
            }
        }
    }
    out.retain(|c| c.lab.iter().any(|l| l >= lb.nd));
    out
}

fn root(uf: &mut [usize], mut i: usize) -> usize {
    while uf[i] != i {
        uf[i] = uf[uf[i]];
        i = uf[i];
    }
    i
}
