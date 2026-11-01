use super::index::{Bits, Index};
use super::registry::Drop;
use super::sites::{self, Site};
use super::slots::Slots;
use crate::fir::{Func, Inst, Op, Term, Val};
use crate::fx::HashMap;

pub(crate) const OTHER: usize = 0;

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct CellVal {
    pub may: Bits,
    pub derived: bool,
}

pub(crate) trait Borrows {
    fn borrows(&self, name: &str, arg: usize) -> bool;
    fn fresh(&self, _name: &str) -> bool {
        false
    }
}

pub(crate) struct Flow {
    pub nseeds: usize,
    pub nparams: usize,
    pub sites: Vec<Site>,
    pub drops: Vec<Drop>,
    pub site_at: HashMap<(usize, usize), usize>,
    pub may: Vec<Bits>,
    pub derived: Vec<bool>,
    pub cin: Vec<Option<Vec<CellVal>>>,
    pub escaped: Bits,
    pub ret_may: Bits,
    pub ret_derived: bool,
    other_bits: Bits,
    pub owner_bits: Bits,
    pub zero_add: HashMap<Val, Val>,
    code_of: HashMap<Val, Val>,
    scratch: Bits,
    uses: Vec<Val>,
}

impl Flow {
    pub fn seed_of_site(&self, k: usize) -> usize {
        1 + self.nparams + k
    }

    pub fn other(&self) -> CellVal {
        CellVal { may: self.other_bits.clone(), derived: false }
    }

    pub fn must(&self, v: Val) -> Option<usize> {
        let m = self.may.get(v as usize)?;
        if self.derived[v as usize] || m.count() != 1 {
            return None;
        }
        m.iter().next().filter(|s| *s > self.nparams)
    }

