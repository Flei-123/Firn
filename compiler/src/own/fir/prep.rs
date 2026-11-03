use super::index::{term_uses, Index};
use super::slots::Slots;
use crate::fir::{Func, Module, Op, Val};
use crate::fx::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

pub(crate) struct Prep {
    pub ix: Index,
    pub sl: Slots,
    pub code_of: HashMap<Val, Val>,
    pub zero_add: HashMap<Val, Val>,
    users: Users,
    owner_users: OnceLock<Users>,
}

impl Prep {
    pub fn build(f: &Func) -> Prep {
        let ix = Index::build(f);
        let sl = Slots::build(f, &ix);
        let users = value_users(f, &ix, &HashMap::default());
        let zero_add = zero_adds(f, &ix);
        Prep { code_of: code_loads(f), zero_add, users, owner_users: OnceLock::new(), ix, sl }
    }

    pub fn users(&self, f: &Func, owners: bool) -> &Users {
        if !owners || self.zero_add.is_empty() {
            return &self.users;
        }
        self.owner_users.get_or_init(|| value_users(f, &self.ix, &self.zero_add))
    }
}

pub(crate) struct Cache {
    slots: Vec<Mutex<Option<Arc<Prep>>>>,
}

impl Cache {
    pub fn new(n: usize) -> Cache {
        Cache { slots: (0..n).map(|_| Mutex::new(None)).collect() }
    }

    pub fn get(&self, m: &Module, i: usize) -> Arc<Prep> {
        let mut slot = self.slots[i].lock().unwrap();
        if let Some(p) = slot.as_ref() {
            return p.clone();
        }
        let p = Arc::new(Prep::build(&m.funcs[i]));
        *slot = Some(p.clone());
        p
    }

    pub fn evict(&self, i: usize) {
        *self.slots[i].lock().unwrap() = None;
    }

    pub fn clear(&mut self) {
        for s in self.slots.iter_mut() {
            *s.get_mut().unwrap() = None;
        }
    }
}

fn zero_adds(f: &Func, ix: &Index) -> HashMap<Val, Val> {
    let mut out = HashMap::default();
    for b in f.blocks.iter() {
        for inst in b.insts.iter() {
            if let (Op::PtrAdd { base, off }, Some(d)) = (&inst.op, inst.dst) {
                if ix.const_of(f, *off) == Some(0) {
                    out.insert(d, *base);
                }
            }
        }
    }
    out
}

pub(crate) struct Users {
    start: Vec<u32>,
    data: Vec<u32>,
}

impl Users {
    pub fn of(&self, v: usize) -> &[u32] {
        match (self.start.get(v), self.start.get(v + 1)) {
            (Some(&a), Some(&b)) => &self.data[a as usize..b as usize],
            _ => &[],
        }
    }
}

fn for_each_use(f: &Func, zero_add: &HashMap<Val, Val>, mut visit: impl FnMut(Val, u32)) {
    let mut buf = Vec::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for inst in b.insts.iter() {
            buf.clear();
            inst.op.uses(&mut buf);
            if let Op::Load { addr } = &inst.op {
                if let Some(&base) = zero_add.get(addr) {
                    buf.push(base);
                }
            }
            for &v in buf.iter() {
                visit(v, bi as u32);
            }
        }
        buf.clear();
        term_uses(&b.term, &mut buf);
        for &v in buf.iter() {
            visit(v, bi as u32);
        }
    }
}

fn value_users(f: &Func, ix: &Index, zero_add: &HashMap<Val, Val>) -> Users {
    let nvals = ix.nvals;
    let mut start = vec![0u32; nvals + 1];
    let mut last = vec![u32::MAX; nvals];
    for_each_use(f, zero_add, |v, bi| {
        if (v as usize) < nvals && last[v as usize] != bi {
            last[v as usize] = bi;
            start[v as usize + 1] += 1;
        }
    });
    for i in 0..nvals {
        start[i + 1] += start[i];
    }
    let mut data = vec![0u32; start[nvals] as usize];
    let mut fill: Vec<u32> = start[..nvals].to_vec();
    last.iter_mut().for_each(|l| *l = u32::MAX);
    for_each_use(f, zero_add, |v, bi| {
        if (v as usize) < nvals && last[v as usize] != bi {
            last[v as usize] = bi;
            data[fill[v as usize] as usize] = bi;
            fill[v as usize] += 1;
        }
    });
    Users { start, data }
}

fn code_loads(f: &Func) -> HashMap<Val, Val> {
    let mut targets: HashSet<Val> = Default::default();
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
