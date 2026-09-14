// SPDX-License-Identifier: GPL-2.0-only
//! Binaerkodierung fuer x86-64 -- Oktette statt Text.
//!
//! Das ist der Baustein, der Firn bisher gefehlt hat. `codegen_x86.rs` waehlt
//! Befehle aus und teilt Register zu, schreibt das Ergebnis aber als Text fuer
//! GNU `as`. Hier steht, wie aus Befehl + Operanden die tatsaechlichen Oktette
//! werden. Ohne das ist kein JIT moeglich, denn ein JIT hat keinen Assembler --
//! er schreibt in eine ausfuehrbare Speicherseite.
//!
//! Der Umfang ist die GEMESSENE Befehlsmenge aus tools/asmstudie, nicht der
//! ganze Befehlssatz: 215 Operandenformen ueber 83 Mnemonics.
//!
//! Die Abnahme ist byteweise Gleichheit mit GNU `as`. Nicht "sieht richtig aus".

#![allow(dead_code)]

// ---------------------------------------------------------------- Register --

/// 64-Bit-Register in der Nummerierung der Architektur.
/// Die Nummer ist das, was in ModRM/SIB landet; Bit 3 wandert ins REX-Praefix.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Reg {
    Rax = 0, Rcx = 1, Rdx = 2, Rbx = 3,
    Rsp = 4, Rbp = 5, Rsi = 6, Rdi = 7,
    R8 = 8, R9 = 9, R10 = 10, R11 = 11,
    R12 = 12, R13 = 13, R14 = 14, R15 = 15,
}

impl Reg {
    pub fn nr(self) -> u8 { self as u8 }
    /// Die unteren drei Bits -- das Feld in ModRM/SIB.
    pub fn tief(self) -> u8 { (self as u8) & 7 }
    /// Bit 3 -- landet in REX.R/.X/.B.
    pub fn hoch(self) -> u8 { ((self as u8) >> 3) & 1 }

    pub fn aus_nr(n: u8) -> Reg {
        use Reg::*;
        match n & 15 {
            0 => Rax, 1 => Rcx, 2 => Rdx, 3 => Rbx,
            4 => Rsp, 5 => Rbp, 6 => Rsi, 7 => Rdi,
            8 => R8, 9 => R9, 10 => R10, 11 => R11,
            12 => R12, 13 => R13, 14 => R14, _ => R15,
        }
    }
}

/// SSE-Register.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Xmm(pub u8);
impl Xmm {
    pub fn tief(self) -> u8 { self.0 & 7 }
    pub fn hoch(self) -> u8 { (self.0 >> 3) & 1 }
}

/// Operandenbreite. Bestimmt Praefix (0x66), REX.W und teils den Opcode.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Breite { B8, B16, B32, B64 }

impl Breite {
    fn rex_w(self) -> bool { self == Breite::B64 }
    fn op16(self) -> bool { self == Breite::B16 }
}

// ------------------------------------------------------------ Speicherform --

/// Ein Speicheroperand. Deckt genau die Adressarten ab, die gemessen wurden:
/// [base], [base+disp8], [base+disp32], [base+index*skala], [rip+marke].
#[derive(Copy, Clone, Debug)]
pub struct Mem {
    pub basis: Option<Reg>,
    pub index: Option<Reg>,
    pub skala: u8,      // 1, 2, 4, 8
    pub disp: i32,
    pub rip: bool,      // RIP-relativ: disp ist dann der Abstand zum naechsten Befehl
}

impl Mem {
    pub fn basis(b: Reg) -> Mem {
        Mem { basis: Some(b), index: None, skala: 1, disp: 0, rip: false }
    }
    pub fn basis_disp(b: Reg, d: i32) -> Mem {
        Mem { basis: Some(b), index: None, skala: 1, disp: d, rip: false }
    }
    pub fn basis_index(b: Reg, i: Reg, s: u8) -> Mem {
        Mem { basis: Some(b), index: Some(i), skala: s, disp: 0, rip: false }
    }
    pub fn basis_index_disp(b: Reg, i: Reg, s: u8, d: i32) -> Mem {
        Mem { basis: Some(b), index: Some(i), skala: s, disp: d, rip: false }
    }
    pub fn rip(d: i32) -> Mem {
        Mem { basis: None, index: None, skala: 1, disp: d, rip: true }
    }

    /// Braucht die Adresse eine Verschiebung im Oktettstrom, und wie gross?
    /// Das ist eine der Stellen, an denen Kodierer typischerweise falsch liegen:
    /// rbp/r13 als Basis KANN kein disp0, weil mod=00 rm=101 fuer RIP reserviert
    /// ist. Also wird daraus zwangsweise disp8 mit dem Wert 0.
    fn modus(&self) -> u8 {
        if self.rip { return 0; }
        let b = match self.basis {
            None => return 0,          // nur Index -> disp32, mod=00
            Some(b) => b,
        };
        let erzwungen = b.tief() == 5; // rbp (5) und r13 (13) -> tief()==5
        if self.disp == 0 && !erzwungen {
            0
        } else if self.disp >= -128 && self.disp <= 127 {
            1
        } else {
            2
        }
    }

    /// Braucht die Adresse ein SIB-Oktett?
    /// Ja, sobald ein Index da ist -- und auch ohne Index, wenn die Basis
    /// rsp/r12 ist, denn rm=100 bedeutet "SIB folgt".
    fn braucht_sib(&self) -> bool {
        if self.rip { return false; }
        if self.index.is_some() { return true; }
        match self.basis {
            None => true,
            Some(b) => b.tief() == 4, // rsp (4) und r12 (12)
        }
    }
}

// ---------------------------------------------------------------- Puffer ----

/// Sammelt die Oktette und merkt sich Stellen, die spaeter gefuellt werden.
#[derive(Default)]
pub struct Puffer {
    pub okt: Vec<u8>,
}

