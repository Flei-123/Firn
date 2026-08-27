// SPDX-License-Identifier: GPL-2.0-only
//! **ROUND 82** — the vector and crypto instructions of the processor
//! (SPEC §8.6).
//!
//! ## Why this file exists
//!
//! Measured on this machine, before this round:
//!
//! | | Firn | reference | factor |
//! |---|---|---|---|
//! | SHA-256 | 22.6 MiB/s | OpenSSL 1424 MB/s | ~60x |
//! | AES-128-CBC | 5.50 MiB/s | OpenSSL 1025 MB/s | ~190x |
//! | AES-128-CFB8 | 0.34 MiB/s | | ~3000x |
//!
//! That gap is NOT the optimizer's. OpenSSL does not compute AES, it
//! *executes* it: `aesenc` is one machine instruction that does a whole
//! round. `sha256rnds2` does two rounds of SHA-256. A scalar implementation
//! needs some forty instructions for the same thing, and no optimizer closes
//! a factor of forty. Firn could not emit those instructions at all — that,
//! and nothing else, is the lack this file removes.
//!
//! ## The design decision: a TYPE plus INTRINSICS, not one or the other
//!
//! Two roads were open, and the round takes both halves of them deliberately:
//!
//! * **`v128` as a value type.** Without it every instruction would have to
//!   take and give its operands through memory, and the compiler could never
//!   keep an intermediate result in an `xmm` register. For AES that is not a
//!   detail: the round chain is strictly serial, `aesenc` has a latency of
//!   four cycles, a store/load round trip costs five to six on top. A value
//!   type is worth roughly a factor of two here, and it is what makes the
//!   difference between "as fast as OpenSSL" and "three times slower".
//!
//! * **Intrinsics instead of operators.** `v128` has NO `+`, `^` or `<<`.
//!   Sixteen octets have no element type; `a + b` would have to mean
//!   `paddb`, `paddw`, `paddd` or `paddq`, and whichever one the language
//!   picked would be wrong three times out of four. The processor's own
//!   instructions carry the reading in their NAME, so the intrinsics do the
//!   same: `__v128_add32`, `__v128_add64`. Nothing is implicit, and reading
//!   the Firn source tells you exactly which instruction comes out.
//!
//! Spelling: `__name(...)`, the house style of every other primitive in this
//! compiler (`__atomic_add` round 47, `__mmio_read32` round 52,
//! `__gc_state`). A leading `@` would have been a second syntax for the same
//! thing and would have needed the lexer, the parser, the formatter and
//! `lib/firnc1` to learn it. The gain would have been nil.
//!
//! ## Runtime detection, and the fallback
//!
//! `__cpu_features() -> u64` asks `cpuid` and gives a bit set back. A program
//! is expected to ask ONCE and then pick its path:
//!
//! ```firn
//! if (__cpu_features() & FEAT_SHA) != 0 { sha256_ni(...) } else { sha256_soft(...) }
//! ```
//!
//! The compiler does NOT insert that check by itself. It could not: it does
//! not know which of the two implementations you consider equivalent. What it
//! guarantees is that asking is cheap and possible everywhere, and
//! `lib/std/crypto` uses exactly this shape — the scalar path stays in the
//! binary, side by side with the fast one, and both are held against the same
//! test vectors (`tools/bench82/run.sh`, section 42 of `test.sh`).
//!
//! The bits (they are also `FEAT_*` constants in `lib/std/cpu.fi`):
//!
//! | bit | feature | `cpuid` |
//! |---|---|---|
//! | 0 | SSE2 | leaf 1, `edx` bit 26 |
//! | 1 | SSE4.1 | leaf 1, `ecx` bit 19 |
//! | 2 | SSE4.2 | leaf 1, `ecx` bit 20 |
//! | 3 | AES-NI | leaf 1, `ecx` bit 25 |
//! | 4 | PCLMULQDQ | leaf 1, `ecx` bit 1 |
//! | 5 | SHA-NI | leaf 7/0, `ebx` bit 29 |
//! | 6 | AVX2 | leaf 7/0, `ebx` bit 5 |
//! | 7 | BMI2 | leaf 7/0, `ebx` bit 8 |
//! | 8 | SSSE3 | leaf 1, `ecx` bit 9 |
//!
//! Leaf 7 is only asked when leaf 0 reports it exists; on a processor that
//! stops at leaf 1 the upper bits simply stay zero. Nothing here can fault
//! on an old machine, and that is the point: `cpuid` itself is available on
//! every x86_64 there is.
//!
//! ## The calling convention and the frame
//!
//! * A `v128` PARAMETER travels in the SSE class of System V AMD64, exactly
//!   like `f32`/`f64`: `xmm0`-`xmm7`, then the stack. A `v128` RESULT comes
//!   back in `xmm0`. That is what `place_args` in `codegen_x86.rs` already
//!   does for the floating point types; `v128` joins the same queue.
//! * All sixteen `xmm` registers are CALLER saved on System V — there is
//!   nothing to rescue in a prologue, and that is why the frame layout gains
//!   nothing but a wider slot: a `v128` value gets sixteen octets instead of
//!   eight, sixteen byte aligned (`rbp` is 16-aligned, so an offset that is a
//!   multiple of sixteen is enough).
//! * The consequence for the register cache below: it must be EMPTIED before
//!   every `call`, `syscall` and `asm`. That is done, and it is the only
//!   invalidation rule there is.
//!
//! ## aarch64
//!
//! The equivalents exist (`aese`/`aesmc`, `sha256h`/`sha256h2`/`sha256su0`)
//! but the aarch64 code generator of round 80 was not on `main` when this
//! round was built. `Op::Simd` therefore ends in a clean, named abort there
//! rather than in wrong code — see `docs/ROUND82.md` §6.

use crate::ast::{Expr, ExprKind};
use crate::codegen_x86::{load_full, store_dst, Emitter, Frame};
use crate::diag::Span;
use crate::fir::{BlockId, FTy, Func, Inst, Op, Term, Val};
use crate::lower::Lower;
use crate::sema::Checker;
use crate::types::Type;

