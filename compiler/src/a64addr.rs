// SPDX-License-Identifier: GPL-2.0-only
use crate::fir::{BinOp, FTy, Func, Op, Val};
use crate::fx::{HashMap, HashSet};

pub(crate) enum Shape {
    Imm(i64),
    Index { idx: Val, shift: u32 },
}

pub(crate) struct Folded {
    pub base: Val,
    pub shape: Shape,
}

#[derive(Default)]
pub(crate) struct AddrFold {
    pub access: HashMap<Val, Folded>,
    pub dead: HashSet<Val>,
}

impl AddrFold {
    pub(crate) fn skips(&self, dst: Option<Val>) -> bool {
        match dst {
            Some(d) => self.dead.contains(&d),
            None => false,
        }
    }
}

fn size_log2(ty: FTy) -> Option<u32> {
    match ty.bits().max(8) {
        8 => Some(0),
        16 => Some(1),
        32 => Some(2),
        64 => Some(3),
        _ => None,
    }
}

fn imm_fits(c: i64, sh: u32) -> bool {
    let size = 1i64 << sh;
    (c >= 0 && c % size == 0 && c / size <= 4095) || (-256..=255).contains(&c)
}

pub(crate) fn analyse(f: &Func, consts: &HashMap<Val, i128>) -> AddrFold {
    let mut out = AddrFold::default();
    if std::env::var_os("FIRN_A64_NO_ADDRFOLD").is_some() || !f.secret.is_empty() || f.constant_time {
        return out;
    }
    let uses = crate::usecount::counts(f);
    let mut defs = vec![0u32; f.val_types.len()];
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if let Some(c) = defs.get_mut(d as usize) {
                    *c += 1;
                }
            }
        }
    }
    let once = |v: Val| uses.get(v as usize) == Some(&1) && defs.get(v as usize) == Some(&1);
    for b in &f.blocks {
        for k in 1..b.insts.len() {
            let acc = &b.insts[k];
            let addr = match &acc.op {
                Op::Load { addr } => *addr,
                Op::Store { addr, val } if val != addr => *addr,
                _ => continue,
            };
            let sh = match size_log2(acc.ty) {
                Some(s) => s,
                None => continue,
            };
            let pa = &b.insts[k - 1];
            if pa.dst != Some(addr) || !once(addr) {
                continue;
            }
            let (base, off) = match &pa.op {
                Op::PtrAdd { base, off } => (*base, *off),
                _ => continue,
            };
            if defs.get(off as usize) == Some(&1) {
                if let Some(c) = consts.get(&off) {
                    let c = *c as i64;
                    if imm_fits(c, sh) {
                        out.access.insert(addr, Folded { base, shape: Shape::Imm(c) });
                        out.dead.insert(addr);
                    }
                    continue;
                }
            }
            if k >= 2 && b.insts[k - 2].dst == Some(off) && once(off) && b.insts[k - 2].ty.bits() == 64 {
                let scaled = match &b.insts[k - 2].op {
                    Op::Bin(BinOp::Shl, x, kc) => consts.get(kc).filter(|c| **c == sh as i128).map(|_| *x),
                    Op::Bin(BinOp::Mul, x, kc) => consts.get(kc).filter(|c| **c == (1i128 << sh)).map(|_| *x),
                    _ => None,
                };
                if let Some(x) = scaled {
                    if x != addr && x != off {
                        out.access.insert(addr, Folded { base, shape: Shape::Index { idx: x, shift: sh } });
                        out.dead.insert(addr);
                        out.dead.insert(off);
                        continue;
                    }
                }
            }
            if sh == 0 && off != addr && off != base {
                out.access.insert(addr, Folded { base, shape: Shape::Index { idx: off, shift: 0 } });
                out.dead.insert(addr);
            }
        }
    }
    out
}
