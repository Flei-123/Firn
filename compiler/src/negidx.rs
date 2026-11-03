// SPDX-License-Identifier: MPL-2.0
use crate::fir::{FTy, Func, Op};

pub(crate) fn run(f: &mut Func) -> usize {
    if std::env::var_os("FIRN_NO_NEGIDX").is_some() {
        return 0;
    }
    let hits = crate::rangecheck::nonneg_signed_index_masks(f);
    for &(bi, ii, s) in &hits {
        let inst = &mut f.blocks[bi].insts[ii];
        inst.op = Op::Cast { src: s, from: FTy::I64 };
    }
    hits.len()
}