// =====================================================================
// The instruction set
// =====================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SimdKind {
    // --- memory -------------------------------------------------------
    Load,
    Store,
    // --- construction / extraction ------------------------------------
    Zero,
    FromU64,
    GetU64,
    GetU32,
    SetU32,
    // --- bitwise ------------------------------------------------------
    Xor,
    And,
    Or,
    AndNot,
    // --- integer arithmetic -------------------------------------------
    Add8,
    Add32,
    Add64,
    Sub32,
    // --- shuffling and shifting ---------------------------------------
    ShuffleB,
    Shuffle32,
    AlignR,
    UnpackLo32,
    UnpackHi32,
    UnpackLo64,
    UnpackHi64,
    ShlBytes,
    ShrBytes,
    Shl32,
    Shr32,
    Shl64,
    Shr64,
    Blend16,
    // --- crypto -------------------------------------------------------
    AesEnc,
    AesEncLast,
    AesDec,
    AesDecLast,
    AesImc,
    AesKeyGenAssist,
    Sha256Rnds2,
    Sha256Msg1,
    Sha256Msg2,
    Pclmul,
    // --- scalar -------------------------------------------------------
    Crc32U8,
    Crc32U64,
    CpuFeatures,
}

impl SimdKind {
    /// Pure = no memory touched, removable when unused, mergeable by CSE.
    /// Exactly two of them are not.
    pub fn is_pure(self) -> bool {
        !matches!(self, SimdKind::Load | SimdKind::Store)
    }
}

/// Operand kind in the signature of an intrinsic.
#[derive(Clone, Copy, PartialEq, Eq)]
enum P {
    /// a `v128`
    V,
    /// `*u8` (reading)
    CPtr,
    /// `*mut u8`
    MPtr,
    U64,
    U32,
    U8,
}

impl P {
    fn ty(self) -> Type {
        match self {
            P::V => Type::V128,
            P::CPtr => Type::ptr(Type::U8, false),
            P::MPtr => Type::ptr(Type::U8, true),
            P::U64 => Type::U64,
            P::U32 => Type::U32,
            P::U8 => Type::U8,
        }
    }
}

/// One entry of the intrinsic table.
struct Sig {
    name: &'static str,
    kind: SimdKind,
    params: &'static [P],
    /// `Some(max)`: a trailing 8-bit immediate, an integer LITERAL in the
    /// source text, `0..=max`.
    imm: Option<u8>,
    ret: Option<P>,
}

const fn s(
    name: &'static str,
    kind: SimdKind,
    params: &'static [P],
    imm: Option<u8>,
    ret: Option<P>,
) -> Sig {
    Sig { name, kind, params, imm, ret }
}

/// THE list. Every name Firn understands, and nothing else.
static TABLE: &[Sig] = &[
    s("__v128_load", SimdKind::Load, &[P::CPtr], None, Some(P::V)),
    s("__v128_store", SimdKind::Store, &[P::MPtr, P::V], None, None),
    s("__v128_zero", SimdKind::Zero, &[], None, Some(P::V)),
    s("__v128_from_u64", SimdKind::FromU64, &[P::U64, P::U64], None, Some(P::V)),
    s("__v128_get_u64", SimdKind::GetU64, &[P::V], Some(1), Some(P::U64)),
    s("__v128_get_u32", SimdKind::GetU32, &[P::V], Some(3), Some(P::U32)),
    s("__v128_set_u32", SimdKind::SetU32, &[P::V, P::U32], Some(3), Some(P::V)),
    s("__v128_xor", SimdKind::Xor, &[P::V, P::V], None, Some(P::V)),
    s("__v128_and", SimdKind::And, &[P::V, P::V], None, Some(P::V)),
    s("__v128_or", SimdKind::Or, &[P::V, P::V], None, Some(P::V)),
    s("__v128_andnot", SimdKind::AndNot, &[P::V, P::V], None, Some(P::V)),
    s("__v128_add8", SimdKind::Add8, &[P::V, P::V], None, Some(P::V)),
    s("__v128_add32", SimdKind::Add32, &[P::V, P::V], None, Some(P::V)),
    s("__v128_add64", SimdKind::Add64, &[P::V, P::V], None, Some(P::V)),
    s("__v128_sub32", SimdKind::Sub32, &[P::V, P::V], None, Some(P::V)),
    s("__v128_shuffle8", SimdKind::ShuffleB, &[P::V, P::V], None, Some(P::V)),
    s("__v128_shuffle32", SimdKind::Shuffle32, &[P::V], Some(255), Some(P::V)),
    s("__v128_alignr", SimdKind::AlignR, &[P::V, P::V], Some(31), Some(P::V)),
    s("__v128_unpacklo32", SimdKind::UnpackLo32, &[P::V, P::V], None, Some(P::V)),
    s("__v128_unpackhi32", SimdKind::UnpackHi32, &[P::V, P::V], None, Some(P::V)),
    s("__v128_unpacklo64", SimdKind::UnpackLo64, &[P::V, P::V], None, Some(P::V)),
    s("__v128_unpackhi64", SimdKind::UnpackHi64, &[P::V, P::V], None, Some(P::V)),
    s("__v128_shl_bytes", SimdKind::ShlBytes, &[P::V], Some(15), Some(P::V)),
    s("__v128_shr_bytes", SimdKind::ShrBytes, &[P::V], Some(15), Some(P::V)),
    s("__v128_shl32", SimdKind::Shl32, &[P::V], Some(31), Some(P::V)),
    s("__v128_shr32", SimdKind::Shr32, &[P::V], Some(31), Some(P::V)),
    s("__v128_shl64", SimdKind::Shl64, &[P::V], Some(63), Some(P::V)),
    s("__v128_shr64", SimdKind::Shr64, &[P::V], Some(63), Some(P::V)),
    s("__v128_blend16", SimdKind::Blend16, &[P::V, P::V], Some(255), Some(P::V)),
    s("__aesenc", SimdKind::AesEnc, &[P::V, P::V], None, Some(P::V)),
    s("__aesenclast", SimdKind::AesEncLast, &[P::V, P::V], None, Some(P::V)),
    s("__aesdec", SimdKind::AesDec, &[P::V, P::V], None, Some(P::V)),
    s("__aesdeclast", SimdKind::AesDecLast, &[P::V, P::V], None, Some(P::V)),
    s("__aesimc", SimdKind::AesImc, &[P::V], None, Some(P::V)),
    s("__aeskeygenassist", SimdKind::AesKeyGenAssist, &[P::V], Some(255), Some(P::V)),
    s("__sha256rnds2", SimdKind::Sha256Rnds2, &[P::V, P::V, P::V], None, Some(P::V)),
    s("__sha256msg1", SimdKind::Sha256Msg1, &[P::V, P::V], None, Some(P::V)),
    s("__sha256msg2", SimdKind::Sha256Msg2, &[P::V, P::V], None, Some(P::V)),
    s("__pclmulqdq", SimdKind::Pclmul, &[P::V, P::V], Some(255), Some(P::V)),
    s("__crc32_u8", SimdKind::Crc32U8, &[P::U64, P::U8], None, Some(P::U64)),
    s("__crc32_u64", SimdKind::Crc32U64, &[P::U64, P::U64], None, Some(P::U64)),
    s("__cpu_features", SimdKind::CpuFeatures, &[], None, Some(P::U64)),
];

