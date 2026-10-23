// SPDX-License-Identifier: MPL-2.0
//! Guard split: the entry guard of a self-recursive function runs at the
//! recursive call sites instead of inside the callee.
//!
//! A function like `fib` starts with a cheap pure test that returns a value
//! computed from the arguments (`if n < 2 { return n }`). Most dynamic calls
//! hit that test, so most of the call cost (frame, saves, return) is spent on
//! calls that do nothing. The pass copies the test and its return value to
//! every self call site; only calls that fail the test stay real calls.
//!
//! The guard is found either in `bb0` (`brcond` right there) or, after
//! `tailrec` made a loop, in the loop header, where the header phis are
//! replaced by the values flowing in from `bb0`.

use crate::fir::{FTy, Func, Inst, Module, Op, Term, Val};
use crate::fx::{HashMap, HashSet};

const MAX_GUARD_INSTS: usize = 12;
const MAX_SITES_PER_FUNC: usize = 4;

struct Guard {
    insts: Vec<Inst>,
    b0_len: usize,
    phis: Vec<(Val, Val)>,
    cond: Val,
    ret_on_then: bool,
    ret_val: Val,
}

fn simple(op: &Op) -> bool {
    matches!(
        op,
        Op::Const(_) | Op::Bin(..) | Op::BinWrapSat { .. } | Op::Cmp { .. } | Op::Un(..) | Op::Cast { .. }
    )
}

fn preds_of(f: &Func, b: u32) -> usize {
    f.blocks.iter().filter(|x| x.term.successors().contains(&b)).count()
}

fn find_guard(f: &Func) -> Option<Guard> {
    if f.constant_time || !f.secret.is_empty() || f.interrupt || f.inline_hint == Some(false) {
        return None;
    }
    let b0 = f.blocks.first()?;
    if b0.insts.iter().any(|i| !simple(&i.op)) {
        return None;
    }
    let mut insts: Vec<Inst> = b0.insts.clone();
    let b0_len = insts.len();
    let mut phis: Vec<(Val, Val)> = Vec::new();
    let (gblock, gid) = match &b0.term {
        Term::BrCond { .. } => (b0, 0u32),
        Term::Br(h) if *h != 0 => {
            let hb = f.blocks.get(*h as usize)?;
            let np = hb.phi_count();
            for i in &hb.insts[..np] {
                if let (Some(d), Op::Phi { incoming }) = (i.dst, &i.op) {
                    let (_, v) = incoming.iter().find(|(p, _)| *p == 0)?;
                    phis.push((d, *v));
                }
            }
            if hb.insts[np..].iter().any(|i| !simple(&i.op)) {
                return None;
            }
            insts.extend(hb.insts[np..].iter().cloned());
            (hb, *h)
        }
        _ => return None,
    };
    let (cond, then_bb, else_bb) = match &gblock.term {
        Term::BrCond { cond, then_bb, else_bb } => (*cond, *then_bb, *else_bb),
        _ => return None,
    };
    for (rb, on_then) in [(then_bb, true), (else_bb, false)] {
        if rb == gid || rb == 0 {
            continue;
        }
        let r = match f.blocks.get(rb as usize) {
            Some(r) => r,
            None => continue,
        };
        let rv = match &r.term {
            Term::Ret(Some(v)) => *v,
            _ => continue,
        };
        if r.phi_count() > 0 || r.insts.iter().any(|i| !simple(&i.op)) || preds_of(f, rb) != 1 {
            continue;
        }
        let mut all = insts.clone();
        all.extend(r.insts.iter().cloned());
        if all.len() > MAX_GUARD_INSTS {
            return None;
        }
        return Some(Guard { insts: all, b0_len, phis, cond, ret_on_then: on_then, ret_val: rv });
    }
    None
}

