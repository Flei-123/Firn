// SPDX-License-Identifier: MPL-2.0
//! **Round TEMPO 9 — the loop that only writes zeros.**
//!
//! ## Why this pass exists
//!
//! `rt.mem_set` is written in Firn, and like this:
//!
//! ```firn
//! fn mem_set(target: u64, value: u8, n: usize) {
//!     var i: usize = 0
//!     while i < n {
//!         st8(target, i, value)
//!         i = i + 1
//!     }
//! }
//! ```
//!
//! That is correct and usable everywhere — and it costs **five instructions
//! per octet**. Measured on the MP3 decoder (`callgrind`, 8 s of sound): the call
//! `mem_set(grbuf, 0, 4608)` once per granule accounts for **14.2 of 160.6
//! million instructions, that is nine percent of the whole program** — and only
//! to set an array to zero.
//!
//! ```text
//!   cmp   $0x1200,%rdx
//!   jae   done
//!   mov   -0x8b0(%rbp),%r11     ; fetch the pointer anew EVERY TIME
//!   movb  $0x0,(%r11,%rdx,1)
//!   lea   0x1(%rdx),%rdx
//!   jmp   head
//! ```
//!
//! x86 has an instruction for this (`rep stosb`), aarch64 a short loop in
//! eight-byte units. Both have long stood in the generator — as `Op::SecureZero`, which
//! serves `secure_zero(inout buf)`. It does **exactly** what is needed
//! here: `size` octets from `addr` to zero. So the pass only has
//! to recognise the loop and replace it with this one instruction.
//!
//! ## The pattern
//!
//! After `mem2reg` and `licm` the loop in FIR always looks the same:
//!
//! ```text
//! P:    ...                          <- pre-header
//!       br H
//! H:    %i = phi [P %null, B %i2]
//!       %c = cmp.lt.uXX %i, %n
//!       brcond %c, B, X
//! B:    %a = add %base, %i
//!       store.u8 %value, %a
//!       %i2 = add %i, %one
//!       br H
//! ```
//!
//! From this becomes `secure_zero(%base, %n)` in the pre-header and `br X` in `H`; the
//! rest is cleared away by `dce`.
//!
//! ## The conditions, and why each single one is necessary
//!
//! * **The body contains EXACTLY these three instructions.** Anything else —
//!   a second memory access, a call, a read — would be an effect
//!   that `rep stosb` does not have.
//! * **The written value is the constant 0 and one octet wide.**
//!   `SecureZero` can only do zeros; a `mem_set(p, 7, n)` stays the
//!   loop.
//! * **The comparison is UNSIGNED.** With `i64`, `n` could be negative:
//!   the loop then runs zero times, whereas `rep stosb` with `rcx = -1`
//!   would fill half the address space. That is not a theoretical
//!   case, but the difference between "do nothing" and "machine gone".
//! * **`%base` and `%n` are defined outside the loop** (parameter,
//!   constant or a block that dominates the pre-header). Otherwise they do not
//!   exist in the pre-header yet.
//! * **The step is exactly 1 and the start exactly 0.** Only then
//!   does the loop hit every octet from `base` to `base+n` and no
//!   other.
//! * **`%i`, `%i2` and `%a` are not read outside the loop.**
//!   The final value of `%i` would be `n`, but supplying that later is work for
//!   a case that does not exist in the existing code.
//! * **Small constant lengths stay a loop.** `rep stosb` has a
//!   start-up time of a few dozen cycles on today's processors; below
//!   sixteen octets the loop is faster.
//!
//! ## What the pass does NOT do
//!
//! It recognises no `mem_copy` (there is `Op::CopyMem` for it, but the
//! loop there reads AND writes, and the question of overlap is a
//! different one) and no value other than zero. Both would be a round of their own
//! with a measurement of their own.

use crate::fir::{BinOp, CmpOp, FTy, Func, Inst, Op, Term, Val};

