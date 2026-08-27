// SPDX-License-Identifier: GPL-2.0-only
//! **Round 80 — the aarch64 (A64) code generator: FIR -> GNU assembler text.**
//!
//! INTERFACE (the same one `codegen_x86.rs` has, and deliberately so):
//!   `pub fn emit(m: &fir::Module) -> Result<String, String>`
//!
//! This file is the proof that FIR is a machine independent intermediate
//! representation and not a thin coat of paint over x86. It reads exactly
//! the same `fir::Module` the x86 generator reads — the frontend, the type
//! checker, the lowering and the optimizer do not know that this file
//! exists — and produces A64 for Linux.
//!
//! ## The model of the register allocation
//!
//! The same one round 1 chose for x86 and that `codegen_x86.rs` still keeps
//! as its base path: **every FIR value gets its own 8-byte slot in the
//! frame**, computing happens in a handful of scratch registers, and across
//! a `bl` no value lives in a register. That is not fast, and it is not
//! meant to be — it is *checkable*. The linear scan of `regalloc.rs` is a
//! second, separate step that x86 got in round 43; aarch64 can get it the
//! same way later, and until then nothing here can be wrong for a reason
//! that has to do with register lifetimes.
//!
//! Scratch registers (AAPCS64 calls x9-x15 "corruptible"):
//!   * `x9`  — first operand / result  (the `rax` of this file)
//!   * `x10` — second operand          (the `rcx`)
//!   * `x11` — third operand           (the `rdx`)
//!   * `x12` — ADDRESSES ONLY. Whenever a frame offset does not fit into
//!             the immediate field of a `ldr`/`str`, the address is built
//!             here. Nothing else ever lives in x12.
//!   * `x13`, `x14` — helpers (large immediates in comparisons, the store
//!             status of the atomic loops, the byte in the copy loops)
//!   * `x16` — the frame size when it is too large for `sub sp, sp, #imm`
//!   * `d0`, `d1` / `s0`, `s1` — the floating point pair
//!
//! Registers x19-x28 (callee-saved) are never handed out, so the prologue
//! saves none of them. x18 is untouched (the platform register).
//!
//! ## The frame
//!
//! ```text
//!            +--------------------+ <- x29 + 16 + 8k : incoming stack args
//!            | saved x29, x30     | <- x29
//!            | value slots        |
//!            | alloca storage     |
//!            | outgoing stack args| <- sp + 0
//!            +--------------------+ <- sp
//! ```
//!
//! `sp` is set ONCE in the prologue and never moves again. That is the one
//! deliberate difference to the x86 base path (which does `sub rsp` /
//! `add rsp` around every call with more than six arguments): on A64 the
//! offsets of `ldr`/`str` are unsigned and scaled, so an `sp` that stays put
//! is what keeps every slot access a single instruction. The area for the
//! arguments of the WIDEST call in the function is therefore reserved once,
//! at the bottom of the frame — `outgoing` below.
//!
//! Every slot address is `sp + (frame_size - off)`, where `off` is the same
//! downward offset from `x29` that `codegen_x86::layout` computes. That way
//! both files partition the frame by the same rule and only address it
//! differently.
//!
//! ## What this generator does NOT do (round 80, stated rather than hidden)
//!
//!   * `#[interrupt]`, inline assembler and the `kernel` profile — all
//!     three are x86 texts by their nature (`iretq`, Intel syntax operands).
//!   * threads (`Op::ThreadSpawn`, `Op::ThreadSelf`): `clone(2)` takes its
//!     arguments in another order here and the thread pointer sits in
//!     `tpidr_el0` rather than behind `arch_prctl`. That is a runtime
//!     question, not a code generation one.
//!   * debug information (`.loc`, `.debug_info`).
//!
//! Each of them reports itself with a clear message. None of them is
//! silently compiled into something else.

use crate::codegen_x86::{block_label, label, Emitter};
use crate::fir::{BinOp, Block, CmpOp, FTy, Func, Inst, Module, Op, Term, UnOp, Val};
use crate::syscalls;
use std::collections::HashMap;

/// Argument registers of AAPCS64, integer class.
const ARG_REGS: [&str; 8] = ["x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7"];
/// Argument registers of AAPCS64, floating point class (the `d` names; for
/// `f32` the same registers are addressed as `s`).
const FARG_REGS: [&str; 8] = ["d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7"];
/// Argument registers of the Linux system call ABI (the number goes to x8).
const SYS_REGS: [&str; 6] = ["x0", "x1", "x2", "x3", "x4", "x5"];

/// first operand / result
pub(crate) const A: &str = "x9";
/// second operand
pub(crate) const B: &str = "x10";
/// third operand
const C: &str = "x11";
/// addresses only — never a value
const ADDR: &str = "x12";
/// helper (large immediates, loop counters)
const T1: &str = "x13";
/// second helper (store status of the atomic loops)
const T2: &str = "x14";

/// The 32-bit name of a 64-bit register.
pub(crate) fn w(r: &str) -> String {
    format!("w{}", &r[1..])
}

/// The register at the computing width.
fn rw(r: &str, bits: u32) -> String {
    if bits <= 32 {
        w(r)
    } else {
        r.to_string()
    }
}

/// The `s` name of a `d` register (the same register, single precision).
fn sreg(d: &str) -> String {
    format!("s{}", &d[1..])
}

/// The `v` name of the same register — that is how a 128-bit value addresses
/// the argument registers of AAPCS64 (`d3` and `v3` are one register).
fn vreg(d: &str) -> String {
    format!("v{}", &d[1..])
}

/// AAPCS64 gives a stack-passed 128-bit vector sixteen octets AND a sixteen
/// octet boundary; the outgoing area of this backend counts in words of
/// eight. Nothing in this repository passes more than eight vector arguments,
/// so the ninth is REFUSED rather than laid out wrong.
fn check_v128_stack(f: &Func, args: &[Val], spot: &[Option<&'static str>]) -> Result<(), String> {
    for (k, a) in args.iter().enumerate() {
        if spot[k].is_none() && f.val_ty(*a) == FTy::V128 {
            return Err(format!(
                "aarch64: a ninth 'v128' argument in '{}' would travel on the stack, \
                 and this code generator does not lay that out yet (round 91)",
                f.name
            ));
        }
    }
    Ok(())
}

fn align_up(x: u64, a: u64) -> u64 {
    if a <= 1 {
        x
    } else {
        (x + a - 1) / a * a
    }
}

/// Frame partition of a function — the same rule as `codegen_x86::layout`,
/// plus the outgoing argument area.
pub(crate) struct Frame {
    /// slot offset per value id (address = x29 - off)
    slot: Vec<u64>,
    /// offset of the storage per `alloca` value (address = x29 - off)
    alloca_off: Vec<Option<u64>>,
    /// total size of the frame below x29 (16-aligned)
    size: u64,
    /// bytes at the bottom of the frame for the arguments of the widest call
    outgoing: u64,
    /// values defined by `Op::Const` — the system call number has to be one
    consts: HashMap<Val, i128>,
}

impl Frame {
    /// Distance of a slot from `sp` (that is what an `ldr` gets to see).
    fn off(&self, v: Val) -> u64 {
        self.size - self.slot[v as usize]
    }
    fn alloca(&self, v: Val) -> Option<u64> {
        self.alloca_off[v as usize].map(|o| self.size - o)
    }
}

fn layout(f: &Func) -> Frame {
    let n = f.val_types.len();
    let mut slot = vec![0u64; n];
    let mut cursor = 0u64;
    for (idx, s) in slot.iter_mut().enumerate() {
        // ROUND 91: a `v128` value needs SIXTEEN octets, and it wants them at
        // a sixteen octet boundary so that `ldr q`/`str q` reach the slot in
        // ONE instruction (their immediate is scaled by sixteen). `sp` stays
        // put and `size` is rounded up to sixteen, so an offset that is a
        // multiple of sixteen stays one.
        if f.val_types.get(idx) == Some(&FTy::V128) {
            cursor = align_up(cursor + 16, 16);
        } else {
            cursor += 8;
        }
        *s = cursor;
    }
    let mut alloca_off: Vec<Option<u64>> = vec![None; n];
    let mut consts: HashMap<Val, i128> = HashMap::new();
    let mut outgoing = 0u64;
    for b in &f.blocks {
        if b.id != f.entry() && b.insts.iter().any(|i| matches!(i.op, Op::Alloca { .. })) {
            continue;
        }
        for i in &b.insts {
            if let Op::Alloca { size, align } = i.op {
                if let Some(d) = i.dst {
                    let a = if align == 0 { 1 } else { align.min(16) };
                    cursor = align_up(cursor + size.max(1), a);
                    alloca_off[d as usize] = Some(cursor);
                }
            }
        }
    }
    // ROUND 92: only a value with EXACTLY ONE definition is the constant its
    // `const` instruction names. After `phi.rs` a value can be written from
    // several blocks -- see the long note in `regalloc.rs::immediate_consts`.
    let mut defs: HashMap<Val, u32> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                *defs.entry(d).or_insert(0) += 1;
            }
        }
    }
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
                if defs.get(&d).copied().unwrap_or(0) == 1 {
                    consts.insert(d, i.ty.truncate(*c));
                }
            }
            // ROUND ARM-FREESTANDING -- the inline assembler stages its
            // operands in the SAME area. `sp` never moves in this backend
            // (that is what keeps every slot access one instruction), so an
            // `asm` block cannot push and pop the way the x86 path does; it
            // parks its operands at the bottom of the frame instead. That
            // area is free at an `asm` block by construction: it belongs to
            // the arguments of an outgoing CALL, and an `asm` block is not
            // one. Reserved is the wider of the two needs -- one word per
            // input, and one per output plus one for the value form.
            if let Op::Asm { in_regs, out_regs, out, .. } = &i.op {
                let need = in_regs
                    .len()
                    .max(out_regs.len() + usize::from(out.is_some()));
                outgoing = outgoing.max(8 * need as u64);
                continue;
            }
            let args = match &i.op {
                Op::Call { args, .. } | Op::CallIndirect { args, .. } => args.as_slice(),
                _ => continue,
            };
            let (_, stack) = place_args(f, args);
            outgoing = outgoing.max(8 * stack.len() as u64);
        }
    }
    outgoing = align_up(outgoing, 16);
    Frame {
        slot,
        alloca_off,
        size: align_up(cursor + outgoing, 16),
        outgoing,
        consts,
    }
}