fn lookup(name: &str) -> Option<&'static Sig> {
    TABLE.iter().find(|e| e.name == name)
}

/// Is this spelling one of the intrinsics? (`lower.rs` asks before it hooks.)
pub(crate) fn is_simd_call(name: &str) -> bool {
    lookup(name).is_some()
}

/// Every name of the set — for `--list-simd` and for the proof that the
/// documentation is complete.
pub fn names() -> Vec<&'static str> {
    TABLE.iter().map(|e| e.name).collect()
}

// =====================================================================
// Type phase
// =====================================================================

/// The immediate operand has to be a literal: it is encoded INTO the
/// instruction, there is no register form of it. A named `const` would be
/// nicer to read but would need the constant evaluator here, and that one
/// runs later than this check.
fn imm_of(ck: &mut Checker, e: &Expr, name: &str, max: u8) -> Option<u8> {
    let v = match &e.kind {
        ExprKind::Int(v) => *v,
        _ => {
            ck.dg.error_note(
                e.span,
                format!("the last argument of '{}' has to be a literal number", name),
                "the value is encoded into the machine instruction; a variable cannot stand there",
            );
            return None;
        }
    };
    if v < 0 || v > max as i128 {
        ck.dg.error(
            e.span,
            format!("'{}': the immediate value has to lie in 0..={}, found {}", name, max, v),
        );
        return None;
    }
    Some(v as u8)
}

/// Hook from `sema::call`. `None` when the name is none of ours — or when the
/// program declares a function of that spelling itself; that one wins, in the
/// same way as with `__atomic_add` (round 47).
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Option<Type> {
    let sig = lookup(name)?;
    if ck.fns.contains_key(name) {
        return None;
    }
    let _ = nspan;
    let want = sig.params.len() + usize::from(sig.imm.is_some());
    if args.len() != want {
        for a in args {
            ck.type_out_expr(a);
        }
        ck.dg.error_note(
            espan,
            format!("'{}' expects {} argument(s), found {}", name, want, args.len()),
            &shape(sig),
        );
        return Some(Type::Error);
    }
    for (i, p) in sig.params.iter().enumerate() {
        let want_ty = p.ty();
        let got = ck.expr(&args[i], Some(&want_ty));
        if got.is_error() {
            continue;
        }
        if !accepts(*p, &got) {
            ck.dg.error_note(
                args[i].span,
                format!(
                    "'{}' expects {} as argument {}, found {}",
                    name,
                    ck.tcx.name_of(&want_ty),
                    i + 1,
                    ck.tcx.name_of(&got)
                ),
                &shape(sig),
            );
            return Some(Type::Error);
        }
    }
    if let Some(max) = sig.imm {
        if imm_of(ck, &args[sig.params.len()], name, max).is_none() {
            return Some(Type::Error);
        }
    }
    Some(match sig.ret {
        Some(p) => p.ty(),
        None => Type::Void,
    })
}

/// A pointer argument accepts both mutabilities where reading is enough, and
/// an untyped integer literal counts as any integer width.
fn accepts(p: P, got: &Type) -> bool {
    match p {
        P::V => *got == Type::V128,
        P::CPtr => matches!(got, Type::Ptr { inner, .. } if **inner == Type::U8),
        P::MPtr => matches!(got, Type::Ptr { mutable: true, inner } if **inner == Type::U8),
        P::U64 => matches!(got, Type::U64 | Type::Usize | Type::UntypedInt),
        P::U32 => matches!(got, Type::U32 | Type::UntypedInt),
        P::U8 => matches!(got, Type::U8 | Type::UntypedInt),
    }
}

fn shape(sig: &Sig) -> String {
    let mut out = format!("the form is {}(", sig.name);
    let mut first = true;
    for p in sig.params {
        if !first {
            out.push_str(", ");
        }
        first = false;
        out.push_str(match p {
            P::V => "v128",
            P::CPtr => "*u8",
            P::MPtr => "*mut u8",
            P::U64 => "u64",
            P::U32 => "u32",
            P::U8 => "u8",
        });
    }
    if let Some(max) = sig.imm {
        if !first {
            out.push_str(", ");
        }
        out.push_str(&format!("<literal 0..={}>", max));
    }
    out.push(')');
    if let Some(r) = &sig.ret {
        out.push_str(match r {
            P::V => " -> v128",
            P::U64 => " -> u64",
            P::U32 => " -> u32",
            _ => "",
        });
    }
    out
}

// =====================================================================
// Lowering phase
// =====================================================================

/// Hook from `lower::lower_call`.
pub(crate) fn lower_call(
    lo: &mut Lower,
    name: &str,
    args: &[Expr],
    span: Span,
) -> Option<Option<Val>> {
    let sig = lookup(name)?;
    if args.len() != sig.params.len() + usize::from(sig.imm.is_some()) {
        return lo.ice(span, "simd intrinsic with wrong arity");
    }
    let mut vals = Vec::with_capacity(sig.params.len());
    for a in args.iter().take(sig.params.len()) {
        vals.push(lo.lower_expr(a)?);
    }
    let imm = match sig.imm {
        Some(_) => match &args[sig.params.len()].kind {
            ExprKind::Int(v) => (*v as i64 as u64 & 0xff) as u8,
            _ => return lo.ice(span, "simd immediate is not a literal"),
        },
        None => 0,
    };
    let op = Op::Simd { kind: sig.kind, args: vals, imm };
    Some(match sig.ret {
        Some(P::V) => Some(lo.push(FTy::V128, op)),
        Some(P::U64) => Some(lo.push(FTy::U64, op)),
        Some(P::U32) => Some(lo.push(FTy::U32, op)),
        Some(_) => return lo.ice(span, "simd return kind not lowerable"),
        None => {
            lo.push_void(FTy::Void, op);
            None
        }
    })
}

