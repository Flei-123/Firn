// SPDX-License-Identifier: GPL-2.0-only
//! Inlining (embedding function bodies) with a size heuristic.
//!
//! How it works: an `Op::Call` to a function present in the same
//! `fir::Module` is replaced by a copy of its body.
//!
//!  * The calling block is **split** at the call site; the body of the
//!    called function is inserted between as a block group of its own.
//!  * FIR knows no phi nodes. The return value therefore travels through an
//!    `alloca` in the entry block of the caller: every `ret v` of the body
//!    becomes `store slot, v` + `br <continuation>`, and the original result
//!    value is defined at the start of the continuation with `load slot`.
//!    Given exactly one `ret`, `mem2reg` (alloca written once) resolves this
//!    detour straight away again.
//!  * `alloca`s of the body move to the entry block of the caller
//!    (FIR invariant: all `alloca` stand in `bb0`).
//!
//! **Module boundaries:** the module system compiles all `.fi` files into ONE
//! `fir::Module` (separate compilation, shared module). Every call of an
//! imported function is therefore just as visible to this pass as a local
//! one — inlining works across module boundaries.
//!
//! Heuristic (deliberately conservative, so that compile time and code size
//! do not explode):
//!  * body at most `MAX_CALLEE_INSTS` instructions,
//!    blocks at most `MAX_CALLEE_BLOCKS`.
//!  * caller at most `MAX_CALLER_INSTS` instructions (stop after that).
//!  * no recursion: if the called function can reach the caller again
//!    through the call graph, nothing is embedded.
//!  * functions with `secret` values or `#[constant_time]` stay outside
//!    (SPEC §9: the check in the code generator works per function).
//!  * at most `MAX_INLINES` embeddings per module.

use crate::fir::{FTy, Func, Inst, Module, Op, Term, Val};
use std::collections::{HashMap, HashSet};

const MAX_CALLEE_INSTS: usize = 40;
const MAX_CALLEE_BLOCKS: usize = 8;
/// Upper bound for the CALLER. It protects compile time and code size — but
/// it must not lock out the hottest function of the program.
///
///
/// MEASURED (14.08.2026): the HTML5 tokenizer `tokenizer__tokenize` has 4.139
/// FIR instructions. With the old bound of 4.000 exactly the function that
/// touches every character of every page got NOT a single embedding —
/// although `sink_emit_char` with 18 instructions and one block sits far
/// below every callee bound. A big function is not automatically cold; with
/// a state machine the opposite holds.
const MAX_CALLER_INSTS: usize = 24000;
const MAX_INLINES: usize = 2000;

/// Can `from` reach `to` through calls?
fn reaches(m: &Module, from: &str, to: &str) -> bool {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut stack = vec![from];
    while let Some(cur) = stack.pop() {
        if cur == to {
            return true;
        }
        if !seen.insert(cur) {
            continue;
        }
        if let Some(f) = m.funcs.iter().find(|f| f.name == cur) {
            for b in &f.blocks {
                for i in &b.insts {
                    if let Op::Call { name, .. } = &i.op {
                        stack.push(name.as_str());
                    }
                }
            }
        }
    }
    false
}

