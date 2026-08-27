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
//! ## What it deliberately does not do
//!
//! No fixpoint over phis (a loop counter's interval would come out of the
//! back edge; the loop guard says the same thing more cheaply and without
//! widening), no interval per program point (one interval per value at the
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
fn facts_per_block(f: &Func) -> Vec<Vec<(Val, bool)>> {
    let n = f.blocks.len();
    let preds = crate::mem2reg::preds(f);
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
        Ana { f, site, cmps, facts: facts_per_block(f) }
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
        self.range_x(v, at, depth, true)
    }

    /// `use_facts = false` is what makes this terminate. A fact says
    /// `v < other`, and finding out how big `other` is must not walk back
    /// into the facts -- `i < n` and `n > i` are the same fact and would
    /// ask each other forever. Below a fact only the STRUCTURE of the other
    /// side counts (a constant, a remainder, a checked index), and the
    /// recursion is bounded by `depth` alone.
    fn range_x(&self, v: Val, at: usize, depth: u32, use_facts: bool) -> Option<Range> {
        let t = self.f.val_ty(v);
        let full = ty_range(t)?;
        let mut r = full;
        if depth > 0 {
            if let Some(s) = self.structural(v, at, depth - 1, full, use_facts) {
                r = meet(r, s);
            }
        }
        if use_facts {
            if let Some(s) = self.from_facts(v, at, t, depth) {
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
                let ra = self.range_x(*a, at, depth, use_facts)?;
                let rb = self.range_x(*b, at, depth, use_facts)?;
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
                let rs = self.range_x(*src, at, depth, use_facts)?;
                if fits(rs, full) {
                    Some(rs)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// What the branches on the way into `at` say about the value.
    fn from_facts(&self, v: Val, at: usize, t: FTy, depth: u32) -> Option<Range> {
        let facts = self.facts.get(at)?;
        if facts.is_empty() {
            return None;
        }
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
            // `v OP other` and the mirrored `other OP v`.
            let (op, other) = if a == v {
                (op, b)
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
            let ro = match self.range_x(other, at, depth, false) {
                Some(x) => x,
                None => continue,
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
                Op::CheckedBin { .. } | Op::CheckedCast { .. } | Op::CheckedDiv { .. }
            )
        })
    });
    if !any || f.blocks.len() > 512 {
        return 0;
    }
    if f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
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
    let n = plan.len();
    for (bi, ii, op) in plan {
        f.blocks[bi].insts[ii].op = op;
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
}