// =====================================================================
// The xmm value cache of the base code path
// =====================================================================
//
// The base path of `codegen_x86.rs` gives every FIR value a frame slot and
// reloads it at every use. For integers that costs an L1 access; for the AES
// round chain it costs the whole gain, because `aesenc` has a latency of four
// cycles and a store/load round trip five to six.
//
// So `v128` values get a small write-back cache over twelve `xmm` registers.
// It is deliberately the simplest thing that is provably right:
//
//  * `xmm4`-`xmm15` are the pool. `xmm0`-`xmm3` stay scratch — `xmm0` is the
//    implicit third operand of `sha256rnds2`, and the floating point paths of
//    `codegen_x86.rs` compute in `xmm0`/`xmm1`.
//  * Every value keeps its frame slot as its HOME. A register copy is marked
//    `dirty` until it has been written back.
//  * INVALIDATION, and this is the whole of it: at the end of a basic block
//    (write back, then forget) and in front of every `call`, `syscall`,
//    `asm` and thread instruction (the same). All sixteen `xmm` registers are
//    caller saved on System V, so a call destroys the lot.
//  * Frame value slots have no address in the program (`layout()` hands them
//    out, nothing takes their address), so no `store`, `copymem` or
//    `secure_zero` can reach one. Memory writes therefore do NOT invalidate.
//
// `FIRN_NO_XMM_CACHE=1` switches the cache off and leaves the pure
// slot-to-slot path — that is how the gain was measured, see
// `docs/ROUND82.md` §3.

const POOL: [&str; 12] = [
    "xmm4", "xmm5", "xmm6", "xmm7", "xmm8", "xmm9", "xmm10", "xmm11", "xmm12",
    "xmm13", "xmm14", "xmm15",
];

/// Pure DATA. Every operation on it is a free function below that takes the
/// whole `Emitter` — the cache has to emit instructions while it reads its
/// own state, and a method would borrow the emitter twice.
pub(crate) struct XmmCache {
    val: [Option<Val>; 12],
    /// Where the entry belongs when it is written back. For an ordinary
    /// value that is its frame slot, for a promoted cell the storage of its
    /// `alloca` — two different tables, so the offset travels with the entry.
    off: [u64; 12],
    dirty: [bool; 12],
    used: [u64; 12],
    /// locked for the instruction currently being emitted
    lock: [bool; 12],
    tick: u64,
    pub(crate) on: bool,
    /// **The retirement plan** (per function, built by `xplan`).
    ///
    /// `home[v] = (b, i)` means: EVERY use of the value `v` lies in block `b`,
    /// and the last of them is the instruction with index `i`. After that
    /// instruction the register may be taken away from `v` WITHOUT writing it
    /// back — nobody will ever read it again.
    ///
    /// Why that is right even in a loop: if `v` is defined in `b` too, the
    /// next pass through `b` defines it afresh. If `v` comes from another
    /// block, its home slot holds it (every block flushes in front of its
    /// terminator), so the reload finds the right value. And if a use lay in
    /// a second block, `home[v]` is `(u32::MAX, 0)` and nothing is retired.
    ///
    /// `u32::MAX` also covers the terminator: a value the `brcond`/`ret`
    /// reads is recorded with the index `insts.len()`, which `xretire` is
    /// never called with.
    home: Vec<(u32, u32)>,
    retire_on: bool,
    /// **Promoted cells.** `cell[d] = Some(offset)` means: the `alloca` value
    /// `d` is sixteen octets big, sixteen byte aligned, and its pointer is
    /// used NOWHERE except as the direct address of a `load`/`store` of the
    /// type `v128`. Then the cell itself may live in an `xmm` register, and
    /// `load`/`store` become register moves.
    ///
    /// Without this a `var s: v128` costs two memory accesses PLUS the
    /// pointer load per read — measured in `sha256_ni_blocks`: 277 `movdqu`
    /// and 304 `mov` per 64 octet block. `mem2reg` cannot help there; it only
    /// promotes cells written ONCE, and a loop variable is written in every
    /// pass.
    cell: Vec<Option<u64>>,
}

impl Default for XmmCache {
    fn default() -> Self {
        XmmCache {
            val: [None; 12],
            off: [0; 12],
            dirty: [false; 12],
            used: [0; 12],
            lock: [false; 12],
            tick: 0,
            on: std::env::var_os("FIRN_NO_XMM_CACHE").is_none(),
            home: Vec::new(),
            retire_on: std::env::var_os("FIRN_NO_XMM_RETIRE").is_none(),
            cell: Vec::new(),
        }
    }
}

/// Build the retirement plan of ONE function. Called by `codegen_x86::emit_func`
/// before the first block; costs one pass over the instructions.
pub(crate) fn xplan(e: &mut Emitter, f: &Func, fr: &Frame) {
    let n = f.val_types.len();
    let mut home = vec![(u32::MAX, 0u32); n];
    // -1 = not yet seen, -2 = used in more than one block, else the block id
    let mut seen: Vec<i64> = vec![-1; n];
    let mut note = |v: Val, b: BlockId, i: u32, home: &mut Vec<(u32, u32)>, seen: &mut Vec<i64>| {
        let k = v as usize;
        if k >= seen.len() {
            return;
        }
        match seen[k] {
            -2 => {}
            -1 => {
                seen[k] = b as i64;
                home[k] = (b, i);
            }
            other if other == b as i64 => {
                if i > home[k].1 {
                    home[k].1 = i;
                }
            }
            _ => {
                seen[k] = -2;
                home[k] = (u32::MAX, 0);
            }
        }
    };
    let mut uses: Vec<Val> = Vec::new();
    for b in &f.blocks {
        for (i, inst) in b.insts.iter().enumerate() {
            uses.clear();
            inst.op.uses(&mut uses);
            for v in uses.iter() {
                note(*v, b.id, i as u32, &mut home, &mut seen);
            }
        }
        // The terminator counts as a use AFTER the last instruction. The
        // index `insts.len()` never reaches `xretire`, so such a value stays
        // until the flush — which is exactly right, the terminator reads it.
        let last = b.insts.len() as u32;
        match &b.term {
            Term::BrCond { cond, .. } => note(*cond, b.id, last, &mut home, &mut seen),
            Term::Switch { val, .. } => note(*val, b.id, last, &mut home, &mut seen),
            Term::Ret(Some(v)) => note(*v, b.id, last, &mut home, &mut seen),
            _ => {}
        }
    }
    e.xmm.home = home;
    e.xmm.cell = promote_cells(f, fr);
}

