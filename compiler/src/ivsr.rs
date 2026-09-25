// SPDX-License-Identifier: MPL-2.0
//! **Round TEMPO 15** -- strength reduction of induction variables (`ivsr`).
//!
//! ## Why
//!
//! `bench/firn/matmul.fi`, the inner loop:
//!
//! ```firn
//! while k < n { s = s + ld32(a, r * n + k) * ld32(b, k * n + cc); k = k + 1 }
//! ```
//!
//! After inlining, every pass computes `a + 4 * (r*n + k)` and
//! `b + 4 * (k*n + cc)` -- two multiplications and four additions for two
//! addresses that simply move by `4` and by `4n` from one pass to the next.
//! `rustc -O` steps two pointers. Measured before this round: Firn 11
//! instructions per pass, Rust 3.75 (with a fourfold unroll).
//!
//! ## What is done
//!
//! For a natural loop with ONE back edge and ONE predecessor outside (the
//! preheader, ending in a plain `br` to the header):
//!
//! 1. **Basic induction variables** are the header phis of an integer type
//!    whose back edge value is `phi + c` (or `phi - c`) with `c` invariant.
//! 2. **Affine values.** Walking the loop body, every instruction that
//!    computes `base + a * iv + b` -- `a`, `b` loop invariant, `base` an
//!    invariant pointer or nothing -- is recorded with that form: `add`,
//!    `sub`, `mul` by an invariant, `shl` by a constant, `ptradd` of an
//!    invariant base. All in the SAME integer type as the induction
//!    variable, plain or wrapping (`+%`) arithmetic only: in `Z / 2^w` the
//!    identity `a * (iv + c) + b = (a * iv + b) + a * c` is exact, so the new
//!    value is bit identical to the old one, overflow or not. Checked
//!    arithmetic (a trap is an observable effect) and conversions (a sign
//!    extension is not linear modulo `2^w`) end the chain.
//! 3. **Roots** are affine values that are used by something that is not
//!    itself affine (a load, a store, a call ...) and whose chain costs at
//!    least two instructions per pass -- the new phi costs one `add`, so
//!    anything less gains nothing. Scaling by 1, 2, 4, 8 counts zero (it
//!    folds into the address, `[b + i*4]`), and so does a last addition
//!    whose every reader is a load or store address. Measured on the MP3
//!    decoder: reducing every chain with any multiplication made `dct_ii_4`
//!    0.64 M and `l3_imdct36` 0.20 M instructions slower (`j * 18` is one
//!    `imul`, the phi one `add` plus a register for the whole loop).
//! 4. **The rewrite.** Each root gets a header phi `q`: in the preheader
//!    `q0 = base + a * init + b` and `step = a * c`, at the end of the latch
//!    `q' = q + step`. The uses of the root read `q`; the old chain dies and
//!    `dce` takes it. Identical forms share one phi. At most
//!    `MAX_NEW_PHIS` per loop -- every phi is a register for the whole loop.
//!
//! Innermost loops first. The preheader code of an inner loop is itself
//! affine in the outer induction variable, so the outer loop is reduced on
//! the next round of the optimizer loop.
//!
//! `FIRN_NO_IVSR=1` switches it off; `FIRN_IVSR_TRACE=1` prints each
//! reduced root.

use crate::fir::{BinOp, FTy, Func, Inst, Op, Term, Val, WrapSatKind};
use std::collections::{HashMap, HashSet};

const MAX_NEW_PHIS: usize = 6;

thread_local! {
    /// Functions this pass has changed (by name): only there can a dead
    /// induction cycle or a twin phi of its own making stand.
    static TOUCHED: std::cell::RefCell<HashSet<String>> = std::cell::RefCell::new(HashSet::new());
}

/// A loop invariant expression, built from values that dominate the
/// preheader and from constants.
#[derive(Clone, Debug, PartialEq)]
enum Inv {
    C(i128),
    V(Val),
    Add(Box<Inv>, Box<Inv>),
    Sub(Box<Inv>, Box<Inv>),
    Mul(Box<Inv>, Box<Inv>),
}

