//! **ROUND KODIERER (the encoder) -- the ARM64 binary encoder.**
//!
//! ARM64 is the friendlier half of the task: **every instruction is exactly
//! four octets long**, there are no prefixes, no ModRM, no SIB and no
//! relaxation -- the length is fixed before the label. The difficulty sits
//! elsewhere instead: in the **immediates**.
//!
//! Three of them are really nasty and therefore stand up here:
//!
//! 1. **Logical immediates** (`and x0, x0, #imm`) are not stored as a number
//!    but as a *pattern*: `N:immr:imms` describes a run of ones of length
//!    `s+1`, rotated by `r`, repeated over a period of 2, 4, 8, 16, 32 or 64
//!    bits. Only numbers that can arise this way are encodable at all --
//!    `#1` yes, `#3` yes, `#5` **no**. [`logical_imm`] works it out and says
//!    honestly `None` when it cannot be done.
//! 2. **Shifted immediates** (`add x0, x0, #imm`): 12 bits, optionally
//!    shifted left by 12 places. So 0...4095 or 4096...16773120 in steps of
//!    4096 -- nothing in between.
//! 3. **Displacements of loads** are **scaled**: `ldr x0, [x1, #16]` stores
//!    the 2, not the 16. If the displacement does not fit the grid (negative
//!    or unaligned), the *unscaled* form `ldur` has to be used -- with
//!    another opcode. `as` does that silently; whoever does not copy it
//!    produces a load that reads the wrong memory location.
//!
//! As with the x86 encoder, every rule is checked against
//! `aarch64-linux-gnu-as`, not written from memory.

#![allow(dead_code)]

// ===========================================================================
// Registers
// ===========================================================================

/// Register number 0-31. 31 is `xzr`/`wzr` **or** `sp` depending on the
/// instruction -- the instruction decides, not the number.
pub type Reg = u8;
pub const ZR: Reg = 31;
pub const SP_R: Reg = 31;
pub const LR: Reg = 30;

/// `x`/`w` register from a name; returns (number, is_64bit, is_sp).
pub fn gpr_by_name(n: &str) -> Option<(Reg, bool, bool)> {
    match n {
        "sp" => return Some((31, true, true)),
        "wsp" => return Some((31, false, true)),
        "xzr" => return Some((31, true, false)),
        "wzr" => return Some((31, false, false)),
        "lr" => return Some((30, true, false)),
        _ => {}
    }
    let (w64, rest) = match n.as_bytes().first()? {
        b'x' => (true, &n[1..]),
        b'w' => (false, &n[1..]),
        _ => return None,
    };
    let v: u16 = rest.parse().ok()?;
    if v < 31 {
        Some((v as Reg, w64, false))
    } else {
        None
    }
}

/// Floating point / vector register: `d0`, `s0`, `q0`, `v0`, `h0`, `b0`.
/// Returns (number, width tag).
pub fn fpr_by_name(n: &str) -> Option<(Reg, char)> {
    let c = *n.as_bytes().first()? as char;
    if !matches!(c, 'd' | 's' | 'q' | 'v' | 'h' | 'b') {
        return None;
    }
    let v: u16 = n[1..].parse().ok()?;
    if v < 32 {
        Some((v as Reg, c))
    } else {
        None
    }
}

/// Condition codes.
pub fn cond_by_name(n: &str) -> Option<u8> {
    Some(match n {
        "eq" => 0,
        "ne" => 1,
        "cs" | "hs" => 2,
        "cc" | "lo" => 3,
        "mi" => 4,
        "pl" => 5,
        "vs" => 6,
        "vc" => 7,
        "hi" => 8,
        "ls" => 9,
        "ge" => 10,
        "lt" => 11,
        "gt" => 12,
        "le" => 13,
        "al" => 14,
        "nv" => 15,
        _ => return None,
    })
}

// ===========================================================================
// Immediate encodings
// ===========================================================================