/// Which `alloca` may live in a register? The conditions are deliberately
/// narrow, because the reward for being wrong here is silently wrong code:
///
///  * sixteen octets big and sixteen byte aligned (so a `v128` fits exactly),
///  * the `alloca` value has a known frame offset,
///  * and EVERY use of its pointer is the `addr` of a `load` or a `store`
///    whose type is `v128`. One `ptradd`, one call argument, one `copymem`,
///    one pointer stored away — and the cell stays in memory.
fn promote_cells(f: &Func, fr: &Frame) -> Vec<Option<u64>> {
    let n = f.val_types.len();
    let mut cand: Vec<Option<u64>> = vec![None; n];
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Alloca { size: 16, align: 16 }) = (i.dst, &i.op) {
                if let Some(Some(off)) = fr.alloca_off.get(d as usize) {
                    cand[d as usize] = Some(*off);
                }
            }
        }
    }
    if cand.iter().all(|c| c.is_none()) {
        return vec![None; n];
    }
    let mut uses: Vec<Val> = Vec::new();
    for b in &f.blocks {
        for inst in &b.insts {
            let ok_addr = match (&inst.op, inst.ty) {
                (Op::Load { addr }, FTy::V128) => Some(*addr),
                (Op::Store { addr, val }, FTy::V128) => {
                    // The pointer must be the ADDRESS, never the value.
                    if cand.get(*val as usize).map(|c| c.is_some()).unwrap_or(false) {
                        cand[*val as usize] = None;
                    }
                    Some(*addr)
                }
                _ => None,
            };
            uses.clear();
            inst.op.uses(&mut uses);
            for v in uses.iter() {
                if Some(*v) == ok_addr {
                    continue;
                }
                if let Some(slot) = cand.get_mut(*v as usize) {
                    *slot = None;
                }
            }
        }
        let t = match &b.term {
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Switch { val, .. } => Some(*val),
            Term::Ret(Some(v)) => Some(*v),
            _ => None,
        };
        if let Some(v) = t {
            if let Some(slot) = cand.get_mut(v as usize) {
                *slot = None;
            }
        }
    }
    cand
}

/// Is this `alloca` value a promoted cell? Then its storage offset.
pub(crate) fn cell_of(e: &Emitter, v: Val) -> Option<u64> {
    e.xmm.cell.get(v as usize).copied().flatten()
}

/// After the instruction with index `idx` in block `bid`: give up every
/// register whose value will never be read again. No write back — that is
/// the whole point.
pub(crate) fn xretire(e: &mut Emitter, bid: BlockId, idx: u32) {
    if !e.xmm.retire_on || e.xmm.home.is_empty() {
        return;
    }
    for k in 0..POOL.len() {
        if let Some(v) = e.xmm.val[k] {
            if e.xmm.home.get(v as usize) == Some(&(bid, idx)) {
                e.xmm.val[k] = None;
                e.xmm.dirty[k] = false;
            }
        }
    }
}

fn slot_at(fr: &Frame, v: Val) -> String {
    format!("xmmword ptr [rbp-{}]", fr.slot[v as usize])
}

fn at(off: u64) -> String {
    format!("xmmword ptr [rbp-{}]", off)
}

/// Write every dirty register back into its home slot and forget everything.
/// Called at the end of every basic block and in front of every `call`,
/// `syscall`, `asm` and thread instruction.
pub(crate) fn xflush(e: &mut Emitter, fr: &Frame) {
    let _ = fr;
    for k in 0..POOL.len() {
        if e.xmm.val[k].is_some() && e.xmm.dirty[k] {
            let home = at(e.xmm.off[k]);
            e.line(&format!("movdqa {}, {}", home, POOL[k]));
        }
        e.xmm.val[k] = None;
        e.xmm.dirty[k] = false;
        e.xmm.lock[k] = false;
    }
}

/// Forget WITHOUT writing back. Only right where nothing can be dirty: at the
/// start of a block, because every predecessor has flushed at its end.
pub(crate) fn xclear(e: &mut Emitter) {
    e.xmm.val = [None; 12];
    e.xmm.dirty = [false; 12];
    e.xmm.lock = [false; 12];
}

fn xpick(e: &mut Emitter, fr: &Frame) -> usize {
    let _ = fr;
    for k in 0..POOL.len() {
        if e.xmm.val[k].is_none() && !e.xmm.lock[k] {
            return k;
        }
    }
    let mut best = usize::MAX;
    for k in 0..POOL.len() {
        if e.xmm.lock[k] {
            continue;
        }
        if best == usize::MAX || e.xmm.used[k] < e.xmm.used[best] {
            best = k;
        }
    }
    // Cannot happen: at most four operands are locked at a time, the pool
    // holds twelve. The fallback keeps the generator total all the same.
    let k = if best == usize::MAX { 0 } else { best };
    if e.xmm.val[k].is_some() && e.xmm.dirty[k] {
        let home = at(e.xmm.off[k]);
        e.line(&format!("movdqa {}, {}", home, POOL[k]));
    }
    e.xmm.val[k] = None;
    e.xmm.dirty[k] = false;
    k
}

fn xtouch(e: &mut Emitter, k: usize) {
    e.xmm.tick += 1;
    e.xmm.used[k] = e.xmm.tick;
    e.xmm.lock[k] = true;
}

/// The register the value `v` stands in — loaded from its slot if it is not
/// there yet. Locked until the end of the current instruction.
fn xget(e: &mut Emitter, fr: &Frame, v: Val) -> &'static str {
    let off = fr.slot[v as usize];
    xget_at(e, fr, v, off)
}

