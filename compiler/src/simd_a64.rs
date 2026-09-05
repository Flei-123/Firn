// SPDX-License-Identifier: GPL-2.0-only
//! **ROUND 91** — the 42 vector and crypto intrinsics of round 82 on the
//! SECOND machine.
//!
//! ## What was measured before this round
//!
//! ```text
//! $ bash tools/aarch64/run.sh
//!   DIFF tests/1613_crypto.fi :: aarch64 compilation failed:
//!        "--target=aarch64-linux cannot emit the vector instruction Load yet."
//!   DIFFERENT: 1
//! ```
//!
//! One case out of 304, and it was the LAST one. Round 82 gave the language
//! `v128` and 42 intrinsics and wrote them for x86-64 only; round 87 made
//! `__cpu_features()` and the two `crc32` intrinsics answer on aarch64 so
//! that a program which only ASKS what the machine can do compiles here.
//! Everything that touches a vector still ended in a named abort — and
//! `lib/std/crypto/accel.fi` MENTIONS `v128`, which is enough to stop the
//! code generator even where the code would never run. That made the whole
//! crypto library uncompilable for aarch64.
//!
//! This file is the other half. It is to `codegen_a64.rs` what `simd.rs` is
//! to `codegen_x86.rs`: the only place in the compiler where an A64 vector
//! instruction is written down.
//!
//! ## The rule this file follows
//!
//! An intrinsic is named after an x86 instruction, and what it MEANS is that
//! instruction's semantics — not "whatever the other machine's instruction of
//! the same name does". Three groups come out of that:
//!
//! 1. **The counterpart is exact.** `pxor` -> `eor`, `paddd` -> `add .4s`,
//!    `punpckldq` -> `zip1 .4s`, `palignr` -> `ext`, `movdqu` -> `ldr q`.
//!    One instruction for one instruction.
//!
//! 2. **The counterpart exists but is laid out differently.** This is the
//!    trap of the round and it is worth naming precisely:
//!
//!    | x86 | ARM |
//!    |---|---|
//!    | `AESENC(s,k) = MixColumns(SubBytes(ShiftRows(s))) ^ k` | `AESE(s,k) = SubBytes(ShiftRows(s ^ k))`, `AESMC(x) = MixColumns(x)` |
//!
//!    ARM's `AESE` already contains the AddRoundKey, and it does it FIRST;
//!    x86's `AESENC` does it LAST. Writing `aese s, k` for `__aesenc(s, k)`
//!    would compile, run, and produce a different cipher. The identity that
//!    does hold is
//!
//!    ```text
//!    AESENC(s, k)     == AESMC(AESE(s, 0)) ^ k
//!    AESENCLAST(s, k) ==       AESE(s, 0)  ^ k
//!    AESDEC(s, k)     == AESIMC(AESD(s, 0)) ^ k
//!    AESDECLAST(s, k) ==        AESD(s, 0)  ^ k
//!    ```
//!
//!    and it is checked, not believed: `tests/1614_simd_ops.fi` runs the
//!    FIPS 197 C.1 vector through the intrinsics on both machines and
//!    `tools/aarch64/run.sh` compares the two outputs.
//!
//! 3. **The counterpart does not decompose the same way at all.** SHA-256.
//!    x86 splits the state into `ABEF`/`CDGH` and does TWO rounds per
//!    instruction (`sha256rnds2`); ARM keeps `ABCD`/`EFGH` and does FOUR
//!    (`sha256h`/`sha256h2`). There is no way to build one out of the other,
//!    so `__sha256rnds2` is built out of plain scalar instructions here —
//!    the two rounds it defines, written out. `__sha256msg2` likewise: ARM's
//!    `sha256su1` folds in a `W[i+9]` term that x86's `SHA256MSG2` does not.
//!    `__sha256msg1` IS `sha256su0`, exactly, and gets it.
//!
//! ## Registers
//!
//! `v16`-`v23` are caller-saved in AAPCS64 and no value of the base path of
//! `codegen_a64.rs` ever lives in a register across two instructions — every
//! FIR value has a frame slot. So the vector scratch set is free at the
//! start of every `Op::Simd`, and so are `x0`-`x7`: they are argument
//! registers, and arguments are only ever loaded immediately before a `bl`
//! (`load_args`) or a `svc` (`emit_syscall`). `__sha256rnds2` needs fourteen
//! 32-bit registers at once and uses them for exactly that reason.
//!
//! `x12` stays what it is everywhere else in this backend: addresses only.

use crate::codegen_a64::{at, imm_into, load_full, store_dst, uniq, w, Frame, A, B};
use crate::codegen_x86::Emitter;
use crate::fir::{Inst, Op, Val};
use crate::simd::SimdKind;

// --------------------------------------------------------------- registers

/// first operand
const VA: &str = "v16";
/// second operand
const VB: &str = "v17";
/// third operand / second helper
const VC: &str = "v18";
/// the result
const VR: &str = "v19";
/// helper (index vectors, blend masks)
const VT: &str = "v20";
/// helper (the zero vector, constants)
const VZ: &str = "v21";

