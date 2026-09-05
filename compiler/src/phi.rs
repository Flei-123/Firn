// SPDX-License-Identifier: GPL-2.0-only
//! **ROUND 92 — phi elimination.** The last thing that happens to FIR before
//! a code generator sees it.
//!
//! ## Why this file exists, and why it is ONE file
//!
//! `mem2reg.rs` puts phi nodes into FIR because that is the only way to say
//! "the value here depends on which edge we came in on". No machine has an
//! instruction for that. So somewhere the phi has to become what it has
//! always meant on a real processor: **a copy at the end of every
//! predecessor**.
//!
//! Round 90's finding was that one question must not have two answers. There
//! the question was "which registers does this instruction destroy" and the
//! two answers had silently drifted apart for three rounds. Phi elimination
//! is exactly that kind of question — every backend could do it for itself,
//! and there are three of them (the x86 base path, the register aware path
//! in `regalloc.rs`, the A64 path). So it is done **once, here**, on FIR,
//! and every backend afterwards reads a phi-free instruction list exactly as
//! it did before round 92. `Op::Phi` never reaches a backend; the arms that
//! catch it there return an internal error rather than emitting anything.
//!
//! ## The one hard part: the copies of an edge happen SIMULTANEOUSLY
//!
//! A block can carry several phis, and the block they come from gets the
//! copies for all of them **at the same moment**. All reads happen before
//! all writes. Written out one after another that is wrong the moment one
//! phi reads what another one writes:
//!
//! ```text
//! bb3:  %a = phi [bb2 %b, ...]        two values swapping places on the
//!       %b = phi [bb2 %a, ...]        back edge of a loop
//! ```
//!
//! Emitting `%a = copy %b` and then `%b = copy %a` gives both values the
//! same content. That is the **swap problem**, and it is not a corner case —
//! it is what a rotation of two variables in a loop looks like after SSA
//! construction.
//!
//! The answer is the standard sequentialization:
//!
//!  1. drop the copies that copy a value onto itself,
//!  2. emit any copy whose **target is nobody else's source** — after that
//!     one nothing can read the old content any more, so it is safe,
//!  3. if nothing is left that is safe, the rest is a **cycle**: rescue one
//!     target into a fresh value, point every source that named it at the
//!     rescue, and the cycle is broken open at that one place.
//!
//! That is provably correct for any permutation and costs one extra value
//! per cycle, which is one `mov` per cycle in the emitted code.
//!
//! ## Critical edges are deliberately NOT split
//!
//! The textbook says: split an edge from a block with several successors
//! into a block with several predecessors, then put the copies in the new
//! block. That matters when the copies are **coalesced** with their source,
//! because then a copy placed too early can overwrite a value another path
//! still needs (the "lost copy" problem).
//!
//! Nothing here coalesces. A copy defines a **new** value that no other path
//! reads, so writing it at the end of a predecessor that also branches
//! somewhere else writes a value that is dead on that other path — and
//! `regalloc.rs` computes its liveness from the code it actually gets, so it
//! sees exactly that. Splitting would add a block per critical edge for no
//! gain, and every added block lengthens the linear numbering the interval
//! allocator works on. When a later round teaches the allocator to coalesce
//! copies, splitting has to come with it; that is written down in
//! docs/ROUND92.md and not left to be rediscovered.
//!
//! ## What the copies cost, and the one place they are given back
//!
//! A phi per loop variable means a copy per back edge, and a copy nobody
//! folds is a `mov` in the innermost loop. Measured on `s = s + i; i = i + 1`
//! the naive form is one instruction LONGER per pass than what round 91
//! produced, because round 91's register allocator kept the counter in a
//! register and updated it in place. A foundation that makes the generated
//! code worse is not a foundation.
//!
//! Giving it back needs no interference graph. In by far the commonest shape
//! the value a phi reads is computed in the predecessor itself and read by
//! nothing else:
//!
//! ```text
//! bb2:  %11 = add %17, %18       <- computed here, read only by the phi
//!       %14 = add %18, %1
//!       br bb1                   <- copies: %17 <- %11, %18 <- %14
//! ```
//!
//! Then the computation can write the phi's value **directly** and the copy
//! disappears: `%17 = add %17, %18`. That is not SSA any more, and it does
//! not have to be — nothing reads FIR as SSA after this pass.
//!
//! It is safe under four conditions, and every one of them is there because
//! dropping it produces wrong code:
//!
//!  * the predecessor has **exactly one successor**. Otherwise writing the
//!    phi's value early would write it on the path that does NOT go to the
//!    phi's block as well, where something may still be reading the old one.
//!  * the value is computed **in that predecessor** and has **exactly one
//!    reader in the whole function** — this phi entry.
//!  * the phi's own value is **not read in the predecessor after that
//!    point**, and is not the source of another copy on the same edge.
//!    `%11 = add %17, %18` reading `%17` at the very instruction that will
//!    write it is fine (operands are read before the result is stored); a
//!    `mul %17, 2` two lines further down is not, and the index comparison
//!    is what tells the two apart.
//!  * the defining instruction is none of the untouchable ones (SPEC 9.2)
//!    and neither value is `secret`.