/// The same for a promoted cell, whose home is its `alloca` storage.
pub(crate) fn xget_cell(e: &mut Emitter, fr: &Frame, c: Val, off: u64) -> &'static str {
    xget_at(e, fr, c, off)
}

pub(crate) fn xdef_cell(e: &mut Emitter, fr: &Frame, c: Val, off: u64) -> &'static str {
    xdef_at(e, fr, c, off)
}

fn xget_at(e: &mut Emitter, fr: &Frame, v: Val, off: u64) -> &'static str {
    if e.xmm.on {
        for k in 0..POOL.len() {
            if e.xmm.val[k] == Some(v) {
                xtouch(e, k);
                return POOL[k];
            }
        }
        let k = xpick(e, fr);
        let home = at(off);
        e.line(&format!("movdqa {}, {}", POOL[k], home));
        e.xmm.val[k] = Some(v);
        e.xmm.off[k] = off;
        e.xmm.dirty[k] = false;
        xtouch(e, k);
        return POOL[k];
    }
    // Cache switched off (`FIRN_NO_XMM_CACHE=1`): three fixed scratch
    // registers, one per operand position of the current instruction.
    let k = (e.xmm.tick % 3) as usize;
    e.xmm.tick += 1;
    let r = ["xmm1", "xmm2", "xmm3"][k];
    let home = at(off);
    e.line(&format!("movdqa {}, {}", r, home));
    r
}

/// A register to WRITE the value `d` into. With the cache on, the write back
/// happens at the next flush; with it off, `xstore` puts it in the slot at
/// once.
fn xdef(e: &mut Emitter, fr: &Frame, d: Val) -> &'static str {
    let off = fr.slot[d as usize];
    xdef_at(e, fr, d, off)
}

fn xdef_at(e: &mut Emitter, fr: &Frame, d: Val, off: u64) -> &'static str {
    if e.xmm.on {
        for k in 0..POOL.len() {
            if e.xmm.val[k] == Some(d) {
                e.xmm.val[k] = None;
                e.xmm.dirty[k] = false;
            }
        }
        let k = xpick(e, fr);
        e.xmm.val[k] = Some(d);
        e.xmm.off[k] = off;
        e.xmm.dirty[k] = true;
        xtouch(e, k);
        return POOL[k];
    }
    "xmm0"
}

/// The counterpart of `xdef` for the switched off cache.
fn xstore(e: &mut Emitter, fr: &Frame, d: Val, r: &str) {
    if !e.xmm.on {
        let home = slot_at(fr, d);
        e.line(&format!("movdqa {}, {}", home, r));
    }
}

/// `Op::Load` out of a promoted cell: a register move, no memory at all.
pub(crate) fn emit_cell_load(e: &mut Emitter, fr: &Frame, d: Val, c: Val, off: u64) {
    xunlock(e);
    let rc = xget_cell(e, fr, c, off);
    let rd = xdef(e, fr, d);
    if rd != rc {
        e.line(&format!("movdqa {}, {}", rd, rc));
    }
    xstore(e, fr, d, rd);
}

/// `Op::Store` into a promoted cell: likewise.
pub(crate) fn emit_cell_store(e: &mut Emitter, fr: &Frame, c: Val, off: u64, v: Val) {
    xunlock(e);
    let rv = xget(e, fr, v);
    let rc = xdef_cell(e, fr, c, off);
    if rc != rv {
        e.line(&format!("movdqa {}, {}", rc, rv));
    }
    if !e.xmm.on {
        let home = at(off);
        e.line(&format!("movdqa {}, {}", home, rc));
    }
}

/// The three registers outside the pool. `xmm0` is the implicit operand of
/// `sha256rnds2` and the working register of the floating point paths.
fn xunlock(e: &mut Emitter) {
    e.xmm.lock = [false; 12];
    if !e.xmm.on {
        e.xmm.tick = 0;
    }
}

// =====================================================================
// Code generation
// =====================================================================

/// Serial number for the labels of the `cpuid` sequence.
fn next_label() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

