use super::index::{Bits, Index};
use super::registry;
use super::slots::Slots;
use crate::fir::{Func, Inst, Op, Term, Val};
use crate::fx::HashMap;

pub(crate) const OTHER: usize = 0;

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct CellVal {
    pub may: Bits,
    pub derived: bool,
}

pub(crate) struct Site {
    pub cell_p: usize,
    pub cell_n: usize,
}

pub(crate) trait Borrows {
    fn borrows(&self, name: &str, arg: usize) -> bool;
}

pub(crate) struct Flow {
    pub nseeds: usize,
    pub nparams: usize,
    pub sites: Vec<Site>,
    pub site_at: HashMap<(usize, usize), usize>,
    pub may: Vec<Bits>,
    pub derived: Vec<bool>,
    pub cin: Vec<Option<Vec<CellVal>>>,
    pub escaped: Bits,
}

impl Flow {
    pub fn seed_of_site(&self, k: usize) -> usize {
        1 + self.nparams + k
    }

    pub fn other(&self) -> CellVal {
        let mut may = Bits::new(self.nseeds);
        may.set(OTHER);
        CellVal { may, derived: false }
    }

    pub fn must(&self, v: Val) -> Option<usize> {
        let m = self.may.get(v as usize)?;
        if self.derived[v as usize] || m.count() != 1 {
            return None;
        }
        m.iter().next().filter(|s| *s > self.nparams)
    }

    pub fn build(f: &Func, ix: &Index, sl: &Slots, br: &dyn Borrows) -> Flow {
        let nparams = f.params.len();
        let mut sites = Vec::new();
        let mut site_at = HashMap::default();
        for &bi in ix.rpo.iter() {
            for (ii, inst) in f.blocks[bi].insts.iter().enumerate() {
                if let Op::Call { name, args } = &inst.op {
                    if registry::producer(name).map_or(false, |p| p.shape == registry::Shape::SretPair) && !args.is_empty() {
                        if let Some((cp, cn)) = sl.producer_cells(args[0]) {
                            site_at.insert((bi, ii), sites.len());
                            sites.push(Site { cell_p: cp, cell_n: cn });
                        }
                    }
                }
            }
        }
        let nseeds = 1 + nparams + sites.len();
        let mut fl = Flow {
            nseeds,
            nparams,
            sites,
            site_at,
            may: vec![Bits::new(nseeds); ix.nvals],
            derived: vec![false; ix.nvals],
            cin: vec![None; f.blocks.len()],
            escaped: Bits::new(nseeds),
        };
        for i in 0..nparams.min(ix.nvals) {
            fl.may[i].set(1 + i);
        }
        let entry = ix.rpo.first().copied().unwrap_or(0);
        let mut cout: Vec<Option<Vec<CellVal>>> = vec![None; f.blocks.len()];
        let mut rounds = 0;
        loop {
            rounds += 1;
            let mut changed = false;
            for &bi in ix.rpo.iter() {
                let mut cells: Vec<CellVal> = if bi == entry {
                    vec![fl.other(); sl.cells.len()]
                } else {
                    let mut acc: Option<Vec<CellVal>> = None;
                    for &p in ix.preds[bi].iter() {
                        if let Some(c) = &cout[p] {
                            match &mut acc {
                                None => acc = Some(c.clone()),
                                Some(a) => join(a, c),
                            }
                        }
                    }
                    match acc {
                        Some(a) => a,
                        None => continue,
                    }
                };
                if fl.cin[bi].as_ref() != Some(&cells) {
                    fl.cin[bi] = Some(cells.clone());
                    changed = true;
                }
                for (ii, inst) in f.blocks[bi].insts.iter().enumerate() {
                    changed |= fl.transfer(f, sl, br, &mut cells, inst, (bi, ii), true);
                }
                if let Term::Ret(Some(v)) = &f.blocks[bi].term {
                    let m = fl.may[*v as usize].clone();
                    fl.escaped.union(&m);
                }
                if cout[bi].as_ref() != Some(&cells) {
                    cout[bi] = Some(cells);
                    changed = true;
                }
            }
            if !changed || rounds > 200 {
                if changed {
                    for k in 1..nseeds {
                        fl.escaped.set(k);
                    }
                }
                break;
            }
        }
        fl
    }

