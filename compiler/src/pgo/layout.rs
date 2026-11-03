// SPDX-License-Identifier: MPL-2.0

use crate::fir::{Func, Term};

fn weight(w: &[Option<u64>], b: usize) -> u64 {
    w.get(b).copied().flatten().unwrap_or(1)
}

fn cold(w: &[Option<u64>], b: usize) -> bool {
    matches!(w.get(b).copied().flatten(), Some(0))
}

fn best_succ(f: &Func, w: &[Option<u64>], placed: &[bool], b: usize) -> Option<usize> {
    let n = f.blocks.len();
    let mut cands: Vec<usize> = Vec::new();
    match &f.blocks[b].term {
        Term::Br(t) => cands.push(*t as usize),
        Term::BrCond { then_bb, else_bb, .. } => {
            cands.push(*else_bb as usize);
            cands.push(*then_bb as usize);
        }
        Term::Switch { cases, default, .. } => {
            cands.push(*default as usize);
            cands.extend(cases.iter().map(|(_, t)| *t as usize));
        }
        Term::Ret(_) | Term::Unset => {}
    }
    let mut best: Option<usize> = None;
    for c in cands {
        if c >= n || placed[c] || cold(w, c) {
            continue;
        }
        if best.map_or(true, |x| weight(w, c) > weight(w, x)) {
            best = Some(c);
        }
    }
    best
}

pub fn emit_order(f: &Func) -> Option<Vec<usize>> {
    let w = super::weights::block_weights(f)?;
    let n = f.blocks.len();
    let mut placed = vec![false; n];
    let mut out: Vec<usize> = Vec::with_capacity(n);
    let mut b = 0usize;
    loop {
        loop {
            placed[b] = true;
            out.push(b);
            match best_succ(f, &w, &placed, b) {
                Some(t) => b = t,
                None => break,
            }
        }
        let next = (0..n).filter(|&i| !placed[i] && !cold(&w, i)).max_by_key(|&i| (weight(&w, i), std::cmp::Reverse(i)));
        match next {
            Some(i) => b = i,
            None => break,
        }
    }
    for i in 0..n {
        if !placed[i] {
            out.push(i);
        }
    }
    Some(out)
}
