// SPDX-License-Identifier: MPL-2.0
//! **Round TEMPO 10 — cutting up lifetimes.**
//!
//! ## The measured problem
//!
//! `FIRN_RA_STATS=1` says for the MP3 decoder: `synth` has 44 simultaneously
//! live values, `l3_huffman` 63 — with **fourteen** registers. What lies above that
//! stays in the frame, and every use fetches it from there:
//!
//! ```text
//!   mov  -0x1760(%rbp),%rax      ; fetch the pointer
//!   lea  (%rax,%r10,1),%r11      ; use it
//! ```
//!
//! Two instructions instead of one, and that **per iteration**. Counted over the whole
//! program: 10.0 of 137.7 million instructions are "fetch from the frame" — the
//! biggest single item that still stands.
//!
//! ## Why the allocator does not solve this by itself
//!
//! The linear scan knows ONE interval per value, from the first to the
//! last touch, and ONE slot. A pointer that is set at the beginning of the
//! function and needed once more at the end thus occupies its
//! register over the whole function — or none at all. In between
//! lies the hot loop, in which exactly this register is missing.
//!
//! The textbook answer is called *live range splitting*: cut the interval into
//! pieces and give each piece a slot of its own. In the allocator
//! itself that would be a rebuild of every output site — `loc(v)` would have to depend on the
//! POSITION.
//!
//! ## The way without a rebuild: cutting with a copy
//!
//! The same result is obtained by making the piece a SEPARATE VALUE. Before
//! the loop there stands
//!
//! ```text
//! P:  %v2 = copy %v
//! ```
//!
//! and every use of `%v` **inside** the loop reads `%v2` from now on. With that `%v2` has
//! a short interval with high weight (readers times loop depth) and almost
//! certainly gets a register, while `%v` may quietly stay in the frame. A fetch
//! per iteration becomes one per loop entry.
//!
//! The allocator needs not a line for this. And if the cut brings nothing — because
//! `%v` is no longer needed after the loop —, the merging from TEMPO 8/10
//! undoes it again by itself:
//! `%v` and `%v2` then do not interfere and get the same slot, the
//! copy disappears.
//!
//! ## When it is cut
//!
//! * The value is **read at least twice** in the loop. With a
//!   single reader the copy would be exactly as expensive as the fetch.
//! * The value is **not written** in the loop — otherwise `%v`
//!   and `%v2` would be different things after the first iteration.
//! * It is **not a constant** (that stands as an immediate operand in the
//!   instruction and never needs a slot) and **not an `alloca`** (whose
//!   address `direct_frame_addrs` calculates without a register anyway).
//! * It is not `secret` (SPEC §9.2).
//! * The loop has a **pre-header** with exactly one exit, as with
//!   `licm`.
//!
//! **`phi` instructions are not rewritten.** A `phi` in the
//! loop head reads for the edge from the pre-header a value that applies BEFORE
//! the copy; the edge from the body reads a different one. Telling
//! these apart would be possible, but the gain lies in the
//! ordinary uses, not in the phis.

use crate::fir::{Func, Inst, Op, Term, Val};
use std::collections::HashSet;

