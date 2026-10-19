// SPDX-License-Identifier: MPL-2.0
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
use crate::regalloc_a64::{self, RaA64};
use crate::syscalls;
use std::collections::HashMap;

/// `--shared` (aarch64 .so): `tpidr_el0` belongs to the HOST (glibc TLS), so the Firn runtime cannot keep its
/// per-thread block pointer there. Instead the library keeps two words:
///   * `__firn_tp_base` -- the first thread block ever registered (`arch_prctl(ARCH_SET_FS, p)`): entry 0 of the
///     thread table = the main thread's block. The table is one mapping of 512-octet entries (`gc.fi` FT_BYTES).
///   * `__firn_tp_hi`   -- number of entries that may be in use (highest registered index + 1).
/// `Op::ThreadSelf` is then "which entry's stack range holds `sp`": entries 1.. carry the low end of their stack
/// (`FT_LO`, offset 272) and its end (`FT_BOTTOM`, offset 40); a stack in none of them is the main thread's.
const SHARED_TP_BASE: &str = "__firn_tp_base";
const SHARED_TP_HI: &str = "__firn_tp_hi";
/// Thread block layout facts this scheme relies on (`lib/gc/gc.fi`; a unit test in `thread.rs` checks them).
pub(crate) const TCB_BYTES_LOG2: u32 = 9;
pub(crate) const TCB_LO: u32 = 272;
pub(crate) const TCB_BOTTOM: u32 = 40;

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

/// `.so` (see SHARED_TP_BASE): leaves in `A` the thread block whose stack range holds `sp`, else the main thread's
/// block (entry 0), else 0 while nothing is registered. Clobbers `A`, `B`, `C`, `T1`, `ADDR`.
fn emit_host_thread_block(e: &mut Emitter) {
        let lp = uniq(e, "tp_loop");
        let nx = uniq(e, "tp_next");
        let dn = uniq(e, "tp_done");
        e.line(&format!("adrp {}, {}", ADDR, SHARED_TP_BASE));
        e.line(&format!("ldr {}, [{}, :lo12:{}]", A, ADDR, SHARED_TP_BASE));
        e.line(&format!("cbz {}, {}", A, dn));
        e.line(&format!("adrp {}, {}", ADDR, SHARED_TP_HI));
        e.line(&format!("ldr {}, [{}, :lo12:{}]", B, ADDR, SHARED_TP_HI));
        e.line(&format!("mov {}, sp", C));
        e.line(&format!("add {}, {}, {}, lsl #{}", B, A, B, TCB_BYTES_LOG2));
        e.line(&format!("add {}, {}, #{}", T1, A, 1u32 << TCB_BYTES_LOG2));
        e.raw(&format!("{}:", lp));
        e.line(&format!("cmp {}, {}", T1, B));
        e.line(&format!("b.hs {}", dn));
        e.line(&format!("ldr {}, [{}, #{}]", ADDR, T1, TCB_LO));
        e.line(&format!("cbz {}, {}", ADDR, nx));
        e.line(&format!("cmp {}, {}", C, ADDR));
        e.line(&format!("b.lo {}", nx));
        e.line(&format!("ldr {}, [{}, #{}]", ADDR, T1, TCB_BOTTOM));
        e.line(&format!("cmp {}, {}", C, ADDR));
        e.line(&format!("b.hs {}", nx));
        e.line(&format!("mov {}, {}", A, T1));
        e.line(&format!("b {}", dn));
        e.raw(&format!("{}:", nx));
        e.line(&format!("add {}, {}, #{}", T1, T1, 1u32 << TCB_BYTES_LOG2));
        e.line(&format!("b {}", lp));
        e.raw(&format!("{}:", dn));
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
    /// ROUND BILLIG: the register allocation. Empty = the old path, every
    /// value in its slot.
    pub(crate) ra: RaA64,
    /// ROUND BILLIG: offset from `sp` of the area in which the prologue
    /// rescues the callee-saved registers it hands out. It sits directly
    /// above the outgoing argument area and below the value slots.
    saved_at: u64,
}

/// Marker in `Frame::slot`: this value has NO slot, it lives in a register
/// or is rebuilt from an immediate (ROUND BILLIG, compact frames).
const NO_SLOT: u64 = u64::MAX;

impl Frame {
    /// Distance of a slot from `sp` (that is what an `ldr` gets to see).
    fn off(&self, v: Val) -> u64 {
        let s = self.slot[v as usize];
        // A register resident value asked for its slot: a code path forgot to
        // ask `fr.ra` first. Fail loudly -- reading the slot would hand out
        // whatever an earlier call left there.
        assert!(
            s != NO_SLOT,
            "internal error: value %{} lives in a register and has no frame slot",
            v
        );
        self.size - s
    }
    fn alloca(&self, v: Val) -> Option<u64> {
        self.alloca_off[v as usize].map(|o| self.size - o)
    }
}

fn layout(f: &Func) -> Frame {
    let n = f.val_types.len();
    // ROUND BILLIG, COMPACT FRAMES. The allocation comes FIRST, so that a
    // value that lives in a register (or is rebuilt from an immediate) does
    // not get a slot at all.
    //
    // WHY: the collector scans the stack conservatively. A frame slot that
    // nothing ever writes keeps whatever an earlier, deeper call left in that
    // stack area -- and the frames of `gc_collect` and `__gc_collect_now` are
    // live while the scan runs. With one slot per value, a frame held a
    // handful of live words and dozens of slots nobody writes (a value in a
    // register never touches its slot). A frame of only the values that
    // really sit in memory has far fewer of those holes, and it is a lot
    // smaller: `gc_collect` 384 -> 176 octets, `__gc_collect_now`
    // 1472 -> 768 (docs/ROUND-A64-REGALLOC.md, third finding).
    let ra = regalloc_a64::allocate(f);
    let mut slot = vec![NO_SLOT; n];
    let mut cursor = 0u64;
    for (idx, s) in slot.iter_mut().enumerate() {
        if ra.reg(idx as Val).is_some() || ra.imm(idx as Val).is_some() || ra.freg(idx as Val).is_some() {
            continue;
        }
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
            let args = match &i.op {
                Op::Call { args, .. } | Op::CallIndirect { args, .. } => args.as_slice(),
                _ => continue,
            };
            let (_, stack) = place_args(f, args);
            outgoing = outgoing.max(crate::codegen_x86::stack_offsets(f, &stack).1);
        }
    }
    outgoing = align_up(outgoing, 16);
    // ROUND BILLIG. The rescue area for the handed out callee-saved
    // registers goes DIRECTLY ABOVE the outgoing arguments: `sp` never moves
    // in this backend, the slots are addressed from the top of the frame
    // (`size - slot`), and the arguments from the bottom. The gap in between
    // was padding until this round.
    let saved_bytes = align_up(8 * (ra.saved.len() + ra.fsaved.len()) as u64, 16);
    Frame {
        slot,
        alloca_off,
        size: align_up(cursor + outgoing + saved_bytes, 16),
        outgoing,
        consts,
        ra,
        saved_at: outgoing,
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
    // ROUND BILLIG: if the value lives in a register, the slot is not read —
    // a `mov` between registers instead of a trip through memory.
    if let Some(c) = fr.ra.imm(v) {
        imm_into(e, r, c);
        return;
    }
    if let Some(rr) = fr.ra.reg(v) {
        if rr != r {
            e.line(&format!("mov {}, {}", r, rr));
        }
        return;
    }
    // ROUND FREG: a float in a `d` register, read as a bit pattern. For an
    // `f32` the `w` form zeroes the upper half, like the slot holds it.
    if let Some((dn, single)) = fr.ra.freg(v) {
        fp_bits_out(e, r, dn, single);
        return;
    }
    let m = at(e, fr, v, 8);
    e.line(&format!("ldr {}, {}", r, m));
}

/// ROUND FREG -- the bit pattern of a float register into a general register.
fn fp_bits_out(e: &mut Emitter, r: &str, dn: &str, single: bool) {
    if single {
        e.line(&format!("fmov {}, {}", w(r), sreg(dn)));
    } else {
        e.line(&format!("fmov {}, {}", r, dn));
    }
}

/// ROUND FREG -- a general register into a float register (bit pattern).
fn fp_bits_in(e: &mut Emitter, dn: &str, r: &str, single: bool) {
    if single {
        e.line(&format!("fmov {}, {}", sreg(dn), w(r)));
    } else {
        e.line(&format!("fmov {}, {}", dn, r));
    }
}

/// ROUND FREG -- the name of a float register at the width of its type.
fn fname(d: &str, single: bool) -> String {
    if single {
        sreg(d)
    } else {
        d.to_string()
    }
}

