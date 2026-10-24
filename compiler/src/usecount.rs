// SPDX-License-Identifier: MPL-2.0
use crate::fir::{Func, Term};

pub(crate) fn counts(f: &Func) -> Vec<u32> {
    let mut uses = vec![0u32; f.val_types.len()];
    let mut buf = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            buf.clear();
            i.op.uses(&mut buf);
            for u in &buf {
                if let Some(c) = uses.get_mut(*u as usize) {
                    *c += 1;
                }
            }
        }
        let tv = match &b.term {
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            Term::Ret(Some(v)) => Some(*v),
            _ => None,
        };
        if let Some(v) = tv {
            if let Some(c) = uses.get_mut(v as usize) {
                *c += 1;
            }
        }
    }
    uses
}