use crate::fir::{BlockId, Func, Inst, Module, Op, Term, Val};

/// Runs over the whole module. Afterwards no `Op::Phi` exists any more.
pub fn eliminate(m: &mut Module) -> Result<(), String> {
    for f in m.funcs.iter_mut() {
        eliminate_func(f)?;
    }
    Ok(())
}

/// One parallel copy: `dst` gets `src`, all of them at the same moment.
struct Par {
    dst: Val,
    ty: crate::fir::FTy,
    src: Val,
    /// ROUND 94 -- the position of the phi this copy resolves. The copy is
    /// the assignment the source program wrote at that place, so it belongs
    /// to that line and to no other.
    loc: crate::fir::Loc,
}

pub(crate) fn eliminate_func(f: &mut Func) -> Result<(), String> {
    if !f.has_phi() {
        return Ok(());
    }
    // The entry block cannot carry a phi: the edge the function is entered
    // through has no block to put a copy in. `mem2reg.rs` refuses to promote
    // anything in a function whose entry block has a predecessor, so this is
    // an assertion and not a case.
    if f.blocks.first().map(|b| b.has_phi()).unwrap_or(false) {
        return Err(format!("internal error: @{} has a phi in the entry block", f.name));
    }
    // Trim entries that stopped being edges; that also folds the phis that
    // have only one answer left, so fewer copies get emitted below.
    crate::mem2reg::simplify_phis(f);
    if !f.has_phi() {
        return Ok(());
    }
    // ROUND 92 -- THE LAST GATE. Everything above this line has had its
    // chance to keep the entry lists in step with the control flow graph;
    // from here on a wrong list becomes wrong MACHINE CODE, and a wrong
    // number in a program is the most expensive thing this compiler can
    // produce. So the invariants are checked once, here, for every function
    // and in every build -- one predecessor table against a list that is
    // already in memory. A failure stops the compilation with a message
    // that names the block instead of shipping a program that computes the
    // wrong answer.
    f.verify_phis()?;

    // ROUND 92 -- collapse the chains of phis first; see `coalesce_chains`.
    coalesce_chains(f);

    let nb = f.blocks.len();
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); nb];
    for (i, b) in f.blocks.iter().enumerate() {
        for s in b.term.successors() {
            let s = s as usize;
            if s < nb && !preds[s].contains(&i) {
                preds[s].push(i);
            }
        }
    }

    // What has to be appended to which block. Collected first and applied
    // afterwards, so that the walk reads a function nobody is changing.
    let mut plan: Vec<(usize, Vec<Par>)> = Vec::new();
    for bi in 0..nb {
        let np = f.blocks[bi].phi_count();
        if np == 0 {
            continue;
        }
        for &p in &preds[bi] {
            let mut par: Vec<Par> = Vec::new();
            for i in f.blocks[bi].insts[..np].iter() {
                let d = match i.dst {
                    Some(d) => d,
                    None => continue,
                };
                let inc = match &i.op {
                    Op::Phi { incoming } => incoming,
                    _ => continue,
                };
                match inc.iter().find(|(q, _)| *q as usize == p) {
                    Some((_, v)) => par.push(Par { dst: d, ty: i.ty, src: *v, loc: i.loc }),
                    None => {
                        return Err(format!(
                            "internal error: @{} bb{}: the phi %{} has no entry for its \
                             predecessor bb{}",
                            f.name, bi, d, p
                        ))
                    }
                }
            }
            if !par.is_empty() {
                plan.push((p, par));
            }
        }
    }

    // How often is every value read? A value this phi is the ONLY reader of
    // may have its definition rewritten to produce the phi's value straight
    // away (see the header).
    let mut uses: std::collections::HashMap<Val, usize> = std::collections::HashMap::new();
    let mut buf = Vec::new();
    for b in f.blocks.iter() {
        for i in b.insts.iter() {
            buf.clear();
            i.op.uses(&mut buf);
            for v in buf.iter() {
                *uses.entry(*v).or_insert(0) += 1;
            }
        }
        let t = match &b.term {
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            Term::Ret(Some(v)) => Some(*v),
            _ => None,
        };
        if let Some(v) = t {
            *uses.entry(v).or_insert(0) += 1;
        }
    }

    // Values a phi defines. One of those must never be folded INTO: it is
    // written by every predecessor of its own block, and rewriting the one
    // definition that happens to sit in this block would leave the others
    // writing a value nobody reads.
    let mut phi_dsts: std::collections::HashSet<Val> = std::collections::HashSet::new();
    for b in f.blocks.iter() {
        for i in b.insts.iter() {
            if let (Some(d), Op::Phi { .. }) = (i.dst, &i.op) {
                phi_dsts.insert(d);
            }
        }
    }

    for (p, par) in plan {
        let par = fold_into_definitions(f, p, par, &uses, &phi_dsts);
        let seq = sequentialize(f, par);
        f.blocks[p].insts.extend(seq);
    }
    // and the phis themselves go
    for b in f.blocks.iter_mut() {
        b.insts.retain(|i| !matches!(i.op, Op::Phi { .. }));
    }
    Ok(())
}

