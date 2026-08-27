// SPDX-License-Identifier: GPL-2.0-only
//! FIR — the intermediate language of our own (SPEC §8.1).
//!
//! Properties (invariants, see docs/FIR.md):
//!  * typed: every instruction has a result type `FTy`
//!  * SSA like: every instruction defines at most ONE new value `%n`,
//!    and every value is defined exactly once
//!  * basic blocks with EXACTLY ONE terminator at the end (`br`, `brcond`, `ret`)
//!  * **ROUND 92** — real phi nodes. The LOWERING still puts every mutable
//!    variable into an `alloca` slot and addresses it with `load`/`store`;
//!    `mem2reg.rs` is what turns those slots into values, and where two
//!    paths bring two different values to the same place it writes an
//!    `Op::Phi`. Phi instructions stand at the START of a block, before
//!    every other instruction, and carry exactly one entry per DISTINCT
//!    predecessor, sorted by block number. Every pass that changes the
//!    control flow graph has to keep that true; `mem2reg::simplify_phis`
//!    is the repair shop, `fir::Func::verify_phis` the assertion.
//!  * `Op::Phi` never comes out of the lowering and never reaches a
//!    backend: `phi.rs` eliminates it into `Op::Copy` right before the code
//!    generator, so `--emit=fir-raw` is phi free (that is what
//!    `tools/fir_compare.sh` compares against the compiler written in Firn)
//!    and every backend stays a straight-line reader of instructions.
//!  * no x86 quirks: registers, stack frames and calling convention come
//!    about in the backend only

use std::fmt::Write as _;

pub type Val = u32;
pub type BlockId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FTy {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    Bool,
    /// IEEE-754 binary64. The value of an `Op::Const` is the BIT PATTERN as
    /// u64 — FIR knows no float literals, only bit patterns.
    F64,
    /// **ROUND 71** — IEEE-754 binary32. The value of an `Op::Const` is the
    /// BIT PATTERN as u32, in the same way.
    F32,
    /// **ROUND 82** — the 128-bit vector value. Sixteen octets without an
    /// element type; it lives in an `xmm` register and has a sixteen byte
    /// aligned home in the frame.
    V128,
    /// pointer (always 64 bits, untyped in FIR)
    Ptr,
    /// no value
    Void,
}

impl FTy {
    pub fn bits(self) -> u32 {
        match self {
            FTy::I8 | FTy::U8 | FTy::Bool => 8,
            FTy::I16 | FTy::U16 => 16,
            FTy::I32 | FTy::U32 | FTy::F32 => 32,
            FTy::I64 | FTy::U64 | FTy::Ptr | FTy::F64 => 64,
            FTy::V128 => 128,
            FTy::Void => 0,
        }
    }
    pub fn bytes(self) -> u64 {
        (self.bits() / 8) as u64
    }
    pub fn signed(self) -> bool {
        matches!(self, FTy::I8 | FTy::I16 | FTy::I32 | FTy::I64)
    }
    /// **ROUND 71** — one of the two floating point types. Everything that
    /// asked `== FTy::F64` before asks this here; that is what keeps the two
    /// widths from drifting apart.
    pub fn is_float(self) -> bool {
        matches!(self, FTy::F32 | FTy::F64)
    }
    pub fn name(self) -> &'static str {
        match self {
            FTy::I8 => "i8",
            FTy::I16 => "i16",
            FTy::I32 => "i32",
            FTy::I64 => "i64",
            FTy::U8 => "u8",
            FTy::U16 => "u16",
            FTy::U32 => "u32",
            FTy::U64 => "u64",
            FTy::Bool => "bool",
            FTy::F64 => "f64",
            FTy::F32 => "f32",
            FTy::V128 => "v128",
            FTy::Ptr => "ptr",
            FTy::Void => "void",
        }
    }
    /// Trim a value to the width/signedness of the type.
    pub fn truncate(self, v: i128) -> i128 {
        let bits = self.bits();
        if bits == 0 || bits >= 128 {
            return v;
        }
        let mask: i128 = (1i128 << bits) - 1;
        let m = v & mask;
        if self == FTy::Bool {
            return if m & 1 != 0 { 1 } else { 0 };
        }
        if self.signed() && (m >> (bits - 1)) & 1 != 0 {
            m - (1i128 << bits)
        } else {
            m
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
}

impl BinOp {
    pub fn name(self) -> &'static str {
        match self {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::Div => "div",
            BinOp::Rem => "rem",
            BinOp::And => "and",
            BinOp::Or => "or",
            BinOp::Xor => "xor",
            BinOp::Shl => "shl",
            BinOp::Shr => "shr",
        }
    }
}

/// **ROUND 72** — the explicit wrap/saturate forms (SPEC §13, `L9`). They
/// exist next to `BinOp` rather than inside it: `BinOp::Add` computes,
/// `WrapOp`/`SatOp` say WHICH DEFINED BEHAVIOUR is wanted when the
/// mathematical result does not fit — a question `BinOp` never had to
/// answer before checked arithmetic made overflow an error in most places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrapSatKind {
    /// `+% -% *%` — silently keep the low order bits (two's complement).
    Wrap,
    /// `+| -| *|` — clamp to the type's own MIN/MAX.
    Sat,
}

