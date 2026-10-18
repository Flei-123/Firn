// SPDX-License-Identifier: MPL-2.0
//! **FIR level dead function pruning** (`--prune-fir`, env `FIRN_PRUNE_FIR=1`).
//!
//! The x86 backend already drops unreachable functions from the finished
//! assembly (`codegen_x86::prune_unreachable`), but only AFTER the optimizer
//! and the code generator have worked on every function of every imported
//! module. A `hello` script pulls in ~740 functions (collector, runtime,
//! std) and uses 4: optimizer + codegen are ~60 % of the cold compile time.
//!
//! This pass runs right after lowering and removes every function that no
//! root can reach. Roots: `main`, the collector init, exported functions,
//! the panic handler, the targets of all method tables, and every function
//! named by an inline asm template of a kept function. Edges: `call`,
//! `fnref`, asm templates.
//!
//! It is opt-in and conservative: tables that the compiler writes outside
//! the function bodies (collector type table, panic tables, ...) are not
//! visible here. If one of them names a pruned function the link fails and
//! the caller (`tools/script_port/firn-run`) rebuilds without the pass.
//! Only for hosted x86-64 Linux executables.

use crate::fir::{Module, Op};
use std::collections::{HashMap, HashSet};

pub fn enabled() -> bool {
    std::env::var("FIRN_PRUNE_FIR").map(|v| v == "1").unwrap_or(false)
}

/// Identifiers of an assembler text (a template names `_F0.symbol` labels).
fn idents(text: &str, mut each: impl FnMut(&str)) {
    let b = text.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'.' || c == b'$';
    let mut i = 0usize;
    while i < b.len() {
        if word(b[i]) {
            let st = i;
            while i < b.len() && word(b[i]) {
                i += 1;
            }
            each(&text[st..i]);
        } else {
            i += 1;
        }
    }
}

/// The functions that `f` can reach directly: `call`, `fnref`, the thread
/// entry of a spawn and the labels an inline asm template names.
pub fn direct_refs(
    f: &crate::fir::Func,
    by_name: &dyn Fn(&str) -> Option<usize>,
    by_label: &dyn Fn(&str) -> Option<usize>,
    out: &mut Vec<usize>,
) {
    for b in &f.blocks {
        for ins in &b.insts {
            match &ins.op {
                Op::Call { name, .. } | Op::FnRef { name } => {
                    if let Some(j) = by_name(name.as_str()) {
                        out.push(j);
                    }
                }
                // the spawn sequence calls the thread entry by name
                Op::ThreadSpawn { .. } => {
                    if let Some(j) = by_name(crate::thread::ENTRY) {
                        out.push(j);
                    }
                }
                Op::Asm { template, .. } => idents(template, |s| {
                    if let Some(j) = by_label(s) {
                        out.push(j);
                    }
                }),
                _ => {}
            }
        }
    }
}

/// Names the pruning (and the lazy lowering) start from, besides `main`,
/// the interrupt handlers and the exports: collector init, panic handler,
/// targets of all method tables.
pub fn root_names() -> Vec<String> {
    let mut named: Vec<String> = vec![crate::gc::FN_INIT.to_string()];
    if let Some(h) = crate::panic_rt::handler() {
        named.push(h);
    }
    named.extend(crate::iface::table_targets());
    named
}

/// Is the pruning allowed for this compilation? (hosted x86-64 Linux exe)
pub fn applicable(only_object: bool, exe: bool) -> bool {
    enabled()
        && matches!(crate::target::active(), crate::target::Target::X86_64)
        && !crate::target::windows()
        && !crate::target::shared()
        && !crate::prof::is_kernel()
        && !only_object
        && exe
}

/// Removes the unreachable functions; returns how many were dropped.
pub fn prune(m: &mut Module) -> usize {
    let n = m.funcs.len();
    let mut by_name: HashMap<&str, usize> = HashMap::with_capacity(n);
    let mut by_label: HashMap<String, usize> = HashMap::with_capacity(n);
    for (i, f) in m.funcs.iter().enumerate() {
        by_name.insert(f.name.as_str(), i);
        by_label.insert(crate::codegen_x86::label(&f.name), i);
    }
    let mut keep = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mark = |i: usize, keep: &mut Vec<bool>, stack: &mut Vec<usize>| {
        if !keep[i] {
            keep[i] = true;
            stack.push(i);
        }
    };
    for (i, f) in m.funcs.iter().enumerate() {
        if f.name == "main" || f.interrupt || crate::extfn::export_link_name(&f.name).is_some() {
            mark(i, &mut keep, &mut stack);
        }
    }
    for s in &root_names() {
        if let Some(&i) = by_name.get(s.as_str()) {
            mark(i, &mut keep, &mut stack);
        }
    }
    while let Some(i) = stack.pop() {
        let mut refs: Vec<usize> = Vec::new();
        direct_refs(
            &m.funcs[i],
            &|n| by_name.get(n).copied(),
            &|n| by_label.get(n).copied(),
            &mut refs,
        );
        for j in refs {
            mark(j, &mut keep, &mut stack);
        }
    }
    let dropped = keep.iter().filter(|k| !**k).count();
    let mut live_refs: HashSet<String> = HashSet::new();
    let mut idx = 0usize;
    if dropped > 0 {
        m.funcs.retain(|_| {
            let k = keep[idx];
            idx += 1;
            k
        });
    }
    for f in &m.funcs {
        for b in &f.blocks {
            for ins in &b.insts {
                if let Op::FnRef { name } = &ins.op {
                    live_refs.insert(name.clone());
                }
            }
        }
    }
    crate::fnval::retain_records(|k| live_refs.contains(k));
    dropped
}