/// **ROUND 92 — the chains of phis, and why they have to go.**
///
/// A nested `if`/`else` does not make ONE join, it makes a cascade of them,
/// and SSA construction puts a phi in every one. `bench/firn/statemachine.fi`
/// has a six-deep tree over four variables, and its inner loop came out of
/// the first version of this round like this:
///
/// ```text
/// .Lmain__bb13:  lea r9, [rdi+1]     <- text = text + 1
///                mov rbx, r8         <- state, unchanged, copied
/// .Lmain__bb14:  mov r15, rbx        <- and copied again
///                mov r13, rdx
///                mov r12, r9
/// .Lmain__bb11:  lea rsi, [rsi+1]    <- k = k + 1
///                mov r8, r15         <- and a third time
///                mov rdx, r13
///                mov rdi, r12
/// ```
///
/// Six register-to-register moves per octet of input that only shuffle three
/// variables through three join blocks. Counted with callgrind, that was
/// **+43 % executed instructions** against round 91 on that program. A
/// foundation that makes branch-heavy code half as slow again is not a
/// foundation.
///
/// The values in such a chain never coexist: `%X` in `bb14` is read by
/// exactly one thing, the phi in `bb11`, and is dead the moment it is read.
/// So they can share ONE name, every copy between them becomes `x = x` and
/// disappears. That is register coalescing, done on the phi graph where it
/// needs no interference graph — the congruence-class idea, cut down to the
/// shape a chain of joins actually has.
///
/// `%X` (a phi in `B`) is merged into `%Y` (a phi in `C`) when ALL of:
///
///  * `B` ends in `br C`, so everything leaving `B` arrives at `C`;
///  * every predecessor of `B` ends in `br B` — this is the critical-edge
///    condition. Without it, writing `%Y` at the end of a predecessor would
///    also write it on a path that never goes to `C`, where the old content
///    may still be wanted;
///  * `%X` has exactly ONE reader in the whole function, that entry. A phi
///    entry naming the phi's own value does not count as a reader — that is
///    what a collapsed chain leaves behind;
///  * no ORDINARY instruction of `B` reads `%Y` — it would read the new
///    content where it wanted the old. A phi of `B` reading `%Y` is fine and
///    is the case this exists for: after the merge that entry is either the
///    phi's own value (no copy at all) or one copy among several on the same
///    edge, and `sequentialize` orders a parallel copy so that every source
///    is read before it is overwritten;
///  * neither value is `secret` (SPEC §9.2).
///
/// Merges are applied in rounds, and inside one round no merge whose target
/// is another merge's source is taken: the conditions were checked against
/// the graph as it stands, and chaining two of them at once would use one of
/// them out of date. A chain of three joins therefore collapses in three
/// rounds; the loop is bounded at sixteen.
fn coalesce_chains(f: &mut Func) {
    let mut buf: Vec<Val> = Vec::new();
    for _round in 0..16 {
        let nb = f.blocks.len();
        // Uses per value. A phi entry naming the phi's own value is not one.
        let mut uses: std::collections::HashMap<Val, usize> = std::collections::HashMap::new();
        for b in f.blocks.iter() {
            for i in b.insts.iter() {
                if let (Some(d), Op::Phi { incoming }) = (i.dst, &i.op) {
                    for (_, v) in incoming.iter() {
                        if *v != d {
                            *uses.entry(*v).or_insert(0) += 1;
                        }
                    }
                    continue;
                }
                buf.clear();
                i.op.uses(&mut buf);
                for v in buf.iter() {
                    *uses.entry(*v).or_insert(0) += 1;
                }
            }
            let t = match &b.term {
                Term::BrCond { cond, .. } => Some(*cond),
                Term::Switch { val, .. } => Some(*val),
                Term::Ret(Some(v)) => Some(*v),
                _ => None,
            };
            if let Some(v) = t {
                *uses.entry(v).or_insert(0) += 1;
            }
        }
        let mut phi_block: std::collections::HashMap<Val, usize> = std::collections::HashMap::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            let np = b.phi_count();
            for i in b.insts[..np].iter() {
                if let Some(d) = i.dst {
                    phi_block.insert(d, bi);
                }
            }
        }
        let mut preds: Vec<Vec<usize>> = vec![Vec::new(); nb];
        for (i, b) in f.blocks.iter().enumerate() {
            for sb in b.term.successors() {
                let sb = sb as usize;
                if sb < nb && !preds[sb].contains(&i) {
                    preds[sb].push(i);
                }
            }
        }

        let mut pairs: Vec<(Val, Val)> = Vec::new();
        for (ci, c) in f.blocks.iter().enumerate() {
            let np = c.phi_count();
            for i in c.insts[..np].iter() {
                let y = match i.dst {
                    Some(d) => d,
                    None => continue,
                };
                let inc = match &i.op {
                    Op::Phi { incoming } => incoming.clone(),
                    _ => continue,
                };
                for (bb, x) in inc.into_iter() {
                    let bi = bb as usize;
                    if x == y || bi >= nb || f.is_secret(x) || f.is_secret(y) {
                        continue;
                    }
                    if phi_block.get(&x) != Some(&bi) {
                        continue; // %X is not a phi of that very block
                    }
                    if uses.get(&x).copied().unwrap_or(0) != 1 {
                        continue;
                    }
                    if !matches!(f.blocks[bi].term, Term::Br(t) if t as usize == ci) {
                        continue;
                    }
                    if !preds[bi]
                        .iter()
                        .all(|&q| matches!(f.blocks[q].term, Term::Br(t) if t as usize == bi))
                    {
                        continue;
                    }
                    // Only ORDINARY instructions of B are asked. A phi of B
                    // that reads %Y is fine and is in fact the case this
                    // whole thing exists for: after the merge that entry is
                    // either the phi's own value (no copy at all) or one
                    // copy among several on the same edge, and
                    // `sequentialize` already knows how to order a parallel
                    // copy so that every source is read before it is
                    // overwritten. An ordinary instruction is different: it
                    // sits INSIDE B, after the copies at the end of B's
                    // predecessors have already run, and would read the new
                    // content where it wanted the old.
                    let mut reads_y = false;
                    for k in f.blocks[bi].insts.iter() {
                        if matches!(k.op, Op::Phi { .. }) {
                            continue;
                        }
                        buf.clear();
                        k.op.uses(&mut buf);
                        if buf.contains(&y) {
                            reads_y = true;
                            break;
                        }
                    }
                    if !reads_y {
                        pairs.push((x, y));
                    }
                }
            }
        }
        if pairs.is_empty() {
            if std::env::var_os("FIRN_PHI_STATS").is_some() && _round == 0 {
                eprintln!("coalesce @{}: no pair at all", f.name);
            }
            return;
        }
        // A GREEDY MATCHING, not a filter. The first version refused every
        // pair whose target was another pair's source -- and in a real chain
        // `x1 -> x2 -> x3` EVERY pair is of that kind, so it refused all of
        // them and coalesced nothing at all (measured: `@main` of
        // `bench/firn/statemachine.fi`, five pairs found, none applied).
        // What is needed is a maximal set of pairs that do not chain WITH
        // EACH OTHER: take a pair unless one of its ends has already been
        // used at the other end. `x1 -> x2` and `x3 -> x4` go in one round,
        // `x2 -> x4` follows in the next, and the chain is gone in two.
        let mut chosen_x: std::collections::HashSet<Val> = std::collections::HashSet::new();
        let mut chosen_y: std::collections::HashSet<Val> = std::collections::HashSet::new();
        let mut map: std::collections::HashMap<Val, Val> = std::collections::HashMap::new();
        for (x, y) in pairs.iter() {
            if chosen_y.contains(x) || chosen_x.contains(y) {
                continue;
            }
            if map.contains_key(x) {
                continue;
            }
            map.insert(*x, *y);
            chosen_x.insert(*x);
            chosen_y.insert(*y);
        }
        if map.is_empty() {
            return;
        }
        if std::env::var_os("FIRN_PHI_STATS").is_some() {
            eprintln!("coalesce @{} round {}: {} pairs, {} applied", f.name, _round, pairs.len(), map.len());
        }
        // %X stands in exactly two places: as the dst of its phi and as the
        // single entry that reads it. Both become %Y.
        for b in f.blocks.iter_mut() {
            for i in b.insts.iter_mut() {
                if let Some(d) = i.dst {
                    if let Some(&ny) = map.get(&d) {
                        i.dst = Some(ny);
                    }
                }
                if let Op::Phi { incoming } = &mut i.op {
                    for e in incoming.iter_mut() {
                        if let Some(&ny) = map.get(&e.1) {
                            e.1 = ny;
                        }
                    }
                }
            }
        }
    }
}