/// `Op::Simd` -> machine instructions. The only place in the compiler where a
/// vector instruction is written down.
pub(crate) fn emit(e: &mut Emitter, fr: &Frame, i: &Inst) -> Result<(), String> {
    let (kind, args, imm) = match &i.op {
        Op::Simd { kind, args, imm } => (*kind, args.clone(), *imm),
        _ => return Err("internal error: simd::emit on a non-simd instruction".into()),
    };
    xunlock(e);

    /// `d = a <m> b`, destructive on the first operand.
    fn bin(e: &mut Emitter, fr: &Frame, m: &str, d: Val, a: Val, b: Val) {
        let ra = xget(e, fr, a);
        let rb = xget(e, fr, b);
        let rd = xdef(e, fr, d);
        if rd != ra {
            e.line(&format!("movdqa {}, {}", rd, ra));
        }
        e.line(&format!("{} {}, {}", m, rd, rb));
        xstore(e, fr, d, rd);
    }
    /// The same with an 8-bit immediate.
    fn bin_imm(e: &mut Emitter, fr: &Frame, m: &str, d: Val, a: Val, b: Val, imm: u8) {
        let ra = xget(e, fr, a);
        let rb = xget(e, fr, b);
        let rd = xdef(e, fr, d);
        if rd != ra {
            e.line(&format!("movdqa {}, {}", rd, ra));
        }
        e.line(&format!("{} {}, {}, {}", m, rd, rb, imm));
        xstore(e, fr, d, rd);
    }
    /// One operand, destructive, with an immediate (`pslldq` and friends).
    fn un_destr(e: &mut Emitter, fr: &Frame, m: &str, d: Val, a: Val, imm: u8) {
        let ra = xget(e, fr, a);
        let rd = xdef(e, fr, d);
        if rd != ra {
            e.line(&format!("movdqa {}, {}", rd, ra));
        }
        e.line(&format!("{} {}, {}", m, rd, imm));
        xstore(e, fr, d, rd);
    }
    /// One operand, NON destructive (`pshufd`, `aesimc`, `aeskeygenassist`).
    fn un_src(e: &mut Emitter, fr: &Frame, m: &str, d: Val, a: Val, imm: Option<u8>) {
        let ra = xget(e, fr, a);
        let rd = xdef(e, fr, d);
        match imm {
            Some(k) => e.line(&format!("{} {}, {}, {}", m, rd, ra, k)),
            None => e.line(&format!("{} {}, {}", m, rd, ra)),
        }
        xstore(e, fr, d, rd);
    }

    let dst = i.dst;
    let need = |d: Option<Val>| -> Result<Val, String> {
        d.ok_or_else(|| "internal error: simd instruction without target".to_string())
    };

    match kind {
        SimdKind::Load => {
            let d = need(dst)?;
            load_full(e, fr, "rax", args[0]);
            let rd = xdef(e, fr, d);
            // UNALIGNED: the pointer comes out of the program and promises
            // nothing. On everything since Nehalem `movdqu` on an aligned
            // address costs exactly as much as `movdqa`.
            e.line(&format!("movdqu {}, xmmword ptr [rax]", rd));
            xstore(e, fr, d, rd);
        }
        SimdKind::Store => {
            let rv = xget(e, fr, args[1]);
            load_full(e, fr, "rax", args[0]);
            e.line(&format!("movdqu xmmword ptr [rax], {}", rv));
        }
        SimdKind::Zero => {
            let d = need(dst)?;
            let rd = xdef(e, fr, d);
            e.line(&format!("pxor {}, {}", rd, rd));
            xstore(e, fr, d, rd);
        }
        SimdKind::FromU64 => {
            let d = need(dst)?;
            load_full(e, fr, "rax", args[0]);
            let rd = xdef(e, fr, d);
            e.line(&format!("movq {}, rax", rd));
            load_full(e, fr, "rax", args[1]);
            e.line("movq xmm1, rax");
            e.line(&format!("punpcklqdq {}, xmm1", rd));
            xstore(e, fr, d, rd);
        }
        SimdKind::GetU64 => {
            let d = need(dst)?;
            let ra = xget(e, fr, args[0]);
            if imm == 0 {
                e.line(&format!("movq rax, {}", ra));
            } else {
                e.line(&format!("pextrq rax, {}, {}", ra, imm));
            }
            store_dst(e, fr, d, "rax");
        }
        SimdKind::GetU32 => {
            let d = need(dst)?;
            let ra = xget(e, fr, args[0]);
            if imm == 0 {
                e.line(&format!("movd eax, {}", ra));
            } else {
                e.line(&format!("pextrd eax, {}, {}", ra, imm));
            }
            store_dst(e, fr, d, "rax");
        }
        SimdKind::SetU32 => {
            let d = need(dst)?;
            let ra = xget(e, fr, args[0]);
            load_full(e, fr, "rax", args[1]);
            let rd = xdef(e, fr, d);
            if rd != ra {
                e.line(&format!("movdqa {}, {}", rd, ra));
            }
            e.line(&format!("pinsrd {}, eax, {}", rd, imm));
            xstore(e, fr, d, rd);
        }
        SimdKind::Xor => bin(e, fr, "pxor", need(dst)?, args[0], args[1]),
        SimdKind::And => bin(e, fr, "pand", need(dst)?, args[0], args[1]),
        SimdKind::Or => bin(e, fr, "por", need(dst)?, args[0], args[1]),
        // `pandn d, s` computes `~d & s`, so the FIRST operand is the negated
        // one — exactly what `andnot(a, b) = ~a & b` says.
        SimdKind::AndNot => bin(e, fr, "pandn", need(dst)?, args[0], args[1]),
        SimdKind::Add8 => bin(e, fr, "paddb", need(dst)?, args[0], args[1]),
        SimdKind::Add32 => bin(e, fr, "paddd", need(dst)?, args[0], args[1]),
        SimdKind::Add64 => bin(e, fr, "paddq", need(dst)?, args[0], args[1]),
        SimdKind::Sub32 => bin(e, fr, "psubd", need(dst)?, args[0], args[1]),
        SimdKind::ShuffleB => bin(e, fr, "pshufb", need(dst)?, args[0], args[1]),
        SimdKind::UnpackLo32 => bin(e, fr, "punpckldq", need(dst)?, args[0], args[1]),
        SimdKind::UnpackHi32 => bin(e, fr, "punpckhdq", need(dst)?, args[0], args[1]),
        SimdKind::UnpackLo64 => bin(e, fr, "punpcklqdq", need(dst)?, args[0], args[1]),
        SimdKind::UnpackHi64 => bin(e, fr, "punpckhqdq", need(dst)?, args[0], args[1]),
        SimdKind::AesEnc => bin(e, fr, "aesenc", need(dst)?, args[0], args[1]),
        SimdKind::AesEncLast => bin(e, fr, "aesenclast", need(dst)?, args[0], args[1]),
        SimdKind::AesDec => bin(e, fr, "aesdec", need(dst)?, args[0], args[1]),
        SimdKind::AesDecLast => bin(e, fr, "aesdeclast", need(dst)?, args[0], args[1]),
        SimdKind::Sha256Msg1 => bin(e, fr, "sha256msg1", need(dst)?, args[0], args[1]),
        SimdKind::Sha256Msg2 => bin(e, fr, "sha256msg2", need(dst)?, args[0], args[1]),
        SimdKind::AlignR => bin_imm(e, fr, "palignr", need(dst)?, args[0], args[1], imm),
        SimdKind::Blend16 => bin_imm(e, fr, "pblendw", need(dst)?, args[0], args[1], imm),
        SimdKind::Pclmul => bin_imm(e, fr, "pclmulqdq", need(dst)?, args[0], args[1], imm),
        SimdKind::ShlBytes => un_destr(e, fr, "pslldq", need(dst)?, args[0], imm),
        SimdKind::ShrBytes => un_destr(e, fr, "psrldq", need(dst)?, args[0], imm),
        SimdKind::Shl32 => un_destr(e, fr, "pslld", need(dst)?, args[0], imm),
        SimdKind::Shr32 => un_destr(e, fr, "psrld", need(dst)?, args[0], imm),
        SimdKind::Shl64 => un_destr(e, fr, "psllq", need(dst)?, args[0], imm),
        SimdKind::Shr64 => un_destr(e, fr, "psrlq", need(dst)?, args[0], imm),
        SimdKind::Shuffle32 => un_src(e, fr, "pshufd", need(dst)?, args[0], Some(imm)),
        SimdKind::AesKeyGenAssist => {
            un_src(e, fr, "aeskeygenassist", need(dst)?, args[0], Some(imm))
        }
        SimdKind::AesImc => un_src(e, fr, "aesimc", need(dst)?, args[0], None),
        SimdKind::Sha256Rnds2 => {
            // The third operand is IMPLICIT: `sha256rnds2` always reads its
            // two round constants out of `xmm0`. That is the reason `xmm0` is
            // not in the pool.
            let d = need(dst)?;
            let ra = xget(e, fr, args[0]);
            let rb = xget(e, fr, args[1]);
            let rk = xget(e, fr, args[2]);
            e.line(&format!("movdqa xmm0, {}", rk));
            let rd = xdef(e, fr, d);
            if rd != ra {
                e.line(&format!("movdqa {}, {}", rd, ra));
            }
            e.line(&format!("sha256rnds2 {}, {}, xmm0", rd, rb));
            xstore(e, fr, d, rd);
        }
        SimdKind::Crc32U8 => {
            let d = need(dst)?;
            load_full(e, fr, "rax", args[0]);
            load_full(e, fr, "rcx", args[1]);
            e.line("crc32 eax, cl");
            store_dst(e, fr, d, "rax");
        }
        SimdKind::Crc32U64 => {
            let d = need(dst)?;
            load_full(e, fr, "rax", args[0]);
            load_full(e, fr, "rcx", args[1]);
            e.line("crc32 rax, rcx");
            store_dst(e, fr, d, "rax");
        }
        SimdKind::CpuFeatures => {
            let d = need(dst)?;
            emit_cpuid(e);
            store_dst(e, fr, d, "rax");
        }
    }
    Ok(())
}

