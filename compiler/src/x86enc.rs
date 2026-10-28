//! **ROUND KODIERER (the encoder) -- the x86-64 binary encoder.**
//!
//! Up to here Firn's way to the machine ended at a *string*:
//! `codegen_x86.rs` wrote `mov rax, qword ptr [rbp - 8]`, and a foreign
//! program (`as`) then wrote the four octets `48 8B 45 F8`. This file writes
//! them itself.
//!
//! ## What is in here and what is not
//!
//! This is a **pure encoder**: it takes an instruction in the form of
//! [`Inst`] and appends its octets to a buffer. It knows no labels, no
//! sections, no files -- only opcode, REX, ModRM, SIB, displacement and
//! immediate. Exactly for that reason it is also the piece a JIT can use
//! unchanged later: there is no input/output in it.
//!
//! Whoever wants to make object files out of assembler text takes
//! `asm_x86.rs` (parser + labels + relaxation) and `elfobj.rs` (ELF writer).
//! Both only call down to here.
//!
//! ## The scope
//!
//! What is encoded is **not** the x86-64 instruction set but exactly what
//! Firn's code generator emits. That was established by counting 4 632 266
//! instruction lines from the whole tree (all of `tests/`, `examples/`,
//! `bin/firnc1.fi`, `lib/browser`, `lib/js`, `lib/css`, `lib/layout`,
//! `lib/paint`, in three build stages): **86 mnemonics in 232 operand forms.**
//! The list is in `docs/ROUND-KODIERER.md`.
//!
//! ## Where the truth comes from
//!
//! Every encoding rule here is checked against **GNU as**, not written from
//! memory. `as` is the yardstick because the old way uses exactly it: as long
//! as the encoder is not octet-identical, switching would be a change of
//! behaviour. Where `as` picks a *shorter* form although a longer one would
//! also be right, this encoder picks the same shorter one:
//!
//! * `add rax, 1`   -> `48 83 C0 01` (imm8 form), not `48 81 C0 ...`
//! * `add rax, 128` -> `48 05 80 00 00 00` (accumulator short form)
//! * `add rbx, 128` -> `48 81 C3 80 00 00 00` (no short form without rax)
//! * `shl rax, 1`   -> `48 D1 E0` (the 1 sits in the opcode)
//! * `shl rax, 0`   -> `48 C1 E0 00` -- `as` does NOT shorten that away, neither do we
//! * `mov rax, 1`   -> `48 C7 C0 01 00 00 00`, **not** `mov eax, 1`
//! * `mov rax, 2^31`-> `48 B8 ...` (imm64, because imm32 no longer suffices)
//!
//! ## The three traps that silently produce wrong code
//!
//! 1. **rsp/r12 as base** (`rm & 7 == 4`) cannot be represented in ModRM --
//!    the combination means "a SIB follows" there. A SIB with `index=100`
//!    (= none) has to be written, otherwise the instruction addresses
//!    something quite different.
//! 2. **rbp/r13 as base with displacement 0** (`rm & 7 == 5`) cannot be
//!    represented with `mod=00` -- there the combination means "RIP relative"
//!    or "disp32 only". `mod=01` with a displacement of 0 has to be written.
//! 3. **spl/bpl/sil/dil** (register 4-7 as an octet) need a REX prefix, even
//!    an empty one (`0x40`). Without REX the same numbers mean ah/ch/dh/bh --
//!    another register.
//!
//! All three are handled explicitly here and checked in `tools/kodierer/`.

#![allow(dead_code)]

// ===========================================================================
// Registers
// ===========================================================================

/// General purpose registers, 0-15 in the machine's order
/// (rax rcx rdx rbx rsp rbp rsi rdi r8..r15).
pub type Gpr = u8;
/// SSE registers xmm0-xmm15.
pub type Xmm = u8;

pub const RAX: Gpr = 0;
pub const RCX: Gpr = 1;
pub const RDX: Gpr = 2;
pub const RBX: Gpr = 3;
pub const RSP: Gpr = 4;
pub const RBP: Gpr = 5;
pub const RSI: Gpr = 6;
pub const RDI: Gpr = 7;

/// The names in the order of the register numbers, per width.
pub const NAME64: [&str; 16] = [
    "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10",
    "r11", "r12", "r13", "r14", "r15",
];
pub const NAME32: [&str; 16] = [
    "eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi", "r8d", "r9d",
    "r10d", "r11d", "r12d", "r13d", "r14d", "r15d",
];
pub const NAME16: [&str; 16] = [
    "ax", "cx", "dx", "bx", "sp", "bp", "si", "di", "r8w", "r9w", "r10w",
    "r11w", "r12w", "r13w", "r14w", "r15w",
];
/// The REX names of the octet registers. **Without** REX 4-7 are called
/// ah/ch/dh/bh and mean something else -- see `needs_rex8`.
pub const NAME8: [&str; 16] = [
    "al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil", "r8b", "r9b", "r10b",
    "r11b", "r12b", "r13b", "r14b", "r15b",
];
/// The legacy names without REX: only these four are reachable without REX.
pub const NAME8_LEGACY: [(&str, u8); 4] =
    [("ah", 4), ("ch", 5), ("dh", 6), ("bh", 7)];

/// Register number from a name; also returns the width in bits.
pub fn gpr_by_name(n: &str) -> Option<(Gpr, u16)> {
    for (i, s) in NAME64.iter().enumerate() {
        if *s == n {
            return Some((i as Gpr, 64));
        }
    }
    for (i, s) in NAME32.iter().enumerate() {
        if *s == n {
            return Some((i as Gpr, 32));
        }
    }
    for (i, s) in NAME16.iter().enumerate() {
        if *s == n {
            return Some((i as Gpr, 16));
        }
    }
    for (i, s) in NAME8.iter().enumerate() {
        if *s == n {
            return Some((i as Gpr, 8));
        }
    }
    None
}

pub fn xmm_by_name(n: &str) -> Option<Xmm> {
    let r = n.strip_prefix("xmm")?;
    let v: u16 = r.parse().ok()?;
    if v < 16 {
        Some(v as Xmm)
    } else {
        None
    }
}

// ===========================================================================
// Operands
// ===========================================================================

/// A memory address: `[base + index*scale + disp]` or `[rip + disp]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mem {
    pub base: Option<Gpr>,
    pub index: Option<Gpr>,
    /// 1, 2, 4 or 8.
    pub scale: u8,
    pub disp: i64,
    /// RIP relative. Then base/index are empty and `disp` is the addend.
    pub rip: bool,
    /// Width of the ACCESS in bits (8/16/32/64/128). Meaningless for `lea`.
    pub bits: u16,
    /// If the displacement is only known later (a label), this holds its
    /// index in the caller's fix-up list. The encoder then writes a
    /// displacement of full width (disp32) with the value `disp`.
    pub disp_fix: Option<u32>,
    /// Segment prefix: 0 = none, 0x64 = fs, 0x65 = gs. It stands right at
    /// the front, even before a mandatory prefix and before REX.
    pub seg: u8,
}

