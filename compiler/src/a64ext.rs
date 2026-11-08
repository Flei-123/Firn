// SPDX-License-Identifier: GPL-2.0-only
use crate::fir::{Func, Op, Val};
use crate::fx::HashMap;

pub(crate) fn zero_extended_loads(f: &Func) -> HashMap<Val, u32> {
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
    let mut out = HashMap::default();
    for b in &f.blocks {
        for i in &b.insts {
            let d = match (&i.op, i.dst) {
                (Op::Load { .. }, Some(d)) if !i.ty.is_float() && i.ty.bits() <= 16 => d,
                _ => continue,
            };
            if defs.get(d as usize) == Some(&1) {
                out.insert(d, i.ty.bits().max(8));
            }
        }
    }
    out
}