fn add(a: Inv, b: Inv) -> Inv {
    match (&a, &b) {
        (Inv::C(0), _) => b,
        (_, Inv::C(0)) => a,
        (Inv::C(x), Inv::C(y)) => Inv::C(x.wrapping_add(*y)),
        _ => Inv::Add(Box::new(a), Box::new(b)),
    }
}
fn sub(a: Inv, b: Inv) -> Inv {
    match (&a, &b) {
        (_, Inv::C(0)) => a,
        (Inv::C(x), Inv::C(y)) => Inv::C(x.wrapping_sub(*y)),
        _ => Inv::Sub(Box::new(a), Box::new(b)),
    }
}
fn mul(a: Inv, b: Inv) -> Inv {
    match (&a, &b) {
        (Inv::C(0), _) | (_, Inv::C(0)) => Inv::C(0),
        (Inv::C(1), _) => b,
        (_, Inv::C(1)) => a,
        (Inv::C(x), Inv::C(y)) => Inv::C(x.wrapping_mul(*y)),
        _ => Inv::Mul(Box::new(a), Box::new(b)),
    }
}

/// `base + a * iv + b`
#[derive(Clone, Debug)]
struct Aff {
    iv: Val,
    a: Inv,
    b: Inv,
    base: Option<Val>,
    /// instructions per pass the chain up to here costs (scaling by
    /// 1/2/4/8 counts nothing -- it folds into the address)
    cost: u32,
}

fn int_ty(t: FTy) -> bool {
    matches!(
        t,
        FTy::I8 | FTy::I16 | FTy::I32 | FTy::I64 | FTy::U8 | FTy::U16 | FTy::U32 | FTy::U64
    )
}

pub fn run(f: &mut Func) -> usize {
    if std::env::var_os("FIRN_NO_IVSR").is_some() {
        return 0;
    }
    if f.constant_time || !f.secret.is_empty() {
        return 0;
    }
    let nb = f.blocks.len();
    if nb < 2 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    let any_backward = f
        .blocks
        .iter()
        .enumerate()
        .any(|(b, blk)| blk.term.successors().into_iter().any(|s| (s as usize) <= b));
    if !any_backward {
        return 0;
    }
    // cheap exit: without a header phi there is no induction variable (and
    // nothing for the two clean-ups either). Measured on `bin/firnc1.fi`:
    // the pass ran 6,128 times and changed something 9 times -- the full
    // analysis on every call was 1.2 s of 4 s optimizer time.
    if !f.blocks.iter().any(|b| b.has_phi()) {
        return 0;
    }
    // the two clean-ups only where this pass has put a phi before (they walk
    // the whole function and found nothing anywhere else)
    let mut total = if TOUCHED.with(|t| t.borrow().contains(&f.name)) {
        remove_dead_cycles(f) + merge_twins(f)
    } else {
        0
    };
    let dt = crate::mem2reg::idoms(f);
    let preds = dt.preds.clone();
    // does `a` dominate `b`? (climb the dominator tree from `b`)
    let dominates = |a: usize, b: usize| -> bool {
        let mut x = b;
        let mut guard = 0usize;
        loop {
            if x == a {
                return true;
            }
            let up = dt.idom[x] as usize;
            if up == x || guard > nb {
                return false;
            }
            x = up;
            guard += 1;
        }
    };
    let mut loops: Vec<(usize, usize, HashSet<usize>)> = Vec::new();
    for h in 0..nb {
        if !f.blocks[h].has_phi() || dt.rpo_num[h] == usize::MAX {
            continue;
        }
        let latches: Vec<usize> = preds[h]
            .iter()
            .copied()
            .filter(|&p| dt.rpo_num[p] != usize::MAX && dominates(h, p))
            .collect();
        if latches.len() != 1 {
            continue;
        }
        let body = crate::licm::natural_loop(h, latches[0], &preds);
        loops.push((h, latches[0], body));
    }
    loops.sort_by_key(|(_, _, body)| body.len());
    if loops.is_empty() {
        return total;
    }
    let mut maps: Option<(Vec<usize>, HashMap<Val, i128>)> = None;
    for (h, latch, body) in loops {
        let pre = match crate::licm::preheader_of(f, h, &body, &preds) {
            Some(p) => p,
            None => continue,
        };
        if maps.is_none() {
            maps = Some(def_maps(f));
        }
        let (defb, consts) = maps.as_ref().unwrap();
        let k = reduce_loop(f, h, latch, pre, &body, defb, consts);
        if k > 0 {
            total += k;
            maps = None; // new values: recompute before the next loop
        }
    }
    total
}