/// The `q` name of a vector register — `ldr`/`str` address the 128-bit view
/// under that name, everything else under `v..`.
fn qn(v: &str) -> String {
    format!("q{}", &v[1..])
}

/// `v16` -> `v16.16b`
fn b16(v: &str) -> String {
    format!("{}.16b", v)
}

/// The label of the eight octets in `.bss` that hold the stack pointer the
/// kernel handed to `_start`. `__cpu_features()` walks the auxiliary vector
/// from there; see `emit_cpu_features`.
pub(crate) const AUXV_LABEL: &str = "__firn_a64_start_sp";

/// Does this module ask what the machine can do? Only then is the label
/// above worth eight octets and four instructions in `_start`.
pub(crate) fn needs_auxv(m: &crate::fir::Module) -> bool {
    m.funcs.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts.iter().any(|i| {
                matches!(i.op, Op::Simd { kind: SimdKind::CpuFeatures, .. })
            })
        })
    })
}

/// The `.bss` word and its alignment, appended once per object file.
pub(crate) fn auxv_data_asm() -> String {
    format!(
        ".section .bss,\"aw\",@nobits\n{}\n{}:\n    .zero 8\n.text\n",
        crate::target::align(8),
        AUXV_LABEL
    )
}

/// The four instructions at the very top of `_start` that keep the pointer.
pub(crate) fn emit_auxv_save(e: &mut Emitter) {
    e.raw("    // ROUND 91: the stack pointer at process start IS the auxiliary");
    e.raw("    // vector (argc, argv, envp, then the AT_* pairs). __cpu_features()");
    e.raw("    // reads AT_HWCAP out of it -- aarch64 has no `cpuid`.");
    e.line(&format!("adrp {}, {}", A, AUXV_LABEL));
    e.line(&format!("add {}, {}, :lo12:{}", A, A, AUXV_LABEL));
    e.line(&format!("mov {}, sp", B));
    e.line(&format!("str {}, [{}]", B, A));
}

// ----------------------------------------------------------------- helpers

/// A `v128` value out of its frame slot into a vector register.
pub(crate) fn vload(e: &mut Emitter, fr: &Frame, r: &str, v: Val) {
    let m = at(e, fr, v, 16);
    e.line(&format!("ldr {}, {}", qn(r), m));
}

/// ...and back. `layout` in `codegen_a64.rs` gives a `v128` value sixteen
/// octets at a sixteen octet boundary, so this is always a single `str q`.
pub(crate) fn vstore(e: &mut Emitter, fr: &Frame, d: Val, r: &str) {
    let m = at(e, fr, d, 16);
    e.line(&format!("str {}, {}", qn(r), m));
}

/// A 128-bit CONSTANT into a vector register. A64 has no way to write one
/// down, so it is built: the two halves through `x9` and `ins`. Every
/// constant in this file is an index vector or a mask, so this happens at
/// most twice per instruction.
fn vconst(e: &mut Emitter, r: &str, lo: u64, hi: u64) {
    if lo == 0 && hi == 0 {
        e.line(&format!("movi {}, #0", b16(r)));
        return;
    }
    imm_into(e, A, lo as i64);
    e.line(&format!("fmov d{}, {}", &r[1..], A));
    imm_into(e, A, hi as i64);
    e.line(&format!("ins {}.d[1], {}", r, A));
}

/// Sixteen index octets as the two halves of a 128-bit constant.
fn bytes128(b: &[u8; 16]) -> (u64, u64) {
    let mut lo = 0u64;
    let mut hi = 0u64;
    for k in 0..8 {
        lo |= (b[k] as u64) << (8 * k);
        hi |= (b[k + 8] as u64) << (8 * k);
    }
    (lo, hi)
}

fn vmov(e: &mut Emitter, d: &str, s: &str) {
    if d != s {
        e.line(&format!("mov {}, {}", b16(d), b16(s)));
    }
}

/// `d = a <mn> b` at the element width `sfx` (`16b`, `4s`, `2d`).
fn bin(e: &mut Emitter, fr: &Frame, mn: &str, sfx: &str, d: Val, a: Val, b: Val) {
    vload(e, fr, VA, a);
    vload(e, fr, VB, b);
    e.line(&format!("{} {}.{}, {}.{}, {}.{}", mn, VR, sfx, VA, sfx, VB, sfx));
    vstore(e, fr, d, VR);
}

/// A shift by an immediate at the element width `sfx`. `ushr` cannot encode
/// a shift of zero (its range is 1..=width), so that case is a move — which
/// is what x86's `psrld xmm, 0` does as well.
fn shift(e: &mut Emitter, fr: &Frame, mn: &str, sfx: &str, d: Val, a: Val, imm: u8) {
    vload(e, fr, VA, a);
    if imm == 0 {
        vmov(e, VR, VA);
    } else {
        e.line(&format!("{} {}.{}, {}.{}, #{}", mn, VR, sfx, VA, sfx, imm));
    }
    vstore(e, fr, d, VR);
}

