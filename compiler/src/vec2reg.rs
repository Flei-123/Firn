// SPDX-License-Identifier: MPL-2.0
//! **Round TEMPO 15** -- a sixteen octet stack cell that is only ever used as
//! ONE vector becomes a `v128` value (`vec2reg`).
//!
//! ## Why
//!
//! SIMD code talks to scalar code through small arrays:
//!
//! ```firn
//! var ai: [i32; 4] = [0; 4]
//! __v128_store((&ai[0]) as *mut u8, v)
//! s16(dst, x, ai[1] as i16)
//! ```
//!
//! `mem2reg` cannot touch such a cell -- it is written sixteen octets wide and
//! read four octets wide, so it is not one scalar. The code generator then
//! writes the zeroes, the vector and reads the lanes back through memory, and
//! (after `licm` hoisted the element addresses) reloads every address from
//! the frame first. Measured in the MP3 decoder (`synth`, `lib/ton/mp3.fi`):
//! four such arrays per pass cost 16 zero stores, 4 vector stores, 2 vector
//! loads and 8 lane loads, each behind an address reload.
//!
//! ## What is done
//!
//! An `alloca` of exactly sixteen octets qualifies when
//!
//!  * its address is used ONLY as the address of `load`/`store` (four octet
//!    `i32`/`u32`/`f32`, at offset 0, 4, 8 or 12 through a `ptradd` with a
//!    constant) or of `simd.Load`/`simd.Store` (offset 0) -- never stored
//!    itself, never passed to a call, never compared;
//!  * every access stands in ONE block.
//!
//! The block is then walked in order with the cell's content as a value:
//! a vector store sets it, a vector load reads it, a lane load becomes
//! `simd.Shuffle32` + `simd.GetU32` (integer lanes; `pshufd` + `movd`, both
//! SSE2) and a zero stored into a lane is remembered as such. A load that the walk cannot answer (the cell not yet written in
//! this block, a float lane of a vector, a lane written with something other
//! than an integer or zero) leaves the whole cell alone. Because the walk
//! starts from "unknown" at the top of the block, a loop body that writes the
//! cell before reading it is handled, one that reads last pass's value is
//! not.
//!
//! Afterwards all stores to the cell are gone (nobody reads it any more) and
//! `dce` removes the `alloca` and its addresses.
//!
//! `FIRN_NO_VEC2REG=1` switches it off (for measuring).

use crate::fir::{FTy, Func, Inst, Op, Term, Val};
use crate::simd::SimdKind;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq)]
enum St {
    /// nothing known (start of the block)
    Unknown,
    /// the lanes in the mask were written with zero, the others unknown
    Zero(u8),
    /// the whole cell holds this `v128`
    Vec(Val),
}

#[derive(Clone, Copy)]
enum Acc {
    /// scalar load of lane `l`, type `ty`
    LoadLane(u8, FTy),
    /// scalar store of `val` into lane `l`
    StoreLane(u8, Val),
    VLoad,
    VStore(Val),
}

fn four_octets(t: FTy) -> bool {
    matches!(t, FTy::I32 | FTy::U32 | FTy::F32)
}

pub fn run(f: &mut Func) -> usize {
    if std::env::var_os("FIRN_NO_VEC2REG").is_some() {
        return 0;
    }
    if f.constant_time || !f.secret.is_empty() || f.blocks.is_empty() {
        return 0;
    }
    let mut total = 0;
    let cells: Vec<Val> = f.blocks[0]
        .insts
        .iter()
        .filter_map(|i| match (&i.op, i.dst) {
            (Op::Alloca { size: 16, .. }, Some(d)) => Some(d),
            _ => None,
        })
        .collect();
    let mut consts: Option<HashMap<Val, i128>> = None;
    for c in cells {
        if consts.is_none() {
            consts = Some(const_of(f));
        }
        if promote_cell(f, c, consts.as_ref().unwrap()) {
            total += 1;
            consts = None; // new values: count again for the next cell
        }
    }
    total + narrow_lanes(f)
}