impl WrapSatKind {
    pub fn name(self) -> &'static str {
        match self {
            WrapSatKind::Wrap => "wrap",
            WrapSatKind::Sat => "sat",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CmpOp {
    pub fn name(self) -> &'static str {
        match self {
            CmpOp::Eq => "eq",
            CmpOp::Ne => "ne",
            CmpOp::Lt => "lt",
            CmpOp::Le => "le",
            CmpOp::Gt => "gt",
            CmpOp::Ge => "ge",
        }
    }
    /// The reversed comparison — the whole point of the round 72 branch
    /// lowering (`threading.rs`): a jump can test `!(a < b)` by testing
    /// `a >= b` directly, without ever materializing the negation.
    pub fn negate(self) -> CmpOp {
        match self {
            CmpOp::Eq => CmpOp::Ne,
            CmpOp::Ne => CmpOp::Eq,
            CmpOp::Lt => CmpOp::Ge,
            CmpOp::Le => CmpOp::Gt,
            CmpOp::Gt => CmpOp::Le,
            CmpOp::Ge => CmpOp::Lt,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    /// arithmetic negation
    Neg,
    /// bitwise not (integer) or logical not (bool)
    Not,
}

#[derive(Clone, Debug)]
pub enum Op {
    /// constant of the instruction type
    Const(i128),
    Bin(BinOp, Val, Val),
    /// **ROUND 72** — `+% -% *%` / `+| -| *|` (SPEC §13, `L9`). `kind` says
    /// whether the defined behaviour on overflow is wrapping or saturating;
    /// `op` is restricted to `Add`/`Sub`/`Mul` by construction (`lower.rs`).
    /// Never checked, never panics — that is the whole point of writing it
    /// out explicitly instead of relying on the build level.
    BinWrapSat { kind: WrapSatKind, op: BinOp, a: Val, b: Val },
    Cmp { op: CmpOp, ty: FTy, a: Val, b: Val },
    Un(UnOp, Val),
    /// conversion; the target type is the instruction type
    Cast { src: Val, from: FTy },
    /// **ROUND 72** — checked `+ - *` (SPEC §13, `L9`). `op` is restricted to
    /// `Add`/`Sub`/`Mul`. On overflow the program aborts through the panic
    /// path described at `msg`: text ready to print (file, line, operator),
    /// the two operand VALUES are appended by the backend at the point where
    /// it already holds them in registers — FIR carries no source positions
    /// (`dwarf.rs`), so the position is baked into `msg` at lowering time,
    /// once, as plain text.
    CheckedBin { op: BinOp, a: Val, b: Val, msg: String },
    /// **ROUND 72** — checked `/` and `%`. Panics on division by zero and,
    /// for signed types, on the `MIN / -1` (`MIN % -1`) special case that
    /// would otherwise raise `SIGFPE` with no message at all. `op` is
    /// restricted to `Div`/`Rem`. Two separate messages because the two
    /// failures are unrelated questions (`b == 0` vs. `a == MIN && b == -1`)
    /// and a reader should not have to guess which one fired from a single
    /// merged sentence.
    CheckedDiv { op: BinOp, a: Val, b: Val, msg_zero: String, msg_range: String },
    /// **ROUND 72** — checked `as` (narrowing only; `lower.rs` never emits
    /// this for a conversion that cannot lose a value). Panics when `src`,
    /// read back after the conversion, would not equal the original value.
    CheckedCast { src: Val, from: FTy, msg: String },
    /// **ROUND 89** — the checked ARRAY INDEX (SPEC §13, `L9`). Hands the
    /// index back unchanged when `idx < len` and panics otherwise, through
    /// the same message path as the checked arithmetic. `len` is the length
    /// of the array, known at compile time — a `*T` has no length and
    /// therefore never grows one of these.
    ///
    /// The comparison is UNSIGNED, which is why one instead of two is
    /// enough: an index is a `usize` (SPEC §14.1 item 2), so "below zero"
    /// is not a state it can be in.
    CheckedIdx { idx: Val, len: u64, msg: String },
    /// stack storage of the function (allowed in the entry block only)
    Alloca { size: u64, align: u64 },
    Load { addr: Val },
    /// no result value; `ty` of the instruction is the stored type
    Store { addr: Val, val: Val },
    /// pointer + byte offset (the offset is i64/u64)
    PtrAdd { base: Val, off: Val },
    Call { name: String, args: Vec<Val> },
    /// Linux syscall: args[0] = number, up to 6 arguments after it
    Syscall { args: Vec<Val> },
    /// block copy (structs/arrays); no result value
    CopyMem { dst: Val, src: Val, size: u64 },
    /// Data independent choice (SPEC §9.3): `cond ? a : b`, `cmov` at the backend.
    /// NO pass may turn it into a branch (SPEC §9.2).
    Select { cond: Val, a: Val, b: Val },
    /// Opaque barrier (`barrier(inout x)`): hands the value back unchanged,
    /// but counts as impenetrable for every pass.
    Barrier { val: Val },
    /// `secure_zero(inout buf)`: zeroes `size` bytes from `addr`. NEVER counts as dead.
    SecureZero { addr: Val, size: Val },
    /// **Round 47** — atomic read-add-write (`atomic.rs`): `[addr] += val` as
    /// ONE machine instruction (`lock xadd`), the result is the OLD value.
    /// Always 64 bits. Foundation of the counter of `Arc[T]` (SPEC §3.4).
    /// Never pure, never mergeable, never movable across another memory
    /// access.
    AtomicAdd { addr: Val, val: Val },
    /// **Round 49** — atomic compare-and-swap (`thread.rs`): if `[addr]` holds
    /// the value `erw`, then `new` is written there. The result is ALWAYS
    /// the value found; the swap happened when it equals `erw`. One machine
    /// instruction (`lock cmpxchg`). Locks can be built with it — with
    /// `lock xadd` alone that does not work, because the transition
    /// "free -> taken" has to be conditional.
    AtomicCas { addr: Val, erw: Val, new: Val },
    /// **Round 49** — create a thread (`thread.rs`, `clone(2)`). The result is
    /// the thread id (> 0) or a negative error value. The child returns from
    /// the system call with its OWN `rsp`; that is why this is an instruction
    /// sequence and no `syscall` call.
    ThreadSpawn { arg: Val, stack: Val, ctid: Val },
    /// **Round 49** — address of the own thread block (`fs:0`, `thread.rs`).
    /// Without `arch_prctl(ARCH_SET_FS)` the result is useless; the runtime
    /// sets the base before it ever reads the value.
    ThreadSelf,
    /// Call through a POINTER — dynamic dispatch (`iface.rs`, round 46).
    /// `target` is the address of the function, everything else as with `Call`.
    CallIndirect { target: Val, args: Vec<Val> },
    /// Address of a method table (`iface.rs`, round 46). `table` is the key
    /// `<interface>.<type>`; the label sits in `.rodata`.
    VtabAddr { table: String },
    /// **Round 58** — address of a FUNCTION RECORD (`fnval.rs`). `name` is
    /// the function whose record it is; the record holds the code address
    /// in word 0 and sits in `.rodata`. This is how a named function
    /// becomes a value.
    FnRef { name: String },
    /// **ROUND 89** — the ADDRESS of a global variable (`statics.rs`).
    /// `name` is the `static` after module mangling; the place itself sits
    /// in `.bss`, `.data` or `.rodata`. Pure and without operands, exactly
    /// like `FnRef`: the address is a link time constant.
    GlobalAddr { name: String },
    /// Address of the state block of the collector (SPEC §3.5, `gc.rs`).
    /// `regs = true`: rescue the callee-saved registers into the block first —
    /// only that makes the CONSERVATIVE register scan honest (SPEC §3.5.3).
    /// Without a `gc class` in the program this instruction never comes about.
    GcAddr { regs: bool },
    /// **Round 52** — inline assembler (`core.rs`, SPEC §2 `profile kernel`).
    ///
    /// ALWAYS `volatile`: this instruction may **never** be removed, duplicated,
    /// merged or moved across another memory access. Exactly that is the trap
    /// of round 40 — the optimizer removed code that it was not allowed to
    /// remove.
    ///
    /// `template` is the assembler text (Intel syntax, `\n` separates lines).
    /// `in_regs[i]` is the register into which the i-th input value is put
    /// in front of the block; `out` is the register whose content is the
    /// result afterwards (the instruction type is `u64` then, otherwise
    /// `void`). `clobber` names registers destroyed on top of that, or `memory`.
    Asm {
        template: String,
        out: Option<String>,
        in_regs: Vec<String>,
        ins: Vec<Val>,
        /// **ROUND 68** — further output operands. `out("rdx") p` writes the
        /// register into the memory location `p` points at, AFTER the
        /// template has run. Any number of them; the value form `out("rax")`
        /// without an expression stays the single result of the expression.
        out_regs: Vec<String>,
        /// The ADDRESSES belonging to `out_regs`, in the same order.
        outs: Vec<Val>,
        clobber: Vec<String>,
    },
    /// **Round 52** — MMIO read (`core.rs`). Like `Op::Load`, but
    /// **volatile**: no pass may merge two accesses, remove one
    /// or move it. The width sits in the instruction type.
    MmioLoad { addr: Val },
    /// **Round 52** — MMIO write (`core.rs`). Like `Op::Store`, but
    /// **volatile** (see `MmioLoad`).
    MmioStore { addr: Val, val: Val },
    /// **ROUND 92** — the PHI NODE (SPEC §8.1). `incoming` holds one pair
    /// `(predecessor block, value)` per DISTINCT predecessor of the block
    /// this instruction stands in, sorted ascending by block number. The
    /// value of the instruction is the entry belonging to the edge the
    /// control flow actually came in on.
    ///
    /// WHY IT EXISTS. Without it `mem2reg.rs` could only resolve `alloca`s
    /// written EXACTLY ONCE — every loop counter is written again on every
    /// pass and therefore stayed in MEMORY. And a value in memory is
    /// invisible to everything that comes after: no induction variables, no
    /// range analysis across a back edge, no loop invariant motion of
    /// anything that touches the counter. This one instruction is what makes
    /// those possible; it is the whole point of round 92.
    ///
    /// INVARIANTS (see `Func::verify_phis`):
    ///   * phis stand at the beginning of their block, before every other
    ///     instruction
    ///   * exactly one entry per distinct predecessor, none for a block that
    ///     is not a predecessor
    ///   * the entry list is sorted by block number, so that two runs of the
    ///     compiler write the same text
    Phi { incoming: Vec<(BlockId, Val)> },
    /// **ROUND 92** — `dst = src`, nothing else. The ONLY producer is
    /// `phi.rs`: eliminating a phi means putting a copy at the end of every
    /// predecessor, and the copies of one edge happen SIMULTANEOUSLY (all
    /// reads before all writes), which is why `phi.rs` has to sequentialize
    /// them and break cycles over a temporary. Nothing else in the compiler
    /// ever creates one, and no optimizer pass ever sees one.
    Copy { src: Val },
    /// **ROUND 82** — ONE vector or crypto machine instruction (`simd.rs`).
    ///
    /// `kind` says which one; `args` are its operands in source order, `imm`
    /// is the 8-bit immediate of the instructions that have one (0 for all
    /// others). The result type of the instruction is the `ty` of the `Inst`
    /// — `V128` for most, `U64`/`U32` for the extractors and `crc32`,
    /// `Void` for `__v128_store`.
    ///
    /// Why ONE variant instead of forty: every one of these instructions is
    /// the same thing to every pass — pure (or, for load/store, memory
    /// touching), not foldable, not reassociable. A pass that had to
    /// enumerate forty variants would be forty chances to forget one.
    Simd { kind: crate::simd::SimdKind, args: Vec<Val>, imm: u8 },
}

impl Op {
    /// Pure = without side effect, may be removed when the result is
    /// unused.
    pub fn is_pure(&self) -> bool {
        match self {
            Op::Const(_)
            | Op::Bin(..)
            | Op::BinWrapSat { .. }
            | Op::Cmp { .. }
            | Op::Un(..)
            | Op::Cast { .. }
            | Op::PtrAdd { .. }
            | Op::Load { .. }
            | Op::Alloca { .. }
            // ROUND 92: a phi computes nothing and a copy computes nothing;
            // both may fall away the moment their result is unused. Careful
            // in `opt.rs`, though: two phis can hold each OTHER alive in a
            // loop, so the dead code pass counts phi uses separately.
            | Op::Phi { .. }
            | Op::Copy { .. }
            | Op::Select { .. } => true,
            // The address of a table in `.rodata` is a constant.
            Op::VtabAddr { .. } | Op::FnRef { .. } | Op::GlobalAddr { .. } => true,
            // ROUND 82: everything except the two memory instructions is a
            // pure register computation and may be removed when unused; CSE
            // may merge two of them. `__v128_load` reads memory and
            // `__v128_store` writes it — those two never.
            Op::Simd { kind, .. } => kind.is_pure(),
            // The state block is always there; rescuing the registers
            // writes memory, though, and must not fall away.
            Op::GcAddr { regs } => !*regs,
            // ROUND 72: a checked operation may ABORT THE PROGRAM. That is
            // an observable effect (SPEC §13, `L9`) — dead code elimination
            // must not remove it even when the result is unused, exactly as
            // `Syscall`/`Call` are never pure.
            Op::CheckedBin { .. }
            | Op::CheckedDiv { .. }
            | Op::CheckedCast { .. }
            | Op::CheckedIdx { .. } => false,
            Op::Store { .. }
            | Op::Call { .. }
            | Op::CallIndirect { .. }
            | Op::Syscall { .. }
            | Op::CopyMem { .. }
            | Op::Barrier { .. }
            | Op::AtomicAdd { .. }
            // ROUND 52: volatile. Never pure, never removable — not even
            // when the result stays unused.
            | Op::Asm { .. }
            | Op::MmioLoad { .. }
            | Op::MmioStore { .. }
            | Op::AtomicCas { .. }
            | Op::ThreadSpawn { .. }
            | Op::SecureZero { .. } => false,
            // The self pointer changes nothing and only reads the thread
            // base; it must NOT be moved across an `arch_prctl`, though.
            // Pure instructions are only REMOVED (when unused) and hoisted
            // by LICM — LICM takes the list above alone, and `ThreadSelf`
            // does not stand in it.
            Op::ThreadSelf => true,
        }
    }

    /// All values read.
    pub fn uses(&self, out: &mut Vec<Val>) {
        match self {
            Op::Const(_)
            | Op::Alloca { .. }
            | Op::GcAddr { .. }
            | Op::VtabAddr { .. }
            | Op::FnRef { .. }
            | Op::GlobalAddr { .. }
            | Op::ThreadSelf => {}
            Op::CallIndirect { target, args } => {
                out.push(*target);
                out.extend_from_slice(args);
            }
            Op::Bin(_, a, b) => {
                out.push(*a);
                out.push(*b);
            }
            Op::BinWrapSat { a, b, .. } => {
                out.push(*a);
                out.push(*b);
            }
            Op::CheckedBin { a, b, .. } | Op::CheckedDiv { a, b, .. } => {
                out.push(*a);
                out.push(*b);
            }
            Op::CheckedCast { src, .. } => out.push(*src),
            Op::CheckedIdx { idx, .. } => out.push(*idx),
            Op::Cmp { a, b, .. } => {
                out.push(*a);
                out.push(*b);
            }
            Op::Un(_, a) => out.push(*a),
            Op::Cast { src, .. } => out.push(*src),
            Op::Load { addr } => out.push(*addr),
            Op::Store { addr, val } => {
                out.push(*addr);
                out.push(*val);
            }
            Op::PtrAdd { base, off } => {
                out.push(*base);
                out.push(*off);
            }
            Op::Call { args, .. } | Op::Syscall { args } => out.extend_from_slice(args),
            Op::CopyMem { dst, src, .. } => {
                out.push(*dst);
                out.push(*src);
            }
            Op::Select { cond, a, b } => {
                out.push(*cond);
                out.push(*a);
                out.push(*b);
            }
            Op::Barrier { val } => out.push(*val),
            Op::AtomicAdd { addr, val } => {
                out.push(*addr);
                out.push(*val);
            }
            Op::AtomicCas { addr, erw, new } => {
                out.push(*addr);
                out.push(*erw);
                out.push(*new);
            }
            Op::ThreadSpawn { arg, stack, ctid } => {
                out.push(*arg);
                out.push(*stack);
                out.push(*ctid);
            }
            Op::SecureZero { addr, size } => {
                out.push(*addr);
                out.push(*size);
            }
            Op::Asm { ins, outs, .. } => {
                out.extend_from_slice(ins);
                // ROUND 68: an output operand READS an address.
                out.extend_from_slice(outs);
            }
            Op::MmioLoad { addr } => out.push(*addr),
            Op::MmioStore { addr, val } => {
                out.push(*addr);
                out.push(*val);
            }
            Op::Simd { args, .. } => out.extend_from_slice(args),
            // ROUND 92. NOTE FOR EVERY LIVENESS ANALYSIS: a phi operand is
            // read at the END OF ITS PREDECESSOR, not at the top of the
            // block the phi stands in. This function cannot express that —
            // it only lists values. `regalloc.rs` never has to care because
            // `phi.rs` has already turned every phi into copies by then;
            // any future consumer that computes liveness on FIR WITH phis
            // in it must special case them here.
            Op::Phi { incoming } => {
                for (_, v) in incoming.iter() {
                    out.push(*v);
                }
            }
            Op::Copy { src } => out.push(*src),
        }
    }

    /// ROUND 92 — the phi entry belonging to the edge coming from `pred`.
    pub fn phi_value(&self, pred: BlockId) -> Option<Val> {
        match self {
            Op::Phi { incoming } => incoming.iter().find(|(b, _)| *b == pred).map(|(_, v)| *v),
            _ => None,
        }
    }
}

/// **ROUND 94** -- the source position of ONE instruction.
///
/// Until this round FIR carried no positions at all: `dwarf.rs` kept a side
/// table keyed by *(function, block, instruction index)*. That table is right
/// exactly as long as nobody moves an instruction -- and moving instructions
/// is what an optimizer does. After inlining, the copied body of the callee
/// sat in the caller's blocks and the side table knew nothing about it, so
/// the debugger reported the line of the CALLER's `fn` for code that came
/// from a completely different place. Measured in this round: `gdb` said
/// `inl.fi:7` for an `add` whose own panic message said `inl.fi:3:18`.
///
/// The position therefore travels WITH the instruction. Every pass that
/// clones, moves or lifts an instruction carries it along for free; one that
/// builds a new instruction has to say where it comes from.
///
/// `line == 0` means "no position" (runtime code, compiler bookkeeping). That
/// is not a lie, it is a gap, and the code generator emits nothing for it
/// rather than letting the previous line stick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Loc {
    /// index into `dwarf::files()` (0 based; `.loc` counts from 1)
    pub file: u32,
    pub line: u32,
    pub col: u32,
}

impl Loc {
    pub const NONE: Loc = Loc { file: 0, line: 0, col: 0 };
    pub fn is_none(&self) -> bool {
        self.line == 0
    }
}

#[derive(Clone, Debug)]
pub struct Inst {
    pub dst: Option<Val>,
    pub ty: FTy,
    pub op: Op,
    /// ROUND 94 -- where this instruction comes from (see [`Loc`]).
    pub loc: Loc,
}

impl Inst {
    /// An instruction without a source position -- for everything a pass
    /// invents out of nothing (a spill copy, an `undef` constant).
    pub fn new(dst: Option<Val>, ty: FTy, op: Op) -> Inst {
        Inst { dst, ty, op, loc: Loc::NONE }
    }
    /// An instruction that INHERITS its position from another one. This is
    /// the constructor a pass should reach for: a value folded out of an
    /// instruction belongs to the same source line as the instruction it came
    /// from.
    pub fn like(dst: Option<Val>, ty: FTy, op: Op, from: Loc) -> Inst {
        Inst { dst, ty, op, loc: from }
    }
}

#[derive(Clone, Debug)]
pub enum Term {
    Br(BlockId),
    BrCond { cond: Val, then_bb: BlockId, else_bb: BlockId },
    /// Multi-way branch over an integer value (SPEC §6.3, `P4`).
    /// `cases` is sorted ascending by label and free of duplicates; every
    /// value not stated goes to `default`. The backend may turn that into a
    /// jump table, but need not.
    Switch { val: Val, ty: FTy, cases: Vec<(i128, BlockId)>, default: BlockId },
    Ret(Option<Val>),
    /// Only during the build; it must not appear any more at the end of the
    /// lowering (invariant: every block has a real terminator).
    Unset,
}

impl Term {
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Term::Br(b) => vec![*b],
            Term::BrCond { then_bb, else_bb, .. } => vec![*then_bb, *else_bb],
            Term::Switch { cases, default, .. } => {
                let mut v: Vec<BlockId> = cases.iter().map(|(_, b)| *b).collect();
                v.push(*default);
                v
            }
            Term::Ret(_) | Term::Unset => vec![],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Block {
    pub id: BlockId,
    pub insts: Vec<Inst>,
    pub term: Term,
}

impl Block {
    /// ROUND 92 — how many phi instructions stand at the front of the block.
    /// The invariant "phis first" makes this a prefix count, so anybody who
    /// wants to insert an ordinary instruction "at the top" inserts at this
    /// index and not at 0.
    pub fn phi_count(&self) -> usize {
        self.insts.iter().take_while(|i| matches!(i.op, Op::Phi { .. })).count()
    }
    pub fn has_phi(&self) -> bool {
        matches!(self.insts.first().map(|i| &i.op), Some(Op::Phi { .. }))
    }
}

#[derive(Clone, Debug)]
pub struct Func {
    pub name: String,
    pub params: Vec<FTy>,
    pub ret: FTy,
    pub blocks: Vec<Block>,
    /// Type of every value ever handed out, indexed by the Val id.
    pub val_types: Vec<FTy>,
    /// Values that stem from `secret[T]` (SPEC §9.2). The optimizer and the
    /// code generator treat them specially: no branch, no data dependent
    /// access, no removal of writes.
    pub secret: std::collections::HashSet<Val>,
    /// `#[constant_time]`: the code generator aborts when a conditional jump
    /// depends on a `secret` value.
    pub constant_time: bool,
    /// **Round 52** — `#[interrupt]`: a calling convention of its own. The code
    /// generator rescues ALL general purpose registers and closes with `iretq`
    /// rather than `ret` (SPEC §2, kernel profile).
    pub interrupt: bool,
    /// **ROUND 94** -- the position every newly pushed instruction is stamped
    /// with. `lower.rs` sets it per statement and per expression; everything
    /// else leaves it alone. It belongs to the FUNCTION and not to a global:
    /// functions are lowered one after another, and a stale stamp would
    /// attribute the prologue of the second to the last line of the first.
    pub loc_stamp: Loc,
}

impl Func {
    /// Creates a function with entry block `bb0`. The parameters receive the
    /// values `%0 .. %(n-1)`.
    pub fn new(name: &str, params: Vec<FTy>, ret: FTy) -> Func {
        let val_types = params.clone();
        Func {
            name: name.to_string(),
            params,
            ret,
            blocks: vec![Block { id: 0, insts: Vec::new(), term: Term::Unset }],
            val_types,
            secret: std::collections::HashSet::new(),
            constant_time: false,
            interrupt: false,
            loc_stamp: Loc::NONE,
        }
    }

    pub fn entry(&self) -> BlockId {
        0
    }

    pub fn param_val(&self, i: usize) -> Val {
        i as Val
    }

    pub fn add_block(&mut self) -> BlockId {
        let id = self.blocks.len() as BlockId;
        self.blocks.push(Block { id, insts: Vec::new(), term: Term::Unset });
        id
    }

    fn new_val(&mut self, ty: FTy) -> Val {
        let v = self.val_types.len() as Val;
        self.val_types.push(ty);
        v
    }

    /// New value without an instruction. The module tests build a body by
    /// hand with it (`licm.rs`, `regalloc.rs`), and since ROUND 82
    /// `peephole.rs` uses it to insert the shift amount of a division that
    /// has become a shift.
    pub fn new_val_pub(&mut self, ty: FTy) -> Val {
        self.new_val(ty)
    }

    /// Appends a value yielding instruction to the end of `b`.
    pub fn push(&mut self, b: BlockId, ty: FTy, op: Op) -> Val {
        let v = self.new_val(ty);
        let loc = self.loc_stamp;
        self.blocks[b as usize].insts.push(Inst { dst: Some(v), ty, op, loc });
        v
    }

    /// Appends an instruction without a result (`store`, `copymem`, void call).
    pub fn push_void(&mut self, b: BlockId, ty: FTy, op: Op) {
        let loc = self.loc_stamp;
        self.blocks[b as usize].insts.push(Inst { dst: None, ty, op, loc });
    }

    /// Inserts an alloca at the front of the entry block (invariant: all
    /// allocas stand in the entry block).
    pub fn alloca(&mut self, size: u64, align: u64) -> Val {
        let v = self.new_val(FTy::Ptr);
        let inst =
            Inst { dst: Some(v), ty: FTy::Ptr, op: Op::Alloca { size, align }, loc: self.loc_stamp };
        let n = self.blocks[0].insts.iter().take_while(|i| matches!(i.op, Op::Alloca { .. })).count();
        self.blocks[0].insts.insert(n, inst);
        v
    }

    pub fn set_term(&mut self, b: BlockId, t: Term) {
        let blk = &mut self.blocks[b as usize];
        if matches!(blk.term, Term::Unset) {
            blk.term = t;
        }
    }

    pub fn is_terminated(&self, b: BlockId) -> bool {
        !matches!(self.blocks[b as usize].term, Term::Unset)
    }

    /// Marks a value as secret (SPEC §9.1).
    ///
    /// There is deliberately NO helper method for it as long as the frontend
    /// cannot produce `secret` values (`secret[T]` is not implemented, SPEC
    /// §14.1): a method that only tests would call would be dead code. The
    /// set `secret` is public; tests write into it directly, and all passes
    /// read it through `is_secret`.

    pub fn is_secret(&self, v: Val) -> bool {
        self.secret.contains(&v)
    }

    pub fn val_ty(&self, v: Val) -> FTy {
        self.val_types.get(v as usize).copied().unwrap_or(FTy::Void)
    }

    /// **ROUND 92** — the phi invariants, checked rather than believed.
    ///
    /// Every pass that touches the control flow graph can break one of the
    /// three, and every one of them breaks the program in a way that only
    /// shows up as a wrong number much later. So they are stated once, here,
    /// and the module tests plus `FIRN_VERIFY_PHI=1` ask for them:
    ///
    ///   1. phis stand at the FRONT of their block
    ///   2. one entry per DISTINCT predecessor, and no entry for a block
    ///      that is not a predecessor
    ///   3. the entries are sorted by block number (determinism: two runs of
    ///      the compiler have to write the same text, and a `HashMap` walk
    ///      does not)
    pub fn verify_phis(&self) -> Result<(), String> {
        let n = self.blocks.len();
        let mut preds: Vec<Vec<BlockId>> = vec![Vec::new(); n];
        for b in &self.blocks {
            for s in b.term.successors() {
                if (s as usize) < n && !preds[s as usize].contains(&b.id) {
                    preds[s as usize].push(b.id);
                }
            }
        }
        for (bi, b) in self.blocks.iter().enumerate() {
            let np = b.phi_count();
            for (ii, i) in b.insts.iter().enumerate() {
                if matches!(i.op, Op::Phi { .. }) && ii >= np {
                    return Err(format!("@{} bb{}: phi at position {}, not at the front",
                                       self.name, b.id, ii));
                }
            }
            for i in b.insts.iter().take(np) {
                let inc = match &i.op {
                    Op::Phi { incoming } => incoming,
                    _ => continue,
                };
                let mut sorted = inc.clone();
                sorted.sort_by_key(|(p, _)| *p);
                if sorted != *inc {
                    return Err(format!("@{} bb{}: phi entries not sorted", self.name, b.id));
                }
                for k in 1..inc.len() {
                    if inc[k].0 == inc[k - 1].0 {
                        return Err(format!(
                            "@{} bb{}: phi %{:?} has TWO entries for bb{}",
                            self.name, b.id, i.dst, inc[k].0
                        ));
                    }
                }
                if inc.len() != preds[bi].len() {
                    return Err(format!(
                        "@{} bb{}: phi has {} entries, the block has {} predecessors",
                        self.name, b.id, inc.len(), preds[bi].len()
                    ));
                }
                for (p, _) in inc.iter() {
                    if !preds[bi].contains(p) {
                        return Err(format!(
                            "@{} bb{}: phi entry for bb{}, which is no predecessor",
                            self.name, b.id, p
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Does any block of this function carry a phi?
    pub fn has_phi(&self) -> bool {
        self.blocks.iter().any(|b| b.has_phi())
    }

    /// Count of all instructions (for optimization tests).
    pub fn inst_count(&self) -> usize {
        self.blocks.iter().map(|b| b.insts.len()).sum()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Module {
    pub funcs: Vec<Func>,
}

impl Module {
    pub fn new() -> Module {
        Module::default()
    }
    pub fn inst_count(&self) -> usize {
        self.funcs.iter().map(|f| f.inst_count()).sum()
    }
    pub fn block_count(&self) -> usize {
        self.funcs.iter().map(|f| f.blocks.len()).sum()
    }

    /// Human readable, stable text format (see docs/FIR.md).
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "; FIR v0");
        for f in &self.funcs {
            let ps: Vec<String> = f
                .params
                .iter()
                .enumerate()
                .map(|(i, t)| format!("%{}: {}", i, t.name()))
                .collect();
            let _ = writeln!(s, "fn @{}({}) -> {} {{", f.name, ps.join(", "), f.ret.name());
            for b in &f.blocks {
                let _ = writeln!(s, "bb{}:", b.id);
                for i in &b.insts {
                    let _ = writeln!(s, "  {}", fmt_inst(i));
                }
                let _ = writeln!(s, "  {}", fmt_term(&b.term));
            }
            let _ = writeln!(s, "}}");
        }
        s
    }
}

fn vlist(vs: &[Val]) -> String {
    vs.iter().map(|v| format!("%{}", v)).collect::<Vec<_>>().join(", ")
}

/// Escaping of the assembler template in the FIR text form. These four
/// characters only, so that BOTH compilers write the text alike, tableless.
pub(crate) fn asm_escape(v: &str) -> String {
    let mut o = String::new();
    for c in v.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            _ => o.push(c),
        }
    }
    o
}

fn fmt_inst(i: &Inst) -> String {
    let head = match i.dst {
        Some(d) => format!("%{} = ", d),
        None => String::new(),
    };
    let t = i.ty.name();
    let body = match &i.op {
        Op::Const(c) => format!("const.{} {}", t, c),
        Op::Bin(op, a, b) => format!("{}.{} %{}, %{}", op.name(), t, a, b),
        Op::BinWrapSat { kind, op, a, b } => {
            format!("{}_{}.{} %{}, %{}", op.name(), kind.name(), t, a, b)
        }
        Op::CheckedBin { op, a, b, msg } => {
            format!("checked_{}.{} %{}, %{} \"{}\"", op.name(), t, a, b, asm_escape(msg))
        }
        Op::CheckedDiv { op, a, b, msg_zero, msg_range } => {
            format!(
                "checked_{}.{} %{}, %{} \"{}\" \"{}\"",
                op.name(), t, a, b, asm_escape(msg_zero), asm_escape(msg_range)
            )
        }
        Op::CheckedCast { src, from, msg } => {
            format!("checked_cast.{}.{} %{} \"{}\"", from.name(), t, src, asm_escape(msg))
        }
        Op::CheckedIdx { idx, len, msg } => {
            format!("checked_idx.{} %{}, len={} \"{}\"", t, idx, len, asm_escape(msg))
        }
        Op::Cmp { op, ty, a, b } => format!("cmp.{}.{} %{}, %{}", op.name(), ty.name(), a, b),
        Op::Un(op, a) => match op {
            UnOp::Neg => format!("neg.{} %{}", t, a),
            UnOp::Not => format!("not.{} %{}", t, a),
        },
        Op::Cast { src, from } => format!("cast.{}.{} %{}", from.name(), t, src),
        Op::Alloca { size, align } => format!("alloca.ptr size={} align={}", size, align),
        Op::Load { addr } => format!("load.{} %{}", t, addr),
        Op::Store { addr, val } => format!("store.{} %{}, %{}", t, val, addr),
        Op::PtrAdd { base, off } => format!("ptradd.ptr %{}, %{}", base, off),
        Op::Call { name, args } => format!("call.{} @{}({})", t, name, vlist(args)),
        Op::CallIndirect { target, args } => {
            format!("calli.{} %{}({})", t, target, vlist(args))
        }
        Op::VtabAddr { table } => format!("vtab.ptr @{}", table),
        Op::FnRef { name } => format!("fnref.ptr @{}", name),
        Op::GlobalAddr { name } => format!("globaladdr.ptr @{}", name),
        Op::Syscall { args } => format!("syscall.{} {}", t, vlist(args)),
        Op::CopyMem { dst, src, size } => format!("copymem %{}, %{}, size={}", dst, src, size),
        Op::Select { cond, a, b } => format!("select.{} %{}, %{}, %{}", t, cond, a, b),
        // ROUND 92: `%7 = phi.i64 [bb1 %3, bb4 %9]` — the entries in block
        // order, which is the order the list is kept in anyway.
        Op::Phi { incoming } => {
            let ps: Vec<String> =
                incoming.iter().map(|(b, v)| format!("bb{} %{}", b, v)).collect();
            format!("phi.{} [{}]", t, ps.join(", "))
        }
        Op::Copy { src } => format!("copy.{} %{}", t, src),
        Op::Barrier { val } => format!("barrier.{} %{}", t, val),
        // ROUND 82: `simd.<kind>.<type> %a, %b, imm=N` — one line per
        // machine instruction, readable in `--emit=fir` like everything else.
        Op::Simd { kind, args, imm } => {
            format!("simd.{:?}.{} {} imm={}", kind, t, vlist(args), imm)
        }
        Op::SecureZero { addr, size } => format!("secure_zero %{}, %{}", addr, size),
        Op::AtomicAdd { addr, val } => format!("atomadd.{} %{}, %{}", t, addr, val),
        Op::Asm { template, out, in_regs, ins, out_regs, outs, clobber } => {
            let mut o = format!("asm.{} \"{}\"", t, asm_escape(template));
            if let Some(r) = out {
                o.push_str(&format!(" out={}", r));
            }
            if !outs.is_empty() {
                let ps: Vec<String> = out_regs
                    .iter()
                    .zip(outs.iter())
                    .map(|(r, v)| format!("{} %{}", r, v))
                    .collect();
                o.push_str(&format!(" out=[{}]", ps.join(", ")));
            }
            if !ins.is_empty() {
                let ps: Vec<String> = in_regs
                    .iter()
                    .zip(ins.iter())
                    .map(|(r, v)| format!("{} %{}", r, v))
                    .collect();
                o.push_str(&format!(" in=[{}]", ps.join(", ")));
            }
            if !clobber.is_empty() {
                o.push_str(&format!(" clobber=[{}]", clobber.join(", ")));
            }
            o
        }
        Op::MmioLoad { addr } => format!("mmio_load.{} %{}", t, addr),
        Op::MmioStore { addr, val } => format!("mmio_store.{} %{}, %{}", t, val, addr),
        Op::AtomicCas { addr, erw, new } => {
            format!("atomcas.{} %{}, %{}, %{}", t, addr, erw, new)
        }
        Op::ThreadSpawn { arg, stack, ctid } => {
            format!("spawn.{} %{}, %{}, %{}", t, arg, stack, ctid)
        }
        Op::ThreadSelf => format!("threadself.{}", t),
        Op::GcAddr { regs } => {
            if *regs {
                "gc_state.ptr regs=1".to_string()
            } else {
                "gc_state.ptr".to_string()
            }
        }
    };
    format!("{}{}", head, body)
}

fn fmt_term(t: &Term) -> String {
    match t {
        Term::Br(b) => format!("br bb{}", b),
        Term::BrCond { cond, then_bb, else_bb } => {
            format!("brcond %{}, bb{}, bb{}", cond, then_bb, else_bb)
        }
        Term::Switch { val, ty, cases, default } => {
            let arms: Vec<String> =
                cases.iter().map(|(k, b)| format!("{} => bb{}", k, b)).collect();
            format!("switch.{} %{} [{}] default bb{}", ty.name(), val, arms.join(", "), default)
        }
        Term::Ret(Some(v)) => format!("ret %{}", v),
        Term::Ret(None) => "ret".to_string(),
        Term::Unset => "<unset>".to_string(),
    }
}
