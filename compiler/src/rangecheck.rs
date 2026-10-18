// SPDX-License-Identifier: MPL-2.0
//! **ROUND SPEED** — the checks that can never fire.
//!
//! INTERFACE (fixed):
//!   `pub(crate) fn remove_provable_arith_checks(f: &mut Func) -> usize`
//!
//! ## Why this pass
//!
//! `docs/ROUND90.md` §4 closed with the sentence this file answers:
//!
//! > The reason is that **LLVM proves most of its checks away and Firn
//! > proves none of them away**. `i + 1` inside `while i < 240` is still a
//! > full checked addition in Firn, and it cannot be anything else until
//! > Firn has a range analysis.
//!
//! Measured before this pass (`python3 tools/bench90/icount.py`, executed
//! instructions, deterministic):
//!
//! | | `release-fast` | `release-safe` | the checks cost |
//! |---|---:|---:|---:|
//! | matmul | 501,828,373 | 1,917,415,771 | **3.82x** |
//! | bubblesort | 306,632,250 | 821,296,445 | **2.68x** |
//!
//! Round 90 made the check itself as cheap as x86 allows — one instruction
//! and one not-taken forward branch. What is left is that there are so many
//! of them, and most of them are decided at compile time.
//!
//! ## What it proves
//!
//! An interval per value, in the MATHEMATICAL value (`i128`), from three
//! sources:
//!
//!  * **structure** — a constant is its own interval; `x % k` with `k`
//!    constant lies in `[0, k-1]`; `x & k` lies in `[0, k]`; an unsigned
//!    `x >> k` lies in `[0, max >> k]`; a checked index lies in
//!    `[0, len-1]`; sums, differences and products of values that have
//!    intervals have the obvious interval — but **only if it fits into the
//!    type**, because `Op::Bin` wraps and a wrapped value is not the sum.
//!  * **the branches on the way in** — `known_facts`-style: walk up the
//!    single-predecessor chain and collect the conditions that must hold.
//!    `while i < 240` gives `i` the interval `[0, 239]` inside the body.
//!    This is the source that matters: the loop counter is a `phi`, and a
//!    phi without a fixpoint has no interval of its own.
//!  * **the type** — everything else is the full range of its type, which
//!    is always sound and proves nothing.
//!
//! A checked operation whose operands' intervals cannot leave the type is
//! replaced by the plain one. Values do not change and no reachable panic
//! disappears, so the pass is **debug preserving** and runs at `dev-fast`
//! as well — which is the level the everyday build uses.
//!
//!  * **phis (r14)** -- the union of the operands, each read at the end of
//!    its predecessor with that block's facts plus the branch of the edge
//!    (`if m > 16 { m = 16 }` gives `m` the interval `[0, 16]`). A guard on an
//!    offset (`j + 1 < n`, the add checked or provably not wrapping) bounds
//!    the base (`j < n - 1`).
//!
//!  * **run-time lengths (r755)** -- `CheckedIdxV` (`v[i]` on a `Vec`/`str`)
//!    compares against a LOADED length. The proof is symbolic (`less`): a
//!    branch `i < len` / `i + k < len`, a phi whose operands are all below
//!    `len` on their own edge, a constant under a guarded `0 < len`. Two
//!    loads of the same word are one value only if no instruction on any
//!    path between them can write memory (`same_len`/`clean_between`: a loop
//!    that stores anywhere keeps its checks, `v.push` or `v[i] = x` alike).
//!
//! ## What it deliberately does not do
//!
//! No fixpoint over phis (a loop counter's interval would come out of the
//! back edge; the loop guard says the same thing more cheaply and without
//! widening; a loop-carried operand reads the phi again, the depth runs out
//! and the union is the whole type), no interval per program point (one interval per value at the
//! block that reads it), no reasoning across calls.
//!
//! ## Soundness
//!
//! Every step that could leave `i128` (`+`, `-`, `*` on two intervals) goes
//! through `checked_*` and gives up instead of wrapping. Every narrowing is
//! an intersection with what was already known, so an interval can only get
//! smaller than the truth if one of the sources lies — and the three
//! sources are: a constant (exact), a dominating comparison (holds by
//! construction of the control flow), and the type (holds by definition).

use crate::fir::{BinOp, CmpOp, FTy, Func, Op, Term, Val};
use std::collections::HashMap;

/// Inclusive interval of the mathematical value.
type Range = (i128, i128);

/// The full range of an integer type — `None` for everything this pass does
/// not reason about (floating point, `bool`, `ptr`, `void`, vectors).
fn ty_range(t: FTy) -> Option<Range> {
    if t.is_float() {
        return None;
    }
    match t {
        FTy::I8 | FTy::I16 | FTy::I32 | FTy::I64 | FTy::U8 | FTy::U16 | FTy::U32 | FTy::U64 => {
            let b = t.bits();
            Some(if t.signed() {
                (-(1i128 << (b - 1)), (1i128 << (b - 1)) - 1)
            } else {
                (0, (1i128 << b) - 1)
            })
        }
        _ => None,
    }
}

fn fits(r: Range, t: Range) -> bool {
    r.0 >= t.0 && r.1 <= t.1
}

fn meet(a: Range, b: Range) -> Range {
    (a.0.max(b.0), a.1.min(b.1))
}

fn invert(op: CmpOp) -> CmpOp {
    match op {
        CmpOp::Eq => CmpOp::Ne,
        CmpOp::Ne => CmpOp::Eq,
        CmpOp::Lt => CmpOp::Ge,
        CmpOp::Ge => CmpOp::Lt,
        CmpOp::Le => CmpOp::Gt,
        CmpOp::Gt => CmpOp::Le,
    }
}

/// Everything the analysis reads. Built once per function, read only.
struct Ana<'a> {
    f: &'a Func,
    /// where each value is defined
    site: HashMap<Val, (usize, usize)>,
    /// every comparison by the bool value it produces
    cmps: HashMap<Val, (CmpOp, FTy, Val, Val)>,
    /// per block: the conditions that hold on every way in, with their truth
    facts: Vec<Vec<(Val, bool)>>,
    /// predecessors per block (for the memory-clean chain walk)
    preds: Vec<Vec<usize>>,
}