/// **The logical immediate.** A number becomes `(N, immr, imms)`, or `None`
/// if the number is no permissible pattern.
///
/// The method (ARM ARM, "DecodeBitMasks" backwards):
///
/// 1. The number must consist of a period `e` in {2,4,8,16,32,64} that
///    repeats over the whole register width.
/// 2. Within the period the ones must be **contiguous** (cyclically).
///    Neither 0 nor all-ones is permissible.
/// 3. `imms` carries the length (minus 1) and the period, `immr` the
///    rotation.
///
/// Example: `0xFFFF_FFFF_FFFF_FFFC` = 62 ones, rotated by 2 -> encodable.
/// `0x5` (101 binary) is not -- the ones are not contiguous.
pub fn logical_imm(value: u64, is64: bool) -> Option<(u32, u32, u32)> {
    let width = if is64 { 64u32 } else { 32 };
    let mut v = value;
    if !is64 {
        // for 32 bits the upper half must be a copy
        if value >> 32 != 0 && value >> 32 != 0xFFFF_FFFF {
            // below we work with the 32-bit number, doubled
        }
        v = (value & 0xFFFF_FFFF) | ((value & 0xFFFF_FFFF) << 32);
    }
    if v == 0 || v == u64::MAX {
        return None;
    }
    // 1. find the smallest period
    let mut e = 2u32;
    while e <= 64 {
        let mask = if e == 64 { u64::MAX } else { (1u64 << e) - 1 };
        let part = v & mask;
        let mut ok = true;
        let mut k = e;
        while k < 64 {
            if (v >> k) & mask != part {
                ok = false;
                break;
            }
            k += e;
        }
        if ok {
            break;
        }
        e *= 2;
    }
    if e > 64 {
        return None;
    }
    if !is64 && e > 32 {
        return None;
    }
    let mask = if e == 64 { u64::MAX } else { (1u64 << e) - 1 };
    let part = v & mask;
    if part == 0 || part == mask {
        return None;
    }
    // 2. cyclically contiguous ones?
    //    We rotate the period so that it ends with a 0 and begins with
    //    ones; then part == (2^ones - 1) must hold.
    let ones = part.count_ones();
    // Search the rotation: the place where a 0->1 transition happens.
    let mut rot = None;
    for r in 0..e {
        let rotated = rotate_right(part, r, e);
        if rotated == (if ones == e { mask } else { (1u64 << ones) - 1 }) {
            rot = Some(r);
            break;
        }
    }
    // `immr` is the rotation that turns the base pattern into the VALUE --
    // above we searched the opposite direction. So mirror.
    let r = (e - rot?) % e;
    // 3. assemble
    // imms: the upper bits encode the period as a mask
    //   e=64 -> 0b000000 (with N=1), e=32 -> 0b011111 & ..., in general:
    //   imms = (!(2*e-1)) & 0x3F | (ones-1)
    let n = if e == 64 { 1u32 } else { 0 };
    let imms = if e == 64 {
        ones - 1
    } else {
        ((!((2 * e) - 1)) & 0x3F) | (ones - 1)
    };
    let immr = r % e;
    let _ = width;
    Some((n, immr, imms))
}

fn rotate_right(v: u64, r: u32, e: u32) -> u64 {
    if e == 64 {
        return v.rotate_right(r);
    }
    let mask = (1u64 << e) - 1;
    let r = r % e;
    ((v >> r) | (v << (e - r))) & mask
}

/// The shifted immediate of `add`/`sub`: 12 bits, optionally `lsl #12`.
/// Returns `(imm12, sh)`.
pub fn addsub_imm(v: u64) -> Option<(u32, u32)> {
    if v <= 0xFFF {
        Some((v as u32, 0))
    } else if v & 0xFFF == 0 && (v >> 12) <= 0xFFF {
        Some(((v >> 12) as u32, 1))
    } else {
        None
    }
}

// ===========================================================================
// Fix-ups
// ===========================================================================

