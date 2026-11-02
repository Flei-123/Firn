use crate::fir::{Inst, Module, Op};
use crate::fx::{HashMap, HashSet};

pub(crate) struct Quiet {
    names: HashSet<String>,
}

impl Quiet {
    pub(crate) fn none() -> Quiet {
        Quiet { names: HashSet::default() }
    }

    pub(crate) fn compute(m: &Module) -> Quiet {
        let index: HashMap<&str, usize> = m.funcs.iter().enumerate().map(|(i, f)| (f.name.as_str(), i)).collect();
        let mut callees: Vec<Vec<usize>> = Vec::with_capacity(m.funcs.len());
        let mut quiet: Vec<bool> = Vec::with_capacity(m.funcs.len());
        for f in m.funcs.iter() {
            let mut ok = true;
            let mut out = Vec::new();
            for inst in f.blocks.iter().flat_map(|b| b.insts.iter()) {
                match &inst.op {
                    Op::Call { name, .. } => match index.get(name.as_str()) {
                        Some(&k) => out.push(k),
                        None => ok = false,
                    },
                    op if loud_op(op) => ok = false,
                    _ => {}
                }
            }
            out.sort_unstable();
            out.dedup();
            callees.push(out);
            quiet.push(ok);
        }
        let mut callers: Vec<Vec<usize>> = vec![Vec::new(); m.funcs.len()];
        for (i, cs) in callees.iter().enumerate() {
            for &c in cs {
                callers[c].push(i);
            }
        }
        let mut work: Vec<usize> = (0..m.funcs.len()).filter(|&i| !quiet[i]).collect();
        while let Some(c) = work.pop() {
            for &i in callers[c].iter() {
                if quiet[i] {
                    quiet[i] = false;
                    work.push(i);
                }
            }
        }
        let names = m.funcs.iter().zip(quiet).filter(|(_, q)| *q).map(|(f, _)| f.name.clone()).collect();
        Quiet { names }
    }

    pub(crate) fn len(&self) -> usize {
        self.names.len()
    }

    pub(crate) fn may_gc(&self, insts: &[Inst]) -> bool {
        insts.iter().any(|i| match &i.op {
            Op::Call { name, .. } => !self.names.contains(name),
            op => loud_op(op),
        })
    }
}

fn loud_op(op: &Op) -> bool {
    matches!(op, Op::CallIndirect { .. } | Op::Syscall { .. } | Op::ThreadSpawn { .. })
}
