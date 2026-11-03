use crate::fir::{Func, Op, Val};
use crate::fx::HashMap;

pub(crate) use super::bits::Bits;

const NO_DEF: u32 = u32::MAX;

pub(crate) struct Index {
    pub nvals: usize,
    pub preds: Vec<Vec<usize>>,
    pub succs: Vec<Vec<usize>>,
    pub rpo: Vec<usize>,
    pub def: Vec<(u32, u32)>,
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
        let mut def = vec![(NO_DEF, NO_DEF); nvals];
        let mut uses = vec![0u32; nvals];
        let mut buf = Vec::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, inst) in b.insts.iter().enumerate() {
                if let Some(d) = inst.dst {
                    if (d as usize) < nvals {
                        def[d as usize] = (bi as u32, ii as u32);
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

    pub fn def_of(&self, v: usize) -> Option<(usize, usize)> {
        let &(b, i) = self.def.get(v)?;
        if b == NO_DEF {
            None
        } else {
            Some((b as usize, i as usize))
        }
    }

    pub fn op<'a>(&self, f: &'a Func, v: Val) -> Option<&'a Op> {
        let (b, i) = self.def_of(v as usize)?;
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