/// Which kind of relative field a fix-up fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixKind {
    /// `b`/`bl`: 26 bits, in words (×4).
    Br26,
    /// `b.cond`/`cbz`/`cbnz`: 19 bits, in words.
    Br19,
    /// `tbz`/`tbnz`: 14 bits, in words.
    Br14,
    /// `adrp`: 21 bits of page offset.
    Adrp,
    /// `add …, #:lo12:sym`: the lower 12 bits.
    Lo12,
    /// `ldr …, [x, #:lo12:sym]`: the lower 12 bits, scaled.
    Lo12Scaled(u32),
}

#[derive(Clone, Copy, Debug)]
pub struct Fixup {
    pub id: u32,
    /// Offset of the instruction word in the buffer.
    pub at: usize,
    pub kind: FixKind,
}

#[derive(Default)]
pub struct Buf {
    pub code: Vec<u8>,
    pub fixups: Vec<Fixup>,
}

impl Buf {
    pub fn new() -> Buf {
        Buf { code: Vec::new(), fixups: Vec::new() }
    }
    pub fn word(&mut self, w: u32) {
        self.code.extend_from_slice(&w.to_le_bytes());
    }
    pub fn len(&self) -> usize {
        self.code.len()
    }
    pub fn is_empty(&self) -> bool {
        self.code.is_empty()
    }
    /// Sets a field in a word that has already been written.
    pub fn patch(&mut self, at: usize, mask: u32, value: u32) {
        let mut w = u32::from_le_bytes([
            self.code[at],
            self.code[at + 1],
            self.code[at + 2],
            self.code[at + 3],
        ]);
        w = (w & !mask) | (value & mask);
        self.code[at..at + 4].copy_from_slice(&w.to_le_bytes());
    }
}

/// Enters a resolved value into the field of a fix-up.
///
/// `rel` is the distance in **octets** for the jumps, the page distance for
/// `adrp`, the absolute value for `lo12`.
pub fn apply_fix(buf: &mut Buf, at: usize, kind: FixKind, rel: i64) -> Result<(), String> {
    match kind {
        FixKind::Br26 => {
            if rel % 4 != 0 {
                return Err("jump target not word aligned".into());
            }
            let w = rel / 4;
            if !(-(1 << 25)..(1 << 25)).contains(&w) {
                return Err(format!("jump distance {} too large for b/bl", rel));
            }
            buf.patch(at, 0x03FF_FFFF, (w as u32) & 0x03FF_FFFF);
        }
        FixKind::Br19 => {
            if rel % 4 != 0 {
                return Err("jump target not word aligned".into());
            }
            let w = rel / 4;
            if !(-(1 << 18)..(1 << 18)).contains(&w) {
                return Err(format!("jump distance {} too large for b.cond/cbz", rel));
            }
            buf.patch(at, 0x00FF_FFE0, ((w as u32) & 0x7FFFF) << 5);
        }
        FixKind::Br14 => {
            if rel % 4 != 0 {
                return Err("jump target not word aligned".into());
            }
            let w = rel / 4;
            if !(-(1 << 13)..(1 << 13)).contains(&w) {
                return Err(format!("jump distance {} too large for tbz/tbnz", rel));
            }
            buf.patch(at, 0x0007_FFE0, ((w as u32) & 0x3FFF) << 5);
        }
        FixKind::Adrp => {
            let p = rel >> 12;
            if !(-(1 << 20)..(1 << 20)).contains(&p) {
                return Err("adrp distance too large".into());
            }
            let v = p as u32;
            buf.patch(at, 0x60FF_FFE0, ((v & 3) << 29) | (((v >> 2) & 0x7FFFF) << 5));
        }
        FixKind::Lo12 => {
            buf.patch(at, 0x003F_FC00, ((rel as u32) & 0xFFF) << 10);
        }
        FixKind::Lo12Scaled(sc) => {
            let v = (rel as u32) & 0xFFF;
            if sc > 0 && v % sc != 0 {
                return Err("lo12 not suitably aligned".into());
            }
            let v = if sc > 0 { v / sc } else { v };
            buf.patch(at, 0x003F_FC00, (v & 0xFFF) << 10);
        }
    }
    Ok(())
}