impl Puffer {
    pub fn neu() -> Puffer { Puffer { okt: Vec::new() } }
    pub fn b(&mut self, v: u8) { self.okt.push(v); }
    pub fn w(&mut self, v: u16) { self.okt.extend_from_slice(&v.to_le_bytes()); }
    pub fn d(&mut self, v: u32) { self.okt.extend_from_slice(&v.to_le_bytes()); }
    pub fn q(&mut self, v: u64) { self.okt.extend_from_slice(&v.to_le_bytes()); }
    pub fn len(&self) -> usize { self.okt.len() }
    pub fn hex(&self) -> String {
        self.okt.iter().map(|x| format!("{:02x}", x)).collect()
    }

    /// REX-Praefix, wenn noetig. `acht_bit_quelle` erzwingt REX auch ohne
    /// gesetzte Bits: spl/bpl/sil/dil sind nur MIT REX erreichbar, ohne REX
    /// bedeuten dieselben Nummern ah/ch/dh/bh.
    fn rex(&mut self, w: bool, r: u8, x: u8, b: u8, erzwingen: bool) {
        let v = 0x40 | ((w as u8) << 3) | (r << 2) | (x << 1) | b;
        if w || r != 0 || x != 0 || b != 0 || erzwingen {
            self.b(v);
        }
    }

    /// ModRM + SIB + Verschiebung fuer einen Speicheroperanden.
    /// `reg` ist das Regfeld (schon auf 3 Bit gekuerzt).
    fn modrm_mem(&mut self, reg: u8, m: &Mem) {
        if m.rip {
            // mod=00, rm=101 -> RIP-relativ, immer disp32
            self.b((reg & 7) << 3 | 5);
            self.d(m.disp as u32);
            return;
        }
        let modus = m.modus();
        if m.braucht_sib() {
            self.b((modus << 6) | ((reg & 7) << 3) | 4);
            let skala_bits = match m.skala { 1 => 0u8, 2 => 1, 4 => 2, 8 => 3, _ => 0 };
            // Kein Index -> Index-Feld 100 (= "keiner")
            let idx = m.index.map(|i| i.tief()).unwrap_or(4);
            let bas = m.basis.map(|b| b.tief()).unwrap_or(5);
            self.b((skala_bits << 6) | (idx << 3) | bas);
            if m.basis.is_none() {
                self.d(m.disp as u32);        // mod=00 + base=101 -> disp32
                return;
            }
        } else {
            let bas = m.basis.map(|b| b.tief()).unwrap_or(5);
            self.b((modus << 6) | ((reg & 7) << 3) | bas);
        }
        match modus {
            1 => self.b(m.disp as i8 as u8),
            2 => self.d(m.disp as u32),
            _ => {}
        }
    }

    fn modrm_reg(&mut self, reg: u8, rm: u8) {
        self.b(0xc0 | ((reg & 7) << 3) | (rm & 7));
    }
}

/// Braucht dieses 8-Bit-Register ein REX-Praefix, um erreichbar zu sein?
fn acht_bit_braucht_rex(r: Reg) -> bool {
    matches!(r, Reg::Rsp | Reg::Rbp | Reg::Rsi | Reg::Rdi)
}

// ------------------------------------------------------------- Befehle ------

pub struct X86;

impl X86 {
    fn praefix_breite(p: &mut Puffer, br: Breite) {
        if br.op16() { p.b(0x66); }
    }

    /// Gemeinsamer Rumpf fuer Befehle der Form `op reg, rm` bzw. `op rm, reg`.
    fn rr(p: &mut Puffer, opcode: u8, br: Breite, reg: Reg, rm: Reg) {
        Self::praefix_breite(p, br);
        let erz = br == Breite::B8 && (acht_bit_braucht_rex(reg) || acht_bit_braucht_rex(rm));
        p.rex(br.rex_w(), reg.hoch(), 0, rm.hoch(), erz);
        p.b(opcode);
        p.modrm_reg(reg.tief(), rm.tief());
    }

    fn rm(p: &mut Puffer, opcode: u8, br: Breite, reg: Reg, m: &Mem) {
        Self::praefix_breite(p, br);
        let erz = br == Breite::B8 && acht_bit_braucht_rex(reg);
        let x = m.index.map(|i| i.hoch()).unwrap_or(0);
        let b = m.basis.map(|b| b.hoch()).unwrap_or(0);
        p.rex(br.rex_w(), reg.hoch(), x, b, erz);
        p.b(opcode);
        p.modrm_mem(reg.tief(), m);
    }

    // ---- mov ---------------------------------------------------------------

    /// `mov rZiel, rQuelle` -- kodiert als 89 /r (rm <- reg), wie GNU as es tut.
    pub fn mov_rr(p: &mut Puffer, br: Breite, ziel: Reg, quelle: Reg) {
        let op = if br == Breite::B8 { 0x88 } else { 0x89 };
        Self::rr(p, op, br, quelle, ziel);
    }

    /// `mov rZiel, [mem]` -- Laden, 8B /r.
    pub fn mov_r_m(p: &mut Puffer, br: Breite, ziel: Reg, m: &Mem) {
        let op = if br == Breite::B8 { 0x8a } else { 0x8b };
        Self::rm(p, op, br, ziel, m);
    }

    /// `mov [mem], rQuelle` -- Speichern, 89 /r.
    pub fn mov_m_r(p: &mut Puffer, br: Breite, m: &Mem, quelle: Reg) {
        let op = if br == Breite::B8 { 0x88 } else { 0x89 };
        Self::rm(p, op, br, quelle, m);
    }