/// Cuts lifetimes at loop boundaries. Returns the number of
/// inserted copies.
pub(crate) fn split_at_loops(f: &mut Func) -> usize {
    let n = f.blocks.len();
    if n < 3 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    // The same cheap pre-check as in `licm`: without a back edge there
    // is no loop.
    let backward = f
        .blocks
        .iter()
        .enumerate()
        .any(|(b, blk)| blk.term.successors().into_iter().any(|s| (s as usize) <= b));
    if !backward {
        return 0;
    }
    let preds = crate::mem2reg::preds(f);
    let dom = crate::mem2reg::dominators(f);

    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (b, blk) in f.blocks.iter().enumerate() {
        for s in blk.term.successors() {
            let h = s as usize;
            if h < n && dom[b][h] {
                edges.push((h, b));
            }
        }
    }
    if edges.is_empty() {
        return 0;
    }
    // Innermost loops first: the smaller body lies further inside.
    let mut loops: Vec<(usize, HashSet<usize>)> = edges
        .into_iter()
        .map(|(h, b)| (h, crate::licm::natural_loop(h, b, &preds)))
        .collect();
    loops.sort_by_key(|(_, body)| body.len());

    // Where is what written? (once for the whole function)
    let nv = f.val_types.len();
    let mut def_in: Vec<Option<usize>> = vec![None; nv];
    for (bi, b) in f.blocks.iter().enumerate() {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if (d as usize) < nv {
                    def_in[d as usize] = Some(bi);
                }
            }
        }
    }
    // Which values are constants or `alloca` addresses?
    let mut raw: Vec<bool> = vec![false; nv];
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if (d as usize) < nv && matches!(i.op, Op::Const(_) | Op::Alloca { .. }) {
                    raw[d as usize] = true;
                }
            }
        }
    }

    let at_least: u32 = match std::env::var("FIRN_SPLIT_MIN") {
        Ok(v) => v.parse().unwrap_or(3),
        Err(_) => 3,
    };
    let mut inserted = 0usize;
    let mut already: HashSet<usize> = HashSet::new(); // Kopf schon bearbeitet

    for (head, body) in loops {
        if !already.insert(head) {
            continue;
        }
        let preheader = match crate::licm::preheader_of(f, head, &body, &preds) {
            Some(p) => p,
            None => continue,
        };
        // --- count: how often is which value READ in the body? ------------
        let mut readers: Vec<u32> = vec![0; f.val_types.len()];
        let mut written: HashSet<Val> = HashSet::new();
        let mut buf = Vec::new();
        for &bi in body.iter() {
            for i in &f.blocks[bi].insts {
                if let Some(d) = i.dst {
                    written.insert(d);
                }
                // A `phi` is not rewritten, so it does not count
                // as a reader either.
                if matches!(i.op, Op::Phi { .. }) {
                    continue;
                }
                buf.clear();
                i.op.uses(&mut buf);
                for u in buf.iter() {
                    if let Some(c) = readers.get_mut(*u as usize) {
                        *c += 1;
                    }
                }
            }
            match &f.blocks[bi].term {
                Term::BrCond { cond: v, .. }
                | Term::Switch { val: v, .. }
                | Term::Ret(Some(v)) => {
                    if let Some(c) = readers.get_mut(*v as usize) {
                        *c += 1;
                    }
                }
                _ => {}
            }
        }
        // --- Bewerber sammeln --------------------------------------------
        let mut candidates: Vec<(u32, Val)> = Vec::new();
        for v in 0..f.val_types.len() {
            let vv = v as Val;
            if readers[v] < at_least || written.contains(&vv) {
                continue;
            }
            if f.is_secret(vv) || raw.get(v).copied().unwrap_or(false) {
                continue;
            }
            // Defined outside the loop, and the definition must dominate the
            // pre-header (otherwise the value does not exist there).
            if v >= f.params.len() {
                match def_in.get(v).copied().flatten() {
                    Some(db) => {
                        if body.contains(&db) {
                            continue;
                        }
                        if db != preheader && !dom[preheader][db] {
                            continue;
                        }
                    }
                    None => continue,
                }
            }
            candidates.push((readers[v], vv));
        }
        if candidates.is_empty() {
            continue;
        }
        candidates.sort_by_key(|(c, v)| (std::cmp::Reverse(*c), *v));

        // --- schneiden ----------------------------------------------------
        let mut map: Vec<(Val, Val)> = Vec::new();
        for (_, v) in candidates.iter().copied() {
            let ty = f.val_ty(v);
            let neu = f.new_val_pub(ty);
            let loc = f.blocks[preheader].insts.last().map(|i| i.loc).unwrap_or_default();
            f.blocks[preheader].insts.push(Inst::like(Some(neu), ty, Op::Copy { src: v }, loc));
            map.push((v, neu));
            inserted += 1;
        }
        // Verwendungen im Rumpf umschreiben (phis ausgenommen).
        for &bi in body.iter() {
            for i in f.blocks[bi].insts.iter_mut() {
                if matches!(i.op, Op::Phi { .. }) {
                    continue;
                }
                i.op.for_each_use_mut(|v| {
                    for (alt, neu) in map.iter().copied() {
                        if *v == alt {
                            *v = neu;
                            break;
                        }
                    }
                });
            }
            match &mut f.blocks[bi].term {
                Term::BrCond { cond: v, .. }
                | Term::Switch { val: v, .. }
                | Term::Ret(Some(v)) => {
                    for (alt, neu) in map.iter().copied() {
                        if *v == alt {
                            *v = neu;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    inserted
}


// ---------------------------------------------------------------------------
// AFTER THE ALLOCATION — the cut that knows what it is good for
// ---------------------------------------------------------------------------
//
// THE MEASUREMENT THAT FORCED THIS SECOND ATTEMPT. The version above
// cuts in the optimiser, that is BEFORE anyone knows which value gets
// a register at all. Measured on the MP3 decoder: 137.7 -> 140.4 million instructions,
// that is two percent WORSE. The reason is simple and obvious in hindsight: where the new value
// gets only a slot, you pay for the copy in the pre-header and gain nothing, because the body
// then simply reads the other slot.
//
// So the other way round. `emit_func_ra` allocates ONCE, asks here which
// values really landed in the frame AND are read several times in a loop, cuts only
// these, and allocates once more. If in doing so not a single one of the new values
// gets a register, the cut is discarded — then it costs only
// translation time and not a single bit in the program.

/// Values that landed in the frame and are read several times in a loop:
/// for each a copy into the pre-header, and in the body everything reads
/// the copy. `None` = there is nothing to cut.
///
/// Returns the changed function and the list of the new values.
pub(crate) fn after_allocation(
    f: &Func,
    in_frame: &dyn Fn(Val) -> bool,
) -> Option<(Func, Vec<Val>)> {
    let n = f.blocks.len();
    if n < 3 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return None;
    }
    let backward = f
        .blocks
        .iter()
        .enumerate()
        .any(|(b, blk)| blk.term.successors().into_iter().any(|s| (s as usize) <= b));
    if !backward {
        return None;
    }
    let preds = crate::mem2reg::preds(f);
    let dom = crate::mem2reg::dominators(f);
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (b, blk) in f.blocks.iter().enumerate() {
        for s in blk.term.successors() {
            let h = s as usize;
            if h < n && dom[b][h] {
                edges.push((h, b));
            }
        }
    }
    if edges.is_empty() {
        return None;
    }
    let mut loops: Vec<(usize, HashSet<usize>)> = edges
        .into_iter()
        .map(|(h, b)| (h, crate::licm::natural_loop(h, b, &preds)))
        .collect();
    loops.sort_by_key(|(_, body)| body.len());

    let nv = f.val_types.len();
    let mut def_in: Vec<Option<usize>> = vec![None; nv];
    let mut raw: Vec<bool> = vec![false; nv];
    for (bi, b) in f.blocks.iter().enumerate() {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if (d as usize) < nv {
                    def_in[d as usize] = Some(bi);
                    if matches!(i.op, Op::Const(_) | Op::Alloca { .. }) {
                        raw[d as usize] = true;
                    }
                }
            }
        }
    }
    let at_least: u32 = match std::env::var("FIRN_SPLIT_MIN") {
        Ok(v) => v.parse().unwrap_or(3),
        Err(_) => 3,
    };

    let mut g = f.clone();
    let mut fresh: Vec<Val> = Vec::new();
    let mut done: HashSet<usize> = HashSet::new();

    for (head, body) in loops {
        if !done.insert(head) {
            continue;
        }
        let preheader = match crate::licm::preheader_of(f, head, &body, &preds) {
            Some(p) => p,
            None => continue,
        };
        let mut readers: Vec<u32> = vec![0; nv];
        let mut written: HashSet<Val> = HashSet::new();
        let mut buf = Vec::new();
        for &bi in body.iter() {
            for i in &f.blocks[bi].insts {
                if let Some(d) = i.dst {
                    written.insert(d);
                }
                buf.clear();
                i.op.uses(&mut buf);
                for u in buf.iter() {
                    if let Some(c) = readers.get_mut(*u as usize) {
                        *c += 1;
                    }
                }
            }
            match &f.blocks[bi].term {
                Term::BrCond { cond: v, .. }
                | Term::Switch { val: v, .. }
                | Term::Ret(Some(v)) => {
                    if let Some(c) = readers.get_mut(*v as usize) {
                        *c += 1;
                    }
                }
                _ => {}
            }
        }
        let mut candidates: Vec<(u32, Val)> = Vec::new();
        for v in 0..nv {
            let vv = v as Val;
            if readers[v] < at_least || written.contains(&vv) {
                continue;
            }
            if f.is_secret(vv) || raw[v] {
                continue;
            }
            // THAT IS THE DIFFERENCE TO THE VERSION ABOVE: only what the allocator
            // really put into the frame.
            if !in_frame(vv) {
                continue;
            }
            if v >= f.params.len() {
                match def_in[v] {
                    Some(db) => {
                        if body.contains(&db) {
                            continue;
                        }
                        if db != preheader && !dom[preheader][db] {
                            continue;
                        }
                    }
                    None => continue,
                }
            }
            candidates.push((readers[v], vv));
        }
        if candidates.is_empty() {
            continue;
        }
        candidates.sort_by_key(|(c, v)| (std::cmp::Reverse(*c), *v));
        let mut map: Vec<(Val, Val)> = Vec::new();
        for (_, v) in candidates.iter().copied() {
            let ty = g.val_ty(v);
            let neu = g.new_val_pub(ty);
            let loc = g.blocks[preheader].insts.last().map(|i| i.loc).unwrap_or_default();
            g.blocks[preheader].insts.push(Inst::like(Some(neu), ty, Op::Copy { src: v }, loc));
            map.push((v, neu));
            fresh.push(neu);
        }
        for &bi in body.iter() {
            for i in g.blocks[bi].insts.iter_mut() {
                i.op.for_each_use_mut(|x| {
                    for (alt, neu) in map.iter().copied() {
                        if *x == alt {
                            *x = neu;
                            break;
                        }
                    }
                });
            }
            match &mut g.blocks[bi].term {
                Term::BrCond { cond: v, .. }
                | Term::Switch { val: v, .. }
                | Term::Ret(Some(v)) => {
                    for (alt, neu) in map.iter().copied() {
                        if *v == alt {
                            *v = neu;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if fresh.is_empty() {
        None
    } else {
        // The cut must not be merged again at once.
        for v in fresh.iter() {
            g.no_coalesce.insert(*v);
        }
        Some((g, fresh))
    }
}
