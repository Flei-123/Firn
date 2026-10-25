// SPDX-License-Identifier: MPL-2.0
//! Heat-ordered continuation of the size-ordered inliner phase (`inline_by_size`).
//!
//! When the growth guard of the size phase stops it inside a tier, the remaining
//! budget is not spent by callee size alone: every call site gets an estimated
//! execution count (loop sites weigh `LOOP_WEIGHT`, a callee inherits the counts of
//! its callers through the call graph) and the sites with the best count per copied
//! instruction are embedded until the extra budget is used up.

use crate::fir::{FTy, Func, Module, Op};
use crate::fx::HashMap;
use crate::inline::{inline_one, inlinable, loop_blocks, CallGraph};

const LOOP_WEIGHT: f64 = 8.0;
const HEAT_ROUNDS: usize = 6;
const HEAT_CAP: f64 = 1e12;
const MAX_CALLER_INSTS: usize = 24000;

pub(crate) struct HeatContext<'a> {
    pub idx: &'a HashMap<String, usize>,
    pub cg: &'a mut CallGraph,
    pub self_rec: &'a mut Vec<Option<bool>>,
    pub kept: &'a mut HashMap<String, Func>,
    pub rec_mode: usize,
    pub max_sites: usize,
    pub max_copied: usize,
}

pub(crate) struct HeatResult {
    pub sites: usize,
    pub copied: usize,
    pub threshold: f64,
}

fn site_weight(hot: &[bool], bi: usize) -> f64 {
    if hot.get(bi).copied().unwrap_or(false) {
        LOOP_WEIGHT
    } else {
        1.0
    }
}

fn call_target(m: &Module, idx: &HashMap<String, usize>, ci: usize, inst: &crate::fir::Inst) -> Option<usize> {
    let (name, args) = match &inst.op {
        Op::Call { name, args } => (name, args),
        _ => return None,
    };
    let gi = *idx.get(name)?;
    if gi == ci {
        return None;
    }
    let callee = &m.funcs[gi];
    if !inlinable(callee) || callee.params.len() != args.len() {
        return None;
    }
    if inst.dst.is_some() && callee.ret == FTy::Void {
        return None;
    }
    Some(gi)
}

fn eligible_caller(f: &Func) -> bool {
    !f.constant_time
        && f.secret.is_empty()
        && f.inst_count() <= MAX_CALLER_INSTS
        && f.blocks.iter().enumerate().all(|(i, b)| b.id as usize == i)
}

fn function_heat(m: &Module, idx: &HashMap<String, usize>, hot: &[Vec<bool>]) -> Vec<f64> {
    let nf = m.funcs.len();
    let mut edges: Vec<(usize, usize, f64)> = Vec::new();
    for (ci, f) in m.funcs.iter().enumerate() {
        for (bi, b) in f.blocks.iter().enumerate() {
            let w = site_weight(&hot[ci], bi);
            for inst in &b.insts {
                if let Op::Call { name, .. } = &inst.op {
                    if let Some(&gi) = idx.get(name) {
                        edges.push((ci, gi, w));
                    }
                }
            }
        }
    }
    let mut heat = vec![1.0f64; nf];
    for _ in 0..HEAT_ROUNDS {
        let mut next = vec![1.0f64; nf];
        for &(f, g, w) in &edges {
            next[g] = (next[g] + w * heat[f]).min(HEAT_CAP);
        }
        heat = next;
    }
    heat
}

fn score_threshold(m: &Module, idx: &HashMap<String, usize>, hot: &[Vec<bool>], heat: &[f64], max_copied: usize) -> f64 {
    let mut cands: Vec<(f64, usize)> = Vec::new();
    for (ci, f) in m.funcs.iter().enumerate() {
        if !eligible_caller(f) {
            continue;
        }
        for (bi, b) in f.blocks.iter().enumerate() {
            let w = site_weight(&hot[ci], bi);
            for inst in &b.insts {
                if let Some(gi) = call_target(m, idx, ci, inst) {
                    let cost = m.funcs[gi].inst_count().max(1);
                    cands.push((heat[ci] * w / cost as f64, cost));
                }
            }
        }
    }
    cands.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut used = 0usize;
    for &(score, cost) in &cands {
        used += cost;
        if used > max_copied {
            return score;
        }
    }
    0.0
}

pub(crate) fn run(m: &mut Module, ctx: &mut HeatContext, changed: &mut Vec<usize>) -> HeatResult {
    let nf = m.funcs.len();
    let mut hot: Vec<Vec<bool>> = m
        .funcs
        .iter()
        .map(|f| {
            let has_call = f.blocks.iter().any(|b| b.insts.iter().any(|i| matches!(i.op, Op::Call { .. })));
            if has_call && f.blocks.iter().enumerate().all(|(i, b)| b.id as usize == i) {
                loop_blocks(f)
            } else {
                Vec::new()
            }
        })
        .collect();
    let heat = function_heat(m, ctx.idx, &hot);
    let threshold = score_threshold(m, ctx.idx, &hot, &heat, ctx.max_copied);
    let mut sites = 0usize;
    let mut copied = 0usize;
    for ci in 0..nf {
        if !eligible_caller(&m.funcs[ci]) {
            continue;
        }
        let caller_rec = match ctx.self_rec[ci] {
            Some(v) => v,
            None => {
                let v = ctx.cg.self_recursive(ci);
                ctx.self_rec[ci] = Some(v);
                v
            }
        };
        if caller_rec && ctx.rec_mode < 2 {
            continue;
        }
        let mut start_b = 0usize;
        loop {
            if sites >= ctx.max_sites || copied >= ctx.max_copied {
                return HeatResult { sites, copied, threshold };
            }
            if !eligible_caller(&m.funcs[ci]) {
                break;
            }
            let mut found: Option<(usize, usize, usize)> = None;
            {
                let caller = &m.funcs[ci];
                'scan: for bi in start_b..caller.blocks.len() {
                    let w = site_weight(&hot[ci], bi);
                    for (ii, inst) in caller.blocks[bi].insts.iter().enumerate() {
                        let gi = match call_target(m, ctx.idx, ci, inst) {
                            Some(g) => g,
                            None => continue,
                        };
                        let cost = m.funcs[gi].inst_count().max(1);
                        if heat[ci] * w / (cost as f64) < threshold {
                            continue;
                        }
                        if ctx.cg.reaches(gi, ci) {
                            continue;
                        }
                        let sr = match ctx.self_rec[gi] {
                            Some(v) => v,
                            None => {
                                let v = ctx.cg.self_recursive(gi);
                                ctx.self_rec[gi] = Some(v);
                                v
                            }
                        };
                        if sr {
                            continue;
                        }
                        found = Some((bi, ii, gi));
                        break 'scan;
                    }
                }
            }
            let (bi, ii, gi) = match found {
                Some(s) => s,
                None => break,
            };
            copied += m.funcs[gi].inst_count();
            if caller_rec && !ctx.kept.contains_key(&m.funcs[ci].name) {
                ctx.kept.insert(m.funcs[ci].name.clone(), m.funcs[ci].clone());
            }
            inline_one(m, ci, bi, ii, gi);
            changed.push(ci);
            sites += 1;
            start_b = bi;
            hot[ci] = loop_blocks(&m.funcs[ci]);
        }
    }
    HeatResult { sites, copied, threshold }
}