    /// `mov rZiel, imm`. Der interessante Fall: passt der Wert in 32 Bit,
    /// nimmt GNU as bei 64-Bit-Zielen trotzdem B8+r mit imm32 -- ABER nur,
    /// wenn der Wert vorzeichenlos hineinpasst; sonst REX.W B8+r mit imm64.
    /// Fuer negative Werte, die in 32 Bit passen, waehlt as C7 /0 mit imm32
    /// (vorzeichenerweitert), weil das kuerzer ist.
    pub fn mov_r_imm(p: &mut Puffer, br: Breite, ziel: Reg, wert: i64) {
        match br {
            Breite::B8 => {
                let erz = acht_bit_braucht_rex(ziel);
                p.rex(false, 0, 0, ziel.hoch(), erz);
                p.b(0xb0 + ziel.tief());
                p.b(wert as u8);
            }
            Breite::B16 => {
                p.b(0x66);
                p.rex(false, 0, 0, ziel.hoch(), false);
                p.b(0xb8 + ziel.tief());
                p.w(wert as u16);
            }
            Breite::B32 => {
                p.rex(false, 0, 0, ziel.hoch(), false);
                p.b(0xb8 + ziel.tief());
                p.d(wert as u32);
            }
            Breite::B64 => {
                if wert >= i32::MIN as i64 && wert <= i32::MAX as i64 {
                    // C7 /0 mit vorzeichenerweitertem imm32 -- 7 Oktette
                    p.rex(true, 0, 0, ziel.hoch(), false);
                    p.b(0xc7);
                    p.modrm_reg(0, ziel.tief());
                    p.d(wert as i32 as u32);
                } else if wert >= 0 && wert <= u32::MAX as i64 {
                    // passt vorzeichenlos in 32 Bit -> B8+r ohne REX.W, 5-6 Oktette
                    p.rex(false, 0, 0, ziel.hoch(), false);
                    p.b(0xb8 + ziel.tief());
                    p.d(wert as u32);
                } else {
                    // echtes movabs
                    p.rex(true, 0, 0, ziel.hoch(), false);
                    p.b(0xb8 + ziel.tief());
                    p.q(wert as u64);
                }
            }
        }
    }

    /// `mov [mem], imm` -- C7 /0 (bzw. C6 /0 fuer 8 Bit).
    pub fn mov_m_imm(p: &mut Puffer, br: Breite, m: &Mem, wert: i64) {
        Self::praefix_breite(p, br);
        let x = m.index.map(|i| i.hoch()).unwrap_or(0);
        let b = m.basis.map(|b| b.hoch()).unwrap_or(0);
        p.rex(br.rex_w(), 0, x, b, false);
        p.b(if br == Breite::B8 { 0xc6 } else { 0xc7 });
        p.modrm_mem(0, m);
        match br {
            Breite::B8 => p.b(wert as u8),
            Breite::B16 => p.w(wert as u16),
            _ => p.d(wert as u32),
        }
    }

    /// `lea rZiel, [mem]` -- 8D /r, nie mit Groessenpraefix am Speicher.
    pub fn lea(p: &mut Puffer, br: Breite, ziel: Reg, m: &Mem) {
        Self::rm(p, 0x8d, br, ziel, m);
    }

    // ---- Rechnen: add/or/adc/sbb/and/sub/xor/cmp -----------------------------
    // Diese acht teilen sich ein Schema. Die Zahl ist das /digit fuer die
    // Immediate-Form und bestimmt zugleich den Opcode der Registerform.

    fn alu_nr(name: &str) -> Option<u8> {
        Some(match name {
            "add" => 0, "or" => 1, "adc" => 2, "sbb" => 3,
            "and" => 4, "sub" => 5, "xor" => 6, "cmp" => 7,
            _ => return None,
        })
    }

    /// `<alu> rm, reg` -- Opcode 00+8*nr (8 Bit) bzw. 01+8*nr.
    pub fn alu_rr(p: &mut Puffer, nr: u8, br: Breite, ziel: Reg, quelle: Reg) {
        let op = if br == Breite::B8 { nr * 8 } else { nr * 8 + 1 };
        Self::rr(p, op, br, quelle, ziel);
    }

    /// `<alu> reg, [mem]` -- Opcode 02+8*nr bzw. 03+8*nr.
    pub fn alu_r_m(p: &mut Puffer, nr: u8, br: Breite, ziel: Reg, m: &Mem) {
        let op = if br == Breite::B8 { nr * 8 + 2 } else { nr * 8 + 3 };
        Self::rm(p, op, br, ziel, m);
    }

    /// `<alu> [mem], reg`.
    pub fn alu_m_r(p: &mut Puffer, nr: u8, br: Breite, m: &Mem, quelle: Reg) {
        let op = if br == Breite::B8 { nr * 8 } else { nr * 8 + 1 };
        Self::rm(p, op, br, quelle, m);
    }

    /// `<alu> reg, imm`. Passt der Wert in ein Oktett, nimmt as 83 /nr mit
    /// imm8 -- ausser bei 8-Bit-Operanden, wo 80 /nr steht.
    pub fn alu_r_imm(p: &mut Puffer, nr: u8, br: Breite, ziel: Reg, wert: i64) {
        // Der Sammler (al/ax/eax/rax) hat eine eigene, kuerzere Kodierung ohne
        // ModRM: 04+8*nr fuer 8 Bit, 05+8*nr sonst. GNU as nimmt sie immer,
        // wenn sie passt -- bei 8 Bit also stets, sonst nur wenn der Wert
        // NICHT in ein Oktett passt (sonst waere 83 /nr kuerzer).
        let ist_sammler = ziel == Reg::Rax;
        let passt_in_imm8 = wert >= -128 && wert <= 127;
        if ist_sammler && (br == Breite::B8 || !passt_in_imm8) {
            Self::praefix_breite(p, br);
            p.rex(br.rex_w(), 0, 0, 0, false);
            if br == Breite::B8 {
                p.b(nr * 8 + 4);
                p.b(wert as u8);
            } else {
                p.b(nr * 8 + 5);
                if br == Breite::B16 { p.w(wert as u16); } else { p.d(wert as u32); }
            }
            return;
        }
        Self::praefix_breite(p, br);
        let erz = br == Breite::B8 && acht_bit_braucht_rex(ziel);
        p.rex(br.rex_w(), 0, 0, ziel.hoch(), erz);
        if br == Breite::B8 {
            p.b(0x80);
            p.modrm_reg(nr, ziel.tief());
            p.b(wert as u8);
        } else if wert >= -128 && wert <= 127 {
            p.b(0x83);
            p.modrm_reg(nr, ziel.tief());
            p.b(wert as i8 as u8);
        } else {
            p.b(0x81);
            p.modrm_reg(nr, ziel.tief());
            if br == Breite::B16 { p.w(wert as u16); } else { p.d(wert as u32); }
        }
    }

