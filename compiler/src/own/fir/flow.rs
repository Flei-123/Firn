use super::carried::Labels;
use super::index::Bits;
use super::prep::Prep;
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
    pub gen: Bits,
}

pub(crate) trait Borrows {
    fn borrows(&self, name: &str, arg: usize) -> bool;
    fn fresh(&self, _name: &str) -> bool {
        false
    }
    fn fresh_drop(&self, _name: &str) -> Drop {
        Drop::Block
    }
    fn deep_borrows(&self, _name: &str, _arg: usize) -> bool {
        false
    }
    fn tree(&self, _name: &str) -> bool {
        false
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mode {
    Plain,
    Deep,
    DeepPtr,
}

impl Mode {
    fn deep(self) -> bool {
        self != Mode::Plain
    }

    fn deep_load(self, ty: crate::fir::FTy) -> bool {
        match self {
            Mode::Plain => false,
            Mode::Deep => ty.bytes() == 8,
            Mode::DeepPtr => ty == crate::fir::FTy::Ptr,
        }
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
    mode: Mode,
    pub absorb: Bits,
    pub absorbed: Vec<(usize, usize)>,
    pub gen: Vec<Bits>,
    pub labels: Option<Labels>,
    gscratch: Bits,
}

impl Flow {
    pub fn seed_of_site(&self, k: usize) -> usize {
        1 + self.nparams + k
    }

    pub fn other(&self) -> CellVal {
        CellVal { may: self.other_bits.clone(), derived: false, gen: self.no_gen() }
    }

    pub fn no_gen(&self) -> Bits {
        Bits::new(self.labels.as_ref().map_or(0, |l| l.width()))
    }

    pub fn build_carried(f: &Func, pr: &Prep, br: &dyn Borrows) -> Flow {
        Flow::build_inner(f, pr, br, true, Mode::Plain, &[], true)
    }

    pub fn must(&self, v: Val) -> Option<usize> {
        let m = self.may.get(v as usize)?;
        if self.derived[v as usize] || m.count() != 1 {
            return None;
        }
        m.iter().next().filter(|s| *s > self.nparams)
    }

    pub fn build(f: &Func, pr: &Prep, br: &dyn Borrows, with_sites: bool) -> Flow {
        Flow::build_with(f, pr, br, with_sites, Mode::Plain, &[])
    }

    pub fn build_with(f: &Func, pr: &Prep, br: &dyn Borrows, with_sites: bool, mode: Mode, absorb: &[usize]) -> Flow {
        Flow::build_inner(f, pr, br, with_sites, mode, absorb, false)
    }

    fn build_inner(f: &Func, pr: &Prep, br: &dyn Borrows, with_sites: bool, mode: Mode, absorb: &[usize], carried: bool) -> Flow {
        let (ix, sl) = (&pr.ix, &pr.sl);
        let nparams = f.params.len();
        let (sites, site_at, drops) = if with_sites {
            let found = sites::find(f, ix, sl, br);
            (found.sites, found.site_at, found.drops)
        } else {
            (Vec::new(), HashMap::default(), Vec::new())
        };
        let nseeds = 1 + nparams + sites.len();
        let mut owner_bits = Bits::new(nseeds);
        for (k, d) in drops.iter().enumerate() {
            if *d == Drop::SlotsOwner {
                owner_bits.set(1 + nparams + k);
            }
        }
        let owners = owner_bits.count() > 0;
        let mut absorb_bits = Bits::new(nseeds);
        absorb.iter().filter(|&&b| b < nseeds).for_each(|&b| absorb_bits.set(b));
        let zero_add = if owners { pr.zero_add.clone() } else { HashMap::default() };
        let code_of = pr.code_of.clone();
        let labels = if carried { Labels::build(f, ix, &site_at) } else { None };
        let nlab = labels.as_ref().map_or(0, |l| l.width());
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
            mode,
            absorb: absorb_bits,
            absorbed: Vec::new(),
            gen: vec![Bits::new(nlab); ix.nvals],
            labels,
            gscratch: Bits::new(nlab),
        };
        for i in 0..nparams.min(ix.nvals) {
            fl.may[i].set(1 + i);
        }
        let entry = ix.rpo.first().copied().unwrap_or(0);
        let deps = pr.users(f, owners || mode.deep());
        let mut dirty = vec![true; f.blocks.len()];
        let mut cout: Vec<Option<Vec<CellVal>>> = vec![None; f.blocks.len()];
        let mut rounds = 0;
        loop {
            rounds += 1;
            let mut changed = false;
            for &bi in ix.rpo.iter() {
                if !dirty[bi] {
                    continue;
                }
                let mut cells: Vec<CellVal> = if bi == entry {
                    vec![fl.other(); sl.cells.len()]
                } else {
                    let mut acc: Option<Vec<CellVal>> = None;
                    for &p in ix.preds[bi].iter() {
                        if let Some(c) = &cout[p] {
                            let aged;
                            let c = match &fl.labels {
                                Some(lb) if lb.ages(p, bi) => {
                                    let mut x = c.clone();
                                    x.iter_mut().for_each(|cv| lb.age(p, bi, &mut cv.gen));
                                    aged = x;
                                    &aged
                                }
                                _ => c,
                            };
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
                dirty[bi] = false;
                if with_sites && fl.cin[bi].as_ref() != Some(&cells) {
                    fl.cin[bi] = Some(cells.clone());
                    changed = true;
                }
                for (ii, inst) in f.blocks[bi].insts.iter().enumerate() {
                    if fl.transfer(f, sl, br, &mut cells, inst, (bi, ii), true) {
                        changed = true;
                        if let Some(d) = inst.dst {
                            for &u in deps[d as usize].iter() {
                                dirty[u as usize] = true;
                            }
                        }
                    }
                }
                if let Term::Ret(Some(v)) = &f.blocks[bi].term {
                    let m = fl.may[*v as usize].clone();
                    fl.ret_may.union(&m);
                    fl.ret_derived |= fl.derived[*v as usize];
                }
                if cout[bi].as_ref() != Some(&cells) {
                    cout[bi] = Some(cells);
                    changed = true;
                    for &s in ix.succs[bi].iter() {
                        dirty[s] = true;
                    }
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

    fn scratch_params(&self) -> Bits {
        Bits::new(self.nseeds)
    }

    fn param_bits_into(&self, addr: Val, acc: &mut Bits) -> bool {
        let mut hit = false;
        for b in self.may[addr as usize].iter() {
            if b >= 1 && b <= self.nparams {
                acc.set(b);
                hit = true;
            }
        }
        hit
    }

    fn absorbs(&self, addr: Val) -> bool {
        if self.absorb.count() == 0 {
            return false;
        }
        let mut any = false;
        for b in self.may[addr as usize].iter() {
            if b == OTHER {
                continue;
            }
            if !self.absorb.get(b) {
                return false;
            }
            any = true;
        }
        any
    }

    fn put(&mut self, v: Val, m: &Bits, g: &Bits, d: bool) -> bool {
        let i = v as usize;
        let mut ch = self.may[i].union(m);
        ch |= self.gen[i].union(g);
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
        let mut gacc = std::mem::take(&mut self.gscratch);
        gacc.reset();
        let mut der = false;
        let mut uses = std::mem::take(&mut self.uses);
        uses.clear();
        match &inst.op {
            Op::Phi { incoming } => {
                for (p, v) in incoming.iter() {
                    acc.union(&self.may[*v as usize]);
                    der |= self.derived[*v as usize];
                    self.gen_from(&mut gacc, *v, Some((*p, at.0)));
                }
            }
            Op::Copy { src } => {
                acc.union(&self.may[*src as usize]);
                gacc.union(&self.gen[*src as usize]);
                der = self.derived[*src as usize];
            }
            Op::Cast { src, .. } => {
                acc.union(&self.may[*src as usize]);
                gacc.union(&self.gen[*src as usize]);
                der = self.derived[*src as usize] || f.val_ty(*src).bytes() != inst.ty.bytes();
            }
            Op::Select { a, b, .. } => {
                acc.union(&self.may[*a as usize]);
                acc.union(&self.may[*b as usize]);
                gacc.union(&self.gen[*a as usize]);
                gacc.union(&self.gen[*b as usize]);
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
                    gacc.union(&self.gen[*v as usize]);
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
                    gacc.assign(&cells[a.cell].gen);
                    der = cells[a.cell].derived || inst.ty.bytes() < 8;
                }
                None => {
                    acc.assign(&self.other_bits);
                    der |= self.owner_field_load(*addr, inst.ty.bytes(), &mut acc);
                    if self.mode.deep_load(inst.ty) {
                        der |= self.param_bits_into(*addr, &mut acc);
                    }
                }
            },
            Op::Store { addr, val } => match sl.access(*addr, inst.ty) {
                Some(a) => {
                    if a.strong {
                        cells[a.cell] = CellVal { may: self.may[*val as usize].clone(), derived: self.derived[*val as usize], gen: self.gen[*val as usize].clone() };
                    } else {
                        let c = &mut cells[a.cell];
                        c.may.union(&self.may[*val as usize]);
                        c.gen.union(&self.gen[*val as usize]);
                        c.may.set(OTHER);
                        c.derived = true;
                    }
                }
                None => {
                    if self.absorbs(*addr) {
                        if !self.absorbed.contains(&at) {
                            self.absorbed.push(at);
                        }
                    } else {
                        self.sink(*val);
                    }
                    if self.mode.deep() {
                        let mut a = self.scratch_params();
                        self.param_bits_into(*addr, &mut a);
                        self.escaped.union(&a);
                    }
                }
            },
            Op::Call { name, args } => {
                let mut pair = false;
                if let Some(&k) = self.site_at.get(&at) {
                    if let Site::Pair { cell_p, cell_n } = self.sites[k] {
                        let s = self.seed_of_site(k);
                        let mut g = self.no_gen();
                        if let Some(d) = self.labels.as_ref().and_then(|l| l.cur(at)) {
                            g.set(d);
                        }
                        cells[cell_p] = CellVal { may: Bits::single(self.nseeds, s), derived: false, gen: g };
                        cells[cell_n] = self.other();
                        pair = true;
                    }
                }
                for (k, v) in args.iter().enumerate() {
                    if (pair && k == 0) || (self.mode == Mode::Plain && br.borrows(name, k)) || (self.mode.deep() && br.deep_borrows(name, k)) {
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
                gacc.reset();
                if let Some(d) = self.labels.as_ref().and_then(|l| l.cur(at)) {
                    gacc.set(d);
                }
            }
        }
        let ch = match (vals, inst.dst) {
            (true, Some(d)) => self.put(d, &acc, &gacc, der),
            _ => false,
        };
        self.scratch = acc;
        self.gscratch = gacc;
        self.uses = uses;
        ch
    }
}

impl Flow {
    fn gen_from(&self, acc: &mut Bits, v: Val, edge: Option<(crate::fir::BlockId, usize)>) {
        let g = &self.gen[v as usize];
        let aging = match (&self.labels, edge) {
            (Some(lb), Some((pid, to))) => lb.block_of(pid).filter(|&from| lb.ages(from, to)).map(|from| (lb, from, to)),
            _ => None,
        };
        match aging {
            Some((lb, from, to)) => {
                let mut x = g.clone();
                lb.age(from, to, &mut x);
                acc.union(&x);
            }
            None => {
                acc.union(g);
            }
        }
    }
}

fn join(a: &mut [CellVal], b: &[CellVal]) {
    for (x, y) in a.iter_mut().zip(b.iter()) {
        x.may.union(&y.may);
        x.gen.union(&y.gen);
        x.derived |= y.derived;
    }
}
