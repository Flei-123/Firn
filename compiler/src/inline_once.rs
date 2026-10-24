// SPDX-License-Identifier: MPL-2.0
//! `inline_once` -- a function with exactly ONE call site in the whole module, that site
//! inside a loop of the caller, is embedded however big it is (within `MAX_BODY_*`): the
//! call, the frame setup and the register saves are paid per iteration for nothing, and
//! the function itself disappears from the output. Not for exported functions, functions
//! whose address is taken, method table targets, recursive ones, bodies with inline asm.
//! Switch: `FIRN_NO_INLINE_ONCE=1`.

use crate::fir::{Module, Op, Term};
use crate::fx::HashMap;
use crate::inline::{allowed_at_all, inline_one, loop_blocks, name_index, CallGraph};

const MAX_BODY_INSTS: usize = 160;
const MAX_BODY_BLOCKS: usize = 24;
const MAX_CALLER_INSTS: usize = 12000;

fn find_call(m: &Module, ci: usize, name: &str) -> Option<(usize, usize)> {
    for (bi, b) in m.funcs[ci].blocks.iter().enumerate() {
        for (ii, i) in b.insts.iter().enumerate() {
            if let Op::Call { name: n, .. } = &i.op {
                if n == name {
                    return Some((bi, ii));
                }
            }
        }
    }
    None
}

fn body_ok(m: &Module, g: usize) -> bool {
    let f = &m.funcs[g];
    if !allowed_at_all(f) || f.inline_hint == Some(false) {
        return false;
    }
    if f.inst_count() > MAX_BODY_INSTS || f.blocks.len() > MAX_BODY_BLOCKS {
        return false;
    }
    let rets = f.blocks.iter().filter(|b| matches!(b.term, Term::Ret(_))).count();
    if f.ret != crate::fir::FTy::Void && rets != 1 {
        return false;
    }
    !f.blocks.iter().any(|b| {
        b.insts.iter().any(|i| {
            matches!(i.op, Op::Asm { .. } | Op::GcAddr { .. } | Op::ThreadSpawn { .. } | Op::ThreadSelf)
        })
    })
}

pub fn run(m: &mut Module) -> Vec<usize> {
    if std::env::var_os("FIRN_NO_INLINE_ONCE").is_some() || crate::target::shared() || crate::prof::is_kernel() {
        return Vec::new();
    }
    let n = m.funcs.len();
    let idx = name_index(m);
    let labels: HashMap<String, usize> =
        m.funcs.iter().enumerate().map(|(i, f)| (crate::codegen_x86::label(&f.name), i)).collect();
    let mut calls = vec![0u32; n];
    let mut home = vec![usize::MAX; n];
    let mut pinned = vec![false; n];
    for (i, f) in m.funcs.iter().enumerate() {
        if f.name == "main" || f.interrupt || crate::extfn::export_link_name(&f.name).is_some() {
            pinned[i] = true;
        }
    }
    for s in crate::prune::root_names() {
        if let Some(&i) = idx.get(s.as_str()) {
            pinned[i] = true;
        }
    }
    if let Some(&i) = idx.get(crate::thread::ENTRY) {
        pinned[i] = true;
    }
    for (ci, f) in m.funcs.iter().enumerate() {
        for b in &f.blocks {
            for i in &b.insts {
                match &i.op {
                    Op::Call { name, .. } => {
                        if let Some(&g) = idx.get(name.as_str()) {
                            calls[g] += 1;
                            home[g] = ci;
                        }
                    }
                    Op::FnRef { name } => {
                        if let Some(&g) = idx.get(name.as_str()) {
                            pinned[g] = true;
                        }
                    }
                    Op::Asm { template, .. } => crate::prune::idents(template, |s| {
                        if let Some(&g) = labels.get(s) {
                            pinned[g] = true;
                        }
                    }),
                    _ => {}
                }
            }
        }
    }
    let mut cands: Vec<usize> = (0..n)
        .filter(|&g| calls[g] == 1 && !pinned[g] && home[g] != g && body_ok(m, g))
        .collect();
    if cands.is_empty() {
        return Vec::new();
    }
    cands.sort_by_key(|&g| m.funcs[g].inst_count());
    let mut cg = CallGraph::new(m, &idx);
    let mut changed: Vec<usize> = Vec::new();
    for g in cands {
        let ci = home[g];
        if ci == usize::MAX || ci == g || cg.reaches(g, ci) || cg.self_recursive(ci) {
            continue;
        }
        let name = m.funcs[g].name.clone();
        let (bi, ii) = match find_call(m, ci, &name) {
            Some(x) => x,
            None => continue,
        };
        if m.funcs[ci].constant_time
            || !m.funcs[ci].secret.is_empty()
            || m.funcs[ci].inst_count() + m.funcs[g].inst_count() > MAX_CALLER_INSTS
            || m.funcs[ci].blocks.iter().enumerate().any(|(k, b)| b.id as usize != k)
        {
            continue;
        }
        match &m.funcs[ci].blocks[bi].insts[ii] {
            crate::fir::Inst { dst, op: Op::Call { args, .. }, .. } => {
                if args.len() != m.funcs[g].params.len()
                    || (dst.is_some() && m.funcs[g].ret == crate::fir::FTy::Void)
                {
                    continue;
                }
            }
            _ => continue,
        }
        if !loop_blocks(&m.funcs[ci])[bi] {
            continue;
        }
        inline_one(m, ci, bi, ii, g);
        for h in 0..n {
            if home[h] == g {
                home[h] = ci;
            }
        }
        changed.push(ci);
    }
    changed.sort();
    changed.dedup();
    changed
}