    /// `<alu> [mem], imm`.
    pub fn alu_m_imm(p: &mut Puffer, nr: u8, br: Breite, m: &Mem, wert: i64) {
        Self::praefix_breite(p, br);
        let x = m.index.map(|i| i.hoch()).unwrap_or(0);
        let b = m.basis.map(|b| b.hoch()).unwrap_or(0);
        p.rex(br.rex_w(), 0, x, b, false);
        if br == Breite::B8 {
            p.b(0x80);
            p.modrm_mem(nr, m);
            p.b(wert as u8);
        } else if wert >= -128 && wert <= 127 {
            p.b(0x83);
            p.modrm_mem(nr, m);
            p.b(wert as i8 as u8);
        } else {
            p.b(0x81);
            p.modrm_mem(nr, m);
            if br == Breite::B16 { p.w(wert as u16); } else { p.d(wert as u32); }
        }
    }

    // ---- test --------------------------------------------------------------

    pub fn test_rr(p: &mut Puffer, br: Breite, a: Reg, b: Reg) {
        let op = if br == Breite::B8 { 0x84 } else { 0x85 };
        // test rm, reg -- as schreibt den ZWEITEN Operanden ins reg-Feld
        Self::rr(p, op, br, b, a);
    }

    // ---- Schieben ----------------------------------------------------------

    fn schiebe_nr(name: &str) -> Option<u8> {
        Some(match name {
            "rol" => 0, "ror" => 1, "rcl" => 2, "rcr" => 3,
            "shl" => 4, "shr" => 5, "sal" => 4, "sar" => 7,
            _ => return None,
        })
    }

    /// `<shift> reg, imm8`. Bei 1 nimmt as die kurze Form D1 /nr.
    pub fn shift_r_imm(p: &mut Puffer, nr: u8, br: Breite, ziel: Reg, wert: u8) {
        Self::praefix_breite(p, br);
        let erz = br == Breite::B8 && acht_bit_braucht_rex(ziel);
        p.rex(br.rex_w(), 0, 0, ziel.hoch(), erz);
        if wert == 1 {
            p.b(if br == Breite::B8 { 0xd0 } else { 0xd1 });
            p.modrm_reg(nr, ziel.tief());
        } else {
            p.b(if br == Breite::B8 { 0xc0 } else { 0xc1 });
            p.modrm_reg(nr, ziel.tief());
            p.b(wert);
        }
    }

    /// `<shift> reg, cl` -- D3 /nr.
    pub fn shift_r_cl(p: &mut Puffer, nr: u8, br: Breite, ziel: Reg) {
        Self::praefix_breite(p, br);
        let erz = br == Breite::B8 && acht_bit_braucht_rex(ziel);
        p.rex(br.rex_w(), 0, 0, ziel.hoch(), erz);
        p.b(if br == Breite::B8 { 0xd2 } else { 0xd3 });
        p.modrm_reg(nr, ziel.tief());
    }

    // ---- Einoperanden-Gruppe F7 -------------------------------------------

    fn f7_nr(name: &str) -> Option<u8> {
        Some(match name {
            "test" => 0, "not" => 2, "neg" => 3,
            "mul" => 4, "imul" => 5, "div" => 6, "idiv" => 7,
            _ => return None,
        })
    }

    pub fn f7_r(p: &mut Puffer, nr: u8, br: Breite, ziel: Reg) {
        Self::praefix_breite(p, br);
        let erz = br == Breite::B8 && acht_bit_braucht_rex(ziel);
        p.rex(br.rex_w(), 0, 0, ziel.hoch(), erz);
        p.b(if br == Breite::B8 { 0xf6 } else { 0xf7 });
        p.modrm_reg(nr, ziel.tief());
    }

    /// inc/dec -- FF /0 bzw. FF /1. (Die kurze 40+r-Form gibt es in 64 Bit nicht,
    /// dort ist 0x40..0x4f das REX-Praefix.)
    pub fn inc_dec_r(p: &mut Puffer, dec: bool, br: Breite, ziel: Reg) {
        Self::praefix_breite(p, br);
        let erz = br == Breite::B8 && acht_bit_braucht_rex(ziel);
        p.rex(br.rex_w(), 0, 0, ziel.hoch(), erz);
        p.b(if br == Breite::B8 { 0xfe } else { 0xff });
        p.modrm_reg(if dec { 1 } else { 0 }, ziel.tief());
    }

    pub fn inc_dec_m(p: &mut Puffer, dec: bool, br: Breite, m: &Mem) {
        Self::praefix_breite(p, br);
        let x = m.index.map(|i| i.hoch()).unwrap_or(0);
        let b = m.basis.map(|b| b.hoch()).unwrap_or(0);
        p.rex(br.rex_w(), 0, x, b, false);
        p.b(if br == Breite::B8 { 0xfe } else { 0xff });
        p.modrm_mem(if dec { 1 } else { 0 }, m);
    }

    // ---- imul mit zwei/drei Operanden --------------------------------------

    /// `imul rZiel, rQuelle` -- 0F AF /r.
    pub fn imul_rr(p: &mut Puffer, br: Breite, ziel: Reg, quelle: Reg) {
        Self::praefix_breite(p, br);
        p.rex(br.rex_w(), ziel.hoch(), 0, quelle.hoch(), false);
        p.b(0x0f); p.b(0xaf);
        p.modrm_reg(ziel.tief(), quelle.tief());
    }

    pub fn imul_r_m(p: &mut Puffer, br: Breite, ziel: Reg, m: &Mem) {
        Self::praefix_breite(p, br);
        let x = m.index.map(|i| i.hoch()).unwrap_or(0);
        let b = m.basis.map(|b| b.hoch()).unwrap_or(0);
        p.rex(br.rex_w(), ziel.hoch(), x, b, false);
        p.b(0x0f); p.b(0xaf);
        p.modrm_mem(ziel.tief(), m);
    }