/// The conditions that must hold in a block.
///
/// NOT the walk up the chain of single predecessors that `opt.rs`
/// (`known_facts`) uses. That walk stops at the first join -- and a loop
/// HEAD is a join, so everything a loop guard says is lost one loop deeper.
/// `bench/firn/matmul.fi`, `release-safe`:
///
/// ```text
/// bb1: %27 = cmp.lt.u64 %148, %1 ; brcond %27, bb2, bb3
/// bb4: %32 = cmp.lt.u64 %149, %1 ; brcond %32, bb5, bb6    <- two predecessors
/// bb5: %36 = checked_mul.u64 %148, %1                      <- needs %27
/// ```
///
/// The chain from `bb5` reaches `bb4`, finds two predecessors and gives up,
/// so `i < 240` was invisible exactly where `i * 240` stands.
///
/// The rule used here is the dominance one: a branch in `p` with the
/// successor `s` proves its condition in every block `s` DOMINATES --
/// provided `s` has no other predecessor, because otherwise being in `s`
/// does not mean the branch went that way. That is sound (control must
/// have passed through `s`), it survives joins, and it costs one dominator
/// matrix, which `licm` computes for every function anyway.
fn facts_per_block(f: &Func, preds: &[Vec<usize>]) -> Vec<Vec<(Val, bool)>> {
    let n = f.blocks.len();
    let dom = crate::mem2reg::dominators(f);
    let mut out: Vec<Vec<(Val, bool)>> = vec![Vec::new(); n];
    for p in 0..n {
        let (cond, th, el) = match f.blocks[p].term {
            Term::BrCond { cond, then_bb, else_bb } => (cond, then_bb as usize, else_bb as usize),
            _ => continue,
        };
        if th >= n || el >= n || th == el {
            continue;
        }
        for (succ, truth) in [(th, true), (el, false)] {
            if succ == p || preds[succ].len() != 1 {
                continue;
            }
            for b in 0..n {
                if dom[b][succ] && !out[b].iter().any(|(c, _)| *c == cond) {
                    out[b].push((cond, truth));
                }
            }
        }
    }
    out
}

