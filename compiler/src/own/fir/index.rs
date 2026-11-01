use crate::fir::{Func, Op, Val};
use crate::fx::HashMap;

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct Bits {
    w: Vec<u64>,
}

impl Bits {
    pub fn new(n: usize) -> Bits {
        Bits { w: vec![0; (n + 63) / 64] }
    }
    pub fn set(&mut self, i: usize) {
        self.w[i / 64] |= 1 << (i % 64);
    }
    pub fn clear(&mut self, i: usize) {
        self.w[i / 64] &= !(1 << (i % 64));
    }
    pub fn get(&self, i: usize) -> bool {
        self.w.get(i / 64).map_or(false, |x| x & (1 << (i % 64)) != 0)
    }
    pub fn union(&mut self, o: &Bits) -> bool {
        let mut ch = false;
        for (a, b) in self.w.iter_mut().zip(o.w.iter()) {
            let n = *a | *b;
            ch |= n != *a;
            *a = n;
        }
        ch
    }
    pub fn count(&self) -> u32 {
        self.w.iter().map(|x| x.count_ones()).sum()
    }
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.w.iter().enumerate().flat_map(|(k, x)| (0..64).filter(move |b| x & (1 << b) != 0).map(move |b| k * 64 + b))
    }
}

pub(crate) struct Index {
    pub nvals: usize,
    pub preds: Vec<Vec<usize>>,
    pub succs: Vec<Vec<usize>>,
    pub rpo: Vec<usize>,
    pub def: Vec<Option<(usize, usize)>>,
    pub uses: Vec<u32>,
}

impl Index {
    pub fn build(f: &Func) -> Index {
        let nb = f.blocks.len();
        let mut bpos = HashMap::default();
        for (i, b) in f.blocks.iter().enumerate() {
            bpos.insert(b.id, i);
        }
        let mut succs = vec![Vec::new(); nb];
        let mut preds = vec![Vec::new(); nb];
        for (i, b) in f.blocks.iter().enumerate() {
            b.term.for_each_succ(|s| {
                if let Some(&j) = bpos.get(&s) {
                    if !succs[i].contains(&j) {
                        succs[i].push(j);
                        preds[j].push(i);
                    }
                }
            });
        }
        let nvals = f.val_types.len();
        let mut def = vec![None; nvals];
        let mut uses = vec![0u32; nvals];
        let mut buf = Vec::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, inst) in b.insts.iter().enumerate() {
                if let Some(d) = inst.dst {
                    if (d as usize) < nvals {
                        def[d as usize] = Some((bi, ii));
                    }
                }
                buf.clear();
                inst.op.uses(&mut buf);
                for v in buf.iter() {
                    if (*v as usize) < nvals {
                        uses[*v as usize] += 1;
                    }
                }
            }
            buf.clear();
            term_uses(&b.term, &mut buf);
            for v in buf.iter() {
                if (*v as usize) < nvals {
                    uses[*v as usize] += 1;
                }
            }
        }
        let rpo = rpo(nb, &succs, bpos.get(&f.entry()).copied().unwrap_or(0));
        Index { nvals, preds, succs, rpo, def, uses }
    }

    pub fn op<'a>(&self, f: &'a Func, v: Val) -> Option<&'a Op> {
        let (b, i) = (*self.def.get(v as usize)?)?;
        Some(&f.blocks[b].insts[i].op)
    }

    pub fn const_of(&self, f: &Func, v: Val) -> Option<i128> {
        match self.op(f, v)? {
            Op::Const(c) => Some(*c),
            _ => None,
        }
    }
}

pub(crate) fn term_uses(t: &crate::fir::Term, out: &mut Vec<Val>) {
    match t {
        crate::fir::Term::BrCond { cond, .. } => out.push(*cond),
        crate::fir::Term::Switch { val, .. } => out.push(*val),
        crate::fir::Term::Ret(Some(v)) => out.push(*v),
        _ => {}
    }
}

fn rpo(nb: usize, succs: &[Vec<usize>], entry: usize) -> Vec<usize> {
    let mut seen = vec![false; nb];
    let mut post = Vec::with_capacity(nb);
    let mut stack: Vec<(usize, usize)> = Vec::new();
    if nb == 0 {
        return post;
    }
    seen[entry] = true;
    stack.push((entry, 0));
    while let Some((b, k)) = stack.pop() {
        if k < succs[b].len() {
            stack.push((b, k + 1));
            let s = succs[b][k];
            if !seen[s] {
                seen[s] = true;
                stack.push((s, 0));
            }
        } else {
            post.push(b);
        }
    }
    post.reverse();
    post
}