/// One AES round in the x86 reading, out of the two ARM instructions that
/// carry the pieces. `enc` picks `aese`/`aesmc` against `aesd`/`aesimc`,
/// `mix` says whether it is the last round.
fn aes_round(e: &mut Emitter, fr: &Frame, d: Val, a: Val, k: Val, enc: bool, mix: bool) {
    vload(e, fr, VA, a);
    vload(e, fr, VB, k);
    // The round key of the ARM instruction is ZERO: its AddRoundKey is not
    // the one x86 means. The real key is xored on afterwards.
    e.line(&format!("movi {}, #0", b16(VZ)));
    vmov(e, VR, VA);
    e.line(&format!("{} {}, {}", if enc { "aese" } else { "aesd" }, b16(VR), b16(VZ)));
    if mix {
        e.line(&format!(
            "{} {}, {}",
            if enc { "aesmc" } else { "aesimc" },
            b16(VR),
            b16(VR)
        ));
    }
    e.line(&format!("eor {}, {}, {}", b16(VR), b16(VR), b16(VB)));
    vstore(e, fr, d, VR);
}

/// `sigma0`/`sigma1` of FIPS 180-4 on a 32-bit register, in place.
/// `x = ror(x,r0) ^ ror(x,r1) ^ (x >> s)`.
fn small_sigma(e: &mut Emitter, r: &str, r0: u32, r1: u32, s: u32, t0: &str, t1: &str) {
    e.line(&format!("ror {}, {}, #{}", w(t0), w(r), r0));
    e.line(&format!("ror {}, {}, #{}", w(t1), w(r), r1));
    e.line(&format!("eor {}, {}, {}", w(t0), w(t0), w(t1)));
    e.line(&format!("lsr {}, {}, #{}", w(t1), w(r), s));
    e.line(&format!("eor {}, {}, {}", w(r), w(t0), w(t1)));
}

/// `Sigma0`/`Sigma1` of FIPS 180-4: three rotations, no shift.
/// `dst = ror(x,r0) ^ ror(x,r1) ^ ror(x,r2)`.
fn big_sigma(e: &mut Emitter, dst: &str, x: &str, r0: u32, r1: u32, r2: u32, t: &str) {
    e.line(&format!("ror {}, {}, #{}", w(dst), w(x), r0));
    e.line(&format!("ror {}, {}, #{}", w(t), w(x), r1));
    e.line(&format!("eor {}, {}, {}", w(dst), w(dst), w(t)));
    e.line(&format!("ror {}, {}, #{}", w(t), w(x), r2));
    e.line(&format!("eor {}, {}, {}", w(dst), w(dst), w(t)));
}

/// `Op::Load` with the type `v128` — through a POINTER out of the program,
/// so nothing about the address is promised and `ldr q` does not care.
pub(crate) fn emit_ptr_load(e: &mut Emitter, fr: &Frame, d: Val, addr: Val) {
    load_full(e, fr, B, addr);
    e.line(&format!("ldr {}, [{}]", qn(VA), B));
    vstore(e, fr, d, VA);
}

/// `Op::Store` with the type `v128`.
pub(crate) fn emit_ptr_store(e: &mut Emitter, fr: &Frame, addr: Val, val: Val) {
    vload(e, fr, VA, val);
    load_full(e, fr, B, addr);
    e.line(&format!("str {}, [{}]", qn(VA), B));
}

// ------------------------------------------------------------------- emit

