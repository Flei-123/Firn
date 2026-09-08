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
/// ROUND INLINE: overridable for measuring. `FIRNC_MAX_INLINES` is read once
/// per pass; unset it keeps the compiled-in default.
const MAX_INLINES: usize = 2000;

/// ROUND INLINE -- a body of at most this many instructions is embedded even
/// after `MAX_INLINES` is exhausted.
///
/// WHY A SECOND, SIZE BASED BOUND. `MAX_INLINES` is a bound on the NUMBER of
/// embeddings, and a number cannot tell a one instruction accessor from a
/// forty instruction body. In `lib/js/run_main.fi` the count runs out long
/// before the collector accessors are reached, so `__gc_ld64` -- ONE
/// instruction -- stayed a real call and became 21 % of the running engine.
///
/// MEASURED (round INLINE, lib/js/run_main.fi, 120,000 round allocating
/// loop, interleaved A/B): raising the pure count bound is NOT monotonic.
///   cap  2,000 (the old value)  1687 ms   baseline
///   cap  5,000                  1670 ms   +6.1 % SLOWER (won 1 of 11 pairs)
///   cap 20,000                  1687 ms  +10.1 % SLOWER (won 0 of 11 pairs)
///   cap 22,138 (saturated)      1228 ms  -27.2 % FASTER (won 15 of 15)
/// A half spent budget is the worst of both worlds: the code has grown but
/// the hot accessors are still calls. That is the shape round SCHLEUSE saw
/// from the other side (`release-fast` slower than `dev-fast`, the inline
/// pass blowing the opcode chain out of the I-cache).
/// MEASURED, same bench, count bound left at 2,000 and only THIS bound
/// varied (interleaved A/B against the old cap 2,000 binary):
///   <=1  insts   956 ms  -36.7 %  (11 of 11 pairs)
///   <=4  insts   929 ms  -38.4 %  (11 of 11)
///   <=8  insts   912 ms  -39.4 %  (11 of 11)
///   <=12 insts   879 ms  -41.5 %  (11 of 11)
/// and 4 / 8 / 12 are a TIE with each other (6:5, 5:6 pairs -- noise), so the
/// middle of the plateau is taken. It also beats embedding EVERYTHING
/// (22,138 inlines, the saturated count bound) by -17.4 %, 11 of 11 pairs,
/// while the binary grows 1.83 MB -> 1.93 MB instead of 1.83 MB -> 3.16 MB.
///
/// That is the I-cache finding of round SCHLEUSE from the other side: what
/// makes an interpreter faster is embedding the ONE INSTRUCTION accessors it
/// runs millions of times, not embedding forty instruction bodies that push
/// the opcode chain out of the cache.
const MAX_ALWAYS_INSTS: usize = 8;

fn max_inlines() -> usize {
    match std::env::var("FIRNC_MAX_INLINES") {
        Ok(v) => v.trim().parse().unwrap_or(MAX_INLINES),
        Err(_) => MAX_INLINES,
    }
}

/// ROUND INLINE -- the SIZE bound that applies once the count budget is used
/// up. See `MAX_ALWAYS_INSTS`. Overridable for measuring.
fn always_insts() -> usize {
    match std::env::var("FIRNC_ALWAYS_INSTS") {
        Ok(v) => v.trim().parse().unwrap_or(MAX_ALWAYS_INSTS),
        Err(_) => MAX_ALWAYS_INSTS,
    }
}

// ROUND INLINE: `reaches` / `reaches_itself_self` are replaced by the memoised
// `Reach` below. The reasoning that a self reachable body must not be
// embedded is unchanged and documented there and at `inlinable`.
//
// The round 37 warning that produced the rule, kept verbatim:
//   with raised bounds (60/10) `__gc_scrub_deep` (29 insts, 9 blocks,
//   recursive) got embedded into `main` -- `tests/520_gc_weak.fi` failed
//   with exit 6, because phantom pointers in the unscrubbed stack fed the
//   collector.

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