impl Mem {
    pub fn base_disp(bits: u16, base: Gpr, disp: i64) -> Mem {
        Mem { base: Some(base), index: None, scale: 1, disp, rip: false, bits, disp_fix: None, seg: 0 }
    }
    pub fn rip_rel(bits: u16, disp: i64, fix: Option<u32>) -> Mem {
        Mem { base: None, index: None, scale: 1, disp, rip: true, bits, disp_fix: fix, seg: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opnd {
    /// General purpose register with width.
    Reg(Gpr, u16),
    Xmm(Xmm),
    Mem(Mem),
    /// Immediate. The width follows from the instruction.
    Imm(i64),
    /// Jump/call target. `disp` is the current estimate (relative to the end
    /// of the instruction), `fix` the number in the fix-up list.
    /// `short` says whether the short form (rel8) is to be taken.
    Rel { disp: i64, fix: Option<u32>, short: bool },
    /// Symbol as an immediate (`mov rax, symbol` -> imm32 with relocation).
    SymImm { addend: i64, fix: Option<u32> },
}

// ===========================================================================
// Instructions
// ===========================================================================

/// Condition codes (`tttn` of the Intel reference). The numeric value is the
/// one that lands in the opcode.
pub const CC_O: u8 = 0x0;
pub const CC_NO: u8 = 0x1;
pub const CC_B: u8 = 0x2;
pub const CC_AE: u8 = 0x3;
pub const CC_E: u8 = 0x4;
pub const CC_NE: u8 = 0x5;
pub const CC_BE: u8 = 0x6;
pub const CC_A: u8 = 0x7;
pub const CC_S: u8 = 0x8;
pub const CC_NS: u8 = 0x9;
pub const CC_P: u8 = 0xA;
pub const CC_NP: u8 = 0xB;
pub const CC_L: u8 = 0xC;
pub const CC_GE: u8 = 0xD;
pub const CC_LE: u8 = 0xE;
pub const CC_G: u8 = 0xF;

/// Name of a condition code -> numeric value. All spellings GNU as knows and
/// the code generator uses.
pub fn cc_by_name(n: &str) -> Option<u8> {
    Some(match n {
        "o" => CC_O,
        "no" => CC_NO,
        "b" | "c" | "nae" => CC_B,
        "ae" | "nb" | "nc" => CC_AE,
        "e" | "z" => CC_E,
        "ne" | "nz" => CC_NE,
        "be" | "na" => CC_BE,
        "a" | "nbe" => CC_A,
        "s" => CC_S,
        "ns" => CC_NS,
        "p" | "pe" => CC_P,
        "np" | "po" => CC_NP,
        "l" | "nge" => CC_L,
        "ge" | "nl" => CC_GE,
        "le" | "ng" => CC_LE,
        "g" | "nle" => CC_G,
        _ => return None,
    })
}

/// The group-1 digits (Intel Vol.2 Appendix A: `/digit` at 80/81/83).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alu {
    Add = 0,
    Or = 1,
    Adc = 2,
    Sbb = 3,
    And = 4,
    Sub = 5,
    Xor = 6,
    Cmp = 7,
}

/// The group-2 digits (shift instructions at C0/C1/D0/D1/D2/D3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shift {
    Rol = 0,
    Ror = 1,
    Rcl = 2,
    Rcr = 3,
    Shl = 4,
    Shr = 5,
    Sar = 7,
}

/// The group-3 digits (at F6/F7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Un3 {
    Test = 0,
    Not = 2,
    Neg = 3,
    Mul = 4,
    Imul = 5,
    Div = 6,
    Idiv = 7,
}

/// An instruction from the `0F` space (SSE, SSE2, SSSE3, SSE4, AES, SHA, CRC32).
///
/// The opcode is one to three octets: `0F op`, `0F 38 op` or `0F 3A op`.
/// The mandatory prefix (`66`, `F2`, `F3`) stands **before** REX.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SseOp {
    /// 0 = none, 0x66, 0xF2 or 0xF3.
    pub pfx: u8,
    /// Second opcode octet: 0 = none, otherwise 0x38 or 0x3A.
    pub esc: u8,
    pub op: u8,
    /// needs REX.W (cvtsi2sd/cvttsd2si with 64 bits, pextrq, crc32 r64).
    pub w: bool,
    /// Fixed digit in the ModRM `reg` field instead of a register
    /// (`pslld xmm, imm8` is `66 0F 72 /6 ib`).
    pub digit: Option<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Inst {
    /// `mov` in all forms. The encoder picks the short form itself.
    Mov { dst: Opnd, src: Opnd },
    Lea { dst: Gpr, dst_bits: u16, src: Mem },
    Alu { op: Alu, dst: Opnd, src: Opnd },
    /// `test`, with an opcode branch of its own (84/85 or F6/F7 /0).
    Test { a: Opnd, b: Opnd },
    Shift { op: Shift, dst: Opnd, amount: Opnd },
    /// One-operand instructions at F6/F7.
    Un3 { op: Un3, dst: Opnd },
    /// `inc`/`dec` (FF /0, FF /1).
    IncDec { dec: bool, dst: Opnd },
    /// `imul r, r/m` (0F AF) and `imul r, r/m, imm` (69/6B).
    Imul2 { dst: Gpr, bits: u16, src: Opnd },
    Imul3 { dst: Gpr, bits: u16, src: Opnd, imm: i64 },
    Push(Opnd),
    Pop(Opnd),
    Ret,
    /// `call`: the target is Rel (direct) or Reg/Mem (indirect).
    Call(Opnd),
    /// `jmp`: the target is Rel (direct) or Reg/Mem (indirect).
    Jmp(Opnd),
    Jcc { cc: u8, target: Opnd },
    Setcc { cc: u8, dst: Opnd },
    Cmovcc { cc: u8, dst: Gpr, bits: u16, src: Opnd },
    /// `movzx`/`movsx` -- `src_bits` is 8 or 16.
    MovExt { signed: bool, dst: Gpr, dst_bits: u16, src: Opnd, src_bits: u16 },
    /// `movsxd r64, r/m32` (63 /r).
    Movsxd { dst: Gpr, src: Opnd },
    Syscall,
    Hlt,
    Cld,
    Ud2,
    Nop,
    Leave,
    /// `cdq` (99) or `cqo` (REX.W 99).
    Cdq { wide: bool },
    /// `cwde`/`cbw`.
    Cwde,
    Cbw,
    RepMovsb,
    RepStosb,
    RepString(&'static [u8]),
    /// `lock cmpxchg r/m, r` (F0 0F B1 /r).
    LockCmpxchg { mem: Mem, src: Gpr, bits: u16 },
    /// `lock xadd r/m, r` (F0 0F C1 /r).
    LockXadd { mem: Mem, src: Gpr, bits: u16 },
    /// Everything from the `0F` space, uniformly.
    ///
    /// `reg` is the ModRM `reg` field. For the ordinary instructions it is
    /// the destination (`addsd xmm1, xmm2` -> reg=xmm1, rm=xmm2), for the
    /// **memory forms** and for `pextrd`/`pextrq`, however, the **source**
    /// (`movdqa [rax], xmm1` -> reg=xmm1, rm=[rax]). Whoever mixes that up
    /// produces an instruction that writes in the wrong direction -- which is
    /// why the parser decides it in one place and not the encoder.
    Sse { op: SseOp, reg: u8, rm: Opnd, imm: Option<u8> },
    /// `movq xmm, r64` / `movd xmm, r32` (66 [REX.W] 0F 6E).
    MovToXmm { dst: Xmm, src: Gpr, bits: u16 },
    /// `movq r64, xmm` / `movd r32, xmm` (66 [REX.W] 0F 7E).
    MovFromXmm { dst: Gpr, src: Xmm, bits: u16 },
    /// `cvtsi2sd`/`cvtsi2ss` xmm, r/m (F2|F3 [REX.W] 0F 2A).
    CvtSi2F { double: bool, dst: Xmm, src: Opnd, src_bits: u16 },
    /// `cvttsd2si`/`cvttss2si` r, xmm (F2|F3 [REX.W] 0F 2C).
    CvtF2Si { double: bool, dst: Gpr, dst_bits: u16, src: Xmm },
    /// `bswap` -- the register sits in the opcode.
    Bswap { reg: Gpr, bits: u16 },
    /// An instruction without operands from the `0F` space (`cpuid` = 0F A2,
    /// `rdtsc` = 0F 31, `xgetbv` = 0F 01 D0 is not among them).
    Zero0F(u8),
    /// Raw octets (emergency exit; used by `asm_x86.rs` for `.byte`).
    Raw(Vec<u8>),
}

// ===========================================================================
// Fix-ups (relocation within the buffer)
// ===========================================================================

/// Where in the buffer there is a field that can only be filled later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fixup {
    /// Number the caller passed along in the operand.
    pub id: u32,
    /// Start of the field in the buffer.
    pub at: usize,
    /// Width of the field in octets (1 or 4).
    pub size: u8,
    /// True if the field is PC relative (jumps, RIP relative addresses).
    pub pcrel: bool,
    /// End of the instruction -- the reference point for `pcrel`.
    pub inst_end: usize,
}