/// Can a function reach itself again through at least one call (direct or
/// indirect recursion)?
///
/// Such bodies are NOT embedded. Inlining unrolls one recursion level and
/// moves its frames into the caller — program code whose effect rests on the
/// stack depth (the stack scrubbing of the conservative GC, `lib/gc`:
/// `__gc_scrub_deep`) loses its effect that way. MEASURED in round 37: with
/// raised bounds (60/10) `__gc_scrub_deep` (29 insts, 9 blocks, recursive)
/// got embedded into `main` — `tests/520_gc_weak.fi` failed with exit 6,
/// because phantom pointers in the unscrubbed stack fed the collector.
///
fn reaches_itself_self(m: &Module, name: &str) -> bool {
    if let Some(f) = m.funcs.iter().find(|f| f.name == name) {
        for b in &f.blocks {
            for i in &b.insts {
                if let Op::Call { name: target, .. } = &i.op {
                    if reaches(m, target, name) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn inlinable(callee: &Func) -> bool {
    // Loop free bodies WITHOUT a return value (effect through pointer
    // arguments, say the sink mutators of the tokenizer) may have more
    // blocks: their control flow is a DAG, and because `dst` is empty, not
    // even the result alloca comes about in the caller — the frame of the
    // caller stays unchanged apart from real body allocas. That is the
    // difference to value bodies: their result cell moves into the entry
    // block of the caller and changes its frame layout — which is fatal
    // for the stack scanning conservative GC (`tests/520_gc_weak.fi`,
    // round 37: `__gc_strong_raw` inlined into `create` produced phantom
    // pointers and exit 6).
    !callee.constant_time
        && callee.secret.is_empty()
        && callee.inst_count() <= MAX_CALLEE_INSTS
        && callee.blocks.len() <= MAX_CALLEE_BLOCKS
        && !callee.blocks.iter().any(|b| matches!(b.term, Term::Unset))
        && callee.blocks.iter().enumerate().all(|(i, b)| b.id as usize == i)
}

/// Looks for a worthwhile call site in the caller `ci`.
/// `self_rec`: precomputed per function (does not change through
/// embeddings — only the caller is mutated).
fn find_site(m: &Module, ci: usize, self_rec: &[bool]) -> Option<(usize, usize, usize)> {
    let caller = &m.funcs[ci];
    if caller.constant_time || !caller.secret.is_empty() {
        return None;
    }
    if caller.inst_count() > MAX_CALLER_INSTS {
        return None;
    }
    if caller.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return None;
    }
    for (bi, b) in caller.blocks.iter().enumerate() {
        for (ii, inst) in b.insts.iter().enumerate() {
            if let Op::Call { name, args } = &inst.op {
                let gi = match m.funcs.iter().position(|f| &f.name == name) {
                    Some(g) => g,
                    None => continue,
                };
                let callee = &m.funcs[gi];
                if gi == ci || !inlinable(callee) {
                    continue;
                }
                if callee.params.len() != args.len() {
                    continue;
                }
                if inst.dst.is_some() && callee.ret == FTy::Void {
                    continue;
                }
                // Recursion (indirect one too) is not embedded.
                if reaches(m, &callee.name, &caller.name) {
                    continue;
                }
                // Self reachable bodies neither (see above).
                if self_rec[gi] {
                    continue;
                }
                return Some((bi, ii, gi));
            }
        }
    }
    None
}

/// Embeds exactly one call site.
fn inline_one(m: &mut Module, ci: usize, bi: usize, mut ii: usize, gi: usize) {
    let callee = m.funcs[gi].clone();
    let (args, dst, ret_ty, call_loc) = match &m.funcs[ci].blocks[bi].insts[ii] {
        // ROUND 94: the position of the CALL. Everything that belongs to the
        // call itself (the result travelling back) keeps it; everything that
        // belongs to the callee's body keeps the callee's own position.
        Inst { dst, ty, op: Op::Call { args, .. }, loc } => (args.clone(), *dst, *ty, *loc),
        _ => return,
    };

    // 1. Create the result slot and the body allocas in the entry block.
    //    `Func::alloca` inserts at the front — that shifts the call site
    //    when it sits in bb0 itself.
    let mut shift = 0usize;
    let result_slot = if dst.is_some() {
        shift += 1;
        Some(m.funcs[ci].alloca(ret_ty.bytes().max(1), ret_ty.bytes().max(1)))
    } else {
        None
    };
    let mut valmap: HashMap<Val, Val> = HashMap::new();
    for (k, a) in args.iter().enumerate() {
        valmap.insert(k as Val, *a);
    }
    for b in &callee.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Alloca { size, align }) = (i.dst, &i.op) {
                let nv = m.funcs[ci].alloca(*size, *align);
                valmap.insert(d, nv);
                shift += 1;
            }
        }
    }
    if bi == 0 {
        ii += shift;
    }

    // 2. Map the remaining values of the body onto new ids.
    let f = &mut m.funcs[ci];
    for b in &callee.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if valmap.contains_key(&d) {
                    continue;
                }
                let nv = f.val_types.len() as Val;
                f.val_types.push(callee.val_types.get(d as usize).copied().unwrap_or(FTy::Void));
                valmap.insert(d, nv);
            }
        }
    }
    let mv = |v: Val| -> Val { valmap.get(&v).copied().unwrap_or(v) };

    // 3. Create the blocks of the body + the continuation block.
    let mut blockmap: HashMap<u32, u32> = HashMap::new();
    for b in &callee.blocks {
        let nb = f.add_block();
        blockmap.insert(b.id, nb);
    }
    let cont = f.add_block();

    // 4. Split the calling block.
    let tail: Vec<Inst> = f.blocks[bi].insts.split_off(ii + 1);
    f.blocks[bi].insts.pop(); // the `call` itself falls away
    let old_term = std::mem::replace(&mut f.blocks[bi].term, Term::Br(blockmap[&callee.entry()]));
    f.blocks[cont as usize].insts = tail;
    f.blocks[cont as usize].term = old_term.clone();

    // ROUND 92 -- THE CALLING BLOCK IS NOT THE PREDECESSOR ANY MORE.
    //
    // Splitting `bi` at the call site moves its TERMINATOR into `cont`. So
    // everything `bi` used to jump to is now jumped to by `cont`, and a phi
    // in one of those blocks still names `bi` as the edge its value comes
    // in on. Re-key it.
    //
    // Found by `tests/303_wtf8_roundtrip.fi`, which printed
    // `0 0 0 0 2048 1112064` instead of `65536 2048 2049 0 1114112 0` at
    // `release-safe` and `release-fast` -- the two levels that inline -- and
    // was right at `dev` and `dev-fast`, which do not. `phi.rs` then put the
    // copy for that edge at the end of a block the control flow no longer
    // takes, so the phi's value was whatever the other edge had left behind.
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
                        e.0 = cont;
                    }
                }
                incoming.sort_by_key(|(p, _)| *p);
            }
        }
    }

    // 5. Define the result value at the start of the continuation.
    // ROUND 94: the detour load belongs to the CALL, not to the callee -- it
    // is the value arriving back at the call site.
    if let (Some(d), Some(slot)) = (dst, result_slot) {
        f.blocks[cont as usize].insts.insert(
            0,
            Inst::like(Some(d), ret_ty, Op::Load { addr: slot }, call_loc),
        );
    }

    // 6. Copy the body.
    for b in &callee.blocks {
        let nb = blockmap[&b.id] as usize;
        for i in &b.insts {
            if matches!(i.op, Op::Alloca { .. }) {
                continue; // stands at the entry block already
            }
            let op = remap_op(&i.op, &mv, Some(&blockmap));
            // ROUND 94 -- THE POINT OF THE ROUND. The copied instruction keeps
            // the position it has in the CALLEE. Without this line the
            // debugger reports the caller's line for code that stands
            // somewhere else entirely, which is exactly the lie that started
            // this round (`fir::Loc`).
            f.blocks[nb].insts.push(Inst::like(i.dst.map(&mv), i.ty, op, i.loc));
        }
        f.blocks[nb].term = match &b.term {
            Term::Br(t) => Term::Br(blockmap[t]),
            Term::BrCond { cond, then_bb, else_bb } => Term::BrCond {
                cond: mv(*cond),
                then_bb: blockmap[then_bb],
                else_bb: blockmap[else_bb],
            },
            Term::Switch { val, ty, cases, default } => Term::Switch {
                val: mv(*val),
                ty: *ty,
                cases: cases.iter().map(|(k, t)| (*k, blockmap[t])).collect(),
                default: blockmap[default],
            },
            Term::Ret(v) => {
                if let (Some(v), Some(slot)) = (v, result_slot) {
                    // The `ret` of the callee: its own position, not the
                    // caller's.
                    f.blocks[nb].insts.push(Inst::like(
                        None,
                        ret_ty,
                        Op::Store { addr: slot, val: mv(*v) },
                        b.insts.last().map(|x| x.loc).unwrap_or(call_loc),
                    ));
                }
                Term::Br(cont)
            }
            Term::Unset => Term::Br(cont),
        };
    }
}