/// ROUND INLINE — the call graph as an index instead of a linear search.
///
/// `find_site` used to resolve every callee name with
/// `m.funcs.iter().position(...)` — a linear scan over all 1638 functions of
/// the JS engine, at every call site, on every pass. The map is built once.
/// Function names never change during the pass (only bodies do), so it stays
/// valid throughout.
struct NameIndex {
    by_name: HashMap<String, usize>,
}

impl NameIndex {
    fn new(m: &Module) -> Self {
        let mut by_name = HashMap::with_capacity(m.funcs.len() * 2);
        for (i, f) in m.funcs.iter().enumerate() {
            // First definition wins — the same rule `position()` followed.
            by_name.entry(f.name.clone()).or_insert(i);
        }
        NameIndex { by_name }
    }
    fn get(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }
}

/// ROUND INLINE — `reaches` memoised over the ORIGINAL call graph.
///
/// The old code ran a fresh DFS over the whole module for every candidate
/// call site, and `reaches_itself_self` ran one DFS per call instruction of
/// every function on top. Both ask the same question about the same graph.
///
/// **Why a snapshot of the graph is the right answer and not a shortcut.**
/// Inlining only ever *removes* a call edge from the caller and copies the
/// callee's edges in its place. So the set of functions reachable from any
/// function is unchanged by embedding: whatever the embedded body could
/// reach, the caller could already reach through the call it replaced. The
/// reachability relation is therefore an invariant of the pass, and computing
/// it once is not an approximation — it is the same answer the repeated DFS
/// gave, minus the repetition.
struct Reach {
    /// adjacency of the original call graph, as indices
    adj: Vec<Vec<usize>>,
    /// memo: for caller index -> set of indices reachable from it
    memo: HashMap<usize, HashSet<usize>>,
}

impl Reach {
    fn new(m: &Module, idx: &NameIndex) -> Self {
        let mut adj: Vec<Vec<usize>> = Vec::with_capacity(m.funcs.len());
        for f in &m.funcs {
            let mut out: Vec<usize> = Vec::new();
            for b in &f.blocks {
                for i in &b.insts {
                    if let Op::Call { name, .. } = &i.op {
                        if let Some(g) = idx.get(name) {
                            out.push(g);
                        }
                    }
                }
            }
            out.sort_unstable();
            out.dedup();
            adj.push(out);
        }
        Reach { adj, memo: HashMap::new() }
    }

    /// Everything reachable from `from` through calls, `from` itself only if
    /// it lies on a cycle. Iterative — the call graph of the JS engine is
    /// deeper than the Rust stack likes.
    fn set_of(&mut self, from: usize) -> &HashSet<usize> {
        if !self.memo.contains_key(&from) {
            let mut seen: HashSet<usize> = HashSet::new();
            let mut stack: Vec<usize> = self.adj[from].clone();
            while let Some(cur) = stack.pop() {
                if !seen.insert(cur) {
                    continue;
                }
                for &n in &self.adj[cur] {
                    if !seen.contains(&n) {
                        stack.push(n);
                    }
                }
            }
            self.memo.insert(from, seen);
        }
        self.memo.get(&from).expect("just inserted")
    }

    /// Can `from` reach `to`? (the old `reaches(m, from, to)`, where
    /// `from == to` counted as true straight away)
    fn reaches(&mut self, from: usize, to: usize) -> bool {
        from == to || self.set_of(from).contains(&to)
    }

    /// Can the body of `g` reach `g` again — direct or indirect recursion?
    /// The old `reaches_itself_self`: true when SOME callee of `g` reaches
    /// `g`. That is exactly "`g` lies on a cycle", i.e. `g` is reachable
    /// from `g` over at least one edge.
    fn self_rec(&mut self, g: usize) -> bool {
        self.set_of(g).contains(&g)
    }
}