/// Where is each value defined, and which values are constants?
fn def_maps(f: &Func) -> (Vec<usize>, HashMap<Val, i128>) {
    let nv = f.val_types.len();
    let mut defb: Vec<usize> = vec![usize::MAX; nv];
    let mut consts: HashMap<Val, i128> = HashMap::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if (d as usize) < nv {
                    defb[d as usize] = bi;
                }
                if let Op::Const(c) = i.op {
                    consts.insert(d, c);
                }
            }
        }
    }
    (defb, consts)
}

fn reduce_loop(
    f: &mut Func,
    h: usize,
    latch: usize,
    pre: usize,
    body: &HashSet<usize>,
    defb: &[usize],
    consts: &HashMap<Val, i128>,
) -> usize {
    // params have no defining instruction: they are invariant everywhere
    let inside = |v: Val| -> bool {
        let d = defb.get(v as usize).copied().unwrap_or(usize::MAX);
        d != usize::MAX && body.contains(&d)
    };
    // an invariant operand as an expression (constants are copied, so a
    // constant defined inside the loop is fine too)
    let inv_of = |v: Val| -> Option<Inv> {
        if let Some(&c) = consts.get(&v) {
            return Some(Inv::C(c));
        }
        if inside(v) {
            None
        } else {
            Some(Inv::V(v))
        }
    };

    // 1. basic induction variables: header phi, back edge value phi +/- c
    let mut ivs: HashMap<Val, (Val, Inv, Val)> = HashMap::new(); // phi -> (init, step, next)
    let lat_id = latch as u32;
    let pre_id = pre as u32;
    let mut def_inst: HashMap<Val, (usize, usize)> = HashMap::new();
    for &bi in body.iter() {
        for (ii, i) in f.blocks[bi].insts.iter().enumerate() {
            if let Some(d) = i.dst {
                def_inst.insert(d, (bi, ii));
            }
        }
    }
    for i in &f.blocks[h].insts {
        let (d, inc) = match (&i.op, i.dst) {
            (Op::Phi { incoming }, Some(d)) if int_ty(i.ty) => (d, incoming),
            _ => continue,
        };
        if inc.len() != 2 {
            continue;
        }
        let init = match inc.iter().find(|(b, _)| *b == pre_id) {
            Some((_, v)) => *v,
            None => continue,
        };
        let next = match inc.iter().find(|(b, _)| *b == lat_id) {
            Some((_, v)) => *v,
            None => continue,
        };
        let (nb, ni) = match def_inst.get(&next) {
            Some(x) => *x,
            None => continue,
        };
        let ni = &f.blocks[nb].insts[ni];
        let step = match &ni.op {
            Op::Bin(BinOp::Add, a, b)
            | Op::BinWrapSat { kind: WrapSatKind::Wrap, op: BinOp::Add, a, b } => {
                if *a == d {
                    inv_of(*b)
                } else if *b == d {
                    inv_of(*a)
                } else {
                    None
                }
            }
            Op::Bin(BinOp::Sub, a, b)
            | Op::BinWrapSat { kind: WrapSatKind::Wrap, op: BinOp::Sub, a, b }
                if *a == d =>
            {
                inv_of(*b).map(|s| sub(Inv::C(0), s))
            }
            _ => None,
        };
        if let (Some(s), true) = (step, ni.ty == i.ty) {
            ivs.insert(d, (init, s, next));
        }
    }
    if ivs.is_empty() {
        return 0;
    }

    // 2. affine values, in dominance order (block order within the body is
    // not dominance order in general, so iterate to a fixpoint)
    let mut aff: HashMap<Val, Aff> = HashMap::new();
    for (&p, _) in ivs.iter() {
        aff.insert(p, Aff { iv: p, a: Inv::C(1), b: Inv::C(0), base: None, cost: 0 });
    }
    let mut order: Vec<usize> = body.iter().copied().collect();
    order.sort();
    loop {
        let mut grew = false;
        for &bi in &order {
            for i in &f.blocks[bi].insts {
                let d = match i.dst {
                    Some(d) if !aff.contains_key(&d) => d,
                    _ => continue,
                };
                let wrap_or_plain = |op: &Op| -> Option<(BinOp, Val, Val)> {
                    match op {
                        Op::Bin(o, a, b) => Some((*o, *a, *b)),
                        Op::BinWrapSat { kind: WrapSatKind::Wrap, op, a, b } => Some((*op, *a, *b)),
                        _ => None,
                    }
                };
                let r: Option<Aff> = if let Op::PtrAdd { base, off } = &i.op {
                    match (inv_of(*base), aff.get(off)) {
                        (Some(Inv::V(bv)), Some(x)) if x.base.is_none() => Some(Aff {
                            iv: x.iv,
                            a: x.a.clone(),
                            b: x.b.clone(),
                            base: Some(bv),
                            cost: x.cost + 1,
                        }),
                        _ => None,
                    }
                } else if let Some((o, a, b)) = wrap_or_plain(&i.op) {
                    if !int_ty(i.ty) {
                        None
                    } else {
                        let (xa, xb) = (aff.get(&a), aff.get(&b));
                        let (ia, ib) = (inv_of(a), inv_of(b));
                        let same = |x: &Aff| x.base.is_none() && f.val_ty(x.iv) == i.ty;
                        match o {
                            BinOp::Add => match (xa, xb) {
                                (Some(x), None) if same(x) => ib.map(|v| Aff {
                                    b: add(x.b.clone(), v),
                                    cost: x.cost + 1,
                                    ..x.clone()
                                }),
                                (None, Some(y)) if same(y) => ia.map(|v| Aff {
                                    b: add(v, y.b.clone()),
                                    cost: y.cost + 1,
                                    ..y.clone()
                                }),
                                (Some(x), Some(y)) if same(x) && same(y) && x.iv == y.iv => {
                                    Some(Aff {
                                        iv: x.iv,
                                        a: add(x.a.clone(), y.a.clone()),
                                        b: add(x.b.clone(), y.b.clone()),
                                        base: None,
                                        cost: x.cost + y.cost + 1,
                                    })
                                }
                                _ => None,
                            },
                            BinOp::Sub => match (xa, xb) {
                                (Some(x), None) if same(x) => ib.map(|v| Aff {
                                    b: sub(x.b.clone(), v),
                                    cost: x.cost + 1,
                                    ..x.clone()
                                }),
                                (None, Some(y)) if same(y) => ia.map(|v| Aff {
                                    iv: y.iv,
                                    a: sub(Inv::C(0), y.a.clone()),
                                    b: sub(v, y.b.clone()),
                                    base: None,
                                    cost: y.cost + 1,
                                }),
                                (Some(x), Some(y)) if same(x) && same(y) && x.iv == y.iv => {
                                    Some(Aff {
                                        iv: x.iv,
                                        a: sub(x.a.clone(), y.a.clone()),
                                        b: sub(x.b.clone(), y.b.clone()),
                                        base: None,
                                        cost: x.cost + y.cost + 1,
                                    })
                                }
                                _ => None,
                            },
                            BinOp::Mul => {
                                let (x, v) = match (xa, xb, &ia, &ib) {
                                    (Some(x), None, _, Some(v)) => (x, v.clone()),
                                    (None, Some(y), Some(v), _) => (y, v.clone()),
                                    _ => continue,
                                };
                                if !same(x) {
                                    None
                                } else {
                                    let cheap = matches!(v, Inv::C(1) | Inv::C(2) | Inv::C(4) | Inv::C(8));
                                    Some(Aff {
                                        iv: x.iv,
                                        a: mul(x.a.clone(), v.clone()),
                                        b: mul(x.b.clone(), v),
                                        base: None,
                                        cost: x.cost + if cheap { 0 } else { 1 },
                                    })
                                }
                            }
                            BinOp::Shl => match (xa, consts.get(&b)) {
                                (Some(x), Some(&k))
                                    if same(x) && k >= 0 && (k as u32) < i.ty.bits() =>
                                {
                                    let m = Inv::C(1i128 << k);
                                    Some(Aff {
                                        iv: x.iv,
                                        a: mul(x.a.clone(), m.clone()),
                                        b: mul(x.b.clone(), m),
                                        base: None,
                                        cost: x.cost + if k <= 3 { 0 } else { 1 },
                                    })
                                }
                                _ => None,
                            },
                            _ => None,
                        }
                    }
                } else {
                    None
                };
                if let Some(x) = r {
                    aff.insert(d, x);
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }

    // nothing costs two instructions per pass: nothing to gain (the common
    // case -- checked before the scan of the whole function below)
    if !aff.values().any(|x| x.cost >= 2) {
        return 0;
    }
    // 3. roots: affine, with a multiplication, used by something non-affine
    let mut used_outside: HashSet<Val> = HashSet::new();
    let mut no_root: HashSet<Val> = HashSet::new();
    let mut buf = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for i in &b.insts {
            let consumer_affine = i.dst.map(|d| aff.contains_key(&d)).unwrap_or(false)
                && body.contains(&bi);
            if consumer_affine {
                continue;
            }
            buf.clear();
            i.op.uses(&mut buf);
            // a CHECKED reader needs the range of its operand (`bce` proves
            // from it that the check cannot fire); a phi in its place has
            // none -- such a value stays as it is
            //
            // A CALL argument neither: the optimizer runs once over every
            // function before `inline`, and a chain reduced there is gone by
            // the time the call is embedded and the whole address becomes
            // visible (`matmul`: `ld32(b, k * n + cc)`, at `release-safe`
            // the `* 4` inside `ld32` then stayed checked, +13 %).
            let checked = matches!(
                i.op,
                Op::CheckedBin { .. }
                    | Op::CheckedDiv { .. }
                    | Op::CheckedCast { .. }
                    | Op::CheckedIdx { .. }
                    | Op::Call { .. }
                    | Op::CallIndirect { .. }
            );
            for u in &buf {
                if aff.contains_key(u) {
                    if checked {
                        no_root.insert(*u);
                    }
                    used_outside.insert(*u);
                }
            }
        }
        let tv: Option<Val> = match &b.term {
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            Term::Ret(Some(v)) => Some(*v),
            _ => None,
        };
        if let Some(v) = tv {
            if aff.contains_key(&v) {
                used_outside.insert(v);
            }
        }
    }
    let mut roots: Vec<Val> = used_outside
        .into_iter()
        .filter(|v| {
            let x = &aff[v];
            worth(f, *v, x) && !ivs.contains_key(v) && !no_root.contains(v) && !ivs.values().any(|(_, _, n)| n == v)
                // a value used after the loop keeps its old definition (the
                // phi would hold the value of the NEXT pass there)
                && !used_after_loop(f, *v, body)
        })
        .collect();
    roots.sort();
    if roots.is_empty() {
        return 0;
    }

    // 4. rewrite
    let trace = std::env::var_os("FIRN_IVSR_TRACE").is_some();
    let mut made: Vec<(String, Val)> = Vec::new();
    let mut map: HashMap<Val, Val> = HashMap::new();
    let pre_loc = f.blocks[pre].insts.last().map(|i| i.loc).unwrap_or(crate::fir::Loc::NONE);
    let mut new_pre: Vec<Inst> = Vec::new();
    let mut new_latch: Vec<Inst> = Vec::new();
    let mut new_phis: Vec<Inst> = Vec::new();
    for r in roots {
        let x = aff[&r].clone();
        let key = format!("{:?}", (x.iv, &x.a, &x.b, x.base, f.val_ty(r)));
        if let Some((_, q)) = made.iter().find(|(k, _)| *k == key) {
            map.insert(r, *q);
            continue;
        }
        if made.len() >= MAX_NEW_PHIS {
            break;
        }
        let ity = f.val_ty(x.iv);
        let rty = f.val_ty(r);
        let (init, step, _) = ivs[&x.iv].clone();
        // q0 = base + a * init + b ; step' = a * step
        let e_init = add(mul(x.a.clone(), Inv::V(init)), x.b.clone());
        let e_step = mul(x.a.clone(), step);
        let v_init = emit(f, &mut new_pre, &e_init, ity, pre_loc);
        let v_step = emit(f, &mut new_pre, &e_step, ity, pre_loc);
        let q0 = match x.base {
            Some(bv) => {
                let q = f.new_val_pub(rty);
                new_pre.push(Inst::like(Some(q), rty, Op::PtrAdd { base: bv, off: v_init }, pre_loc));
                q
            }
            None => v_init,
        };
        let q = f.new_val_pub(rty);
        let qn = f.new_val_pub(rty);
        let op_next = match x.base {
            Some(_) => Op::PtrAdd { base: q, off: v_step },
            None => Op::BinWrapSat { kind: WrapSatKind::Wrap, op: BinOp::Add, a: q, b: v_step },
        };
        let lat_loc = f.blocks[latch].insts.last().map(|i| i.loc).unwrap_or(pre_loc);
        new_latch.push(Inst::like(Some(qn), rty, op_next, lat_loc));
        let mut inc = vec![(pre as u32, q0), (latch as u32, qn)];
        inc.sort_by_key(|(b, _)| *b);
        new_phis.push(Inst::like(Some(q), rty, Op::Phi { incoming: inc }, pre_loc));
        if trace {
            eprintln!("IVSR {} root=%{} -> phi %{} (iv %{}, a={:?}, b={:?}, base={:?})",
                f.name, r, q, x.iv, x.a, x.b, x.base);
        }
        made.push((key, q));
        map.insert(r, q);
    }
    if map.is_empty() {
        return 0;
    }
    TOUCHED.with(|t| {
        t.borrow_mut().insert(f.name.clone());
    });
    // the preheader code goes in front of its terminator (a block's insts
    // never contain the terminator), the phis at the top of the header, the
    // steps at the end of the latch
    f.blocks[pre].insts.extend(new_pre);
    let hp = f.blocks[h].phi_count();
    for (k, p) in new_phis.into_iter().enumerate() {
        f.blocks[h].insts.insert(hp + k, p);
    }
    f.blocks[latch].insts.extend(new_latch);
    // rewrite the uses of the roots, but not inside the new step code
    crate::mem2reg::replace_uses(f, &map);
    map.len()
}

/// Does replacing the chain by one `add` per pass save anything? The chain
/// costs `x.cost`; when its last step is an addition and every reader is a
/// load or store address, that addition folds into the address mode
/// (`[base + index*4]`) and costs nothing either.
fn worth(f: &Func, r: Val, x: &Aff) -> bool {
    let mut cost = x.cost;
    let mut last_add = false;
    let mut all_addr = true;
    let mut buf = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            if i.dst == Some(r) {
                last_add = matches!(
                    i.op,
                    Op::PtrAdd { .. }
                        | Op::Bin(BinOp::Add, ..)
                        | Op::BinWrapSat { op: BinOp::Add, .. }
                );
            }
            buf.clear();
            i.op.uses(&mut buf);
            if buf.contains(&r) {
                let addr_only = match &i.op {
                    Op::Load { addr } => *addr == r,
                    Op::Store { addr, val } => *addr == r && *val != r,
                    _ => false,
                };
                all_addr &= addr_only;
            }
        }
    }
    if last_add && all_addr && cost > 0 {
        cost -= 1;
    }
    cost >= 2
}

