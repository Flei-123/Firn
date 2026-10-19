// SPDX-License-Identifier: MPL-2.0
//! **Loop idiom: the hand written copy loop becomes one block copy.**
//!
//! Sister pass of `memset.rs`. It recognises the counting loop
//!
//! ```text
//! P:    ...                          <- pre-header
//!       br H
//! H:    %i = phi [P %zero, B %i2]
//!       %c = cmp.lt.u64 %i, %n
//!       brcond %c, B, X
//! B:    %o = mul %i, 8               <- scale (absent for 1 byte, or `shl`)
//!       %s = add %src, %o
//!       %v = load.u64 %s
//!       %d = add %dst, %o
//!       store.u64 %v, %d
//!       %i2 = add %i, 1
//!       br H
//! ```
//!
//! and replaces it by `Op::MemCopy { dst, src, n, esz }` in the pre-header
//! (`rep movsb` / `rep movsd` / `rep movsq` on x86-64).
//!
//! ## Why overlap is no problem
//!
//! The loop copies element by element in FORWARD direction: load element `i`,
//! store element `i`, next. `rep movs{b,d,q}` is DEFINED as exactly that
//! sequence (each repetition loads one element and stores one element, in
//! ascending order, with DF = 0), so the result is identical for every
//! overlap: `a[i+1] = a[i]` (a fill with `a[0]`) gives the same bytes as the
//! loop, and so does `dst = src + 3` with 8 byte elements. A hand rolled
//! 16 byte block loop would NOT have this property (it reads 16 bytes before
//! it writes any of them), which is why the pass uses the string instructions.
//! The processor may take a slow path for overlapping operands, but never a
//! different result.
//!
//! ## Conditions (each one is necessary)
//!
//! * Body = address arithmetic + EXACTLY ONE load + ONE store of the loaded
//!   value, same width (8, 32 or 64 bit, no floats) + the increment. Any other
//!   instruction would be an effect the string instruction does not have.
//! * Both addresses are `base + i * esz` with `esz` = the access width, bases
//!   defined outside the loop; start 0, step 1; the compare is UNSIGNED and
//!   64 bit wide (a signed `n < 0` runs zero times, `rcx = -1` would not).
//! * Nothing defined in the loop is used outside of it.
//! * Constant trip counts below 16 elements stay loops (start-up cost of `rep`).
//! * Only the checks-free form (`add`/`mul`/`shl`/`ptradd`) is matched, i.e.
//!   release-fast; the checked forms contain a panic exit the instruction
//!   would drop.
//! * x86-64 only (`opt.rs` gates the pass). The a64 and wasm back ends never
//!   see `MemCopy`.
//!
//! Switch off with `--no-pass=memcpy`.

use crate::fir::{BinOp, CmpOp, FTy, Func, Inst, Op, Term, Val};

static SITES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Emit the copy. `rdi` = dst, `rsi` = src, `rcx` = element count; `rax` is
/// scratch (never the home of a value). `rep movs` has a start-up cost of a
/// few dozen cycles, so a short copy (under 64 bytes) runs a plain loop with
/// the same forward, element by element order instead.
pub(crate) fn emit_rep(e: &mut crate::codegen_x86::Emitter, esz: u8) {
    let id = SITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let small = format!(".Lmcpy_small{}", id);
    let big = format!(".Lmcpy_big{}", id);
    let done = format!(".Lmcpy_done{}", id);
    let (mov_reg, instr, step) = match esz {
        1 => ("al", "movsb", 1),
        4 => ("eax", "movsd", 4),
        _ => ("rax", "movsq", 8),
    };
    e.line(&format!("cmp rcx, {}", 64 / step));
    e.line(&format!("jae {}", big));
    e.line("test rcx, rcx");
    e.line(&format!("jz {}", done));
    e.raw(&format!("{}:", small));
    let w = match esz {
        1 => "byte",
        4 => "dword",
        _ => "qword",
    };
    e.line(&format!("mov {}, {} ptr [rsi]", mov_reg, w));
    e.line(&format!("mov {} ptr [rdi], {}", w, mov_reg));
    e.line(&format!("add rsi, {}", step));
    e.line(&format!("add rdi, {}", step));
    e.line("dec rcx");
    e.line(&format!("jnz {}", small));
    e.line(&format!("jmp {}", done));
    e.raw(&format!("{}:", big));
    e.line("cld");
    e.line(&format!("rep {}", instr));
    e.raw(&format!("{}:", done));
}