/// Target buffer of an encoding run.
#[derive(Default)]
pub struct Buf {
    pub code: Vec<u8>,
    pub fixups: Vec<Fixup>,
}

impl Buf {
    pub fn new() -> Buf {
        Buf { code: Vec::new(), fixups: Vec::new() }
    }
    #[inline]
    fn b(&mut self, v: u8) {
        self.code.push(v);
    }
    #[inline]
    fn d16(&mut self, v: i64) {
        self.code.extend_from_slice(&(v as u16).to_le_bytes());
    }
    #[inline]
    fn d32(&mut self, v: i64) {
        self.code.extend_from_slice(&(v as u32).to_le_bytes());
    }
    #[inline]
    fn d64(&mut self, v: i64) {
        self.code.extend_from_slice(&(v as u64).to_le_bytes());
    }
    #[inline]
    pub fn len(&self) -> usize {
        self.code.len()
    }
    pub fn is_empty(&self) -> bool {
        self.code.is_empty()
    }
}

// ===========================================================================
// Building blocks: REX, ModRM, SIB
// ===========================================================================

/// **Bring an immediate to the operand width.**
///
/// `as` first cuts an immediate to the width of the instruction and THEN
/// decides whether the short form (imm8) suffices. The difference is not
/// theoretical: `and r8d, 0xFFFFFFFC` is `-4` as a 32-bit value and thus
/// fits `83 /4 fc` (four octets). Without the cutting one takes
/// 4294967292 for too big and writes `81 /4 fc ff ff ff` -- seven octets,
/// same effect, another length. And another length moves every following
/// jump.
#[inline]
fn norm_imm(v: i64, bits: u16) -> i64 {
    match bits {
        8 => v as u8 as i8 as i64,
        16 => v as u16 as i16 as i64,
        32 => v as u32 as i32 as i64,
        _ => v,
    }
}

#[inline]
fn fits_i8(v: i64) -> bool {
    (-128..=127).contains(&v)
}
#[inline]
fn fits_i32(v: i64) -> bool {
    (-2147483648..=2147483647).contains(&v)
}

/// Does this octet register need a REX prefix to be meant at all?
/// spl/bpl/sil/dil (4-7) are called ah/ch/dh/bh without REX.
#[inline]
fn needs_rex8(r: Gpr) -> bool {
    (4..=7).contains(&r)
}

/// Collects the REX bits while one instruction is encoded.
#[derive(Default, Clone, Copy)]
struct Rex {
    w: bool,
    r: bool,
    x: bool,
    b: bool,
    /// forces a REX even if all bits are 0 (octet registers 4-7).
    force: bool,
}

impl Rex {
    fn byte(self) -> Option<u8> {
        let v = 0x40
            | ((self.w as u8) << 3)
            | ((self.r as u8) << 2)
            | ((self.x as u8) << 1)
            | (self.b as u8);
        if v != 0x40 || self.force {
            Some(v)
        } else {
            None
        }
    }
}

/// Everything an `r/m` operand contributes to the encoding.
struct Rm {
    /// the three bits in the ModRM byte
    modrm_mod: u8,
    modrm_rm: u8,
    sib: Option<u8>,
    /// Displacement: (value, width in octets 0/1/4)
    disp: (i64, u8),
    /// Number of the fix-up, if the displacement is a label.
    disp_fix: Option<u32>,
    /// Is the displacement PC relative (RIP)?
    pcrel: bool,
    rex_x: bool,
    rex_b: bool,
    seg: u8,
}

fn rm_reg(r: Gpr) -> Rm {
    Rm {
        modrm_mod: 0b11,
        modrm_rm: r & 7,
        sib: None,
        disp: (0, 0),
        disp_fix: None,
        pcrel: false,
        rex_x: false,
        rex_b: r >= 8,
        seg: 0,
    }
}

