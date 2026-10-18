// SPDX-License-Identifier: MPL-2.0
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
/// Upper bound on the instructions of a TRIVIAL LEAF (see `trivial_leaf`).
const MAX_TRIVIAL_INSTS: usize = 6;
/// Safety bound on trivial embeddings per module (code size guard).
const MAX_TRIVIAL_INLINES: usize = 200_000;
/// Budget of the size-ordered phase (see `inline_by_size`). It replaces the
/// first-come-first-served budget of MAX_INLINES for the bodies it covers.
const MAX_PRIO_INLINES: usize = 200_000;
/// Growth guard of the size-ordered phase, in percent of the module size
/// before it started.
const MAX_GROWTH_PCT: usize = 40;
/// Largest callee embedded into a (self-)recursive caller in the middle
/// version kept for the frame budget (see `REC_SMALL`).
const REC_CALLER_TIER: usize = 10;

/// ROUND TK2 (Certus, r136) -- THE FRAME BUDGET OF A RECURSIVE CALLER.
///
/// A recursive caller pays every octet of frame growth once per recursion
/// level, which is why the size-ordered phase gave it nothing (round INLGZ).
/// Since the value slots are shared by lifetime (regalloc.rs,
/// `compact_frame`) an embedded body costs a recursive frame little -- but
/// "little" is a measurement, not a promise. So a recursive caller now takes
/// the same sites as every other caller (`FIRNC_INLINE_REC=2`, the default on
/// x86-64), its body as it was BEFORE the first such embedding is kept here,
/// optimised and phi-eliminated like the module (opt.rs, main.rs), and the
/// code generator compiles both and keeps the embedded one only if its frame
/// stays within `rec_frame_within_budget` (codegen_x86::emit). Otherwise the
/// function goes out as it was. `FIRNC_INLINE_REC=0` is round INLGZ (nothing
/// into recursive callers), `=1` the old one-block experiment.
static REC_ORIGINALS: std::sync::Mutex<Option<HashMap<String, Func>>> =
    std::sync::Mutex::new(None);
/// The same callers as they stood after the tiers up to `REC_CALLER_TIER` --
/// only the small bodies embedded. The code generator falls back to this
/// middle version when the full one breaks the budget (the frame of the
/// Certus `parse_primary` grew 112 -> 960 octets with everything embedded,
/// 112 -> 128 with the small bodies only).
static REC_SMALL: std::sync::Mutex<Option<HashMap<String, Func>>> =
    std::sync::Mutex::new(None);

fn table_take(t: &std::sync::Mutex<Option<HashMap<String, Func>>>) -> HashMap<String, Func> {
    t.lock().map(|mut g| g.take().unwrap_or_default()).unwrap_or_default()
}

fn table_put(t: &std::sync::Mutex<Option<HashMap<String, Func>>>, m: HashMap<String, Func>) {
    if let Ok(mut g) = t.lock() {
        match g.as_mut() {
            Some(x) => x.extend(m),
            None => *g = Some(m),
        }
    }
}

/// The kept bodies (the table is empty afterwards).
pub fn rec_originals_take() -> HashMap<String, Func> {
    table_take(&REC_ORIGINALS)
}

/// Put (back) kept bodies.
pub fn rec_originals_put(m: HashMap<String, Func>) {
    table_put(&REC_ORIGINALS, m)
}

/// The middle versions (small bodies only), taken like the kept ones.
pub fn rec_small_take() -> HashMap<String, Func> {
    table_take(&REC_SMALL)
}

/// Put (back) middle versions.
pub fn rec_small_put(m: HashMap<String, Func>) {
    table_put(&REC_SMALL, m)
}

/// The frame of the embedded body may be this much larger than the frame of
/// the kept one: 64 octets or an eighth, whichever is more.
pub fn rec_frame_within_budget(embedded: u64, kept: u64) -> bool {
    let slack = std::cmp::max(64, kept / 8);
    embedded <= kept + slack
}

fn env_usize(name: &str, dflt: usize) -> usize {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(dflt)
}