fn used_after_loop(f: &Func, v: Val, body: &HashSet<usize>) -> bool {
    let mut buf = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        if body.contains(&bi) {
            continue;
        }
        for i in &b.insts {
            buf.clear();
            i.op.uses(&mut buf);
            if buf.contains(&v) {
                return true;
            }
        }
        let tv: Option<Val> = match &b.term {
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            Term::Ret(Some(x)) => Some(*x),
            _ => None,
        };
        if tv == Some(v) {
            return true;
        }
    }
    false
}

fn emit(f: &mut Func, out: &mut Vec<Inst>, e: &Inv, ty: FTy, loc: crate::fir::Loc) -> Val {
    match e {
        Inv::V(v) => *v,
        Inv::C(c) => {
            let d = f.new_val_pub(ty);
            out.push(Inst::like(Some(d), ty, Op::Const(ty.truncate(*c)), loc));
            d
        }
        Inv::Add(a, b) | Inv::Sub(a, b) | Inv::Mul(a, b) => {
            let va = emit(f, out, a, ty, loc);
            let vb = emit(f, out, b, ty, loc);
            let op = match e {
                Inv::Add(..) => BinOp::Add,
                Inv::Sub(..) => BinOp::Sub,
                _ => BinOp::Mul,
            };
            let d = f.new_val_pub(ty);
            out.push(Inst::like(
                Some(d),
                ty,
                Op::BinWrapSat { kind: WrapSatKind::Wrap, op, a: va, b: vb },
                loc,
            ));
            d
        }
    }
}