/// `Op::Simd` -> A64 instructions. The counterpart of `simd::emit`.
pub(crate) fn emit(e: &mut Emitter, fr: &Frame, i: &Inst) -> Result<(), String> {
    let (kind, args, imm) = match &i.op {
        Op::Simd { kind, args, imm } => (*kind, args.clone(), *imm),
        _ => return Err("internal error: simd_a64::emit on a non-simd instruction".into()),
    };
    let dst = i.dst;
    let need = |d: Option<Val>| -> Result<Val, String> {
        d.ok_or_else(|| "internal error: simd instruction without target".to_string())
    };

    match kind {
        // --- memory ----------------------------------------------------
        // UNALIGNED, like x86's `movdqu`: the pointer comes out of the
        // program and promises nothing. `ldr q` does not care.
        SimdKind::Load => {
            let d = need(dst)?;
            load_full(e, fr, B, args[0]);
            e.line(&format!("ldr {}, [{}]", qn(VA), B));
            vstore(e, fr, d, VA);
        }
        SimdKind::Store => {
            vload(e, fr, VA, args[1]);
            load_full(e, fr, B, args[0]);
            e.line(&format!("str {}, [{}]", qn(VA), B));
        }
        // --- construction / extraction ---------------------------------
        SimdKind::Zero => {
            let d = need(dst)?;
            e.line(&format!("movi {}, #0", b16(VR)));
            vstore(e, fr, d, VR);
        }
        SimdKind::FromU64 => {
            let d = need(dst)?;
            load_full(e, fr, A, args[0]);
            // `fmov d..` writes the low half AND clears the high one, so the
            // register is defined before `ins` fills the rest.
            e.line(&format!("fmov d{}, {}", &VR[1..], A));
            load_full(e, fr, A, args[1]);
            e.line(&format!("ins {}.d[1], {}", VR, A));
            vstore(e, fr, d, VR);
        }
        SimdKind::GetU64 => {
            let d = need(dst)?;
            vload(e, fr, VA, args[0]);
            e.line(&format!("umov {}, {}.d[{}]", A, VA, imm));
            store_dst(e, fr, d, A);
        }
        SimdKind::GetU32 => {
            let d = need(dst)?;
            vload(e, fr, VA, args[0]);
            // A write to a `w` register clears the upper half — the same
            // rule that makes `movd eax, xmm` a zero extension on x86.
            e.line(&format!("umov {}, {}.s[{}]", w(A), VA, imm));
            store_dst(e, fr, d, A);
        }
        SimdKind::SetU32 => {
            let d = need(dst)?;
            vload(e, fr, VA, args[0]);
            load_full(e, fr, A, args[1]);
            vmov(e, VR, VA);
            e.line(&format!("ins {}.s[{}], {}", VR, imm, w(A)));
            vstore(e, fr, d, VR);
        }
        // --- bitwise ---------------------------------------------------
        SimdKind::Xor => bin(e, fr, "eor", "16b", need(dst)?, args[0], args[1]),
        SimdKind::And => bin(e, fr, "and", "16b", need(dst)?, args[0], args[1]),
        SimdKind::Or => bin(e, fr, "orr", "16b", need(dst)?, args[0], args[1]),
        // `andnot(a, b) = ~a & b`, and `bic d, n, m` is `n & ~m` — so the
        // operands change places, and only here.
        SimdKind::AndNot => bin(e, fr, "bic", "16b", need(dst)?, args[1], args[0]),
        // --- integer arithmetic ----------------------------------------
        SimdKind::Add8 => bin(e, fr, "add", "16b", need(dst)?, args[0], args[1]),
        SimdKind::Add32 => bin(e, fr, "add", "4s", need(dst)?, args[0], args[1]),
        SimdKind::Add64 => bin(e, fr, "add", "2d", need(dst)?, args[0], args[1]),
        SimdKind::Sub32 => bin(e, fr, "sub", "4s", need(dst)?, args[0], args[1]),
        // --- shuffling and shifting ------------------------------------
        // `pshufb`: an index with its top bit set writes a zero octet, and
        // only the low four bits count otherwise. `tbl` writes a zero for
        // every index >= 16 — so masking with 0x8f turns the one rule into
        // the other exactly.
        SimdKind::ShuffleB => {
            let d = need(dst)?;
            vload(e, fr, VA, args[0]);
            vload(e, fr, VB, args[1]);
            vconst(e, VZ, 0x8f8f_8f8f_8f8f_8f8f, 0x8f8f_8f8f_8f8f_8f8f);
            e.line(&format!("and {}, {}, {}", b16(VT), b16(VB), b16(VZ)));
            e.line(&format!("tbl {}, {{{}}}, {}", b16(VR), b16(VA), b16(VT)));
            vstore(e, fr, d, VR);
        }
        // `pshufd`: four 32-bit lanes picked by two bits each. A64 has no
        // lane shuffle with an immediate, so the permutation becomes an
        // index vector and one `tbl`.
        SimdKind::Shuffle32 => {
            let d = need(dst)?;
            let mut idx = [0u8; 16];
            for lane in 0..4usize {
                let src = ((imm >> (2 * lane)) & 3) as u8;
                for b in 0..4usize {
                    idx[4 * lane + b] = 4 * src + b as u8;
                }
            }
            let (lo, hi) = bytes128(&idx);
            vload(e, fr, VA, args[0]);
            vconst(e, VT, lo, hi);
            e.line(&format!("tbl {}, {{{}}}, {}", b16(VR), b16(VA), b16(VT)));
            vstore(e, fr, d, VR);
        }
        // `palignr(a, b, n)`: the 32 octets `a:b` shifted right by n, low
        // half kept. `ext d, n, m, #k` takes the sixteen octets starting at
        // k of `n:m` — the same window, with the operands the other way
        // round. Above sixteen the shift walks out of `b` into `a` and
        // zeroes come in at the top; x86 says the same.
        SimdKind::AlignR => {
            let d = need(dst)?;
            vload(e, fr, VA, args[0]);
            vload(e, fr, VB, args[1]);
            if imm == 0 {
                vmov(e, VR, VB);
            } else if imm < 16 {
                e.line(&format!("ext {}, {}, {}, #{}", b16(VR), b16(VB), b16(VA), imm));
            } else if imm == 16 {
                vmov(e, VR, VA);
            } else if imm < 32 {
                e.line(&format!("movi {}, #0", b16(VZ)));
                e.line(&format!("ext {}, {}, {}, #{}", b16(VR), b16(VA), b16(VZ), imm - 16));
            } else {
                e.line(&format!("movi {}, #0", b16(VR)));
            }
            vstore(e, fr, d, VR);
        }
        SimdKind::UnpackLo32 => bin(e, fr, "zip1", "4s", need(dst)?, args[0], args[1]),
        SimdKind::UnpackHi32 => bin(e, fr, "zip2", "4s", need(dst)?, args[0], args[1]),
        SimdKind::UnpackLo64 => bin(e, fr, "zip1", "2d", need(dst)?, args[0], args[1]),
        SimdKind::UnpackHi64 => bin(e, fr, "zip2", "2d", need(dst)?, args[0], args[1]),
        // `pslldq` / `psrldq`: a shift of the whole register by whole
        // octets, zeroes coming in. `ext` against the zero vector is that.
        SimdKind::ShlBytes => {
            let d = need(dst)?;
            vload(e, fr, VA, args[0]);
            if imm == 0 {
                vmov(e, VR, VA);
            } else if imm < 16 {
                e.line(&format!("movi {}, #0", b16(VZ)));
                e.line(&format!("ext {}, {}, {}, #{}", b16(VR), b16(VZ), b16(VA), 16 - imm));
            } else {
                e.line(&format!("movi {}, #0", b16(VR)));
            }
            vstore(e, fr, d, VR);
        }
        SimdKind::ShrBytes => {
            let d = need(dst)?;
            vload(e, fr, VA, args[0]);
            if imm == 0 {
                vmov(e, VR, VA);
            } else if imm < 16 {
                e.line(&format!("movi {}, #0", b16(VZ)));
                e.line(&format!("ext {}, {}, {}, #{}", b16(VR), b16(VA), b16(VZ), imm));
            } else {
                e.line(&format!("movi {}, #0", b16(VR)));
            }
            vstore(e, fr, d, VR);
        }
        SimdKind::Shl32 => shift(e, fr, "shl", "4s", need(dst)?, args[0], imm),
        SimdKind::Shr32 => shift(e, fr, "ushr", "4s", need(dst)?, args[0], imm),
        SimdKind::Shl64 => shift(e, fr, "shl", "2d", need(dst)?, args[0], imm),
        SimdKind::Shr64 => shift(e, fr, "ushr", "2d", need(dst)?, args[0], imm),
        // `pblendw`: eight 16-bit lanes, one bit of the immediate each.
        // `bsl` takes the mask in the DESTINATION, so the mask is built
        // there and the two sources follow.
        SimdKind::Blend16 => {
            let d = need(dst)?;
            let mut m = [0u8; 16];
            for lane in 0..8usize {
                let f = if (imm >> lane) & 1 != 0 { 0xffu8 } else { 0x00 };
                m[2 * lane] = f;
                m[2 * lane + 1] = f;
            }
            let (lo, hi) = bytes128(&m);
            vload(e, fr, VA, args[0]);
            vload(e, fr, VB, args[1]);
            vconst(e, VR, lo, hi);
            e.line(&format!("bsl {}, {}, {}", b16(VR), b16(VB), b16(VA)));
            vstore(e, fr, d, VR);
        }
        // --- crypto: AES -----------------------------------------------
        SimdKind::AesEnc => aes_round(e, fr, need(dst)?, args[0], args[1], true, true),
        SimdKind::AesEncLast => aes_round(e, fr, need(dst)?, args[0], args[1], true, false),
        SimdKind::AesDec => aes_round(e, fr, need(dst)?, args[0], args[1], false, true),
        SimdKind::AesDecLast => aes_round(e, fr, need(dst)?, args[0], args[1], false, false),
        // The one AES instruction that means the same on both machines.
        SimdKind::AesImc => {
            let d = need(dst)?;
            vload(e, fr, VA, args[0]);
            e.line(&format!("aesimc {}, {}", b16(VR), b16(VA)));
            vstore(e, fr, d, VR);
        }
        // `aeskeygenassist(a, rcon)`:
        //     dst[31:0]    = SubWord(a[63:32])
        //     dst[63:32]   = RotWord(SubWord(a[63:32])) ^ rcon
        //     dst[95:64]   = SubWord(a[127:96])
        //     dst[127:96]  = RotWord(SubWord(a[127:96])) ^ rcon
        //
        // ARM has no plain SubBytes — `aese` always shifts the rows first.
        // So the rows are shifted BACK before it (one `tbl` with the inverse
        // permutation), and what comes out is SubBytes and nothing else.
        // The rest is a second `tbl` that picks the four words and one `eor`
        // that puts the round constant in.
        SimdKind::AesKeyGenAssist => {
            let d = need(dst)?;
            vload(e, fr, VA, args[0]);
            // ShiftRows sends octet i to (i + 4*(i mod 4)) mod 16; this is
            // the inverse of that map, as an index vector.
            let inv: [u8; 16] = [0, 13, 10, 7, 4, 1, 14, 11, 8, 5, 2, 15, 12, 9, 6, 3];
            let (lo, hi) = bytes128(&inv);
            vconst(e, VT, lo, hi);
            e.line(&format!("tbl {}, {{{}}}, {}", b16(VR), b16(VA), b16(VT)));
            e.line(&format!("movi {}, #0", b16(VZ)));
            e.line(&format!("aese {}, {}", b16(VR), b16(VZ)));
            let sel: [u8; 16] = [4, 5, 6, 7, 5, 6, 7, 4, 12, 13, 14, 15, 13, 14, 15, 12];
            let (slo, shi) = bytes128(&sel);
            vconst(e, VT, slo, shi);
            e.line(&format!("tbl {}, {{{}}}, {}", b16(VC), b16(VR), b16(VT)));
            let rc = (imm as u64) << 32;
            vconst(e, VZ, rc, rc);
            e.line(&format!("eor {}, {}, {}", b16(VR), b16(VC), b16(VZ)));
            vstore(e, fr, d, VR);
        }
        // --- crypto: SHA-256 -------------------------------------------
        // `sha256msg1(a, b)` = a[e] + sigma0(concat(a,b)[e+1]) for the four
        // lanes. ARM's `sha256su0 Vd.4S, Vn.4S` is that, word for word.
        SimdKind::Sha256Msg1 => {
            let d = need(dst)?;
            vload(e, fr, VA, args[0]);
            vload(e, fr, VB, args[1]);
            vmov(e, VR, VA);
            e.line(&format!("sha256su0 {}.4s, {}.4s", VR, VB));
            vstore(e, fr, d, VR);
        }
        // `sha256msg2(a, b)`:
        //     W16 = a[0] + sigma1(b[2]);   W18 = a[2] + sigma1(W16)
        //     W17 = a[1] + sigma1(b[3]);   W19 = a[3] + sigma1(W17)
        // ARM's `sha256su1` is NOT this: it adds a `W[i+9]` term of its own.
        // So the four words are computed where the dependency chain is
        // cheapest — in the general registers.
        SimdKind::Sha256Msg2 => {
            let d = need(dst)?;
            vload(e, fr, VA, args[0]);
            vload(e, fr, VB, args[1]);
            // w9 = W16, w10 = W17, w11 = W18, w15 = W19; w13 carries the
            // lane of `a`, w14/w17 are the two rotation temporaries.
            for (out, lane_b, lane_a) in [("x9", 2u8, 0u8), ("x10", 3, 1)] {
                e.line(&format!("umov {}, {}.s[{}]", w(out), VB, lane_b));
                small_sigma(e, out, 17, 19, 10, "x14", "x17");
                e.line(&format!("umov {}, {}.s[{}]", w("x13"), VA, lane_a));
                e.line(&format!("add {}, {}, {}", w(out), w("x13"), w(out)));
            }
            for (out, src, lane_a) in [("x11", "x9", 2u8), ("x15", "x10", 3)] {
                e.line(&format!("mov {}, {}", w(out), w(src)));
                small_sigma(e, out, 17, 19, 10, "x14", "x17");
                e.line(&format!("umov {}, {}.s[{}]", w("x13"), VA, lane_a));
                e.line(&format!("add {}, {}, {}", w(out), w("x13"), w(out)));
            }
            e.line(&format!("fmov s{}, {}", &VR[1..], w("x9")));
            e.line(&format!("ins {}.s[1], {}", VR, w("x10")));
            e.line(&format!("ins {}.s[2], {}", VR, w("x11")));
            e.line(&format!("ins {}.s[3], {}", VR, w("x15")));
            vstore(e, fr, d, VR);
        }
        // `sha256rnds2(src1, src2, wk)` — TWO rounds of FIPS 180-4 with the
        // state spread over two registers:
        //     A = src2[3] B = src2[2] C = src1[3] D = src1[2]
        //     E = src2[1] F = src2[0] G = src1[1] H = src1[0]
        //     WK0 = wk[0], WK1 = wk[1]
        //     result = { F, E, B, A } after both rounds
        // ARM's `sha256h`/`sha256h2` do FOUR rounds and keep `ABCD`/`EFGH`;
        // neither can be halved, so the two rounds stand here as what they
        // are. Fourteen registers, no memory traffic in between.
        SimdKind::Sha256Rnds2 => {
            let d = need(dst)?;
            vload(e, fr, VA, args[0]);
            vload(e, fr, VB, args[1]);
            vload(e, fr, VC, args[2]);
            e.raw("    // __sha256rnds2: two SHA-256 rounds, written out");
            for (r, v, lane) in [
                ("x0", VB, 3u8), // A
                ("x1", VB, 2),   // B
                ("x2", VA, 3),   // C
                ("x3", VA, 2),   // D
                ("x4", VB, 1),   // E
                ("x5", VB, 0),   // F
                ("x6", VA, 1),   // G
                ("x7", VA, 0),   // H
                ("x9", VC, 0),   // WK0
                ("x10", VC, 1),  // WK1
            ] {
                e.line(&format!("umov {}, {}.s[{}]", w(r), v, lane));
            }
            for wk in ["x9", "x10"] {
                // T1 = Ch(E,F,G) + Sigma1(E) + WK + H
                e.line(&format!("and {}, {}, {}", w("x11"), w("x4"), w("x5")));
                e.line(&format!("bic {}, {}, {}", w("x13"), w("x6"), w("x4")));
                e.line(&format!("eor {}, {}, {}", w("x11"), w("x11"), w("x13")));
                big_sigma(e, "x13", "x4", 6, 11, 25, "x14");
                e.line(&format!("add {}, {}, {}", w("x11"), w("x11"), w("x13")));
                e.line(&format!("add {}, {}, {}", w("x11"), w("x11"), w("x7")));
                e.line(&format!("add {}, {}, {}", w("x11"), w("x11"), w(wk)));
                // newE = T1 + D
                e.line(&format!("add {}, {}, {}", w("x15"), w("x11"), w("x3")));
                // newA = T1 + Maj(A,B,C) + Sigma0(A)
                e.line(&format!("and {}, {}, {}", w("x13"), w("x0"), w("x1")));
                e.line(&format!("and {}, {}, {}", w("x14"), w("x0"), w("x2")));
                e.line(&format!("eor {}, {}, {}", w("x13"), w("x13"), w("x14")));
                e.line(&format!("and {}, {}, {}", w("x14"), w("x1"), w("x2")));
                e.line(&format!("eor {}, {}, {}", w("x13"), w("x13"), w("x14")));
                big_sigma(e, "x14", "x0", 2, 13, 22, "x17");
                e.line(&format!("add {}, {}, {}", w("x13"), w("x13"), w("x14")));
                e.line(&format!("add {}, {}, {}", w("x11"), w("x11"), w("x13")));
                // D<-C, C<-B, B<-A, A<-newA;  H<-G, G<-F, F<-E, E<-newE
                e.line(&format!("mov {}, {}", w("x3"), w("x2")));
                e.line(&format!("mov {}, {}", w("x2"), w("x1")));
                e.line(&format!("mov {}, {}", w("x1"), w("x0")));
                e.line(&format!("mov {}, {}", w("x0"), w("x11")));
                e.line(&format!("mov {}, {}", w("x7"), w("x6")));
                e.line(&format!("mov {}, {}", w("x6"), w("x5")));
                e.line(&format!("mov {}, {}", w("x5"), w("x4")));
                e.line(&format!("mov {}, {}", w("x4"), w("x15")));
            }
            e.line(&format!("fmov s{}, {}", &VR[1..], w("x5")));
            e.line(&format!("ins {}.s[1], {}", VR, w("x4")));
            e.line(&format!("ins {}.s[2], {}", VR, w("x1")));
            e.line(&format!("ins {}.s[3], {}", VR, w("x0")));
            vstore(e, fr, d, VR);
        }
        // `pclmulqdq(a, b, imm)`: bit 0 of the immediate picks the qword of
        // `a`, bit 4 the one of `b`. A64 splits that into two instructions,
        // `pmull` (low against low) and `pmull2` (high against high), so the
        // chosen halves are moved down first and one `pmull` does the rest.
        SimdKind::Pclmul => {
            let d = need(dst)?;
            vload(e, fr, VA, args[0]);
            vload(e, fr, VB, args[1]);
            if imm & 1 != 0 {
                e.line(&format!("ext {}, {}, {}, #8", b16(VR), b16(VA), b16(VA)));
            } else {
                vmov(e, VR, VA);
            }
            if imm & 0x10 != 0 {
                e.line(&format!("ext {}, {}, {}, #8", b16(VT), b16(VB), b16(VB)));
            } else {
                vmov(e, VT, VB);
            }
            e.line(&format!("pmull {}.1q, {}.1d, {}.1d", VR, VR, VT));
            vstore(e, fr, d, VR);
        }
        // --- scalar ----------------------------------------------------
        // ROUND 87: SSE 4.2's `crc32` computes the CASTAGNOLI polynomial,
        // and A64 has exactly that as `crc32cb`/`crc32cx`.
        SimdKind::Crc32U8 => {
            let d = need(dst)?;
            load_full(e, fr, A, args[0]);
            load_full(e, fr, B, args[1]);
            e.line(&format!("crc32cb {}, {}, {}", w(A), w(A), w(B)));
            store_dst(e, fr, d, A);
        }
        SimdKind::Crc32U64 => {
            let d = need(dst)?;
            load_full(e, fr, A, args[0]);
            load_full(e, fr, B, args[1]);
            e.line(&format!("crc32cx {}, {}, {}", w(A), w(A), B));
            store_dst(e, fr, d, A);
        }
        SimdKind::CpuFeatures => {
            let d = need(dst)?;
            emit_cpu_features(e);
            store_dst(e, fr, d, A);
        }
    }
    Ok(())
}