impl<'a> Ana<'a> {
    fn new(f: &'a Func) -> Ana<'a> {
        let mut site = HashMap::new();
        let mut cmps = HashMap::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, i) in b.insts.iter().enumerate() {
                if let Some(d) = i.dst {
                    site.insert(d, (bi, ii));
                    if let Op::Cmp { op, ty, a, b: rhs } = &i.op {
                        cmps.insert(d, (*op, *ty, *a, *rhs));
                    }
                }
            }
        }
        let preds = crate::mem2reg::preds(f);
        Ana { f, site, cmps, facts: facts_per_block(f, &preds), preds }
    }

    fn op_of(&self, v: Val) -> Option<&'a Op> {
        let (b, i) = self.site.get(&v)?;
        Some(&self.f.blocks[*b].insts[*i].op)
    }

    /// The interval of `v` as it is known in block `at`.
    ///
    /// `depth` bounds the recursion; `0` means "the type and the facts, but
    /// do not look at how the value was computed".
    fn range(&self, v: Val, at: usize, depth: u32) -> Option<Range> {
        self.range_x(v, at, depth, true, None)
    }

    /// `use_facts = false` is what makes this terminate. A fact says
    /// `v < other`, and finding out how big `other` is must not walk back
    /// into the facts -- `i < n` and `n > i` are the same fact and would
    /// ask each other forever. Below a fact only the STRUCTURE of the other
    /// side counts (a constant, a remainder, a checked index), and the
    /// recursion is bounded by `depth` alone.
    ///
    /// `ex` is one more fact that holds on the way into `at`: the branch of
    /// the edge a phi operand arrives on (see `phi_range`).
    fn range_x(
        &self,
        v: Val,
        at: usize,
        depth: u32,
        use_facts: bool,
        ex: Option<(Val, bool)>,
    ) -> Option<Range> {
        let t = self.f.val_ty(v);
        let full = ty_range(t)?;
        let mut r = full;
        if depth > 0 {
            if let Some(s) = self.structural(v, at, depth - 1, full, use_facts, ex) {
                r = meet(r, s);
            }
        }
        // Without `use_facts` only the branch of the phi edge itself counts
        // (it is one comparison, and the other side of it is read by
        // structure alone), the facts of the block do not.
        if use_facts || ex.is_some() {
            if let Some(s) = self.from_facts(v, at, t, depth, ex, use_facts) {
                r = meet(r, s);
            }
        }
        if r.0 > r.1 {
            // Contradiction — the block is unreachable. Say nothing.
            return Some(full);
        }
        Some(r)
    }

    /// What the defining instruction says about the value.
    fn structural(
        &self,
        v: Val,
        at: usize,
        depth: u32,
        full: Range,
        use_facts: bool,
        ex: Option<(Val, bool)>,
    ) -> Option<Range> {
        let op = self.op_of(v)?;
        let t = self.f.val_ty(v);
        let signed = t.signed();
        let cst = |x: Val| -> Option<i128> {
            match self.op_of(x) {
                Some(Op::Const(c)) => Some(*c),
                _ => None,
            }
        };
        match op {
            Op::Const(c) => Some((*c, *c)),
            // `x % k` and `x & k` bound the result without knowing `x`.
            Op::Bin(BinOp::Rem, _, b) | Op::CheckedDiv { op: BinOp::Rem, b, .. } if !signed => {
                let k = cst(*b)?;
                if k > 0 {
                    Some((0, k - 1))
                } else {
                    None
                }
            }
            Op::Bin(BinOp::And, a, b) if !signed => {
                let k = cst(*b).or_else(|| cst(*a))?;
                if k >= 0 {
                    Some((0, k))
                } else {
                    None
                }
            }
            Op::Bin(BinOp::Shr, _, b) if !signed => {
                let k = cst(*b)?;
                if k >= 0 && k < t.bits() as i128 {
                    Some((0, full.1 >> k))
                } else {
                    None
                }
            }
            // ROUND 89's checked index hands back an index below the length.
            Op::CheckedIdx { len, .. } => Some((0, (*len as i128) - 1)),
            Op::Bin(o @ (BinOp::Add | BinOp::Sub | BinOp::Mul), a, b)
            | Op::CheckedBin { op: o @ (BinOp::Add | BinOp::Sub | BinOp::Mul), a, b, .. } => {
                let ra = self.range_x(*a, at, depth, use_facts, ex)?;
                let rb = self.range_x(*b, at, depth, use_facts, ex)?;
                let r = combine(*o, ra, rb)?;
                // `Op::Bin` WRAPS. Only an interval that cannot leave the
                // type describes the value that really comes out.
                if fits(r, full) {
                    Some(r)
                } else {
                    None
                }
            }
            // A conversion is the identity exactly when the value fits into
            // the target type; the interval then travels unchanged.
            Op::Cast { src, .. } | Op::CheckedCast { src, .. } => {
                let rs = self.range_x(*src, at, depth, use_facts, ex)?;
                if fits(rs, full) {
                    Some(rs)
                } else {
                    None
                }
            }
            Op::Phi { incoming } => self.phi_range(v, incoming, depth, use_facts),
            _ => None,
        }
    }

    /// The interval of a phi: the union of what each operand is on the edge
    /// it arrives on. The operand is read at the END of its predecessor,
    /// with the facts of that block plus the branch of the edge itself
    /// (`if m > 16 { m = 16 }`: the other operand arrives under `m <= 16`).
    ///
    /// No fixpoint. A loop-carried operand asks for the phi again, the depth
    /// runs out and that read says "the whole type" -- which makes the union
    /// the whole type and proves nothing, so it is sound. What does work is
    /// an operand the loop guard bounds (`i + 1` under `i < 240`) and every
    /// acyclic join of bounded values.
    fn phi_range(&self, v: Val, incoming: &[(u32, Val)], depth: u32, use_facts: bool) -> Option<Range> {
        let (home, _) = *self.site.get(&v)?;
        let n = self.f.blocks.len();
        if incoming.is_empty() || incoming.len() > 4 {
            return None;
        }
        let mut acc: Option<Range> = None;
        for &(pb, val) in incoming {
            let pb = pb as usize;
            if pb >= n {
                return None;
            }
            let ex = match self.f.blocks[pb].term {
                Term::BrCond { cond, then_bb, else_bb }
                    if then_bb != else_bb && (then_bb as usize == home) != (else_bb as usize == home) =>
                {
                    Some((cond, then_bb as usize == home))
                }
                _ => None,
            };
            let _ = use_facts;
            let r = self.range_x(val, pb, depth, true, ex)?;
            acc = Some(match acc {
                None => r,
                Some(a) => (a.0.min(r.0), a.1.max(r.1)),
            });
        }
        acc
    }

    /// `x` as `v + k` (mathematically), if the definition says so and the
    /// addition cannot wrap: a checked one has already panicked otherwise.
    fn offset_of(&self, x: Val, v: Val, at: usize, depth: u32) -> Option<i128> {
        let cst = |y: Val| -> Option<i128> {
            match self.op_of(y) {
                Some(Op::Const(c)) => Some(*c),
                _ => None,
            }
        };
        let (op, a, b, checked) = match self.op_of(x)? {
            Op::CheckedBin { op: o @ (BinOp::Add | BinOp::Sub), a, b, .. } => (*o, *a, *b, true),
            Op::Bin(o @ (BinOp::Add | BinOp::Sub), a, b) => (*o, *a, *b, false),
            _ => return None,
        };
        let (base, k) = match op {
            BinOp::Add if a == v => (a, cst(b)?),
            BinOp::Add if b == v => (b, cst(a)?),
            BinOp::Sub if a == v => (a, cst(b)?.checked_neg()?),
            _ => return None,
        };
        if !checked {
            // A wrapping addition is `v + k` only if it stays in the type.
            let full = ty_range(self.f.val_ty(x))?;
            let rv = self.range_x(base, at, depth, false, None)?;
            if !fits((rv.0.checked_add(k)?, rv.1.checked_add(k)?), full) {
                return None;
            }
        }
        Some(k)
    }

    // ---------------------------------------------------------------- r755 --
    // The checked index with a run-time length (`CheckedIdxV`): `idx < len`
    // where `len` is a LOADED word. There is no interval for it, the proof is
    // symbolic: a branch `idx < len` (or `idx + k < len`), a phi whose every
    // operand is below `len` on its own edge, a constant below a guarded
    // `0 < len`. Two loads of the same word count as one value only if no
    // instruction that may write memory sits between them on a chain of
    // single predecessors (`same_len`).

    /// No instruction on any path from `from` to `to` (both definition
    /// sites) can write memory, and every path to `to` passes `from`.
    ///
    /// Walks the predecessors of `to` backwards and stops at the block of
    /// `from` (only its tail is read). Blocks in between are read whole, so
    /// a loop body counts: it is clean only if no instruction of it writes.
    /// Reaching the entry without passing `from` means `from` does not
    /// dominate `to`: no answer.
    fn clean_between(&self, from: (usize, usize), to: (usize, usize)) -> bool {
        // phis and copies are register moves, they write no memory
        let writes = |b: usize, lo: usize, hi: usize| -> bool {
            self.f.blocks[b].insts[lo..hi]
                .iter()
                .any(|i| !matches!(i.op, Op::Phi { .. } | Op::Copy { .. }) && crate::opt::writes_memory(&i.op))
        };
        let (fb, fi) = from;
        let (tb, ti) = to;
        if tb == fb {
            return fi < ti && !writes(tb, fi + 1, ti);
        }
        if writes(tb, 0, ti) {
            return false;
        }
        let n = self.f.blocks.len();
        // `tb` is NOT marked: a loop back to it runs its whole body, also
        // the part after `to`.
        let mut seen = vec![false; n];
        let mut work: Vec<usize> = vec![tb];
        let mut visited = 0usize;
        while let Some(cur) = work.pop() {
            if self.preds[cur].is_empty() {
                return false;
            }
            for &p in &self.preds[cur] {
                if p == fb {
                    if !seen[p] {
                        seen[p] = true;
                        if writes(p, fi + 1, self.f.blocks[p].insts.len()) {
                            return false;
                        }
                    }
                    continue;
                }
                if seen[p] {
                    continue;
                }
                seen[p] = true;
                visited += 1;
                if visited > 64 || writes(p, 0, self.f.blocks[p].insts.len()) {
                    return false;
                }
                work.push(p);
            }
        }
        true
    }

    fn addr_eq(&self, p: Val, q: Val) -> bool {
        if p == q {
            return true;
        }
        let cst = |y: Val| -> Option<i128> {
            match self.op_of(y) {
                Some(Op::Const(c)) => Some(*c),
                _ => None,
            }
        };
        match (self.op_of(p), self.op_of(q)) {
            (Some(Op::PtrAdd { base: b1, off: o1 }), Some(Op::PtrAdd { base: b2, off: o2 })) => {
                b1 == b2 && (o1 == o2 || (cst(*o1).is_some() && cst(*o1) == cst(*o2)))
            }
            _ => false,
        }
    }

    /// `a` and `b` are the same word of memory read twice, with nothing
    /// that could write memory in between (or they are one value).
    fn same_len(&self, a: Val, b: Val) -> bool {
        if a == b {
            return true;
        }
        let (pa, pb) = match (self.op_of(a), self.op_of(b)) {
            (Some(Op::Load { addr: x }), Some(Op::Load { addr: y })) => (*x, *y),
            _ => return false,
        };
        if self.f.val_ty(a) != self.f.val_ty(b) || !self.addr_eq(pa, pb) {
            return false;
        }
        let (sa, sb) = match (self.site.get(&a), self.site.get(&b)) {
            (Some(x), Some(y)) => (*x, *y),
            _ => return false,
        };
        self.clean_between(sa, sb) || self.clean_between(sb, sa)
    }

    /// `x <= y` for two unsigned values, structurally.
    fn le_val(&self, x: Val, y: Val, at: usize) -> bool {
        if x == y {
            return true;
        }
        let cst = |v: Val| -> Option<i128> {
            match self.op_of(v) {
                Some(Op::Const(c)) => Some(*c),
                _ => None,
            }
        };
        if let (Some(a), Some(b)) = (cst(x), cst(y)) {
            return a <= b;
        }
        matches!(self.offset_of(y, x, at, DEPTH), Some(k) if k >= 0)
    }

    /// `x < l` (unsigned, both `u64`) holds at the END of block `at`
    /// (plus the branch `ex` of the edge it is read on).
    fn less(&self, x: Val, l: Val, at: usize, ex: Option<(Val, bool)>, depth: u32) -> bool {
        let mut facts: Vec<(Val, bool)> = self.facts.get(at).cloned().unwrap_or_default();
        if let Some(e) = ex {
            if !facts.iter().any(|(c, _)| *c == e.0) {
                facts.push(e);
            }
        }
        for (cond, truth) in &facts {
            let (op, cty, a, b) = match self.cmps.get(cond) {
                Some(c) => *c,
                None => continue,
            };
            if cty != FTy::U64 {
                continue;
            }
            let op = if *truth { op } else { invert(op) };
            let (lo, hi, strict) = match op {
                CmpOp::Lt => (a, b, true),
                CmpOp::Gt => (b, a, true),
                _ => continue,
            };
            if strict && self.le_val(x, lo, at) && self.same_len(hi, l) {
                return true;
            }
        }
        if depth == 0 {
            return false;
        }
        // a checked index of the same length is below it by its contract
        if let Some(Op::CheckedIdxV { len, .. }) = self.op_of(x) {
            if self.same_len(*len, l) {
                return true;
            }
        }
        if let Some(Op::Phi { incoming }) = self.op_of(x) {
            let (home, _) = match self.site.get(&x) {
                Some(s) => *s,
                None => return false,
            };
            let n = self.f.blocks.len();
            if incoming.is_empty() || incoming.len() > 4 {
                return false;
            }
            for &(pb, val) in incoming.iter() {
                let pb = pb as usize;
                if pb >= n {
                    return false;
                }
                let ex = match self.f.blocks[pb].term {
                    Term::BrCond { cond, then_bb, else_bb }
                        if then_bb != else_bb && (then_bb as usize == home) != (else_bb as usize == home) =>
                    {
                        Some((cond, then_bb as usize == home))
                    }
                    _ => None,
                };
                if !self.less(val, l, pb, ex, depth - 1) {
                    return false;
                }
            }
            return true;
        }
        false
    }

    /// What the branches on the way into `at` say about the value.
    fn from_facts(
        &self,
        v: Val,
        at: usize,
        t: FTy,
        depth: u32,
        ex: Option<(Val, bool)>,
        use_base: bool,
    ) -> Option<Range> {
        let empty: Vec<(Val, bool)> = Vec::new();
        let base = if use_base { self.facts.get(at).unwrap_or(&empty) } else { &empty };
        let mut all: Vec<(Val, bool)> = base.clone();
        if let Some(e) = ex {
            if !all.iter().any(|(c, _)| *c == e.0) {
                all.push(e);
            }
        }
        if all.is_empty() {
            return None;
        }
        let facts = &all;
        let mut r = ty_range(t)?;
        for (cond, truth) in facts {
            let (op, cty, a, b) = match self.cmps.get(cond) {
                Some(x) => *x,
                None => continue,
            };
            // The comparison has to be about a value of exactly this type;
            // otherwise "less than" may not mean what it looks like
            // (`cmp.lt.i64` over a `u64` value is a different question).
            if cty != t {
                continue;
            }
            let op = if *truth { op } else { invert(op) };
            // `v OP other` and the mirrored `other OP v`; also `v + k OP
            // other` (`while j + 1 < n` bounds `j` by `n - 1`).
            let mut shift: i128 = 0;
            let (op, other) = if a == v {
                (op, b)
            } else if let Some(k) = self.offset_of(a, v, at, 0) {
                shift = k;
                (op, b)
            } else if let Some(k) = self.offset_of(b, v, at, 0) {
                shift = k;
                (
                    match op {
                        CmpOp::Lt => CmpOp::Gt,
                        CmpOp::Le => CmpOp::Ge,
                        CmpOp::Gt => CmpOp::Lt,
                        CmpOp::Ge => CmpOp::Le,
                        x => x,
                    },
                    a,
                )
            } else if b == v {
                (
                    match op {
                        CmpOp::Lt => CmpOp::Gt,
                        CmpOp::Le => CmpOp::Ge,
                        CmpOp::Gt => CmpOp::Lt,
                        CmpOp::Ge => CmpOp::Le,
                        x => x,
                    },
                    a,
                )
            } else {
                continue;
            };
            // The other side needs an interval of its own; asking for it
            // must not walk back into this fact, hence `depth = 0` there.
            let ro = match self.range_x(other, at, depth, false, None) {
                Some(x) => x,
                None => continue,
            };
            // `v + shift OP other`  =>  `v OP other - shift`
            let ro = match (ro.0.checked_sub(shift), ro.1.checked_sub(shift)) {
                (Some(l), Some(h)) => (l, h),
                _ => continue,
            };
            match op {
                CmpOp::Lt => r.1 = r.1.min(ro.1.saturating_sub(1)),
                CmpOp::Le => r.1 = r.1.min(ro.1),
                CmpOp::Gt => r.0 = r.0.max(ro.0.saturating_add(1)),
                CmpOp::Ge => r.0 = r.0.max(ro.0),
                CmpOp::Eq => {
                    r.0 = r.0.max(ro.0);
                    r.1 = r.1.min(ro.1);
                }
                CmpOp::Ne => {}
            }
        }
        Some(r)
    }
}