/// A `v128` value out of an `xmm` register into its frame slot, and back.
/// Used by `codegen_x86.rs` for parameters, results and `Op::Load`/`Op::Store`
/// of the type `v128`.
pub(crate) fn slot_from_reg(e: &mut Emitter, fr: &Frame, d: Val, r: &str) {
    let at = slot_at(fr, d);
    e.line(&format!("movdqa {}, {}", at, r));
}

pub(crate) fn reg_from_slot(e: &mut Emitter, fr: &Frame, r: &str, v: Val) {
    let at = slot_at(fr, v);
    e.line(&format!("movdqa {}, {}", r, at));
}

/// `Op::Load` / `Op::Store` with the type `v128`: through a POINTER out of the
/// program, therefore `movdqu`.
pub(crate) fn emit_ptr_load(e: &mut Emitter, fr: &Frame, d: Val, addr: Val) {
    xunlock(e);
    load_full(e, fr, "rax", addr);
    let rd = xdef(e, fr, d);
    e.line(&format!("movdqu {}, xmmword ptr [rax]", rd));
    xstore(e, fr, d, rd);
}

pub(crate) fn emit_ptr_store(e: &mut Emitter, fr: &Frame, addr: Val, val: Val) {
    xunlock(e);
    let rv = xget(e, fr, val);
    load_full(e, fr, "rax", addr);
    e.line(&format!("movdqu xmmword ptr [rax], {}", rv));
}

/// `__cpu_features()` — the `cpuid` sequence, result in `rax`.
///
/// `rbx` is CALLEE saved and `cpuid` overwrites it; that is why it is pushed.
/// The other scratch registers used (`r9`, `r10`, `r11`) are caller saved and
/// carry no value on the base path of the code generator.
pub(crate) fn emit_cpuid_pub(e: &mut Emitter) {
    emit_cpuid(e)
}

fn emit_cpuid(e: &mut Emitter) {
    let n = next_label();
    let done = format!(".Lcpuid_done_{}", n);
    let no7 = format!(".Lcpuid_no7_{}", n);
    e.raw("    # __cpu_features(): cpuid leaf 1 and leaf 7/0 -> bit set");
    e.line("push rbx");
    e.line("xor r9d, r9d");
    e.line("xor eax, eax");
    e.line("xor ecx, ecx");
    e.line("cpuid");
    e.line("mov r10d, eax");
    e.line("cmp r10d, 1");
    e.line(&format!("jb {}", done));
    e.line("mov eax, 1");
    e.line("xor ecx, ecx");
    e.line("cpuid");
    // (register of leaf 1, bit in it, bit in the result)
    for (src, bit, out) in [
        ("edx", 26u32, 0u32), // SSE2
        ("ecx", 19, 1),       // SSE4.1
        ("ecx", 20, 2),       // SSE4.2
        ("ecx", 25, 3),       // AES-NI
        ("ecx", 1, 4),        // PCLMULQDQ
        ("ecx", 9, 8),        // SSSE3
    ] {
        e.line(&format!("mov r11d, {}", src));
        e.line(&format!("shr r11d, {}", bit));
        e.line("and r11d, 1");
        if out > 0 {
            e.line(&format!("shl r11d, {}", out));
        }
        e.line("or r9d, r11d");
    }
    e.line("cmp r10d, 7");
    e.line(&format!("jb {}", no7));
    e.line("mov eax, 7");
    e.line("xor ecx, ecx");
    e.line("cpuid");
    // SHA-NI, AVX2 and BMI2 all sit in `ebx` of leaf 7, subleaf 0.
    for (bit, out) in [(29u32, 5u32), (5, 6), (8, 7)] {
        e.line("mov r11d, ebx");
        e.line(&format!("shr r11d, {}", bit));
        e.line("and r11d, 1");
        e.line(&format!("shl r11d, {}", out));
        e.line("or r9d, r11d");
    }
    e.raw(&format!("{}:", no7));
    e.raw(&format!("{}:", done));
    e.line("pop rbx");
    e.line("mov rax, r9");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_is_unique() {
        let mut seen = std::collections::HashSet::new();
        for e in TABLE {
            assert!(seen.insert(e.name), "duplicate: {}", e.name);
        }
    }

    #[test]
    fn every_name_carries_the_house_prefix() {
        for e in TABLE {
            assert!(e.name.starts_with("__"), "{}", e.name);
        }
    }

    #[test]
    fn only_load_and_store_are_impure() {
        let impure: Vec<_> =
            TABLE.iter().filter(|e| !e.kind.is_pure()).map(|e| e.name).collect();
        assert_eq!(impure, vec!["__v128_load", "__v128_store"]);
    }
}
