// SPDX-License-Identifier: MPL-2.0
//! `gtaken` -- which statics have their address taken anywhere in the program.
//!
//! All modules of one program are merged into one `fir::Module`, so the answer
//! is exact for the whole program. A static whose address only ever appears as
//! the address operand of a load, a store or a copy (directly or through
//! `ptradd` chains) cannot be reached through any other pointer; `loadcse`
//! then lets stores through unrelated pointers leave its value alone.
//! Switch: `FIRN_NO_GNOALIAS`.

use crate::fir::{Func, Module, Op, Term, Val};
use crate::fx::{HashMap, HashSet};
use std::cell::RefCell;
use std::sync::Arc;

pub type Taken = Arc<HashSet<String>>;

thread_local! {
    static TAKEN: RefCell<Option<Taken>> = const { RefCell::new(None) };
}

pub fn get() -> Option<Taken> {
    TAKEN.with(|t| t.borrow().clone())
}

pub fn set(v: Option<Taken>) {
    TAKEN.with(|t| *t.borrow_mut() = v);
}

pub fn install(m: &Module) {
    if std::env::var_os("FIRN_NO_GNOALIAS").is_some() {
        set(None);
        return;
    }
    let mut taken: HashSet<String> = HashSet::default();
    for f in &m.funcs {
        scan(f, &mut taken);
    }
    set(Some(Arc::new(taken)));
}

fn scan(f: &Func, taken: &mut HashSet<String>) {
    let mut derived: HashMap<Val, String> = HashMap::default();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::GlobalAddr { name }) = (i.dst, &i.op) {
                derived.insert(d, name.clone());
            }
        }
    }
    if derived.is_empty() {
        return;
    }
    loop {
        let mut grew = false;
        for b in &f.blocks {
            for i in &b.insts {
                if let (Some(d), Op::PtrAdd { base, .. }) = (i.dst, &i.op) {
                    if !derived.contains_key(&d) {
                        if let Some(n) = derived.get(base).cloned() {
                            derived.insert(d, n);
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
    let mark = |v: &Val, taken: &mut HashSet<String>| {
        if let Some(n) = derived.get(v) {
            taken.insert(n.clone());
        }
    };
    let mut buf: Vec<Val> = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            match &i.op {
                Op::GlobalAddr { .. } | Op::Load { .. } | Op::CopyMem { .. } => {}
                Op::Store { val, .. } => mark(val, taken),
                Op::PtrAdd { off, .. } => mark(off, taken),
                other => {
                    buf.clear();
                    other.uses(&mut buf);
                    for v in &buf {
                        mark(v, taken);
                    }
                }
            }
        }
        match &b.term {
            Term::BrCond { cond, .. } => mark(cond, taken),
            Term::Switch { val, .. } => mark(val, taken),
            Term::Ret(Some(v)) => mark(v, taken),
            _ => {}
        }
    }
}
