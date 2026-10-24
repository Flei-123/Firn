// SPDX-License-Identifier: MPL-2.0
use crate::fir::{BinOp, FTy, Func, Inst, Op};
use crate::fx::HashMap;

fn log2_exact(v: i128) -> Option<u32> {
    if v <= 1 || v & (v - 1) != 0 {
        return None;
    }
    Some(v.trailing_zeros())
}

struct Edit {
    block: usize,
    at: usize,
    shift: bool,
    ty: FTy,
    a: u32,
    dst: u32,
    imm: i128,
}

pub(crate) fn run(f: &mut Func) -> usize {
    if std::env::var_os("FIRN_NO_DIVNONNEG").is_some() || f.constant_time || !f.secret.is_empty() {
        return 0;
    }
    let mut consts: HashMap<u32, i128> = HashMap::default();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
                consts.insert(d, *c);
            }
        }
    }
    let mut cands: Vec<Edit> = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ii, i) in b.insts.iter().enumerate() {
            let d = match i.dst {
                Some(d) => d,
                None => continue,
            };
            let (op, a, b2) = match &i.op {
                Op::Bin(op @ (BinOp::Div | BinOp::Rem), a, b2) => (*op, *a, *b2),
                _ => continue,
            };
            if !i.ty.signed() || i.ty.is_float() || i.ty.bits() < 32 {
                continue;
            }
            let k = match consts.get(&b2).copied().and_then(log2_exact) {
                Some(k) if k < i.ty.bits() - 1 => k,
                _ => continue,
            };
            if op == BinOp::Div {
                cands.push(Edit { block: bi, at: ii, shift: true, ty: i.ty, a, dst: d, imm: k as i128 });
            } else {
                cands.push(Edit { block: bi, at: ii, shift: false, ty: i.ty, a, dst: d, imm: (1i128 << k) - 1 });
            }
        }
    }
    if cands.is_empty() {
        return 0;
    }
    let queries: Vec<(usize, u32)> = cands.iter().map(|e| (e.block, e.a)).collect();
    let ok = crate::rangecheck::nonneg_at(f, &queries);
    let mut edits: Vec<Edit> = cands.into_iter().zip(ok).filter(|(_, o)| *o).map(|(e, _)| e).collect();
    if edits.is_empty() {
        return 0;
    }
    edits.sort_by(|x, y| (y.block, y.at).cmp(&(x.block, x.at)));
    let n = edits.len();
    for e in edits {
        let cv = f.new_val_pub(e.ty);
        let here = f.blocks[e.block].insts[e.at].loc;
        let cinst = Inst::like(Some(cv), e.ty, Op::Const(e.imm), here);
        let newop = if e.shift { Op::Bin(BinOp::Shr, e.a, cv) } else { Op::Bin(BinOp::And, e.a, cv) };
        let blk = &mut f.blocks[e.block];
        blk.insts[e.at] = Inst::like(Some(e.dst), e.ty, newop, here);
        blk.insts.insert(e.at, cinst);
    }
    n
}