/// May THIS instruction be made to write a value that other instructions
/// also write?
///
/// ROUND 92 -- WHY THIS IS A WHITELIST AND NOT A `!matches!`.
///
/// Folding a copy away means the value gets more than one definition, and
/// everything below the optimizer was written when FIR was SSA and one
/// definition was all there could be. Two places drew a conclusion from "the
/// single definition of this value is a `const`" and had to be taught to
/// count first (`regalloc.rs::immediate_consts`, `codegen_a64.rs::layout`;
/// `tests/018_while_sum.fi` returned 0 instead of 55 until they were).
///
/// The one that cannot be taught that cheaply is the CELL ALIAS: a `load`
/// out of an `alloca` that `regalloc.rs` keeps in a register does not read
/// memory at all, it reads that register, and it says so by mapping its
/// result value to it. A value written somewhere else as well would then
/// read the cell register at every use.
///
/// So: a list of plain computations, not a list of the dangerous ones. A new
/// `Op` variant is not foldable until somebody has thought about it, and
/// that is the right default.
fn foldable_def(op: &Op) -> bool {
    matches!(
        op,
        Op::Const(_)
            | Op::Bin(..)
            | Op::BinWrapSat { .. }
            | Op::CheckedBin { .. }
            | Op::CheckedDiv { .. }
            | Op::CheckedCast { .. }
            | Op::CheckedIdx { .. }
            | Op::Cmp { .. }
            | Op::Un(..)
            | Op::Cast { .. }
            | Op::PtrAdd { .. }
            | Op::Call { .. }
            | Op::CallIndirect { .. }
            | Op::Syscall { .. }
            | Op::VtabAddr { .. }
            | Op::FnRef { .. }
            | Op::GlobalAddr { .. }
    )
}