/// `(get_u32(shuffle32(v, l), 0) as i16)` -> `get_u16(v, 2 l)` (`pextrw`,
/// SSE2): the 16-bit lane `2 l` IS the low half of the 32-bit lane `l`, and
/// the narrowing keeps exactly that half. Five instructions per sample in
/// `synth` (`pshufd`, `movd`, two moves, `movsxd`) become one.
fn narrow_lanes(f: &mut Func) -> usize {
    let mut def: HashMap<Val, (SimdKind, Val, u8)> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Op::Simd { kind, args, imm }, Some(d)) = (&i.op, i.dst) {
                if matches!(kind, SimdKind::GetU32 | SimdKind::Shuffle32) && args.len() == 1 {
                    def.insert(d, (*kind, args[0], *imm));
                }
            }
        }
    }
    if def.is_empty() {
        return 0;
    }
    let mut n = 0;
    for b in f.blocks.iter_mut() {
        for i in b.insts.iter_mut() {
            let (src, from) = match &i.op {
                Op::Cast { src, from } => (*src, *from),
                _ => continue,
            };
            if !matches!(from, FTy::I32 | FTy::U32) || !matches!(i.ty, FTy::I16 | FTy::U16) {
                continue;
            }
            let (x, lane) = match def.get(&src) {
                Some(&(SimdKind::GetU32, x, 0)) => match def.get(&x) {
                    Some(&(SimdKind::Shuffle32, y, s)) => (y, s & 3),
                    _ => (x, 0),
                },
                Some(&(SimdKind::GetU32, x, l)) => (x, l & 3),
                _ => continue,
            };
            i.op = Op::Simd { kind: SimdKind::GetU16, args: vec![x], imm: 2 * lane };
            n += 1;
        }
    }
    n
}

fn const_of(f: &Func) -> HashMap<Val, i128> {
    let mut m = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Op::Const(k), Some(d)) = (&i.op, i.dst) {
                m.insert(d, *k);
            }
        }
    }
    m
}