fn combine(op: BinOp, a: Range, b: Range) -> Option<Range> {
    match op {
        BinOp::Add => Some((a.0.checked_add(b.0)?, a.1.checked_add(b.1)?)),
        BinOp::Sub => Some((a.0.checked_sub(b.1)?, a.1.checked_sub(b.0)?)),
        BinOp::Mul => {
            let c = [
                a.0.checked_mul(b.0)?,
                a.0.checked_mul(b.1)?,
                a.1.checked_mul(b.0)?,
                a.1.checked_mul(b.1)?,
            ];
            Some((*c.iter().min()?, *c.iter().max()?))
        }
        _ => None,
    }
}

/// How deep the interval search follows the definitions. Four is enough for
/// `a * n + k` with a bounded `a` and `k`, and it keeps the pass linear in
/// practice.
const DEPTH: u32 = 4;

/// Replaces every checked operation whose check provably cannot fire with
/// the unchecked one. Yields how many.
pub(crate) fn remove_provable_arith_checks(f: &mut Func) -> usize {
    let any = f.blocks.iter().any(|b| {
        b.insts.iter().any(|i| {
            matches!(
                i.op,
                Op::CheckedBin { .. }
                    | Op::CheckedCast { .. }
                    | Op::CheckedDiv { .. }
                    | Op::CheckedIdx { .. }
                    | Op::CheckedIdxV { .. }
            )
        })
    });
    if !any || f.blocks.len() > 512 {
        return 0;
    }
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    // `(block, position, result, index)` of every array index whose check
    // cannot fire: the result is the index itself, so uses are redirected.
    let mut idx_hits: Vec<(usize, usize, Val, Val)> = Vec::new();
    let plan: Vec<(usize, usize, Op)> = {
        let ana = Ana::new(f);
        let mut plan = Vec::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, inst) in b.insts.iter().enumerate() {
                let ty = inst.ty;
                let full = match ty_range(ty) {
                    Some(x) => x,
                    None => continue,
                };
                // A `secret` value never loses a check: the panic path is
                // part of what `#[constant_time]` promises (SPEC §9.2).
                if inst.dst.map(|d| f.is_secret(d)).unwrap_or(false) {
                    continue;
                }
                let new = match &inst.op {
                    // The checked index: when the interval of the index lies
                    // inside `[0, len-1]` the comparison can never be taken.
                    // Reads the same intervals as the arithmetic checks, so
                    // `x & 15`, `x % 16`, `i * 8 + j` under loop guards and
                    // the like are decided here. A secret index keeps its
                    // check (SPEC §9.2).
                    Op::CheckedIdx { idx, len, .. } => {
                        if f.is_secret(*idx) {
                            continue;
                        }
                        if let (Some(d), Some(r)) = (inst.dst, ana.range(*idx, bi, DEPTH)) {
                            if r.0 >= 0 && r.1 < *len as i128 {
                                idx_hits.push((bi, ii, d, *idx));
                            }
                        }
                        continue;
                    }
                    Op::CheckedIdxV { idx, len, .. } => {
                        if f.is_secret(*idx) {
                            continue;
                        }
                        if let Some(d) = inst.dst {
                            if f.val_ty(*idx) == FTy::U64 && ana.less(*idx, *len, bi, None, DEPTH) {
                                idx_hits.push((bi, ii, d, *idx));
                            }
                        }
                        continue;
                    }
                    Op::CheckedBin { op, a, b: rhs, .. } => {
                        if f.is_secret(*a) || f.is_secret(*rhs) {
                            continue;
                        }
                        let ra = match ana.range(*a, bi, DEPTH) {
                            Some(x) => x,
                            None => continue,
                        };
                        let rb = match ana.range(*rhs, bi, DEPTH) {
                            Some(x) => x,
                            None => continue,
                        };
                        match combine(*op, ra, rb) {
                            Some(r) if fits(r, full) => Op::Bin(*op, *a, *rhs),
                            _ => continue,
                        }
                    }
                    Op::CheckedCast { src, from, .. } => {
                        if f.is_secret(*src) {
                            continue;
                        }
                        match ana.range(*src, bi, DEPTH) {
                            Some(r) if fits(r, full) => Op::Cast { src: *src, from: *from },
                            _ => continue,
                        }
                    }
                    Op::CheckedDiv { op, a, b: rhs, .. } => {
                        if f.is_secret(*a) || f.is_secret(*rhs) {
                            continue;
                        }
                        let rb = match ana.range(*rhs, bi, DEPTH) {
                            Some(x) => x,
                            None => continue,
                        };
                        // Division by zero has to stay possible unless it is
                        // provably impossible.
                        if rb.0 <= 0 && rb.1 >= 0 {
                            continue;
                        }
                        // The signed special case: `MIN / -1` has no result.
                        if ty.signed() {
                            let ra = match ana.range(*a, bi, DEPTH) {
                                Some(x) => x,
                                None => continue,
                            };
                            if rb.0 <= -1 && rb.1 >= -1 && ra.0 <= full.0 {
                                continue;
                            }
                        }
                        Op::Bin(*op, *a, *rhs)
                    }
                    _ => continue,
                };
                plan.push((bi, ii, new));
            }
        }
        plan
    };
    let mut n = plan.len();
    for (bi, ii, op) in plan {
        f.blocks[bi].insts[ii].op = op;
    }
    // THE SAME INDEX, CHECKED BEFORE. A check of `idx` against `len1` that
    // dominates a check of the same value against `len2 >= len1` has
    // already panicked for every index the second one would reject, so the
    // second can never fire. (`a[i]` then `b[i]`, `len a <= len b`.) The
    // dominating one may itself be removed by a proof above or by a third
    // dominating check -- dominance is a strict order, so no cycle can make
    // two checks justify each other.
    {
        let mut sites: Vec<(usize, usize, Val, Val, u64)> = Vec::new();
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, inst) in b.insts.iter().enumerate() {
                if let (Some(d), Op::CheckedIdx { idx, len, .. }) = (inst.dst, &inst.op) {
                    if !f.is_secret(*idx) && !idx_hits.iter().any(|h| h.0 == bi && h.1 == ii) {
                        sites.push((bi, ii, d, *idx, *len));
                    }
                }
            }
        }
        if sites.len() > 1 {
            let dom = crate::mem2reg::dominators(f);
            let mut extra: Vec<(usize, usize, Val, Val)> = Vec::new();
            for &(b2, i2, d2, x2, l2) in &sites {
                let covered = sites.iter().any(|&(b1, i1, _, x1, l1)| {
                    x1 == x2
                        && l1 <= l2
                        && (b1, i1) != (b2, i2)
                        && ((b1 == b2 && i1 < i2) || (b1 != b2 && dom[b2][b1]))
                });
                if covered {
                    extra.push((b2, i2, d2, x2));
                }
            }
            idx_hits.extend(extra);
        }
    }
    if !idx_hits.is_empty() {
        let mut map: HashMap<Val, Val> = HashMap::new();
        for (bi, ii, dst, idx) in idx_hits {
            f.blocks[bi].insts[ii].op = Op::Const(0);
            map.insert(dst, idx);
        }
        n += map.len();
        crate::mem2reg::replace_uses(f, &map);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{Block, FTy, Inst, Op, Term};

    /// `while i < 240 { i = i + 1 }` with `i` as a phi — the shape round 90
    /// named as the one that could not be proven.
    fn counted_loop() -> Func {
        let mut f = Func::new("t", vec![], FTy::U64);
        let zero = f.new_val_pub(FTy::U64);
        let lim = f.new_val_pub(FTy::U64);
        let one = f.new_val_pub(FTy::U64);
        let i = f.new_val_pub(FTy::U64);
        let c = f.new_val_pub(FTy::Bool);
        let nx = f.new_val_pub(FTy::U64);
        f.blocks = vec![
            Block {
                id: 0,
                insts: vec![
                    Inst::new(Some(zero), FTy::U64, Op::Const(0)),
                    Inst::new(Some(lim), FTy::U64, Op::Const(240)),
                    Inst::new(Some(one), FTy::U64, Op::Const(1)),
                ],
                term: Term::Br(1),
            },
            Block {
                id: 1,
                insts: vec![
                    Inst::new(
                        Some(i),
                        FTy::U64,
                        Op::Phi { incoming: vec![(0, zero), (2, nx)] },
                    ),
                    Inst::new(
                        Some(c),
                        FTy::Bool,
                        Op::Cmp { op: CmpOp::Lt, ty: FTy::U64, a: i, b: lim },
                    ),
                ],
                term: Term::BrCond { cond: c, then_bb: 2, else_bb: 3 },
            },
            Block {
                id: 2,
                insts: vec![Inst::new(
                    Some(nx),
                    FTy::U64,
                    Op::CheckedBin { op: BinOp::Add, a: i, b: one, msg: "x".into() },
                )],
                term: Term::Br(1),
            },
            Block { id: 3, insts: vec![], term: Term::Ret(Some(i)) },
        ];
        f
    }

    #[test]
    fn the_counter_of_a_counted_loop_needs_no_check() {
        let mut f = counted_loop();
        assert_eq!(remove_provable_arith_checks(&mut f), 1);
        assert!(matches!(f.blocks[2].insts[0].op, Op::Bin(BinOp::Add, ..)));
    }

    #[test]
    fn without_the_guard_the_check_stays() {
        let mut f = counted_loop();
        // The loop guard becomes `i != 240` — that says nothing about how
        // big `i` is, so the addition may overflow and has to stay checked.
        f.blocks[1].insts[1].op = Op::Cmp { op: CmpOp::Ne, ty: FTy::U64, a: 3, b: 1 };
        assert_eq!(remove_provable_arith_checks(&mut f), 0);
        assert!(matches!(f.blocks[2].insts[0].op, Op::CheckedBin { .. }));
    }

    #[test]
    fn a_remainder_bounds_the_conversion() {
        // `(x % 251) as u8` — 250 fits into a u8, so the checked cast goes.
        let mut f = Func::new("t", vec![FTy::U64], FTy::U8);
        let k = f.push(0, FTy::U64, Op::Const(251));
        let r = f.push(0, FTy::U64, Op::Bin(BinOp::Rem, 0, k));
        let c = f.push(
            0,
            FTy::U8,
            Op::CheckedCast { src: r, from: FTy::U64, msg: "x".into() },
        );
        f.set_term(0, Term::Ret(Some(c)));
        assert_eq!(remove_provable_arith_checks(&mut f), 1);
        assert!(matches!(f.blocks[0].insts[2].op, Op::Cast { .. }));
    }

    #[test]
    fn a_remainder_that_does_not_fit_keeps_its_check() {
        let mut f = Func::new("t", vec![FTy::U64], FTy::U8);
        let k = f.push(0, FTy::U64, Op::Const(300));
        let r = f.push(0, FTy::U64, Op::Bin(BinOp::Rem, 0, k));
        let c = f.push(
            0,
            FTy::U8,
            Op::CheckedCast { src: r, from: FTy::U64, msg: "x".into() },
        );
        f.set_term(0, Term::Ret(Some(c)));
        assert_eq!(remove_provable_arith_checks(&mut f), 0);
    }

    #[test]
    fn division_by_a_constant_that_is_not_zero_needs_no_check() {
        let mut f = Func::new("t", vec![FTy::U64], FTy::U64);
        let k = f.push(0, FTy::U64, Op::Const(10));
        let q = f.push(
            0,
            FTy::U64,
            Op::CheckedDiv {
                op: BinOp::Div,
                a: 0,
                b: k,
                msg_zero: "z".into(),
                msg_range: "r".into(),
            },
        );
        f.set_term(0, Term::Ret(Some(q)));
        assert_eq!(remove_provable_arith_checks(&mut f), 1);
    }

    #[test]
    fn division_by_an_unknown_value_keeps_its_check() {
        let mut f = Func::new("t", vec![FTy::U64, FTy::U64], FTy::U64);
        let q = f.push(
            0,
            FTy::U64,
            Op::CheckedDiv {
                op: BinOp::Div,
                a: 0,
                b: 1,
                msg_zero: "z".into(),
                msg_range: "r".into(),
            },
        );
        f.set_term(0, Term::Ret(Some(q)));
        assert_eq!(remove_provable_arith_checks(&mut f), 0);
    }

    fn idx_func(len: u64, mask: i128, rem: bool) -> Func {
        let mut f = Func::new("t", vec![FTy::U64], FTy::U64);
        let k = f.push(0, FTy::U64, Op::Const(mask));
        let r = f.push(0, FTy::U64, Op::Bin(if rem { BinOp::Rem } else { BinOp::And }, 0, k));
        let c = f.push(0, FTy::U64, Op::CheckedIdx { idx: r, len, msg: "x".into() });
        f.set_term(0, Term::Ret(Some(c)));
        f
    }

    #[test]
    fn a_masked_index_needs_no_bounds_check() {
        let mut f = idx_func(16, 15, false);
        assert_eq!(remove_provable_arith_checks(&mut f), 1);
        assert!(!f.blocks[0].insts.iter().any(|i| matches!(i.op, Op::CheckedIdx { .. })));
        // the result was redirected to the index itself
        assert!(matches!(f.blocks[0].term, Term::Ret(Some(v)) if v == 2));
    }

    #[test]
    fn a_remainder_index_needs_no_bounds_check() {
        let mut f = idx_func(16, 16, true);
        assert_eq!(remove_provable_arith_checks(&mut f), 1);
    }

    #[test]
    fn an_index_one_past_the_end_keeps_its_bounds_check() {
        let mut f = idx_func(15, 15, false);
        assert_eq!(remove_provable_arith_checks(&mut f), 0);
        assert!(matches!(f.blocks[0].insts[2].op, Op::CheckedIdx { .. }));
        let mut g = idx_func(16, 17, true);
        assert_eq!(remove_provable_arith_checks(&mut g), 0);
    }

    #[test]
    fn a_dominating_check_of_the_same_index_covers_a_longer_array() {
        let mut f = Func::new("t", vec![FTy::U64], FTy::U64);
        let a = f.push(0, FTy::U64, Op::CheckedIdx { idx: 0, len: 16, msg: "a".into() });
        let b = f.push(0, FTy::U64, Op::CheckedIdx { idx: 0, len: 32, msg: "b".into() });
        let s = f.push(0, FTy::U64, Op::Bin(BinOp::Add, a, b));
        f.set_term(0, Term::Ret(Some(s)));
        assert_eq!(remove_provable_arith_checks(&mut f), 1);
        assert!(matches!(f.blocks[0].insts[0].op, Op::CheckedIdx { len: 16, .. }));
        assert!(matches!(f.blocks[0].insts[1].op, Op::Const(0)));
    }

    #[test]
    fn a_dominating_check_against_a_longer_array_covers_nothing() {
        let mut f = Func::new("t", vec![FTy::U64], FTy::U64);
        let a = f.push(0, FTy::U64, Op::CheckedIdx { idx: 0, len: 32, msg: "a".into() });
        let b = f.push(0, FTy::U64, Op::CheckedIdx { idx: 0, len: 16, msg: "b".into() });
        let s = f.push(0, FTy::U64, Op::Bin(BinOp::Add, a, b));
        f.set_term(0, Term::Ret(Some(s)));
        assert_eq!(remove_provable_arith_checks(&mut f), 0);
    }

    #[test]
    fn an_unknown_index_keeps_its_bounds_check() {
        let mut f = Func::new("t", vec![FTy::U64], FTy::U64);
        let c = f.push(0, FTy::U64, Op::CheckedIdx { idx: 0, len: 16, msg: "x".into() });
        f.set_term(0, Term::Ret(Some(c)));
        assert_eq!(remove_provable_arith_checks(&mut f), 0);
    }

    #[test]
    fn an_unbounded_addition_keeps_its_check() {
        let mut f = Func::new("t", vec![FTy::U64, FTy::U64], FTy::U64);
        let s = f.push(
            0,
            FTy::U64,
            Op::CheckedBin { op: BinOp::Add, a: 0, b: 1, msg: "x".into() },
        );
        f.set_term(0, Term::Ret(Some(s)));
        assert_eq!(remove_provable_arith_checks(&mut f), 0);
    }

    #[test]
    fn a_signed_division_by_minus_one_keeps_its_check() {
        let mut f = Func::new("t", vec![FTy::I64], FTy::I64);
        let k = f.push(0, FTy::I64, Op::Const(-1));
        let q = f.push(
            0,
            FTy::I64,
            Op::CheckedDiv {
                op: BinOp::Div,
                a: 0,
                b: k,
                msg_zero: "z".into(),
                msg_range: "r".into(),
            },
        );
        f.set_term(0, Term::Ret(Some(q)));
        assert_eq!(remove_provable_arith_checks(&mut f), 0);
    }

    /// `if x > 16 { x = 16 }; a[x]` -- the phi joins `x` (arriving under
    /// `x <= 16`) and the constant 16.
    fn clamp_func(len: u64) -> Func {
        let mut f = Func::new("t", vec![FTy::U64], FTy::U64);
        let k = f.new_val_pub(FTy::U64);
        let c = f.new_val_pub(FTy::Bool);
        let m = f.new_val_pub(FTy::U64);
        let ix = f.new_val_pub(FTy::U64);
        f.blocks = vec![
            Block {
                id: 0,
                insts: vec![
                    Inst::new(Some(k), FTy::U64, Op::Const(16)),
                    Inst::new(Some(c), FTy::Bool, Op::Cmp { op: CmpOp::Gt, ty: FTy::U64, a: 0, b: k }),
                ],
                term: Term::BrCond { cond: c, then_bb: 1, else_bb: 2 },
            },
            Block { id: 1, insts: vec![], term: Term::Br(2) },
            Block {
                id: 2,
                insts: vec![
                    Inst::new(Some(m), FTy::U64, Op::Phi { incoming: vec![(0, 0), (1, k)] }),
                    Inst::new(Some(ix), FTy::U64, Op::CheckedIdx { idx: m, len, msg: "x".into() }),
                ],
                term: Term::Ret(Some(ix)),
            },
        ];
        f
    }

    #[test]
    fn a_clamped_index_needs_no_bounds_check() {
        let mut f = clamp_func(17);
        assert_eq!(remove_provable_arith_checks(&mut f), 1);
    }

    #[test]
    fn a_clamp_one_past_the_end_keeps_its_bounds_check() {
        let mut f = clamp_func(16);
        assert_eq!(remove_provable_arith_checks(&mut f), 0);
    }

    #[test]
    fn a_phi_with_an_unbounded_operand_keeps_its_bounds_check() {
        let mut f = clamp_func(17);
        // the direct edge now carries the parameter under `x > 16`
        f.blocks[0].term = Term::BrCond { cond: 2, then_bb: 2, else_bb: 1 };
        assert_eq!(remove_provable_arith_checks(&mut f), 0);
    }

    /// `if j + 1 < 16 { a[j] }` -- the guard is about `j + 1`.
    fn offset_func(len: u64) -> Func {
        let mut f = Func::new("t", vec![FTy::U64], FTy::U64);
        let one = f.new_val_pub(FTy::U64);
        let lim = f.new_val_pub(FTy::U64);
        let s = f.new_val_pub(FTy::U64);
        let c = f.new_val_pub(FTy::Bool);
        let ix = f.new_val_pub(FTy::U64);
        f.blocks = vec![
            Block {
                id: 0,
                insts: vec![
                    Inst::new(Some(one), FTy::U64, Op::Const(1)),
                    Inst::new(Some(lim), FTy::U64, Op::Const(16)),
                    Inst::new(
                        Some(s),
                        FTy::U64,
                        Op::CheckedBin { op: BinOp::Add, a: 0, b: one, msg: "x".into() },
                    ),
                    Inst::new(Some(c), FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::U64, a: s, b: lim }),
                ],
                term: Term::BrCond { cond: c, then_bb: 1, else_bb: 2 },
            },
            Block {
                id: 1,
                insts: vec![Inst::new(Some(ix), FTy::U64, Op::CheckedIdx { idx: 0, len, msg: "x".into() })],
                term: Term::Ret(Some(ix)),
            },
            Block { id: 2, insts: vec![], term: Term::Ret(Some(0)) },
        ];
        f
    }

    #[test]
    fn a_guard_on_an_offset_bounds_the_base() {
        let mut f = offset_func(15);
        // the checked add (`j + 1 < 16` => `j <= 14`) is gone and so is the index check
        assert!(remove_provable_arith_checks(&mut f) >= 1);
        assert!(!f.blocks[1].insts.iter().any(|i| matches!(i.op, Op::CheckedIdx { .. })));
    }

    #[test]
    fn a_guard_on_an_offset_does_not_bound_the_base_too_far() {
        let mut f = offset_func(14);
        remove_provable_arith_checks(&mut f);
        assert!(f.blocks[1].insts.iter().any(|i| matches!(i.op, Op::CheckedIdx { .. })));
    }
    /// `if i < len { v[i] }` with the length read twice (r755). `store`
    /// puts a store between the two reads.
    fn vec_func(store: bool) -> Func {
        let mut f = Func::new("t", vec![FTy::Ptr, FTy::U64], FTy::U64);
        let off = f.new_val_pub(FTy::I64);
        let a = f.new_val_pub(FTy::Ptr);
        let l1 = f.new_val_pub(FTy::U64);
        let c = f.new_val_pub(FTy::Bool);
        let l2 = f.new_val_pub(FTy::U64);
        let ix = f.new_val_pub(FTy::U64);
        let mut tail = vec![];
        if store {
            tail.push(Inst::new(None, FTy::U64, Op::Store { addr: a, val: 1 }));
        }
        tail.push(Inst::new(Some(l2), FTy::U64, Op::Load { addr: a }));
        tail.push(Inst::new(Some(ix), FTy::U64, Op::CheckedIdxV { idx: 1, len: l2, msg: "x".into() }));
        f.blocks = vec![
            Block {
                id: 0,
                insts: vec![
                    Inst::new(Some(off), FTy::I64, Op::Const(8)),
                    Inst::new(Some(a), FTy::Ptr, Op::PtrAdd { base: 0, off }),
                    Inst::new(Some(l1), FTy::U64, Op::Load { addr: a }),
                    Inst::new(Some(c), FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::U64, a: 1, b: l1 }),
                ],
                term: Term::BrCond { cond: c, then_bb: 1, else_bb: 2 },
            },
            Block { id: 1, insts: tail, term: Term::Ret(Some(ix)) },
            Block { id: 2, insts: vec![], term: Term::Ret(Some(1)) },
        ];
        f
    }

    #[test]
    fn an_index_below_a_guarded_length_needs_no_check() {
        let mut f = vec_func(false);
        assert_eq!(remove_provable_arith_checks(&mut f), 1);
        assert!(!f.blocks[1].insts.iter().any(|i| matches!(i.op, Op::CheckedIdxV { .. })));
    }

    #[test]
    fn a_store_between_the_two_length_reads_keeps_the_check() {
        let mut f = vec_func(true);
        assert_eq!(remove_provable_arith_checks(&mut f), 0);
    }
}