/// A phi whose only reader is its own step, and a step whose only reader is
/// the phi: a dead induction cycle. `dce` cannot see it (each keeps the other
/// alive), and this pass produces them -- a root reduced before `inline`
/// made its chain visible gets reduced again afterwards, and the first phi
/// is left over. Removes both instructions; returns how many cycles.
fn remove_dead_cycles(f: &mut Func) -> usize {
    // candidates first: a phi and a back edge value `phi + x` / `ptradd phi, x`
    let nv = f.val_types.len();
    let mut is_phi = vec![false; nv];
    let mut any = false;
    for b in &f.blocks {
        for i in &b.insts[..b.phi_count()] {
            if let Some(d) = i.dst {
                if (d as usize) < nv {
                    is_phi[d as usize] = true;
                    any = true;
                }
            }
        }
    }
    if !any {
        return 0;
    }
    // step value -> the phi it steps
    let mut step_of: HashMap<Val, Val> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                let p = match &i.op {
                    Op::Bin(BinOp::Add, a, _)
                    | Op::Bin(BinOp::Sub, a, _)
                    | Op::BinWrapSat { kind: WrapSatKind::Wrap, op: BinOp::Add, a, .. }
                    | Op::BinWrapSat { kind: WrapSatKind::Wrap, op: BinOp::Sub, a, .. } => *a,
                    Op::PtrAdd { base, .. } => *base,
                    _ => continue,
                };
                if (p as usize) < nv && is_phi[p as usize] {
                    step_of.insert(d, p);
                }
            }
        }
    }
    if step_of.is_empty() {
        return 0;
    }
    let mut interesting = vec![false; nv];
    for (&n, &p) in &step_of {
        interesting[n as usize] = true;
        interesting[p as usize] = true;
    }
    let mut uses = vec![0u32; nv];
    let mut buf = Vec::new();
    let mut count = |v: Val, uses: &mut Vec<u32>| {
        if (v as usize) < nv && interesting[v as usize] {
            uses[v as usize] += 1;
        }
    };
    for b in &f.blocks {
        for i in &b.insts {
            buf.clear();
            i.op.uses(&mut buf);
            for u in &buf {
                count(*u, &mut uses);
            }
        }
        match &b.term {
            Term::BrCond { cond, .. } => count(*cond, &mut uses),
            Term::Switch { val, .. } => count(*val, &mut uses),
            Term::Ret(Some(v)) => count(*v, &mut uses),
            _ => {}
        }
    }
    let mut dead: HashSet<Val> = HashSet::new();
    for b in &f.blocks {
        for i in &b.insts[..b.phi_count()] {
            if let (Op::Phi { incoming }, Some(p)) = (&i.op, i.dst) {
                if uses[p as usize] != 1 {
                    continue;
                }
                for (_, n) in incoming {
                    if step_of.get(n) == Some(&p) && uses[*n as usize] == 1 {
                        dead.insert(p);
                        dead.insert(*n);
                    }
                }
            }
        }
    }
    if dead.is_empty() {
        return 0;
    }
    for b in f.blocks.iter_mut() {
        b.insts.retain(|i| !i.dst.map(|d| dead.contains(&d)).unwrap_or(false));
    }
    dead.len() / 2
}