    /// `imul rZiel, rQuelle, imm` -- 6B /r imm8 oder 69 /r imm32.
    pub fn imul_rri(p: &mut Puffer, br: Breite, ziel: Reg, quelle: Reg, wert: i64) {
        Self::praefix_breite(p, br);
        p.rex(br.rex_w(), ziel.hoch(), 0, quelle.hoch(), false);
        if wert >= -128 && wert <= 127 {
            p.b(0x6b);
            p.modrm_reg(ziel.tief(), quelle.tief());
            p.b(wert as i8 as u8);
        } else {
            p.b(0x69);
            p.modrm_reg(ziel.tief(), quelle.tief());
            if br == Breite::B16 { p.w(wert as u16); } else { p.d(wert as u32); }
        }
    }

    pub fn imul_rmi(p: &mut Puffer, br: Breite, ziel: Reg, m: &Mem, wert: i64) {
        Self::praefix_breite(p, br);
        let x = m.index.map(|i| i.hoch()).unwrap_or(0);
        let b = m.basis.map(|b| b.hoch()).unwrap_or(0);
        p.rex(br.rex_w(), ziel.hoch(), x, b, false);
        if wert >= -128 && wert <= 127 {
            p.b(0x6b);
            p.modrm_mem(ziel.tief(), m);
            p.b(wert as i8 as u8);
        } else {
            p.b(0x69);
            p.modrm_mem(ziel.tief(), m);
            p.d(wert as u32);
        }
    }

    // ---- Erweitern ---------------------------------------------------------

    /// `movzx rZiel, rQuelle` -- 0F B6 (aus 8 Bit) / 0F B7 (aus 16 Bit).
    pub fn movzx_rr(p: &mut Puffer, ziel_br: Breite, quell_br: Breite, ziel: Reg, quelle: Reg) {
        Self::praefix_breite(p, ziel_br);
        let erz = quell_br == Breite::B8 && acht_bit_braucht_rex(quelle);
        p.rex(ziel_br.rex_w(), ziel.hoch(), 0, quelle.hoch(), erz);
        p.b(0x0f);
        p.b(if quell_br == Breite::B8 { 0xb6 } else { 0xb7 });
        p.modrm_reg(ziel.tief(), quelle.tief());
    }

    pub fn movzx_r_m(p: &mut Puffer, ziel_br: Breite, quell_br: Breite, ziel: Reg, m: &Mem) {
        Self::praefix_breite(p, ziel_br);
        let x = m.index.map(|i| i.hoch()).unwrap_or(0);
        let b = m.basis.map(|b| b.hoch()).unwrap_or(0);
        p.rex(ziel_br.rex_w(), ziel.hoch(), x, b, false);
        p.b(0x0f);
        p.b(if quell_br == Breite::B8 { 0xb6 } else { 0xb7 });
        p.modrm_mem(ziel.tief(), m);
    }

    /// `movsx` -- 0F BE / 0F BF.
    pub fn movsx_rr(p: &mut Puffer, ziel_br: Breite, quell_br: Breite, ziel: Reg, quelle: Reg) {
        Self::praefix_breite(p, ziel_br);
        let erz = quell_br == Breite::B8 && acht_bit_braucht_rex(quelle);
        p.rex(ziel_br.rex_w(), ziel.hoch(), 0, quelle.hoch(), erz);
        p.b(0x0f);
        p.b(if quell_br == Breite::B8 { 0xbe } else { 0xbf });
        p.modrm_reg(ziel.tief(), quelle.tief());
    }

    pub fn movsx_r_m(p: &mut Puffer, ziel_br: Breite, quell_br: Breite, ziel: Reg, m: &Mem) {
        Self::praefix_breite(p, ziel_br);
        let x = m.index.map(|i| i.hoch()).unwrap_or(0);
        let b = m.basis.map(|b| b.hoch()).unwrap_or(0);
        p.rex(ziel_br.rex_w(), ziel.hoch(), x, b, false);
        p.b(0x0f);
        p.b(if quell_br == Breite::B8 { 0xbe } else { 0xbf });
        p.modrm_mem(ziel.tief(), m);
    }

    /// `movsxd rZiel64, rQuelle32` -- 63 /r, immer mit REX.W.
    pub fn movsxd_rr(p: &mut Puffer, ziel: Reg, quelle: Reg) {
        p.rex(true, ziel.hoch(), 0, quelle.hoch(), false);
        p.b(0x63);
        p.modrm_reg(ziel.tief(), quelle.tief());
    }

    pub fn movsxd_r_m(p: &mut Puffer, ziel: Reg, m: &Mem) {
        let x = m.index.map(|i| i.hoch()).unwrap_or(0);
        let b = m.basis.map(|b| b.hoch()).unwrap_or(0);
        p.rex(true, ziel.hoch(), x, b, false);
        p.b(0x63);
        p.modrm_mem(ziel.tief(), m);
    }

    // ---- Stapel ------------------------------------------------------------

    /// `push r64` -- 50+r. REX nur wegen des hohen Bits, nie REX.W.
    pub fn push_r(p: &mut Puffer, r: Reg) {
        p.rex(false, 0, 0, r.hoch(), false);
        p.b(0x50 + r.tief());
    }

    pub fn pop_r(p: &mut Puffer, r: Reg) {
        p.rex(false, 0, 0, r.hoch(), false);
        p.b(0x58 + r.tief());
    }

    // ---- Bedingungen -------------------------------------------------------

    /// Bedingungsnummern (tttn) -- die Reihenfolge ist Architekturvorgabe.
    pub fn cc_nr(name: &str) -> Option<u8> {
        Some(match name {
            "o" => 0x0, "no" => 0x1,
            "b" | "c" | "nae" => 0x2,
            "ae" | "nb" | "nc" => 0x3,
            "e" | "z" => 0x4,
            "ne" | "nz" => 0x5,
            "be" | "na" => 0x6,
            "a" | "nbe" => 0x7,
            "s" => 0x8, "ns" => 0x9,
            "p" | "pe" => 0xa,
            "np" | "po" => 0xb,
            "l" | "nge" => 0xc,
            "ge" | "nl" => 0xd,
            "le" | "ng" => 0xe,
            "g" | "nle" => 0xf,
            _ => return None,
        })
    }