fn promote_cell(f: &mut Func, cell: Val, consts: &HashMap<Val, i128>) -> bool {
    // 1. the addresses: the cell itself and `ptradd cell, const`
    let mut lane_of: HashMap<Val, u8> = HashMap::new();
    lane_of.insert(cell, 0);
    for b in &f.blocks {
        for i in &b.insts {
            if let (Op::PtrAdd { base, off }, Some(d)) = (&i.op, i.dst) {
                if *base == cell {
                    match consts.get(off) {
                        Some(&k) if k >= 0 && k <= 12 && k % 4 == 0 => {
                            lane_of.insert(d, (k / 4) as u8);
                        }
                        _ => return false,
                    }
                }
            }
        }
    }
    // 2. every use must be an access of the recognised form, all in one block
    let mut block: Option<usize> = None;
    let mut uses = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for i in &b.insts {
            // the derivation itself
            if let Op::PtrAdd { base, .. } = &i.op {
                if *base == cell {
                    continue;
                }
            }
            uses.clear();
            i.op.uses(&mut uses);
            if !uses.iter().any(|u| lane_of.contains_key(u)) {
                continue;
            }
            let ok = match &i.op {
                Op::Load { addr } => lane_of.contains_key(addr) && four_octets(i.ty),
                Op::Store { addr, val } => {
                    lane_of.contains_key(addr)
                        && !lane_of.contains_key(val)
                        && four_octets(f.val_ty(*val))
                }
                Op::Simd { kind: SimdKind::Load, args, .. } => {
                    args.len() == 1 && lane_of.get(&args[0]) == Some(&0)
                }
                Op::Simd { kind: SimdKind::Store, args, .. } => {
                    args.len() == 2
                        && lane_of.get(&args[0]) == Some(&0)
                        && !lane_of.contains_key(&args[1])
                }
                _ => false,
            };
            if !ok {
                return false;
            }
            match block {
                None => block = Some(bi),
                Some(x) if x == bi => {}
                _ => return false,
            }
        }
        let tv: Option<Val> = match &b.term {
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            Term::Ret(Some(v)) => Some(*v),
            _ => None,
        };
        if let Some(v) = tv {
            if lane_of.contains_key(&v) {
                return false;
            }
        }
    }
    let bi = match block {
        Some(b) => b,
        None => return false,
    };
    // 3. dry run: can every read be answered?
    let accs: Vec<(usize, Acc)> = f.blocks[bi]
        .insts
        .iter()
        .enumerate()
        .filter_map(|(k, i)| {
            let a = match &i.op {
                Op::Load { addr } => Acc::LoadLane(*lane_of.get(addr)?, i.ty),
                Op::Store { addr, val } => Acc::StoreLane(*lane_of.get(addr)?, *val),
                Op::Simd { kind: SimdKind::Load, args, .. } if lane_of.contains_key(&args[0]) => {
                    Acc::VLoad
                }
                Op::Simd { kind: SimdKind::Store, args, .. } if lane_of.contains_key(&args[0]) => {
                    Acc::VStore(args[1])
                }
                _ => return None,
            };
            Some((k, a))
        })
        .collect();
    let is_zero = |v: Val| consts.get(&v) == Some(&0);
    let int_lane = |t: FTy| matches!(t, FTy::I32 | FTy::U32);
    {
        let mut st = St::Unknown;
        for (_, a) in &accs {
            st = match (*a, st) {
                (Acc::VStore(v), _) => St::Vec(v),
                (Acc::VLoad, St::Vec(_)) => st,
                (Acc::VLoad, St::Zero(0xF)) => st,
                (Acc::VLoad, _) => return false,
                (Acc::LoadLane(l, _), St::Zero(m)) if m & (1 << l) != 0 => st,
                (Acc::LoadLane(_, t), St::Vec(_)) if int_lane(t) => st,
                (Acc::LoadLane(..), _) => return false,
                (Acc::StoreLane(l, v), St::Unknown) if is_zero(v) => St::Zero(1 << l),
                (Acc::StoreLane(l, v), St::Zero(m)) if is_zero(v) => St::Zero(m | (1 << l)),
                // a real lane write into a known vector would need `pinsrd`
                // (SSE4.1) -- the baseline is SSE2, so such a cell stays
                (Acc::StoreLane(..), _) => return false,
            };
        }
    }
    // 4. the rewrite
    let old = std::mem::take(&mut f.blocks[bi].insts);
    let mut out: Vec<Inst> = Vec::with_capacity(old.len());
    let mut map: HashMap<Val, Val> = HashMap::new();
    let mut st = St::Unknown;
    let mut ai = 0usize;
    for (k, inst) in old.into_iter().enumerate() {
        if ai >= accs.len() || accs[ai].0 != k {
            out.push(inst);
            continue;
        }
        let a = accs[ai].1;
        ai += 1;
        let loc = inst.loc;
        match a {
            Acc::VStore(v) => st = St::Vec(*map.get(&v).unwrap_or(&v)),
            Acc::StoreLane(l, v) => match st {
                St::Unknown => st = St::Zero(1 << l),
                St::Zero(m) => st = St::Zero(m | (1 << l)),
                St::Vec(_) => unreachable!("vec2reg: lane write into a vector passed the dry run ({})", v),
            },
            Acc::VLoad => {
                let d = inst.dst.expect("simd.Load has a result");
                match st {
                    St::Vec(x) => {
                        map.insert(d, x);
                    }
                    _ => {
                        // Zero(0xF): the four lanes are zero
                        let nv = f.new_val_pub(FTy::V128);
                        out.push(Inst::like(
                            Some(nv),
                            FTy::V128,
                            Op::Simd { kind: SimdKind::Zero, args: vec![], imm: 0 },
                            loc,
                        ));
                        st = St::Vec(nv);
                        map.insert(d, nv);
                    }
                }
            }
            Acc::LoadLane(l, t) => {
                let d = inst.dst;
                match st {
                    St::Vec(x) => {
                        // `pextrd` is SSE4.1; the baseline is SSE2. So lane
                        // `l` is first shuffled to the bottom (`pshufd`) and
                        // then moved out (`movd`) -- two instructions, both
                        // SSE2, and `cse` merges a shuffle used twice.
                        let src = if l == 0 {
                            x
                        } else {
                            let sv = f.new_val_pub(FTy::V128);
                            out.push(Inst::like(
                                Some(sv),
                                FTy::V128,
                                Op::Simd { kind: SimdKind::Shuffle32, args: vec![x], imm: l },
                                loc,
                            ));
                            sv
                        };
                        out.push(Inst::like(
                            d,
                            t,
                            Op::Simd { kind: SimdKind::GetU32, args: vec![src], imm: 0 },
                            loc,
                        ))
                    }
                    _ => out.push(Inst::like(d, t, Op::Const(0), loc)),
                }
            }
        }
    }
    f.blocks[bi].insts = out;
    // chains cannot arise inside one cell: a mapped value is always a value
    // that was stored, never one of this cell's own loads
    crate::mem2reg::replace_uses(f, &map);
    true
}