/// The heart: an address becomes mod, rm, SIB and displacement.
///
/// The two traps 1 and 2 of the header comment are in here.
fn rm_mem(m: &Mem) -> Result<Rm, String> {
    if m.rip {
        // mod=00, rm=101 is RIP relative (and ONLY that -- a pure 32-bit
        // absolute address no longer exists in 64-bit mode like that).
        return Ok(Rm {
            modrm_mod: 0b00,
            modrm_rm: 0b101,
            sib: None,
            disp: (m.disp, 4),
            disp_fix: m.disp_fix,
            pcrel: true,
            rex_x: false,
            rex_b: false,
            seg: m.seg,
        });
    }
    let scale_bits = match m.scale {
        1 => 0u8,
        2 => 1,
        4 => 2,
        8 => 3,
        s => return Err(format!("invalid scale {}", s)),
    };
    if m.index == Some(RSP) {
        // rsp cannot be an index -- the number 100 in the SIB means
        // "no index".
        return Err("rsp cannot be an index register".to_string());
    }

    match (m.base, m.index) {
        // --- neither base nor index: [disp32] ---------------------------
        (None, None) => Ok(Rm {
            modrm_mod: 0b00,
            modrm_rm: 0b100,
            sib: Some((scale_bits << 6) | (0b100 << 3) | 0b101),
            disp: (m.disp, 4),
            disp_fix: m.disp_fix,
            pcrel: false,
            rex_x: false,
            rex_b: false,
            seg: m.seg,
        }),
        // --- index only: [idx*s + disp32] -------------------------------
        (None, Some(ix)) => Ok(Rm {
            modrm_mod: 0b00,
            modrm_rm: 0b100,
            sib: Some((scale_bits << 6) | ((ix & 7) << 3) | 0b101),
            disp: (m.disp, 4),
            disp_fix: m.disp_fix,
            pcrel: false,
            rex_x: ix >= 8,
            rex_b: false,
            seg: m.seg,
        }),
        // --- base, maybe an index ---------------------------------------
        (Some(b), ix) => {
            // TRAP 2: rbp/r13 (rm&7 == 5) cannot be represented with mod=00.
            let must_disp = (b & 7) == 0b101;
            let (mmod, dsz) = if m.disp_fix.is_some() {
                // A label: always full width, otherwise the length could
                // no longer be right later.
                (0b10u8, 4u8)
            } else if m.disp == 0 && !must_disp {
                (0b00, 0)
            } else if fits_i8(m.disp) {
                (0b01, 1)
            } else if fits_i32(m.disp) {
                (0b10, 4)
            } else {
                return Err(format!("displacement {} does not fit 32 bits", m.disp));
            };
            // TRAP 1: rsp/r12 (rm&7 == 4) absolutely need a SIB.
            let need_sib = ix.is_some() || (b & 7) == 0b100;
            if need_sib {
                let ixb = match ix {
                    Some(r) => r & 7,
                    None => 0b100, // "no index"
                };
                Ok(Rm {
                    modrm_mod: mmod,
                    modrm_rm: 0b100,
                    sib: Some((scale_bits << 6) | (ixb << 3) | (b & 7)),
                    disp: (m.disp, dsz),
                    disp_fix: m.disp_fix,
                    pcrel: false,
                    rex_x: ix.map(|r| r >= 8).unwrap_or(false),
                    rex_b: b >= 8,
                    seg: m.seg,
                })
            } else {
                Ok(Rm {
                    modrm_mod: mmod,
                    modrm_rm: b & 7,
                    sib: None,
                    disp: (m.disp, dsz),
                    disp_fix: m.disp_fix,
                    pcrel: false,
                    rex_x: false,
                    rex_b: b >= 8,
                    seg: m.seg,
                })
            }
        }
    }
}

/// Writes a complete instruction with ModRM.
///
/// `pfx66`  - operand size prefix (16-bit instructions)
/// `mpfx`   - mandatory prefix of an SSE instruction (0x66/0xF2/0xF3), comes
///            **before** REX. That is no matter of taste: the order is
///            prescribed, `F2 REX 0F 58` is right, `REX F2 0F 58` is not.
/// `opc`    - one to three opcode octets
/// `reg`    - the `reg` field of the ModRM (register or /digit)
/// `rm`     - the resolved r/m operand
/// `imm`    - immediate (value, width in octets), width 0 = none
#[allow(clippy::too_many_arguments)]
fn emit_modrm(
    buf: &mut Buf,
    mpfx: u8,
    pfx66: bool,
    rex: Rex,
    opc: &[u8],
    reg: u8,
    rm: &Rm,
    imm: (i64, u8),
    imm_fix: Option<u32>,
) {
    if rm.seg != 0 {
        buf.b(rm.seg);
    }
    if mpfx != 0 {
        buf.b(mpfx);
    }
    if pfx66 {
        buf.b(0x66);
    }
    let mut rex = rex;
    rex.r |= reg >= 8;
    rex.x |= rm.rex_x;
    rex.b |= rm.rex_b;
    if let Some(rb) = rex.byte() {
        buf.b(rb);
    }
    for o in opc {
        buf.b(*o);
    }
    buf.b((rm.modrm_mod << 6) | ((reg & 7) << 3) | rm.modrm_rm);
    if let Some(s) = rm.sib {
        buf.b(s);
    }
    // displacement
    let disp_at = buf.len();
    match rm.disp.1 {
        0 => {}
        1 => buf.b(rm.disp.0 as u8),
        4 => buf.d32(rm.disp.0),
        n => unreachable!("displacement width {}", n),
    }
    // immediate
    let imm_at = buf.len();
    match imm.1 {
        0 => {}
        1 => buf.b(imm.0 as u8),
        2 => buf.d16(imm.0),
        4 => buf.d32(imm.0),
        8 => buf.d64(imm.0),
        n => unreachable!("immediate width {}", n),
    }
    let end = buf.len();
    if let Some(id) = rm.disp_fix {
        buf.fixups.push(Fixup { id, at: disp_at, size: rm.disp.1, pcrel: rm.pcrel, inst_end: end });
    }
    if let Some(id) = imm_fix {
        buf.fixups.push(Fixup { id, at: imm_at, size: imm.1, pcrel: false, inst_end: end });
    }
}

/// Builds the r/m part and the basic setting of REX from an operand.
fn rm_of(o: &Opnd) -> Result<(Rm, bool), String> {
    // second return value: REX is forced (octet registers 4-7)
    match o {
        Opnd::Reg(r, bits) => Ok((rm_reg(*r), *bits == 8 && needs_rex8(*r))),
        Opnd::Xmm(r) => Ok((rm_reg(*r), false)),
        Opnd::Mem(m) => Ok((rm_mem(m)?, false)),
        _ => Err(format!("no r/m operand: {:?}", o)),
    }
}

/// Width of an operand in bits.
fn width_of(o: &Opnd) -> Option<u16> {
    match o {
        Opnd::Reg(_, b) => Some(*b),
        Opnd::Mem(m) => Some(m.bits),
        _ => None,
    }
}

/// The three things the width changes in an instruction: REX.W (64),
/// the 0x66 prefix (16) and the opcode change at 8 bits.
struct Wid {
    w: bool,
    p66: bool,
    byte: bool,
}
fn wid(bits: u16) -> Wid {
    Wid { w: bits == 64, p66: bits == 16, byte: bits == 8 }
}

// ===========================================================================
// The encoding function
// ===========================================================================

