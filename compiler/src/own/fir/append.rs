use crate::fir::{FTy, Func, Inst, Module, Op, Term, Val};
use crate::fx::HashMap;

pub(crate) const APPEND: &str = "__own_str_append";

const SOURCES: &[&str] = &["__str_concat", "__own_str_concat2", "__str_concat3", "__own_str_concat3", "__str_concat4", "__own_str_concat4"];

const RUNTIME_PREFIXES: &[&str] = &["__own_", "__str_", "__gc"];

const MAX_PARTS: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Drop {
    Keep = 0,
    Free = 1,
    Poison = 2,
}

pub(crate) fn run(m: &mut Module, free_fn: &str, poison_fn: &str) {
    if std::env::var_os("FIRN_OWN_NO_APPEND").is_some() || !m.funcs.iter().any(|f| f.name == APPEND) {
        return;
    }
    let (mut sites, mut fused) = (0usize, 0usize);
    for f in m.funcs.iter_mut() {
        if RUNTIME_PREFIXES.iter().any(|p| f.name.starts_with(p)) {
            continue;
        }
        let (s, d) = rewrite(f, free_fn, poison_fn);
        sites += s;
        fused += d;
    }
    if std::env::var_os("FIRN_OWN_STATS").is_some() {
        eprintln!("ownership fir: {} append sites, {} with fused drop", sites, fused);
    }
}

pub(crate) fn source(name: &str) -> bool {
    SOURCES.contains(&name)
}

fn defs(f: &Func) -> HashMap<Val, Op> {
    let mut out = HashMap::default();
    for b in f.blocks.iter() {
        for inst in b.insts.iter() {
            if let (Some(d), Op::Load { .. } | Op::PtrAdd { .. } | Op::Const(_)) = (inst.dst, &inst.op) {
                out.insert(d, inst.op.clone());
            }
        }
    }
    out
}

fn place(defs: &HashMap<Val, Op>, mut v: Val) -> (Val, i128) {
    let mut off = 0i128;
    for _ in 0..8 {
        let Some(Op::PtrAdd { base, off: o }) = defs.get(&v) else { break };
        let Some(Op::Const(c)) = defs.get(o) else { break };
        off += *c;
        v = *base;
    }
    (v, off)
}

fn loaded_from(defs: &HashMap<Val, Op>, v: Val, slot: (Val, i128)) -> bool {
    matches!(defs.get(&v), Some(Op::Load { addr }) if place(defs, *addr) == slot)
}

fn self_append(defs: &HashMap<Val, Op>, args: &[Val]) -> bool {
    if args.len() < 5 || args.len() % 2 == 0 || (args.len() - 1) / 2 > MAX_PARTS {
        return false;
    }
    let (base, off) = place(defs, args[0]);
    loaded_from(defs, args[1], (base, off)) && loaded_from(defs, args[2], (base, off + 8))
}

fn drop_after(f: &Func, bi: usize, ii: usize, old: Val, free_fn: &str, poison_fn: &str) -> Option<(usize, Drop)> {
    let insts = &f.blocks[bi].insts;
    for (j, inst) in insts.iter().enumerate().skip(ii + 1) {
        let Op::Call { name, args } = &inst.op else { return None };
        let kind = if name == free_fn {
            Drop::Free
        } else if name == poison_fn {
            Drop::Poison
        } else {
            return None;
        };
        if args.len() == 1 && args[0] == old {
            return Some((j, kind));
        }
    }
    None
}

fn succ_ids(t: &Term) -> Vec<u32> {
    match t {
        Term::Br(b) => vec![*b],
        Term::BrCond { then_bb, else_bb, .. } => vec![*then_bb, *else_bb],
        Term::Switch { cases, default, .. } => {
            let mut v: Vec<u32> = cases.iter().map(|c| c.1).collect();
            v.push(*default);
            v
        }
        _ => Vec::new(),
    }
}

fn on_cycle(f: &Func, start: usize) -> bool {
    let pos: HashMap<u32, usize> = f.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let mut seen = vec![false; f.blocks.len()];
    let mut stack: Vec<usize> = succ_ids(&f.blocks[start].term).iter().filter_map(|id| pos.get(id).copied()).collect();
    while let Some(b) = stack.pop() {
        if b == start {
            return true;
        }
        if seen[b] {
            continue;
        }
        seen[b] = true;
        stack.extend(succ_ids(&f.blocks[b].term).iter().filter_map(|id| pos.get(id).copied()));
    }
    false
}

fn rewrite(f: &mut Func, free_fn: &str, poison_fn: &str) -> (usize, usize) {
    let defs = defs(f);
    let (mut sites, mut fused) = (0usize, 0usize);
    let mut looped: Vec<Option<bool>> = vec![None; f.blocks.len()];
    for bi in 0..f.blocks.len() {
        let mut ii = 0;
        while ii < f.blocks[bi].insts.len() {
            let hit = match &f.blocks[bi].insts[ii].op {
                Op::Call { name, args } if SOURCES.contains(&name.as_str()) && self_append(&defs, args) => Some(args[1]),
                _ => None,
            };
            let Some(old) = hit else {
                ii += 1;
                continue;
            };
            if !*looped[bi].get_or_insert_with(|| on_cycle(f, bi)) {
                ii += 1;
                continue;
            }
            let mut kind = Drop::Keep;
            if let Some((j, k)) = drop_after(f, bi, ii, old, free_fn, poison_fn) {
                f.blocks[bi].insts.remove(j);
                kind = k;
                fused += 1;
            }
            let zero = f.new_val_pub(FTy::I64);
            let flag = f.new_val_pub(FTy::U64);
            let mut pre = vec![Inst::new(Some(zero), FTy::I64, Op::Const(0)), Inst::new(Some(flag), FTy::U64, Op::Const(kind as i128))];
            if let Op::Call { name, args } = &mut f.blocks[bi].insts[ii].op {
                *name = APPEND.to_string();
                while args.len() < 1 + 2 * MAX_PARTS {
                    args.push(zero);
                }
                args.push(flag);
            }
            let n = pre.len();
            f.blocks[bi].insts.splice(ii..ii, pre.drain(..));
            ii += n + 1;
            sites += 1;
        }
    }
    (sites, fused)
}