    pub fn build(f: &Func, ix: &Index, sl: &Slots, br: &dyn Borrows, with_sites: bool) -> Flow {
        let nparams = f.params.len();
        let (sites, site_at, drops) = if with_sites {
            let found = sites::find(f, ix, sl, br);
            (found.sites, found.site_at, found.drops)
        } else {
            (Vec::new(), HashMap::default(), Vec::new())
        };
        let nseeds = 1 + nparams + sites.len();
        let mut owner_bits = Bits::new(nseeds);
        let mut zero_add = HashMap::default();
        for (k, d) in drops.iter().enumerate() {
            if *d == Drop::SlotsOwner {
                owner_bits.set(1 + nparams + k);
            }
        }
        let code_of = code_loads(f);
        if owner_bits.count() > 0 {
            for b in f.blocks.iter() {
                for inst in b.insts.iter() {
                    if let (Op::PtrAdd { base, off }, Some(d)) = (&inst.op, inst.dst) {
                        if ix.const_of(f, *off) == Some(0) {
                            zero_add.insert(d, *base);
                        }
                    }
                }
            }
        }
        let mut fl = Flow {
            nseeds,
            nparams,
            sites,
            drops,
            site_at,
            may: vec![Bits::new(nseeds); ix.nvals],
            derived: vec![false; ix.nvals],
            cin: vec![None; f.blocks.len()],
            escaped: Bits::new(nseeds),
            ret_may: Bits::new(nseeds),
            ret_derived: false,
            other_bits: Bits::single(nseeds, OTHER),
            owner_bits,
            zero_add,
            code_of,
            scratch: Bits::new(nseeds),
            uses: Vec::new(),
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
                    fl.ret_may.union(&m);
                    fl.ret_derived |= fl.derived[*v as usize];
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
                        fl.ret_may.set(k);
                    }
                }
                break;
            }
        }
        fl
    }

    fn owner_field_load(&self, addr: Val, bytes: u64, acc: &mut Bits) -> bool {
        if bytes != 8 || self.owner_bits.count() == 0 {
            return false;
        }
        let base = self.zero_add.get(&addr).copied().unwrap_or(addr);
        let mut hit = false;
        for v in [addr, base] {
            if self.derived[v as usize] {
                continue;
            }
            for s in self.may[v as usize].iter() {
                if self.owner_bits.get(s) {
                    acc.set(s);
                    hit = true;
                }
            }
        }
        hit
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
        let Flow { may, escaped, .. } = self;
        escaped.union(&may[v as usize]);
    }

    pub fn transfer(&mut self, f: &Func, sl: &Slots, br: &dyn Borrows, cells: &mut [CellVal], inst: &Inst, at: (usize, usize), vals: bool) -> bool {
        let mut acc = std::mem::take(&mut self.scratch);
        acc.reset();
        let mut der = false;
        let mut uses = std::mem::take(&mut self.uses);
        uses.clear();
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
            Op::PtrAdd { .. } if inst.dst.map_or(false, |d| sl.is_addr(d)) => acc.assign(&self.other_bits),
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
                acc.set(OTHER);
                for v in uses.iter() {
                    acc.union(&self.may[*v as usize]);
                }
                der = true;
            }
            Op::Const(0) | Op::FnRef { .. } => {}
            Op::Cmp { .. }
            | Op::Const(_)
            | Op::Alloca { .. }
            | Op::GlobalAddr { .. }
            | Op::VtabAddr { .. }
            | Op::GcAddr { .. }
            | Op::ThreadSelf => acc.assign(&self.other_bits),
            Op::Load { addr } => match sl.access(*addr, inst.ty) {
                Some(a) => {
                    acc.assign(&cells[a.cell].may);
                    der = cells[a.cell].derived || inst.ty.bytes() < 8;
                }
                None => {
                    acc.assign(&self.other_bits);
                    der |= self.owner_field_load(*addr, inst.ty.bytes(), &mut acc);
                }
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
                let mut pair = false;
                if let Some(&k) = self.site_at.get(&at) {
                    if let Site::Pair { cell_p, cell_n } = self.sites[k] {
                        let s = self.seed_of_site(k);
                        cells[cell_p] = CellVal { may: Bits::single(self.nseeds, s), derived: false };
                        cells[cell_n] = CellVal { may: self.other_bits.clone(), derived: false };
                        pair = true;
                    }
                }
                for (k, v) in args.iter().enumerate() {
                    if (pair && k == 0) || br.borrows(name, k) {
                        continue;
                    }
                    self.sink(*v);
                }
                acc.assign(&self.other_bits);
            }
            Op::CallIndirect { target, args } => {
                let env = self.code_of.get(target).copied().filter(|e| args.last() == Some(e) && !self.derived[*e as usize]);
                self.sink(*target);
                for v in args.iter() {
                    if Some(*v) != env {
                        self.sink(*v);
                    }
                }
                acc.assign(&self.other_bits);
            }
            _ => {
                inst.op.uses(&mut uses);
                for v in uses.iter() {
                    self.sink(*v);
                }
                acc.assign(&self.other_bits);
            }
        }
        if let Some(&k) = self.site_at.get(&at) {
            if self.sites[k] == Site::Value {
                acc.reset();
                acc.set(self.seed_of_site(k));
                der = false;
            }
        }
        let ch = match (vals, inst.dst) {
            (true, Some(d)) => self.put(d, &acc, der),
            _ => false,
        };
        self.scratch = acc;
        self.uses = uses;
        ch
    }
}

fn join(a: &mut [CellVal], b: &[CellVal]) {
    for (x, y) in a.iter_mut().zip(b.iter()) {
        x.may.union(&y.may);
        x.derived |= y.derived;
    }
}

fn code_loads(f: &Func) -> HashMap<Val, Val> {
    let mut targets: crate::fx::HashSet<Val> = Default::default();
    for b in f.blocks.iter() {
        for inst in b.insts.iter() {
            if let Op::CallIndirect { target, .. } = &inst.op {
                targets.insert(*target);
            }
        }
    }
    let mut out = HashMap::default();
    if targets.is_empty() {
        return out;
    }
    for b in f.blocks.iter() {
        for inst in b.insts.iter() {
            if let (Op::Load { addr }, Some(d)) = (&inst.op, inst.dst) {
                if targets.contains(&d) {
                    out.insert(d, *addr);
                }
            }
        }
    }
    out
}