/// Lets the instruction that computes a phi's value write that value itself
/// wherever it is safe (see the header). Yields the copies that are left.
fn fold_into_definitions(
    f: &mut Func,
    p: usize,
    par: Vec<Par>,
    uses: &std::collections::HashMap<Val, usize>,
    phi_dsts: &std::collections::HashSet<Val>,
) -> Vec<Par> {
    // Only when the predecessor goes exactly one way -- and `Term::Br` is
    // asked for by name rather than "one successor", because a `switch` with
    // a single target still READS its selector in the terminator, and the
    // check below deliberately does not look at terminators.
    if !matches!(f.blocks[p].term, Term::Br(_)) {
        return par;
    }
    // ROUND 92 -- THE SOURCES OF THE WHOLE EDGE, TAKEN BEFORE ANYTHING MOVES.
    //
    // The copies of one edge happen SIMULTANEOUSLY. Folding one of them
    // moves its write EARLIER, to the middle of the block -- and if any
    // OTHER copy of the same edge reads that value, that other copy would
    // then read the new content instead of the old one.
    //
    // The first version of this function only looked at the copies it had
    // already decided about, not at the ones still to come. `sqrt(2.0)` came
    // out as `1.5`: `lib/std/core.fi::sqrt` runs Newton until `g != old`
    // stops being true, the back edge is the parallel copy
    // `{ g <- gnew, old <- g }`, and folding `g <- gnew` first made `old`
    // read the NEW `g`. Both were equal after one pass, the loop ended, and
    // 1.5 is exactly the first Newton step for 2.
    let srcs: Vec<Val> = par.iter().map(|c| c.src).collect();
    let mut out: Vec<Par> = Vec::new();
    let mut buf: Vec<Val> = Vec::new();
    for (own, c) in par.into_iter().enumerate() {
        // read by this phi entry and by nothing else
        if uses.get(&c.src).copied().unwrap_or(0) != 1
            || c.src == c.dst
            || phi_dsts.contains(&c.src)
            || f.is_secret(c.src)
            || f.is_secret(c.dst)
        {
            out.push(c);
            continue;
        }
        // ... and computed right here
        let j = match f.blocks[p].insts.iter().position(|i| i.dst == Some(c.src)) {
            Some(j) => j,
            None => {
                out.push(c);
                continue;
            }
        };
        let bad = {
            let i = &f.blocks[p].insts[j];
            i.ty != c.ty || !foldable_def(&i.op)
        };
        if bad {
            out.push(c);
            continue;
        }
        // the phi's own value must not be read after that point ...
        let mut blocked = false;
        for i in f.blocks[p].insts[j + 1..].iter() {
            buf.clear();
            i.op.uses(&mut buf);
            if buf.contains(&c.dst) {
                blocked = true;
                break;
            }
        }
        // ... nor by ANY other copy of this same edge, whether it has been
        // decided already or is still to come. The terminator needs no
        // check: the block ends in `br`, and `br` reads nothing.
        if !blocked && srcs.iter().enumerate().any(|(k, s)| k != own && *s == c.dst) {
            blocked = true;
        }
        if blocked {
            out.push(c);
            continue;
        }
        f.blocks[p].insts[j].dst = Some(c.dst);
    }
    out
}