// ------------------------------------------------------------- small helpers

/// Builds a 64-bit constant into `r` with `movz`/`movk`/`movn` — the only
/// way A64 knows. Nothing here goes through a literal pool: a pool would
/// need `.ltorg` placement and a second pass, and `movz`+3×`movk` is at most
/// four instructions for ANY value.
pub(crate) fn imm_into(e: &mut Emitter, r: &str, v: i64) {
    let u = v as u64;
    if u == 0 {
        e.line(&format!("mov {}, xzr", r));
        return;
    }
    let c = [u & 0xffff, (u >> 16) & 0xffff, (u >> 32) & 0xffff, (u >> 48) & 0xffff];
    let nz: Vec<usize> = (0..4).filter(|i| c[*i] != 0).collect();
    let nf: Vec<usize> = (0..4).filter(|i| c[*i] != 0xffff).collect();
    let shifted = |val: u64, i: usize| -> String {
        if i == 0 {
            format!("#{}", val)
        } else {
            format!("#{}, lsl #{}", val, 16 * i)
        }
    };
    if nz.len() == 1 {
        e.line(&format!("movz {}, {}", r, shifted(c[nz[0]], nz[0])));
        return;
    }
    // `movn` writes the COMPLEMENT: everything above the named 16-bit chunk
    // becomes ones. So it fits whenever at most one chunk differs from
    // 0xffff -- including -1 itself, where none does (`movn r, #0`).
    if nf.len() <= 1 {
        let i = *nf.first().unwrap_or(&0);
        e.line(&format!("movn {}, {}", r, shifted(!c[i] & 0xffff, i)));
        return;
    }
    let first = nz[0];
    e.line(&format!("movz {}, {}", r, shifted(c[first], first)));
    for i in nz.iter().skip(1) {
        e.line(&format!("movk {}, {}", r, shifted(c[*i], *i)));
    }
}

/// `dst = src + imm` for any immediate.
fn add_imm(e: &mut Emitter, dst: &str, src: &str, imm: u64) {
    if imm == 0 {
        e.line(&format!("mov {}, {}", dst, src));
    } else if imm <= 4095 {
        e.line(&format!("add {}, {}, #{}", dst, src, imm));
    } else if imm % 4096 == 0 && (imm >> 12) <= 4095 {
        e.line(&format!("add {}, {}, #{}, lsl #12", dst, src, imm >> 12));
    } else {
        imm_into(e, dst, imm as i64);
        e.line(&format!("add {}, {}, {}", dst, src, dst));
    }
}

/// A memory operand for `base + off` at access width `scale`. If the offset
/// fits the scaled immediate field it becomes `[base, #off]`; otherwise the
/// address is built in `x12` — which is why nothing but an address ever
/// lives there.
fn at_base(e: &mut Emitter, base: &str, off: u64, scale: u64) -> String {
    if off % scale == 0 && off / scale <= 4095 {
        if off == 0 {
            format!("[{}]", base)
        } else {
            format!("[{}, #{}]", base, off)
        }
    } else {
        add_imm(e, ADDR, base, off);
        format!("[{}]", ADDR)
    }
}

/// The slot of a value as a memory operand.
pub(crate) fn at(e: &mut Emitter, fr: &Frame, v: Val, scale: u64) -> String {
    let off = fr.off(v);
    at_base(e, "sp", off, scale)
}

/// Loads the complete 8-byte slot of a value.
pub(crate) fn load_full(e: &mut Emitter, fr: &Frame, r: &str, v: Val) {
    let m = at(e, fr, v, 8);
    e.line(&format!("ldr {}, {}", r, m));
}

/// Loads a value sign/zero extended to `to_bits` (32 or 64) — the counterpart
/// of `codegen_x86::load_ext`, and it has the same duty: the upper bits of a
/// slot are NOT guaranteed, so an 8/16/32-bit value has to be widened while
/// it is read.
fn load_ext(e: &mut Emitter, fr: &Frame, r: &str, v: Val, ty: FTy, to_bits: u32) {
    let bits = ty.bits().max(8);
    if bits >= to_bits {
        let m = at(e, fr, v, if to_bits <= 32 { 4 } else { 8 });
        e.line(&format!("ldr {}, {}", rw(r, to_bits), m));
        return;
    }
    match (ty.signed(), bits) {
        (true, 8) => {
            let m = at(e, fr, v, 1);
            e.line(&format!("ldrsb {}, {}", rw(r, to_bits), m));
        }
        (true, 16) => {
            let m = at(e, fr, v, 2);
            e.line(&format!("ldrsh {}, {}", rw(r, to_bits), m));
        }
        (true, _) => {
            // ldrsw exists in the 64-bit form only — and only that form is
            // ever needed (32 -> 64).
            let m = at(e, fr, v, 4);
            e.line(&format!("ldrsw {}, {}", r, m));
        }
        (false, 8) => {
            let m = at(e, fr, v, 1);
            e.line(&format!("ldrb {}, {}", w(r), m));
        }
        (false, 16) => {
            let m = at(e, fr, v, 2);
            e.line(&format!("ldrh {}, {}", w(r), m));
        }
        (false, _) => {
            // A write to a `w` register zeroes the upper half — the same
            // rule x86 uses for `mov eax, ...`.
            let m = at(e, fr, v, 4);
            e.line(&format!("ldr {}, {}", w(r), m));
        }
    }
}

/// Writes a register (full 64 bits) into the slot of the target value.
pub(crate) fn store_dst(e: &mut Emitter, fr: &Frame, d: Val, r: &str) {
    let m = at(e, fr, d, 8);
    e.line(&format!("str {}, {}", r, m));
}

/// A floating point value out of its slot into `d0`/`d1` (or `s0`/`s1`).
fn load_fp(e: &mut Emitter, fr: &Frame, x: &str, v: Val, single: bool) {
    if single {
        let m = at(e, fr, v, 4);
        e.line(&format!("ldr {}, {}", sreg(x), m));
    } else {
        let m = at(e, fr, v, 8);
        e.line(&format!("ldr {}, {}", x, m));
    }
}

/// The way back. For `f32` through `w9`: that zeroes the upper half of the
/// word, so the slot holds a defined value and not the leftovers of an
/// earlier instruction — the same reason `codegen_x86::store_xmm` goes
/// through `movd eax`.
fn store_fp(e: &mut Emitter, fr: &Frame, d: Val, x: &str, single: bool) {
    if single {
        e.line(&format!("fmov {}, {}", w(A), sreg(x)));
        store_dst(e, fr, d, A);
    } else {
        let m = at(e, fr, d, 8);
        e.line(&format!("str {}, {}", x, m));
    }
}

/// `cmp reg, imm` for any immediate.
fn cmp_imm(e: &mut Emitter, r: &str, bits: u32, v: i64) {
    if (0..=4095).contains(&v) {
        e.line(&format!("cmp {}, #{}", rw(r, bits), v));
    } else if (-4095..0).contains(&v) {
        e.line(&format!("cmn {}, #{}", rw(r, bits), -v));
    } else {
        imm_into(e, T1, v);
        e.line(&format!("cmp {}, {}", rw(r, bits), rw(T1, bits)));
    }
}

/// A label that appears exactly once in the output. `e.out` only ever grows,
/// so its length is a running number that costs no state.
pub(crate) fn uniq(e: &Emitter, tag: &str) -> String {
    format!(".La64_{}_{}", tag, e.out.len())
}

/// **WHERE does argument number `k` sit?** (AAPCS64 §6.4)
///
/// Two register files, filled independently: integer words go to x0-x7,
/// floating point words to d0-d7. Whatever finds no register of ITS class
/// left travels on the stack, in the order of writing. That is the same
/// shape System V has on x86 — with eight integer registers instead of six,
/// which is the only thing that actually differs here.
fn place_args(f: &Func, args: &[Val]) -> (Vec<Option<&'static str>>, Vec<Val>) {
    let mut int_i = 0usize;
    let mut fp_i = 0usize;
    let mut spot: Vec<Option<&'static str>> = Vec::with_capacity(args.len());
    let mut stack: Vec<Val> = Vec::new();
    for a in args {
        // ROUND 91: AAPCS64 §6.4 knows ONE floating point/SIMD class. A
        // 128-bit vector queues up in it exactly like an `f64` — v0-v7, then
        // the stack.
        if f.val_ty(*a).is_float() || f.val_ty(*a) == FTy::V128 {
            if fp_i < FARG_REGS.len() {
                spot.push(Some(FARG_REGS[fp_i]));
                fp_i += 1;
                continue;
            }
        } else if int_i < ARG_REGS.len() {
            spot.push(Some(ARG_REGS[int_i]));
            int_i += 1;
            continue;
        }
        spot.push(None);
        stack.push(*a);
    }
    (spot, stack)
}