/// Appends the octets of `i` to `buf`. The only public entry.
pub fn encode(buf: &mut Buf, i: &Inst) -> Result<(), String> {
    match i {
        Inst::Raw(v) => {
            buf.code.extend_from_slice(v);
            Ok(())
        }
        Inst::Mov { dst, src } => enc_mov(buf, dst, src),
        Inst::Lea { dst, dst_bits, src } => {
            let rm = rm_mem(src)?;
            let w = wid(*dst_bits);
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, ..Default::default() },
                &[0x8D],
                *dst,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        Inst::Alu { op, dst, src } => enc_alu(buf, *op, dst, src),
        Inst::Test { a, b } => enc_test(buf, a, b),
        Inst::Shift { op, dst, amount } => enc_shift(buf, *op, dst, amount),
        Inst::Un3 { op, dst } => {
            let bits = width_of(dst).ok_or("unknown width")?;
            let w = wid(bits);
            let (rm, f8) = rm_of(dst)?;
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, force: f8, ..Default::default() },
                &[if w.byte { 0xF6 } else { 0xF7 }],
                *op as u8,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        Inst::IncDec { dec, dst } => {
            let bits = width_of(dst).ok_or("unknown width")?;
            let w = wid(bits);
            let (rm, f8) = rm_of(dst)?;
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, force: f8, ..Default::default() },
                &[if w.byte { 0xFE } else { 0xFF }],
                if *dec { 1 } else { 0 },
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        Inst::Imul2 { dst, bits, src } => {
            let w = wid(*bits);
            let (rm, _) = rm_of(src)?;
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, ..Default::default() },
                &[0x0F, 0xAF],
                *dst,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        Inst::Imul3 { dst, bits, src, imm } => {
            let w = wid(*bits);
            let immn = norm_imm(*imm, *bits);
            let imm = &immn;
            let (rm, _) = rm_of(src)?;
            // 6B = imm8, 69 = imm16/32 — `as` takes the short form where it fits.
            let (opc, isz) = if fits_i8(*imm) {
                (0x6Bu8, 1u8)
            } else if *bits == 16 {
                (0x69, 2)
            } else {
                (0x69, 4)
            };
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, ..Default::default() },
                &[opc],
                *dst,
                &rm,
                (*imm, isz),
                None,
            );
            Ok(())
        }
        Inst::Push(o) => enc_push_pop(buf, o, true),
        Inst::Pop(o) => enc_push_pop(buf, o, false),
        Inst::Ret => {
            buf.b(0xC3);
            Ok(())
        }
        Inst::Call(t) => enc_call_jmp(buf, t, true),
        Inst::Jmp(t) => enc_call_jmp(buf, t, false),
        Inst::Jcc { cc, target } => match target {
            Opnd::Rel { disp, fix, short } => {
                if *short {
                    buf.b(0x70 | cc);
                    let at = buf.len();
                    buf.b(*disp as u8);
                    let end = buf.len();
                    if let Some(id) = fix {
                        buf.fixups.push(Fixup { id: *id, at, size: 1, pcrel: true, inst_end: end });
                    }
                } else {
                    buf.b(0x0F);
                    buf.b(0x80 | cc);
                    let at = buf.len();
                    buf.d32(*disp);
                    let end = buf.len();
                    if let Some(id) = fix {
                        buf.fixups.push(Fixup { id: *id, at, size: 4, pcrel: true, inst_end: end });
                    }
                }
                Ok(())
            }
            _ => Err("jcc needs a relative target".to_string()),
        },
        Inst::Setcc { cc, dst } => {
            let (rm, f8) = rm_of(dst)?;
            emit_modrm(
                buf,
                0,
                false,
                Rex { force: f8, ..Default::default() },
                &[0x0F, 0x90 | cc],
                0,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        Inst::Cmovcc { cc, dst, bits, src } => {
            let w = wid(*bits);
            let (rm, _) = rm_of(src)?;
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, ..Default::default() },
                &[0x0F, 0x40 | cc],
                *dst,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        Inst::MovExt { signed, dst, dst_bits, src, src_bits } => {
            let w = wid(*dst_bits);
            let (rm, f8) = rm_of(src)?;
            let opc = match (*signed, *src_bits) {
                (false, 8) => 0xB6u8,
                (false, 16) => 0xB7,
                (true, 8) => 0xBE,
                (true, 16) => 0xBF,
                _ => return Err(format!("movzx/movsx from {} bits", src_bits)),
            };
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, force: f8, ..Default::default() },
                &[0x0F, opc],
                *dst,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        Inst::Movsxd { dst, src } => {
            let (rm, _) = rm_of(src)?;
            emit_modrm(
                buf,
                0,
                false,
                Rex { w: true, ..Default::default() },
                &[0x63],
                *dst,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        Inst::Syscall => {
            buf.b(0x0F);
            buf.b(0x05);
            Ok(())
        }
        Inst::Hlt => {
            buf.b(0xF4);
            Ok(())
        }
        Inst::Cld => {
            buf.b(0xFC);
            Ok(())
        }
        Inst::Ud2 => {
            buf.b(0x0F);
            buf.b(0x0B);
            Ok(())
        }
        Inst::Nop => {
            buf.b(0x90);
            Ok(())
        }
        Inst::Leave => {
            buf.b(0xC9);
            Ok(())
        }
        Inst::Cdq { wide } => {
            if *wide {
                buf.b(0x48);
            }
            buf.b(0x99);
            Ok(())
        }
        Inst::Cwde => {
            buf.b(0x98);
            Ok(())
        }
        Inst::Cbw => {
            buf.b(0x66);
            buf.b(0x98);
            Ok(())
        }
        Inst::RepMovsb => {
            buf.b(0xF3);
            buf.b(0xA4);
            Ok(())
        }
        Inst::RepStosb => {
            buf.b(0xF3);
            buf.b(0xAA);
            Ok(())
        }
        Inst::RepString(bytes) => {
            for b in bytes.iter() {
                buf.b(*b);
            }
            Ok(())
        }
        Inst::LockCmpxchg { mem, src, bits } => {
            buf.b(0xF0);
            let w = wid(*bits);
            let rm = rm_mem(mem)?;
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, ..Default::default() },
                &[0x0F, if w.byte { 0xB0 } else { 0xB1 }],
                *src,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        Inst::LockXadd { mem, src, bits } => {
            buf.b(0xF0);
            let w = wid(*bits);
            let rm = rm_mem(mem)?;
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, ..Default::default() },
                &[0x0F, if w.byte { 0xC0 } else { 0xC1 }],
                *src,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        Inst::Sse { op, reg, rm: rmop, imm } => {
            let (rm, f8) = rm_of(rmop)?;
            let mut opc: Vec<u8> = Vec::with_capacity(3);
            opc.push(0x0F);
            if op.esc != 0 {
                opc.push(op.esc);
            }
            opc.push(op.op);
            let r = op.digit.unwrap_or(*reg);
            emit_modrm(
                buf,
                op.pfx,
                false,
                Rex { w: op.w, force: f8, ..Default::default() },
                &opc,
                r,
                &rm,
                match imm {
                    Some(v) => (*v as i64, 1),
                    None => (0, 0),
                },
                None,
            );
            Ok(())
        }
        Inst::Bswap { reg, bits } => {
            let rex = Rex { w: *bits == 64, b: *reg >= 8, ..Default::default() };
            if let Some(rb) = rex.byte() {
                buf.b(rb);
            }
            buf.b(0x0F);
            buf.b(0xC8 | (*reg & 7));
            Ok(())
        }
        Inst::Zero0F(op) => {
            buf.b(0x0F);
            buf.b(*op);
            Ok(())
        }
        Inst::MovToXmm { dst, src, bits } => {
            let rm = rm_reg(*src);
            emit_modrm(
                buf,
                0x66,
                false,
                Rex { w: *bits == 64, ..Default::default() },
                &[0x0F, 0x6E],
                *dst,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        Inst::MovFromXmm { dst, src, bits } => {
            let rm = rm_reg(*dst);
            emit_modrm(
                buf,
                0x66,
                false,
                Rex { w: *bits == 64, ..Default::default() },
                &[0x0F, 0x7E],
                *src,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        Inst::CvtSi2F { double, dst, src, src_bits } => {
            let (rm, _) = rm_of(src)?;
            emit_modrm(
                buf,
                if *double { 0xF2 } else { 0xF3 },
                false,
                Rex { w: *src_bits == 64, ..Default::default() },
                &[0x0F, 0x2A],
                *dst,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        Inst::CvtF2Si { double, dst, dst_bits, src } => {
            let rm = rm_reg(*src);
            emit_modrm(
                buf,
                if *double { 0xF2 } else { 0xF3 },
                false,
                Rex { w: *dst_bits == 64, ..Default::default() },
                &[0x0F, 0x2C],
                *dst,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// mov
// ---------------------------------------------------------------------------

fn enc_mov(buf: &mut Buf, dst: &Opnd, src: &Opnd) -> Result<(), String> {
    match (dst, src) {
        // --- register/memory <- register: 88/89 -------------------------
        (_, Opnd::Reg(sr, sb)) => {
            let w = wid(*sb);
            let (rm, f8a) = rm_of(dst)?;
            let f8b = *sb == 8 && needs_rex8(*sr);
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, force: f8a || f8b, ..Default::default() },
                &[if w.byte { 0x88 } else { 0x89 }],
                *sr,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        // --- register <- memory: 8A/8B ----------------------------------
        (Opnd::Reg(dr, db), Opnd::Mem(_)) => {
            let w = wid(*db);
            let (rm, _) = rm_of(src)?;
            let f8 = *db == 8 && needs_rex8(*dr);
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, force: f8, ..Default::default() },
                &[if w.byte { 0x8A } else { 0x8B }],
                *dr,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        // --- register <- immediate --------------------------------------
        (Opnd::Reg(dr, db), Opnd::Imm(v0)) => {
            let w = wid(*db);
            let vv = norm_imm(*v0, *db);
            let v = &vv;
            match *db {
                8 => {
                    // B0+rb ib
                    let mut rex = Rex { force: needs_rex8(*dr), ..Default::default() };
                    rex.b = *dr >= 8;
                    if let Some(rb) = rex.byte() {
                        buf.b(rb);
                    }
                    buf.b(0xB0 | (*dr & 7));
                    buf.b(*v as u8);
                }
                16 => {
                    buf.b(0x66);
                    let rex = Rex { b: *dr >= 8, ..Default::default() };
                    if let Some(rb) = rex.byte() {
                        buf.b(rb);
                    }
                    buf.b(0xB8 | (*dr & 7));
                    buf.d16(*v);
                }
                32 => {
                    let rex = Rex { b: *dr >= 8, ..Default::default() };
                    if let Some(rb) = rex.byte() {
                        buf.b(rb);
                    }
                    buf.b(0xB8 | (*dr & 7));
                    buf.d32(*v);
                }
                64 => {
                    if fits_i32(*v) {
                        // REX.W C7 /0 id — what `as` takes as long as it suffices.
                        let rm = rm_reg(*dr);
                        emit_modrm(
                            buf,
                            0,
                            false,
                            Rex { w: true, ..Default::default() },
                            &[0xC7],
                            0,
                            &rm,
                            (*v, 4),
                            None,
                        );
                    } else {
                        // REX.W B8+rd io
                        let rex = Rex { w: true, b: *dr >= 8, ..Default::default() };
                        if let Some(rb) = rex.byte() {
                            buf.b(rb);
                        }
                        buf.b(0xB8 | (*dr & 7));
                        buf.d64(*v);
                    }
                }
                b => return Err(format!("mov r{}, imm", b)),
            }
            let _ = w;
            Ok(())
        }
        // --- Register <- Symbol (mov rax, sym) ---------------------------
        (Opnd::Reg(dr, db), Opnd::SymImm { addend, fix }) => {
            if *db == 64 {
                let rm = rm_reg(*dr);
                emit_modrm(
                    buf,
                    0,
                    false,
                    Rex { w: true, ..Default::default() },
                    &[0xC7],
                    0,
                    &rm,
                    (*addend, 4),
                    *fix,
                );
            } else if *db == 32 {
                let rex = Rex { b: *dr >= 8, ..Default::default() };
                if let Some(rb) = rex.byte() {
                    buf.b(rb);
                }
                buf.b(0xB8 | (*dr & 7));
                let at = buf.len();
                buf.d32(*addend);
                let end = buf.len();
                if let Some(id) = fix {
                    buf.fixups.push(Fixup { id: *id, at, size: 4, pcrel: false, inst_end: end });
                }
            } else {
                return Err(format!("mov r{}, symbol", db));
            }
            Ok(())
        }
        // --- memory <- immediate: C6/C7 /0 -----------------------------
        (Opnd::Mem(m), Opnd::Imm(v0)) => {
            let w = wid(m.bits);
            let vv = norm_imm(*v0, m.bits);
            let v = &vv;
            let rm = rm_mem(m)?;
            let isz = match m.bits {
                8 => 1u8,
                16 => 2,
                _ => 4,
            };
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, ..Default::default() },
                &[if w.byte { 0xC6 } else { 0xC7 }],
                0,
                &rm,
                (*v, isz),
                None,
            );
            Ok(())
        }
        (Opnd::Mem(m), Opnd::SymImm { addend, fix }) => {
            let w = wid(m.bits);
            let rm = rm_mem(m)?;
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, ..Default::default() },
                &[0xC7],
                0,
                &rm,
                (*addend, 4),
                *fix,
            );
            Ok(())
        }
        _ => Err(format!("mov {:?}, {:?} not encodable", dst, src)),
    }
}

// ---------------------------------------------------------------------------
// Group 1 (add/or/and/sub/xor/cmp)
// ---------------------------------------------------------------------------

fn enc_alu(buf: &mut Buf, op: Alu, dst: &Opnd, src: &Opnd) -> Result<(), String> {
    let d = op as u8;
    match (dst, src) {
        // r/m, r  → digit*8 + 1 (or +0 at 8 bits)
        (_, Opnd::Reg(sr, sb)) => {
            let w = wid(*sb);
            let (rm, f8a) = rm_of(dst)?;
            let f8b = *sb == 8 && needs_rex8(*sr);
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, force: f8a || f8b, ..Default::default() },
                &[d * 8 + if w.byte { 0 } else { 1 }],
                *sr,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        // r, m  → digit*8 + 3 (or +2 at 8 bits)
        (Opnd::Reg(dr, db), Opnd::Mem(_)) => {
            let w = wid(*db);
            let (rm, _) = rm_of(src)?;
            let f8 = *db == 8 && needs_rex8(*dr);
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, force: f8, ..Default::default() },
                &[d * 8 + if w.byte { 2 } else { 3 }],
                *dr,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        // r/m, imm
        (_, Opnd::Imm(v0)) => {
            let bits = width_of(dst).ok_or("unknown width")?;
            let vv = norm_imm(*v0, bits);
            let v = &vv;
            let w = wid(bits);
            // Accumulator short form: only for al/ax/eax/rax and only if
            // the imm8 form would not be shorter already.
            let is_acc = matches!(dst, Opnd::Reg(0, _));
            if w.byte {
                if is_acc {
                    buf.b(d * 8 + 4);
                    buf.b(*v as u8);
                    return Ok(());
                }
                let (rm, f8) = rm_of(dst)?;
                emit_modrm(
                    buf,
                    0,
                    false,
                    Rex { force: f8, ..Default::default() },
                    &[0x80],
                    d,
                    &rm,
                    (*v, 1),
                    None,
                );
                return Ok(());
            }
            if fits_i8(*v) {
                // 83 /digit ib -- the short form always wins, even with rax.
                let (rm, _) = rm_of(dst)?;
                emit_modrm(
                    buf,
                    0,
                    w.p66,
                    Rex { w: w.w, ..Default::default() },
                    &[0x83],
                    d,
                    &rm,
                    (*v, 1),
                    None,
                );
                return Ok(());
            }
            let isz = if bits == 16 { 2u8 } else { 4 };
            if is_acc {
                if w.p66 {
                    buf.b(0x66);
                }
                if let Some(rb) = (Rex { w: w.w, ..Default::default() }).byte() {
                    buf.b(rb);
                }
                buf.b(d * 8 + 5);
                match isz {
                    2 => buf.d16(*v),
                    _ => buf.d32(*v),
                }
                return Ok(());
            }
            let (rm, _) = rm_of(dst)?;
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, ..Default::default() },
                &[0x81],
                d,
                &rm,
                (*v, isz),
                None,
            );
            Ok(())
        }
        (_, Opnd::SymImm { addend, fix }) => {
            let bits = width_of(dst).ok_or("unknown width")?;
            let w = wid(bits);
            let (rm, _) = rm_of(dst)?;
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, ..Default::default() },
                &[0x81],
                d,
                &rm,
                (*addend, 4),
                *fix,
            );
            Ok(())
        }
        _ => Err(format!("alu {:?}, {:?} not encodable", dst, src)),
    }
}

// ---------------------------------------------------------------------------
// test
// ---------------------------------------------------------------------------

fn enc_test(buf: &mut Buf, a: &Opnd, b: &Opnd) -> Result<(), String> {
    match b {
        Opnd::Reg(sr, sb) => {
            let w = wid(*sb);
            let (rm, f8a) = rm_of(a)?;
            let f8b = *sb == 8 && needs_rex8(*sr);
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, force: f8a || f8b, ..Default::default() },
                &[if w.byte { 0x84 } else { 0x85 }],
                *sr,
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        Opnd::Imm(v0) => {
            let bits = width_of(a).ok_or("unknown width")?;
            let vv = norm_imm(*v0, bits);
            let v = &vv;
            let w = wid(bits);
            let is_acc = matches!(a, Opnd::Reg(0, _));
            if is_acc {
                if w.p66 {
                    buf.b(0x66);
                }
                if let Some(rb) = (Rex { w: w.w, ..Default::default() }).byte() {
                    buf.b(rb);
                }
                buf.b(if w.byte { 0xA8 } else { 0xA9 });
                match bits {
                    8 => buf.b(*v as u8),
                    16 => buf.d16(*v),
                    _ => buf.d32(*v),
                }
                return Ok(());
            }
            let (rm, f8) = rm_of(a)?;
            let isz = match bits {
                8 => 1u8,
                16 => 2,
                _ => 4,
            };
            emit_modrm(
                buf,
                0,
                w.p66,
                Rex { w: w.w, force: f8, ..Default::default() },
                &[if w.byte { 0xF6 } else { 0xF7 }],
                0,
                &rm,
                (*v, isz),
                None,
            );
            Ok(())
        }
        _ => Err(format!("test {:?}, {:?} not encodable", a, b)),
    }
}

// ---------------------------------------------------------------------------
// Shift instructions
// ---------------------------------------------------------------------------

fn enc_shift(buf: &mut Buf, op: Shift, dst: &Opnd, amount: &Opnd) -> Result<(), String> {
    let bits = width_of(dst).ok_or("unknown width")?;
    let w = wid(bits);
    let (rm, f8) = rm_of(dst)?;
    let rex = Rex { w: w.w, force: f8, ..Default::default() };
    match amount {
        // shift r/m, cl → D2/D3
        Opnd::Reg(RCX, 8) => {
            emit_modrm(buf, 0, w.p66, rex, &[if w.byte { 0xD2 } else { 0xD3 }], op as u8, &rm, (0, 0), None);
            Ok(())
        }
        Opnd::Imm(1) => {
            // the 1 sits in the opcode -- exactly what `as` takes too
            emit_modrm(buf, 0, w.p66, rex, &[if w.byte { 0xD0 } else { 0xD1 }], op as u8, &rm, (0, 0), None);
            Ok(())
        }
        Opnd::Imm(v) => {
            emit_modrm(buf, 0, w.p66, rex, &[if w.byte { 0xC0 } else { 0xC1 }], op as u8, &rm, (*v, 1), None);
            Ok(())
        }
        _ => Err(format!("shift amount {:?} not encodable", amount)),
    }
}

// ---------------------------------------------------------------------------
// push / pop
// ---------------------------------------------------------------------------

fn enc_push_pop(buf: &mut Buf, o: &Opnd, push: bool) -> Result<(), String> {
    match o {
        Opnd::Reg(r, _) => {
            let rex = Rex { b: *r >= 8, ..Default::default() };
            if let Some(rb) = rex.byte() {
                buf.b(rb);
            }
            buf.b(if push { 0x50 } else { 0x58 } | (*r & 7));
            Ok(())
        }
        Opnd::Imm(v) if push => {
            if fits_i8(*v) {
                buf.b(0x6A);
                buf.b(*v as u8);
            } else {
                buf.b(0x68);
                buf.d32(*v);
            }
            Ok(())
        }
        Opnd::Mem(m) => {
            let rm = rm_mem(m)?;
            emit_modrm(
                buf,
                0,
                false,
                Rex::default(),
                &[0xFF],
                if push { 6 } else { 0 },
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        _ => Err(format!("push/pop {:?} not encodable", o)),
    }
}

// ---------------------------------------------------------------------------
// call / jmp
// ---------------------------------------------------------------------------

fn enc_call_jmp(buf: &mut Buf, t: &Opnd, call: bool) -> Result<(), String> {
    match t {
        Opnd::Rel { disp, fix, short } => {
            if call {
                // `call` has no short form.
                buf.b(0xE8);
                let at = buf.len();
                buf.d32(*disp);
                let end = buf.len();
                if let Some(id) = fix {
                    buf.fixups.push(Fixup { id: *id, at, size: 4, pcrel: true, inst_end: end });
                }
            } else if *short {
                buf.b(0xEB);
                let at = buf.len();
                buf.b(*disp as u8);
                let end = buf.len();
                if let Some(id) = fix {
                    buf.fixups.push(Fixup { id: *id, at, size: 1, pcrel: true, inst_end: end });
                }
            } else {
                buf.b(0xE9);
                let at = buf.len();
                buf.d32(*disp);
                let end = buf.len();
                if let Some(id) = fix {
                    buf.fixups.push(Fixup { id: *id, at, size: 4, pcrel: true, inst_end: end });
                }
            }
            Ok(())
        }
        Opnd::Reg(_, _) | Opnd::Mem(_) => {
            let (rm, _) = rm_of(t)?;
            // FF /2 = call, FF /4 = jmp. In 64-bit mode the operand size
            // is always 64 here -- no REX.W needed.
            emit_modrm(
                buf,
                0,
                false,
                Rex::default(),
                &[0xFF],
                if call { 2 } else { 4 },
                &rm,
                (0, 0),
                None,
            );
            Ok(())
        }
        _ => Err(format!("call/jmp {:?} not encodable", t)),
    }
}

// ===========================================================================
// Length without writing -- for the jump relaxation
// ===========================================================================

/// How many octets would `i` occupy? Encodes into a throw-away buffer; that
/// is simpler and less error-prone than a second length table that would
/// have to be kept up to date. (The cost is irrelevant: the relaxation only
/// runs over jumps, and those are short.)
pub fn encoded_len(i: &Inst) -> Result<usize, String> {
    let mut b = Buf::new();
    encode(&mut b, i)?;
    Ok(b.len())
}

// ===========================================================================
// Self-checks
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(i: Inst) -> Vec<u8> {
        let mut b = Buf::new();
        encode(&mut b, &i).expect("encodable");
        b.code
    }
    fn hex(v: &[u8]) -> String {
        v.iter().map(|x| format!("{:02x}", x)).collect::<Vec<_>>().join(" ")
    }

    /// The instruction from the header comment of the study.
    #[test]
    fn mov_rax_from_rbp_minus_eight() {
        let i = Inst::Mov {
            dst: Opnd::Reg(RAX, 64),
            src: Opnd::Mem(Mem::base_disp(64, RBP, -8)),
        };
        assert_eq!(hex(&enc(i)), "48 8b 45 f8");
    }

    #[test]
    fn trap_one_rsp_needs_sib() {
        // [rsp] cannot be represented without a SIB.
        let i = Inst::Mov {
            dst: Opnd::Reg(RAX, 64),
            src: Opnd::Mem(Mem { base: Some(RSP), index: None, scale: 1, disp: 0, rip: false, bits: 64, disp_fix: None, seg: 0 }),
        };
        assert_eq!(hex(&enc(i)), "48 8b 04 24");
        // r12 the same (r12 & 7 == 4)
        let i = Inst::Mov {
            dst: Opnd::Reg(RAX, 64),
            src: Opnd::Mem(Mem { base: Some(12), index: None, scale: 1, disp: 0, rip: false, bits: 64, disp_fix: None, seg: 0 }),
        };
        assert_eq!(hex(&enc(i)), "49 8b 04 24");
    }

    #[test]
    fn trap_two_rbp_with_zero_needs_disp8() {
        let i = Inst::Mov {
            dst: Opnd::Reg(RAX, 64),
            src: Opnd::Mem(Mem::base_disp(64, RBP, 0)),
        };
        assert_eq!(hex(&enc(i)), "48 8b 45 00");
        // r13 the same (r13 & 7 == 5)
        let i = Inst::Mov {
            dst: Opnd::Reg(RAX, 64),
            src: Opnd::Mem(Mem::base_disp(64, 13, 0)),
        };
        assert_eq!(hex(&enc(i)), "49 8b 45 00");
    }

    #[test]
    fn trap_three_spl_needs_empty_rex() {
        // mov spl, al  ->  40 88 c4  (without the 40 it would be `mov ah, al`)
        let i = Inst::Mov { dst: Opnd::Reg(4, 8), src: Opnd::Reg(0, 8) };
        assert_eq!(hex(&enc(i)), "40 88 c4");
        // sete spl → 40 0f 94 c4
        let i = Inst::Setcc { cc: CC_E, dst: Opnd::Reg(4, 8) };
        assert_eq!(hex(&enc(i)), "40 0f 94 c4");
    }

    #[test]
    fn sofortwert_kurzformen_wie_bei_as() {
        assert_eq!(hex(&enc(Inst::Alu { op: Alu::Add, dst: Opnd::Reg(RAX, 64), src: Opnd::Imm(1) })), "48 83 c0 01");
        assert_eq!(hex(&enc(Inst::Alu { op: Alu::Add, dst: Opnd::Reg(RAX, 64), src: Opnd::Imm(128) })), "48 05 80 00 00 00");
        assert_eq!(hex(&enc(Inst::Alu { op: Alu::Add, dst: Opnd::Reg(RBX, 64), src: Opnd::Imm(128) })), "48 81 c3 80 00 00 00");
        assert_eq!(hex(&enc(Inst::Mov { dst: Opnd::Reg(RAX, 64), src: Opnd::Imm(1) })), "48 c7 c0 01 00 00 00");
        assert_eq!(hex(&enc(Inst::Mov { dst: Opnd::Reg(RAX, 64), src: Opnd::Imm(2147483648) })), "48 b8 00 00 00 80 00 00 00 00");
        assert_eq!(hex(&enc(Inst::Shift { op: Shift::Shl, dst: Opnd::Reg(RAX, 64), amount: Opnd::Imm(1) })), "48 d1 e0");
        assert_eq!(hex(&enc(Inst::Shift { op: Shift::Shl, dst: Opnd::Reg(RAX, 64), amount: Opnd::Imm(0) })), "48 c1 e0 00");
    }

    #[test]
    fn sse_prefix_stands_before_rex() {
        // addsd xmm8, xmm12 → f2 45 0f 58 c4
        let i = Inst::Sse { op: SseOp { pfx: 0xF2, esc: 0, op: 0x58, w: false, digit: None }, reg: 8, rm: Opnd::Xmm(12), imm: None };
        assert_eq!(hex(&enc(i)), "f2 45 0f 58 c4");
        // movq xmm0, rax → 66 48 0f 6e c0
        let i = Inst::MovToXmm { dst: 0, src: RAX, bits: 64 };
        assert_eq!(hex(&enc(i)), "66 48 0f 6e c0");
    }

    #[test]
    fn index_without_base() {
        // lea rax, [r11*4] -> SIB with base=101 and disp32
        let m = Mem { base: None, index: Some(11), scale: 4, disp: 0, rip: false, bits: 64, disp_fix: None, seg: 0 };
        let i = Inst::Lea { dst: RAX, dst_bits: 64, src: m };
        assert_eq!(hex(&enc(i)), "4a 8d 04 9d 00 00 00 00");
    }
}
