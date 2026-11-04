// SPDX-License-Identifier: MPL-2.0

use super::format::{Descriptor, FileEntry, FuncEntry};
use crate::codegen_x86::Emitter;
use crate::fir::{Func, Module};
use crate::fx::HashMap;
use std::sync::RwLock;

pub const COUNTERS: &str = "__firn_pgo_counters";
pub const DESC: &str = "__firn_pgo_desc";
pub const PATH: &str = "__firn_pgo_path";
pub const DUMP: &str = "__firn_pgo_dump";
pub const EXIT_HOOK: &str = "__firn_pgo_exit_hook";
const FAIL: &str = "__firn_pgo_dump_done";

struct Plan {
    bases: HashMap<String, (usize, usize)>,
    desc: Descriptor,
}

static PLAN: RwLock<Option<Plan>> = RwLock::new(None);

fn block_locs(f: &Func, bi: usize) -> Vec<(u32, u32, u32)> {
    let mut v: Vec<(u32, u32, u32)> = Vec::new();
    for i in &f.blocks[bi].insts {
        if i.loc.is_none() {
            continue;
        }
        let k = (i.loc.file, i.loc.line, i.loc.col);
        if !v.contains(&k) {
            v.push(k);
        }
    }
    v
}

pub fn plan(m: &Module) {
    if super::gen_path().is_none() {
        return;
    }
    let files: Vec<FileEntry> = crate::dwarf::files()
        .into_iter()
        .map(|p| FileEntry { hash: super::hash::file_hash(&p), path: p })
        .collect();
    let mut bases: HashMap<String, (usize, usize)> = HashMap::default();
    let mut funcs: Vec<FuncEntry> = Vec::new();
    let mut next = 0usize;
    for f in &m.funcs {
        if bases.contains_key(&f.name) {
            continue;
        }
        let n = f.blocks.len();
        bases.insert(f.name.clone(), (next, n));
        next += n;
        funcs.push(FuncEntry {
            name: f.name.clone(),
            hash: super::hash::of(&f.name).unwrap_or(0),
            blocks: (0..n).map(|bi| block_locs(f, bi)).collect(),
        });
    }
    *PLAN.write().unwrap_or_else(|e| e.into_inner()) = Some(Plan { bases, desc: Descriptor { files, funcs } });
}

pub fn enabled() -> bool {
    PLAN.read().unwrap_or_else(|e| e.into_inner()).is_some()
}

pub fn counter_line(fname: &str, nblocks: usize, bi: usize) -> Option<String> {
    let g = PLAN.read().unwrap_or_else(|e| e.into_inner());
    let (base, n) = *g.as_ref()?.bases.get(fname)?;
    if n != nblocks || bi >= n {
        return None;
    }
    Some(format!("inc qword ptr [rip + {} + {}]", COUNTERS, 8 * (base + bi)))
}

pub fn emit_exit_hook_call(e: &mut Emitter) {
    if !enabled() {
        return;
    }
    e.line("lea rsp, [rsp - 128]");
    e.line(&format!("call {}", EXIT_HOOK));
    e.line("lea rsp, [rsp + 128]");
}

pub fn emit_dump_call(e: &mut Emitter) {
    if enabled() {
        e.line(&format!("call {}", DUMP));
    }
}

fn ascii(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() + 16);
    for &b in bytes {
        match b {
            b'"' => s.push_str("\\\""),
            b'\\' => s.push_str("\\\\"),
            b'\n' => s.push_str("\\n"),
            b'\t' => s.push_str("\\t"),
            0x20..=0x7e => s.push(b as char),
            _ => s.push_str(&format!("\\{:03o}", b)),
        }
    }
    s
}

pub fn emit_runtime(e: &mut Emitter) {
    let g = PLAN.read().unwrap_or_else(|e| e.into_inner());
    let plan = match g.as_ref() {
        Some(p) => p,
        None => return,
    };
    let path = match super::gen_path() {
        Some(p) => p.display().to_string(),
        None => return,
    };
    let desc = plan.desc.to_text();
    let ncnt = plan.desc.counters().max(1);
    e.raw(".text");
    e.raw(&format!(".globl {}", EXIT_HOOK));
    e.raw(&format!("{}:", EXIT_HOOK));
    e.line("cmp rax, 60");
    e.line(&format!("je {}", DUMP));
    e.line("cmp rax, 231");
    e.line(&format!("je {}", DUMP));
    e.line("ret");
    e.raw(&format!(".globl {}", DUMP));
    e.raw(&format!("{}:", DUMP));
    for r in SAVED {
        e.line(&format!("push {}", r));
    }
    e.line("mov eax, 2");
    e.line(&format!("lea rdi, [rip + {}]", PATH));
    e.line("mov esi, 577");
    e.line("mov edx, 420");
    e.line("syscall");
    e.line("test rax, rax");
    e.line(&format!("js {}", FAIL));
    e.line("mov r12, rax");
    for (sym, len) in [(DESC, desc.len()), (COUNTERS, ncnt * 8)] {
        e.line("mov eax, 1");
        e.line("mov rdi, r12");
        e.line(&format!("lea rsi, [rip + {}]", sym));
        e.line(&format!("mov rdx, {}", len));
        e.line("syscall");
    }
    e.line("mov eax, 3");
    e.line("mov rdi, r12");
    e.line("syscall");
    e.raw(&format!("{}:", FAIL));
    for r in SAVED.iter().rev() {
        e.line(&format!("pop {}", r));
    }
    e.line("ret");
    e.raw(".section .rodata");
    e.raw(&format!("{}:", PATH));
    e.raw(&format!("    .asciz \"{}\"", ascii(path.as_bytes())));
    e.raw(&format!("{}:", DESC));
    e.raw(&format!("    .ascii \"{}\"", ascii(desc.as_bytes())));
    e.raw(".section .bss,\"aw\",@nobits");
    e.raw("    .p2align 3");
    e.raw(&format!("{}:", COUNTERS));
    e.raw(&format!("    .zero {}", ncnt * 8));
    e.raw(".text");
}

const SAVED: [&str; 10] = ["rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12"];
