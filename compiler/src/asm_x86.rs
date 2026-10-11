//! **ROUND KODIERER (the encoder) -- from assembler text to an object file, without `as`.**
//!
//! `x86enc.rs` can turn single instructions into octets. This file makes an
//! assembler out of it: it parses exactly the text `codegen_x86.rs` writes,
//! manages sections, labels and symbols, resolves jumps, decides between the
//! short and the long form (relaxation) and collects the relocations the
//! linker needs.
//!
//! ## Why through the text and not directly from FIR?
//!
//! Because that gives a **counter check with millions of real instructions**.
//! The old way sends the same text through `as`; whoever lets both ways loose
//! on the same input can compare octet by octet. An encoder that is fed
//! directly from FIR would not have this test bench -- one would have to
//! believe it. The detour through the text costs little (the parser is small
//! because the dialect is small) and buys certainty.
//!
//! Once the certainty is there, `codegen_x86.rs` can later produce
//! [`x86enc::Inst`] directly instead of text without a rebuild -- the encoder
//! underneath stays the same.
//!
//! ## What is copied from `as` here -- and what is not
//!
//! Copied, because otherwise the octets differ:
//!
//! * **Relaxation.** `jmp`/`jcc` to a label in the same section are encoded
//!   short (rel8) if the distance suffices, otherwise long (rel32). `as`
//!   starts short and only lets jumps grow; that is a fixed point, and it is
//!   computed the same way here.
//! * **The asymmetry between `jmp` and `call`.** `jmp b` to a *global* `b` in
//!   the same section is resolved directly by `as` (`eb 0d`). `call b` to the
//!   same `b` is **not** -- there `R_X86_64_PLT32` always results. Reason: the
//!   jump goes through the relaxation, the call through the relocation, and a
//!   global symbol may be replaced when linking. Whoever overlooks that
//!   produces code that is only noticed at dynamic linking.
//! * **Local versus global.** A *local* symbol (`.L...`) in the same section is
//!   resolved directly everywhere -- also for `call` and for
//!   `lea ...[rip + .L...]`. A global one never (except for the jump).
//! * **Relocation against section symbols.** If a relocation points to a
//!   *local* label in another section, `as` writes it against the **section
//!   symbol** with the label's offset as the addend -- not against the label
//!   itself.
//!
//! Not copied (deliberately, see `docs/ROUND-KODIERER.md`):
//!
//! * **DWARF.** `.loc`/`.file` are read and discarded. The internal way thus
//!   produces an object file without debug information -- that corresponds to
//!   a build without `-g` and is the reason why the old way stays the default.

#![allow(dead_code)]

use crate::x86enc::{self as E, Buf, Inst, Mem, Opnd};
use std::collections::HashMap;

// ===========================================================================
// Result
// ===========================================================================

pub const SEC_TEXT: usize = 0;
pub const SEC_DATA: usize = 1;
pub const SEC_RODATA: usize = 2;
pub const SEC_NOTE: usize = 3;
/// `.bss` -- content zero, but occupies room in the image (ELF: `NOBITS`).
pub const SEC_BSS: usize = 4;
pub const N_SEC: usize = 5;

/// Relocation kinds that occur here (values per the System V ABI, x86-64).
pub const R_X86_64_64: u32 = 1;
pub const R_X86_64_PC32: u32 = 2;
pub const R_X86_64_32: u32 = 10;
pub const R_X86_64_32S: u32 = 11;
pub const R_X86_64_PLT32: u32 = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reloc {
    pub offset: u64,
    /// Number in `Assembled::symbols`.
    pub sym: usize,
    pub kind: u32,
    pub addend: i64,
}

#[derive(Clone, Debug)]
pub struct Section {
    pub name: &'static str,
    pub bytes: Vec<u8>,
    pub relocs: Vec<Reloc>,
    pub align: u64,
}

#[derive(Clone, Debug)]
pub struct Sym {
    pub name: String,
    /// `None` = not defined in this file (extern).
    pub section: Option<usize>,
    pub value: u64,
    pub global: bool,
    /// Section symbol (`.text`, `.rodata`, …).
    pub is_section: bool,
}

pub struct Assembled {
    pub sections: Vec<Section>,
    pub symbols: Vec<Sym>,
}

// ===========================================================================
// Intermediate representation: pieces
// ===========================================================================

/// Where a jump/an address points.
#[derive(Clone, Debug)]
struct Target {
    name: String,
}