/// `FIRN_MAX_INLINES=<n>` overrides the module bound (measuring aid).
fn max_inlines() -> usize {
    std::env::var("FIRN_MAX_INLINES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(MAX_INLINES)
}

/// Name -> position of every function. The list of functions does not change
/// while bodies are embedded (only the caller's body does), so one index serves
/// a whole phase.
fn name_index(m: &Module) -> HashMap<String, usize> {
    m.funcs.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect()
}

/// Can a function reach itself again through at least one call (direct or
/// indirect recursion)? One answer per function, from the call graph taken
/// once (`CallGraph::self_recursive`).
///
/// Such bodies are NOT embedded. Inlining unrolls one recursion level and
/// moves its frames into the caller — program code whose effect rests on the
/// stack depth (the stack scrubbing of the conservative GC, `lib/gc`:
/// `__gc_scrub_deep`) loses its effect that way. MEASURED in round 37: with
/// raised bounds (60/10) `__gc_scrub_deep` (29 insts, 9 blocks, recursive)
/// got embedded into `main` — `tests/520_gc_weak.fi` failed with exit 6,
/// because phantom pointers in the unscrubbed stack fed the collector.
///
/// (Until the Certus compiler was merged this was a walk by NAME over the whole
/// module for every function, with a linear search of the function list at
/// every step: 16 seconds for the 400 000 instructions of the Certus JavaScript
/// engine. With the index and the memoised graph it is a fraction of a second.)
fn self_recursive_all(cg: &mut CallGraph, n: usize) -> Vec<bool> {
    (0..n).map(|i| cg.self_recursive(i)).collect()
}

/// The HARD blocks: these are about correctness, not size. `#[inline]`
/// lifts NONE of them.
///
///  * `#[constant_time]` / `secret` -- the check in the code generator works
///    per function (SPEC 9.2). Inlined into a caller without the mark it
///    would be gone, and a timing side channel promise silently broken.
///  * `#[interrupt]` -- its own calling sequence, ends with `iretq` instead
///    of `ret` (round 52). The body does not belong into an ordinary frame.
///  * an unfinished block (`Term::Unset`) or a block list whose numbers are
///    not in order -- then `blockmap` is wrong.
fn allowed_at_all(callee: &Func) -> bool {
    !callee.constant_time
        && callee.secret.is_empty()
        && !callee.interrupt
        && !callee.blocks.iter().any(|b| matches!(b.term, Term::Unset))
        && callee.blocks.iter().enumerate().all(|(i, b)| b.id as usize == i)
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
    if !allowed_at_all(callee) {
        return false;
    }
    // Round EINBETTEN -- the explicit will beats the size rule.
    match callee.inline_hint {
        // `#[no_inline]`: never, no exceptions.
        Some(false) => return false,
        // `#[inline]`: the size limits do not apply. That is what the JIT
        // needs for its helpers -- their bodies are above 40 instructions,
        // and without this path the call stays.
        Some(true) => return true,
        None => {}
    }
    callee.inst_count() <= MAX_CALLEE_INSTS && callee.blocks.len() <= MAX_CALLEE_BLOCKS
}

/// Looks for a worthwhile call site in the caller `ci`.
/// `self_rec`: precomputed per function (does not change through
/// embeddings — only the caller is mutated).
/// Blocks of `f` that lie on a loop (inside the natural loop of some back
/// edge).
fn loop_blocks(f: &Func) -> Vec<bool> {
    let n = f.blocks.len();
    let mut on = vec![false; n];
    if n < 2 {
        return on;
    }
    let dt = crate::mem2reg::idoms(f);
    let preds = dt.preds.clone();
    let dominates = |a: usize, b: usize| -> bool {
        let mut x = b;
        for _ in 0..=n {
            if x == a {
                return true;
            }
            let up = dt.idom[x] as usize;
            if up == x {
                return false;
            }
            x = up;
        }
        false
    };
    for (b, blk) in f.blocks.iter().enumerate() {
        if dt.rpo_num[b] == usize::MAX {
            continue;
        }
        for s in blk.term.successors() {
            let h = s as usize;
            if h < n && dominates(h, b) {
                for x in crate::licm::natural_loop(h, b, &preds) {
                    on[x] = true;
                }
            }
        }
    }
    on
}

/// `hot_only` (TEMPO 15, the second round): only call sites INSIDE a loop,
/// and only callees that call nothing themselves -- the leaves whose body is
/// what the loop runs, not a whole subtree.
fn find_site_x(
    m: &Module,
    ci: usize,
    self_rec: &[bool],
    requested_only: bool,
    hot_only: bool,
    leaves: Option<&HashSet<String>>,
    idx: &HashMap<String, usize>,
    cg: &mut CallGraph,
) -> Option<(usize, usize, usize)> {
    let caller = &m.funcs[ci];
    if caller.constant_time || !caller.secret.is_empty() {
        return None;
    }
    let hot: Vec<bool> = if hot_only {
        // cheap exit first: no call of an eligible leaf at all (the set is
        // fixed at the start of the round), or no backward edge
        let has_call = caller.blocks.iter().any(|b| {
            b.insts.iter().any(|i| match &i.op {
                Op::Call { name, .. } => leaves.map(|l| l.contains(name.as_str())).unwrap_or(true),
                _ => false,
            })
        });
        if !has_call {
            return None;
        }
        let on = loop_blocks(caller);
        if !on.iter().any(|x| *x) {
            return None;
        }
        on
    } else {
        Vec::new()
    };
    // The caller limit protects compile time and code size. But it must not
    // lock out an EXPLICITLY requested inlining -- otherwise `#[inline]`
    // would depend on how big the caller happens to be. So it is checked
    // per call site below, not here for the whole function.
    let caller_full = caller.inst_count() > MAX_CALLER_INSTS;
    if caller.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return None;
    }
    for (bi, b) in caller.blocks.iter().enumerate() {
        if hot_only && !hot[bi] {
            continue;
        }
        for (ii, inst) in b.insts.iter().enumerate() {
            if let Op::Call { name, args } = &inst.op {
                let gi = match idx.get(name.as_str()) {
                    Some(&g) => g,
                    None => continue,
                };
                let callee = &m.funcs[gi];
                if gi == ci || !inlinable(callee) {
                    continue;
                }
                if hot_only
                    && callee.blocks.iter().any(|b| {
                        b.insts.iter().any(|i| {
                            matches!(i.op, Op::Call { .. } | Op::CallIndirect { .. } | Op::Syscall { .. })
                        })
                    })
                {
                    continue;
                }
                // A full caller only takes what is explicitly requested.
                if caller_full && callee.inline_hint != Some(true) {
                    continue;
                }
                // Default level: ONLY what carries `#[inline]`.
                if requested_only && callee.inline_hint != Some(true) {
                    continue;
                }
                if callee.params.len() != args.len() {
                    continue;
                }
                if inst.dst.is_some() && callee.ret == FTy::Void {
                    continue;
                }
                // Recursion (indirect one too) is not embedded.
                if cg.reaches(gi, ci) {
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
fn inline_one(m: &mut Module, ci: usize, bi: usize, ii: usize, gi: usize) {
    inline_one_pooled(m, ci, bi, ii, gi, None)
}

/// Stack slots the leaf inliner already made in one caller, by (size, align).
type SlotPool = HashMap<(u64, u64), Vec<Val>>;

/// `pool`: only for LEAF callees (no call inside, so two embedded bodies are
/// never alive at the same time): the k-th slot of a size/alignment class is
/// shared by all embedded bodies of the caller instead of growing the frame
/// by one slot per site. Safe because a body writes its slot before it reads
/// it (compiler-made parameter spills) and nothing in it can call out.
fn inline_one_pooled(
    m: &mut Module,
    ci: usize,
    bi: usize,
    mut ii: usize,
    gi: usize,
    mut pool: Option<&mut SlotPool>,
) {
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
    let mut used: HashMap<(u64, u64), usize> = HashMap::new();
    for b in &callee.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Alloca { size, align }) = (i.dst, &i.op) {
                if let Some(pool) = pool.as_deref_mut() {
                    let key = (*size, *align);
                    let k = used.entry(key).or_insert(0);
                    let slots = pool.entry(key).or_default();
                    if *k < slots.len() {
                        valmap.insert(d, slots[*k]);
                        *k += 1;
                        continue;
                    }
                    *k += 1;
                    let nv = m.funcs[ci].alloca(*size, *align);
                    slots.push(nv);
                    valmap.insert(d, nv);
                    shift += 1;
                    continue;
                }
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
    let mut new_ids: Vec<u32> = Vec::with_capacity(callee.blocks.len() + 1);
    for b in &callee.blocks {
        let nb = f.add_block();
        blockmap.insert(b.id, nb);
        new_ids.push(nb);
    }
    let cont = f.add_block();
    new_ids.push(cont);

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

    // Round EINBETTEN: move the body right behind the calling block.
    // Reason and measurement are at `reorder_blocks`.
    reorder_blocks(f, bi, &new_ids);
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
        Op::CheckedIdxV { idx, len, msg } => {
            Op::CheckedIdxV { idx: mv(*idx), len: mv(*len), msg: msg.clone() }
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

/// Round EINBETTEN -- only the EXPLICITLY requested inlinings.
///
/// This is the pass for the default level (`dev-fast`). The size rule stays
/// off there: it makes the call stack unreadable, and `dev-fast` is the
/// level people debug at. `#[inline]` on the other hand is a promise to the
/// programmer -- it must not depend on the build switch, otherwise one
/// silently measures something other than what one built (as round JIT
/// did).
pub fn inline_module_requested_only(m: &mut Module) -> usize {
    inline_module_inner(m, true)
}

/// Round EINBETTEN -- MOVE THE BLOCKS OF THE INLINED BODY FORWARD.
///
/// `Func::add_block` only appends. An inlined body therefore lands at the
/// END of the function, even if the call site sits in the first loop. That
/// is not cosmetic:
///
/// `regalloc.rs` builds live intervals as `[smallest position, largest
/// position]` over the LINEAR block order (`live.block_start` /
/// `live.block_end`). If the body sits behind all other blocks, the
/// interval of every value that lives into the body and back out spans the
/// WHOLE function -- including unrelated loops. There it collides with
/// their values and gets spilled.
///
/// MEASURED (dev-fast, round EINBETTEN):
///   * one loop, the body lands right next to it:
///     2.35 ns -> 1.08 ns per iteration = 2.2x BETTER.
///   * the same computation, but TWO loops in `main`, the body lands behind
///     the second: 2.37 ns -> 3.44 ns = WORSE.
/// Same body, same inlining -- only the block distance decides.
///
/// So the blocks are reordered here: the body and the continuation block
/// move right behind the calling block. Block numbers are an INDEX
/// (everywhere in the compiler `b.id as usize == i`, see `mem2reg.rs`,
/// `opt.rs`, `regalloc.rs`), so number, jump targets and the block operands
/// of every `phi` have to be rewritten together.
pub(crate) fn reorder_blocks(f: &mut Func, after: usize, new_ids: &[u32]) {
    let n = f.blocks.len();
    if new_ids.is_empty() || after >= n {
        return;
    }
    // The desired order: everything up to and including `after`, then the
    // new blocks, then the rest.
    let new_set: std::collections::HashSet<u32> = new_ids.iter().copied().collect();
    let mut order: Vec<u32> = Vec::with_capacity(n);
    for i in 0..=after {
        let id = i as u32;
        if !new_set.contains(&id) {
            order.push(id);
        }
    }
    for &b in new_ids {
        order.push(b);
    }
    for i in (after + 1)..n {
        let id = i as u32;
        if !new_set.contains(&id) {
            order.push(id);
        }
    }
    if order.len() != n {
        return; // something is off -- better not reorder at all
    }
    apply_block_order(f, &order);
}

/// Brings the blocks into the order `order` (old numbers, every block exactly once)
/// and rewrites jump targets and phi edges.
pub(crate) fn apply_block_order(f: &mut Func, order: &[u32]) {
    let n = f.blocks.len();
    if order.len() != n {
        return;
    }
    // old -> new
    let mut map = vec![0u32; n];
    for (new_i, &old) in order.iter().enumerate() {
        map[old as usize] = new_i as u32;
    }
    // Bring the blocks into the new order ...
    let old_blocks = std::mem::take(&mut f.blocks);
    let mut by_id: Vec<Option<crate::fir::Block>> = old_blocks.into_iter().map(Some).collect();
    let mut new_blocks: Vec<crate::fir::Block> = Vec::with_capacity(n);
    for &old in order {
        let mut b = by_id[old as usize].take().expect("block handed out twice");
        b.id = map[old as usize];
        new_blocks.push(b);
    }
    f.blocks = new_blocks;
    // ... and carry every block operand along: jump targets and phi edges.
    let mv = |b: &u32| -> u32 { map[*b as usize] };
    for b in f.blocks.iter_mut() {
        b.term = match &b.term {
            Term::Br(t) => Term::Br(mv(t)),
            Term::BrCond { cond, then_bb, else_bb } => {
                Term::BrCond { cond: *cond, then_bb: mv(then_bb), else_bb: mv(else_bb) }
            }
            Term::Switch { val, ty, cases, default } => Term::Switch {
                val: *val,
                ty: *ty,
                cases: cases.iter().map(|(k, t)| (*k, mv(t))).collect(),
                default: mv(default),
            },
            Term::Ret(v) => Term::Ret(*v),
            Term::Unset => Term::Unset,
        };
        for i in b.insts.iter_mut() {
            if let Op::Phi { incoming } = &mut i.op {
                for e in incoming.iter_mut() {
                    e.0 = map[e.0 as usize];
                }
                incoming.sort_by_key(|(p, _)| *p);
            }
        }
    }
}

/// A trivial leaf: one block, no calls, at most MAX_TRIVIAL_INSTS
/// instructions, no allocas. Accessors such as `__gc_ld64` (one load) and
/// `__gc_st64` (one store). A leaf cannot reach any caller, so no recursion
/// check is needed, and the body is never bigger than the call sequence it
/// replaces -- so these sites do not count against MAX_INLINES.
///
/// WHY (Certus, round FLOAT): a program that is ONE big module -- the browser
/// -- has far more than MAX_INLINES = 2000 candidate sites, and the budget
/// goes first come first served in module order. The 2000 were gone long
/// before the collector runtime (`lib/gc/gc.fi`: 316 calls of `__gc_ld64`, a
/// function whose body is one load) got a single one. Measured with
/// FIRNC_INLINE_DEBUG: 2000 embeddings, 2 of them `__gc_ld64`; callgrind on a
/// JS bench: 166 calls of `__gc_ld64` and 71 of `__gc_st64` PER ITERATION.
/// With this phase 6842 trivial sites were embedded in the interpreter
/// driver alone, and the allocation bench went from 11 547 to 6 010
/// instructions per iteration.
fn trivial_leaf(f: &Func) -> bool {
    inlinable(f)
        && f.blocks.len() == 1
        && f.inst_count() <= MAX_TRIVIAL_INSTS
        && !f.blocks.iter().any(|b| {
            b.insts
                .iter()
                .any(|i| matches!(i.op, Op::Call { .. } | Op::CallIndirect { .. } | Op::Alloca { .. }))
        })
}

/// **r591** -- the leaf inliner of `dev-fast`.
///
/// `dev-fast` has no size-rule inliner (it makes the call stack unreadable),
/// so every tiny helper (`str.chars`, `vec_elem_ptr`, `str.length`, the
/// `rt.ld*`/`st*` accessors) cost a full call: ~17 instructions for `chars`
/// of which two are the body. Measured on OpenPlan `derive`: 18 M calls of
/// `str.chars` alone = 0.4 G instructions.
///
/// What stays readable: a DEV LEAF calls nothing, is loop free, has at most
/// parameter-spill allocas (shared by all embedded bodies of one caller) and
/// contains NOTHING that can trap or print a position -- no
/// `Checked*` op (a panic stands in the frame of the function whose code
/// panics, with its backtrace unchanged), no syscall, atomic, barrier, asm,
/// MMIO or thread op. It is therefore never on a call stack unless the
/// debugger stops INSIDE it, and no panic message, exit code or backtrace of
/// a program changes. Every copied instruction keeps the position of the
/// callee (`inline_one`).
const MAX_DEV_LEAF_INSTS: usize = 18;
const MAX_DEV_LEAF_BLOCKS: usize = 4;

fn dev_leaf(f: &Func) -> bool {
    if !allowed_at_all(f)
        || f.inline_hint == Some(false)
        || f.blocks.len() > MAX_DEV_LEAF_BLOCKS
        || f.inst_count() > MAX_DEV_LEAF_INSTS
        || !f.no_coalesce.is_empty()
    {
        return false;
    }
    for b in &f.blocks {
        // loop free: every edge goes forward (blocks are numbered by position)
        if b.term.successors().iter().any(|s| *s <= b.id) {
            return false;
        }
        if b.insts.iter().any(|i| !matches!(
            i.op,
            Op::Const(_)
                | Op::Bin(..)
                | Op::BinWrapSat { .. }
                | Op::Cmp { .. }
                | Op::Un(..)
                | Op::Cast { .. }
                | Op::Load { .. }
                | Op::Store { .. }
                | Op::PtrAdd { .. }
                | Op::Select { .. }
                | Op::Alloca { .. }
                | Op::Phi { .. }
                | Op::CheckedBin { .. }
                | Op::CheckedDiv { .. }
                | Op::CheckedCast { .. }
                | Op::CheckedIdx { .. }
        )) {
            return false;
        }
    }
    true
}

/// r591 -- one round of the dev-fast leaf inliner. Returns the number of
/// embeddings and the callers that received one.
pub fn inline_module_dev_leaves(m: &mut Module) -> (usize, Vec<usize>) {
    let dbg = std::env::var("FIRNC_INLINE_DEBUG").is_ok();
    let leaf: Vec<bool> = m.funcs.iter().map(dev_leaf).collect();
    let mut changed = Vec::new();
    let loops_only = std::env::var("FIRN_LEAF_ALL").is_err();
    let n = inline_flagged_leaves_x(m, &leaf, dbg, loops_only, &mut changed);
    changed.sort();
    changed.dedup();
    (n, changed)
}

/// Phase 1 of the first round: embed every call of a trivial leaf, outside
/// the budget. Returns the number of embeddings; the callers that received
/// one are appended to `changed`.
fn inline_trivial_leaves(m: &mut Module, dbg: bool, changed: &mut Vec<usize>) -> usize {
    let triv: Vec<bool> = m.funcs.iter().map(trivial_leaf).collect();
    inline_flagged_leaves(m, &triv, dbg, changed)
}

/// Embeds every call of a function flagged in `triv` (outside the budget; the
/// callee must be a leaf, so it cannot reach its caller).
fn inline_flagged_leaves(m: &mut Module, triv: &[bool], dbg: bool, changed: &mut Vec<usize>) -> usize {
    inline_flagged_leaves_x(m, triv, dbg, false, changed)
}

/// `loops_only`: only call sites inside a loop of the caller (the sites that
/// run often); a caller without such a site is not touched and not
/// re-optimized -- that is what keeps the compile time of `dev-fast` down.
fn inline_flagged_leaves_x(
    m: &mut Module,
    triv: &[bool],
    dbg: bool,
    loops_only: bool,
    changed: &mut Vec<usize>,
) -> usize {
    if !triv.iter().any(|t| *t) {
        return 0;
    }
    let idx: HashMap<String, usize> =
        m.funcs.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
    let mut n = 0usize;
    for ci in 0..m.funcs.len() {
        {
            let c = &m.funcs[ci];
            if c.constant_time || !c.secret.is_empty() {
                continue;
            }
        }
        let mut pool: SlotPool = HashMap::new();
        // Resume the scan where the last embedding happened: the blocks
        // before it hold no trivial call any more.
        let mut start_b = 0usize;
        loop {
            if n >= MAX_TRIVIAL_INLINES {
                return n;
            }
            let caller = &m.funcs[ci];
            if caller.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
                break;
            }
            let hot: Vec<bool> = if loops_only { loop_blocks(caller) } else { Vec::new() };
            if loops_only && !hot.iter().any(|h| *h) {
                break;
            }
            let mut site: Option<(usize, usize, usize)> = None;
            'scan: for bi in start_b..caller.blocks.len() {
                if loops_only && !hot[bi] {
                    continue;
                }
                for (ii, inst) in caller.blocks[bi].insts.iter().enumerate() {
                    if let Op::Call { name, args } = &inst.op {
                        if let Some(&gi) = idx.get(name) {
                            if gi != ci
                                && triv[gi]
                                && m.funcs[gi].params.len() == args.len()
                                && !(inst.dst.is_some() && m.funcs[gi].ret == FTy::Void)
                            {
                                site = Some((bi, ii, gi));
                                break 'scan;
                            }
                        }
                    }
                }
            }
            match site {
                Some((bi, ii, gi)) => {
                    if dbg {
                        eprintln!("inline-trivial: {} <- {}", m.funcs[ci].name, m.funcs[gi].name);
                    }
                    inline_one_pooled(m, ci, bi, ii, gi, Some(&mut pool));
                    changed.push(ci);
                    n += 1;
                    start_b = bi;
                }
                None => break,
            }
        }
    }
    n
}

/// Call graph on function indices, taken once before the size-ordered phase.
/// Embedding only ever REMOVES reachability (the edge f->g becomes
/// f->callees(g), which f reached through g anyway), so a graph taken before
/// is a superset of the current one: every "reaches" answer from it is
/// conservative, and a cached `false` stays true.
struct CallGraph {
    succ: Vec<Vec<usize>>,
    memo: HashMap<(usize, usize), bool>,
    mark: Vec<u32>,
    epoch: u32,
}

impl CallGraph {
    fn new(m: &Module, idx: &HashMap<String, usize>) -> CallGraph {
        let succ = m
            .funcs
            .iter()
            .map(|f| {
                let mut v: Vec<usize> = Vec::new();
                for b in &f.blocks {
                    for i in &b.insts {
                        if let Op::Call { name, .. } = &i.op {
                            if let Some(&g) = idx.get(name) {
                                v.push(g);
                            }
                        }
                    }
                }
                v.sort_unstable();
                v.dedup();
                v
            })
            .collect();
        CallGraph { succ, memo: HashMap::new(), mark: vec![0; m.funcs.len()], epoch: 0 }
    }

    /// Can `from` reach `to` through calls (`from == to` counts)?
    fn reaches(&mut self, from: usize, to: usize) -> bool {
        if from == to {
            return true;
        }
        if let Some(&r) = self.memo.get(&(from, to)) {
            return r;
        }
        self.epoch += 1;
        let ep = self.epoch;
        let mut stack = vec![from];
        let mut found = false;
        while let Some(cur) = stack.pop() {
            if cur == to {
                found = true;
                break;
            }
            if self.mark[cur] == ep {
                continue;
            }
            self.mark[cur] = ep;
            stack.extend(self.succ[cur].iter().copied());
        }
        self.memo.insert((from, to), found);
        found
    }

    /// Can `g` reach itself through at least one call?
    fn self_recursive(&mut self, g: usize) -> bool {
        let succ = self.succ[g].clone();
        succ.into_iter().any(|c| self.reaches(c, g))
    }
}

/// Phase 2 of the first round (Certus, round INLGZ) -- THE BUDGET GOES TO
/// THE SMALLEST BODIES FIRST, NOT TO THE FIRST FUNCTIONS OF THE MODULE.
///
/// The first-come loop embeds the first eligible site of the first function
/// that has one and stops after MAX_INLINES (2000). A program that is one big
/// module -- the Certus browser -- has the budget gone on start-up and UI
/// code before the JavaScript engine, the lexer and the HTML buffers get a
/// single embedding: `html.mem.cp_push` (11 instructions) stayed a call 64
/// million times per github.com load.
///
/// Now: in rounds of growing callee size (one per size, from just above the
/// trivial leaves), every caller embeds every eligible site whose body is at
/// most the tier size. Small bodies are the ones where the call costs as much
/// as the body, and they are exactly the hot helpers (buffer push/at, tag
/// tests, accessors). Budget MAX_PRIO_INLINES sites, and a growth guard in
/// percent of the module (when it stops the phase it stops at a size
/// boundary: everything smaller is in). Certus (2.8 million FIR instructions):
/// every one of the ~37 500 eligible sites fits (+16 % instructions) and the
/// build got FASTER (103 s -> ~55 s), because the first-come loop restarts at
/// function 0 after every embedding and searches callees and the call graph
/// linearly. Measured there: one github.com load 41.06 -> 35.22 billion
/// instructions (with the table-driven inflate of the same round).
///
/// Eligibility is the same as in the first-come loop (`inlinable`, caller
/// bound, recursion, self recursion, constant time).
///
/// A (self-)recursive CALLER is special: every octet of frame growth is paid
/// once per recursion level. Measured: with every site up to 40 instructions
/// embedded and one slot per value, the regexp backtracker's frame grew
/// 7.8 -> 10.5 KiB and `/(a|b)*c/` over 60 000 characters ran off the stack.
/// With the value slots shared by lifetime (regalloc.rs `compact_frame`) the
/// growth is small, so on x86-64 (`FIRNC_INLINE_REC=2`, see `REC_ORIGINALS`)
/// a recursive caller takes the same sites as everybody else, and the code
/// generator decides per function whether the embedded body fits the frame
/// budget. Elsewhere (`FIRNC_INLINE_REC=0`) it takes nothing. LIMIT: the call
/// graph sees direct calls only.
///
/// `FIRNC_INLINE_OLD=1` skips this phase (A/B), `FIRNC_INLINE_BUDGET` and
/// `FIRNC_INLINE_GROWTH` override the two bounds, `FIRNC_INLINE_STATS=1`
/// prints the numbers per tier.
fn inline_by_size(m: &mut Module, dbg: bool, changed: &mut Vec<usize>) -> usize {
    let budget = env_usize("FIRNC_INLINE_BUDGET", MAX_PRIO_INLINES);
    let growth_pct = env_usize("FIRNC_INLINE_GROWTH", MAX_GROWTH_PCT);
    let stats = std::env::var_os("FIRNC_INLINE_STATS").is_some();
    // The frame budget is decided in codegen_x86 and only the x86-64 frames
    // share slots by lifetime; the other back ends keep round INLGZ.
    let rec_default = if matches!(crate::target::active(), crate::target::Target::X86_64) { 2 } else { 0 };
    let rec_mode = env_usize("FIRNC_INLINE_REC", rec_default);
    inline_by_size_with(m, dbg || stats, changed, budget, growth_pct, rec_mode)
}

/// `inline_by_size` with the two bounds and the recursion mode as parameters
/// (the unit tests set them without touching the process environment).
/// `rec_mode`: 0 nothing into recursive callers, 1 only one-block bodies up to
/// `REC_CALLER_TIER`, 2 everything, with the originals kept for the frame
/// budget.
fn inline_by_size_with(
    m: &mut Module,
    dbg: bool,
    changed: &mut Vec<usize>,
    budget: usize,
    growth_pct: usize,
    rec_mode: usize,
) -> usize {
    let idx: HashMap<String, usize> =
        m.funcs.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
    let mut cg = CallGraph::new(m, &idx);
    let nf = m.funcs.len();
    let mut self_rec: Vec<Option<bool>> = vec![None; nf];
    let base = m.inst_count();
    let max_growth = base * growth_pct / 100;
    let mut grown = 0usize;
    let mut n = 0usize;
    let mut per_tier: Vec<usize> = Vec::new();
    let mut kept: HashMap<String, Func> = HashMap::new();
    let mut small: HashMap<String, Func> = HashMap::new();
    // One tier per callee size, from just above the trivial leaves up: when
    // the growth guard stops the phase, it stops at a size boundary.
    let tiers: Vec<usize> = (MAX_TRIVIAL_INSTS + 1..=MAX_CALLEE_INSTS).collect();
    'tiers: for &tier in tiers.iter() {
        let at_start = n;
        for ci in 0..nf {
            {
                let c = &m.funcs[ci];
                if c.constant_time || !c.secret.is_empty() {
                    continue;
                }
            }
            let caller_rec = match self_rec[ci] {
                Some(v) => v,
                None => {
                    let v = cg.self_recursive(ci);
                    self_rec[ci] = Some(v);
                    v
                }
            };
            if caller_rec && (rec_mode == 0 || (rec_mode == 1 && tier > REC_CALLER_TIER)) {
                continue;
            }
            let mut start_b = 0usize;
            loop {
                if n >= budget || grown >= max_growth {
                    per_tier.push(n - at_start);
                    break 'tiers;
                }
                let caller = &m.funcs[ci];
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
                        let gi = match idx.get(name) {
                            Some(&g) => g,
                            None => continue,
                        };
                        if gi == ci {
                            continue;
                        }
                        let callee = &m.funcs[gi];
                        if callee.inst_count() > tier || !inlinable(callee) {
                            continue;
                        }
                        if callee.params.len() != args.len() {
                            continue;
                        }
                        if inst.dst.is_some() && callee.ret == FTy::Void {
                            continue;
                        }
                        // Mode 1: into a recursive caller only bodies that
                        // leave no frame slot behind (one block, no alloca).
                        if caller_rec
                            && rec_mode == 1
                            && !(callee.blocks.len() == 1
                                && !callee.blocks[0]
                                    .insts
                                    .iter()
                                    .any(|i| matches!(i.op, Op::Alloca { .. })))
                        {
                            continue;
                        }
                        if cg.reaches(gi, ci) {
                            continue;
                        }
                        let sr = match self_rec[gi] {
                            Some(v) => v,
                            None => {
                                let v = cg.self_recursive(gi);
                                self_rec[gi] = Some(v);
                                v
                            }
                        };
                        if sr {
                            continue;
                        }
                        site = Some((bi, ii, gi));
                        break 'scan;
                    }
                }
                match site {
                    Some((bi, ii, gi)) => {
                        if dbg && std::env::var_os("FIRNC_INLINE_DEBUG").is_some() {
                            eprintln!(
                                "inline: {} <- {} ({} insts, {} blocks, tier {})",
                                m.funcs[ci].name,
                                m.funcs[gi].name,
                                m.funcs[gi].inst_count(),
                                m.funcs[gi].blocks.len(),
                                tier
                            );
                        }
                        grown += m.funcs[gi].inst_count();
                        // ROUND TK2 (r136): keep the recursive caller as it
                        // was before its first embedding of this phase.
                        if caller_rec && rec_mode >= 2 && !kept.contains_key(&m.funcs[ci].name) {
                            kept.insert(m.funcs[ci].name.clone(), m.funcs[ci].clone());
                        }
                        inline_one(m, ci, bi, ii, gi);
                        changed.push(ci);
                        n += 1;
                        start_b = bi;
                    }
                    None => break,
                }
            }
        }
        per_tier.push(n - at_start);
        // ROUND TK2 (r136): after the last small tier, the recursive callers
        // that took something so far are snapshot as the middle version
        // (small bodies only).
        if rec_mode >= 2 && tier == REC_CALLER_TIER {
            for name in kept.keys() {
                if let Some(&ci) = idx.get(name) {
                    small.insert(name.clone(), m.funcs[ci].clone());
                }
            }
        }
    }
    if dbg {
        eprintln!(
            "inline-by-size: {} sites, tiers {:?} -> {:?}, module {} insts, +{} copied (guard {}), {} recursive callers kept for the frame budget",
            n, tiers, per_tier, base, grown, max_growth, kept.len()
        );
    }
    rec_originals_put(kept);
    rec_small_put(small);
    n
}

/// Embeds as long as the heuristic allows. Yields the number of embedded
/// calls.
pub fn inline_module(m: &mut Module) -> usize {
    inline_module_inner(m, false)
}

/// TEMPO 15 -- the SECOND round, after every function has been optimized
/// again, restricted to call sites inside loops and to callees that call
/// nothing (`find_site_x`, `hot_only`). Unrestricted it cost too much:
/// `bin/firnc1.fi` +17 % `.text` and ~70 % compile time (measured). The first round works caller by caller in module order, not bottom
/// up: `mp3__scale_pcm4` (31 instructions optimized) first received three
/// copies of `bcast` and was then, UNoptimized, 55 instructions -- above the
/// bound -- when `synth` asked for it. After the clean-up it is small again,
/// and this round embeds it. Returns the number of embeddings and the
/// callers that received one (only those need optimizing again).
pub fn inline_module_again(m: &mut Module) -> (usize, Vec<usize>) {
    let mut changed = Vec::new();
    let n = inline_module_track_x(m, false, true, &mut changed);
    changed.sort();
    changed.dedup();
    (n, changed)
}

fn inline_module_inner(m: &mut Module, requested_only: bool) -> usize {
    let mut changed = Vec::new();
    inline_module_track(m, requested_only, &mut changed)
}

fn inline_module_track(m: &mut Module, requested_only: bool, changed: &mut Vec<usize>) -> usize {
    inline_module_track_x(m, requested_only, false, changed)
}

fn inline_module_track_x(
    m: &mut Module,
    requested_only: bool,
    hot_only: bool,
    changed: &mut Vec<usize>,
) -> usize {
    let mut n = 0usize;
    let dbg = std::env::var("FIRNC_INLINE_DEBUG").is_ok();
    // Phase 1 of the first round: trivial leaves, outside the budget (not
    // under the default level, which embeds only what carries `#[inline]`).
    let trivial_done = if !hot_only && !requested_only && std::env::var_os("FIRNC_NO_TRIVIAL").is_none() {
        let t = inline_trivial_leaves(m, dbg, changed);
        if dbg {
            eprintln!("inline: {} trivial leaf sites", t);
        }
        t
    } else {
        0
    };
    n += trivial_done;
    // Phase 2: the size-ordered embedding (`inline_by_size`); the first-come
    // loop below then mops up what is left (explicit `#[inline]` requests of
    // bodies above the size bound, sites that only appeared through earlier
    // embeddings) with its own budget.
    let size_done = if !hot_only && !requested_only && std::env::var_os("FIRNC_INLINE_OLD").is_none() {
        inline_by_size(m, dbg, changed)
    } else {
        0
    };
    n += size_done;
    // the second round only embeds leaves: small bodies without any call
    let leaves: Option<HashSet<String>> = if hot_only {
        Some(
            m.funcs
                .iter()
                .filter(|g| {
                    inlinable(g)
                        && !g.blocks.iter().any(|b| {
                            b.insts.iter().any(|i| {
                                matches!(i.op, Op::Call { .. } | Op::CallIndirect { .. } | Op::Syscall { .. })
                            })
                        })
                })
                .map(|g| g.name.clone())
                .collect(),
        )
    } else {
        None
    };
    // Determined once: it hangs off the body of the callee only, which never
    // changes through embeddings (only the caller is mutated).
    // A leaf calls nothing and cannot recurse -- the second round embeds
    // leaves only and skips this walk over the whole call graph (it was the
    // bigger half of that round's cost on `bin/firnc1.fi`).
    let idx = name_index(m);
    let mut cg = CallGraph::new(m, &idx);
    let self_rec: Vec<bool> = if hot_only {
        vec![false; m.funcs.len()]
    } else {
        self_recursive_all(&mut cg, m.funcs.len())
    };
    // TEMPO 15: an embedding changes the CALLER only, so a caller that had
    // no site left keeps having none -- the scan stays at `ci` until it is
    // exhausted instead of starting again at function 0 after every
    // embedding. Same result, but linear instead of quadratic in the number
    // of embeddings (measured on `bin/firnc1.fi` with the bound lifted: the
    // restart made it run for minutes).
    let mut start = 0usize;
    'outer: loop {
        for ci in start..m.funcs.len() {
            if let Some((bi, ii, gi)) =
                find_site_x(m, ci, &self_rec, requested_only, hot_only, leaves.as_ref(), &idx, &mut cg)
            {
                if dbg {
                    eprintln!("inline: {} <- {} ({} insts, {} blocks)",
                        m.funcs[ci].name, m.funcs[gi].name,
                        m.funcs[gi].inst_count(), m.funcs[gi].blocks.len());
                }
                inline_one(m, ci, bi, ii, gi);
                changed.push(ci);
                n += 1;
                if n - trivial_done - size_done >= max_inlines() {
                    break 'outer;
                }
                start = ci;
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

    fn main_calling(callee: &str) -> Func {
        let mut f = Func::new("main", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(2));
        let b = f.push(0, FTy::I32, Op::Const(40));
        let r = f.push(0, FTy::I32, Op::Call { name: callee.into(), args: vec![a, b] });
        f.set_term(0, Term::Ret(Some(r)));
        f
    }

    /// Like `main_calling`, but the call sits in a loop (block 1 loops on
    /// itself): the dev-fast leaf inliner only touches such call sites.
    fn main_calling_in_loop(callee: &str) -> Func {
        let mut f = Func::new("main", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(2));
        let b = f.push(0, FTy::I32, Op::Const(40));
        let l = f.add_block();
        let x = f.add_block();
        f.set_term(0, Term::Br(l));
        let r = f.push(l, FTy::I32, Op::Call { name: callee.into(), args: vec![a, b] });
        f.set_term(l, Term::BrCond { cond: r, then_bb: l, else_bb: x });
        f.set_term(x, Term::Ret(Some(r)));
        f
    }

    fn calls_in(m: &Module, func: &str) -> usize {
        let f = m.funcs.iter().find(|f| f.name == func).expect("function");
        f.blocks.iter().flat_map(|b| b.insts.iter()).filter(|i| matches!(i.op, Op::Call { .. })).count()
    }

    /// r591: the dev-fast leaf inliner embeds a call-free, loop-free body
    /// when the call site is inside a loop of the caller, and leaves a call
    /// outside every loop alone.
    #[test]
    fn dev_leaf_is_embedded() {
        let mut m = Module::new();
        m.funcs.push(add_fn());
        m.funcs.push(main_calling_in_loop("add"));
        let (n, changed) = inline_module_dev_leaves(&mut m);
        assert_eq!(n, 1);
        assert_eq!(changed.len(), 1);
        assert_eq!(calls_in(&m, "main"), 0);

        let mut m = Module::new();
        m.funcs.push(add_fn());
        m.funcs.push(main_calling("add"));
        let (n, changed) = inline_module_dev_leaves(&mut m);
        assert_eq!(n, 0);
        assert!(changed.is_empty());
        assert_eq!(calls_in(&m, "main"), 1);
    }

    /// r591: a callee that calls anything is no dev leaf (its frame would
    /// vanish from a call stack), nor is one that carries `#[no_inline]`.
    #[test]
    fn dev_leaf_refuses_callers_and_no_inline() {
        let mut m = Module::new();
        let mut g = Func::new("g", vec![FTy::I32, FTy::I32], FTy::I32);
        let r = g.push(0, FTy::I32, Op::Call { name: "add".into(), args: vec![0, 1] });
        g.set_term(0, Term::Ret(Some(r)));
        m.funcs.push(add_fn());
        m.funcs.push(g);
        m.funcs.push(main_calling("g"));
        assert!(!dev_leaf(&m.funcs[1]));
        let mut h = add_fn();
        h.inline_hint = Some(false);
        assert!(!dev_leaf(&h));
        assert!(dev_leaf(&m.funcs[0]));
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

    /// Round EINBETTEN: `#[no_inline]` forbids inlining, even if the body is
    /// tiny and the size rule would have taken it.
    #[test]
    fn no_inline_forbids_inlining() {
        let mut m = Module::new();
        let mut g = add_fn();
        g.inline_hint = Some(false);
        m.funcs.push(g);
        let mut f = Func::new("main", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(2));
        let r = f.push(0, FTy::I32, Op::Call { name: "add".into(), args: vec![a, a] });
        f.set_term(0, Term::Ret(Some(r)));
        m.funcs.push(f);
        assert_eq!(inline_module(&mut m), 0);
    }

    /// `#[inline]` lifts the SIZE LIMIT: a body above `MAX_CALLEE_INSTS` is
    /// inlined, which the rule would otherwise refuse.
    #[test]
    fn inline_lifts_the_size_limit() {
        // A body with clearly more than MAX_CALLEE_INSTS instructions.
        let mut g = Func::new("big", vec![FTy::I32], FTy::I32);
        let mut cur: Val = 0;
        for _ in 0..(MAX_CALLEE_INSTS + 10) {
            cur = g.push(0, FTy::I32, Op::Bin(BinOp::Add, cur, 0));
        }
        g.set_term(0, Term::Ret(Some(cur)));
        assert!(g.inst_count() > MAX_CALLEE_INSTS);
        let build = |g: Func| {
            let mut m = Module::new();
            m.funcs.push(g);
            let mut f = Func::new("main", vec![], FTy::I32);
            let a = f.push(0, FTy::I32, Op::Const(1));
            let r = f.push(0, FTy::I32, Op::Call { name: "big".into(), args: vec![a] });
            f.set_term(0, Term::Ret(Some(r)));
            m.funcs.push(f);
            m
        };
        // without the mark: the size rule refuses
        let mut m1 = build(g.clone());
        assert_eq!(inline_module(&mut m1), 0);
        // with #[inline]: inlined
        g.inline_hint = Some(true);
        let mut m2 = build(g);
        assert_eq!(inline_module(&mut m2), 1);
    }

    /// The HARD blocks stay: `#[inline]` on a `#[constant_time]` function
    /// changes nothing. That is a question of correctness (SPEC 9.2).
    #[test]
    fn inline_does_not_lift_the_hard_blocks() {
        let mut m = Module::new();
        let mut g = add_fn();
        g.constant_time = true;
        g.inline_hint = Some(true);
        m.funcs.push(g);
        let mut f = Func::new("main", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(2));
        let r = f.push(0, FTy::I32, Op::Call { name: "add".into(), args: vec![a, a] });
        f.set_term(0, Term::Ret(Some(r)));
        m.funcs.push(f);
        assert_eq!(inline_module(&mut m), 0);
    }

    /// Recursion stays blocked too, with `#[inline]` as without.
    #[test]
    fn inline_does_not_break_the_recursion_block() {
        let mut m = Module::new();
        let mut f = Func::new("fact", vec![FTy::I32], FTy::I32);
        f.inline_hint = Some(true);
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

    /// The pass of the default level takes ONLY what carries `#[inline]`.
    #[test]
    fn requested_only_takes_only_marked() {
        let build = |hint: Option<bool>| {
            let mut m = Module::new();
            let mut g = add_fn();
            g.inline_hint = hint;
            m.funcs.push(g);
            let mut f = Func::new("main", vec![], FTy::I32);
            let a = f.push(0, FTy::I32, Op::Const(2));
            let r = f.push(0, FTy::I32, Op::Call { name: "add".into(), args: vec![a, a] });
            f.set_term(0, Term::Ret(Some(r)));
            m.funcs.push(f);
            m
        };
        // without the mark: the default pass leaves the call alone ...
        let mut m1 = build(None);
        assert_eq!(inline_module_requested_only(&mut m1), 0);
        // ... although the size rule would have taken it.
        let mut m2 = build(None);
        assert_eq!(inline_module(&mut m2), 1);
        // with the mark: the default pass takes it too.
        let mut m3 = build(Some(true));
        assert_eq!(inline_module_requested_only(&mut m3), 1);
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

    /// A straight-line function of `n` instructions (`n` adds of the
    /// parameter), one block, no calls: a leaf of a size we choose.
    fn chain_fn(name: &str, n: usize) -> Func {
        let mut g = Func::new(name, vec![FTy::I32], FTy::I32);
        let mut v: Val = 0;
        for _ in 0..n {
            v = g.push(0, FTy::I32, Op::Bin(BinOp::Add, v, 0));
        }
        g.set_term(0, Term::Ret(Some(v)));
        g
    }

    /// A caller that calls each of `callees` once per entry, in order.
    fn caller_of(name: &str, callees: &[&str]) -> Func {
        let mut f = Func::new(name, vec![], FTy::I32);
        let mut acc = f.push(0, FTy::I32, Op::Const(1));
        for c in callees {
            acc = f.push(0, FTy::I32, Op::Call { name: (*c).into(), args: vec![acc] });
        }
        f.set_term(0, Term::Ret(Some(acc)));
        f
    }

    fn calls_to_func(f: &Func, callee: &str) -> usize {
        f.blocks
            .iter()
            .map(|b| b.insts.iter().filter(|i| matches!(&i.op, Op::Call { name, .. } if name == callee)).count())
            .sum()
    }

    fn calls_to(m: &Module, caller: &str, callee: &str) -> usize {
        calls_to_func(m.funcs.iter().find(|f| f.name == caller).expect("caller"), callee)
    }

    #[test]
    fn trivial_leaves_do_not_count_against_the_budget() {
        // 2500 call sites of a one instruction leaf: more than MAX_INLINES
        // (2000), and every one of them must be embedded (Certus: the GC
        // runtime's `__gc_ld64` accessors).
        let mut m = Module::new();
        m.funcs.push(chain_fn("leaf", 1));
        let names: Vec<&str> = std::iter::repeat("leaf").take(2500).collect();
        m.funcs.push(caller_of("user", &names));
        let n = inline_module(&mut m);
        assert_eq!(n, 2500);
        assert_eq!(calls_to(&m, "user", "leaf"), 0);
    }

    #[test]
    fn a_leaf_above_the_trivial_bound_is_not_a_trivial_leaf() {
        assert!(trivial_leaf(&chain_fn("a", MAX_TRIVIAL_INSTS)));
        assert!(!trivial_leaf(&chain_fn("b", MAX_TRIVIAL_INSTS + 1)));
        // calls and allocas disqualify
        let mut g = Func::new("c", vec![], FTy::I32);
        let r = g.push(0, FTy::I32, Op::Call { name: "x".into(), args: vec![] });
        g.set_term(0, Term::Ret(Some(r)));
        assert!(!trivial_leaf(&g));
    }

    #[test]
    fn size_ordered_phase_takes_the_smallest_bodies_first() {
        // budget of ONE site: the 8 instruction body must win against the 30
        // instruction body although the big one is called first.
        let mut m = Module::new();
        m.funcs.push(chain_fn("big", 30));
        m.funcs.push(chain_fn("small", 8));
        m.funcs.push(caller_of("user", &["big", "small"]));
        let mut changed = Vec::new();
        let n = inline_by_size_with(&mut m, false, &mut changed, 1, 100, 0);
        assert_eq!(n, 1);
        assert_eq!(calls_to(&m, "user", "small"), 0);
        assert_eq!(calls_to(&m, "user", "big"), 1);
        assert_eq!(changed, vec![2]);
    }

    #[test]
    fn size_ordered_phase_stops_at_the_growth_guard() {
        // The guard is a percentage of the module size before the phase: a
        // module of 1 203 instructions (8 + 5 + a filler of 1 190) at 1 % lets
        // 12 instructions be copied. The guard is checked BEFORE each
        // embedding: 8 copied < 12 embeds a second one, 16 >= 12 stops.
        let mut m = Module::new();
        m.funcs.push(chain_fn("small", 8));
        m.funcs.push(caller_of("user", &["small", "small", "small", "small"]));
        m.funcs.push(chain_fn("filler", 1190));
        assert_eq!(m.inst_count(), 1203);
        let mut changed = Vec::new();
        let n = inline_by_size_with(&mut m, false, &mut changed, 1000, 1, 0);
        assert_eq!(n, 2);
        assert_eq!(calls_to(&m, "user", "small"), 2);
    }

    #[test]
    fn size_ordered_phase_gives_a_recursive_caller_nothing() {
        // `rec` calls itself and a small body: its frame must not grow, so
        // the small body stays a call (the trivial-leaf phase is separate).
        let mut m = Module::new();
        m.funcs.push(chain_fn("small", 8));
        m.funcs.push(caller_of("rec", &["small", "rec"]));
        let mut changed = Vec::new();
        let n = inline_by_size_with(&mut m, false, &mut changed, 1000, 100, 0);
        assert_eq!(n, 0);
        assert_eq!(calls_to(&m, "rec", "small"), 1);
    }

    /// The tables of the frame budget are process wide, so the tests that
    /// use them take turns.
    static REC_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn size_ordered_phase_keeps_the_original_of_a_recursive_caller() {
        // mode 2: `rec` takes the small body, and its body as it was before is
        // kept for the frame budget; the middle version (after the small
        // tiers) is kept too because the body is below REC_CALLER_TIER.
        let _guard = REC_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = rec_originals_take();
        let _ = rec_small_take();
        let mut m = Module::new();
        m.funcs.push(chain_fn("small", 8));
        m.funcs.push(caller_of("rec_k", &["small", "rec_k"]));
        let mut changed = Vec::new();
        let n = inline_by_size_with(&mut m, false, &mut changed, 1000, 100, 2);
        assert_eq!(n, 1);
        assert_eq!(calls_to(&m, "rec_k", "small"), 0);
        let kept = rec_originals_take();
        assert_eq!(kept.len(), 1);
        assert_eq!(calls_to_func(&kept["rec_k"], "small"), 1, "the original still calls the body");
        let mid = rec_small_take();
        assert_eq!(mid.len(), 1);
        assert_eq!(calls_to_func(&mid["rec_k"], "small"), 0, "the middle version has the small body in");
    }

    #[test]
    fn size_ordered_phase_mode_one_takes_only_one_block_bodies() {
        let _guard = REC_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = rec_originals_take();
        let _ = rec_small_take();
        let mut m = Module::new();
        m.funcs.push(chain_fn("small", 8));
        m.funcs.push(caller_of("rec_1", &["small", "rec_1"]));
        let mut changed = Vec::new();
        let n = inline_by_size_with(&mut m, false, &mut changed, 1000, 100, 1);
        // `chain_fn` is a single block without an alloca: taken, and nothing
        // is kept in mode 1
        assert_eq!(n, 1);
        assert!(rec_originals_take().is_empty());
        assert!(rec_small_take().is_empty());
    }

    #[test]
    fn the_frame_budget_is_sixty_four_octets_or_an_eighth() {
        assert!(rec_frame_within_budget(112, 112));
        assert!(rec_frame_within_budget(176, 112)); // 112 + 64
        assert!(!rec_frame_within_budget(177, 112));
        assert!(rec_frame_within_budget(1_150, 1_024)); // 1_024 + 128
        assert!(!rec_frame_within_budget(1_153, 1_024));
    }

    #[test]
    fn size_ordered_phase_leaves_recursion_alone() {
        // the callee reaches the caller again: embedding would unroll a level
        let mut m = Module::new();
        m.funcs.push(caller_of("a", &["b"]));
        m.funcs.push(caller_of("b", &["a"]));
        let mut changed = Vec::new();
        let n = inline_by_size_with(&mut m, false, &mut changed, 1000, 100, 0);
        assert_eq!(n, 0);
    }
}
