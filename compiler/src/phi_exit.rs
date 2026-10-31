// SPDX-License-Identifier: MPL-2.0
//! Splits loop exit edges that carry phi copies, so the copies run once at
//! the exit instead of on every pass through the exiting block.
//! Switch: `FIRN_NO_EXIT_SPLIT`.

use crate::fir::{Func, Op};

fn enabled() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("FIRN_NO_EXIT_SPLIT").is_none())
}

fn carries_copy(f: &Func, p: usize, s: usize) -> bool {
    f.blocks[s].insts.iter().any(|i| match (&i.op, i.dst) {
        (Op::Phi { incoming }, Some(d)) => incoming.iter().any(|(q, v)| *q as usize == p && *v != d),
        _ => false,
    })
}

fn loop_bodies(f: &Func) -> Vec<Vec<bool>> {
    let n = f.blocks.len();
    let preds = crate::mem2reg::preds(f);
    let dom = crate::mem2reg::dominators(f);
    let mut out = Vec::new();
    for (b, blk) in f.blocks.iter().enumerate() {
        for s in blk.term.successors() {
            let h = s as usize;
            if h < n && dom.has(b, h) {
                let mut body = vec![false; n];
                body[h] = true;
                let mut stack = Vec::new();
                if !body[b] {
                    body[b] = true;
                    stack.push(b);
                }
                while let Some(x) = stack.pop() {
                    for &q in &preds[x] {
                        if !body[q] {
                            body[q] = true;
                            stack.push(q);
                        }
                    }
                }
                out.push(body);
            }
        }
    }
    out
}

fn exit_edges(f: &Func) -> Vec<(usize, usize)> {
    let n = f.blocks.len();
    let mut cand: Vec<(usize, usize)> = Vec::new();
    for (p, blk) in f.blocks.iter().enumerate() {
        let mut ss = blk.term.successors();
        ss.sort_unstable();
        ss.dedup();
        if ss.len() < 2 {
            continue;
        }
        for s in ss {
            let s = s as usize;
            if s < n && f.blocks[s].phi_count() > 0 && carries_copy(f, p, s) {
                cand.push((p, s));
            }
        }
    }
    if cand.is_empty() {
        return cand;
    }
    let bodies = loop_bodies(f);
    cand.retain(|&(p, s)| bodies.iter().any(|body| body[p] && !body[s]));
    cand
}

pub(crate) fn split_loop_exit_edges(f: &mut Func) -> usize {
    if !enabled() || f.blocks.len() < 2 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    let mut edges = exit_edges(f);
    edges.sort_unstable_by(|a, b| b.cmp(a));
    let mut done = 0;
    for k in 0..edges.len() {
        let (p, s) = edges[k];
        let z = crate::promote::split_edge(f, p, s);
        crate::inline::reorder_blocks(f, p, &[z as u32]);
        for e in edges.iter_mut().skip(k + 1) {
            if e.1 > p {
                e.1 += 1;
            }
            if e.0 > p {
                e.0 += 1;
            }
        }
        done += 1;
    }
    done
}
