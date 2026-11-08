// SPDX-License-Identifier: GPL-2.0-only
use crate::fir::{BinOp, Func, Op, Val};
use crate::fx::{HashMap, HashSet};

pub(crate) struct ShiftOperand {
    pub other: Val,
    pub source: Val,
    pub mnemonic: &'static str,
    pub amount: u32,
}

#[derive(Default)]
pub(crate) struct ShiftFold {
    pub fused: HashMap<Val, ShiftOperand>,
    pub dead: HashSet<Val>,
}

fn mnemonic(op: BinOp, signed: bool) -> Option<&'static str> {
    match (op, signed) {
        (BinOp::Shl, _) => Some("lsl"),
        (BinOp::Shr, true) => Some("asr"),
        (BinOp::Shr, false) => Some("lsr"),
        _ => None,
    }
}

fn takes_shifted_operand(op: BinOp) -> bool {
    matches!(op, BinOp::Add | BinOp::Sub | BinOp::And | BinOp::Or | BinOp::Xor)
}

fn commutes(op: BinOp) -> bool {
    !matches!(op, BinOp::Sub)
}

pub(crate) fn analyse(f: &Func, consts: &HashMap<Val, i128>) -> ShiftFold {
    let mut out = ShiftFold::default();
    if std::env::var_os("FIRN_A64_NO_SHIFTFOLD").is_some() || !f.secret.is_empty() || f.constant_time {
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
            let user = &b.insts[k];
            let (op, a, c) = match &user.op {
                Op::Bin(op, a, c) if takes_shifted_operand(*op) => (*op, *a, *c),
                _ => continue,
            };
            let (dst, bits) = match (user.dst, user.ty.bits()) {
                (Some(d), 32 | 64) if !user.ty.is_float() => (d, user.ty.bits()),
                _ => continue,
            };
            if defs.get(dst as usize) != Some(&1) {
                continue;
            }
            let prev = &b.insts[k - 1];
            let shifted = match prev.dst {
                Some(s) if s == c || (s == a && commutes(op)) => s,
                _ => continue,
            };
            if !once(shifted) || prev.ty != user.ty {
                continue;
            }
            let (source, amount_val, shift_op) = match &prev.op {
                Op::Bin(sop @ (BinOp::Shl | BinOp::Shr), x, n) => (*x, *n, *sop),
                _ => continue,
            };
            let amount = match consts.get(&amount_val) {
                Some(n) if defs.get(amount_val as usize) == Some(&1) && *n > 0 && *n < bits as i128 => *n as u32,
                _ => continue,
            };
            let other = if shifted == a { c } else { a };
            if other == shifted || source == shifted {
                continue;
            }
            let name = match mnemonic(shift_op, user.ty.signed()) {
                Some(m) => m,
                None => continue,
            };
            out.fused.insert(dst, ShiftOperand { other, source, mnemonic: name, amount });
            out.dead.insert(shifted);
        }
    }
    out
}
