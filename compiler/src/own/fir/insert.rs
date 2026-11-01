use super::live::{Free, Handle};
use super::slots::Slots;
use crate::fir::{FTy, Func, Inst, Op, Val};

fn cell_load(f: &mut Func, sl: &Slots, cell: usize, out: &mut Vec<Inst>) -> Val {
    let (s, off) = sl.cells[cell];
    let mut addr = sl.base[s];
    if off != 0 {
        let c = f.new_val_pub(FTy::I64);
        out.push(Inst::new(Some(c), FTy::I64, Op::Const(off as i128)));
        let p = f.new_val_pub(FTy::Ptr);
        out.push(Inst::new(Some(p), FTy::Ptr, Op::PtrAdd { base: addr, off: c }));
        addr = p;
    }
    let h = f.new_val_pub(FTy::I64);
    out.push(Inst::new(Some(h), FTy::I64, Op::Load { addr }));
    h
}

pub(crate) fn apply(f: &mut Func, sl: &Slots, mut frees: Vec<Free>, drop_fns: &[&str]) -> usize {
    frees.sort_by(|a, b| (b.block, b.pos).cmp(&(a.block, a.pos)));
    let n = frees.len();
    for fr in frees {
        let mut seq = Vec::new();
        let h = match fr.handle {
            Handle::Val(v) => v,
            Handle::Cell(c) => cell_load(f, sl, c, &mut seq),
        };
        seq.push(Inst::new(None, FTy::Void, Op::Call { name: drop_fns[fr.site].to_string(), args: vec![h] }));
        let insts = &mut f.blocks[fr.block].insts;
        let at = fr.pos.min(insts.len());
        insts.splice(at..at, seq);
    }
    n
}
