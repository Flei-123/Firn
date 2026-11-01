use super::index::{term_uses, Index};
use super::slots::Slots;
use crate::fir::{Func, Module, Op, Val};
use crate::fx::{HashMap, HashSet};
use std::sync::OnceLock;

pub(crate) struct Prep {
    pub ix: Index,
    pub sl: Slots,
    pub code_of: HashMap<Val, Val>,
    pub zero_add: HashMap<Val, Val>,
    users: Vec<Vec<u32>>,
    owner_users: OnceLock<Vec<Vec<u32>>>,
}

impl Prep {
    pub fn build(f: &Func) -> Prep {
        let ix = Index::build(f);
        let sl = Slots::build(f, &ix);
        let users = value_users(f, &ix, &HashMap::default());
        let zero_add = zero_adds(f, &ix);
        Prep { code_of: code_loads(f), zero_add, users, owner_users: OnceLock::new(), ix, sl }
    }

    pub fn users(&self, f: &Func, owners: bool) -> &[Vec<u32>] {
        if !owners || self.zero_add.is_empty() {
            return &self.users;
        }
        self.owner_users.get_or_init(|| value_users(f, &self.ix, &self.zero_add))
    }
}

pub(crate) struct Cache {
    slots: Vec<OnceLock<Prep>>,
}

impl Cache {
    pub fn new(n: usize) -> Cache {
        Cache { slots: (0..n).map(|_| OnceLock::new()).collect() }
    }

    pub fn get(&self, m: &Module, i: usize) -> &Prep {
        self.slots[i].get_or_init(|| Prep::build(&m.funcs[i]))
    }

    pub fn built(&self, i: usize) -> Option<&Prep> {
        self.slots[i].get()
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

fn value_users(f: &Func, ix: &Index, zero_add: &HashMap<Val, Val>) -> Vec<Vec<u32>> {
    let mut users: Vec<Vec<u32>> = vec![Vec::new(); ix.nvals];
    let mut buf = Vec::new();
    let add = |users: &mut Vec<Vec<u32>>, v: Val, bi: usize| {
        if let Some(u) = users.get_mut(v as usize) {
            if u.last() != Some(&(bi as u32)) {
                u.push(bi as u32);
            }
        }
    };
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
                add(&mut users, v, bi);
            }
        }
        buf.clear();
        term_uses(&b.term, &mut buf);
        for &v in buf.iter() {
            add(&mut users, v, bi);
        }
    }
    users
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
