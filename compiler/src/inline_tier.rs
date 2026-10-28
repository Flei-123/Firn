// SPDX-License-Identifier: MPL-2.0

use crate::fir::{FTy, Func, Module, Op};
use crate::fx::HashMap;
use crate::inline::{embed_body, inline_one, inlinable, CallGraph, MAX_CALLER_INSTS, REC_CALLER_TIER};

pub(crate) struct TierEnv<'a> {
    pub tier: usize,
    pub rec_mode: usize,
    pub idx: &'a HashMap<String, usize>,
    pub self_rec: &'a [bool],
    pub cg: &'a CallGraph,
}

pub(crate) struct Plan {
    sites: Vec<(usize, usize, usize)>,
    growth: Vec<usize>,
    body: Option<Func>,
    examined: Vec<usize>,
}

pub(crate) struct TierState<'a> {
    pub n: &'a mut usize,
    pub grown: &'a mut usize,
    pub budget: usize,
    pub max_growth: usize,
    pub changed: &'a mut Vec<usize>,
    pub kept: &'a mut HashMap<String, Func>,
}

fn plan_caller(m: &Module, ci: usize, env: &TierEnv) -> Plan {
    let mut plan = Plan { sites: Vec::new(), growth: Vec::new(), body: None, examined: Vec::new() };
    let first = &m.funcs[ci];
    if first.constant_time || !first.secret.is_empty() {
        return plan;
    }
    let caller_rec = env.self_rec[ci];
    if caller_rec && (env.rec_mode == 0 || (env.rec_mode == 1 && env.tier > REC_CALLER_TIER)) {
        return plan;
    }
    let mut start_b = 0usize;
    loop {
        let caller = plan.body.as_ref().unwrap_or(first);
        if caller.inst_count() > MAX_CALLER_INSTS {
            break;
        }
        if caller.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
            break;
        }
        let mut site: Option<(usize, usize, usize)> = None;
        'scan: for bi in start_b..caller.blocks.len() {
            for (ii, inst) in caller.blocks[bi].insts.iter().enumerate() {
                let (name, args) = match &inst.op {
                    Op::Call { name, args } => (name, args),
                    _ => continue,
                };
                let gi = match env.idx.get(name) {
                    Some(&g) => g,
                    None => continue,
                };
                if gi == ci {
                    continue;
                }
                if gi < ci {
                    plan.examined.push(gi);
                }
                let callee = &m.funcs[gi];
                if callee.inst_count() > env.tier || !inlinable(callee) {
                    continue;
                }
                if callee.params.len() != args.len() {
                    continue;
                }
                if inst.dst.is_some() && callee.ret == FTy::Void {
                    continue;
                }
                if caller_rec
                    && env.rec_mode == 1
                    && !(callee.blocks.len() == 1
                        && !callee.blocks[0].insts.iter().any(|i| matches!(i.op, Op::Alloca { .. })))
                {
                    continue;
                }
                if env.cg.reaches_shared(gi, ci) {
                    continue;
                }
                if env.self_rec[gi] {
                    continue;
                }
                site = Some((bi, ii, gi));
                break 'scan;
            }
        }
        match site {
            Some((bi, ii, gi)) => {
                plan.growth.push(m.funcs[gi].inst_count());
                plan.sites.push((bi, ii, gi));
                let mut w = plan.body.take().unwrap_or_else(|| first.clone());
                embed_body(&mut w, &m.funcs[gi], bi, ii, None);
                plan.body = Some(w);
                start_b = bi;
            }
            None => break,
        }
    }
    plan.examined.sort_unstable();
    plan.examined.dedup();
    plan
}

pub(crate) fn run_tier(m: &mut Module, env: &TierEnv, st: &mut TierState, threads: usize) -> bool {
    let nf = m.funcs.len();
    let all: Vec<usize> = (0..nf).collect();
    let ctx = crate::target::state();
    let mut plans: Vec<Option<Plan>> = {
        let shared: &Module = m;
        crate::parfn::map_slice_with(threads, &all, |&ci| {
            crate::target::set_state(ctx);
            Some(plan_caller(shared, ci, env))
        })
    };
    let mut modified = vec![false; nf];
    for ci in 0..nf {
        let planned = plans[ci].take().expect("plan");
        let plan = if planned.examined.iter().any(|&g| modified[g]) {
            plan_caller(m, ci, env)
        } else {
            planned
        };
        if plan.sites.is_empty() {
            continue;
        }
        let caller_rec = env.self_rec[ci];
        if caller_rec && env.rec_mode >= 2 && !st.kept.contains_key(&m.funcs[ci].name) {
            st.kept.insert(m.funcs[ci].name.clone(), m.funcs[ci].clone());
        }
        let total = plan.sites.len();
        let mut applied = 0usize;
        let mut stop = false;
        for k in 0..total {
            *st.grown += plan.growth[k];
            *st.n += 1;
            st.changed.push(ci);
            applied += 1;
            if *st.n >= st.budget || *st.grown >= st.max_growth {
                stop = true;
                break;
            }
        }
        if applied == total {
            m.funcs[ci] = plan.body.expect("planned body");
        } else {
            for &(bi, ii, gi) in &plan.sites[..applied] {
                inline_one(m, ci, bi, ii, gi);
            }
        }
        modified[ci] = true;
        if stop {
            return true;
        }
    }
    false
}