/// Run it; returns the number of replaced loops.
pub fn recognise(f: &mut Func) -> usize {
    let nb = f.blocks.len();
    if nb < 3 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
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

    // (pre-header, head, body, dst, src, n, esz)
    let mut plan: Vec<(usize, usize, usize, Val, Val, Val, u8)> = Vec::new();
    let mut used = vec![false; nb];
    for h in 0..nb {
        if used[h] {
            continue;
        }
        if let Some((p, b, dst, src, n, esz)) = pattern(f, &preds, &defblock, npar, &dom, h) {
            used[h] = true;
            used[b] = true;
            plan.push((p, h, b, dst, src, n, esz));
        }
    }
    let count = plan.len();
    for (p, h, _b, dst, src, n, esz) in plan {
        let x = match f.blocks[h].term {
            Term::BrCond { else_bb, .. } => else_bb,
            _ => continue,
        };
        let loc = f.blocks[p].insts.last().map(|i| i.loc).unwrap_or_default();
        f.blocks[p].insts.push(Inst {
            dst: None,
            ty: FTy::Void,
            op: Op::MemCopy { dst, src, n, esz },
            loc,
        });
        f.blocks[h].term = Term::Br(x);
    }
    count
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

/// How a body value relates to the loop variable.
#[derive(Clone, Copy, PartialEq)]
enum Idx {
    /// `i * scale`
    Scaled(i128),
    /// `base + i * scale`; the base is the stored value.
    Addr(Val, i128),
}

fn pattern(
    f: &Func,
    preds: &[Vec<usize>],
    defblock: &[Option<usize>],
    npar: usize,
    dom: &[Vec<bool>],
    h: usize,
) -> Option<(usize, usize, Val, Val, Val, u8)> {
    // --- head: phi, unsigned 64 bit compare, conditional jump -------------
    let hb = &f.blocks[h];
    if hb.insts.len() != 2 {
        return None;
    }
    let (cond, body, exit) = match hb.term {
        Term::BrCond { cond, then_bb, else_bb } => (cond, then_bb as usize, else_bb as usize),
        _ => return None,
    };
    if body >= f.blocks.len() || exit >= f.blocks.len() || body == h {
        return None;
    }
    let (i_val, inc) = match (&hb.insts[0].op, hb.insts[0].dst) {
        (Op::Phi { incoming }, Some(d)) if incoming.len() == 2 => (d, incoming.clone()),
        _ => return None,
    };
    let n_val = match (&hb.insts[1].op, hb.insts[1].dst) {
        (Op::Cmp { op: CmpOp::Lt, ty, a, b }, Some(d))
            if d == cond && *a == i_val && !ty.signed() && !ty.is_float() && ty.bits() == 64 =>
        {
            *b
        }
        _ => return None,
    };

    // --- body: classify every instruction ---------------------------------
    let rb = &f.blocks[body];
    if !matches!(rb.term, Term::Br(t) if t as usize == h) {
        return None;
    }
    if preds[body].len() != 1 || preds[body][0] != h {
        return None;
    }
    let mut rel: Vec<(Val, Idx)> = Vec::new();
    let mut load: Option<(Val, Val, FTy)> = None; // (result, addr, type)
    let mut store: Option<(Val, Val, FTy)> = None; // (value, addr, type)
    let mut step: Option<Val> = None;
    let get = |rel: &[(Val, Idx)], v: Val| rel.iter().find(|(x, _)| *x == v).map(|(_, r)| *r);
    for inst in rb.insts.iter() {
        match (&inst.op, inst.dst) {
            (Op::Bin(BinOp::Mul, a, b), Some(d)) => {
                let (x, k) = if *a == i_val { (*a, *b) } else { (*b, *a) };
                if x != i_val {
                    return None;
                }
                rel.push((d, Idx::Scaled(const_of(f, k)?)));
            }
            (Op::Bin(BinOp::Shl, a, b), Some(d)) if *a == i_val => {
                let s = const_of(f, *b)?;
                if !(0..=3).contains(&s) {
                    return None;
                }
                rel.push((d, Idx::Scaled(1i128 << s)));
            }
            (Op::Bin(BinOp::Add, a, b), Some(d)) | (Op::PtrAdd { base: a, off: b }, Some(d)) => {
                let is_ptradd = matches!(inst.op, Op::PtrAdd { .. });
                // The increment: i + 1, on the back edge of the phi.
                if !is_ptradd
                    && (*a == i_val || *b == i_val)
                    && inc.iter().any(|(q, v)| *q as usize == body && *v == d)
                {
                    let other = if *a == i_val { *b } else { *a };
                    if const_of(f, other) != Some(1) || step.is_some() {
                        return None;
                    }
                    step = Some(d);
                    continue;
                }
                // base + offset, where the offset is `i` or `i * k`.
                let (base, off) = if is_ptradd {
                    (*a, *b)
                } else if *a == i_val || get(&rel, *a).is_some() {
                    (*b, *a)
                } else {
                    (*a, *b)
                };
                let scale = if off == i_val {
                    1
                } else {
                    match get(&rel, off)? {
                        Idx::Scaled(k) => k,
                        _ => return None,
                    }
                };
                if base == i_val || get(&rel, base).is_some() {
                    return None;
                }
                rel.push((d, Idx::Addr(base, scale)));
            }
            (Op::Load { addr }, Some(d)) => {
                if load.is_some() || inst.ty.is_float() {
                    return None;
                }
                load = Some((d, *addr, inst.ty));
            }
            (Op::Store { val, addr }, None) => {
                if store.is_some() || inst.ty.is_float() {
                    return None;
                }
                store = Some((*val, *addr, inst.ty));
            }
            _ => return None,
        }
    }
    let (lv, laddr, lty) = load?;
    let (sv, saddr, sty) = store?;
    let i2_val = step?;
    if sv != lv || lty.bits() != sty.bits() {
        return None;
    }
    let esz = match lty.bits() {
        8 => 1u8,
        32 => 4,
        64 => 8,
        _ => return None,
    };
    let (src, sscale) = match get(&rel, laddr)? {
        Idx::Addr(b, k) => (b, k),
        _ => return None,
    };
    let (dst, dscale) = match get(&rel, saddr)? {
        Idx::Addr(b, k) => (b, k),
        _ => return None,
    };
    if sscale != esz as i128 || dscale != esz as i128 {
        return None;
    }

    // --- start 0; pre-header leads only into the loop ---------------------
    let from = inc.iter().find(|(q, _)| *q as usize != body)?;
    let p = from.0 as usize;
    if p >= f.blocks.len() || const_of(f, from.1) != Some(0) {
        return None;
    }
    if !matches!(f.blocks[p].term, Term::Br(t) if t as usize == h) {
        return None;
    }
    if preds[h].len() != 2 || !preds[h].contains(&p) || !preds[h].contains(&body) {
        return None;
    }
    // --- operands exist in the pre-header ---------------------------------
    for v in [dst, src, n_val] {
        if (v as usize) < npar {
            continue;
        }
        match defblock.get(v as usize).copied().flatten() {
            Some(db) if db == p || dom[p][db] => {}
            _ => return None,
        }
    }
    if dst == i_val || src == i_val || n_val == i_val {
        return None;
    }
    // --- nothing from the loop is used outside ----------------------------
    let mut inner: Vec<Val> = vec![i_val, i2_val, cond];
    for inst in &rb.insts {
        if let Some(d) = inst.dst {
            inner.push(d);
        }
    }
    let mut buf = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        if bi == body {
            continue;
        }
        for inst in &b.insts {
            if bi == h && (matches!(inst.op, Op::Phi { .. }) || inst.dst == Some(cond)) {
                continue;
            }
            buf.clear();
            inst.op.uses(&mut buf);
            if buf.iter().any(|u| inner.contains(u)) {
                return None;
            }
        }
        match &b.term {
            Term::BrCond { cond: c, .. } if inner.contains(c) && !(bi == h && *c == cond) => {
                return None
            }
            Term::Switch { val, .. } | Term::Ret(Some(val)) if inner.contains(val) => return None,
            _ => {}
        }
    }
    // --- tiny constant trip counts stay loops -----------------------------
    if let Some(k) = const_of(f, n_val) {
        if k < 16 {
            return None;
        }
    }
    Some((p, body, dst, src, n_val, esz))
}