/// Puts the arguments in place: first the stack part (it needs `x9` as a
/// ferry), then the floating point registers, then the integer registers.
fn load_args(e: &mut Emitter, f: &Func, fr: &Frame, args: &[Val], spot: &[Option<&'static str>]) {
    let stack: Vec<Val> = args
        .iter()
        .enumerate()
        .filter(|(k, _)| spot[*k].is_none())
        .map(|(_, a)| *a)
        .collect();
    for (k, a) in stack.iter().enumerate() {
        load_full(e, fr, A, *a);
        let m = at_base(e, "sp", 8 * k as u64, 8);
        e.line(&format!("str {}, {}", A, m));
    }
    for (k, a) in args.iter().enumerate() {
        if let Some(r) = spot[k] {
            if r.starts_with('d') {
                if f.val_ty(*a) == FTy::V128 {
                    crate::simd_a64::vload(e, fr, &vreg(r), *a);
                } else {
                    load_fp(e, fr, r, *a, f.val_ty(*a) == FTy::F32);
                }
            }
        }
    }
    for (k, a) in args.iter().enumerate() {
        if let Some(r) = spot[k] {
            if !r.starts_with('d') {
                load_full(e, fr, r, *a);
            }
        }
    }
}

// ------------------------------------------------------------------ the module

pub fn emit(m: &Module) -> Result<String, String> {
    // ROUND ARM-FREESTANDING: round 80 refused the kernel profile here. It
    // does not any more -- see `emit_start` below and `emit_func`'s
    // `#[interrupt]` arm. What a freestanding object file must NOT have is
    // exactly what the x86 path has not had since round 52: no `_start`, no
    // collector start, no `svc`.
    let freestanding = crate::prof::is_kernel();
    // ROUND 83: `Emitter` is the x86 file's struct and grew an xmm value
    // cache in round 82. This backend never touches it -- an empty cache is
    // the honest initial value, not a special case.
    let mut e = Emitter::default();
    e.raw(&format!(
        "// generated by {} {} — own aarch64 code generator (no LLVM)",
        crate::config::compiler_name(),
        crate::config::VERSION
    ));
    // ROUND 91: `aese`, `sha256su0`, `crc32cb` and `pmull` are OPTIONAL
    // extensions of armv8-a, and GNU as refuses them without being told --
    // "selected processor does not support `aese ...'". Round 87 emitted
    // `crc32cb` without this line; no test in the suite reached that path,
    // so nobody found out. The line is the whole fix, and it costs an
    // unused program nothing: it selects what the ASSEMBLER accepts, not
    // what the processor has. What the processor has is a run time question
    // and `__cpu_features()` answers it.
    e.raw(".arch armv8-a+crypto+crc");
    e.raw(".text");
    // ROUND 91's auxiliary vector pointer is asked for OUTSIDE the start
    // block: `_start` is what saves it, and the data word that holds it is
    // emitted at the very bottom of this function. A freestanding object
    // file has neither -- there is no process start to read an auxiliary
    // vector from.
    let auxv = !freestanding && crate::simd_a64::needs_auxv(m);
    if !freestanding {
    e.raw(".globl _start");
    e.raw("_start:");
    // At process start `sp` points at [argc][argv0]...[0][envp...]. That
    // pointer becomes the FIRST parameter of `main` — the same start block
    // rule the x86 path follows, so `fn main(start: u64)` reads its command
    // line on both machines out of the same place.
    // HOOK gc (ROUND 88): the collector starts itself -- the same rule as on
    // x86-64 (codegen_x86.rs). It stands BEFORE `mov x30, xzr`, because `bl`
    // writes the return address into x30 and would undo the zeroing.
    // ROUND 91: the auxiliary vector, BEFORE anything else. `sp` still
    // points at it here; four instructions keep the pointer so that
    // `__cpu_features()` can read AT_HWCAP later (simd_a64.rs).
    if auxv {
        crate::simd_a64::emit_auxv_save(&mut e);
    }
    if crate::gc::runtime_active() {
        e.line(&format!("bl {}", label(crate::gc::FN_INIT)));
    }
    e.line("mov x29, xzr");
    e.line("mov x30, xzr");
    e.line("mov x0, sp");
    e.line(&format!("and {}, x0, #-16", A));
    e.line(&format!("mov sp, {}", A));
    e.line(&format!("bl {}", label("main")));
    e.line("mov w0, w0");
    e.line("mov x8, #93"); // exit(2) — 60 on x86-64, 93 here
    e.line("svc #0");
    e.line("brk #0");
    }

    // ROUND ARM-FREESTANDING: a freestanding object file has no entry point
    // at all -- the kernel's own assembler file has it, and the linker
    // script says which symbol that is. Demanding `main` here would refuse
    // every kernel. Same rule, same reason, same wording as the x86 side.
    if !freestanding && !m.funcs.iter().any(|f| f.name == "main") {
        return Err("no entry point: 'fn main() -> i32' is missing".to_string());
    }
    for f in &m.funcs {
        emit_func(&mut e, f)?;
    }
    // The data blocks are machine independent (`.quad` labels and numbers);
    // only the alignment directive means something different in the two
    // ports of GNU as, and `target::align` answers that.
    if crate::gc::has_classes() || crate::gc::runtime_active() {
        e.raw(&crate::gc::ty_table_asm());
    }
    if crate::iface::has_interfaces() {
        e.raw(&crate::iface::tables_asm());
    }
    if crate::fnval::has_records() {
        e.raw(&crate::fnval::records_asm());
    }
    // ROUND 83: the message table and the trampoline of the checked
    // arithmetic, once per object file and only when the program contains
    // a checked operation at all (`panic_rt::any_registered`) -- the same
    // guard the x86 path uses, so `release-fast` still pays nothing.
    if crate::panic_rt::any_registered() {
        e.raw(&crate::panic_rt::rodata_asm());
        e.raw(&crate::panic_rt_a64::trampoline_asm());
    }
    // ROUND 89 (statics.rs): the data section of the global variables. The
    // text is identical to the x86-64 one -- `.byte`/`.zero` in
    // `.bss`/`.data`/`.rodata` say the same thing on both machines; only
    // the two instructions that ADDRESS it differ (`Op::GlobalAddr`).
    if crate::statics::any() {
        e.raw(&crate::statics::data_asm());
    }
    if auxv {
        e.raw(&crate::simd_a64::auxv_data_asm());
    }
    e.raw(".section .note.GNU-stack,\"\",%progbits");
    Ok(e.out)
}

/// **ROUND ARM-FREESTANDING — the A64 exception entry.**
///
/// The registers an `#[interrupt]` function has to carry there and back.
/// On x86 that list is a matter of taste within the caller-saved set; here
/// it is not, and for a reason worth writing down: **A64 saves NOTHING by
/// itself.** Where the x86 processor has already pushed `ss:rsp`, `rflags`
/// and `cs:rip` before the first instruction of the handler runs, an A64
/// exception writes the return address into `ELR_EL1` and the flags into
/// `SPSR_EL1` — two SYSTEM registers, not the stack — and jumps into the
/// vector table. Not one general purpose register is touched, which means
/// not one of them may be touched here either until it is safe.
///
/// x0-x18 plus x30 is the whole corruptible set of AAPCS64 (§`REGISTER_A64`
/// in `core.rs`), and x30 is in it because the body of the handler will
/// `bl` somewhere. x19-x28 are callee-saved and this backend never hands
/// them out, so an interrupted thread finds them the way it left them. The
/// floating point and vector registers are NOT saved — the same decision
/// the x86 side made (SPEC §2: in the kernel the FPU state belongs to the
/// interrupted thread, and `#[allow_fp]` is what says somebody thought
/// about it).
const INT_SAVE_A64: [(&str, &str); 10] = [
    ("x0", "x1"),
    ("x2", "x3"),
    ("x4", "x5"),
    ("x6", "x7"),
    ("x8", "x9"),
    ("x10", "x11"),
    ("x12", "x13"),
    ("x14", "x15"),
    ("x16", "x17"),
    ("x18", "x30"),
];
/// 10 pairs of 8 octets each — and already a multiple of sixteen, so `sp`
/// keeps the alignment AAPCS64 demands of it.
const INT_SAVE_A64_BYTES: u64 = 160;

fn emit_func(e: &mut Emitter, f: &Func) -> Result<(), String> {
    let fr = layout(f);
    e.raw("");
    e.raw(&format!(".globl {}", label(&f.name)));
    e.raw(&format!("{}:", label(&f.name)));
    if f.interrupt {
        e.raw("    // interrupt: A64 saves nothing by itself — every");
        e.raw("    // corruptible register belongs to the interrupted thread.");
        e.line(&format!("sub sp, sp, #{}", INT_SAVE_A64_BYTES));
        for (k, (a, b)) in INT_SAVE_A64.iter().enumerate() {
            e.line(&format!("stp {}, {}, [sp, #{}]", a, b, 16 * k));
        }
    }
    e.line("stp x29, x30, [sp, #-16]!");
    e.line("mov x29, sp");
    if fr.size > 0 {
        if fr.size <= 4095 {
            e.line(&format!("sub sp, sp, #{}", fr.size));
        } else {
            imm_into(e, "x16", fr.size as i64);
            e.line("sub sp, sp, x16");
        }
    }
    // The parameters into their slots. `place_args` is asked with the
    // parameter VALUES, so the caller's rule and the callee's rule are
    // literally the same function.
    let pvals: Vec<Val> = (0..f.params.len() as Val).collect();
    let (spot, _stack) = place_args(f, &pvals);
    check_v128_stack(f, &pvals, &spot)?;
    let mut stack_i = 0usize;
    for (i, _t) in f.params.iter().enumerate() {
        match spot[i] {
            Some(r) if r.starts_with('d') => {
                if f.params[i] == FTy::V128 {
                    crate::simd_a64::vstore(e, &fr, i as Val, &vreg(r));
                } else {
                    store_fp(e, &fr, i as Val, r, f.params[i] == FTy::F32);
                }
            }
            Some(r) => store_dst(e, &fr, i as Val, r),
            None => {
                // Incoming stack arguments sit ABOVE the saved x29/x30 pair.
                let off = 16 + 8 * stack_i as u64;
                stack_i += 1;
                let m = at_base(e, "x29", off, 8);
                e.line(&format!("ldr {}, {}", A, m));
                store_dst(e, &fr, i as Val, A);
            }
        }
    }
    // ROUND 83: one counter per function, so that two checked sites in
    // the same function get two label pairs and two functions never
    // collide -- the same rule (and the same struct) the x86 side uses.
    let mut site = crate::panic_rt::SiteCounter::new(&f.name);
    for b in &f.blocks {
        e.raw(&format!("{}:", block_label(&f.name, b.id)));
        emit_block(e, f, &fr, b, &mut site)?;
    }
    Ok(())
}

fn emit_epilogue(e: &mut Emitter, fr: &Frame, interrupt: bool) {
    if fr.size > 0 {
        if fr.size <= 4095 {
            e.line(&format!("add sp, sp, #{}", fr.size));
        } else {
            imm_into(e, "x16", fr.size as i64);
            e.line("add sp, sp, x16");
        }
    }
    e.line("ldp x29, x30, [sp], #16");
    if interrupt {
        // ROUND ARM-FREESTANDING: backwards, then `eret`. That one
        // instruction restores the program counter out of `ELR_EL1` and the
        // flags out of `SPSR_EL1` at the same time — a `ret` would jump to
        // whatever x30 happens to hold and leave the exception level where
        // it is, which is not a return but a second fault waiting.
        for (k, (a, b)) in INT_SAVE_A64.iter().enumerate() {
            e.line(&format!("ldp {}, {}, [sp, #{}]", a, b, 16 * k));
        }
        e.line(&format!("add sp, sp, #{}", INT_SAVE_A64_BYTES));
        e.line("eret");
        return;
    }
    e.line("ret");
}

fn emit_block(
    e: &mut Emitter,
    f: &Func,
    fr: &Frame,
    b: &Block,
    site: &mut crate::panic_rt::SiteCounter,
) -> Result<(), String> {
    for i in &b.insts {
        emit_inst(e, f, fr, i, site)?;
    }
    match &b.term {
        Term::Br(t) => e.line(&format!("b {}", block_label(&f.name, *t))),
        Term::Switch { .. } => emit_switch(e, f, fr, &b.term)?,
        Term::BrCond { cond, then_bb, else_bb } => {
            // SPEC §9.2 — the same rule as on x86: inside `#[constant_time]`
            // no conditional branch may depend on a secret value.
            if f.constant_time && f.is_secret(*cond) {
                return Err(format!(
                    "#[constant_time]: conditional jump in '{}' depends on a secret value (%{})",
                    f.name, cond
                ));
            }
            if f.val_ty(*cond) != FTy::Bool {
                return Err(format!(
                    "internal error: condition %{} in '{}' is {}, expected bool",
                    cond,
                    f.name,
                    f.val_ty(*cond).name()
                ));
            }
            let m = at(e, fr, *cond, 1);
            e.line(&format!("ldrb {}, {}", w(A), m));
            e.line(&format!("cbnz {}, {}", w(A), block_label(&f.name, *then_bb)));
            e.line(&format!("b {}", block_label(&f.name, *else_bb)));
        }
        Term::Ret(v) => {
            if let Some(v) = v {
                if f.ret == FTy::V128 {
                    // AAPCS64 hands a 128-bit vector back in v0.
                    crate::simd_a64::vload(e, fr, "v0", *v);
                } else if f.ret.is_float() {
                    load_fp(e, fr, "d0", *v, f.ret == FTy::F32);
                } else {
                    load_full(e, fr, "x0", *v);
                }
            }
            emit_epilogue(e, fr, f.interrupt);
        }
        Term::Unset => {
            return Err(format!(
                "internal error: block bb{} in '{}' has no terminator",
                b.id, f.name
            ))
        }
    }
    Ok(())
}

// -------------------------------------------------------------------- switch

/// from this many cases onwards a table pays off (the same numbers as
/// `codegen_switch.rs`, so both machines branch the same way)
const MIN_TABLE_CASES: usize = 8;
const MIN_DENSITY: usize = 40;
const MAX_TABLE_ENTRIES: i128 = 65536;

fn emit_switch(e: &mut Emitter, f: &Func, fr: &Frame, term: &Term) -> Result<(), String> {
    let (val, ty, cases, default) = match term {
        Term::Switch { val, ty, cases, default } => (*val, *ty, cases, *default),
        _ => return Err("internal error: emit_switch without switch".to_string()),
    };
    if f.constant_time && f.is_secret(val) {
        return Err(format!(
            "#[constant_time]: switch in '{}' depends on a secret value (%{})",
            f.name, val
        ));
    }
    if ty == FTy::Void {
        return Err("internal error: switch over void".to_string());
    }
    let dflt = block_label(&f.name, default);
    if cases.is_empty() {
        e.line(&format!("b {}", dflt));
        return Ok(());
    }
    let bits = if ty.bits() > 32 { 64 } else { 32 };
    load_ext(e, fr, A, val, ty, bits);

    let min = cases.iter().map(|(k, _)| *k).min().unwrap();
    let max = cases.iter().map(|(k, _)| *k).max().unwrap();
    let extent = max - min + 1;
    let table = cases.len() >= MIN_TABLE_CASES
        && extent > 0
        && extent <= MAX_TABLE_ENTRIES
        && (cases.len() as i128) * 100 / extent >= MIN_DENSITY as i128;
    if !table {
        for (k, target) in cases.iter() {
            cmp_imm(e, A, bits, *k as i64);
            e.line(&format!("b.eq {}", block_label(&f.name, *target)));
        }
        e.line(&format!("b {}", dflt));
        return Ok(());
    }
    // index = value - min; outside of [0, extent) control goes to default.
    if min != 0 {
        imm_into(e, T1, min as i64);
        e.line(&format!("sub {}, {}, {}", rw(A, bits), rw(A, bits), rw(T1, bits)));
    }
    cmp_imm(e, A, bits, (extent - 1) as i64);
    e.line(&format!("b.hi {}", dflt));
    if bits == 32 {
        // A write to a `w` register zeroed the upper half, and the range
        // check above proved the index is inside the table.
        e.line(&format!("mov {}, {}", w(A), w(A)));
    }
    let lbl = uniq(e, "tbl");
    e.line(&format!("adrp {}, {}", ADDR, lbl));
    e.line(&format!("add {}, {}, :lo12:{}", ADDR, ADDR, lbl));
    e.line(&format!("ldr {}, [{}, {}, lsl #3]", B, ADDR, A));
    e.line(&format!("br {}", B));
    e.raw(".section .rodata");
    e.raw(&crate::target::align(8));
    e.raw(&format!("{}:", lbl));
    let mut i = 0usize;
    let mut k = min;
    while k <= max {
        while i < cases.len() && cases[i].0 < k {
            i += 1;
        }
        if i < cases.len() && cases[i].0 == k {
            e.raw(&format!(".quad {}", block_label(&f.name, cases[i].1)));
        } else {
            e.raw(&format!(".quad {}", dflt));
        }
        k += 1;
    }
    e.raw(".text");
    Ok(())
}

// -------------------------------------------------------------- instructions

fn emit_inst(
    e: &mut Emitter,
    f: &Func,
    fr: &Frame,
    i: &Inst,
    site: &mut crate::panic_rt::SiteCounter,
) -> Result<(), String> {
    let ty = i.ty;
    match &i.op {
        Op::Const(c) => {
            let d = i.dst.ok_or("internal error: const without target")?;
            imm_into(e, A, ty.truncate(*c) as i64);
            store_dst(e, fr, d, A);
        }
        Op::Bin(op, a, b) => {
            let d = i.dst.ok_or("internal error: binary operation without target")?;
            emit_bin(e, fr, *op, ty, *a, *b, d)?;
        }
        Op::Cmp { op, ty: oty, a, b } => {
            let d = i.dst.ok_or("internal error: comparison without target")?;
            if oty.is_float() {
                // `fcmp` sets N/Z/C/V so that the UNORDERED case (NaN) is
                // C=1, V=1, N=0, Z=0. With that, `mi`, `gt`, `ls`, `ge` and
                // `eq` are all false and `ne` is true — exactly what
                // IEEE-754 demands, without a single extra instruction.
                // On x86 the same result needs the parity flag and a swap of
                // the operands; that is the whole difference.
                let single = *oty == FTy::F32;
                load_fp(e, fr, "d0", *a, single);
                load_fp(e, fr, "d1", *b, single);
                if single {
                    e.line("fcmp s0, s1");
                } else {
                    e.line("fcmp d0, d1");
                }
                let cc = match op {
                    CmpOp::Eq => "eq",
                    CmpOp::Ne => "ne",
                    CmpOp::Lt => "mi",
                    CmpOp::Le => "ls",
                    CmpOp::Gt => "gt",
                    CmpOp::Ge => "ge",
                };
                e.line(&format!("cset {}, {}", w(A), cc));
                store_dst(e, fr, d, A);
                return Ok(());
            }
            let bits = if oty.bits() > 32 { 64 } else { 32 };
            load_ext(e, fr, A, *a, *oty, bits);
            load_ext(e, fr, B, *b, *oty, bits);
            e.line(&format!("cmp {}, {}", rw(A, bits), rw(B, bits)));
            let cc = match (op, oty.signed()) {
                (CmpOp::Eq, _) => "eq",
                (CmpOp::Ne, _) => "ne",
                (CmpOp::Lt, true) => "lt",
                (CmpOp::Lt, false) => "lo",
                (CmpOp::Le, true) => "le",
                (CmpOp::Le, false) => "ls",
                (CmpOp::Gt, true) => "gt",
                (CmpOp::Gt, false) => "hi",
                (CmpOp::Ge, true) => "ge",
                (CmpOp::Ge, false) => "hs",
            };
            e.line(&format!("cset {}, {}", w(A), cc));
            store_dst(e, fr, d, A);
        }
        Op::Un(op, a) => {
            let d = i.dst.ok_or("internal error: unary operation without target")?;
            if ty.is_float() {
                if !matches!(op, UnOp::Neg) {
                    return Err(format!("internal error: '!' is not defined for {}", ty.name()));
                }
                let single = ty == FTy::F32;
                load_fp(e, fr, "d0", *a, single);
                if single {
                    e.line("fneg s0, s0");
                } else {
                    e.line("fneg d0, d0");
                }
                store_fp(e, fr, d, "d0", single);
                return Ok(());
            }
            let bits = if ty.bits() > 32 { 64 } else { 32 };
            load_full(e, fr, A, *a);
            match op {
                UnOp::Neg => e.line(&format!("neg {}, {}", rw(A, bits), rw(A, bits))),
                UnOp::Not => {
                    if ty == FTy::Bool {
                        e.line(&format!("eor {}, {}, #1", w(A), w(A)));
                    } else {
                        e.line(&format!("mvn {}, {}", rw(A, bits), rw(A, bits)));
                    }
                }
            }
            store_dst(e, fr, d, A);
        }
        Op::Cast { src, from } => {
            let d = i.dst.ok_or("internal error: conversion without target")?;
            if ty.is_float() && from.is_float() {
                if ty == *from {
                    load_full(e, fr, A, *src);
                    store_dst(e, fr, d, A);
                    return Ok(());
                }
                load_fp(e, fr, "d0", *src, *from == FTy::F32);
                if ty == FTy::F64 {
                    e.line("fcvt d0, s0");
                } else {
                    e.line("fcvt s0, d0");
                }
                store_fp(e, fr, d, "d0", ty == FTy::F32);
                return Ok(());
            }
            if ty.is_float() {
                // Integer -> floating point. The value is widened to 64 bits
                // FIRST and then converted as a signed one — literally what
                // `cvtsi2sd` does on x86, including its reservation about
                // unsigned values above 2^63 (SPEC §14.1.f64).
                load_ext(e, fr, A, *src, *from, 64);
                if ty == FTy::F32 {
                    e.line(&format!("scvtf s0, {}", A));
                } else {
                    e.line(&format!("scvtf d0, {}", A));
                }
                store_fp(e, fr, d, "d0", ty == FTy::F32);
                return Ok(());
            }
            if from.is_float() {
                // Floating point -> integer, cutting towards zero.
                //
                // DIFFERENCE, and it is a real one: `fcvtzs` SATURATES
                // (NaN -> 0, too large -> the largest representable value),
                // while `cvttsd2si` yields 0x8000000000000000 for every case
                // it cannot represent. Both are outside what the language
                // defines; docs/ROUND80.md names it.
                load_fp(e, fr, "d0", *src, *from == FTy::F32);
                if *from == FTy::F32 {
                    e.line(&format!("fcvtzs {}, s0", A));
                } else {
                    e.line(&format!("fcvtzs {}, d0", A));
                }
                store_dst(e, fr, d, A);
                return Ok(());
            }
            if ty == FTy::Bool {
                // Safety net: bool holds 0/1 only. Only the lower `from` bits
                // decide, so they are read zero extended and tested.
                let bits = from.bits().max(8);
                let to = if bits > 32 { 64 } else { 32 };
                match bits {
                    8 => {
                        let m = at(e, fr, *src, 1);
                        e.line(&format!("ldrb {}, {}", w(A), m));
                    }
                    16 => {
                        let m = at(e, fr, *src, 2);
                        e.line(&format!("ldrh {}, {}", w(A), m));
                    }
                    32 => {
                        let m = at(e, fr, *src, 4);
                        e.line(&format!("ldr {}, {}", w(A), m));
                    }
                    _ => load_full(e, fr, A, *src),
                }
                e.line(&format!("cmp {}, #0", rw(A, to)));
                e.line(&format!("cset {}, ne", w(A)));
            } else {
                load_ext(e, fr, A, *src, *from, 64);
            }
            store_dst(e, fr, d, A);
        }
        Op::GcAddr { regs } => {
            let d = i.dst.ok_or("internal error: gc_state without target")?;
            emit_gc_addr(e, *regs);
            store_dst(e, fr, d, A);
        }
        Op::Alloca { .. } => {
            let d = i.dst.ok_or("internal error: alloca without target")?;
            let off = fr.alloca(d).ok_or("internal error: alloca without space")?;
            add_imm(e, A, "sp", off);
            store_dst(e, fr, d, A);
        }
        Op::Load { addr } | Op::MmioLoad { addr } => {
            let d = i.dst.ok_or("internal error: load without target")?;
            // ROUND 91: sixteen octets through a pointer out of the program.
            if ty == FTy::V128 {
                crate::simd_a64::emit_ptr_load(e, fr, d, *addr);
                return Ok(());
            }
            load_full(e, fr, B, *addr);
            let bits = ty.bits().max(8);
            match bits {
                8 => e.line(&format!("ldrb {}, [{}]", w(A), B)),
                16 => e.line(&format!("ldrh {}, [{}]", w(A), B)),
                32 => e.line(&format!("ldr {}, [{}]", w(A), B)),
                _ => e.line(&format!("ldr {}, [{}]", A, B)),
            }
            store_dst(e, fr, d, A);
        }
        Op::Store { addr, val } | Op::MmioStore { addr, val } => {
            if ty == FTy::V128 {
                crate::simd_a64::emit_ptr_store(e, fr, *addr, *val);
                return Ok(());
            }
            load_full(e, fr, B, *addr);
            load_full(e, fr, A, *val);
            let bits = ty.bits().max(8);
            match bits {
                8 => e.line(&format!("strb {}, [{}]", w(A), B)),
                16 => e.line(&format!("strh {}, [{}]", w(A), B)),
                32 => e.line(&format!("str {}, [{}]", w(A), B)),
                _ => e.line(&format!("str {}, [{}]", A, B)),
            }
        }
        Op::PtrAdd { base, off } => {
            let d = i.dst.ok_or("internal error: ptradd without target")?;
            load_full(e, fr, A, *base);
            load_full(e, fr, B, *off);
            e.line(&format!("add {}, {}, {}", A, A, B));
            store_dst(e, fr, d, A);
        }
        Op::Call { name, args } => {
            let (spot, _stack) = place_args(f, args);
            check_v128_stack(f, args, &spot)?;
            load_args(e, f, fr, args, &spot);
            e.line(&format!("bl {}", label(name)));
            if let Some(d) = i.dst {
                if ty == FTy::V128 {
                    crate::simd_a64::vstore(e, fr, d, "v0");
                } else if ty.is_float() {
                    store_fp(e, fr, d, "d0", ty == FTy::F32);
                } else {
                    store_dst(e, fr, d, "x0");
                }
            }
        }
        Op::CallIndirect { target, args } => {
            let (spot, _stack) = place_args(f, args);
            check_v128_stack(f, args, &spot)?;
            load_args(e, f, fr, args, &spot);
            // x9 is no argument register — the target may be loaded last
            // without destroying anything that is already in place.
            load_full(e, fr, A, *target);
            e.line(&format!("blr {}", A));
            if let Some(d) = i.dst {
                if ty == FTy::V128 {
                    crate::simd_a64::vstore(e, fr, d, "v0");
                } else if ty.is_float() {
                    store_fp(e, fr, d, "d0", ty == FTy::F32);
                } else {
                    store_dst(e, fr, d, "x0");
                }
            }
        }
        Op::VtabAddr { table } => {
            let d = i.dst.ok_or("internal error: vtab without target")?;
            let l = crate::iface::table_label(table);
            e.line(&format!("adrp {}, {}", A, l));
            e.line(&format!("add {}, {}, :lo12:{}", A, A, l));
            store_dst(e, fr, d, A);
        }
        Op::FnRef { name } => {
            let d = i.dst.ok_or("internal error: fnref without target")?;
            let l = crate::fnval::record_label(name);
            e.line(&format!("adrp {}, {}", A, l));
            e.line(&format!("add {}, {}, :lo12:{}", A, A, l));
            store_dst(e, fr, d, A);
        }
        // ROUND 89 (statics.rs): aarch64 has no rip-relative addressing
        // mode — the address of a global is built out of a PAGE
        // (`adrp`, +/-4 GiB, 4 KiB granular) and the offset inside that
        // page (`add ..., :lo12:`). Two instructions, one address; the
        // linker fills both relocations.
        Op::GlobalAddr { name } => {
            let d = i.dst.ok_or("internal error: globaladdr without target")?;
            let l = crate::statics::label_of(name);
            e.line(&format!("adrp {}, {}", A, l));
            e.line(&format!("add {}, {}, :lo12:{}", A, A, l));
            store_dst(e, fr, d, A);
        }
        Op::Syscall { args } => emit_syscall(e, fr, i, args)?,
        Op::Select { cond, a, b } => {
            // Data independent choice: `csel`, never a branch (SPEC §9.2).
            let d = i.dst.ok_or("internal error: select without target")?;
            load_full(e, fr, C, *cond);
            load_full(e, fr, B, *a);
            load_full(e, fr, A, *b);
            e.line(&format!("tst {}, #255", C));
            e.line(&format!("csel {}, {}, {}, ne", A, B, A));
            store_dst(e, fr, d, A);
        }
        // ROUND 92 -- see the same arm in `codegen_x86.rs`. This machine has
        // no register allocation at all, so every value lives in the frame
        // and a copy is one `ldr` plus one `str`.
        Op::Copy { src } => {
            let d = i.dst.ok_or("internal error: copy without target")?;
            load_full(e, fr, A, *src);
            store_dst(e, fr, d, A);
        }
        Op::Phi { .. } => {
            return Err("internal error: phi in the code generator (phi.rs did not run)".into())
        }
        Op::Barrier { val } => {
            let d = i.dst.ok_or("internal error: barrier without target")?;
            load_full(e, fr, A, *val);
            e.raw("    // barrier: opaque to every optimization pass");
            store_dst(e, fr, d, A);
        }
        Op::SecureZero { addr, size } => {
            load_full(e, fr, A, *addr);
            load_full(e, fr, B, *size);
            let end = uniq(e, "sz_end");
            let top = uniq(e, "sz_top");
            e.line(&format!("cbz {}, {}", B, end));
            e.raw(&format!("{}:", top));
            e.line(&format!("strb wzr, [{}], #1", A));
            e.line(&format!("subs {}, {}, #1", B, B));
            e.line(&format!("b.ne {}", top));
            e.raw(&format!("{}:", end));
        }
        Op::AtomicAdd { addr, val } => {
            // Round 47 on this machine: without the large system extensions
            // (`ldaddal`) an atomic add is the exclusive pair in a loop. The
            // result is the OLD value, exactly as `lock xadd` gives it.
            let d = i.dst.ok_or("internal error: atomadd without target")?;
            load_full(e, fr, B, *addr);
            load_full(e, fr, C, *val);
            let top = uniq(e, "atomadd");
            e.raw(&format!("{}:", top));
            e.line(&format!("ldaxr {}, [{}]", A, B));
            e.line(&format!("add {}, {}, {}", T1, A, C));
            e.line(&format!("stlxr {}, {}, [{}]", w(T2), T1, B));
            e.line(&format!("cbnz {}, {}", w(T2), top));
            store_dst(e, fr, d, A);
        }
        Op::AtomicCas { addr, erw, new } => {
            let d = i.dst.ok_or("internal error: atomcas without target")?;
            load_full(e, fr, B, *addr);
            load_full(e, fr, C, *erw);
            load_full(e, fr, T1, *new);
            let top = uniq(e, "cas");
            let out = format!("{}_out", top);
            let done = format!("{}_done", top);
            e.raw(&format!("{}:", top));
            e.line(&format!("ldaxr {}, [{}]", A, B));
            e.line(&format!("cmp {}, {}", A, C));
            e.line(&format!("b.ne {}", out));
            e.line(&format!("stlxr {}, {}, [{}]", w(T2), T1, B));
            e.line(&format!("cbnz {}, {}", w(T2), top));
            e.line(&format!("b {}", done));
            e.raw(&format!("{}:", out));
            // The exclusive monitor stays open when the store is skipped.
            e.line("clrex");
            e.raw(&format!("{}:", done));
            store_dst(e, fr, d, A);
        }
        Op::ThreadSpawn { arg, stack, ctid } => {
            let d = i.dst.ok_or("internal error: spawn without target")?;
            load_full(e, fr, "x0", *arg);
            load_full(e, fr, "x1", *stack);
            load_full(e, fr, "x2", *ctid);
            crate::thread::spawn_sequence_a64(e);
            store_dst(e, fr, d, "x0");
        }
        Op::ThreadSelf => {
            // The counterpart of `mov rax, qword ptr fs:0`. AArch64 keeps the
            // thread pointer in a system register that EL0 may read AND
            // write, so the whole `arch_prctl` detour falls away here
            // (`syscalls.rs`, `SetThreadPointer`). Before anything sets it,
            // the register is 0 — the same starting value `fs` has, which is
            // what `__thread_tcb` in lib/gc/gc.fi checks for.
            let d = i.dst.ok_or("internal error: threadself without target")?;
            e.line(&format!("mrs {}, tpidr_el0", A));
            store_dst(e, fr, d, A);
        }
        Op::CopyMem { dst, src, size } => {
            if *size == 0 {
                return Ok(());
            }
            load_full(e, fr, A, *dst);
            load_full(e, fr, B, *src);
            // Up to four words: straight line. Anything bigger: a byte loop.
            if *size <= 32 && size % 8 == 0 {
                for k in 0..(*size / 8) {
                    e.line(&format!("ldr {}, [{}, #{}]", T1, B, 8 * k));
                    e.line(&format!("str {}, [{}, #{}]", T1, A, 8 * k));
                }
                return Ok(());
            }
            imm_into(e, C, *size as i64);
            let top = uniq(e, "cpy");
            e.raw(&format!("{}:", top));
            e.line(&format!("ldrb {}, [{}], #1", w(T1), B));
            e.line(&format!("strb {}, [{}], #1", w(T1), A));
            e.line(&format!("subs {}, {}, #1", C, C));
            e.line(&format!("b.ne {}", top));
        }
        // ROUND ARM-FREESTANDING (round 80 refused this) -- inline
        // assembler on A64. ALWAYS volatile: the lines stand exactly once
        // and exactly here.
        //
        // The one real difference to the x86 path is where the operands
        // wait. `codegen_x86` writes `push`/`pop` around the result
        // registers; this backend cannot, because `sp` is set once in the
        // prologue and every frame slot is addressed relative to it (moving
        // `sp` would move all of them). So the operands are parked in the
        // OUTGOING ARGUMENT AREA at the bottom of the frame, which `layout`
        // has widened for exactly this and which is free at an `asm` block
        // because an `asm` block is not a call.
        //
        // Parking is what makes the whole thing safe against the user
        // naming one of this backend's own scratch registers: x12 (address
        // building) and x13 (helper) are both allowed operand names, so
        // every register that still carries a value is written to memory
        // BEFORE any address is computed with them.
        Op::Asm { template, out, in_regs, ins, out_regs, outs, clobber } => {
            e.raw("    // asm (volatile): must be neither removed nor moved");
            // 1. Every input value into the parking area first, ...
            for (k, v) in ins.iter().enumerate() {
                load_full(e, fr, T1, *v);
                e.line(&format!("str {}, [sp, #{}]", T1, 8 * k));
            }
            // ... and only then into the registers the template names. Two
            // passes, because reading a frame slot may build its address in
            // x12 -- which may itself be one of those registers.
            for (k, r) in in_regs.iter().enumerate() {
                let stem = crate::core::stem(r)
                    .ok_or_else(|| format!("unknown asm register '{}'", r))?;
                e.line(&format!("ldr {}, [sp, #{}]", stem, 8 * k));
            }
            for line in template.split('\n') {
                e.line(line);
            }
            // 2. Everything the template produced out of the registers and
            // into the parking area, before a single address is built.
            for (k, r) in out_regs.iter().enumerate() {
                let stem = crate::core::stem(r)
                    .ok_or_else(|| format!("unknown asm register '{}'", r))?;
                e.line(&format!("str {}, [sp, #{}]", stem, 8 * k));
            }
            if let Some(r) = out {
                let stem = crate::core::stem(r)
                    .ok_or_else(|| format!("unknown asm register '{}'", r))?;
                e.line(&format!("str {}, [sp, #{}]", stem, 8 * outs.len()));
            }
            // 3. The value form into its slot.
            if let Some(_) = out {
                let d = i.dst.ok_or("internal error: asm with out but without target")?;
                e.line(&format!("ldr {}, [sp, #{}]", T1, 8 * outs.len()));
                store_dst(e, fr, d, T1);
            }
            // 4. ROUND 68's memory outputs: the register goes into `*p`.
            // The parked VALUE is read first and the ADDRESS built second --
            // the address building is the step that may use x12.
            for k in 0..outs.len() {
                e.line(&format!("ldr {}, [sp, #{}]", T1, 8 * k));
                load_full(e, fr, ADDR, outs[k]);
                e.line(&format!("str {}, [{}]", T1, ADDR));
            }
            // The clobber list costs nothing on this path and is written
            // down all the same: on the base path no FIR value survives in a
            // register across an instruction, so there is nothing to rescue.
            // `regalloc.rs` -- the one pass for which the list would matter
            // -- refuses a function with an `asm` block outright
            // (`regalloc.rs`, "Inline-Assembler"), and it is an x86 pass
            // besides.
            if !clobber.is_empty() {
                e.raw(&format!("    // asm clobber: {}", clobber.join(", ")));
            }
        }
        // ROUND 83 -- the checked arithmetic of round 72 on this machine
        // (docs/ROUND80.md section 7, panic_rt_a64.rs). Both operands are
        // read extended to a full 64 bits, exactly as the x86 path reads
        // them; the check itself computes at the type's own width.
        Op::CheckedBin { op, a, b, msg } => {
            let d = i.dst.ok_or("internal error: checked binary operation without target")?;
            load_ext(e, fr, A, *a, ty, 64);
            load_ext(e, fr, B, *b, ty, 64);
            crate::panic_rt_a64::emit_checked_bin(e, *op, ty, msg, site);
            store_dst(e, fr, d, A);
        }
        Op::CheckedDiv { op, a, b, msg_zero, msg_range } => {
            let d = i.dst.ok_or("internal error: checked division without target")?;
            load_ext(e, fr, A, *a, ty, 64);
            load_ext(e, fr, B, *b, ty, 64);
            crate::panic_rt_a64::emit_checked_div(e, *op, ty, msg_zero, msg_range, site);
            store_dst(e, fr, d, A);
        }
        // ROUND 89 -- the checked ARRAY INDEX (SPEC section 13, item L9).
        // The index is a `usize`, so ONE unsigned comparison against the
        // length decides both ends at once.
        Op::CheckedIdx { idx, len, msg } => {
            let d = i.dst.ok_or("internal error: checked index without target")?;
            load_ext(e, fr, A, *idx, ty, 64);
            crate::panic_rt_a64::emit_checked_idx(e, *len, msg, site);
            store_dst(e, fr, d, A);
        }
        Op::CheckedCast { src, from, msg } => {
            let d = i.dst.ok_or("internal error: checked cast without target")?;
            load_ext(e, fr, A, *src, *from, 64);
            crate::panic_rt_a64::emit_checked_cast(e, *from, ty, msg, site);
            store_dst(e, fr, d, A);
        }
        Op::BinWrapSat { kind, op, a, b } => {
            let d = i.dst.ok_or("internal error: wrap/sat binary operation without target")?;
            load_ext(e, fr, A, *a, ty, 64);
            load_ext(e, fr, B, *b, ty, 64);
            crate::panic_rt_a64::emit_wrap_sat(e, *kind, *op, ty, site)?;
            store_dst(e, fr, d, A);
        }
        // ROUND 82 on ROUND 80, corrected in ROUND 87, FINISHED IN ROUND 91.
        //
        // Round 82 gave the language `v128` and 42 intrinsics and wrote them
        // for x86-64. Round 80's code generator refused every one of them,
        // which made `lib/std/crypto/accel.fi` -- and with it the whole
        // crypto library -- uncompilable for this machine: `tests/1613_
        // crypto.fi` was the LAST case of `tools/aarch64/run.sh` that
        // differed between the two machines.
        //
        // `simd_a64.rs` is the other half. It stands to this file as
        // `simd.rs` stands to `codegen_x86.rs`, and this match is exhaustive:
        // there is no vector instruction left that this backend refuses.
        Op::Simd { .. } => crate::simd_a64::emit(e, fr, i)?,
    }
    Ok(())
}

/// `Op::GcAddr` — address of the state block of the collector in `x9`.
fn emit_gc_addr(e: &mut Emitter, regs: bool) {
    let l = crate::gc::STATE_LABEL;
    e.line(&format!("adrp {}, {}", A, l));
    e.line(&format!("add {}, {}, :lo12:{}", A, A, l));
    if !regs {
        return;
    }
    // The conservative register scan (SPEC §3.5.3) reads six words out of
    // the state block. On this path no value ever lives in a callee-saved
    // register across a call — everything is in the frame, and the frame is
    // scanned. The six words are filled with the callee-saved registers all
    // the same, so the block means the same thing on both machines.
    let off = crate::gc::REG_SAVE_OFF;
    for (i, r) in ["x19", "x20", "x21", "x22", "x23", "x24"].iter().enumerate() {
        let m = at_base(e, A, off + 8 * i as u64, 8);
        e.line(&format!("str {}, {}", r, m));
    }
}

/// `Op::Syscall` — `svc #0`, the number in x8, the arguments in x0-x5.
///
/// The number in FIR is the x86-64 number (see `syscalls.rs`); it has to be
/// a CONSTANT here, because the translation happens at compile time. Every
/// `syscall` in this repository writes a literal or a `const`, so the
/// requirement costs nothing — and a computed number is refused rather than
/// run as the wrong call.
fn emit_syscall(e: &mut Emitter, fr: &Frame, i: &Inst, args: &[Val]) -> Result<(), String> {
    if args.is_empty() {
        return Err("internal error: syscall without number".to_string());
    }
    if args.len() > 7 {
        return Err("syscall with more than 6 arguments".to_string());
    }
    let nr = *fr.consts.get(&args[0]).ok_or_else(|| {
        "aarch64: the system call number has to be a constant (it is translated at compile time, see syscalls.rs)"
            .to_string()
    })? as i64;
    let form = syscalls::aarch64(nr)
        .ok_or_else(|| format!("aarch64: system call {} is not in the table (syscalls.rs)", nr))?;
    let given: Vec<Val> = args[1..].to_vec();
    // `arch_prctl(ARCH_SET_FS, p)` is no system call here at all.
    if let syscalls::A64::SetThreadPointer = form {
        match fr.consts.get(&given[0]) {
            Some(v) if *v as i64 == syscalls::ARCH_SET_FS => {}
            _ => {
                return Err(
                    "aarch64: only arch_prctl(ARCH_SET_FS, p) can be translated (it becomes 'msr tpidr_el0')"
                        .to_string(),
                )
            }
        }
        load_full(e, fr, A, given[1]);
        e.line(&format!("msr tpidr_el0, {}", A));
        if let Some(d) = i.dst {
            e.line(&format!("mov {}, xzr", A));
            store_dst(e, fr, d, A);
        }
        return Ok(());
    }
    let (number, at_fdcwd) = match form {
        syscalls::A64::Direct(n) => (n, false),
        syscalls::A64::AtFdcwd(n) => (n, true),
        syscalls::A64::ForkClone(n) => {
            // fork() -> clone(SIGCHLD, 0, 0, 0, 0). The arguments the
            // library writes are the padding zeroes; they have to BE zero.
            for (k, a) in given.iter().enumerate() {
                if !matches!(fr.consts.get(a), Some(0)) {
                    return Err(format!(
                        "aarch64: fork(2) becomes clone(SIGCHLD); argument {} is not the padding zero",
                        k + 1
                    ));
                }
            }
            imm_into(e, "x0", syscalls::SIGCHLD);
            for r in SYS_REGS.iter().skip(1) {
                e.line(&format!("mov {}, xzr", r));
            }
            imm_into(e, "x8", n as i64);
            e.line("svc #0");
            if let Some(d) = i.dst {
                store_dst(e, fr, d, "x0");
            }
            return Ok(());
        }
        syscalls::A64::Dup3(n) => {
            // dup2(old, new) -> dup3(old, new, 0)
            for (k, a) in given.iter().enumerate().skip(2) {
                if !matches!(fr.consts.get(a), Some(0)) {
                    return Err(format!(
                        "aarch64: dup2(2) becomes dup3; argument {} is not the padding zero",
                        k + 1
                    ));
                }
            }
            load_full(e, fr, "x0", given[0]);
            load_full(e, fr, "x1", given[1]);
            e.line("mov x2, xzr");
            imm_into(e, "x8", n as i64);
            e.line("svc #0");
            if let Some(d) = i.dst {
                store_dst(e, fr, d, "x0");
            }
            return Ok(());
        }
        syscalls::A64::SetThreadPointer => unreachable!(),
        syscalls::A64::Missing(why) => {
            return Err(format!("aarch64: system call {} — {}", nr, why))
        }
    };
    if at_fdcwd {
        // openat(AT_FDCWD, path, flags, mode): one register more than the
        // call had. Arguments past the fifth have to be the padding zeroes
        // the library writes, otherwise something would fall off the end.
        for (k, a) in given.iter().enumerate().skip(5) {
            match fr.consts.get(a) {
                Some(0) => {}
                _ => {
                    return Err(format!(
                        "aarch64: system call {} becomes openat and needs a register more; argument {} is not the padding zero",
                        nr,
                        k + 1
                    ))
                }
            }
        }
        for (k, a) in given.iter().enumerate().take(5) {
            load_full(e, fr, SYS_REGS[k + 1], *a);
        }
        imm_into(e, "x0", syscalls::AT_FDCWD);
    } else {
        for (k, a) in given.iter().enumerate() {
            load_full(e, fr, SYS_REGS[k], *a);
        }
    }
    imm_into(e, "x8", number as i64);
    e.line("svc #0");
    if let Some(d) = i.dst {
        store_dst(e, fr, d, "x0");
    }
    Ok(())
}

fn emit_bin(
    e: &mut Emitter,
    fr: &Frame,
    op: BinOp,
    ty: FTy,
    a: Val,
    b: Val,
    d: Val,
) -> Result<(), String> {
    let bits = if ty.bits() > 32 { 64 } else { 32 };
    if ty.is_float() {
        let m = match (op, ty) {
            (BinOp::Add, _) => "fadd",
            (BinOp::Sub, _) => "fsub",
            (BinOp::Mul, _) => "fmul",
            (BinOp::Div, _) => "fdiv",
            _ => {
                return Err(format!(
                    "internal error: operator '{:?}' is not defined for {}",
                    op,
                    ty.name()
                ))
            }
        };
        let single = ty == FTy::F32;
        load_fp(e, fr, "d0", a, single);
        load_fp(e, fr, "d1", b, single);
        if single {
            e.line(&format!("{} s0, s0, s1", m));
        } else {
            e.line(&format!("{} d0, d0, d1", m));
        }
        store_fp(e, fr, d, "d0", single);
        return Ok(());
    }
    match op {
        BinOp::Add | BinOp::Sub | BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Mul => {
            // For these the low order bits do not depend on the width, which
            // is why the computing happens at 32/64 bits and the result gets
            // cut to the type width when it is read again.
            load_full(e, fr, A, a);
            load_full(e, fr, B, b);
            let m = match op {
                BinOp::Add => "add",
                BinOp::Sub => "sub",
                BinOp::And => "and",
                BinOp::Or => "orr",
                BinOp::Xor => "eor",
                _ => "mul",
            };
            e.line(&format!(
                "{} {}, {}, {}",
                m,
                rw(A, bits),
                rw(A, bits),
                rw(B, bits)
            ));
            store_dst(e, fr, d, A);
        }
        BinOp::Div | BinOp::Rem => {
            // The operands are brought exactly to the computing width (the
            // upper bits of a slot are not guaranteed), then divided to match
            // the sign. A64 has no remainder instruction: `msub` computes
            // a - (a/b)*b out of the quotient, which is the same value.
            load_ext(e, fr, A, a, ty, bits);
            load_ext(e, fr, B, b, ty, bits);
            let m = if ty.signed() { "sdiv" } else { "udiv" };
            e.line(&format!(
                "{} {}, {}, {}",
                m,
                rw(C, bits),
                rw(A, bits),
                rw(B, bits)
            ));
            if op == BinOp::Div {
                store_dst(e, fr, d, C);
            } else {
                e.line(&format!(
                    "msub {}, {}, {}, {}",
                    rw(A, bits),
                    rw(C, bits),
                    rw(B, bits),
                    rw(A, bits)
                ));
                store_dst(e, fr, d, A);
            }
        }
        BinOp::Shl | BinOp::Shr => {
            // Widen the left operand exactly, so that a right shift of an
            // 8/16-bit type pulls the right bits along. The shift count is
            // taken modulo the width by the hardware — on both machines.
            load_ext(e, fr, A, a, ty, bits);
            load_full(e, fr, B, b);
            let m = match (op, ty.signed()) {
                (BinOp::Shl, _) => "lsl",
                (_, true) => "asr",
                (_, false) => "lsr",
            };
            e.line(&format!(
                "{} {}, {}, {}",
                m,
                rw(A, bits),
                rw(A, bits),
                rw(B, bits)
            ));
            store_dst(e, fr, d, A);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{Func, Module, Op, Term};

    fn build(m: &Module) -> String {
        crate::target::flag_set("aarch64-linux").unwrap();
        let s = emit(m).expect("codegen");
        crate::target::reset();
        s
    }

    fn simple_module() -> Module {
        let mut f = Func::new("main", vec![], FTy::I32);
        let c = f.push(0, FTy::I32, Op::Const(42));
        f.set_term(0, Term::Ret(Some(c)));
        Module { funcs: vec![f] }
    }

    #[test]
    fn start_block_prologue_and_exit() {
        let s = build(&simple_module());
        assert!(s.contains("_start:"), "{}", s);
        assert!(s.contains("stp x29, x30, [sp, #-16]!"), "{}", s);
        assert!(s.contains("movz x9, #42"), "{}", s);
        // exit(2) is 93 here and 60 on x86 — that is the whole point of
        // syscalls.rs.
        assert!(s.contains("mov x8, #93"), "{}", s);
        assert!(s.contains("svc #0"), "{}", s);
    }

    #[test]
    fn frame_is_16_aligned_and_sp_never_moves_again() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let p = f.alloca(12, 4);
        let c = f.push(0, FTy::I32, Op::Const(1));
        f.push_void(0, FTy::I32, Op::Store { addr: p, val: c });
        f.set_term(0, Term::Ret(Some(c)));
        let fr = layout(&f);
        assert_eq!(fr.size % 16, 0);
        assert!(fr.size >= 12);
        let s = build(&Module { funcs: vec![f] });
        // exactly one `sub sp` (the prologue) and one `add sp` (the epilogue)
        assert_eq!(s.matches("sub sp, sp,").count(), 1, "{}", s);
        assert_eq!(s.matches("add sp, sp,").count(), 1, "{}", s);
    }

    /// More than eight arguments: from the ninth word on they travel on the
    /// stack — and they are written into the area that the prologue reserved,
    /// not pushed (AAPCS64 §6.4).
    #[test]
    fn stack_args_from_the_ninth_word() {
        let mut m = Module::new();
        let mut f = Func::new("f", vec![FTy::I64; 10], FTy::I64);
        let p9 = f.param_val(9);
        f.set_term(0, Term::Ret(Some(p9)));
        m.funcs.push(f);
        let mut g = Func::new("main", vec![], FTy::I32);
        let mut args = Vec::new();
        for k in 0..10 {
            args.push(g.push(0, FTy::I64, Op::Const(k as i128)));
        }
        let r = g.push(0, FTy::I64, Op::Call { name: "f".to_string(), args });
        let rc = g.push(0, FTy::I32, Op::Cast { src: r, from: FTy::I64 });
        g.set_term(0, Term::Ret(Some(rc)));
        m.funcs.push(g);
        let asm = build(&m);
        // the callee reads them above the saved pair
        assert!(asm.contains("[x29, #16]"), "{}", asm);
        assert!(asm.contains("[x29, #24]"), "{}", asm);
        // the caller writes them at the bottom of its own frame
        assert!(asm.contains("str x9, [sp]"), "{}", asm);
        assert!(asm.contains("str x9, [sp, #8]"), "{}", asm);
    }

    #[test]
    fn large_constants_come_out_as_movz_movk() {
        // ROUND 83: `Emitter` is the x86 file's struct and grew an xmm value
    // cache in round 82. This backend never touches it -- an empty cache is
    // the honest initial value, not a special case.
    let mut e = Emitter::default();
        imm_into(&mut e, "x9", 0x1234_5678_9abc_def0u64 as i64);
        assert!(e.out.contains("movz x9, #57072"), "{}", e.out);
        assert_eq!(e.out.matches("movk").count(), 3, "{}", e.out);
        // -1 is the case in which NO chunk differs from 0xffff -- one
        // instruction, and it was four before the case was noticed.
        let mut e2 = Emitter::default();
        imm_into(&mut e2, "x9", -1);
        assert_eq!(e2.out.trim(), "movn x9, #0", "{}", e2.out);
        let mut e3 = Emitter::default();
        imm_into(&mut e3, "x9", -65537);
        assert_eq!(e3.out.trim(), "movn x9, #1, lsl #16", "{}", e3.out);
    }

    #[test]
    fn a_syscall_number_gets_translated_and_not_passed_through() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let nr = f.push(0, FTy::I64, Op::Const(1)); // write on x86-64
        let fd = f.push(0, FTy::I64, Op::Const(1));
        let buf = f.alloca(8, 8);
        let n = f.push(0, FTy::I64, Op::Const(1));
        f.push_void(0, FTy::I64, Op::Syscall { args: vec![nr, fd, buf, n] });
        let z = f.push(0, FTy::I32, Op::Const(0));
        f.set_term(0, Term::Ret(Some(z)));
        let s = build(&Module { funcs: vec![f] });
        assert!(s.contains("mov x8, #64") || s.contains("movz x8, #64"), "{}", s);
    }
}