/// Run it; returns the number of replaced loops.
pub fn recognise(f: &mut Func) -> usize {
    let nb = f.blocks.len();
    if nb < 3 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    // Vorgaenger je Block.
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); nb];
    for (i, b) in f.blocks.iter().enumerate() {
        for s in b.term.successors() {
            let s = s as usize;
            if s < nb && !preds[s].contains(&i) {
                preds[s].push(i);
            }
        }
    }
    let dom = crate::mem2reg::dominators(f);
    // Where is which value defined? (block, instruction)
    let nv = f.val_types.len();
    let mut defblock: Vec<Option<usize>> = vec![None; nv];
    for (bi, b) in f.blocks.iter().enumerate() {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if (d as usize) < nv {
                    defblock[d as usize] = Some(bi);
                }
            }
        }
    }
    let npar = f.params.len();

    let mut plan: Vec<(usize, usize, usize, Val, Val)> = Vec::new(); // (P, H, B, base, n)
    let mut used: Vec<bool> = vec![false; nb];

    'head: for h in 0..nb {
        if used[h] {
            continue;
        }
        let hit = match pattern(f, &preds, &defblock, npar, &dom, h) {
            Some(t) => t,
            None => continue 'head,
        };
        let (p, b, base, n) = hit;
        used[h] = true;
        used[b] = true;
        plan.push((p, h, b, base, n));
    }
    if plan.is_empty() {
        return 0;
    }
    let count = plan.len();
    for (p, h, _b, base, n) in plan {
        // The target of the loop is the exit of `H`.
        let x = match f.blocks[h].term {
            Term::BrCond { then_bb, else_bb, .. } => {
                // `then` is the body, so `else` is the exit.
                let body = then_bb;
                let _ = body;
                else_bb
            }
            _ => continue,
        };
        // `secure_zero` at the end of the pre-header, before its jump.
        let loc = f.blocks[p].insts.last().map(|i| i.loc).unwrap_or_default();
        f.blocks[p].insts.push(Inst {
            dst: None,
            ty: FTy::Void,
            op: Op::SecureZero { addr: base, size: n },
            loc,
        });
        f.blocks[h].term = Term::Br(x);
    }
    count
}