/// Looks for a worthwhile call site in the caller `ci`, starting at
/// `(from_bi, from_ii)`.
///
/// ROUND INLINE — the scan RESUMES instead of starting over. `inline_one`
/// only ever appends blocks and rewrites the calling block; everything before
/// the call site that was just handled has already been judged and cannot
/// have become inlinable in the meantime. Restarting at block 0 re-walked the
/// whole (and growing) body after every single embedding, which is one of the
/// four quadratic factors this round removed.
fn find_site(
    m: &Module,
    ci: usize,
    from_bi: usize,
    from_ii: usize,
    idx: &NameIndex,
    reach: &mut Reach,
    // ROUND INLINE: `Some(k)` = the count budget is used up, only bodies of
    // at most `k` instructions are still embedded. `None` = no extra bound.
    small_only: Option<usize>,
) -> Option<(usize, usize, usize)> {
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
    for bi in from_bi..caller.blocks.len() {
        let b = &caller.blocks[bi];
        let start = if bi == from_bi { from_ii } else { 0 };
        for ii in start..b.insts.len() {
            let inst = &b.insts[ii];
            if let Op::Call { name, args } = &inst.op {
                let gi = match idx.get(name) {
                    Some(g) => g,
                    None => continue,
                };
                let callee = &m.funcs[gi];
                if gi == ci || !inlinable(callee) {
                    continue;
                }
                if let Some(k) = small_only {
                    if callee.inst_count() > k {
                        continue;
                    }
                }
                if callee.params.len() != args.len() {
                    continue;
                }
                if inst.dst.is_some() && callee.ret == FTy::Void {
                    continue;
                }
                // Recursion (indirect one too) is not embedded.
                if reach.reaches(gi, ci) {
                    continue;
                }
                // Self reachable bodies neither (see above).
                if reach.self_rec(gi) {
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
///
/// ROUND INLINE — a worklist instead of `'outer: loop { for ci in 0.. }`.
///
/// The old driver restarted the scan at function 0 after EVERY embedding and
/// `continue 'outer`'d out of the loop, so reaching function 1600 of the JS
/// engine meant walking functions 0..1599 again — 2000 times over. Together
/// with the linear name lookup, the per-site call graph DFS and the
/// restart-at-block-0 inside `find_site` that is what made the pass O(n²) and
/// 5.8 s of the 12 s compile.
///
/// The replacement walks each caller ONCE and stays with it until it has no
/// site left, remembering where the last site was found. `inline_one` mutates
/// only the caller, so no other function's verdict can change while we work.
pub fn inline_module(m: &mut Module) -> usize {
    let mut n = 0usize;
    let dbg = std::env::var("FIRNC_INLINE_DEBUG").is_ok();
    let cap = max_inlines();
    let always = always_insts();
    let idx = NameIndex::new(m);
    let mut reach = Reach::new(m, &idx);

    for ci in 0..m.funcs.len() {
        // Where the previous site sat: the scan resumes here instead of
        // walking the (now longer) body from the start again.
        let mut bi = 0usize;
        let mut ii = 0usize;
        loop {
            // Below the count budget everything the heuristic allows; above
            // it only bodies at most `always` instructions long. With
            // `always == 0` that is the old behaviour exactly: the pass stops.
            let small_only = if n >= cap { Some(always) } else { None };
            if let Some(0) = small_only {
                break;
            }
            match find_site(m, ci, bi, ii, &idx, &mut reach, small_only) {
                Some((fbi, fii, gi)) => {
                    if dbg {
                        eprintln!(
                            "inline: {} <- {} ({} insts, {} blocks)",
                            m.funcs[ci].name,
                            m.funcs[gi].name,
                            m.funcs[gi].inst_count(),
                            m.funcs[gi].blocks.len()
                        );
                    }
                    inline_one(m, ci, fbi, fii, gi);
                    n += 1;
                    // The call at (fbi, fii) is gone: the block was split
                    // there and everything after it moved into the new
                    // continuation block at the end. Carry on at the same
                    // block from the same index — what stands there now is
                    // the first instruction of the embedded body.
                    bi = fbi;
                    ii = fii;
                }
                None => break,
            }
        }
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