fn expand_site(f: &mut Func, g: &Guard, bi: usize, ii: usize) -> Val {
    let (args, dst, ty, loc, name) = match &f.blocks[bi].insts[ii] {
        Inst { dst: Some(d), ty, op: Op::Call { args, name }, loc } => (args.clone(), *d, *ty, *loc, name.clone()),
        _ => unreachable!(),
    };
    let mut map: HashMap<Val, Val> = HashMap::default();
    for (k, a) in args.iter().enumerate() {
        map.insert(k as Val, *a);
    }
    let mut copies: Vec<Inst> = Vec::new();
    for (idx, i) in g.insts.iter().enumerate() {
        if idx == g.b0_len {
            for (d, v) in &g.phis {
                let r = *map.get(v).unwrap_or(v);
                map.insert(*d, r);
            }
        }
        let mv = |v: Val| *map.get(&v).unwrap_or(&v);
        let mut op = i.op.clone();
        remap(&mut op, &mv);
        let nd = i.dst.map(|d| {
            let nv = f.val_types.len() as Val;
            f.val_types.push(f.val_types.get(d as usize).copied().unwrap_or(i.ty));
            nv
        });
        if let (Some(d), Some(n)) = (i.dst, nd) {
            map.insert(d, n);
        }
        copies.push(Inst { dst: nd, ty: i.ty, op, loc: i.loc });
    }
    if g.insts.len() <= g.b0_len {
        for (d, v) in &g.phis {
            let r = *map.get(v).unwrap_or(v);
            map.insert(*d, r);
        }
    }
    let mv = |v: Val| *map.get(&v).unwrap_or(&v);
    let cond = mv(g.cond);
    let fast_val = mv(g.ret_val);

    let slow_b = f.add_block();
    let join = f.add_block();
    let tail: Vec<Inst> = f.blocks[bi].insts.split_off(ii + 1);
    f.blocks[bi].insts.pop();
    let old_term = std::mem::replace(&mut f.blocks[bi].term, Term::Unset);
    if let Some(p) = copies.iter().position(|i| i.dst == Some(cond)) {
        let c = copies.remove(p);
        copies.push(c);
    }
    f.blocks[bi].insts.extend(copies);
    let (t, e) = if g.ret_on_then { (join, slow_b) } else { (slow_b, join) };
    f.blocks[bi].term = Term::BrCond { cond, then_bb: t, else_bb: e };
    let slow_dst = f.val_types.len() as Val;
    f.val_types.push(ty);
    f.blocks[slow_b as usize].insts.push(Inst { dst: Some(slow_dst), ty, op: Op::Call { name, args }, loc });
    f.blocks[slow_b as usize].term = Term::Br(join);
    let mut jinsts = vec![Inst { dst: Some(dst), ty, op: Op::Phi { incoming: vec![(bi as u32, fast_val), (slow_b, slow_dst)] }, loc }];
    jinsts.extend(tail);
    f.blocks[join as usize].insts = jinsts;
    f.blocks[join as usize].term = old_term.clone();
    for sb in old_term.successors() {
        let sb = sb as usize;
        if sb >= f.blocks.len() {
            continue;
        }
        let np = f.blocks[sb].phi_count();
        for i in f.blocks[sb].insts[..np].iter_mut() {
            if let Op::Phi { incoming } = &mut i.op {
                for e in incoming.iter_mut() {
                    if e.0 as usize == bi {
                        e.0 = join;
                    }
                }
                incoming.sort_by_key(|(p, _)| *p);
            }
        }
    }
    slow_dst
}

fn remap(op: &mut Op, mv: &dyn Fn(Val) -> Val) {
    match op {
        Op::Bin(_, a, b) => {
            *a = mv(*a);
            *b = mv(*b);
        }
        Op::BinWrapSat { a, b, .. } => {
            *a = mv(*a);
            *b = mv(*b);
        }
        Op::Cmp { a, b, .. } => {
            *a = mv(*a);
            *b = mv(*b);
        }
        Op::Un(_, a) => *a = mv(*a),
        Op::Cast { src, .. } => *src = mv(*src),
        _ => {}
    }
}

fn run_func(f: &mut Func) -> usize {
    let g = match find_guard(f) {
        Some(g) => g,
        None => return 0,
    };
    let me = f.name.clone();
    let mut done: HashSet<Val> = HashSet::default();
    let mut n = 0;
    while n < MAX_SITES_PER_FUNC {
        let mut site = None;
        'scan: for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, i) in b.insts.iter().enumerate() {
                if let (Some(d), Op::Call { name, args }) = (i.dst, &i.op) {
                    if *name == me && args.len() == f.params.len() && !done.contains(&d) && i.ty != FTy::Void {
                        site = Some((bi, ii));
                        break 'scan;
                    }
                }
            }
        }
        let (bi, ii) = match site {
            Some(s) => s,
            None => break,
        };
        let d = expand_site(f, &g, bi, ii);
        done.insert(d);
        n += 1;
    }
    n
}

/// Returns the indices of the functions that changed.
pub fn run(m: &mut Module) -> Vec<usize> {
    let mut changed = Vec::new();
    for (i, f) in m.funcs.iter_mut().enumerate() {
        if run_func(f) > 0 {
            changed.push(i);
        }
    }
    changed
}