    /// `set<cc> r8` -- 0F 90+cc /0.
    pub fn setcc_r(p: &mut Puffer, cc: u8, ziel: Reg) {
        let erz = acht_bit_braucht_rex(ziel);
        p.rex(false, 0, 0, ziel.hoch(), erz);
        p.b(0x0f);
        p.b(0x90 + cc);
        p.modrm_reg(0, ziel.tief());
    }

    /// `cmov<cc> rZiel, rQuelle` -- 0F 40+cc /r.
    pub fn cmovcc_rr(p: &mut Puffer, cc: u8, br: Breite, ziel: Reg, quelle: Reg) {
        Self::praefix_breite(p, br);
        p.rex(br.rex_w(), ziel.hoch(), 0, quelle.hoch(), false);
        p.b(0x0f);
        p.b(0x40 + cc);
        p.modrm_reg(ziel.tief(), quelle.tief());
    }

    // ---- Spruenge ----------------------------------------------------------
    //
    // Die Sprungweite ist der Grund, warum ein Assembler zwei Durchlaeufe
    // braucht. `rel` ist der Abstand vom ENDE des Sprungbefehls zum Ziel --
    // und dieses Ende haengt davon ab, wie lang der Sprung wird. Deshalb
    // rechnet der Aufrufer mit der Laenge, die hier entsteht.

    /// `jmp rel32` -- E9. Immer die lange Form.
    pub fn jmp_rel32(p: &mut Puffer, rel: i32) {
        p.b(0xe9);
        p.d(rel as u32);
    }

    /// `jmp rel8` -- EB. Nur wenn der Abstand hineinpasst.
    pub fn jmp_rel8(p: &mut Puffer, rel: i8) {
        p.b(0xeb);
        p.b(rel as u8);
    }

    /// `j<cc> rel32` -- 0F 80+cc.
    pub fn jcc_rel32(p: &mut Puffer, cc: u8, rel: i32) {
        p.b(0x0f);
        p.b(0x80 + cc);
        p.d(rel as u32);
    }

    /// `j<cc> rel8` -- 70+cc.
    pub fn jcc_rel8(p: &mut Puffer, cc: u8, rel: i8) {
        p.b(0x70 + cc);
        p.b(rel as u8);
    }

    /// `call rel32` -- E8. Es gibt keine kurze Form.
    pub fn call_rel32(p: &mut Puffer, rel: i32) {
        p.b(0xe8);
        p.d(rel as u32);
    }

    /// `call r64` -- FF /2. Kein REX.W noetig, 64 Bit ist die Vorgabe.
    pub fn call_r(p: &mut Puffer, r: Reg) {
        p.rex(false, 0, 0, r.hoch(), false);
        p.b(0xff);
        p.modrm_reg(2, r.tief());
    }

    /// `jmp [mem]` -- FF /4.
    pub fn jmp_m(p: &mut Puffer, m: &Mem) {
        let x = m.index.map(|i| i.hoch()).unwrap_or(0);
        let b = m.basis.map(|b| b.hoch()).unwrap_or(0);
        p.rex(false, 0, x, b, false);
        p.b(0xff);
        p.modrm_mem(4, m);
    }

    // ---- ohne Operanden ----------------------------------------------------

    pub fn ret(p: &mut Puffer) { p.b(0xc3); }
    pub fn syscall(p: &mut Puffer) { p.b(0x0f); p.b(0x05); }
    pub fn hlt(p: &mut Puffer) { p.b(0xf4); }
    pub fn ud2(p: &mut Puffer) { p.b(0x0f); p.b(0x0b); }
    pub fn cld(p: &mut Puffer) { p.b(0xfc); }
    pub fn cdq(p: &mut Puffer) { p.b(0x99); }
    pub fn cqo(p: &mut Puffer) { p.b(0x48); p.b(0x99); }
    pub fn rep_movsb(p: &mut Puffer) { p.b(0xf3); p.b(0xa4); }
    pub fn rep_stosb(p: &mut Puffer) { p.b(0xf3); p.b(0xaa); }

    // ---- Atomares ----------------------------------------------------------

    /// `lock xadd [mem], reg` -- F0 0F C1 /r.
    pub fn lock_xadd_m_r(p: &mut Puffer, br: Breite, m: &Mem, quelle: Reg) {
        p.b(0xf0);
        Self::praefix_breite(p, br);
        let x = m.index.map(|i| i.hoch()).unwrap_or(0);
        let b = m.basis.map(|b| b.hoch()).unwrap_or(0);
        p.rex(br.rex_w(), quelle.hoch(), x, b, false);
        p.b(0x0f); p.b(0xc1);
        p.modrm_mem(quelle.tief(), m);
    }

    /// `lock cmpxchg [mem], reg` -- F0 0F B1 /r.
    pub fn lock_cmpxchg_m_r(p: &mut Puffer, br: Breite, m: &Mem, quelle: Reg) {
        p.b(0xf0);
        Self::praefix_breite(p, br);
        let x = m.index.map(|i| i.hoch()).unwrap_or(0);
        let b = m.basis.map(|b| b.hoch()).unwrap_or(0);
        p.rex(br.rex_w(), quelle.hoch(), x, b, false);
        p.b(0x0f); p.b(0xb1);
        p.modrm_mem(quelle.tief(), m);
    }

    // ---- Gleitkomma --------------------------------------------------------
    //
    // Diese Befehle tragen einen Praefix, der VOR dem REX steht: F2 fuer
    // doppelte, F3 fuer einfache Genauigkeit, 66 fuer die ganzzahlige
    // Bewegung. Das ist eine haeufige Fehlerquelle -- REX muss immer direkt
    // vor dem Opcode stehen, also NACH diesem Praefix.

    fn sse_rr(p: &mut Puffer, praefix: Option<u8>, opcode: u8, w: bool, a: u8, ah: u8, b: u8, bh: u8) {
        if let Some(pf) = praefix { p.b(pf); }
        p.rex(w, ah, 0, bh, false);
        p.b(0x0f);
        p.b(opcode);
        p.b(0xc0 | ((a & 7) << 3) | (b & 7));
    }

