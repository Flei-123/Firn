use super::live::{Free, Handle};
use super::slots::Slots;
use crate::fir::{FTy, Func, Inst, Op, Val};
use crate::fx::{HashMap, HashSet};

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

pub(crate) fn apply(f: &mut Func, sl: &Slots, frees: Vec<Free>, drop_fns: &[&str], reuse: &HashSet<usize>) -> (usize, HashMap<usize, Val>) {
    let mut order: Vec<(usize, Free)> = frees.into_iter().enumerate().collect();
    order.sort_by(|a, b| (b.1.block, b.1.pos).cmp(&(a.1.block, a.1.pos)));
    let mut handles: HashMap<usize, Val> = HashMap::default();
    let mut n = 0;
    for (i, fr) in order {
        let mut seq = Vec::new();
        let h = match fr.handle {
            Handle::Val(v) => v,
            Handle::Cell(c) => cell_load(f, sl, c, &mut seq),
        };
        if reuse.contains(&i) {
            handles.insert(i, h);
        } else {
            seq.push(Inst::new(None, FTy::Void, Op::Call { name: drop_fns[fr.site].to_string(), args: vec![h] }));
            n += 1;
        }
        let insts = &mut f.blocks[fr.block].insts;
        let at = fr.pos.min(insts.len());
        insts.splice(at..at, seq);
    }
    (n, handles)
}