/// Two header phis with the same entry value that both step by the same
/// invariant (`p1' = p1 + s`, `p2' = p2 + s`) hold the same value in every
/// pass. `cse` cannot see it (the step instructions read different phis);
/// this pass makes such twins when a chain is reduced once before `inline`
/// and its copy again after. The second twin is replaced by the first.
fn merge_twins(f: &mut Func) -> usize {
    // only the back edge values of phis are of interest
    let mut want: HashSet<Val> = HashSet::new();
    for b in &f.blocks {
        for i in &b.insts[..b.phi_count()] {
            if let Op::Phi { incoming } = &i.op {
                for (_, v) in incoming {
                    want.insert(*v);
                }
            }
        }
    }
    let mut def: HashMap<Val, Op> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if want.contains(&d) && matches!(i.op, Op::Bin(..) | Op::BinWrapSat { .. } | Op::PtrAdd { .. }) {
                    def.insert(d, i.op.clone());
                }
            }
        }
    }
    let step = |n: Val, p: Val| -> Option<(u8, Val)> {
        match def.get(&n)? {
            Op::Bin(BinOp::Add, a, b) | Op::BinWrapSat { kind: WrapSatKind::Wrap, op: BinOp::Add, a, b } => {
                if *a == p {
                    Some((0, *b))
                } else {
                    None
                }
            }
            Op::PtrAdd { base, off } if *base == p => Some((1, *off)),
            _ => None,
        }
    };
    let mut map: HashMap<Val, Val> = HashMap::new();
    for b in &f.blocks {
        // (entries with the latch value replaced by its step) -> phi
        let mut seen: Vec<(Vec<(u32, Val, Option<(u8, Val)>)>, FTy, Val)> = Vec::new();
        for i in &b.insts {
            let (p, inc) = match (&i.op, i.dst) {
                (Op::Phi { incoming }, Some(p)) => (p, incoming),
                _ => continue,
            };
            let key: Vec<(u32, Val, Option<(u8, Val)>)> = inc
                .iter()
                .map(|(bb, v)| match step(*v, p) {
                    Some(st) => (*bb, Val::MAX, Some(st)),
                    None => (*bb, *v, None),
                })
                .collect();
            // at least one entry must be a step of the phi itself
            if !key.iter().any(|(_, _, s)| s.is_some()) {
                continue;
            }
            if let Some((_, _, q)) = seen.iter().find(|(k, t, _)| *k == key && *t == i.ty) {
                map.insert(p, *q);
            } else {
                seen.push((key, i.ty, p));
            }
        }
    }
    if map.is_empty() {
        return 0;
    }
    let n = map.len();
    crate::mem2reg::replace_uses(f, &map);
    // Nobody reads the twin now; its step reads `q` and is a copy of `q`'s
    // own step (`cse`), and `dce` removes the unread phi and then the step.
    n
}