    pub fn addsd(p: &mut Puffer, a: Xmm, b: Xmm) { Self::sse_rr(p, Some(0xf2), 0x58, false, a.tief(), a.hoch(), b.tief(), b.hoch()); }
    pub fn subsd(p: &mut Puffer, a: Xmm, b: Xmm) { Self::sse_rr(p, Some(0xf2), 0x5c, false, a.tief(), a.hoch(), b.tief(), b.hoch()); }
    pub fn mulsd(p: &mut Puffer, a: Xmm, b: Xmm) { Self::sse_rr(p, Some(0xf2), 0x59, false, a.tief(), a.hoch(), b.tief(), b.hoch()); }
    pub fn divsd(p: &mut Puffer, a: Xmm, b: Xmm) { Self::sse_rr(p, Some(0xf2), 0x5e, false, a.tief(), a.hoch(), b.tief(), b.hoch()); }
    pub fn addss(p: &mut Puffer, a: Xmm, b: Xmm) { Self::sse_rr(p, Some(0xf3), 0x58, false, a.tief(), a.hoch(), b.tief(), b.hoch()); }
    pub fn subss(p: &mut Puffer, a: Xmm, b: Xmm) { Self::sse_rr(p, Some(0xf3), 0x5c, false, a.tief(), a.hoch(), b.tief(), b.hoch()); }
    pub fn mulss(p: &mut Puffer, a: Xmm, b: Xmm) { Self::sse_rr(p, Some(0xf3), 0x59, false, a.tief(), a.hoch(), b.tief(), b.hoch()); }
    pub fn divss(p: &mut Puffer, a: Xmm, b: Xmm) { Self::sse_rr(p, Some(0xf3), 0x5e, false, a.tief(), a.hoch(), b.tief(), b.hoch()); }
    pub fn ucomisd(p: &mut Puffer, a: Xmm, b: Xmm) { Self::sse_rr(p, Some(0x66), 0x2e, false, a.tief(), a.hoch(), b.tief(), b.hoch()); }
    pub fn ucomiss(p: &mut Puffer, a: Xmm, b: Xmm) { Self::sse_rr(p, None, 0x2e, false, a.tief(), a.hoch(), b.tief(), b.hoch()); }
    pub fn cvtss2sd(p: &mut Puffer, a: Xmm, b: Xmm) { Self::sse_rr(p, Some(0xf3), 0x5a, false, a.tief(), a.hoch(), b.tief(), b.hoch()); }
    pub fn cvtsd2ss(p: &mut Puffer, a: Xmm, b: Xmm) { Self::sse_rr(p, Some(0xf2), 0x5a, false, a.tief(), a.hoch(), b.tief(), b.hoch()); }

    /// `movq xmm, r64` -- 66 REX.W 0F 6E /r.
    pub fn movq_x_r(p: &mut Puffer, x: Xmm, r: Reg) {
        Self::sse_rr(p, Some(0x66), 0x6e, true, x.tief(), x.hoch(), r.tief(), r.hoch());
    }
    /// `movq r64, xmm` -- 66 REX.W 0F 7E /r.
    pub fn movq_r_x(p: &mut Puffer, r: Reg, x: Xmm) {
        Self::sse_rr(p, Some(0x66), 0x7e, true, x.tief(), x.hoch(), r.tief(), r.hoch());
    }
    /// `movd xmm, r32` -- 66 0F 6E /r.
    pub fn movd_x_r(p: &mut Puffer, x: Xmm, r: Reg) {
        Self::sse_rr(p, Some(0x66), 0x6e, false, x.tief(), x.hoch(), r.tief(), r.hoch());
    }
    pub fn movd_r_x(p: &mut Puffer, r: Reg, x: Xmm) {
        Self::sse_rr(p, Some(0x66), 0x7e, false, x.tief(), x.hoch(), r.tief(), r.hoch());
    }

    /// `cvtsi2sd xmm, r64` -- F2 REX.W 0F 2A /r.
    pub fn cvtsi2sd(p: &mut Puffer, br: Breite, x: Xmm, r: Reg) {
        Self::sse_rr(p, Some(0xf2), 0x2a, br.rex_w(), x.tief(), x.hoch(), r.tief(), r.hoch());
    }
    pub fn cvtsi2ss(p: &mut Puffer, br: Breite, x: Xmm, r: Reg) {
        Self::sse_rr(p, Some(0xf3), 0x2a, br.rex_w(), x.tief(), x.hoch(), r.tief(), r.hoch());
    }
    /// `cvttsd2si r64, xmm` -- F2 REX.W 0F 2C /r.
    pub fn cvttsd2si(p: &mut Puffer, br: Breite, r: Reg, x: Xmm) {
        Self::sse_rr(p, Some(0xf2), 0x2c, br.rex_w(), r.tief(), r.hoch(), x.tief(), x.hoch());
    }
    pub fn cvttss2si(p: &mut Puffer, br: Breite, r: Reg, x: Xmm) {
        Self::sse_rr(p, Some(0xf3), 0x2c, br.rex_w(), r.tief(), r.hoch(), x.tief(), x.hoch());
    }
}

// ------------------------------------------------------------ Textweg -------
//
// Damit sich die Kodierung gegen GNU as pruefen laesst, muss derselbe Text,
// den codegen_x86.rs heute ausgibt, hier hineingehen. Dieser Teil ist
// AUSDRUECKLICH nur fuer die Abnahme da -- der Uebersetzer wird spaeter direkt
// die Funktionen oben aufrufen, ohne den Umweg ueber Text.

pub mod text {
    use super::*;