/// Does the pattern fit at `h`? Returns `(pre-header, body, base, n)`.
fn pattern(
    f: &Func,
    preds: &[Vec<usize>],
    defblock: &[Option<usize>],
    npar: usize,
    dom: &[Vec<bool>],
    h: usize,
) -> Option<(usize, usize, Val, Val)> {
    // --- the head: one phi, one comparison, one conditional jump ---------
    let kb = &f.blocks[h];
    if kb.insts.len() != 2 {
        return None;
    }
    let (cond, body, ausgang) = match kb.term {
        Term::BrCond { cond, then_bb, else_bb } => (cond, then_bb as usize, else_bb as usize),
        _ => return None,
    };
    if body >= f.blocks.len() || ausgang >= f.blocks.len() || body == h {
        return None;
    }
    let iv = match (&kb.insts[0].op, kb.insts[0].dst) {
        (Op::Phi { incoming }, Some(d)) => {
            if incoming.len() != 2 {
                return None;
            }
            (d, incoming.clone())
        }
        _ => return None,
    };
    let (i_val, inc) = iv;
    // The comparison: `i < n`, UNSIGNED.
    let (n_val, cmp_ty) = match (&kb.insts[1].op, kb.insts[1].dst) {
        (Op::Cmp { op: CmpOp::Lt, ty, a, b }, Some(d)) if d == cond && *a == i_val => (*b, *ty),
        _ => return None,
    };
    if cmp_ty.signed() || cmp_ty.is_float() {
        return None;
    }

    // --- the body: exactly three instructions, jump back ----------------
    let rb = &f.blocks[body];
    if rb.insts.len() != 3 || !matches!(rb.term, Term::Br(t) if t as usize == h) {
        return None;
    }
    if preds[body].len() != 1 || preds[body][0] != h {
        return None;
    }
    // Adresse, Speichern, Fortschalten -- in beliebiger Reihenfolge.
    let mut addr: Option<(Val, Val)> = None; // (Ergebnis, base)
    let mut value: Option<(Val, Val)> = None; // (gespeicherter Wert, Adresse)
    let mut step: Option<(Val, Val)> = None; // (Ergebnis, Schrittkonstante)
    let mut store_ty = FTy::Void;
    for inst in rb.insts.iter() {
        match (&inst.op, inst.dst) {
            (Op::Bin(BinOp::Add, a, b), Some(d)) if *a == i_val || *b == i_val => {
                let other = if *a == i_val { *b } else { *a };
                // The increment is recognised by its result being on
                // the back edge of the phi.
                if inc.iter().any(|(q, v)| *q as usize == body && *v == d) {
                    if step.is_some() {
                        return None;
                    }
                    step = Some((d, other));
                } else {
                    if addr.is_some() {
                        return None;
                    }
                    addr = Some((d, other));
                }
            }
            (Op::Store { val, addr: a }, None) => {
                if value.is_some() {
                    return None;
                }
                store_ty = inst.ty;
                value = Some((*val, *a));
            }
            _ => return None,
        }
    }
    let (a_val, base) = addr?;
    let (v_val, a_used) = value?;
    let (i2_val, step) = step?;
    if a_used != a_val {
        return None;
    }
    // One octet per iteration.
    if store_ty.bits() != 8 {
        return None;
    }

    // --- die Konstanten: Anfang 0, Schrittweite 1, Wert 0 ----------------
    let vor = inc.iter().find(|(q, _)| *q as usize != body)?;
    let p = vor.0 as usize;
    if p >= f.blocks.len() {
        return None;
    }
    if const_of(f, vor.1) != Some(0) {
        return None;
    }
    if const_of(f, step) != Some(1) {
        return None;
    }
    if const_of(f, v_val) != Some(0) {
        return None;
    }
    // The pre-header must have EXACTLY ONE successor (its jump goes to
    // `h`), otherwise the inserted `secure_zero` also writes on the path
    // that never enters the loop.
    if !matches!(f.blocks[p].term, Term::Br(t) if t as usize == h) {
        return None;
    }
    // `h` has exactly two predecessors: the pre-header and the body.
    if preds[h].len() != 2 || !preds[h].contains(&p) || !preds[h].contains(&body) {
        return None;
    }

    // --- `base` and `n` already exist in the pre-header ------------------
    for v in [base, n_val] {
        if (v as usize) < npar {
            continue; // Parameter
        }
        match defblock.get(v as usize).copied().flatten() {
            Some(db) => {
                if db != p && !dom[p][db] {
                    return None;
                }
            }
            None => return None,
        }
    }
    // Neither `base` nor `n` may be the loop variable.
    if base == i_val || n_val == i_val || base == n_val {
        return None;
    }

    // --- nothing from the loop is read outside -----------------------------
    let mut buf = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        if bi == body {
            continue;
        }
        for inst in &b.insts {
            // The phi in the head reads the increment -- that is the
            // back edge and does not count.
            if bi == h && matches!(inst.op, Op::Phi { .. }) {
                continue;
            }
            // The comparison in the head reads the loop variable.
            if bi == h && inst.dst == Some(cond) {
                continue;
            }
            buf.clear();
            inst.op.uses(&mut buf);
            if buf.iter().any(|u| *u == i_val || *u == i2_val || *u == a_val) {
                return None;
            }
        }
        match &b.term {
            Term::BrCond { cond: c, .. } => {
                if *c == i_val || *c == i2_val || *c == a_val {
                    return None;
                }
            }
            Term::Switch { val, .. } | Term::Ret(Some(val)) => {
                if *val == i_val || *val == i2_val || *val == a_val {
                    return None;
                }
            }
            _ => {}
        }
    }

    // --- winzige feste Laengen bleiben Schleife --------------------------
    if let Some(k) = const_of(f, n_val) {
        if k < 16 {
            return None;
        }
    }
    Some((p, body, base, n_val))
}