/// ROUND 92 -- `blockmap` is the callee's block numbering translated into
/// the caller's. Only `Op::Phi` needs it: its entries name BLOCKS, and a
/// block of the callee has a different number inside the caller. Everything
/// else names values alone and passes `None`.
fn remap_op(op: &Op, mv: &dyn Fn(Val) -> Val, blockmap: Option<&HashMap<u32, u32>>) -> Op {
    match op {
        Op::Const(c) => Op::Const(*c),
        Op::Phi { incoming } => {
            let mut inc: Vec<(crate::fir::BlockId, Val)> = incoming
                .iter()
                .map(|(b, v)| (blockmap.map(|m| m[b]).unwrap_or(*b), mv(*v)))
                .collect();
            inc.sort_by_key(|(b, _)| *b);
            Op::Phi { incoming: inc }
        }
        Op::Copy { src } => Op::Copy { src: mv(*src) },
        Op::Alloca { size, align } => Op::Alloca { size: *size, align: *align },
        Op::Bin(o, a, b) => Op::Bin(*o, mv(*a), mv(*b)),
        // ROUND 72 — checked/wrap/sat arithmetic: same operand shape as
        // `Op::Bin`, the message text travels unchanged (it names no FIR
        // value, only file/line/operator text baked in at lowering time).
        Op::BinWrapSat { kind, op, a, b } => {
            Op::BinWrapSat { kind: *kind, op: *op, a: mv(*a), b: mv(*b) }
        }
        Op::CheckedBin { op, a, b, msg } => {
            Op::CheckedBin { op: *op, a: mv(*a), b: mv(*b), msg: msg.clone() }
        }
        Op::CheckedDiv { op, a, b, msg_zero, msg_range } => Op::CheckedDiv {
            op: *op,
            a: mv(*a),
            b: mv(*b),
            msg_zero: msg_zero.clone(),
            msg_range: msg_range.clone(),
        },
        Op::CheckedCast { src, from, msg } => {
            Op::CheckedCast { src: mv(*src), from: *from, msg: msg.clone() }
        }
        Op::CheckedIdx { idx, len, msg } => {
            Op::CheckedIdx { idx: mv(*idx), len: *len, msg: msg.clone() }
        }
        Op::Cmp { op, ty, a, b } => Op::Cmp { op: *op, ty: *ty, a: mv(*a), b: mv(*b) },
        Op::Un(o, a) => Op::Un(*o, mv(*a)),
        Op::Cast { src, from } => Op::Cast { src: mv(*src), from: *from },
        Op::Load { addr } => Op::Load { addr: mv(*addr) },
        Op::Store { addr, val } => Op::Store { addr: mv(*addr), val: mv(*val) },
        Op::PtrAdd { base, off } => Op::PtrAdd { base: mv(*base), off: mv(*off) },
        Op::Call { name, args } => {
            Op::Call { name: name.clone(), args: args.iter().map(|a| mv(*a)).collect() }
        }
        Op::CallIndirect { target, args } => Op::CallIndirect {
            target: mv(*target),
            args: args.iter().map(|a| mv(*a)).collect(),
        },
        Op::Simd { kind, args, imm } => Op::Simd {
            kind: *kind,
            args: args.iter().map(|a| mv(*a)).collect(),
            imm: *imm,
        },
        Op::VtabAddr { table } => Op::VtabAddr { table: table.clone() },
        Op::FnRef { name } => Op::FnRef { name: name.clone() },
        Op::GlobalAddr { name } => Op::GlobalAddr { name: name.clone() },
        Op::Syscall { args } => Op::Syscall { args: args.iter().map(|a| mv(*a)).collect() },
        Op::CopyMem { dst, src, size } => {
            Op::CopyMem { dst: mv(*dst), src: mv(*src), size: *size }
        }
        Op::Select { cond, a, b } => Op::Select { cond: mv(*cond), a: mv(*a), b: mv(*b) },
        Op::Barrier { val } => Op::Barrier { val: mv(*val) },
        Op::SecureZero { addr, size } => Op::SecureZero { addr: mv(*addr), size: mv(*size) },
        Op::AtomicAdd { addr, val } => Op::AtomicAdd { addr: mv(*addr), val: mv(*val) },
        Op::AtomicCas { addr, erw, new } => {
            Op::AtomicCas { addr: mv(*addr), erw: mv(*erw), new: mv(*new) }
        }
        Op::ThreadSpawn { arg, stack, ctid } => {
            Op::ThreadSpawn { arg: mv(*arg), stack: mv(*stack), ctid: mv(*ctid) }
        }
        Op::ThreadSelf => Op::ThreadSelf,
        Op::GcAddr { regs } => Op::GcAddr { regs: *regs },
        Op::Asm { template, out, in_regs, ins, out_regs, outs, clobber } => Op::Asm {
            template: template.clone(),
            out: out.clone(),
            in_regs: in_regs.clone(),
            ins: ins.iter().map(|a| mv(*a)).collect(),
            out_regs: out_regs.clone(),
            outs: outs.iter().map(|a| mv(*a)).collect(),
            clobber: clobber.clone(),
        },
        Op::MmioLoad { addr } => Op::MmioLoad { addr: mv(*addr) },
        Op::MmioStore { addr, val } => Op::MmioStore { addr: mv(*addr), val: mv(*val) },
    }
}