    pub fn reg_aus_name(s: &str) -> Option<(Reg, Breite)> {
        use Reg::*;
        let t = s.trim();
        let r64 = [("rax", Rax), ("rcx", Rcx), ("rdx", Rdx), ("rbx", Rbx),
                   ("rsp", Rsp), ("rbp", Rbp), ("rsi", Rsi), ("rdi", Rdi),
                   ("r8", R8), ("r9", R9), ("r10", R10), ("r11", R11),
                   ("r12", R12), ("r13", R13), ("r14", R14), ("r15", R15)];
        for (n, r) in r64 { if t == n { return Some((r, Breite::B64)); } }
        let r32 = [("eax", Rax), ("ecx", Rcx), ("edx", Rdx), ("ebx", Rbx),
                   ("esp", Rsp), ("ebp", Rbp), ("esi", Rsi), ("edi", Rdi),
                   ("r8d", R8), ("r9d", R9), ("r10d", R10), ("r11d", R11),
                   ("r12d", R12), ("r13d", R13), ("r14d", R14), ("r15d", R15)];
        for (n, r) in r32 { if t == n { return Some((r, Breite::B32)); } }
        let r16 = [("ax", Rax), ("cx", Rcx), ("dx", Rdx), ("bx", Rbx),
                   ("sp", Rsp), ("bp", Rbp), ("si", Rsi), ("di", Rdi),
                   ("r8w", R8), ("r9w", R9), ("r10w", R10), ("r11w", R11),
                   ("r12w", R12), ("r13w", R13), ("r14w", R14), ("r15w", R15)];
        for (n, r) in r16 { if t == n { return Some((r, Breite::B16)); } }
        let r8 = [("al", Rax), ("cl", Rcx), ("dl", Rdx), ("bl", Rbx),
                  ("spl", Rsp), ("bpl", Rbp), ("sil", Rsi), ("dil", Rdi),
                  ("r8b", R8), ("r9b", R9), ("r10b", R10), ("r11b", R11),
                  ("r12b", R12), ("r13b", R13), ("r14b", R14), ("r15b", R15)];
        for (n, r) in r8 { if t == n { return Some((r, Breite::B8)); } }
        None
    }

    pub fn xmm_aus_name(s: &str) -> Option<Xmm> {
        let t = s.trim();
        if let Some(rest) = t.strip_prefix("xmm") {
            if let Ok(n) = rest.parse::<u8>() {
                if n < 16 { return Some(Xmm(n)); }
            }
        }
        None
    }

    /// Zahl lesen -- dezimal, auch negativ, auch hexadezimal.
    pub fn zahl(s: &str) -> Option<i64> {
        let t = s.trim();
        if let Some(h) = t.strip_prefix("0x") { return i64::from_str_radix(h, 16).ok(); }
        if let Some(h) = t.strip_prefix("-0x") {
            return i64::from_str_radix(h, 16).ok().map(|v| -v);
        }
        t.parse::<i64>().ok()
    }

    /// Einen Speicheroperanden wie `qword ptr [rax+rcx*8+16]` zerlegen.
    pub fn mem_aus_text(s: &str) -> Option<(Mem, Option<Breite>)> {
        let t = s.trim();
        let mut br = None;
        let mut rest = t;
        for (n, b) in [("byte ptr", Breite::B8), ("word ptr", Breite::B16),
                       ("dword ptr", Breite::B32), ("qword ptr", Breite::B64)] {
            if let Some(r) = rest.strip_prefix(n) { br = Some(b); rest = r.trim(); break; }
        }
        let innen = rest.strip_prefix('[')?.strip_suffix(']')?;
        if let Some(r) = innen.trim().strip_prefix("rip") {
            // RIP-relativ: der Wert steht erst nach dem Binden fest
            let _ = r;
            return Some((Mem::rip(0), br));
        }
        let mut basis = None;
        let mut index = None;
        let mut skala = 1u8;
        let mut disp = 0i64;
        // in Summanden zerlegen, Vorzeichen mitnehmen
        let mut teile: Vec<(i8, String)> = Vec::new();
        let mut akt = String::new();
        let mut vz = 1i8;
        for c in innen.chars() {
            if c == '+' || c == '-' {
                if !akt.trim().is_empty() { teile.push((vz, akt.trim().to_string())); }
                akt.clear();
                vz = if c == '-' { -1 } else { 1 };
            } else {
                akt.push(c);
            }
        }
        if !akt.trim().is_empty() { teile.push((vz, akt.trim().to_string())); }
        for (v, teil) in teile {
            if teil.contains('*') {
                let mut it = teil.split('*');
                let rn = it.next()?.trim();
                let sn = it.next()?.trim();
                index = Some(reg_aus_name(rn)?.0);
                skala = sn.parse::<u8>().ok()?;
            } else if let Some((r, _)) = reg_aus_name(&teil) {
                if basis.is_none() { basis = Some(r); } else { index = Some(r); }
            } else {
                disp += (v as i64) * zahl(&teil)?;
            }
        }
        Some((Mem { basis, index, skala, disp: disp as i32, rip: false }, br))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(f: impl Fn(&mut Puffer)) -> String {
        let mut p = Puffer::neu();
        f(&mut p);
        p.hex()
    }

    #[test]
    fn mov_register() {
        // Die Grundform, gegen GNU as geprueft.
        assert_eq!(hex(|p| X86::mov_rr(p, Breite::B64, Reg::Rax, Reg::Rcx)), "4889c8");
        assert_eq!(hex(|p| X86::mov_rr(p, Breite::B64, Reg::R12, Reg::R13)), "4d89ec");
    }

    #[test]
    fn speicher_sonderfaelle() {
        // rsp als Basis erzwingt ein SIB-Oktett (24).
        assert_eq!(hex(|p| X86::mov_r_m(p, Breite::B64, Reg::Rax, &Mem::basis(Reg::Rsp))),
                   "488b0424");
        // rbp als Basis kann kein disp0 -- wird zu disp8 mit Wert 0.
        assert_eq!(hex(|p| X86::mov_r_m(p, Breite::B64, Reg::Rax, &Mem::basis(Reg::Rbp))),
                   "488b4500");
    }

    #[test]
    fn acht_bit_register_brauchen_rex() {
        // sil ist ohne REX gar nicht erreichbar -- ohne REX waere es dh.
        let mit = hex(|p| X86::mov_rr(p, Breite::B8, Reg::Rsi, Reg::Rax));
        assert!(mit.starts_with("40"), "sil braucht REX, bekam: {}", mit);
    }
}