/// Value of a constant, if the value is one.
fn const_of(f: &Func, v: Val) -> Option<i128> {
    for b in &f.blocks {
        for i in &b.insts {
            if i.dst == Some(v) {
                return match &i.op {
                    Op::Const(c) => Some(i.ty.truncate(*c)),
                    _ => None,
                };
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{Block, Func};

    /// `fn f(p: u64, n: u64) { var i = 0; while i < n { st8(p, i, 0); i += 1 } }`
    fn loop_fn(signed: bool) -> Func {
        let mut f = Func::new("t", vec![FTy::U64, FTy::U64], FTy::Void);
        // %0 = p, %1 = n
        let null = f.new_val_pub(FTy::U64);
        let one = f.new_val_pub(FTy::U64);
        let value = f.new_val_pub(FTy::U8);
        let i = f.new_val_pub(FTy::U64);
        let c = f.new_val_pub(FTy::Bool);
        let a = f.new_val_pub(FTy::U64);
        let i2 = f.new_val_pub(FTy::U64);
        let ct = if signed { FTy::I64 } else { FTy::U64 };
        f.blocks = vec![
            Block {
                id: 0,
                insts: vec![
                    Inst::new(Some(null), FTy::U64, Op::Const(0)),
                    Inst::new(Some(one), FTy::U64, Op::Const(1)),
                    Inst::new(Some(value), FTy::U8, Op::Const(0)),
                ],
                term: Term::Br(1),
            },
            Block {
                id: 1,
                insts: vec![
                    Inst::new(
                        Some(i),
                        FTy::U64,
                        Op::Phi { incoming: vec![(0, null), (2, i2)] },
                    ),
                    Inst::new(
                        Some(c),
                        FTy::Bool,
                        Op::Cmp { op: CmpOp::Lt, ty: ct, a: i, b: 1 },
                    ),
                ],
                term: Term::BrCond { cond: c, then_bb: 2, else_bb: 3 },
            },
            Block {
                id: 2,
                insts: vec![
                    Inst::new(Some(a), FTy::U64, Op::Bin(BinOp::Add, 0, i)),
                    Inst::new(None, FTy::U8, Op::Store { val: value, addr: a }),
                    Inst::new(Some(i2), FTy::U64, Op::Bin(BinOp::Add, i, one)),
                ],
                term: Term::Br(1),
            },
            Block { id: 3, insts: vec![], term: Term::Ret(None) },
        ];
        f
    }

    #[test]
    fn the_unsigned_zero_loop_becomes_one_instruction() {
        let mut f = loop_fn(false);
        assert_eq!(recognise(&mut f), 1);
        assert!(f.blocks[0].insts.iter().any(|i| matches!(i.op, Op::SecureZero { .. })));
        assert!(matches!(f.blocks[1].term, Term::Br(3)));
    }

    /// THE DANGEROUS CASE: with a sign the loop runs zero times for
    /// negative `n` -- `rep stosb` with `rcx = -1` does not.
    #[test]
    fn the_signed_comparison_stays_a_loop() {
        let mut f = loop_fn(true);
        assert_eq!(recognise(&mut f), 0);
    }

    /// A second memory access in the body is no longer a zero loop.
    #[test]
    fn a_second_store_in_the_body_is_refused() {
        let mut f = loop_fn(false);
        let extra = f.new_val_pub(FTy::U8);
        f.blocks[2].insts.insert(
            1,
            Inst::new(Some(extra), FTy::U8, Op::Load { addr: 0 }),
        );
        assert_eq!(recognise(&mut f), 0);
    }
}