/// `__cpu_features()` on aarch64: **AT_HWCAP out of the auxiliary vector**,
/// mapped onto the bit set of `lib/std/cpu.fi`.
///
/// There is no `cpuid` here. The usual answer is `getauxval(AT_HWCAP)` — a
/// libc function, and this compiler has no libc. What libc does is read the
/// vector the kernel wrote above the initial stack, and that pointer is
/// still there: `_start` keeps it (`emit_auxv_save`). The walk is
/// argc, argv, the NULL after argv, the environment, the NULL after it, and
/// then pairs of (type, value) until type 0. Type 16 is AT_HWCAP.
///
/// The MAPPING is the honest part of this function. The bits are named after
/// x86 features because that is where they were minted; what they mean is
/// "the intrinsics of this family work on this machine":
///
/// | bit | name | aarch64 answer | why |
/// |---|---|---|---|
/// | 0 | SSE2 | HWCAP_ASIMD | the 128-bit integer instructions |
/// | 1 | SSE4.1 | HWCAP_ASIMD | `pblendw` -> `bsl`, `pextrd` -> `umov` |
/// | 2 | SSE4.2 | HWCAP_CRC32 | this bit is only ever asked about `crc32` |
/// | 3 | AES-NI | HWCAP_AES | `aese`/`aesmc`/`aesd`/`aesimc` |
/// | 4 | PCLMULQDQ | HWCAP_PMULL | `pmull` |
/// | 5 | SHA-NI | HWCAP_SHA2 | `sha256su0` (the other two are built) |
/// | 6 | AVX2 | 0 | no 256-bit register here, and none is emitted |
/// | 7 | BMI2 | 0 | not used by any intrinsic of round 82 |
/// | 8 | SSSE3 | HWCAP_ASIMD | `pshufb` -> `tbl`, `palignr` -> `ext` |
///
/// A machine without those bits gets zero for them and every dispatch in
/// `lib/std/crypto` takes its scalar path — which is what round 82 promised
/// and what round 87 already made true for this target.
fn emit_cpu_features(e: &mut Emitter) {
    let base = uniq(e, "hwcap");
    let env = format!("{}_env", base);
    let aux = format!("{}_aux", base);
    let done = format!("{}_done", base);
    e.raw("    // __cpu_features(): AT_HWCAP from the auxiliary vector");
    e.line(&format!("adrp {}, {}", A, AUXV_LABEL));
    e.line(&format!("add {}, {}, :lo12:{}", A, A, AUXV_LABEL));
    e.line(&format!("ldr {}, [{}]", A, A));
    e.line("mov x13, xzr");
    e.line(&format!("cbz {}, {}", A, done));
    e.line(&format!("ldr {}, [{}]", B, A)); // argc
    e.line(&format!("add {}, {}, #8", A, A));
    e.line(&format!("add {}, {}, {}, lsl #3", A, A, B)); // past argv[]
    e.line(&format!("add {}, {}, #8", A, A)); // past its NULL
    e.raw(&format!("{}:", env));
    e.line(&format!("ldr {}, [{}], #8", B, A));
    e.line(&format!("cbnz {}, {}", B, env));
    e.raw(&format!("{}:", aux));
    e.line(&format!("ldr {}, [{}]", B, A));
    e.line(&format!("cbz {}, {}", B, done));
    e.line(&format!("ldr x11, [{}, #8]", A));
    e.line(&format!("add {}, {}, #16", A, A));
    e.line(&format!("cmp {}, #16", B)); // AT_HWCAP
    e.line(&format!("b.ne {}", aux));
    e.line("mov x13, x11");
    e.raw(&format!("{}:", done));
    e.line(&format!("mov {}, xzr", A));
    // (HWCAP bit, bit in the answer)
    for (hw, out) in [(1u32, 0u32), (1, 1), (1, 8), (3, 3), (4, 4), (6, 5), (7, 2)] {
        e.line(&format!("ubfx x14, x13, #{}, #1", hw));
        if out == 0 {
            e.line(&format!("orr {}, {}, x14", A, A));
        } else {
            e.line(&format!("orr {}, {}, x14, lsl #{}", A, A, out));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixteen_octets_split_little_endian() {
        let mut b = [0u8; 16];
        b[0] = 1;
        b[8] = 2;
        assert_eq!(bytes128(&b), (1, 2));
        let idx: [u8; 16] = [0, 13, 10, 7, 4, 1, 14, 11, 8, 5, 2, 15, 12, 9, 6, 3];
        let (lo, hi) = bytes128(&idx);
        assert_eq!(lo, 0x0b_0e_01_04_07_0a_0d_00);
        assert_eq!(hi, 0x03_06_09_0c_0f_02_05_08);
    }

    /// The inverse of ShiftRows really is the inverse. `aeskeygenassist`
    /// stands or falls with it, and it is one line of arithmetic.
    #[test]
    fn the_inverse_shiftrows_table_is_the_inverse() {
        let inv: [u8; 16] = [0, 13, 10, 7, 4, 1, 14, 11, 8, 5, 2, 15, 12, 9, 6, 3];
        for i in 0..16usize {
            // ShiftRows takes octet (i + 4*(i mod 4)) mod 16 to position i.
            let shifted = (i + 4 * (i % 4)) % 16;
            assert_eq!(inv[shifted], i as u8, "position {}", i);
        }
    }

    #[test]
    fn q_and_b_names() {
        assert_eq!(qn("v16"), "q16");
        assert_eq!(b16("v16"), "v16.16b");
    }
}