#[derive(Clone, Debug)]
enum Piece {
    /// Fully encoded, the length is fixed. `relocs` are relocations relative
    /// to the start of the piece.
    Fixed { bytes: Vec<u8>, relocs: Vec<PendReloc> },
    /// A jump whose length depends on the distance.
    Branch { kind: BrKind, target: Target, long: bool },
    /// Alignment: pad up to a multiple of `n`.
    Align { n: u64 },
    /// A label at this place.
    Label { name: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BrKind {
    Jmp,
    Jcc(u8),
    /// `call` -- never short, but resolvable against local labels.
    Call,
}

/// A relocation whose symbol is only known at the end.
#[derive(Clone, Debug)]
struct PendReloc {
    /// Offset within the piece.
    at: usize,
    name: String,
    kind: u32,
    addend: i64,
}

// ===========================================================================
// The parser
// ===========================================================================

pub struct Asm {
    pieces: [Vec<Piece>; N_SEC],
    cur: usize,
    globals: Vec<String>,
    aligns: [u64; N_SEC],
    /// Counter for the numeric labels (`1:` ... `jnz 1f`).
    numeric: HashMap<u32, u32>,
    line_no: usize,
}

impl Default for Asm {
    fn default() -> Self {
        Self::new()
    }
}

impl Asm {
    pub fn new() -> Asm {
        Asm {
            pieces: [Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            cur: SEC_TEXT,
            globals: Vec::new(),
            aligns: [1, 1, 1, 1, 1],
            numeric: HashMap::new(),
            line_no: 0,
        }
    }

    fn push(&mut self, p: Piece) {
        self.pieces[self.cur].push(p);
    }

    fn err(&self, m: impl AsRef<str>) -> String {
        format!("line {}: {}", self.line_no, m.as_ref())
    }

    /// Takes in the whole assembler text.
    pub fn feed(&mut self, text: &str) -> Result<(), String> {
        for (n, raw) in text.lines().enumerate() {
            self.line_no = n + 1;
            self.line(raw)?;
        }
        Ok(())
    }

    fn line(&mut self, raw: &str) -> Result<(), String> {
        // Cut off a comment -- but not inside a string.
        let mut s = raw;
        let mut in_str = false;
        let mut esc = false;
        for (i, c) in raw.char_indices() {
            if esc {
                esc = false;
                continue;
            }
            match c {
                '\\' if in_str => esc = true,
                '"' => in_str = !in_str,
                '#' if !in_str => {
                    s = &raw[..i];
                    break;
                }
                _ => {}
            }
        }
        let s = s.trim();
        if s.is_empty() {
            return Ok(());
        }

        // A label? (`name:` at the start; an instruction may still follow)
        let mut s = s;
        loop {
            let bytes = s.as_bytes();
            let mut i = 0;
            // a numeric label `1:`
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i > 0 && i < bytes.len() && bytes[i] == b':' {
                let n: u32 = s[..i].parse().map_err(|_| self.err("label number"))?;
                let c = self.numeric.entry(n).or_insert(0);
                *c += 1;
                let name = format!("\u{1}L{}\u{2}{}", n, *c);
                self.push(Piece::Label { name });
                s = s[i + 1..].trim();
                if s.is_empty() {
                    return Ok(());
                }
                continue;
            }
            // an ordinary label
            i = 0;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric()
                    || matches!(bytes[i], b'_' | b'.' | b'$' | b'#'))
            {
                i += 1;
            }
            if i > 0 && i < bytes.len() && bytes[i] == b':' && !s[..i].starts_with(".intel") {
                let name = s[..i].to_string();
                self.push(Piece::Label { name });
                s = s[i + 1..].trim();
                if s.is_empty() {
                    return Ok(());
                }
                continue;
            }
            break;
        }

        if s.starts_with('.') && !s.contains(':') {
            return self.directive(s);
        }
        // Directives that can contain a colon after all
        // (`.section .note.GNU-stack,"",@progbits`) are caught above,
        // because they begin with `.` and are no labels.
        if s.starts_with('.') {
            return self.directive(s);
        }
        self.instruction(s)
    }

    // -----------------------------------------------------------------
    // Directives
    // -----------------------------------------------------------------

    fn directive(&mut self, s: &str) -> Result<(), String> {
        let (d, rest) = match s.find(char::is_whitespace) {
            Some(i) => (&s[..i], s[i..].trim()),
            None => (s, ""),
        };
        match d {
            ".intel_syntax" | ".file" | ".loc" | ".ident" | ".cfi_startproc"
            | ".cfi_endproc" | ".type" | ".size" => Ok(()),
            ".text" => {
                self.cur = SEC_TEXT;
                Ok(())
            }
            ".data" => {
                self.cur = SEC_DATA;
                Ok(())
            }
            ".section" => {
                let name = rest.split(',').next().unwrap_or("").trim();
                self.cur = match name {
                    ".text" => SEC_TEXT,
                    ".data" => SEC_DATA,
                    ".rodata" => SEC_RODATA,
                    ".note.GNU-stack" => SEC_NOTE,
                    ".bss" => SEC_BSS,
                    other => return Err(self.err(format!("unknown section {}", other))),
                };
                Ok(())
            }
            ".globl" | ".global" => {
                self.globals.push(rest.to_string());
                Ok(())
            }
            ".align" | ".balign" => {
                let n: u64 = rest.parse().map_err(|_| self.err("alignment number"))?;
                if n > self.aligns[self.cur] {
                    self.aligns[self.cur] = n;
                }
                self.push(Piece::Align { n });
                Ok(())
            }
            ".p2align" => {
                let e: u32 = rest.split(',').next().unwrap_or("0").trim().parse()
                    .map_err(|_| self.err("alignment number"))?;
                let n = 1u64 << e;
                if n > self.aligns[self.cur] {
                    self.aligns[self.cur] = n;
                }
                self.push(Piece::Align { n });
                Ok(())
            }
            ".zero" | ".space" | ".skip" => {
                let n: usize = rest.split(',').next().unwrap_or("0").trim().parse()
                    .map_err(|_| self.err("size"))?;
                self.push(Piece::Fixed { bytes: vec![0u8; n], relocs: Vec::new() });
                Ok(())
            }
            ".byte" | ".word" | ".short" | ".long" | ".int" | ".quad" => {
                let w = match d {
                    ".byte" => 1usize,
                    ".word" | ".short" => 2,
                    ".long" | ".int" => 4,
                    _ => 8,
                };
                for item in split_commas(rest) {
                    let item = item.trim();
                    if item.is_empty() {
                        continue;
                    }
                    match parse_int(item) {
                        Some(v) => {
                            let b = (v as u64).to_le_bytes();
                            self.push(Piece::Fixed {
                                bytes: b[..w].to_vec(),
                                relocs: Vec::new(),
                            });
                        }
                        None => {
                            // a symbol -- relocation of full width
                            let (name, add) = split_addend(item);
                            let kind = match w {
                                8 => R_X86_64_64,
                                4 => R_X86_64_32,
                                _ => return Err(self.err("symbol in a field that is too narrow")),
                            };
                            self.push(Piece::Fixed {
                                bytes: vec![0u8; w],
                                relocs: vec![PendReloc { at: 0, name, kind, addend: add }],
                            });
                        }
                    }
                }
                Ok(())
            }
            ".ascii" | ".string" | ".asciz" => {
                let mut out = Vec::new();
                for item in split_commas_strings(rest) {
                    out.extend_from_slice(&unescape(&item)?);
                    if d != ".ascii" {
                        out.push(0);
                    }
                }
                self.push(Piece::Fixed { bytes: out, relocs: Vec::new() });
                Ok(())
            }
            other => Err(self.err(format!("unknown directive {}", other))),
        }
    }

    // -----------------------------------------------------------------
    // Instructions
    // -----------------------------------------------------------------

    fn instruction(&mut self, s: &str) -> Result<(), String> {
        let (mut m, mut rest) = match s.find(char::is_whitespace) {
            Some(i) => (s[..i].to_ascii_lowercase(), s[i..].trim()),
            None => (s.to_ascii_lowercase(), ""),
        };
        // A prefix (`lock`, `rep`) belongs to the instruction and not to the
        // operands: `lock cmpxchg qword ptr [rcx], rdx` has TWO operands,
        // not `cmpxchg qword ptr [rcx]` and `rdx`.
        if matches!(m.as_str(), "lock" | "rep" | "repe" | "repne" | "repz" | "repnz") {
            let (m2, rest2) = match rest.find(char::is_whitespace) {
                Some(i) => (rest[..i].to_ascii_lowercase(), rest[i..].trim()),
                None => (rest.to_ascii_lowercase(), ""),
            };
            if m2.is_empty() {
                return Err(self.err("prefix without instruction"));
            }
            m = format!("{} {}", m, m2);
            rest = rest2;
        }
        let ops: Vec<String> = split_operands(rest);

        // --- jumps and calls first: they become branch pieces ---
        match m.as_str() {
            "jmp" | "call" => {
                if ops.len() == 1 && is_symbolish(&ops[0]) {
                    let t = self.resolve_numeric(&ops[0]);
                    let kind = if m == "jmp" { BrKind::Jmp } else { BrKind::Call };
                    self.push(Piece::Branch { kind, target: Target { name: t }, long: m == "call" });
                    return Ok(());
                }
            }
            _ => {
                if let Some(cc) = m.strip_prefix('j').and_then(E::cc_by_name) {
                    if ops.len() == 1 && is_symbolish(&ops[0]) {
                        let t = self.resolve_numeric(&ops[0]);
                        self.push(Piece::Branch {
                            kind: BrKind::Jcc(cc),
                            target: Target { name: t },
                            long: false,
                        });
                        return Ok(());
                    }
                }
            }
        }

        // --- everything else is encoded at once -----------------------
        let (inst, relocs) = self.build(&m, &ops)?;
        let mut buf = Buf::new();
        E::encode(&mut buf, &inst).map_err(|e| self.err(e))?;
        // Map the encoder's fix-ups onto relocations.
        let mut out = Vec::new();
        for f in &buf.fixups {
            let (name, add, kind) = relocs
                .get(f.id as usize)
                .ok_or_else(|| self.err("fix-up without symbol"))?
                .clone();
            let addend = if f.pcrel {
                // The reference point is the end of the instruction: the addend has to
                // subtract the octets between the field start and the instruction end.
                add - ((f.inst_end - f.at) as i64)
            } else {
                add
            };
            out.push(PendReloc { at: f.at, name, kind, addend });
        }
        self.push(Piece::Fixed { bytes: buf.code, relocs: out });
        Ok(())
    }

    /// Builds an instruction from the `0F` space. Here the decision is made
    /// which operand goes into the `reg` field and which into the `r/m` field.
    #[allow(clippy::too_many_arguments)]
    fn sse2(
        &self,
        m: &str,
        a: &str,
        b: &str,
        c: Option<&str>,
        pfx: u8,
        esc: u8,
        op: u8,
        form: SseForm,
        syms: &mut Vec<(String, i64, u32)>,
    ) -> Result<(Inst, Vec<(String, i64, u32)>), String> {
        let imm8 = match c {
            Some(t) => match parse_operand(t, 8, syms)? {
                Opnd::Imm(v) => Some(v as u8),
                _ => return Err(self.err(format!("{} needs an immediate", m))),
            },
            None => None,
        };
        let (pfx, opc, digit, w, reg, rm) = match form {
            SseForm::Rm | SseForm::RmI => {
                let d = parse_operand(a, 128, syms)?;
                let dx = match d {
                    Opnd::Xmm(x) => x,
                    Opnd::Reg(r, _) => r, // pinsrd takes an xmm on the left, but
                    _ => return Err(self.err(format!("{} needs xmm on the left", m))),
                };
                let sw = hint_width(b).unwrap_or(128);
                let src = parse_operand(b, if sw == 128 { 128 } else { sw }, syms)?;
                // pinsrq/pextrq need REX.W
                let w = m.ends_with('q') && (m.starts_with("pinsr") || m.starts_with("pextr"));
                (pfx, op, None::<u8>, w, dx, src)
            }
            SseForm::MrI => {
                // `pextrd r/m32, xmm, imm8` -- the xmm is the SOURCE.
                let sw = hint_width(a).unwrap_or(32);
                let dst = parse_operand(a, sw, syms)?;
                let sx = match parse_operand(b, 128, syms)? {
                    Opnd::Xmm(x) => x,
                    _ => return Err(self.err(format!("{} needs xmm on the right", m))),
                };
                let w = m == "pextrq";
                (pfx, op, None, w, sx, dst)
            }
            SseForm::Mov(store_op) => {
                let wa = hint_width(a);
                let dst_is_mem = a.trim_start().starts_with('[')
                    || matches!(wa, Some(8) | Some(16) | Some(32) | Some(64) | Some(128))
                        && E::gpr_by_name(a.trim()).is_none()
                        && E::xmm_by_name(a.trim()).is_none();
                if dst_is_mem {
                    let dst = parse_operand(a, 128, syms)?;
                    let sx = match parse_operand(b, 128, syms)? {
                        Opnd::Xmm(x) => x,
                        _ => return Err(self.err(format!("{} needs xmm as the source", m))),
                    };
                    (pfx, store_op, None, false, sx, dst)
                } else {
                    let dx = match parse_operand(a, 128, syms)? {
                        Opnd::Xmm(x) => x,
                        _ => return Err(self.err(format!("{} needs xmm as the destination", m))),
                    };
                    let src = parse_operand(b, 128, syms)?;
                    (pfx, op, None, false, dx, src)
                }
            }
            SseForm::Digit(d) => {
                let dx = match parse_operand(a, 128, syms)? {
                    Opnd::Xmm(x) => x,
                    _ => return Err(self.err(format!("{} needs xmm", m))),
                };
                let v = match parse_operand(b, 8, syms)? {
                    Opnd::Imm(v) => v as u8,
                    // `psrlq xmm, xmm` exists too -- then the non-digit form
                    other => {
                        let base = match op {
                            0x71 => 0xD1u8,
                            0x72 => 0xD2,
                            _ => 0xD3,
                        };
                        let opx = match d {
                            2 => base,
                            4 => base + 0x10,
                            _ => base + 0x20,
                        };
                        return Ok((
                            Inst::Sse {
                                op: E::SseOp { pfx, esc, op: opx, w: false, digit: None },
                                reg: dx,
                                rm: other,
                                imm: None,
                            },
                            std::mem::take(syms),
                        ));
                    }
                };
                return Ok((
                    Inst::Sse {
                        op: E::SseOp { pfx, esc, op, w: false, digit: Some(d) },
                        reg: dx,
                        rm: Opnd::Xmm(dx),
                        imm: Some(v),
                    },
                    std::mem::take(syms),
                ));
            }
        };
        let _ = digit;
        Ok((
            Inst::Sse {
                op: E::SseOp { pfx, esc, op: opc, w, digit: None },
                reg,
                rm,
                imm: imm8,
            },
            std::mem::take(syms),
        ))
    }

    fn resolve_numeric(&self, t: &str) -> String {
        // `1f` = next label `1:` forwards, `1b` = the last one backwards.
        let b = t.as_bytes();
        if b.len() >= 2 && b[..b.len() - 1].iter().all(|c| c.is_ascii_digit()) {
            let n: u32 = t[..t.len() - 1].parse().unwrap_or(0);
            let seen = *self.numeric.get(&n).unwrap_or(&0);
            return match b[b.len() - 1] {
                b'f' => format!("\u{1}L{}\u{2}{}", n, seen + 1),
                b'b' => format!("\u{1}L{}\u{2}{}", n, seen),
                _ => t.to_string(),
            };
        }
        t.to_string()
    }

    /// Builds an [`Inst`] from a mnemonic and operand texts.
    ///
    /// Second return value: the symbols the fix-ups in the instruction refer to
    /// (number = order).
    #[allow(clippy::type_complexity)]
    fn build(
        &self,
        m: &str,
        ops: &[String],
    ) -> Result<(Inst, Vec<(String, i64, u32)>), String> {
        let mut syms: Vec<(String, i64, u32)> = Vec::new();
        let mut p = |o: &str, want: u16| -> Result<Opnd, String> { parse_operand(o, want, &mut syms) };

        // Prefix instructions without operands: `rep movsb`, `rep stosb`
        if let Some(r) = m.strip_prefix("rep ") {
            let i = match r {
                "movsb" => Inst::RepMovsb,
                "stosb" => Inst::RepStosb,
                other => return Err(self.err(format!("rep {}", other))),
            };
            return Ok((i, syms));
        }

        // Without operands
        if ops.is_empty() {
            let i = match m {
                "ret" => Inst::Ret,
                "syscall" => Inst::Syscall,
                "hlt" => Inst::Hlt,
                "cld" => Inst::Cld,
                "ud2" => Inst::Ud2,
                "nop" => Inst::Nop,
                "leave" => Inst::Leave,
                "cdq" => Inst::Cdq { wide: false },
                "cqo" => Inst::Cdq { wide: true },
                "cwde" => Inst::Cwde,
                "cbw" => Inst::Cbw,
                "cpuid" => Inst::Zero0F(0xA2),
                "rdtsc" => Inst::Zero0F(0x31),
                "emms" => Inst::Zero0F(0x77),
                other => return Err(self.err(format!("unknown instruction '{}'", other))),
            };
            return Ok((i, syms));
        }

        // One operand
        if ops.len() == 1 {
            let o0 = &ops[0];
            if let Some(cc) = m.strip_prefix("set").and_then(E::cc_by_name) {
                return Ok((Inst::Setcc { cc, dst: p(o0, 8)? }, syms));
            }
            let i = match m {
                "push" => Inst::Push(p(o0, 64)?),
                "pop" => Inst::Pop(p(o0, 64)?),
                "call" => Inst::Call(p(o0, 64)?),
                "jmp" => Inst::Jmp(p(o0, 64)?),
                "not" => Inst::Un3 { op: E::Un3::Not, dst: p(o0, 64)? },
                "neg" => Inst::Un3 { op: E::Un3::Neg, dst: p(o0, 64)? },
                "mul" => Inst::Un3 { op: E::Un3::Mul, dst: p(o0, 64)? },
                "imul" => Inst::Un3 { op: E::Un3::Imul, dst: p(o0, 64)? },
                "div" => Inst::Un3 { op: E::Un3::Div, dst: p(o0, 64)? },
                "idiv" => Inst::Un3 { op: E::Un3::Idiv, dst: p(o0, 64)? },
                "bswap" => {
                    let d = p(o0, 64)?;
                    let (r, b) = reg_of(&d).ok_or_else(|| self.err("bswap needs a register"))?;
                    Inst::Bswap { reg: r, bits: b }
                }
                "inc" => Inst::IncDec { dec: false, dst: p(o0, 64)? },
                "dec" => Inst::IncDec { dec: true, dst: p(o0, 64)? },
                other => return Err(self.err(format!("unknown instruction '{}' with 1 operand", other))),
            };
            return Ok((i, syms));
        }

        // Two operands
        if ops.len() == 2 {
            let (a, b) = (&ops[0], &ops[1]);
            // Prefix instructions
            if let Some(rest) = m.strip_prefix("lock ") {
                let dst = p(a, 64)?;
                let src = p(b, 64)?;
                let (mem, bits) = match dst {
                    Opnd::Mem(mm) => (mm, mm.bits),
                    _ => return Err(self.err("lock needs memory")),
                };
                let sr = match src {
                    Opnd::Reg(r, _) => r,
                    _ => return Err(self.err("lock needs a register")),
                };
                let i = match rest {
                    "cmpxchg" => Inst::LockCmpxchg { mem, src: sr, bits },
                    "xadd" => Inst::LockXadd { mem, src: sr, bits },
                    other => return Err(self.err(format!("lock {}", other))),
                };
                return Ok((i, syms));
            }
            if let Some(cc) = m.strip_prefix("cmov").and_then(E::cc_by_name) {
                let d = p(a, 64)?;
                let (dr, bits) = reg_of(&d).ok_or_else(|| self.err("cmov needs a register"))?;
                return Ok((Inst::Cmovcc { cc, dst: dr, bits, src: p(b, bits)? }, syms));
            }
            // The compare pseudo operations: `cmpltps x, y` is `cmpps x, y, 1`
            // (the predicate is part of the mnemonic, `as` fills the imm8 in).
            if let Some(pred) = cmp_ps_predicate(m) {
                return self.sse2(m, a, b, Some(pred), 0x00, 0, 0xC2, SseForm::RmI, &mut syms);
            }
            // Everything from the 0F space
            if let Some((pfx, esc, op, form)) = sse_table(m) {
                return self.sse2(m, a, b, None, pfx, esc, op, form, &mut syms);
            }
            // crc32 r32/r64, r/m8|16|32|64 -- the width sits in the opcode
            if m == "crc32" {
                let d = parse_operand(a, 64, &mut syms)?;
                let (dr, dbits) = reg_of(&d).ok_or_else(|| self.err("crc32 needs a register"))?;
                let sw = hint_width(b).unwrap_or(dbits);
                let src = parse_operand(b, sw, &mut syms)?;
                let opc = if sw == 8 { 0xF0u8 } else { 0xF1 };
                return Ok((
                    Inst::Sse {
                        op: E::SseOp { pfx: 0xF2, esc: 0x38, op: opc, w: dbits == 64, digit: None },
                        reg: dr,
                        rm: src,
                        imm: None,
                    },
                    syms,
                ));
            }
            // popcnt/lzcnt/tzcnt/bsf/bsr r, r/m
            if let Some((pfx, opc)) = match m {
                "popcnt" => Some((0xF3u8, 0xB8u8)),
                "lzcnt" => Some((0xF3, 0xBD)),
                "tzcnt" => Some((0xF3, 0xBC)),
                "bsf" => Some((0x00, 0xBC)),
                "bsr" => Some((0x00, 0xBD)),
                _ => None,
            } {
                let d = parse_operand(a, 64, &mut syms)?;
                let (dr, dbits) = reg_of(&d)
                    .ok_or_else(|| self.err(format!("{} needs a register", m)))?;
                let src = parse_operand(b, dbits, &mut syms)?;
                return Ok((
                    Inst::Sse {
                        op: E::SseOp { pfx, esc: 0, op: opc, w: dbits == 64, digit: None },
                        reg: dr,
                        rm: src,
                        imm: None,
                    },
                    syms,
                ));
            }
            let i = match m {
                "mov" => {
                    // The width comes from the operand that knows it.
                    let (wa, wb) = (hint_width(a), hint_width(b));
                    let w = wa.or(wb).unwrap_or(64);
                    let dst = p(a, w)?;
                    let w2 = width_hint_of(&dst).or(wb).unwrap_or(w);
                    let src = p(b, w2)?;
                    // movq/movd between xmm and a general purpose register
                    match (&dst, &src) {
                        (Opnd::Xmm(x), Opnd::Reg(r, rb)) => {
                            Inst::MovToXmm { dst: *x, src: *r, bits: *rb }
                        }
                        (Opnd::Reg(r, rb), Opnd::Xmm(x)) => {
                            Inst::MovFromXmm { dst: *r, src: *x, bits: *rb }
                        }
                        _ => Inst::Mov { dst, src },
                    }
                }
                "movq" | "movd" => {
                    let bits = if m == "movq" { 64u16 } else { 32 };
                    let dst = p(a, bits)?;
                    let src = p(b, bits)?;
                    match (&dst, &src) {
                        (Opnd::Xmm(x), Opnd::Reg(r, _)) => {
                            Inst::MovToXmm { dst: *x, src: *r, bits }
                        }
                        (Opnd::Reg(r, _), Opnd::Xmm(x)) => {
                            Inst::MovFromXmm { dst: *r, src: *x, bits }
                        }
                        // `movq xmm, xmm/m64` = F3 0F 7E, `movd xmm, m32` = 66 0F 6E
                        (Opnd::Xmm(x), _) => Inst::Sse {
                            op: E::SseOp {
                                pfx: if m == "movq" { 0xF3 } else { 0x66 },
                                esc: 0,
                                op: if m == "movq" { 0x7E } else { 0x6E },
                                w: false,
                                digit: None,
                            },
                            reg: *x,
                            rm: src,
                            imm: None,
                        },
                        // `movq m64, xmm` = 66 0F D6, `movd m32, xmm` = 66 0F 7E
                        (_, Opnd::Xmm(x)) => Inst::Sse {
                            op: E::SseOp {
                                pfx: 0x66,
                                esc: 0,
                                op: if m == "movq" { 0xD6 } else { 0x7E },
                                w: false,
                                digit: None,
                            },
                            reg: *x,
                            rm: dst,
                            imm: None,
                        },
                        _ => return Err(self.err(format!("{} {}, {}", m, a, b))),
                    }
                }
                "lea" => {
                    let d = p(a, 64)?;
                    let (dr, bits) = reg_of(&d).ok_or_else(|| self.err("lea needs a register"))?;
                    match p(b, 64)? {
                        Opnd::Mem(mm) => Inst::Lea { dst: dr, dst_bits: bits, src: mm },
                        _ => return Err(self.err("lea needs an address")),
                    }
                }
                "add" | "or" | "adc" | "sbb" | "and" | "sub" | "xor" | "cmp" => {
                    let op = match m {
                        "add" => E::Alu::Add,
                        "or" => E::Alu::Or,
                        "adc" => E::Alu::Adc,
                        "sbb" => E::Alu::Sbb,
                        "and" => E::Alu::And,
                        "sub" => E::Alu::Sub,
                        "xor" => E::Alu::Xor,
                        _ => E::Alu::Cmp,
                    };
                    let w = hint_width(a).or_else(|| hint_width(b)).unwrap_or(64);
                    let dst = p(a, w)?;
                    let w2 = width_hint_of(&dst).unwrap_or(w);
                    Inst::Alu { op, dst, src: p(b, w2)? }
                }
                "test" => {
                    let w = hint_width(a).or_else(|| hint_width(b)).unwrap_or(64);
                    let x = p(a, w)?;
                    let w2 = width_hint_of(&x).unwrap_or(w);
                    Inst::Test { a: x, b: p(b, w2)? }
                }
                "shl" | "sal" | "shr" | "sar" | "rol" | "ror" => {
                    let op = match m {
                        "shl" | "sal" => E::Shift::Shl,
                        "shr" => E::Shift::Shr,
                        "sar" => E::Shift::Sar,
                        "rol" => E::Shift::Rol,
                        _ => E::Shift::Ror,
                    };
                    let w = hint_width(a).unwrap_or(64);
                    Inst::Shift { op, dst: p(a, w)?, amount: p(b, 8)? }
                }
                "imul" => {
                    let d = p(a, 64)?;
                    let (dr, bits) = reg_of(&d).ok_or_else(|| self.err("imul needs a register"))?;
                    Inst::Imul2 { dst: dr, bits, src: p(b, bits)? }
                }
                "movzx" | "movsx" => {
                    let d = p(a, 64)?;
                    let (dr, dbits) = reg_of(&d).ok_or_else(|| self.err("movzx needs a register"))?;
                    let sw = hint_width(b).ok_or_else(|| self.err("unknown source width"))?;
                    Inst::MovExt {
                        signed: m == "movsx",
                        dst: dr,
                        dst_bits: dbits,
                        src: p(b, sw)?,
                        src_bits: sw,
                    }
                }
                "movsxd" => {
                    let d = p(a, 64)?;
                    let (dr, _) = reg_of(&d).ok_or_else(|| self.err("movsxd needs a register"))?;
                    Inst::Movsxd { dst: dr, src: p(b, 32)? }
                }
                "cvtsi2sd" | "cvtsi2ss" => {
                    let d = p(a, 128)?;
                    let dx = match d {
                        Opnd::Xmm(x) => x,
                        _ => return Err(self.err("cvtsi2 needs xmm")),
                    };
                    let sw = hint_width(b).unwrap_or(64);
                    Inst::CvtSi2F { double: m == "cvtsi2sd", dst: dx, src: p(b, sw)?, src_bits: sw }
                }
                "cvttsd2si" | "cvttss2si" => {
                    let d = p(a, 64)?;
                    let (dr, bits) = reg_of(&d).ok_or_else(|| self.err("cvtt needs a register"))?;
                    let sx = match p(b, 128)? {
                        Opnd::Xmm(x) => x,
                        _ => return Err(self.err("cvtt needs xmm")),
                    };
                    Inst::CvtF2Si { double: m == "cvttsd2si", dst: dr, dst_bits: bits, src: sx }
                }
                other => return Err(self.err(format!("unknown instruction '{}' with 2 operands", other))),
            };
            return Ok((i, syms));
        }

        // Three operands: SSE with an immediate
        if ops.len() == 3 {
            if let Some((pfx, esc, op, form)) = sse_table(m) {
                // `sha256rnds2 xmm1, xmm2, xmm0` -- the third operand is
                // the prescribed xmm0 and is NOT in the encoding.
                // `as` takes both spellings; the octets are the same.
                if m == "sha256rnds2" {
                    if ops[2].trim() != "xmm0" {
                        return Err(self.err("sha256rnds2 demands xmm0 as the third operand"));
                    }
                    return self.sse2(m, &ops[0], &ops[1], None, pfx, esc, op, form, &mut syms);
                }
                return self.sse2(m, &ops[0], &ops[1], Some(&ops[2]), pfx, esc, op, form, &mut syms);
            }
        }

        // Three operands: only `imul r, r/m, imm`
        if ops.len() == 3 && m == "imul" {
            let d = p(&ops[0], 64)?;
            let (dr, bits) = reg_of(&d).ok_or_else(|| self.err("imul needs a register"))?;
            let src = p(&ops[1], bits)?;
            let imm = match p(&ops[2], bits)? {
                Opnd::Imm(v) => v,
                _ => return Err(self.err("imul needs an immediate")),
            };
            return Ok((Inst::Imul3 { dst: dr, bits, src, imm }, syms));
        }

        Err(self.err(format!("'{}' with {} operands", m, ops.len())))
    }

    // -----------------------------------------------------------------
    // Resolving: labels, relaxation, relocations
    // -----------------------------------------------------------------

    /// Computes lengths and offsets until nothing changes any more, and
    /// builds the finished sections from them.
    pub fn finish(mut self) -> Result<Assembled, String> {
        // Which names are global?
        let mut is_global: HashMap<&str, ()> = HashMap::new();
        for g in &self.globals {
            is_global.insert(g.as_str(), ());
        }

        // --- fixed point of the relaxation ----------------------------
        // `as` starts short and only lets jumps grow. That makes the fixed
        // point unique and it is reached in a few passes.
        let mut offsets: [Vec<u64>; N_SEC] = Default::default();
        let mut labels: HashMap<String, (usize, u64)> = HashMap::new();
        for _round in 0..64 {
            // 1. offsets from the current lengths
            labels.clear();
            for sec in 0..N_SEC {
                let mut off = 0u64;
                let mut v = Vec::with_capacity(self.pieces[sec].len());
                for p in &self.pieces[sec] {
                    v.push(off);
                    match p {
                        Piece::Fixed { bytes, .. } => off += bytes.len() as u64,
                        Piece::Branch { kind, long, .. } => {
                            off += br_len(*kind, *long) as u64
                        }
                        Piece::Align { n } => {
                            let r = off % n;
                            if r != 0 {
                                off += n - r;
                            }
                        }
                        Piece::Label { name } => {
                            labels.insert(name.clone(), (sec, off));
                        }
                    }
                }
                offsets[sec] = v;
            }
            // 2. check the jumps
            let mut changed = false;
            for sec in 0..N_SEC {
                for idx in 0..self.pieces[sec].len() {
                    let (kind, tname, long) = match &self.pieces[sec][idx] {
                        Piece::Branch { kind, target, long } => {
                            (*kind, target.name.clone(), *long)
                        }
                        _ => continue,
                    };
                    if long {
                        continue;
                    }
                    let resolvable = match labels.get(&tname) {
                        Some((tsec, _)) => {
                            *tsec == sec
                                && (kind != BrKind::Call || !is_global.contains_key(tname.as_str()))
                        }
                        None => false,
                    };
                    if !resolvable {
                        // extern/global -> always long
                        self.pieces[sec][idx] = Piece::Branch {
                            kind,
                            target: Target { name: tname },
                            long: true,
                        };
                        changed = true;
                        continue;
                    }
                    let here = offsets[sec][idx];
                    let (_, tgt) = labels[&tname];
                    let end = here + br_len(kind, false) as u64;
                    let disp = tgt as i64 - end as i64;
                    if !(-128..=127).contains(&disp) {
                        self.pieces[sec][idx] = Piece::Branch {
                            kind,
                            target: Target { name: tname },
                            long: true,
                        };
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        // --- symbol table --------------------------------------------
        // `as` writes the section symbols first, then the local ones, then
        // the global ones. We need the section symbols anyway: relocations
        // against local labels go through them.
        let secnames: [&'static str; N_SEC] =
            [".text", ".data", ".rodata", ".note.GNU-stack", ".bss"];
        let mut symbols: Vec<Sym> = Vec::new();
        let mut symidx: HashMap<String, usize> = HashMap::new();
        for (i, n) in secnames.iter().enumerate() {
            symidx.insert(format!("\u{3}sec{}", i), symbols.len());
            symbols.push(Sym {
                name: (*n).to_string(),
                section: Some(i),
                value: 0,
                global: false,
                is_section: true,
            });
        }
        for g in &self.globals {
            if symidx.contains_key(g.as_str()) {
                continue;
            }
            let (sec, val) = match labels.get(g.as_str()) {
                Some((s, v)) => (Some(*s), *v),
                None => (None, 0),
            };
            symidx.insert(g.clone(), symbols.len());
            symbols.push(Sym {
                name: g.clone(),
                section: sec,
                value: val,
                global: true,
                is_section: false,
            });
        }

        // --- write the octets ----------------------------------------
        let mut sections: Vec<Section> = (0..N_SEC)
            .map(|i| Section {
                name: secnames[i],
                bytes: Vec::new(),
                relocs: Vec::new(),
                align: self.aligns[i],
            })
            .collect();

        for sec in 0..N_SEC {
            let pieces = std::mem::take(&mut self.pieces[sec]);
            for p in pieces {
                let base = sections[sec].bytes.len() as u64;
                match p {
                    Piece::Label { .. } => {}
                    Piece::Align { n } => {
                        let r = base % n;
                        if r != 0 {
                            let pad = (n - r) as usize;
                            if sec == SEC_TEXT {
                                push_nops(&mut sections[sec].bytes, pad);
                            } else {
                                sections[sec].bytes.extend(std::iter::repeat(0u8).take(pad));
                            }
                        }
                    }
                    Piece::Fixed { bytes, relocs } => {
                        sections[sec].bytes.extend_from_slice(&bytes);
                        for r in relocs {
                            let (si, extra) = sym_for(
                                &r.name,
                                &labels,
                                &is_global,
                                &mut symbols,
                                &mut symidx,
                            );
                            sections[sec].relocs.push(Reloc {
                                offset: base + r.at as u64,
                                sym: si,
                                kind: r.kind,
                                addend: r.addend + extra,
                            });
                        }
                    }
                    Piece::Branch { kind, target, long } => {
                        let resolvable = match labels.get(&target.name) {
                            Some((tsec, _)) => {
                                *tsec == sec
                                    && (kind != BrKind::Call
                                        || !is_global.contains_key(target.name.as_str()))
                            }
                            None => false,
                        };
                        let mut buf = Buf::new();
                        if resolvable {
                            let (_, tgt) = labels[&target.name];
                            let end = base + br_len(kind, long) as u64;
                            let disp = tgt as i64 - end as i64;
                            let o = Opnd::Rel { disp, fix: None, short: !long };
                            let inst = match kind {
                                BrKind::Jmp => Inst::Jmp(o),
                                BrKind::Call => Inst::Call(o),
                                BrKind::Jcc(cc) => Inst::Jcc { cc, target: o },
                            };
                            E::encode(&mut buf, &inst)?;
                            sections[sec].bytes.extend_from_slice(&buf.code);
                        } else {
                            let o = Opnd::Rel { disp: 0, fix: Some(0), short: false };
                            let inst = match kind {
                                BrKind::Jmp => Inst::Jmp(o),
                                BrKind::Call => Inst::Call(o),
                                BrKind::Jcc(cc) => Inst::Jcc { cc, target: o },
                            };
                            E::encode(&mut buf, &inst)?;
                            let f = buf.fixups[0];
                            sections[sec].bytes.extend_from_slice(&buf.code);
                            let (si, extra) = sym_for(
                                &target.name,
                                &labels,
                                &is_global,
                                &mut symbols,
                                &mut symidx,
                            );
                            // jump/call to a symbol: PLT32, addend -4.
                            sections[sec].relocs.push(Reloc {
                                offset: base + f.at as u64,
                                sym: si,
                                kind: R_X86_64_PLT32,
                                addend: extra - 4,
                            });
                        }
                    }
                }
            }
        }

        Ok(Assembled { sections, symbols })
    }
}

/// Finds (or creates) the symbol a relocation runs against.
///
/// The rule `as` applies is in here: a **local** label gets no row of its own
/// in the symbol table; the relocation runs against the **section symbol**
/// and the label's offset moves into the addend. A **global** symbol stands
/// for itself.
fn sym_for(
    name: &str,
    labels: &HashMap<String, (usize, u64)>,
    is_global: &HashMap<&str, ()>,
    symbols: &mut Vec<Sym>,
    symidx: &mut HashMap<String, usize>,
) -> (usize, i64) {
    if is_global.contains_key(name) {
        if let Some(i) = symidx.get(name) {
            return (*i, 0);
        }
    }
    if let Some((sec, off)) = labels.get(name) {
        if !is_global.contains_key(name) {
            let key = format!("\u{3}sec{}", sec);
            return (symidx[&key], *off as i64);
        }
        let i = *symidx.get(name).unwrap();
        let _ = off;
        return (i, 0);
    }
    // unknown → external symbol
    if let Some(i) = symidx.get(name) {
        return (*i, 0);
    }
    let i = symbols.len();
    symidx.insert(name.to_string(), i);
    symbols.push(Sym {
        name: name.to_string(),
        section: None,
        value: 0,
        global: true,
        is_section: false,
    });
    (i, 0)
}

fn br_len(kind: BrKind, long: bool) -> usize {
    match (kind, long) {
        (BrKind::Call, _) => 5,
        (BrKind::Jmp, false) => 2,
        (BrKind::Jmp, true) => 5,
        (BrKind::Jcc(_), false) => 2,
        (BrKind::Jcc(_), true) => 6,
    }
}

/// Padding in the code section: `as` writes multi-octet `nop`s, not 0x90.
fn push_nops(out: &mut Vec<u8>, mut n: usize) {
    const NOPS: [&[u8]; 12] = [
        &[],
        &[0x90],
        &[0x66, 0x90],
        &[0x0F, 0x1F, 0x00],
        &[0x0F, 0x1F, 0x40, 0x00],
        &[0x0F, 0x1F, 0x44, 0x00, 0x00],
        &[0x66, 0x0F, 0x1F, 0x44, 0x00, 0x00],
        &[0x0F, 0x1F, 0x80, 0x00, 0x00, 0x00, 0x00],
        &[0x0F, 0x1F, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00],
        &[0x66, 0x0F, 0x1F, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00],
        &[0x66, 0x66, 0x0F, 0x1F, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00],
        &[0x66, 0x66, 0x66, 0x0F, 0x1F, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00],
    ];
    while n > 0 {
        let k = n.min(11);
        out.extend_from_slice(NOPS[k]);
        n -= k;
    }
}

// ===========================================================================
// Splitting operands
// ===========================================================================

/// Is this operand text a label/a symbol (and no register, no address, no
/// number)? Only then does `jmp x` become a jump piece.
fn is_symbolish(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    if b[0].is_ascii_digit() {
        // the numeric labels `1f` / `1b`
        return b.len() >= 2
            && matches!(b[b.len() - 1], b'f' | b'b')
            && b[..b.len() - 1].iter().all(|c| c.is_ascii_digit());
    }
    if !(b[0].is_ascii_alphabetic() || matches!(b[0], b'_' | b'.')) {
        return false;
    }
    // a register name is no symbol
    if E::gpr_by_name(s).is_some() || E::xmm_by_name(s).is_some() {
        return false;
    }
    // a size word introduces an address
    let low = s.to_ascii_lowercase();
    for w in ["byte ptr", "word ptr", "dword ptr", "qword ptr", "xmmword ptr"] {
        if low.starts_with(w) {
            return false;
        }
    }
    !s.contains('[')
}

/// Splits the operand list at the top-level commas.
fn split_operands(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut depth = 0;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '[' => {
                depth += 1;
                cur.push(c);
            }
            ']' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur = String::new();
            }
            _ => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

fn split_commas(s: &str) -> Vec<String> {
    s.split(',').map(|x| x.trim().to_string()).collect()
}

/// Like `split_commas`, but commas in quotation marks do not separate.
fn split_commas_strings(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    let mut esc = false;
    for c in s.chars() {
        if esc {
            cur.push(c);
            esc = false;
            continue;
        }
        match c {
            '\\' if in_str => {
                cur.push(c);
                esc = true;
            }
            '"' => {
                in_str = !in_str;
                cur.push(c);
            }
            ',' if !in_str => {
                out.push(cur.trim().to_string());
                cur = String::new();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn unescape(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim();
    let inner = s
        .strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .ok_or_else(|| format!("not a string: {}", s))?;
    let mut out = Vec::new();
    let b = inner.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            i += 1;
            match b[i] {
                b'n' => out.push(b'\n'),
                b't' => out.push(b'\t'),
                b'r' => out.push(b'\r'),
                b'0' => out.push(0),
                b'\\' => out.push(b'\\'),
                b'"' => out.push(b'"'),
                b'a' => out.push(7),
                b'b' => out.push(8),
                b'f' => out.push(12),
                b'v' => out.push(11),
                b'x' => {
                    let mut v = 0u32;
                    let mut k = 0;
                    while i + 1 < b.len() && (b[i + 1] as char).is_ascii_hexdigit() && k < 2 {
                        i += 1;
                        v = v * 16 + (b[i] as char).to_digit(16).unwrap();
                        k += 1;
                    }
                    out.push(v as u8);
                }
                c if c.is_ascii_digit() => {
                    // octal, up to three digits
                    let mut v = (c - b'0') as u32;
                    let mut k = 1;
                    while i + 1 < b.len() && (b[i + 1] as char).is_digit(8) && k < 3 {
                        i += 1;
                        v = v * 8 + (b[i] - b'0') as u32;
                        k += 1;
                    }
                    out.push(v as u8);
                }
                c => out.push(c),
            }
            i += 1;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Ok(out)
}

fn parse_int(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return i64::from_str_radix(h, 16).ok().or_else(|| {
            u64::from_str_radix(h, 16).ok().map(|v| v as i64)
        });
    }
    if let Some(h) = s.strip_prefix("-0x").or_else(|| s.strip_prefix("-0X")) {
        return i64::from_str_radix(h, 16).ok().map(|v| -v);
    }
    s.parse::<i64>()
        .ok()
        .or_else(|| s.parse::<u64>().ok().map(|v| v as i64))
}

/// `sym + 8` / `sym - 4` / `sym`
fn split_addend(s: &str) -> (String, i64) {
    let s = s.trim();
    for (i, c) in s.char_indices().skip(1) {
        if c == '+' || c == '-' {
            let name = s[..i].trim().to_string();
            let num = s[i..].replace(' ', "");
            if let Ok(v) = num.parse::<i64>() {
                return (name, v);
            }
        }
    }
    (s.to_string(), 0)
}

/// Which width does the operand text reveal by itself?
fn hint_width(o: &str) -> Option<u16> {
    let l = o.trim();
    if let Some((_, b)) = E::gpr_by_name(l) {
        return Some(b);
    }
    if E::xmm_by_name(l).is_some() {
        return Some(128);
    }
    let low = l.to_ascii_lowercase();
    for (w, bits) in [
        ("byte ptr", 8u16),
        ("word ptr", 16),
        ("dword ptr", 32),
        ("qword ptr", 64),
        ("xmmword ptr", 128),
    ] {
        if low.starts_with(w) {
            return Some(bits);
        }
    }
    None
}

fn width_hint_of(o: &Opnd) -> Option<u16> {
    match o {
        Opnd::Reg(_, b) => Some(*b),
        Opnd::Mem(m) => Some(m.bits),
        Opnd::Xmm(_) => Some(128),
        _ => None,
    }
}

fn reg_of(o: &Opnd) -> Option<(u8, u16)> {
    match o {
        Opnd::Reg(r, b) => Some((*r, *b)),
        _ => None,
    }
}

/// Parses a single operand.
///
/// `want` is the width the instruction expects in case the text itself names
/// none (immediates, labels).
fn parse_operand(
    o: &str,
    want: u16,
    syms: &mut Vec<(String, i64, u32)>,
) -> Result<Opnd, String> {
    let s = o.trim();
    if let Some((r, b)) = E::gpr_by_name(s) {
        return Ok(Opnd::Reg(r, b));
    }
    if let Some(x) = E::xmm_by_name(s) {
        return Ok(Opnd::Xmm(x));
    }
    if let Some(v) = parse_int(s) {
        return Ok(Opnd::Imm(v));
    }
    // Memory: [..] with or without a size word, with or without a segment
    let low = s.to_ascii_lowercase();
    let mut bits = want;
    let mut rest = s;
    for (w, b) in [
        ("byte ptr", 8u16),
        ("word ptr", 16),
        ("dword ptr", 32),
        ("qword ptr", 64),
        ("xmmword ptr", 128),
    ] {
        if low.starts_with(w) {
            bits = b;
            rest = s[w.len()..].trim();
            break;
        }
    }
    // Segment prefix `fs:0` (Firn uses it for the thread's own area)
    if rest.starts_with("fs:") || rest.starts_with("gs:") {
        let seg = if rest.starts_with("fs:") { 0x64u8 } else { 0x65 };
        let after = &rest[3..];
        let a = after.trim();
        if let Some(inner) = a.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
            let mut o = parse_mem(inner, bits, syms)?;
            if let Opnd::Mem(ref mut mm) = o {
                mm.seg = seg;
            }
            return Ok(o);
        }
        let d = parse_int(a).ok_or_else(|| format!("segment address: {}", rest))?;
        return Ok(Opnd::Mem(Mem {
            base: None,
            index: None,
            scale: 1,
            disp: d,
            rip: false,
            bits,
            disp_fix: None,
            seg,
        }));
    }
    if let Some(inner) = rest.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
        return parse_mem(inner, bits, syms);
    }
    // A symbol as an immediate.
    let (name, add) = split_addend(s);
    let id = syms.len() as u32;
    syms.push((name, add, R_X86_64_32S));
    Ok(Opnd::SymImm { addend: add, fix: Some(id) })
}

/// The content of `[...]`.
fn parse_mem(
    inner: &str,
    bits: u16,
    syms: &mut Vec<(String, i64, u32)>,
) -> Result<Opnd, String> {
    let t = inner.trim();
    // RIP relative: `rip + name` or `rip - 4`
    if let Some(r) = t.strip_prefix("rip") {
        let r = r.trim();
        let r = r.strip_prefix('+').unwrap_or(r).trim();
        if let Some(v) = parse_int(r) {
            return Ok(Opnd::Mem(Mem::rip_rel(bits, v, None)));
        }
        let (name, add) = split_addend(r);
        let id = syms.len() as u32;
        syms.push((name, add, R_X86_64_PC32));
        return Ok(Opnd::Mem(Mem::rip_rel(bits, add, Some(id))));
    }

    let mut base: Option<u8> = None;
    let mut index: Option<u8> = None;
    let mut scale: u8 = 1;
    let mut disp: i64 = 0;

    // Split into pieces: separate at + and -, keep the sign.
    let mut terms: Vec<(i64, String)> = Vec::new();
    let mut sign = 1i64;
    let mut cur = String::new();
    for c in t.chars() {
        match c {
            '+' | '-' => {
                if !cur.trim().is_empty() {
                    terms.push((sign, cur.trim().to_string()));
                }
                cur = String::new();
                sign = if c == '-' { -1 } else { 1 };
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        terms.push((sign, cur.trim().to_string()));
    }

    for (sg, term) in terms {
        if let Some(star) = term.find('*') {
            let (r, sc) = term.split_at(star);
            let sc = &sc[1..];
            let (rn, _) = E::gpr_by_name(r.trim())
                .ok_or_else(|| format!("not a register: {}", r))?;
            index = Some(rn);
            scale = sc.trim().parse::<u8>().map_err(|_| format!("scale: {}", sc))?;
            continue;
        }
        if let Some((rn, _)) = E::gpr_by_name(term.trim()) {
            if base.is_none() && sg > 0 {
                base = Some(rn);
            } else if index.is_none() {
                index = Some(rn);
                scale = 1;
            } else {
                return Err(format!("too many registers in [{}]", inner));
            }
            continue;
        }
        if let Some(v) = parse_int(&term) {
            disp += sg * v;
            continue;
        }
        return Err(format!("cannot parse part '{}' in [{}]", term, inner));
    }

    Ok(Opnd::Mem(Mem { base, index, scale, disp, rip: false, bits, disp_fix: None, seg: 0 }))
}

/// How an instruction from the `0F` space distributes its operands.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SseForm {
    /// `op xmm_dst, xmm/m_src` — reg = destination.
    Rm,
    /// `op xmm_dst, xmm/m_src, imm8`.
    RmI,
    /// `op r/m_dst, xmm_src, imm8` — reg = SOURCE (pextrd/pextrq).
    MrI,
    /// Load/store form: `mov... xmm, xmm/m` **or** `mov... m, xmm`.
    /// The second opcode is the memory case.
    Mov(u8),
    /// `op xmm, imm8` with a fixed digit in the reg field (psrlq, pslldq ...).
    Digit(u8),
}

/// The predicate (the imm8 of `cmpps`) of a compare pseudo operation such as
/// `cmpltps`, as the text of an immediate.
fn cmp_ps_predicate(m: &str) -> Option<&'static str> {
    Some(match m {
        "cmpeqps" => "0",
        "cmpltps" => "1",
        "cmpleps" => "2",
        "cmpunordps" => "3",
        "cmpneqps" => "4",
        "cmpnltps" => "5",
        "cmpnleps" => "6",
        "cmpordps" => "7",
        _ => return None,
    })
}

/// The instructions from the `0F` space that Firn's code generator emits.
///
/// Established from `compiler/src/simd.rs` and from counting the tree; every
/// line is checked against `as` (`tools/kodierer/run.sh`).
/// Returns: (mandatory prefix, second opcode octet, opcode, form).
fn sse_table(m: &str) -> Option<(u8, u8, u8, SseForm)> {
    use SseForm::*;
    Some(match m {
        // --- floating point, scalar --------------------------------
        "addsd" => (0xF2, 0, 0x58, Rm),
        "subsd" => (0xF2, 0, 0x5C, Rm),
        "mulsd" => (0xF2, 0, 0x59, Rm),
        "divsd" => (0xF2, 0, 0x5E, Rm),
        "sqrtsd" => (0xF2, 0, 0x51, Rm),
        "minsd" => (0xF2, 0, 0x5D, Rm),
        "maxsd" => (0xF2, 0, 0x5F, Rm),
        "addss" => (0xF3, 0, 0x58, Rm),
        "subss" => (0xF3, 0, 0x5C, Rm),
        "mulss" => (0xF3, 0, 0x59, Rm),
        "divss" => (0xF3, 0, 0x5E, Rm),
        "sqrtss" => (0xF3, 0, 0x51, Rm),
        "minss" => (0xF3, 0, 0x5D, Rm),
        "maxss" => (0xF3, 0, 0x5F, Rm),
        "ucomisd" => (0x66, 0, 0x2E, Rm),
        "comisd" => (0x66, 0, 0x2F, Rm),
        "ucomiss" => (0x00, 0, 0x2E, Rm),
        "comiss" => (0x00, 0, 0x2F, Rm),
        "cvtss2sd" => (0xF3, 0, 0x5A, Rm),
        "cvtsd2ss" => (0xF2, 0, 0x5A, Rm),
        // --- floating point, packed single (the v128 f32 operations) ---
        "addps" => (0x00, 0, 0x58, Rm),
        "subps" => (0x00, 0, 0x5C, Rm),
        "mulps" => (0x00, 0, 0x59, Rm),
        "divps" => (0x00, 0, 0x5E, Rm),
        "sqrtps" => (0x00, 0, 0x51, Rm),
        "minps" => (0x00, 0, 0x5D, Rm),
        "maxps" => (0x00, 0, 0x5F, Rm),
        "cvtdq2ps" => (0x00, 0, 0x5B, Rm),
        "cvtps2dq" => (0x66, 0, 0x5B, Rm),
        "cvttps2dq" => (0xF3, 0, 0x5B, Rm),
        "unpcklps" => (0x00, 0, 0x14, Rm),
        "unpckhps" => (0x00, 0, 0x15, Rm),
        "shufps" => (0x00, 0, 0xC6, RmI),
        "cmpps" => (0x00, 0, 0xC2, RmI),
        // --- moving (load and store form) ---------------------------
        "movsd" => (0xF2, 0, 0x10, Mov(0x11)),
        "movss" => (0xF3, 0, 0x10, Mov(0x11)),
        "movaps" => (0x00, 0, 0x28, Mov(0x29)),
        "movapd" => (0x66, 0, 0x28, Mov(0x29)),
        "movups" => (0x00, 0, 0x10, Mov(0x11)),
        "movupd" => (0x66, 0, 0x10, Mov(0x11)),
        "movlps" => (0x00, 0, 0x12, Mov(0x13)),
        "movhps" => (0x00, 0, 0x16, Mov(0x17)),
        "movdqa" => (0x66, 0, 0x6F, Mov(0x7F)),
        "movdqu" => (0xF3, 0, 0x6F, Mov(0x7F)),
        // --- bitwise, integer --------------------------------------
        "xorps" => (0x00, 0, 0x57, Rm),
        "xorpd" => (0x66, 0, 0x57, Rm),
        "andps" => (0x00, 0, 0x54, Rm),
        "andpd" => (0x66, 0, 0x54, Rm),
        "andnps" => (0x00, 0, 0x55, Rm),
        "andnpd" => (0x66, 0, 0x55, Rm),
        "orps" => (0x00, 0, 0x56, Rm),
        "orpd" => (0x66, 0, 0x56, Rm),
        "pxor" => (0x66, 0, 0xEF, Rm),
        "pand" => (0x66, 0, 0xDB, Rm),
        "pandn" => (0x66, 0, 0xDF, Rm),
        "por" => (0x66, 0, 0xEB, Rm),
        "paddb" => (0x66, 0, 0xFC, Rm),
        "paddw" => (0x66, 0, 0xFD, Rm),
        "paddd" => (0x66, 0, 0xFE, Rm),
        "paddq" => (0x66, 0, 0xD4, Rm),
        "psubb" => (0x66, 0, 0xF8, Rm),
        "psubw" => (0x66, 0, 0xF9, Rm),
        "psubd" => (0x66, 0, 0xFA, Rm),
        "psubq" => (0x66, 0, 0xFB, Rm),
        "pmuludq" => (0x66, 0, 0xF4, Rm),
        "pmulld" => (0x66, 0x38, 0x40, Rm),
        "pcmpeqb" => (0x66, 0, 0x74, Rm),
        "pcmpeqd" => (0x66, 0, 0x76, Rm),
        "pcmpgtd" => (0x66, 0, 0x66, Rm),
        "punpcklbw" => (0x66, 0, 0x60, Rm),
        "punpckldq" => (0x66, 0, 0x62, Rm),
        "punpckhdq" => (0x66, 0, 0x6A, Rm),
        "punpcklqdq" => (0x66, 0, 0x6C, Rm),
        "punpckhqdq" => (0x66, 0, 0x6D, Rm),
        "packusdw" => (0x66, 0x38, 0x2B, Rm),
        "pshufb" => (0x66, 0x38, 0x00, Rm),
        "ptest" => (0x66, 0x38, 0x17, Rm),
        // --- with an immediate -------------------------------------
        "pshufd" => (0x66, 0, 0x70, RmI),
        "pshuflw" => (0xF2, 0, 0x70, RmI),
        "pshufhw" => (0xF3, 0, 0x70, RmI),
        "palignr" => (0x66, 0x3A, 0x0F, RmI),
        "pblendw" => (0x66, 0x3A, 0x0E, RmI),
        "pclmulqdq" => (0x66, 0x3A, 0x44, RmI),
        "aeskeygenassist" => (0x66, 0x3A, 0xDF, RmI),
        "pinsrd" => (0x66, 0x3A, 0x22, RmI),
        "pinsrq" => (0x66, 0x3A, 0x22, RmI),
        "pinsrb" => (0x66, 0x3A, 0x20, RmI),
        "pinsrw" => (0x66, 0, 0xC4, RmI),
        "pextrd" => (0x66, 0x3A, 0x16, MrI),
        "pextrq" => (0x66, 0x3A, 0x16, MrI),
        "pextrb" => (0x66, 0x3A, 0x14, MrI),
        "pextrw" => (0x66, 0x3A, 0x15, MrI),
        // --- shifts with a fixed digit -----------------------------
        "psllw" => (0x66, 0, 0x71, Digit(6)),
        "pslld" => (0x66, 0, 0x72, Digit(6)),
        "psllq" => (0x66, 0, 0x73, Digit(6)),
        "pslldq" => (0x66, 0, 0x73, Digit(7)),
        "psrlw" => (0x66, 0, 0x71, Digit(2)),
        "psrld" => (0x66, 0, 0x72, Digit(2)),
        "psrlq" => (0x66, 0, 0x73, Digit(2)),
        "psrldq" => (0x66, 0, 0x73, Digit(3)),
        "psraw" => (0x66, 0, 0x71, Digit(4)),
        "psrad" => (0x66, 0, 0x72, Digit(4)),
        // --- AES and SHA -------------------------------------------
        "aesenc" => (0x66, 0x38, 0xDC, Rm),
        "aesenclast" => (0x66, 0x38, 0xDD, Rm),
        "aesdec" => (0x66, 0x38, 0xDE, Rm),
        "aesdeclast" => (0x66, 0x38, 0xDF, Rm),
        "aesimc" => (0x66, 0x38, 0xDB, Rm),
        "sha256rnds2" => (0x00, 0x38, 0xCB, Rm),
        "sha256msg1" => (0x00, 0x38, 0xCC, Rm),
        "sha256msg2" => (0x00, 0x38, 0xCD, Rm),
        "sha1msg1" => (0x00, 0x38, 0xC9, Rm),
        "sha1msg2" => (0x00, 0x38, 0xCA, Rm),
        "sha1nexte" => (0x00, 0x38, 0xC8, Rm),
        "sha1rnds4" => (0x00, 0x3A, 0xCC, RmI),
        _ => return None,
    })
}

/// The convenient overall entry: text in, object content out.
pub fn assemble(text: &str) -> Result<Assembled, String> {
    let mut a = Asm::new();
    a.feed(text)?;
    a.finish()
}

// ===========================================================================
// Transition to the ELF writer
// ===========================================================================

/// The section numbers `elfobj` uses: `.text .data .bss .rodata
/// .note.GNU-stack`. `.bss` sits in between because `as` lays it out that way.
fn elf_index(sec: usize) -> usize {
    match sec {
        SEC_TEXT => 0,
        SEC_DATA => 1,
        SEC_BSS => 2,
        SEC_RODATA => 3,
        _ => 4,
    }
}

/// The whole way: assembler text -> octets of an ELF object file.
pub fn assemble_to_object(text: &str) -> Result<Vec<u8>, String> {
    let a = assemble(text)?;
    let mut secs = crate::elfobj::standard_sections_bss(
        a.sections[SEC_TEXT].bytes.clone(),
        a.sections[SEC_DATA].bytes.clone(),
        a.sections[SEC_RODATA].bytes.clone(),
        a.sections[SEC_BSS].bytes.len(),
        [
            a.sections[SEC_TEXT].align,
            a.sections[SEC_DATA].align,
            a.sections[SEC_RODATA].align,
        ],
    );
    for s in 0..N_SEC {
        let ei = elf_index(s);
        for r in &a.sections[s].relocs {
            secs[ei].relocs.push(crate::elfobj::OutReloc {
                offset: r.offset,
                sym: r.sym,
                kind: r.kind,
                addend: r.addend,
            });
        }
    }
    let syms: Vec<crate::elfobj::OutSym> = a
        .symbols
        .iter()
        .map(|s| crate::elfobj::OutSym {
            name: s.name.clone(),
            section: s.section.map(elf_index),
            value: s.value,
            global: s.global,
            is_section: s.is_section,
        })
        .collect();
    Ok(crate::elfobj::write(crate::elfobj::EM_X86_64, &secs, &syms))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The octets of `.text` for a piece of assembler text.
    fn text_of(s: &str) -> String {
        let a = assemble(&format!(".intel_syntax noprefix\n.text\n{}\n", s)).expect("assembles");
        a.sections[SEC_TEXT].bytes.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
    }

    /// The octets `as` produces (objdump of the same lines), checked in by hand
    /// once: the packed single operations of the `v128` type.
    #[test]
    fn packed_single_operations_match_as() {
        let cases: &[(&str, &str)] = &[
            ("addps xmm0, xmm1", "0f 58 c1"),
            ("subps xmm2, xmm3", "0f 5c d3"),
            ("mulps xmm8, xmm9", "45 0f 59 c1"),
            ("divps xmm1, xmmword ptr [rax]", "0f 5e 08"),
            ("sqrtps xmm1, xmm2", "0f 51 ca"),
            ("minps xmm1, xmm2", "0f 5d ca"),
            ("maxps xmm1, xmm2", "0f 5f ca"),
            ("cvtdq2ps xmm1, xmm2", "0f 5b ca"),
            ("cvtps2dq xmm1, xmm2", "66 0f 5b ca"),
            ("cvttps2dq xmm1, xmm2", "f3 0f 5b ca"),
            ("unpcklps xmm1, xmm2", "0f 14 ca"),
            ("unpckhps xmm1, xmm2", "0f 15 ca"),
            ("shufps xmm0, xmm1, 0x1b", "0f c6 c1 1b"),
            ("cmpps xmm0, xmm1, 3", "0f c2 c1 03"),
            ("cmpeqps xmm0, xmm1", "0f c2 c1 00"),
            ("cmpltps xmm0, xmm1", "0f c2 c1 01"),
            ("cmpleps xmm2, xmm3", "0f c2 d3 02"),
            ("cmpunordps xmm0, xmm1", "0f c2 c1 03"),
            ("cmpneqps xmm0, xmm1", "0f c2 c1 04"),
            ("cmpnltps xmm9, xmm10", "45 0f c2 ca 05"),
            ("cmpnleps xmm0, xmm1", "0f c2 c1 06"),
            ("cmpordps xmm0, xmm1", "0f c2 c1 07"),
            ("movlps xmm0, qword ptr [rax]", "0f 12 00"),
            ("movlps qword ptr [rax], xmm1", "0f 13 08"),
            ("movlps qword ptr [rbp - 8], xmm9", "44 0f 13 4d f8"),
            ("movhps xmm0, qword ptr [rax]", "0f 16 00"),
            ("movhps qword ptr [rax], xmm1", "0f 17 08"),
        ];
        for (src, want) in cases {
            assert_eq!(&text_of(src), want, "{}", src);
        }
    }
}