    fn put(&mut self, v: Val, m: &Bits, d: bool) -> bool {
        let i = v as usize;
        let mut ch = self.may[i].union(m);
        if d && !self.derived[i] {
            self.derived[i] = true;
            ch = true;
        }
        ch
    }

    fn sink(&mut self, v: Val) {
        let m = self.may[v as usize].clone();
        self.escaped.union(&m);
    }

    pub fn transfer(&mut self, f: &Func, sl: &Slots, br: &dyn Borrows, cells: &mut [CellVal], inst: &Inst, at: (usize, usize), vals: bool) -> bool {
        let mut other = Bits::new(self.nseeds);
        other.set(OTHER);
        let mut acc = Bits::new(self.nseeds);
        let mut der = false;
        let mut uses: Vec<Val> = Vec::new();
        match &inst.op {
            Op::Phi { incoming } => {
                for (_, v) in incoming.iter() {
                    acc.union(&self.may[*v as usize]);
                    der |= self.derived[*v as usize];
                }
            }
            Op::Copy { src } => {
                acc.union(&self.may[*src as usize]);
                der = self.derived[*src as usize];
            }
            Op::Cast { src, .. } => {
                acc.union(&self.may[*src as usize]);
                der = self.derived[*src as usize] || f.val_ty(*src).bytes() != inst.ty.bytes();
            }
            Op::Select { a, b, .. } => {
                acc.union(&self.may[*a as usize]);
                acc.union(&self.may[*b as usize]);
                der = self.derived[*a as usize] || self.derived[*b as usize];
            }
            Op::PtrAdd { .. } if inst.dst.map_or(false, |d| sl.is_addr(d)) => acc = other.clone(),
            Op::PtrAdd { .. }
            | Op::Bin(..)
            | Op::BinWrapSat { .. }
            | Op::CheckedBin { .. }
            | Op::CheckedDiv { .. }
            | Op::Un(..)
            | Op::CheckedCast { .. }
            | Op::CheckedIdx { .. }
            | Op::CheckedIdxV { .. } => {
                inst.op.uses(&mut uses);
                acc = other.clone();
                for v in uses.iter() {
                    acc.union(&self.may[*v as usize]);
                }
                der = true;
            }
            Op::Cmp { .. }
            | Op::Const(_)
            | Op::Alloca { .. }
            | Op::GlobalAddr { .. }
            | Op::FnRef { .. }
            | Op::VtabAddr { .. }
            | Op::GcAddr { .. }
            | Op::ThreadSelf => acc = other.clone(),
            Op::Load { addr } => match sl.access(*addr, inst.ty) {
                Some(a) => {
                    acc = cells[a.cell].may.clone();
                    der = cells[a.cell].derived || inst.ty.bytes() < 8;
                }
                None => acc = other.clone(),
            },
            Op::Store { addr, val } => match sl.access(*addr, inst.ty) {
                Some(a) => {
                    if a.strong {
                        cells[a.cell] = CellVal { may: self.may[*val as usize].clone(), derived: self.derived[*val as usize] };
                    } else {
                        let c = &mut cells[a.cell];
                        c.may.union(&self.may[*val as usize]);
                        c.may.set(OTHER);
                        c.derived = true;
                    }
                }
                None => self.sink(*val),
            },
            Op::Call { name, args } => {
                let site = self.site_at.get(&at).copied();
                if let Some(k) = site {
                    let s = self.seed_of_site(k);
                    let (cp, cn) = (self.sites[k].cell_p, self.sites[k].cell_n);
                    let mut m = Bits::new(self.nseeds);
                    m.set(s);
                    cells[cp] = CellVal { may: m, derived: false };
                    cells[cn] = CellVal { may: other.clone(), derived: false };
                }
                for (k, v) in args.iter().enumerate() {
                    if (site.is_some() && k == 0) || br.borrows(name, k) {
                        continue;
                    }
                    self.sink(*v);
                }
                acc = other.clone();
            }
            _ => {
                inst.op.uses(&mut uses);
                for v in uses.iter() {
                    self.sink(*v);
                }
                acc = other.clone();
            }
        }
        match (vals, inst.dst) {
            (true, Some(d)) => self.put(d, &acc, der),
            _ => false,
        }
    }
}

fn join(a: &mut [CellVal], b: &[CellVal]) {
    for (x, y) in a.iter_mut().zip(b.iter()) {
        x.may.union(&y.may);
        x.derived |= y.derived;
    }
}