/// Turns ONE parallel copy into a sequence of `Op::Copy` instructions that
/// has the same effect. See the header for why this is not simply a loop.
fn sequentialize(f: &mut Func, mut par: Vec<Par>) -> Vec<Inst> {
    let mut out: Vec<Inst> = Vec::new();
    // 1. `x = x` does nothing.
    par.retain(|c| c.dst != c.src);
    let mut guard = 0usize;
    while !par.is_empty() {
        guard += 1;
        if guard > 4 * par.len() + 64 {
            break; // cannot happen: every round either emits or breaks a cycle
        }
        // 2. a target nobody else still reads
        let free = par.iter().position(|c| !par.iter().any(|o| o.src == c.dst));
        match free {
            Some(k) => {
                let c = par.remove(k);
                out.push(Inst::like(Some(c.dst), c.ty, Op::Copy { src: c.src }, c.loc));
            }
            None => {
                // 3. everything left is a cycle: rescue one target
                let ty = par[0].ty;
                let victim = par[0].dst;
                let tmp = f.new_val_pub(ty);
                let vloc = par[0].loc;
                out.push(Inst::like(Some(tmp), ty, Op::Copy { src: victim }, vloc));
                for c in par.iter_mut() {
                    if c.src == victim {
                        c.src = tmp;
                    }
                }
            }
        }
    }
    out
}

