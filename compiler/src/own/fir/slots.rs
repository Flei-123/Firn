use super::index::{term_uses, Index};
use super::registry;
use crate::fir::{FTy, Func, Op, Val};
use crate::fx::HashMap;

pub(crate) struct Slots {
    pub addr: HashMap<Val, (usize, i64)>,
    pub cells: Vec<(usize, i64)>,
    pub cell_id: HashMap<(usize, i64), usize>,
    pub base: Vec<Val>,
}

pub(crate) struct Access {
    pub cell: usize,
    pub strong: bool,
}

fn fits(off: i64, ty: FTy) -> bool {
    ty != FTy::V128 && ty != FTy::Void && ty.bytes() <= 8 && off >= 0 && (off % 8) as u64 + ty.bytes() <= 8
}

impl Slots {
    pub fn build(f: &Func, ix: &Index) -> Slots {
        let mut base = Vec::new();
        let mut addr: HashMap<Val, (usize, i64)> = HashMap::default();
        for b in f.blocks.iter() {
            for inst in b.insts.iter() {
                if let (Op::Alloca { .. }, Some(d)) = (&inst.op, inst.dst) {
                    addr.insert(d, (base.len(), 0));
                    base.push(d);
                }
            }
        }
        loop {
            let mut grew = false;
            for b in f.blocks.iter() {
                for inst in b.insts.iter() {
                    if let (Op::PtrAdd { base: pb, off }, Some(d)) = (&inst.op, inst.dst) {
                        if addr.contains_key(&d) {
                            continue;
                        }
                        if let (Some(&(s, o)), Some(c)) = (addr.get(pb), ix.const_of(f, *off)) {
                            if c >= 0 && c < (1 << 30) {
                                addr.insert(d, (s, o + c as i64));
                                grew = true;
                            }
                        }
                    }
                }
            }
            if !grew {
                break;
            }
        }
        let mut bad = vec![false; base.len()];
        let mut touched: Vec<(usize, i64)> = Vec::new();
        let mut buf = Vec::new();
        for b in f.blocks.iter() {
            for inst in b.insts.iter() {
                buf.clear();
                inst.op.uses(&mut buf);
                if !buf.iter().any(|v| addr.contains_key(v)) {
                    continue;
                }
                match &inst.op {
                    Op::Load { addr: a } => {
                        let (s, o) = addr[a];
                        if fits(o, inst.ty) {
                            touched.push((s, o - o % 8));
                        } else {
                            bad[s] = true;
                        }
                    }
                    Op::Store { addr: a, val } => {
                        if let Some(&(s, _)) = addr.get(val) {
                            bad[s] = true;
                        }
                        if let Some(&(s, o)) = addr.get(a) {
                            if fits(o, inst.ty) {
                                touched.push((s, o - o % 8));
                            } else {
                                bad[s] = true;
                            }
                        }
                    }
                    Op::PtrAdd { base: pb, off } if inst.dst.map_or(false, |d| addr.contains_key(&d)) => {
                        if let Some(&(s, _)) = addr.get(off) {
                            bad[s] = true;
                        }
                        let _ = pb;
                    }
                    Op::Call { name, args } if registry::producer(name).is_some() => {
                        let unused = inst.dst.map_or(true, |d| ix.uses[d as usize] == 0);
                        for (k, v) in args.iter().enumerate() {
                            if let Some(&(s, o)) = addr.get(v) {
                                if k == 0 && unused && o % 8 == 0 {
                                    touched.push((s, o));
                                    touched.push((s, o + 8));
                                } else {
                                    bad[s] = true;
                                }
                            }
                        }
                    }
                    _ => {
                        for v in buf.iter() {
                            if let Some(&(s, _)) = addr.get(v) {
                                bad[s] = true;
                            }
                        }
                    }
                }
            }
            buf.clear();
            term_uses(&b.term, &mut buf);
            for v in buf.iter() {
                if let Some(&(s, _)) = addr.get(v) {
                    bad[s] = true;
                }
            }
        }
        addr.retain(|_, (s, _)| !bad[*s]);
        let mut cells = Vec::new();
        let mut cell_id = HashMap::default();
        for (s, o) in touched {
            if bad[s] || cell_id.contains_key(&(s, o)) {
                continue;
            }
            cell_id.insert((s, o), cells.len());
            cells.push((s, o));
        }
        Slots { addr, cells, cell_id, base }
    }

    pub fn access(&self, a: Val, ty: FTy) -> Option<Access> {
        let &(s, o) = self.addr.get(&a)?;
        let cell = *self.cell_id.get(&(s, o - o % 8))?;
        Some(Access { cell, strong: o % 8 == 0 && ty.bytes() == 8 })
    }

    pub fn producer_cells(&self, dst: Val) -> Option<(usize, usize)> {
        let &(s, o) = self.addr.get(&dst)?;
        Some((*self.cell_id.get(&(s, o))?, *self.cell_id.get(&(s, o + 8))?))
    }

    pub fn is_addr(&self, v: Val) -> bool {
        self.addr.contains_key(&v)
    }
}