/// Loads a value sign/zero extended to `to_bits` (32 or 64) — the counterpart
/// of `codegen_x86::load_ext`, and it has the same duty: the upper bits of a
/// slot are NOT guaranteed, so an 8/16/32-bit value has to be widened while
/// it is read.
fn load_ext(e: &mut Emitter, fr: &Frame, r: &str, v: Val, ty: FTy, to_bits: u32) {
    let bits = ty.bits().max(8);
    // ROUND BILLIG: the same widening, but out of a register. The upper bits
    // of a register are as little guaranteed as those of a slot — whoever
    // wrote it wrote a full word, and what stood above the type's width is
    // leftovers. So the extension is not saved, only the memory access is.
    if let Some(c) = fr.ra.imm(v) {
        // The very same 64-bit pattern the widening load would have left in
        // the register, worked out here instead of read from a slot.
        imm_into(e, r, widen_const(c, ty, to_bits));
        return;
    }
    if let Some(rr) = fr.ra.reg(v) {
        ext_reg(e, r, rr, ty, to_bits);
        return;
    }
    if let Some((dn, single)) = fr.ra.freg(v) {
        fp_bits_out(e, r, dn, single);
        ext_reg(e, r, r, ty, to_bits);
        return;
    }
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

/// ROUND BILLIG — `dst = src`, widened from `ty` to `to_bits`, both in
/// registers. The counterpart of the `ldrsb`/`ldrh`/`ldrsw` family in
/// `load_ext`, and it has to exist because a register holds exactly as
/// little above the width of its type as a slot does.
fn ext_reg(e: &mut Emitter, dst: &str, src: &str, ty: FTy, to_bits: u32) {
    let bits = ty.bits().max(8);
    if bits >= to_bits {
        if dst != src || to_bits <= 32 {
            // A write to a `w` register zeroes the upper half — that is the
            // point when to_bits is 32, so it is NOT skipped for dst == src.
            e.line(&format!("mov {}, {}", rw(dst, to_bits), rw(src, to_bits)));
        }
        return;
    }
    let d = rw(dst, to_bits);
    match (ty.signed(), bits) {
        (true, 8) => e.line(&format!("sxtb {}, {}", d, w(src))),
        (true, 16) => e.line(&format!("sxth {}, {}", d, w(src))),
        (true, _) => e.line(&format!("sxtw {}, {}", dst, w(src))),
        (false, 8) => e.line(&format!("uxtb {}, {}", w(dst), w(src))),
        (false, 16) => e.line(&format!("uxth {}, {}", w(dst), w(src))),
        (false, _) => e.line(&format!("mov {}, {}", w(dst), w(src))),
    }
}

/// ROUND BILLIG — the ZERO extended read of the lower `bits` of a value,
/// out of its register or out of its slot. Two places wanted exactly this
/// and wrote it out by hand with `at()`: the `bool` of a `br_cond` and the
/// `Cast` to `bool`. Both would have kept reading the slot of a value that
/// no longer lives there.
/// ROUND BILLIG, STUFE 2 -- THE OPERAND ITSELF.
///
/// Stage 1 only replaced `ldr`/`str` with `mov`: the same number of
/// instructions, just without the trip through memory. The gain of round 43
/// on x86 (-26 % executed instructions) does not come from that; it comes
/// from the allocated register being the OPERAND. Four instructions
///
///     mov x9, x22 / movz x10, #1 / add x9, x9, x10 / mov x22, x9
///
/// are one: `add x22, x22, x10`. These three little functions are what turns
/// the one into the other, and they are used only where the whole operation
/// is a SINGLE instruction that reads all its sources before it writes its
/// target -- then `d == a` is harmless and needs no ordering rule.
fn src(e: &mut Emitter, fr: &Frame, v: Val, scratch: &'static str) -> &'static str {
    if let Some(r) = fr.ra.reg(v) {
        return r;
    }
    load_full(e, fr, scratch, v);
    scratch
}

/// The same, but the value has to arrive widened to `bits`. A register whose
/// type is already at least that wide IS the widened value; anything narrower
/// has to go through the scratch register, because `sxtb`/`uxth` write a
/// target and we must not write into an allocated register that still holds
/// something else.
fn src_ext(
    e: &mut Emitter,
    fr: &Frame,
    v: Val,
    ty: FTy,
    bits: u32,
    scratch: &'static str,
) -> &'static str {
    if ty.bits().max(8) >= bits {
        if let Some(r) = fr.ra.reg(v) {
            return r;
        }
    }
    load_ext(e, fr, scratch, v, ty, bits);
    scratch
}

/// The register the result is computed in: the allocated one if there is
/// one, otherwise the scratch register the caller names.
/// ROUND BILLIG -- a rebuilt constant that A64 takes as an IMMEDIATE in the
/// `add`/`sub`/`cmp` family: unsigned, twelve bits. Then not even the
/// `movz` is left. Deliberately NOT for `and`/`orr`/`eor`: their immediate
/// field is the bitmask encoding `N:immr:imms`, which can express 0xff but
/// not 3, and getting that wrong produces a program that runs and computes
/// something else.
fn imm12(fr: &Frame, v: Val) -> Option<u64> {
    let c = fr.ra.imm(v)?;
    if (0..=4095).contains(&c) {
        Some(c as u64)
    } else {
        None
    }
}

/// r842: a known constant that the BITMASK immediate of `and`/`orr`/`eor` can express at the computing width
/// (`bits` = 32 or 64). `src/imm12` above explain why this needs the encoding check and not a range check: 0xff and
/// 0xffffffff are encodable, 3 and 0x5 are not. The pattern is what the register form would have held in its low
/// `bits` bits, so the result of the instruction is bit for bit the one of the register form.
fn logical_const(fr: &Frame, v: Val, bits: u32) -> Option<u64> {
    let c = if let Some(c) = fr.ra.imm(v) { c as i128 } else { *fr.consts.get(&v)? };
    let pat = if bits >= 64 { c as u64 } else { (c as u64) & 0xffff_ffff };
    crate::a64enc::logical_imm(pat, bits >= 64)?;
    Some(pat)
}

fn dreg(fr: &Frame, d: Val, scratch: &'static str) -> &'static str {
    fr.ra.reg(d).unwrap_or(scratch)
}

/// Write back -- and it does nothing when the target already IS a register.
fn commit(e: &mut Emitter, fr: &Frame, d: Val, r: &str) {
    if fr.ra.reg(d).is_none() {
        store_dst(e, fr, d, r);
    }
}

/// ROUND BILLIG -- what a widening load leaves in the register, as a number.
/// `to_bits == 32` means the upper half is zero (a write to a `w` register
/// zeroes it), so the answer is always a full 64-bit pattern.
fn widen_const(c: i64, ty: FTy, to_bits: u32) -> i64 {
    let bits = ty.bits().max(8).min(64);
    let low = if bits >= 64 { c as u64 } else { (c as u64) & ((1u64 << bits) - 1) };
    let v: u64 = if bits >= to_bits {
        low
    } else if ty.signed() {
        let sh = 64 - bits;
        (((low << sh) as i64) >> sh) as u64
    } else {
        low
    };
    if to_bits <= 32 {
        (v & 0xffff_ffff) as i64
    } else {
        v as i64
    }
}

fn zx_between(e: &mut Emitter, dst: &str, src: &str, bits: u32) {
    match bits {
        8 => e.line(&format!("uxtb {}, {}", w(dst), w(src))),
        16 => e.line(&format!("uxth {}, {}", w(dst), w(src))),
        32 => e.line(&format!("mov {}, {}", w(dst), w(src))),
        _ => {
            if dst != src {
                e.line(&format!("mov {}, {}", dst, src));
            }
        }
    }
}

/// ROUND BILLIG -- `dst = zero_extend(src[bits-1:0])`, both registers.
fn load_zx(e: &mut Emitter, fr: &Frame, r: &str, v: Val, bits: u32) {
    if let Some(c) = fr.ra.imm(v) {
        let m: u64 = if bits >= 64 { !0 } else { (1u64 << bits) - 1 };
        imm_into(e, r, ((c as u64) & m) as i64);
        return;
    }
    if let Some((dn, single)) = fr.ra.freg(v) {
        fp_bits_out(e, r, dn, single);
        zx_between(e, r, r, bits);
        return;
    }
    if let Some(rr) = fr.ra.reg(v) {
        match bits {
            8 => e.line(&format!("uxtb {}, {}", w(r), w(rr))),
            16 => e.line(&format!("uxth {}, {}", w(r), w(rr))),
            32 => e.line(&format!("mov {}, {}", w(r), w(rr))),
            _ => {
                if r != rr {
                    e.line(&format!("mov {}, {}", r, rr));
                }
            }
        }
        return;
    }
    match bits {
        8 => {
            let m = at(e, fr, v, 1);
            e.line(&format!("ldrb {}, {}", w(r), m));
        }
        16 => {
            let m = at(e, fr, v, 2);
            e.line(&format!("ldrh {}, {}", w(r), m));
        }
        32 => {
            let m = at(e, fr, v, 4);
            e.line(&format!("ldr {}, {}", w(r), m));
        }
        _ => load_full(e, fr, r, v),
    }
}

/// Writes a register (full 64 bits) into the slot of the target value.
pub(crate) fn store_dst(e: &mut Emitter, fr: &Frame, d: Val, r: &str) {
    // ROUND BILLIG: the target lives in a register — then the frame does not
    // hold a second, stale copy of it. There is nothing that reads the slot
    // of such a value: `at()` is reached only through `load_full`,
    // `load_ext`, `load_fp` and `store_fp`, and all four ask `fr.ra` first.
    if let Some(rr) = fr.ra.reg(d) {
        if rr != r {
            e.line(&format!("mov {}, {}", rr, r));
        }
        return;
    }
    if let Some((dn, single)) = fr.ra.freg(d) {
        fp_bits_in(e, dn, r, single);
        return;
    }
    let m = at(e, fr, d, 8);
    e.line(&format!("str {}, {}", r, m));
}

/// A floating point value out of its slot into `d0`/`d1` (or `s0`/`s1`).
fn load_fp(e: &mut Emitter, fr: &Frame, x: &str, v: Val, single: bool) {
    // r826: a constant `fmov` can build is rebuilt at the use (no slot, no load)
    if let Some(c) = fr.ra.imm(v) {
        let bits = if single { c as u64 & 0xffff_ffff } else { c as u64 };
        if let Some(t) = crate::regalloc_a64::fp_imm_text(bits, single) {
            if t == "0" {
                e.line(&format!("fmov {}, {}", fname(x, single), if single { "wzr" } else { "xzr" }));
            } else {
                e.line(&format!("fmov {}, #{}", fname(x, single), t));
            }
            return;
        }
    }
    // ROUND FREG: already in a `d` register -- one `fmov`, or none.
    if let Some((dn, _)) = fr.ra.freg(v) {
        if dn != x {
            e.line(&format!("fmov {}, {}", fname(x, single), fname(dn, single)));
        }
        return;
    }
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
    if let Some((dn, _)) = fr.ra.freg(d) {
        if dn != x {
            e.line(&format!("fmov {}, {}", fname(dn, single), fname(x, single)));
        }
        return;
    }
    if single {
        e.line(&format!("fmov {}, {}", w(A), sreg(x)));
        store_dst(e, fr, d, A);
    } else {
        let m = at(e, fr, d, 8);
        e.line(&format!("str {}, {}", x, m));
    }
}

/// ROUND FREG -- the float operand AS a register: its own `d` register if it
/// has one, else it is loaded into `scratch` (`d0`/`d1`). Returns the register.
fn fsrc(e: &mut Emitter, fr: &Frame, v: Val, scratch: &'static str, single: bool) -> &'static str {
    if let Some((dn, _)) = fr.ra.freg(v) {
        return dn;
    }
    load_fp(e, fr, scratch, v, single);
    scratch
}

/// ROUND FREG -- the register a float result is computed in.
fn fdst(fr: &Frame, d: Val, scratch: &'static str) -> &'static str {
    fr.ra.freg(d).map(|x| x.0).unwrap_or(scratch)
}

/// ROUND FREG -- write back a float result; nothing to do if it was computed
/// in its own register.
fn fcommit(e: &mut Emitter, fr: &Frame, d: Val, r: &str, single: bool) {
    if fr.ra.freg(d).is_none() {
        store_fp(e, fr, d, r, single);
    }
}

/// ROUND FREG -- `d = s` for floats, by the shortest road.
fn fcopy(e: &mut Emitter, fr: &Frame, d: Val, s: Val, single: bool) {
    if let Some((dn, _)) = fr.ra.freg(d) {
        load_fp(e, fr, dn, s, single);
    } else if let Some((sn, _)) = fr.ra.freg(s) {
        store_fp(e, fr, d, sn, single);
    } else {
        load_full(e, fr, A, s);
        store_dst(e, fr, d, A);
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
    // r570: a `v128` on the stack takes 16 octets at a 16 octet boundary (AAPCS64 §6.4, rule C.14/C.15).
    let (offs, _) = crate::codegen_x86::stack_offsets(f, &stack);
    for (k, a) in stack.iter().enumerate() {
        if f.val_ty(*a) == FTy::V128 {
            // v16 is no argument register (v0-v7) and nothing is cached across the call.
            crate::simd_a64::vload(e, fr, "v16", *a);
            let m = at_base(e, "sp", offs[k], 16);
            e.line(&format!("str q16, {}", m));
            continue;
        }
        load_full(e, fr, A, *a);
        let m = at_base(e, "sp", offs[k], 8);
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
    if crate::prof::is_kernel() {
        return Err(format!(
            "--target={} does not support the kernel profile yet (round 80)",
            crate::target::Target::Aarch64.name()
        ));
    }
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
    // `--shared` without a `fn main`: no `_start` (nothing would call `main`).
    let no_start = crate::target::shared() && !m.funcs.iter().any(|f| f.name == "main");
    let auxv = crate::simd_a64::needs_auxv(m);
    if !no_start {
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

    if !no_start && !m.funcs.iter().any(|f| f.name == "main") {
        return Err("no entry point: 'fn main() -> i32' is missing".to_string());
    }
    // r845: which functions can never reach a safepoint (the register allocation may keep values in x25-x28 across
    // a call of them).
    regalloc_a64::compute_silent(m);
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
    if crate::statics::any_data() {
        e.raw(&crate::statics::data_asm());
    }
    if auxv {
        e.raw(&crate::simd_a64::auxv_data_asm());
    }
    if crate::target::host_tp() {
        e.raw(&format!(
            ".bss\n.balign 8\n{}:\n    .zero 8\n{}:\n    .zero 8",
            SHARED_TP_BASE, SHARED_TP_HI
        ));
    }
    if crate::target::shared() {
        e.raw(".text");
        e.raw(&crate::so_thunk::runtime_asm()?);
    }
    e.raw(".section .note.GNU-stack,\"\",%progbits");
    Ok(cold_checks(&spill_peephole(&e.out)))
}

fn emit_func(e: &mut Emitter, f: &Func) -> Result<(), String> {
    if f.interrupt {
        return Err(format!(
            "#[interrupt] ('{}') is not supported on aarch64 (round 80)",
            f.name
        ));
    }
    let fr = layout(f);
    e.raw("");
    e.raw(&format!(".globl {}", label(&f.name)));
    e.raw(&format!("{}:", label(&f.name)));
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
    // ROUND BILLIG: rescue the callee-saved registers that the allocation
    // hands out. It happens AFTER `sub sp` (the area is addressed from `sp`,
    // and `sp` never moves again) and BEFORE the parameters are placed —
    // a parameter may itself land in one of these registers.
    if !fr.ra.saved.is_empty() {
        e.raw("    // ROUND BILLIG: the registers the allocation hands out");
        for (k, r) in fr.ra.saved.iter().enumerate() {
            let m = at_base(e, "sp", fr.saved_at + 8 * k as u64, 8);
            e.line(&format!("str {}, {}", r, m));
        }
    }
    // ROUND FREG: the low halves of the callee-saved float registers in use,
    // behind the integer ones.
    for (k, r) in fr.ra.fsaved.iter().enumerate() {
        let m = at_base(e, "sp", fr.saved_at + 8 * (fr.ra.saved.len() + k) as u64, 8);
        e.line(&format!("str {}, {}", r, m));
    }
    // The parameters into their slots. `place_args` is asked with the
    // parameter VALUES, so the caller's rule and the callee's rule are
    // literally the same function.
    let pvals: Vec<Val> = (0..f.params.len() as Val).collect();
    let (spot, stack_vals) = place_args(f, &pvals);
    let (stack_offs, _) = crate::codegen_x86::stack_offsets(f, &stack_vals);
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
                let off = 16 + stack_offs[stack_i];
                stack_i += 1;
                if f.params[i] == FTy::V128 {
                    let m = at_base(e, "x29", off, 16);
                    e.line(&format!("ldr q16, {}", m));
                    crate::simd_a64::vstore(e, &fr, i as Val, "v16");
                    continue;
                }
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
    let fusable = fusable_compares(f);
    for (k, b) in f.blocks.iter().enumerate() {
        e.raw(&format!("{}:", block_label(&f.name, b.id)));
        let next = f.blocks.get(k + 1).map(|n| n.id);
        emit_block(e, f, &fr, b, next, fusable[b.id as usize], &mut site)?;
    }
    Ok(())
}

/// For every block: does it end in `cmp` + `brcond` on exactly that compare, with the compare result read
/// nowhere else? Then the flags feed the branch directly (`cmp` + `b.cc`) and the boolean is never built.
fn fusable_compares(f: &Func) -> Vec<bool> {
    let mut uses = vec![0u32; f.val_types.len()];
    let mut buf: Vec<Val> = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            buf.clear();
            i.op.uses(&mut buf);
            for u in &buf {
                if let Some(c) = uses.get_mut(*u as usize) {
                    *c += 1;
                }
            }
        }
        let tv = match &b.term {
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            Term::Ret(Some(v)) => Some(*v),
            _ => None,
        };
        if let Some(v) = tv {
            if let Some(c) = uses.get_mut(v as usize) {
                *c += 1;
            }
        }
    }
    let mut out = vec![false; f.blocks.len().max(1) + 1];
    for b in &f.blocks {
        if let (Some(last), Term::BrCond { cond, .. }) = (b.insts.last(), &b.term) {
            if let (Op::Cmp { ty, .. }, Some(d)) = (&last.op, last.dst) {
                // a vector compare does not exist here, but stay on the safe side
                if d == *cond
                    && uses.get(d as usize) == Some(&1)
                    && *ty != FTy::V128
                    && !f.is_secret(d)
                    && (b.id as usize) < out.len()
                {
                    out[b.id as usize] = true;
                }
            }
        }
    }
    out
}

/// The condition code that is the exact logical negation of `cc` (valid for floats with NaN as well: it
/// negates the FLAG predicate, not the arithmetic relation).
fn inv_cc(cc: &str) -> &'static str {
    match cc {
        "eq" => "ne",
        "ne" => "eq",
        "hs" => "lo",
        "lo" => "hs",
        "mi" => "pl",
        "pl" => "mi",
        "vs" => "vc",
        "vc" => "vs",
        "hi" => "ls",
        "ls" => "hi",
        "ge" => "lt",
        "lt" => "ge",
        "gt" => "le",
        _ => "gt", // "le"
    }
}

/// A conditional branch on `cc` with the two targets of a `brcond`; the target that is the next block in
/// the output is not branched to.
fn cond_branch(e: &mut Emitter, f: &Func, cc: &str, then_bb: u32, else_bb: u32, next: Option<u32>) {
    if Some(else_bb) == next {
        e.line(&format!("b.{} {}", cc, block_label(&f.name, then_bb)));
    } else if Some(then_bb) == next {
        e.line(&format!("b.{} {}", inv_cc(cc), block_label(&f.name, else_bb)));
    } else {
        e.line(&format!("b.{} {}", cc, block_label(&f.name, then_bb)));
        e.line(&format!("b {}", block_label(&f.name, else_bb)));
    }
}

fn emit_epilogue(e: &mut Emitter, fr: &Frame) {
    // Round BILLIG: back out of the frame before `sp` moves. The return
    // value is already in x0/d0/v0, and `at_base` only ever uses x12.
    if !fr.ra.saved.is_empty() {
        for (k, r) in fr.ra.saved.iter().enumerate() {
            let m = at_base(e, "sp", fr.saved_at + 8 * k as u64, 8);
            e.line(&format!("ldr {}, {}", r, m));
        }
    }
    for (k, r) in fr.ra.fsaved.iter().enumerate() {
        let m = at_base(e, "sp", fr.saved_at + 8 * (fr.ra.saved.len() + k) as u64, 8);
        e.line(&format!("ldr {}, {}", r, m));
    }
    if fr.size > 0 {
        if fr.size <= 4095 {
            e.line(&format!("add sp, sp, #{}", fr.size));
        } else {
            imm_into(e, "x16", fr.size as i64);
            e.line("add sp, sp, x16");
        }
    }
    e.line("ldp x29, x30, [sp], #16");
    e.line("ret");
}

fn emit_block(
    e: &mut Emitter,
    f: &Func,
    fr: &Frame,
    b: &Block,
    next: Option<u32>,
    fused: bool,
    site: &mut crate::panic_rt::SiteCounter,
) -> Result<(), String> {
    let bi = b.id as usize;
    let mut fused_cc: Option<&'static str> = None;
    for (ii, i) in b.insts.iter().enumerate() {
        if fused && ii + 1 == b.insts.len() {
            // the compare feeds the branch of this block: flags only, no boolean
            if let Op::Cmp { op, ty: oty, a, b: rb } = &i.op {
                fused_cc = Some(emit_cmp_flags(e, fr, *op, *oty, *a, *rb));
            }
        } else {
            emit_inst(e, f, fr, i, site)?;
        }
        // Round BILLIG: a POINTER died here. The collector's scan is
        // conservative; a dead pointer in a callee-saved register keeps its
        // whole tree alive -- and the prologue of every called function also
        // carries it into that function's frame. A `mov rN, xzr` at the place
        // of death costs one instruction, and only where one really dies.
        for r in fr.ra.clear_after(bi, ii) {
            e.line(&format!("mov {}, xzr", r));
        }
    }
    match &b.term {
        Term::Br(t) => {
            if Some(*t) != next {
                e.line(&format!("b {}", block_label(&f.name, *t)))
            }
        }
        Term::Switch { .. } => emit_switch(e, f, fr, &b.term)?,
        Term::BrCond { then_bb, else_bb, .. } if fused_cc.is_some() => {
            cond_branch(e, f, fused_cc.unwrap_or("ne"), *then_bb, *else_bb, next);
        }
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
            load_zx(e, fr, A, *cond, 8);
            if Some(*then_bb) == next {
                e.line(&format!("cbz {}, {}", w(A), block_label(&f.name, *else_bb)));
            } else {
                e.line(&format!("cbnz {}, {}", w(A), block_label(&f.name, *then_bb)));
                if Some(*else_bb) != next {
                    e.line(&format!("b {}", block_label(&f.name, *else_bb)));
                }
            }
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
            emit_epilogue(e, fr);
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
    e.raw(crate::target::reloc_rodata());
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

/// Emits the comparison (flags only) and returns the condition code that holds when `a op b` is true.
fn emit_cmp_flags(e: &mut Emitter, fr: &Frame, op: CmpOp, oty: FTy, a: Val, b: Val) -> &'static str {
    if oty.is_float() {
        // `fcmp` sets N/Z/C/V so that the UNORDERED case (NaN) is
        // C=1, V=1, N=0, Z=0. With that, `mi`, `gt`, `ls`, `ge` and
        // `eq` are all false and `ne` is true -- exactly what
        // IEEE-754 demands, without a single extra instruction.
        let single = oty == FTy::F32;
        let ra = fsrc(e, fr, a, "d0", single);
        let rb = fsrc(e, fr, b, "d1", single);
        e.line(&format!("fcmp {}, {}", fname(ra, single), fname(rb, single)));
        return match op {
            CmpOp::Eq => "eq",
            CmpOp::Ne => "ne",
            CmpOp::Lt => "mi",
            CmpOp::Le => "ls",
            CmpOp::Gt => "gt",
            CmpOp::Ge => "ge",
        };
    }
    let bits = if oty.bits() > 32 { 64 } else { 32 };
    let ra = src_ext(e, fr, a, oty, bits, A);
    // `cmp_imm` already knows every form A64 has for a comparison
    // against a number, including the negative one over `cmn`.
    match fr.ra.imm(b) {
        Some(c) if oty.bits().max(8) >= bits => cmp_imm(e, ra, bits, c),
        _ => {
            let rb = src_ext(e, fr, b, oty, bits, B);
            e.line(&format!("cmp {}, {}", rw(ra, bits), rw(rb, bits)));
        }
    }
    match (op, oty.signed()) {
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
    }
}

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
            // ROUND BILLIG: a rebuilt constant has neither slot nor register.
            if fr.ra.imm(d).is_some() {
                return Ok(());
            }
            // ... and if it HAS a register, the constant goes straight into
            // it instead of through x9.
            let t = fr.ra.reg(d).unwrap_or(A);
            imm_into(e, t, ty.truncate(*c) as i64);
            if t == A {
                store_dst(e, fr, d, A);
            }
        }
        Op::Bin(op, a, b) => {
            let d = i.dst.ok_or("internal error: binary operation without target")?;
            emit_bin(e, fr, *op, ty, *a, *b, d)?;
        }
        Op::Cmp { op, ty: oty, a, b } => {
            let d = i.dst.ok_or("internal error: comparison without target")?;
            let cc = emit_cmp_flags(e, fr, *op, *oty, *a, *b);
            let rd = dreg(fr, d, A);
            e.line(&format!("cset {}, {}", w(rd), cc));
            commit(e, fr, d, rd);
        }
        Op::Un(op, a) => {
            let d = i.dst.ok_or("internal error: unary operation without target")?;
            // Round GAPS (fbits.rs): a copy of the pattern, like on x86-64.
            if matches!(op, UnOp::Bits) {
                load_full(e, fr, A, *a);
                if ty.bits() <= 32 {
                    e.line(&format!("mov {}, {}", w(A), w(A)));
                }
                store_dst(e, fr, d, A);
                return Ok(());
            }
            if ty.is_float() {
                if matches!(op, UnOp::Not) {
                    return Err(format!("internal error: '!' is not defined for {}", ty.name()));
                }
                let single = ty == FTy::F32;
                let rs = fsrc(e, fr, *a, "d0", single);
                let rd = fdst(fr, d, "d0");
                // Round GAPS: `fsqrt` next to `fneg` (fsqrt.rs).
                let m = if matches!(op, UnOp::Sqrt) { "fsqrt" } else { "fneg" };
                e.line(&format!("{} {}, {}", m, fname(rd, single), fname(rs, single)));
                fcommit(e, fr, d, rd, single);
                return Ok(());
            }
            if matches!(op, UnOp::Sqrt) {
                return Err(format!("internal error: sqrt is not defined for {}", ty.name()));
            }
            let bits = if ty.bits() > 32 { 64 } else { 32 };
            load_full(e, fr, A, *a);
            match op {
                UnOp::Neg => e.line(&format!("neg {}, {}", rw(A, bits), rw(A, bits))),
                UnOp::Sqrt | UnOp::Bits => unreachable!(),
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
                    fcopy(e, fr, d, *src, ty == FTy::F32);
                    return Ok(());
                }
                let rs = fsrc(e, fr, *src, "d0", *from == FTy::F32);
                let rd = fdst(fr, d, "d0");
                e.line(&format!("fcvt {}, {}", fname(rd, ty == FTy::F32), fname(rs, *from == FTy::F32)));
                fcommit(e, fr, d, rd, ty == FTy::F32);
                return Ok(());
            }
            if ty.is_float() {
                // Integer -> floating point. The value is widened to 64 bits
                // FIRST and then converted as a signed one — literally what
                // `cvtsi2sd` does on x86, including its reservation about
                // unsigned values above 2^63 (SPEC §14.1.f64).
                load_ext(e, fr, A, *src, *from, 64);
                let rd = fdst(fr, d, "d0");
                e.line(&format!("scvtf {}, {}", fname(rd, ty == FTy::F32), A));
                fcommit(e, fr, d, rd, ty == FTy::F32);
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
                let rs = fsrc(e, fr, *src, "d0", *from == FTy::F32);
                let rd = dreg(fr, d, A);
                e.line(&format!("fcvtzs {}, {}", rd, fname(rs, *from == FTy::F32)));
                commit(e, fr, d, rd);
                return Ok(());
            }
            if ty == FTy::Bool {
                // Safety net: bool holds 0/1 only. Only the lower `from` bits
                // decide, so they are read zero extended and tested.
                let bits = from.bits().max(8);
                let to = if bits > 32 { 64 } else { 32 };
                load_zx(e, fr, A, *src, bits);
                e.line(&format!("cmp {}, #0", rw(A, to)));
                e.line(&format!("cset {}, ne", w(A)));
            } else {
                load_ext(e, fr, A, *src, *from, 64);
            }
            store_dst(e, fr, d, A);
        }
        Op::GcAddr { regs } => {
            let d = i.dst.ok_or("internal error: gc_state without target")?;
            emit_gc_addr_dead(e, *regs, fr.ra.dead_at(d));
            store_dst(e, fr, d, A);
        }
        Op::Alloca { .. } => {
            let d = i.dst.ok_or("internal error: alloca without target")?;
            // ROUND BILLIG: a promoted cell has no storage and no address --
            // `promotable_cells` has proven that the address never leaves the
            // direct operand of a `load`/`store`, and both of those are
            // intercepted above. So there is nothing to compute here.
            if fr.ra.cell(d).is_some() {
                return Ok(());
            }
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
            let bits = ty.bits().max(8);
            // ROUND BILLIG -- THE PROMOTED CELL. An `alloca` of at most eight
            // octets whose address never leaves the direct operand of a
            // `load`/`store` needs no memory at all: it IS a register. That
            // is what replaces the phi nodes FIR does not have, and it is
            // what brings the counter of a `while` loop into a register
            // (`regalloc.rs`, step 2 -- the same rule, the same analysis).
            if let Some((cr, _ct)) = fr.ra.cell(*addr) {
                // `ldrb`/`ldrh`/`ldr w` widen with ZEROES. Out of the
                // register it has to be the same widening, or a stored -1 as
                // u8 would come back as -1 instead of 255.
                if let Some(dr) = fr.ra.reg(d) {
                    zx_between(e, dr, cr, bits);
                } else {
                    zx_between(e, A, cr, bits);
                    store_dst(e, fr, d, A);
                }
                return Ok(());
            }
            // ROUND FREG: a float with a `d` register is loaded straight into it.
            if ty.is_float() {
                if let Some((dn, _)) = fr.ra.freg(d) {
                    let rb = src(e, fr, *addr, B);
                    e.line(&format!("ldr {}, [{}]", fname(dn, ty == FTy::F32), rb));
                    return Ok(());
                }
            }
            let rb = src(e, fr, *addr, B);
            let rd = dreg(fr, d, A);
            match bits {
                8 => e.line(&format!("ldrb {}, [{}]", w(rd), rb)),
                16 => e.line(&format!("ldrh {}, [{}]", w(rd), rb)),
                32 => e.line(&format!("ldr {}, [{}]", w(rd), rb)),
                _ => e.line(&format!("ldr {}, [{}]", rd, rb)),
            }
            commit(e, fr, d, rd);
        }
        Op::Store { addr, val } | Op::MmioStore { addr, val } => {
            if ty == FTy::V128 {
                crate::simd_a64::emit_ptr_store(e, fr, *addr, *val);
                return Ok(());
            }
            // ROUND BILLIG: into the cell register instead of into memory.
            // The FULL word goes in; the reading side widens out of the low
            // bits, exactly as `ldrb` after `strb` does.
            if let Some((cr, _ct)) = fr.ra.cell(*addr) {
                load_full(e, fr, cr, *val);
                return Ok(());
            }
            if ty.is_float() {
                if let Some((vn, _)) = fr.ra.freg(*val) {
                    let rb = src(e, fr, *addr, B);
                    e.line(&format!("str {}, [{}]", fname(vn, ty == FTy::F32), rb));
                    return Ok(());
                }
            }
            let rb = src(e, fr, *addr, B);
            let rv = src(e, fr, *val, A);
            let bits = ty.bits().max(8);
            match bits {
                8 => e.line(&format!("strb {}, [{}]", w(rv), rb)),
                16 => e.line(&format!("strh {}, [{}]", w(rv), rb)),
                32 => e.line(&format!("str {}, [{}]", w(rv), rb)),
                _ => e.line(&format!("str {}, [{}]", rv, rb)),
            }
        }
        Op::PtrAdd { base, off } => {
            let d = i.dst.ok_or("internal error: ptradd without target")?;
            let rb = src(e, fr, *base, A);
            let ro = src(e, fr, *off, B);
            let rd = dreg(fr, d, A);
            e.line(&format!("add {}, {}, {}", rd, rb, ro));
            commit(e, fr, d, rd);
        }
        Op::Call { name, args } => {
            let (spot, _stack) = place_args(f, args);
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
            // A `v128` is sixteen octets: since round TEMPO 4 mem2reg
            // promotes it, and the copy phi.rs makes of it must not leave
            // the upper half behind (tests/1613_crypto, 1614_simd_ops).
            if ty == FTy::V128 {
                crate::simd_a64::emit_copy_v128(e, fr, d, *src);
                return Ok(());
            }
            // Round BILLIG: ONE `mov` when the target has a register --
            // `phi.rs` turns every phi into copies, so this is the single
            // most frequent instruction in a loop.
            if ty.is_float() {
                fcopy(e, fr, d, *src, ty == FTy::F32);
                return Ok(());
            }
            if let Some(dr) = fr.ra.reg(d) {
                load_full(e, fr, dr, *src);
                return Ok(());
            }
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
            if crate::target::host_tp() {
                // `.so`: `tpidr_el0` belongs to the HOST (glibc TLS). Find the calling thread's block by its
                // stack range (see SHARED_TP_BASE). Result: the entry whose [FT_LO, FT_BOTTOM) holds `sp`,
                // else the main thread's block (entry 0), else 0 while nothing is registered.
                emit_host_thread_block(e);
            } else {
                e.line(&format!("mrs {}, tpidr_el0", A));
            }
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
        Op::Asm { .. } => {
            return Err(
                "inline assembler is x86 text and has no meaning on aarch64 (round 80)".to_string(),
            )
        }
        // ROUND 83 -- the checked arithmetic of round 72 on this machine
        // (docs/ROUND80.md section 7, panic_rt_a64.rs). Both operands are
        // read extended to a full 64 bits, exactly as the x86 path reads
        // them; the check itself computes at the type's own width.
        Op::CheckedBin { op, a, b, msg } => {
            let d = i.dst.ok_or("internal error: checked binary operation without target")?;
            // r850: 64-bit add/sub with everything in registers: `adds rd, ra, rb|#imm`, nothing else on the hot path
            if ty.bits() == 64 && matches!(op, crate::fir::BinOp::Add | crate::fir::BinOp::Sub) && std::env::var_os("FIRN_A64_NO_DIRECT").is_none() {
                if let (Some(rd), Some(ra)) = (fr.ra.reg(d), fr.ra.reg(*a)) {
                    let rhs = if let Some(c) = fr.ra.imm(*b) {
                        let c = widen_const(c, ty, 64);
                        if (0..=4095).contains(&c) {
                            Some(crate::panic_rt_a64::Rhs::Imm(c as u64))
                        } else {
                            None
                        }
                    } else {
                        fr.ra.reg(*b).map(crate::panic_rt_a64::Rhs::Reg)
                    };
                    if let Some(rhs) = rhs {
                        if crate::panic_rt_a64::emit_checked_addsub_direct(e, *op, ty, msg, site, rd, ra, rhs) {
                            return Ok(());
                        }
                    }
                }
            }
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
        Op::CheckedIdxV { idx, len, msg } => {
            let d = i.dst.ok_or("internal error: checked index without target")?;
            load_ext(e, fr, A, *idx, ty, 64);
            load_ext(e, fr, B, *len, FTy::U64, 64);
            crate::panic_rt_a64::emit_checked_idx_v(e, msg, site);
            store_dst(e, fr, d, A);
        }
        Op::CheckedCast { src, from, msg } => {
            let d = i.dst.ok_or("internal error: checked cast without target")?;
            load_ext(e, fr, A, *src, *from, 64);
            // r454: a cast TO a 64-bit type loses nothing (the check compares the value with itself): no code, no panic arm.
            if ty.bits() < 64 {
                crate::panic_rt_a64::emit_checked_cast(e, *from, ty, msg, site);
            }
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
///
/// Round BILLIG: `dead` names the registers that hold nothing alive at this
/// position. `xzr` goes into the block for them instead of the register
/// content -- otherwise a dead pointer in a callee-saved register keeps its
/// whole tree alive (see `regalloc_a64::safepoints`).
fn emit_gc_addr_dead(e: &mut Emitter, regs: bool, dead: &[&'static str]) {
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
        let q = if dead.contains(r) { "xzr" } else { *r };
        e.line(&format!("str {}, {}", q, m));
    }
}

/// The old name, for every caller that has no assignment.
fn emit_gc_addr(e: &mut Emitter, regs: bool) {
    emit_gc_addr_dead(e, regs, &[]);
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
        // `arch_prctl(ARCH_GET_FS, &out)`: `out = tpidr_el0` (the HOST's thread pointer in a `.so`; `thread_attach`
        // uses it to find the end of a host thread's stack frames, see lib/gc/gc.fi).
        if let Some(v) = fr.consts.get(&given[0]) {
            if *v as i64 == syscalls::ARCH_GET_FS {
                load_full(e, fr, A, given[1]);
                e.line(&format!("mrs {}, tpidr_el0", B));
                e.line(&format!("str {}, [{}]", B, A));
                if let Some(d) = i.dst {
                    e.line(&format!("mov {}, xzr", A));
                    store_dst(e, fr, d, A);
                }
                return Ok(());
            }
        }
        if let Some(v) = fr.consts.get(&given[0]) {
            if *v as i64 == syscalls::ARCH_GET_TCB {
                // r571: the same lookup `Op::ThreadSelf` does. In a `.so` the answer can be entry 0 for a thread that
                // has no block (the stack is in no entry's range); the runtime tells those apart by the thread id in
                // the block (`__thread_known` in lib/gc/gc.fi), exactly as it does behind the x86-64 `gs` base.
                load_full(e, fr, T2, given[1]);
                if crate::target::host_tp() {
                    emit_host_thread_block(e);
                } else {
                    e.line(&format!("mrs {}, tpidr_el0", A));
                }
                e.line(&format!("str {}, [{}]", A, T2));
                if let Some(d) = i.dst {
                    e.line(&format!("mov {}, xzr", A));
                    store_dst(e, fr, d, A);
                }
                return Ok(());
            }
        }
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
        if crate::target::host_tp() {
            // Register the thread block `p` (see SHARED_TP_BASE): the first one becomes the base, then
            // `__firn_tp_hi = max(hi, (p - base) / 512 + 1)`. p = 0 (a thread leaving) changes nothing.
            let dn = uniq(e, "setfs_done");
            let have = uniq(e, "setfs_have");
            let retry = uniq(e, "setfs_retry");
            let skip = uniq(e, "setfs_skip");
            e.line(&format!("cbz {}, {}", A, dn));
            e.line(&format!("adrp {}, {}", ADDR, SHARED_TP_BASE));
            e.line(&format!("add {}, {}, :lo12:{}", ADDR, ADDR, SHARED_TP_BASE));
            e.line(&format!("ldr {}, [{}]", B, ADDR));
            e.line(&format!("cbnz {}, {}", B, have));
            e.line(&format!("str {}, [{}]", A, ADDR));
            e.line(&format!("mov {}, {}", B, A));
            e.raw(&format!("{}:", have));
            e.line(&format!("sub {}, {}, {}", B, A, B));
            e.line(&format!("lsr {}, {}, #{}", B, B, TCB_BYTES_LOG2));
            e.line(&format!("add {}, {}, #1", B, B));
            e.line(&format!("adrp {}, {}", ADDR, SHARED_TP_HI));
            e.line(&format!("add {}, {}, :lo12:{}", ADDR, ADDR, SHARED_TP_HI));
            e.raw(&format!("{}:", retry));
            e.line(&format!("ldaxr {}, [{}]", C, ADDR));
            e.line(&format!("cmp {}, {}", C, B));
            e.line(&format!("b.hs {}", skip));
            e.line(&format!("stlxr {}, {}, [{}]", w(T2), B, ADDR));
            e.line(&format!("cbnz {}, {}", w(T2), retry));
            e.line(&format!("b {}", dn));
            e.raw(&format!("{}:", skip));
            e.line("clrex");
            e.raw(&format!("{}:", dn));
        } else {
            e.line(&format!("msr tpidr_el0, {}", A));
        }
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
        syscalls::A64::PpollMs(n) => {
            // ROUND C-059 (Certus): poll(fds, nfds, ms)
            //                    -> ppoll(fds, nfds, timespec*, NULL, 0)
            //
            // The timeout changes SHAPE. `poll` counts milliseconds in a
            // register, `ppoll` wants the address of a
            // `timespec { i64 sec; i64 nsec; }`. The value is therefore
            // taken apart here and written into 16 bytes of stack.
            //
            // Two cases the library really writes:
            //   * a constant -1 — "wait forever". `ppoll` says that with a
            //     NULL pointer, not with a timespec.
            //   * everything else — sec = ms / 1000, nsec = (ms % 1000) * 1e6.
            //
            // Arguments past the third have to be the padding zeroes.
            for (k, a) in given.iter().enumerate().skip(3) {
                if !matches!(fr.consts.get(a), Some(0)) {
                    return Err(format!(
                        "aarch64: poll(2) becomes ppoll; argument {} is not the padding zero",
                        k + 1
                    ));
                }
            }
            load_full(e, fr, "x0", given[0]);
            load_full(e, fr, "x1", given[1]);
            match fr.consts.get(&given[2]) {
                // "wait forever" -> NULL, no timespec at all.
                Some(v) if *v as i64 == -1 => {
                    e.line("mov x2, xzr");
                }
                _ => {
                    // ms -> timespec on the stack. x9/x10/x11 are scratch
                    // registers here; the frame is not touched.
                    load_full(e, fr, "x9", given[2]);
                    e.line("sub sp, sp, #16");
                    // sec = ms / 1000, nsec = (ms - sec * 1000) * 1000000
                    imm_into(e, "x10", 1000);
                    e.line("sdiv x11, x9, x10");          // x11 = sec
                    e.line("msub x9, x11, x10, x9");      // x9  = ms % 1000
                    imm_into(e, "x10", 1_000_000);
                    e.line("mul x9, x9, x10");            // x9  = nsec
                    e.line("stp x11, x9, [sp]");
                    e.line("mov x2, sp");
                }
            }
            e.line("mov x3, xzr");                        // sigmask = NULL
            e.line("mov x4, xzr");                        // sigsetsize = 0
            imm_into(e, "x8", n as i64);
            e.line("svc #0");
            // The stack is only given back where it was taken.
            if !matches!(fr.consts.get(&given[2]), Some(v) if *v as i64 == -1) {
                e.line("add sp, sp, #16");
            }
            if let Some(d) = i.dst {
                store_dst(e, fr, d, "x0");
            }
            return Ok(());
        }
        syscalls::A64::AlarmItimer(n) => {
            // alarm(sec) -> setitimer(ITIMER_REAL, &itimerval, NULL) with
            // itimerval = { interval {0, 0}, value {sec, 0} } (32 bytes on the
            // stack). Arguments past the first must be the padding zeroes.
            for (k, a) in given.iter().enumerate().skip(1) {
                if !matches!(fr.consts.get(a), Some(0)) {
                    return Err(format!(
                        "aarch64: alarm(2) becomes setitimer; argument {} is not the padding zero",
                        k + 1
                    ));
                }
            }
            load_full(e, fr, "x9", given[0]);
            e.line("sub sp, sp, #32");
            e.line("stp xzr, xzr, [sp]");
            e.line("stp x9, xzr, [sp, #16]");
            e.line("mov x0, xzr");                        // ITIMER_REAL
            e.line("mov x1, sp");
            e.line("mov x2, xzr");                        // old value = NULL
            imm_into(e, "x8", n as i64);
            e.line("svc #0");
            e.line("add sp, sp, #32");
            if let Some(d) = i.dst {
                store_dst(e, fr, d, "x0");
            }
            return Ok(());
        }
        syscalls::A64::RenameAt(n) => {
            // FIRN r64: renameat(AT_FDCWD, old, AT_FDCWD, new).
            load_full(e, fr, "x1", given[0]);
            load_full(e, fr, "x3", given[1]);
            imm_into(e, "x0", syscalls::AT_FDCWD);
            imm_into(e, "x2", syscalls::AT_FDCWD);
            imm_into(e, "x8", n as i64);
            e.line("svc #0");
            if let Some(d) = i.dst {
                store_dst(e, fr, d, "x0");
            }
            return Ok(());
        }
        syscalls::A64::Rewrite(n, form) => {
            // FIRN r75: stat/lstat/pipe/rmdir/readlink -> the *at forms.
            // `A(j)` = the j-th argument the program wrote, `I(v)` = immediate.
            // Arguments the form does not use must be the padding zeroes.
            let used = form
                .iter()
                .filter_map(|x| if let syscalls::X::A(j) = x { Some(*j) } else { None })
                .max()
                .map_or(0, |j| j + 1);
            for (k, a) in given.iter().enumerate().skip(used) {
                if !matches!(fr.consts.get(a), Some(0)) {
                    return Err(format!(
                        "aarch64: system call {} is rewritten to the *at form; argument {} is not the padding zero",
                        nr,
                        k + 1
                    ));
                }
            }
            if given.len() < used {
                return Err(format!("aarch64: system call {} needs {} arguments", nr, used));
            }
            // Load the program's arguments first (they may be spilled
            // anywhere), then the immediates; targets are fresh x0..x5 only.
            for (k, x) in form.iter().enumerate() {
                if let syscalls::X::A(j) = x {
                    load_full(e, fr, SYS_REGS[k], given[*j]);
                }
            }
            for (k, x) in form.iter().enumerate() {
                if let syscalls::X::I(c) = x {
                    imm_into(e, SYS_REGS[k], *c);
                }
            }
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
        let ra = fsrc(e, fr, a, "d0", single);
        let rb = fsrc(e, fr, b, "d1", single);
        let rd = fdst(fr, d, "d0");
        e.line(&format!("{} {}, {}, {}", m, fname(rd, single), fname(ra, single), fname(rb, single)));
        fcommit(e, fr, d, rd, single);
        return Ok(());
    }
    match op {
        BinOp::Add | BinOp::Sub | BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Mul => {
            // For these the low order bits do not depend on the width, which
            // is why the computing happens at 32/64 bits and the result gets
            // cut to the type width when it is read again.
            if matches!(op, BinOp::Add | BinOp::Sub) {
                if let Some(c) = imm12(fr, b) {
                    let ra = src(e, fr, a, A);
                    let rd = dreg(fr, d, A);
                    let m = if op == BinOp::Add { "add" } else { "sub" };
                    e.line(&format!("{} {}, {}, #{}", m, rw(rd, bits), rw(ra, bits), c));
                    commit(e, fr, d, rd);
                    return Ok(());
                }
            }
            if matches!(op, BinOp::And | BinOp::Or | BinOp::Xor) {
                // r842: the operation commutes, so the constant may sit on either side.
                let hit = logical_const(fr, b, bits)
                    .map(|c| (a, c))
                    .or_else(|| logical_const(fr, a, bits).map(|c| (b, c)));
                if let Some((x, c)) = hit {
                    let rx = src(e, fr, x, A);
                    let rd = dreg(fr, d, A);
                    let m = match op {
                        BinOp::And => "and",
                        BinOp::Or => "orr",
                        _ => "eor",
                    };
                    e.line(&format!("{} {}, {}, #{}", m, rw(rd, bits), rw(rx, bits), c));
                    commit(e, fr, d, rd);
                    return Ok(());
                }
            }
            if op == BinOp::Mul {
                // Multiplying by a power of two is a shift (the low bits are the same).
                let pow2 = |v: Val| match fr.ra.imm(v) {
                    Some(c) if c > 1 && (c as u64).is_power_of_two() => Some((c as u64).trailing_zeros()),
                    _ => None,
                };
                let hit = pow2(b).map(|k| (a, k)).or_else(|| pow2(a).map(|k| (b, k)));
                if let Some((x, k)) = hit {
                    let ra = src(e, fr, x, A);
                    let rd = dreg(fr, d, A);
                    e.line(&format!("lsl {}, {}, #{}", rw(rd, bits), rw(ra, bits), k));
                    commit(e, fr, d, rd);
                    return Ok(());
                }
            }
            let ra = src(e, fr, a, A);
            let rb = src(e, fr, b, B);
            let rd = dreg(fr, d, A);
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
                rw(rd, bits),
                rw(ra, bits),
                rw(rb, bits)
            ));
            commit(e, fr, d, rd);
        }
        BinOp::Div | BinOp::Rem => {
            // The operands are brought exactly to the computing width (the
            // upper bits of a slot are not guaranteed), then divided to match
            // the sign. A64 has no remainder instruction: `msub` computes
            // a - (a/b)*b out of the quotient, which is the same value.
            let ra = src_ext(e, fr, a, ty, bits, A);
            let rb = src_ext(e, fr, b, ty, bits, B);
            let m = if ty.signed() { "sdiv" } else { "udiv" };
            if op == BinOp::Div {
                let rd = dreg(fr, d, C);
                e.line(&format!("{} {}, {}, {}", m, rw(rd, bits), rw(ra, bits), rw(rb, bits)));
                commit(e, fr, d, rd);
            } else {
                // The quotient goes into C, and C is never an allocated
                // register -- so `msub` may write its target even when that
                // target is one of the two operands.
                e.line(&format!("{} {}, {}, {}", m, rw(C, bits), rw(ra, bits), rw(rb, bits)));
                let rd = dreg(fr, d, A);
                e.line(&format!(
                    "msub {}, {}, {}, {}",
                    rw(rd, bits),
                    rw(C, bits),
                    rw(rb, bits),
                    rw(ra, bits)
                ));
                commit(e, fr, d, rd);
            }
        }
        BinOp::Rotr32 => {
            // r837: rotate the low 32 bits right; a write to a `w` register
            // zero-extends, which is exactly the contract of `Rotr32`.
            let ra = src(e, fr, a, A);
            let rd = dreg(fr, d, A);
            match fr.ra.imm(b) {
                Some(k) if (1..=31).contains(&k) => {
                    e.line(&format!("ror {}, {}, #{}", w(rd), w(ra), k));
                }
                _ => {
                    let rb = src(e, fr, b, B);
                    e.line(&format!("ror {}, {}, {}", w(rd), w(ra), w(rb)));
                }
            }
            commit(e, fr, d, rd);
        }
        BinOp::Shl | BinOp::Shr => {
            // Widen the left operand exactly, so that a right shift of an
            // 8/16-bit type pulls the right bits along. The shift count is
            // taken modulo the width by the hardware — on both machines.
            let ra = src_ext(e, fr, a, ty, bits, A);
            let m = match (op, ty.signed()) {
                (BinOp::Shl, _) => "lsl",
                (_, true) => "asr",
                (_, false) => "lsr",
            };
            // r842: a constant count is the immediate form (one instruction less). The hardware takes the count
            // modulo the width in the register form; the immediate form keeps exactly that by reducing it here.
            let kc = fr.ra.imm(b).map(|c| c as i128).or_else(|| fr.consts.get(&b).copied());
            if let Some(k) = kc {
                let rd = dreg(fr, d, A);
                let k = (k as u64) & (bits as u64 - 1);
                e.line(&format!("{} {}, {}, #{}", m, rw(rd, bits), rw(ra, bits), k));
                commit(e, fr, d, rd);
                return Ok(());
            }
            let rb = src(e, fr, b, B);
            let rd = dreg(fr, d, A);
            e.line(&format!(
                "{} {}, {}, {}",
                m,
                rw(rd, bits),
                rw(ra, bits),
                rw(rb, bits)
            ));
            commit(e, fr, d, rd);
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
        // ROUND BILLIG: until this round the constant went `movz x9, #42`
        // / `str x9, [sp]` / `ldr x0, [sp]` -- three instructions and two
        // memory accesses to hand a literal to `ret`. A constant that
        // `imm_into` writes in ONE instruction now has neither a slot nor a
        // register (`regalloc_a64::cheap_const`); it is rebuilt where it is
        // used, and the use here is the return register.
        assert!(s.contains("movz x0, #42"), "{}", s);
        assert!(!s.contains("str x9, [sp]"), "the constant still goes through the frame:\n{}", s);
        // exit(2) is 93 here and 60 on x86 — that is the whole point of
        // syscalls.rs.
        assert!(s.contains("mov x8, #93"), "{}", s);
        assert!(s.contains("svc #0"), "{}", s);
    }

    /// ROUND BILLIG, compact frames: a value that lives in a register or is
    /// rebuilt from an immediate has no slot. Thirty-three values here, all of
    /// them in registers (the six that the allocator hands out) or immediates:
    /// the frame holds the saved registers and little else. Before the change it
    /// was one 8-octet slot per value.
    #[test]
    fn values_in_registers_have_no_slot() {
        let mut f = Func::new("main", vec![], FTy::I32);
        let mut acc = f.push(0, FTy::I32, Op::Const(1));
        for k in 0..16 {
            let c = f.push(0, FTy::I32, Op::Const(100 + k));
            acc = f.push(0, FTy::I32, Op::Bin(crate::fir::BinOp::Add, acc, c));
        }
        f.set_term(0, Term::Ret(Some(acc)));
        let n = f.val_types.len() as u64;
        let fr = layout(&f);
        assert!(fr.size < 8 * n / 2, "frame of {} octets for {} values", fr.size, n);
        // and the program still builds: no path asks for a slot that is not there
        let s = build(&Module { funcs: vec![f] });
        assert!(s.contains("ret"), "{}", s);
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

/// Parses `str|ldr <reg>, [sp, #<off>]` (no write-back) into (mnemonic, reg, rest-of-operand).
fn sp_slot(line: &str) -> Option<(&str, &str, &str)> {
    let t = line.strip_prefix("    ")?;
    let (mn, rest) = t.split_once(' ')?;
    if mn != "str" && mn != "ldr" {
        return None;
    }
    let (reg, addr) = rest.split_once(", ")?;
    if !addr.starts_with("[sp") || addr.ends_with('!') || !addr.ends_with(']') {
        return None;
    }
    Some((mn, reg, addr))
}

/// Text peephole: `str r,[sp,#o]` directly followed (only `.loc` lines between) by `ldr r,[sp,#o]` drops the
/// load (the value is still in `r`); `ldr r2,[sp,#o]` of the same slot becomes `mov r2, r` for x/w registers.
/// A label or any other instruction between the two keeps the pair untouched.
fn spill_peephole(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut owned: Vec<(usize, String)> = Vec::new();
    // index in `out` of the last str, valid while only .loc lines followed
    let mut last_str: Option<usize> = None;
    for line in text.lines() {
        if line.starts_with("    .loc ") {
            out.push(line);
            continue;
        }
        if let (Some((mn, reg, addr)), Some(si)) = (sp_slot(line), last_str) {
            if mn == "ldr" {
                if let Some((_, sreg, saddr)) = sp_slot(out[si]) {
                    if saddr == addr {
                        if sreg == reg {
                            continue; // value still in the register
                        }
                        let c0 = sreg.as_bytes()[0];
                        if c0 == reg.as_bytes()[0] && (c0 == b'x' || c0 == b'w') {
                            owned.push((out.len(), format!("    mov {}, {}", reg, sreg)));
                            out.push("");
                            last_str = None;
                            continue;
                        }
                    }
                }
            }
        }
        last_str = match sp_slot(line) {
            Some(("str", _, _)) => Some(out.len()),
            _ => None,
        };
        out.push(line);
    }
    let mut r = String::with_capacity(text.len());
    let mut oi = 0;
    for (i, l) in out.iter().enumerate() {
        if oi < owned.len() && owned[oi].0 == i {
            r.push_str(&owned[oi].1);
            oi += 1;
        } else {
            r.push_str(l);
        }
        r.push('\n');
    }
    r
}

/// Moves the panic block of an overflow/range check out of the hot path. The emitted shape is
/// `b.cc .LchksiteN` / `b .LokN` / `.LchksiteN:` <panic call> / `.LokN:`; the unconditional `b .LokN` is dropped and
/// the panic block is parked behind the next unconditional branch (nothing falls through it), so the hot path
/// just falls into `.LokN`.
fn cold_checks(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let n = lines.len();
    let mut skip = vec![false; n];
    // block start -> (end exclusive) for blocks to move; parked[after_idx] = blocks to emit after that line
    let mut pending: Vec<(usize, usize)> = Vec::new();
    let mut parked: std::collections::HashMap<usize, Vec<(usize, usize)>> = std::collections::HashMap::new();
    let mut drop_b = vec![false; n];
    let is_term = |l: &str| {
        let t = l.trim_start();
        t == "ret" || (t.starts_with("b ") && !t.starts_with("b.")) || t.starts_with("br ")
    };
    let mut i = 0;
    while i < n {
        if let Some(prev) = i.checked_sub(1) {
            let l = lines[i];
            if l.starts_with(".Lchksite") && l.ends_with(':') {
                // previous line must be `    b <ok>` and the block must end with the ok label
                if let Some(ok) = lines[prev].strip_prefix("    b ") {
                    let ok_line = format!("{}:", ok);
                    let mut j = i + 1;
                    let mut good = false;
                    while j < n && j < i + 40 {
                        if lines[j] == ok_line {
                            good = true;
                            break;
                        }
                        if lines[j].ends_with(':') && !lines[j].starts_with("    ") {
                            break;
                        }
                        j += 1;
                    }
                    if good && j > i + 1 && is_term(lines[j - 1]) && !lines[i + 1..j].iter().any(|x| x.ends_with(':') && !x.starts_with("    ")) {
                        pending.push((i, j));
                        drop_b[prev] = true;
                        for k in i..j {
                            skip[k] = true;
                        }
                        i = j;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    // park each block after the next terminator following its ok label
    let mut waiting: Vec<(usize, usize)> = Vec::new();
    let mut pk = 0;
    for idx in 0..n {
        while pk < pending.len() && pending[pk].1 <= idx {
            waiting.push(pending[pk]);
            pk += 1;
        }
        if !waiting.is_empty() && !skip[idx] && !drop_b[idx] && is_term(lines[idx]) && idx >= waiting[0].1 {
            parked.entry(idx).or_default().extend(waiting.drain(..));
        }
    }
    // blocks that found no terminator are left in place (restore)
    let mut restore = vec![false; n];
    for (a, b) in &waiting {
        for k in *a..*b {
            skip[k] = false;
        }
        restore[*a - 1] = true;
    }
    let mut r = String::with_capacity(text.len());
    for idx in 0..n {
        if drop_b[idx] && !restore[idx] {
            continue;
        }
        if !skip[idx] {
            r.push_str(lines[idx]);
            r.push('\n');
        }
        if let Some(bs) = parked.get(&idx) {
            for (a, b) in bs {
                for k in *a..*b {
                    r.push_str(lines[k]);
                    r.push('\n');
                }
            }
        }
    }
    r
}