/// The block numbers a terminator names, as mutable references — used by the
/// tests below and by anybody who needs to rewrite edges.
#[allow(dead_code)]
pub(crate) fn term_targets(t: &mut Term) -> Vec<&mut BlockId> {
    match t {
        Term::Br(b) => vec![b],
        Term::BrCond { then_bb, else_bb, .. } => vec![then_bb, else_bb],
        Term::Switch { cases, default, .. } => {
            let mut v: Vec<&mut BlockId> = cases.iter_mut().map(|(_, b)| b).collect();
            v.push(default);
            v
        }
        Term::Ret(_) | Term::Unset => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{BinOp, CmpOp, FTy};

    /// The swap: two phis on one edge that read each other. Emitted naively
    /// both values end up the same; this is the test that says they do not.
    #[test]
    fn swapping_phis_do_not_collapse() {
        let mut f = Func::new("swap", vec![FTy::I64, FTy::I64], FTy::I64);
        let head = f.add_block();
        let body = f.add_block();
        let done = f.add_block();
        f.set_term(0, Term::Br(head));
        // bb1: %a = phi [bb0 %0, bb2 %b] ; %b = phi [bb0 %1, bb2 %a]
        let a = f.new_val_pub(FTy::I64);
        let b = f.new_val_pub(FTy::I64);
        f.blocks[head as usize].insts.push(Inst {
            dst: Some(a),
            ty: FTy::I64,
            op: Op::Phi { incoming: vec![(0, 0), (body, b)] },
            loc: crate::fir::Loc::NONE,
        });
        f.blocks[head as usize].insts.push(Inst {
            dst: Some(b),
            ty: FTy::I64,
            op: Op::Phi { incoming: vec![(0, 1), (body, a)] },
            loc: crate::fir::Loc::NONE,
        });
        let c = f.push(head, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I64, a, b });
        f.set_term(head, Term::BrCond { cond: c, then_bb: body, else_bb: done });
        f.set_term(body, Term::Br(head));
        f.set_term(done, Term::Ret(Some(a)));
        assert!(f.verify_phis().is_ok(), "{:?}", f.verify_phis());

        eliminate_func(&mut f).unwrap();
        assert!(!f.has_phi());
        // The back edge block now holds three copies, not two: one of them
        // is the rescue that breaks the cycle open.
        let copies: Vec<(Val, Val)> = f.blocks[body as usize]
            .insts
            .iter()
            .filter_map(|i| match (&i.op, i.dst) {
                (Op::Copy { src }, Some(d)) => Some((d, *src)),
                _ => None,
            })
            .collect();
        assert_eq!(copies.len(), 3, "a two-cycle needs one rescue: {:?}", copies);
        // Play the sequence through: a and b really do swap.
        let mut env: std::collections::HashMap<Val, i64> =
            std::collections::HashMap::new();
        env.insert(a, 11);
        env.insert(b, 22);
        for (d, s) in &copies {
            let v = *env.get(s).unwrap_or(&0);
            env.insert(*d, v);
        }
        assert_eq!(env[&a], 22);
        assert_eq!(env[&b], 11);
    }

    /// A chain (not a cycle) needs no rescue, and the order matters: the
    /// target that nobody reads any more has to be written first.
    #[test]
    fn chained_copies_keep_their_order() {
        let mut f = Func::new("chain", vec![FTy::I64, FTy::I64], FTy::I64);
        let head = f.add_block();
        let body = f.add_block();
        let done = f.add_block();
        f.set_term(0, Term::Br(head));
        let x = f.new_val_pub(FTy::I64);
        let y = f.new_val_pub(FTy::I64);
        // %x = phi [bb0 %0, bb2 %y] ; %y = phi [bb0 %1, bb2 %0]
        // On the back edge that is the parallel copy { x <- y, y <- %0 }.
        // `x` is nobody's source, so it has to be written FIRST; the other
        // order would give x the new y.
        f.blocks[head as usize].insts.push(Inst {
            dst: Some(x),
            ty: FTy::I64,
            op: Op::Phi { incoming: vec![(0, 0), (body, y)] },
            loc: crate::fir::Loc::NONE,
        });
        f.blocks[head as usize].insts.push(Inst {
            dst: Some(y),
            ty: FTy::I64,
            op: Op::Phi { incoming: vec![(0, 1), (body, 0)] },
            loc: crate::fir::Loc::NONE,
        });
        let c = f.push(head, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I64, a: x, b: y });
        f.set_term(head, Term::BrCond { cond: c, then_bb: body, else_bb: done });
        f.set_term(body, Term::Br(head));
        f.set_term(done, Term::Ret(Some(x)));
        eliminate_func(&mut f).unwrap();
        let copies: Vec<(Val, Val)> = f.blocks[body as usize]
            .insts
            .iter()
            .filter_map(|i| match (&i.op, i.dst) {
                (Op::Copy { src }, Some(d)) => Some((d, *src)),
                _ => None,
            })
            .collect();
        assert_eq!(copies.len(), 2);
        assert_eq!(copies[0], (x, y));
        assert_eq!(copies[1], (y, 0));
    }

    /// ROUND 92 REGRESSION -- `sqrt(2.0)` came out as `1.5`.
    ///
    /// `lib/std/core.fi::sqrt` runs Newton until the value stops moving:
    /// `while g != old { old = g; g = (g + x/g)/2 }`. The back edge is the
    /// parallel copy `{ g <- gnew, old <- g }`. Folding `g <- gnew` into the
    /// instruction that computes `gnew` writes `g` in the MIDDLE of the
    /// latch -- and the second copy reads `g`. It then read the new one,
    /// `old == g` held after a single pass, and 1.5 is exactly the first
    /// Newton step for 2.
    ///
    /// What this test asks: the fold does not happen, and the two copies
    /// come out in the order that keeps the old value readable.
    #[test]
    fn a_copy_that_another_copy_reads_is_not_folded_away() {
        let mut f = Func::new("newton", vec![FTy::I64, FTy::I64], FTy::I64);
        let head = f.add_block();
        let latch = f.add_block();
        let done = f.add_block();
        f.set_term(0, Term::Br(head));
        let g = f.new_val_pub(FTy::I64);
        let old = f.new_val_pub(FTy::I64);
        let gnew = f.new_val_pub(FTy::I64);
        f.blocks[head as usize].insts.push(Inst {
            dst: Some(g),
            ty: FTy::I64,
            op: Op::Phi { incoming: vec![(0, 0), (latch, gnew)] },
            loc: crate::fir::Loc::NONE,
        });
        f.blocks[head as usize].insts.push(Inst {
            dst: Some(old),
            ty: FTy::I64,
            op: Op::Phi { incoming: vec![(0, 1), (latch, g)] },
            loc: crate::fir::Loc::NONE,
        });
        let c = f.push(head, FTy::Bool, Op::Cmp { op: CmpOp::Ne, ty: FTy::I64, a: g, b: old });
        f.set_term(head, Term::BrCond { cond: c, then_bb: latch, else_bb: done });
        f.blocks[latch as usize]
            .insts
            .push(Inst::new(Some(gnew), FTy::I64, Op::Bin(BinOp::Add, g, g)));
        f.set_term(latch, Term::Br(head));
        f.set_term(done, Term::Ret(Some(g)));
        assert!(f.verify_phis().is_ok(), "{:?}", f.verify_phis());

        eliminate_func(&mut f).unwrap();
        let insts: Vec<(Option<Val>, String)> = f.blocks[latch as usize]
            .insts
            .iter()
            .map(|i| {
                (
                    i.dst,
                    match &i.op {
                        Op::Copy { src } => format!("copy {}", src),
                        Op::Bin(..) => "add".to_string(),
                        _ => "other".to_string(),
                    },
                )
            })
            .collect();
        // `gnew` still gets its own instruction -- the fold is refused.
        assert_eq!(insts[0], (Some(gnew), "add".to_string()), "{:?}", insts);
        // `old <- g` HAS to come before `g <- gnew`.
        assert_eq!(insts[1], (Some(old), format!("copy {}", g)), "{:?}", insts);
        assert_eq!(insts[2], (Some(g), format!("copy {}", gnew)), "{:?}", insts);
    }

    /// The other side of the same coin: when nothing else reads the value,
    /// the copy really does disappear into the instruction that computes it.
    #[test]
    fn a_copy_nobody_else_reads_is_folded_into_its_definition() {
        let mut f = Func::new("count", vec![FTy::I64], FTy::I64);
        let head = f.add_block();
        let latch = f.add_block();
        let done = f.add_block();
        f.set_term(0, Term::Br(head));
        let i = f.new_val_pub(FTy::I64);
        let inext = f.new_val_pub(FTy::I64);
        f.blocks[head as usize].insts.push(Inst {
            dst: Some(i),
            ty: FTy::I64,
            op: Op::Phi { incoming: vec![(0, 0), (latch, inext)] },
            loc: crate::fir::Loc::NONE,
        });
        let c = f.push(head, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I64, a: i, b: 0 });
        f.set_term(head, Term::BrCond { cond: c, then_bb: latch, else_bb: done });
        let one = f.push(latch, FTy::I64, Op::Const(1));
        f.blocks[latch as usize]
            .insts
            .push(Inst::new(Some(inext), FTy::I64, Op::Bin(BinOp::Add, i, one)));
        f.set_term(latch, Term::Br(head));
        f.set_term(done, Term::Ret(Some(i)));
        eliminate_func(&mut f).unwrap();
        assert!(
            f.blocks[latch as usize].insts.iter().all(|x| !matches!(x.op, Op::Copy { .. })),
            "a copy is left over"
        );
        // The `add` writes the loop variable itself now.
        assert!(f.blocks[latch as usize]
            .insts
            .iter()
            .any(|x| x.dst == Some(i) && matches!(x.op, Op::Bin(BinOp::Add, ..))));
    }

    /// A loop counter, end to end: `mem2reg` promotes it, `phi.rs` takes the
    /// phi apart again, and nothing is left standing in memory.
    #[test]
    fn a_loop_counter_ends_up_without_an_alloca() {
        // fn f(n: i64) -> i64 { var i = 0; while i < n { i = i + 1 } return i }
        let mut f = Func::new("count", vec![FTy::I64], FTy::I64);
        let slot = f.alloca(8, 8);
        let z = f.push(0, FTy::I64, Op::Const(0));
        f.push_void(0, FTy::I64, Op::Store { addr: slot, val: z });
        let head = f.add_block();
        let body = f.add_block();
        let done = f.add_block();
        f.set_term(0, Term::Br(head));
        let l1 = f.push(head, FTy::I64, Op::Load { addr: slot });
        let c = f.push(head, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I64, a: l1, b: 0 });
        f.set_term(head, Term::BrCond { cond: c, then_bb: body, else_bb: done });
        let l2 = f.push(body, FTy::I64, Op::Load { addr: slot });
        let one = f.push(body, FTy::I64, Op::Const(1));
        let s = f.push(body, FTy::I64, Op::Bin(BinOp::Add, l2, one));
        f.push_void(body, FTy::I64, Op::Store { addr: slot, val: s });
        f.set_term(body, Term::Br(head));
        let l3 = f.push(done, FTy::I64, Op::Load { addr: slot });
        f.set_term(done, Term::Ret(Some(l3)));

        assert!(crate::mem2reg::promote_allocas(&mut f) >= 3);
        assert!(f.verify_phis().is_ok(), "{:?}", f.verify_phis());
        // No alloca, no load, no store left anywhere.
        for b in &f.blocks {
            for i in &b.insts {
                assert!(
                    !matches!(i.op, Op::Alloca { .. } | Op::Load { .. } | Op::Store { .. }),
                    "{} still touches memory",
                    crate::fir::Module { funcs: vec![f.clone()] }.to_text()
                );
            }
        }
        assert!(f.has_phi());
        eliminate_func(&mut f).unwrap();
        assert!(!f.has_phi());
    }
}