/// Embeds as long as the heuristic allows. Yields the number of embedded
/// calls.
pub fn inline_module(m: &mut Module) -> usize {
    let mut n = 0usize;
    let dbg = std::env::var("FIRNC_INLINE_DEBUG").is_ok();
    // Determined once: it hangs off the body of the callee only, which never
    // changes through embeddings (only the caller is mutated).
    let self_rec: Vec<bool> = m
        .funcs
        .iter()
        .map(|f| reaches_itself_self(m, &f.name))
        .collect();
    'outer: loop {
        for ci in 0..m.funcs.len() {
            if let Some((bi, ii, gi)) = find_site(m, ci, &self_rec) {
                if dbg {
                    eprintln!("inline: {} <- {} ({} insts, {} blocks)",
                        m.funcs[ci].name, m.funcs[gi].name,
                        m.funcs[gi].inst_count(), m.funcs[gi].blocks.len());
                }
                inline_one(m, ci, bi, ii, gi);
                n += 1;
                if n >= MAX_INLINES {
                    break 'outer;
                }
                continue 'outer;
            }
        }
        break;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{BinOp, CmpOp, Term};

    fn add_fn() -> Func {
        let mut g = Func::new("add", vec![FTy::I32, FTy::I32], FTy::I32);
        let s = g.push(0, FTy::I32, Op::Bin(BinOp::Add, 0, 1));
        g.set_term(0, Term::Ret(Some(s)));
        g
    }

    #[test]
    fn less_call_becomes_embedded_and_folded() {
        let mut m = Module::new();
        m.funcs.push(add_fn());
        let mut f = Func::new("main", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(2));
        let b = f.push(0, FTy::I32, Op::Const(40));
        let r = f.push(0, FTy::I32, Op::Call { name: "add".into(), args: vec![a, b] });
        f.set_term(0, Term::Ret(Some(r)));
        m.funcs.push(f);
        assert_eq!(inline_module(&mut m), 1);
        let main = m.funcs.iter().find(|f| f.name == "main").expect("main");
        assert!(!main
            .blocks
            .iter()
            .any(|b| b.insts.iter().any(|i| matches!(i.op, Op::Call { .. }))));
        crate::opt::optimize(&mut m);
        let main = m.funcs.iter().find(|f| f.name == "main").expect("main");
        // 2 + 40 becomes a single constant after embedding
        assert_eq!(main.inst_count(), 1);
        assert!(main.blocks[0].insts.iter().any(|i| matches!(i.op, Op::Const(42))));
    }

    #[test]
    fn recursion_becomes_not_embedded() {
        let mut m = Module::new();
        let mut f = Func::new("fact", vec![FTy::I32], FTy::I32);
        let one = f.push(0, FTy::I32, Op::Const(1));
        let c = f.push(0, FTy::Bool, Op::Cmp { op: CmpOp::Le, ty: FTy::I32, a: 0, b: one });
        let bt = f.add_block();
        let be = f.add_block();
        f.set_term(0, Term::BrCond { cond: c, then_bb: bt, else_bb: be });
        f.set_term(bt, Term::Ret(Some(one)));
        let sub = f.push(be, FTy::I32, Op::Bin(BinOp::Sub, 0, one));
        let rc = f.push(be, FTy::I32, Op::Call { name: "fact".into(), args: vec![sub] });
        let mu = f.push(be, FTy::I32, Op::Bin(BinOp::Mul, 0, rc));
        f.set_term(be, Term::Ret(Some(mu)));
        m.funcs.push(f);
        assert_eq!(inline_module(&mut m), 0);
    }

    #[test]
    fn several_rets_stay_correct() {
        // fn max(a,b) { if a<b { return b } return a }
        let mut g = Func::new("max", vec![FTy::I32, FTy::I32], FTy::I32);
        let c = g.push(0, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I32, a: 0, b: 1 });
        let bt = g.add_block();
        let be = g.add_block();
        g.set_term(0, Term::BrCond { cond: c, then_bb: bt, else_bb: be });
        g.set_term(bt, Term::Ret(Some(1)));
        g.set_term(be, Term::Ret(Some(0)));
        let mut m = Module::new();
        m.funcs.push(g);
        let mut f = Func::new("main", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(3));
        let b = f.push(0, FTy::I32, Op::Const(9));
        let r = f.push(0, FTy::I32, Op::Call { name: "max".into(), args: vec![a, b] });
        f.set_term(0, Term::Ret(Some(r)));
        m.funcs.push(f);
        assert_eq!(inline_module(&mut m), 1);
        crate::opt::optimize(&mut m);
        let main = m.funcs.iter().find(|f| f.name == "main").expect("main");
        assert!(main.blocks[0].insts.iter().any(|i| matches!(i.op, Op::Const(9))));
    }

    #[test]
    fn constant_time_funcs_stay_separate() {
        let mut m = Module::new();
        let mut g = add_fn();
        g.constant_time = true;
        m.funcs.push(g);
        let mut f = Func::new("main", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(2));
        let r = f.push(0, FTy::I32, Op::Call { name: "add".into(), args: vec![a, a] });
        f.set_term(0, Term::Ret(Some(r)));
        m.funcs.push(f);
        assert_eq!(inline_module(&mut m), 0);
    }
}
