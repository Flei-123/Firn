// SPDX-License-Identifier: GPL-2.0-only
//! Binaerkodierung fuer ARM64 (AArch64).
//!
//! Jeder Befehl ist genau vier Oktette lang -- das macht die Sache einerseits
//! einfacher als bei x86 (keine Praefixe, keine variable Laenge), andererseits
//! schwerer: die Felder sind fest verdrahtete Bitbereiche, und manche Werte
//! passen nur in einer besonderen Darstellung hinein.
//!
//! Die zwei Stellen, an denen es wirklich knifflig wird:
//!
//!   1. Grosse Konstanten. Es gibt kein Feld fuer 64 Bit. Eine Konstante wird
//!      aus MOVZ/MOVK in 16-Bit-Stuecken zusammengesetzt -- und MOVN deckt die
//!      Faelle ab, in denen das Komplement kuerzer ist.
//!
//!   2. Bitmuster-Konstanten fuer and/orr/eor/tst. Das ist KEIN Zahlenfeld,
//!      sondern eine Beschreibung eines sich wiederholenden Bitmusters aus
//!      (N, immr, imms). Nur Werte, die sich so beschreiben lassen, sind
//!      ueberhaupt zulaessig -- 4095 geht, 4097 nicht.

#![allow(dead_code)]

/// Ein Wort ist immer 32 Bit.
pub type Wort = u32;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Grosse { W, X }

impl Grosse {
    fn sf(self) -> u32 { if self == Grosse::X { 1 } else { 0 } }
}

/// Registernummer 0..31. 31 bedeutet je nach Befehl xzr/wzr oder sp.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct R(pub u32);

/// Gleitkommaregister.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct V(pub u32);

pub const ZR: R = R(31);
pub const SP: R = R(31);

/// Bedingungscodes -- Reihenfolge ist Architekturvorgabe.
pub fn cond_nr(s: &str) -> Option<u32> {
    Some(match s {
        "eq" => 0, "ne" => 1,
        "cs" | "hs" => 2,
        "cc" | "lo" => 3,
        "mi" => 4, "pl" => 5,
        "vs" => 6, "vc" => 7,
        "hi" => 8, "ls" => 9,
        "ge" => 10, "lt" => 11,
        "gt" => 12, "le" => 13,
        "al" => 14, "nv" => 15,
        _ => return None,
    })
}

pub struct A64;

impl A64 {
    // ---- Rechnen mit Sofortwert -------------------------------------------

    /// `add/sub Xd, Xn, #imm{, lsl #12}` -- Feld ist 12 Bit, dazu ein Schalter
    /// fuer die Verschiebung um 12 Stellen. Groessere Werte gehen NICHT.
    pub fn addsub_imm(g: Grosse, sub: bool, setflags: bool,
                      rd: R, rn: R, imm: u32, shift12: bool) -> Option<Wort> {
        if imm > 0xfff { return None; }
        let mut w: u32 = 0;
        w |= g.sf() << 31;
        w |= (sub as u32) << 30;
        w |= (setflags as u32) << 29;
        w |= 0b100010 << 23;
        w |= (shift12 as u32) << 22;
        w |= imm << 10;
        w |= rn.0 << 5;
        w |= rd.0;
        Some(w)
    }

    /// `add/sub Xd, Xn, Xm{, lsl #n}` -- geschobene Registerform.
    pub fn addsub_reg(g: Grosse, sub: bool, setflags: bool,
                      rd: R, rn: R, rm: R, shift_art: u32, betrag: u32) -> Option<Wort> {
        if betrag > 63 { return None; }
        let mut w: u32 = 0;
        w |= g.sf() << 31;
        w |= (sub as u32) << 30;
        w |= (setflags as u32) << 29;
        w |= 0b01011 << 24;
        w |= shift_art << 22;       // 00 lsl, 01 lsr, 10 asr
        w |= rm.0 << 16;
        w |= betrag << 10;
        w |= rn.0 << 5;
        w |= rd.0;
        Some(w)
    }

    // ---- Bitmuster-Konstanten ---------------------------------------------

    /// Die eigentliche Kunst: einen Wert als (N, immr, imms) darstellen.
    ///
    /// Ein zulaessiger Wert besteht aus einem Muster der Laenge 2, 4, 8, 16,
    /// 32 oder 64, das sich ueber die ganze Breite wiederholt. Innerhalb einer
    /// Periode stehen zuerst `anzahl` Einsen, danach Nullen -- das Ganze darf
    /// rotiert sein. Alles andere ist nicht kodierbar.
    ///
    /// Rueckgabe: (N, immr, imms) oder None, wenn der Wert nicht darstellbar ist.
    pub fn bitmuster(wert: u64, g: Grosse) -> Option<(u32, u32, u32)> {
        let breite = if g == Grosse::X { 64 } else { 32 };
        let wert = if breite == 32 { (wert as u32) as u64 } else { wert };
        // Alles Nullen oder alles Einsen ist nicht darstellbar.
        if wert == 0 { return None; }
        if breite == 64 && wert == u64::MAX { return None; }
        if breite == 32 && wert == 0xffff_ffff { return None; }

        // Kleinste Periode suchen, mit der sich der Wert wiederholt.
        let mut periode = 2usize;
        while periode <= breite {
            let stueck = wert & ((1u128 << periode) - 1) as u64;
            let mut passt = true;
            let mut pos = periode;
            while pos < breite {
                if ((wert >> pos) & (((1u128 << periode) - 1) as u64)) != stueck {
                    passt = false;
                    break;
                }
                pos += periode;
            }
            if passt { break; }
            periode *= 2;
        }
        if periode > breite { return None; }

        let maske = if periode == 64 { u64::MAX } else { (1u64 << periode) - 1 };
        let stueck = wert & maske;

        // Innerhalb der Periode: die Einsen muessen zusammenhaengen (rotiert).
        let einsen = stueck.count_ones();
        if einsen == 0 || einsen == periode as u32 { return None; }

        // Rotation finden, die die Einsen an den Anfang holt.
        // Ein zusammenhaengender Block ist daran erkennbar, dass nach dem
        // Rotieren die Einsen die unteren Stellen fuellen.
        let mut rot = None;
        for r in 0..periode {
            let gedreht = ((stueck >> r) | (stueck << (periode - r))) & maske;
            if gedreht == ((1u64 << einsen) - 1) {
                rot = Some(r);
                break;
            }
        }
        let r = rot?;

        // immr ist die RECHTSrotation. Oben wurde r als Linksrotation gesucht
        // (die die Einsen nach unten holt), also muss hier das Gegenstueck
        // stehen: eine Rechtsdrehung um (Periode - r). Genau hier lag ein
        // Fehler, den GNU as aufgedeckt hat: fuer 2^60 schreibt as immr=4,
        // nicht 60.
        let immr = ((periode - r) % periode) as u32;
        // imms: die oberen Bits kodieren die Periode als invertierte Maske.
        let imms_grund: u32 = match periode {
            2 => 0b111100,
            4 => 0b111000,
            8 => 0b110000,
            16 => 0b100000,
            32 => 0b000000,
            64 => 0b000000,
            _ => return None,
        };
        let n = if periode == 64 { 1 } else { 0 };
        let imms = imms_grund | (einsen - 1);
        if periode != 64 && (imms & 0b111111) > 0b111111 { return None; }
        Some((n, immr, imms))
    }

    /// `and/orr/eor/ands Xd, Xn, #muster`.
    /// opc: 00 and, 01 orr, 10 eor, 11 ands (ands ist tst mit Xd=zr).
    pub fn logisch_imm(g: Grosse, opc: u32, rd: R, rn: R, wert: u64) -> Option<Wort> {
        let (n, immr, imms) = Self::bitmuster(wert, g)?;
        if g == Grosse::W && n == 1 { return None; } // N darf bei 32 Bit nicht 1 sein
        let mut w: u32 = 0;
        w |= g.sf() << 31;
        w |= opc << 29;
        w |= 0b100100 << 23;
        w |= n << 22;
        w |= immr << 16;
        w |= imms << 10;
        w |= rn.0 << 5;
        w |= rd.0;
        Some(w)
    }

    /// `and/orr/eor Xd, Xn, Xm` -- Registerform.
    pub fn logisch_reg(g: Grosse, opc: u32, rd: R, rn: R, rm: R,
                       shift_art: u32, betrag: u32, negiert: bool) -> Wort {
        let mut w: u32 = 0;
        w |= g.sf() << 31;
        w |= opc << 29;
        w |= 0b01010 << 24;
        w |= shift_art << 22;
        w |= (negiert as u32) << 21;
        w |= rm.0 << 16;
        w |= betrag << 10;
        w |= rn.0 << 5;
        w |= rd.0;
        w
    }

    // ---- Konstanten laden --------------------------------------------------

    /// `movz/movn/movk Xd, #imm16{, lsl #n}`.
    /// opc: 00 movn, 10 movz, 11 movk.
    pub fn mov_wide(g: Grosse, opc: u32, rd: R, imm16: u32, shift: u32) -> Option<Wort> {
        if imm16 > 0xffff { return None; }
        if shift % 16 != 0 { return None; }
        let hw = shift / 16;
        if g == Grosse::W && hw > 1 { return None; }
        if hw > 3 { return None; }
        let mut w: u32 = 0;
        w |= g.sf() << 31;
        w |= opc << 29;
        w |= 0b100101 << 23;
        w |= hw << 21;
        w |= imm16 << 5;
        w |= rd.0;
        Some(w)
    }

    /// Eine beliebige 64-Bit-Konstante in eine Folge von Befehlen zerlegen.
    ///
    /// Das ist die Antwort auf "es gibt kein Feld fuer 64 Bit": zuerst das
    /// erste noetige 16-Bit-Stueck mit MOVZ, dann die restlichen mit MOVK.
    /// Sind die meisten Stuecke 0xffff, ist MOVN kuerzer -- dann wird das
    /// Komplement geladen.
    pub fn konstante(g: Grosse, rd: R, wert: u64) -> Vec<Wort> {
        let breite = if g == Grosse::X { 4 } else { 2 };
        let wert = if g == Grosse::W { (wert as u32) as u64 } else { wert };

        let stueck = |i: u32| ((wert >> (i * 16)) & 0xffff) as u32;
        let null_stuecke = (0..breite).filter(|&i| stueck(i) == 0).count();
        let eins_stuecke = (0..breite).filter(|&i| stueck(i) == 0xffff).count();

        let mut out = Vec::new();
        if eins_stuecke > null_stuecke {
            // MOVN laedt das Komplement eines Stueckes, Rest mit MOVK.
            let komp = !wert;
            let kstueck = |i: u32| ((komp >> (i * 16)) & 0xffff) as u32;
            let erstes = (0..breite).find(|&i| kstueck(i) != 0).unwrap_or(0);
            out.push(Self::mov_wide(g, 0b00, rd, kstueck(erstes), erstes * 16).unwrap());
            for i in 0..breite {
                if i == erstes { continue; }
                if stueck(i) != 0xffff {
                    out.push(Self::mov_wide(g, 0b11, rd, stueck(i), i * 16).unwrap());
                }
            }
        } else {
            let erstes = (0..breite).find(|&i| stueck(i) != 0);
            match erstes {
                None => { out.push(Self::mov_wide(g, 0b10, rd, 0, 0).unwrap()); }
                Some(e) => {
                    out.push(Self::mov_wide(g, 0b10, rd, stueck(e), e * 16).unwrap());
                    for i in (e + 1)..breite {
                        if stueck(i) != 0 {
                            out.push(Self::mov_wide(g, 0b11, rd, stueck(i), i * 16).unwrap());
                        }
                    }
                }
            }
        }
        out
    }

    // ---- Laden und Speichern ----------------------------------------------

    /// `ldr/str Xt, [Xn, #imm]` -- unsigned offset. Der Wert im Feld ist der
    /// Abstand GETEILT durch die Zugriffsgroesse; er muss also ein Vielfaches
    /// davon sein und passt nur bis 4095*Groesse.
    ///
    /// groesse_log: 0=byte, 1=halbwort, 2=wort, 3=doppelwort.
    pub fn ldst_imm(groesse_log: u32, laden: bool, rt: R, rn: R, abstand: i64) -> Option<Wort> {
        let skala = 1i64 << groesse_log;
        if abstand < 0 || abstand % skala != 0 { return None; }
        let feld = (abstand / skala) as u64;
        if feld > 0xfff { return None; }
        let mut w: u32 = 0;
        w |= groesse_log << 30;
        w |= 0b111 << 27;
        w |= 1 << 24;                       // unsigned offset
        w |= (laden as u32) << 22;
        w |= (feld as u32) << 10;
        w |= rn.0 << 5;
        w |= rt.0;
        Some(w)
    }

    /// `ldr/str Xt, [Xn], #imm` -- Anpassung NACH dem Zugriff.
    /// Der Versatz ist hier NICHT skaliert: er zaehlt in Oktetten und ist
    /// vorzeichenbehaftet (9 Bit). Das ist ein anderer Zahlenraum als beim
    /// unsigned offset -- eine klassische Verwechslung.
    pub fn ldst_post(groesse_log: u32, laden: bool, rt: R, rn: R, versatz: i64) -> Option<Wort> {
        if versatz < -256 || versatz > 255 { return None; }
        let mut w: u32 = 0;
        w |= groesse_log << 30;
        w |= 0b111 << 27;
        w |= (laden as u32) << 22;
        w |= ((versatz as u32) & 0x1ff) << 12;
        w |= 0b01 << 10;               // 01 = nach dem Zugriff
        w |= rn.0 << 5;
        w |= rt.0;
        Some(w)
    }

    /// Vorzeichenerweiterndes Laden: ldrsb/ldrsh/ldrsw.
    /// `ziel_x` sagt, ob in ein 64-Bit-Register geladen wird.
    pub fn ldst_imm_sign(groesse_log: u32, ziel_x: bool, rt: R, rn: R, abstand: i64) -> Option<Wort> {
        let skala = 1i64 << groesse_log;
        if abstand < 0 || abstand % skala != 0 { return None; }
        let feld = (abstand / skala) as u64;
        if feld > 0xfff { return None; }
        let mut w: u32 = 0;
        w |= groesse_log << 30;
        w |= 0b111 << 27;
        w |= 1 << 24;
        // opc = 10 fuer 64-Bit-Ziel, 11 fuer 32-Bit-Ziel
        w |= (if ziel_x { 0b10 } else { 0b11 }) << 22;
        w |= (feld as u32) << 10;
        w |= rn.0 << 5;
        w |= rt.0;
        Some(w)
    }

    /// `ldr/str Xt, [Xn, Xm{, lsl #n}]` -- Registerversatz mit Skalierung.
    pub fn ldst_reg(groesse_log: u32, laden: bool, rt: R, rn: R, rm: R,
                    option: u32, s: bool) -> Wort {
        let mut w: u32 = 0;
        w |= groesse_log << 30;
        w |= 0b111 << 27;
        w |= (laden as u32) << 22;
        w |= 1 << 21;
        w |= rm.0 << 16;
        w |= option << 13;          // 011 = lsl (uxtx)
        w |= (s as u32) << 12;
        w |= 0b10 << 10;
        w |= rn.0 << 5;
        w |= rt.0;
        w
    }

    /// `ldp/stp Xt1, Xt2, [Xn, #imm]` sowie die Formen mit Vorab-Anpassung.
    /// art: 01 nach dem Zugriff, 10 ohne Anpassung, 11 vor dem Zugriff.
    pub fn ldstp(g: Grosse, laden: bool, art: u32, rt1: R, rt2: R, rn: R, abstand: i64) -> Option<Wort> {
        let skala: i64 = if g == Grosse::X { 8 } else { 4 };
        if abstand % skala != 0 { return None; }
        let feld = abstand / skala;
        if feld < -64 || feld > 63 { return None; }
        let mut w: u32 = 0;
        w |= g.sf() << 31;
        w |= 0b101 << 27;
        w |= art << 23;
        w |= (laden as u32) << 22;
        w |= ((feld as u32) & 0x7f) << 15;
        w |= rt2.0 << 10;
        w |= rn.0 << 5;
        w |= rt1.0;
        Some(w)
    }

    // ---- Spruenge ----------------------------------------------------------

    /// `b`/`bl` -- der Abstand ist in WORTEN angegeben, nicht in Oktetten.
    /// Reichweite: +-128 MiB.
    pub fn b(link: bool, abstand_okt: i64) -> Option<Wort> {
        if abstand_okt % 4 != 0 { return None; }
        let worte = abstand_okt / 4;
        if worte < -(1 << 25) || worte >= (1 << 25) { return None; }
        let mut w: u32 = 0;
        w |= (link as u32) << 31;
        w |= 0b101 << 26;
        w |= (worte as u32) & 0x03ff_ffff;
        Some(w)
    }

    /// `b.<cond>` -- Reichweite nur +-1 MiB, das ist deutlich enger.
    pub fn b_cond(cond: u32, abstand_okt: i64) -> Option<Wort> {
        if abstand_okt % 4 != 0 { return None; }
        let worte = abstand_okt / 4;
        if worte < -(1 << 18) || worte >= (1 << 18) { return None; }
        let mut w: u32 = 0;
        w |= 0b0101_0100 << 24;
        w |= ((worte as u32) & 0x7ffff) << 5;
        w |= cond;
        Some(w)
    }

    /// `cbz`/`cbnz` -- ebenfalls +-1 MiB.
    pub fn cb(g: Grosse, nz: bool, rt: R, abstand_okt: i64) -> Option<Wort> {
        if abstand_okt % 4 != 0 { return None; }
        let worte = abstand_okt / 4;
        if worte < -(1 << 18) || worte >= (1 << 18) { return None; }
        let mut w: u32 = 0;
        w |= g.sf() << 31;
        w |= 0b011010 << 25;
        w |= (nz as u32) << 24;
        w |= ((worte as u32) & 0x7ffff) << 5;
        w |= rt.0;
        Some(w)
    }

    /// `tbz`/`tbnz` -- nur +-32 KiB, die kuerzeste Reichweite ueberhaupt.
    pub fn tb(nz: bool, rt: R, bit: u32, abstand_okt: i64) -> Option<Wort> {
        if abstand_okt % 4 != 0 { return None; }
        let worte = abstand_okt / 4;
        if worte < -(1 << 13) || worte >= (1 << 13) { return None; }
        if bit > 63 { return None; }
        let mut w: u32 = 0;
        w |= (bit >> 5) << 31;
        w |= 0b011011 << 25;
        w |= (nz as u32) << 24;
        w |= (bit & 31) << 19;
        w |= ((worte as u32) & 0x3fff) << 5;
        w |= rt.0;
        Some(w)
    }

    /// `br`/`blr`/`ret` -- Sprung ueber ein Register.
    pub fn br(opc: u32, rn: R) -> Wort {
        let mut w: u32 = 0;
        w |= 0b1101011 << 25;
        w |= opc << 21;
        w |= 0b11111 << 16;
        w |= rn.0 << 5;
        w
    }

    /// `adrp Xd, sym` -- laedt die Seitenadresse. Der Wert ist der Abstand
    /// der SEITEN, nicht der Oktette.
    pub fn adrp(rd: R, seiten_abstand: i64) -> Option<Wort> {
        if seiten_abstand < -(1 << 20) || seiten_abstand >= (1 << 20) { return None; }
        let v = seiten_abstand as u32;
        let lo = v & 3;
        let hi = (v >> 2) & 0x7ffff;
        let mut w: u32 = 0;
        w |= 1 << 31;
        w |= lo << 29;
        w |= 0b10000 << 24;
        w |= hi << 5;
        w |= rd.0;
        Some(w)
    }

    // ---- Multiplizieren und Teilen ----------------------------------------

    /// `madd/msub Xd, Xn, Xm, Xa`. mul ist madd mit Xa=zr,
    /// mneg/msub entsprechend.
    pub fn madd(g: Grosse, sub: bool, rd: R, rn: R, rm: R, ra: R) -> Wort {
        let mut w: u32 = 0;
        w |= g.sf() << 31;
        w |= 0b11011 << 24;
        w |= rm.0 << 16;
        w |= (sub as u32) << 15;
        w |= ra.0 << 10;
        w |= rn.0 << 5;
        w |= rd.0;
        w
    }

    /// `smulh/umulh` -- die oberen 64 Bit eines 128-Bit-Produkts.
    pub fn mulh(vorzeichen: bool, rd: R, rn: R, rm: R) -> Wort {
        let mut w: u32 = 0;
        w |= 1 << 31;
        w |= 0b11011 << 24;
        w |= (if vorzeichen { 0b010 } else { 0b110 }) << 21;
        w |= rm.0 << 16;
        w |= 0b11111 << 10;
        w |= rn.0 << 5;
        w |= rd.0;
        w
    }

    /// `sdiv/udiv`.
    pub fn div(g: Grosse, vorzeichen: bool, rd: R, rn: R, rm: R) -> Wort {
        let mut w: u32 = 0;
        w |= g.sf() << 31;
        w |= 0b11010110 << 21;
        w |= rm.0 << 16;
        w |= 0b00001 << 11;
        w |= (vorzeichen as u32) << 10;
        w |= rn.0 << 5;
        w |= rd.0;
        w
    }

    /// Schieben ueber ein Register: lslv/lsrv/asrv.
    /// op2: 00 lsl, 01 lsr, 10 asr, 11 ror.
    pub fn schieben_reg(g: Grosse, op2: u32, rd: R, rn: R, rm: R) -> Wort {
        let mut w: u32 = 0;
        w |= g.sf() << 31;
        w |= 0b11010110 << 21;
        w |= rm.0 << 16;
        w |= 0b0010 << 12;
        w |= op2 << 10;
        w |= rn.0 << 5;
        w |= rd.0;
        w
    }

    // ---- Bitfelder ---------------------------------------------------------

    /// `ubfm/sbfm` -- die Grundlage von lsl/lsr/asr mit Sofortwert sowie
    /// von uxtb/uxth/sxtb/sxth/sxtw.
    pub fn bfm(g: Grosse, opc: u32, rd: R, rn: R, immr: u32, imms: u32) -> Wort {
        let n = if g == Grosse::X { 1 } else { 0 };
        let mut w: u32 = 0;
        w |= g.sf() << 31;
        w |= opc << 29;
        w |= 0b100110 << 23;
        w |= n << 22;
        w |= immr << 16;
        w |= imms << 10;
        w |= rn.0 << 5;
        w |= rd.0;
        w
    }

    /// `cset Wd, cond` ist csinc Wd, wzr, wzr, invertierte Bedingung.
    pub fn csinc(g: Grosse, rd: R, rn: R, rm: R, cond: u32) -> Wort {
        let mut w: u32 = 0;
        w |= g.sf() << 31;
        w |= 0b11010100 << 21;
        w |= rm.0 << 16;
        w |= cond << 12;
        w |= 1 << 10;
        w |= rn.0 << 5;
        w |= rd.0;
        w
    }

    /// `csel Xd, Xn, Xm, cond`.
    pub fn csel(g: Grosse, rd: R, rn: R, rm: R, cond: u32) -> Wort {
        let mut w: u32 = 0;
        w |= g.sf() << 31;
        w |= 0b11010100 << 21;
        w |= rm.0 << 16;
        w |= cond << 12;
        w |= rn.0 << 5;
        w |= rd.0;
        w
    }

    // ---- Systembefehle -----------------------------------------------------

    pub fn svc(imm: u32) -> Wort { (0b11010100000 << 21) | ((imm & 0xffff) << 5) | 1 }
    pub fn brk(imm: u32) -> Wort { (0b11010100001 << 21) | ((imm & 0xffff) << 5) }
    pub fn ret_reg(rn: R) -> Wort { Self::br(0b0010, rn) }
    pub fn clrex() -> Wort { 0xd5033f5f }

    /// `ldaxr/stlxr` -- die Bausteine fuer atomare Abschnitte.
    pub fn ldaxr(g: Grosse, rt: R, rn: R) -> Wort {
        let groesse = if g == Grosse::X { 3u32 } else { 2 };
        let mut w: u32 = 0;
        w |= groesse << 30;
        w |= 0b001000 << 24;
        w |= 1 << 22;
        w |= 0b11111 << 16;
        w |= 1 << 15;
        w |= 0b11111 << 10;
        w |= rn.0 << 5;
        w |= rt.0;
        w
    }

    pub fn stlxr(g: Grosse, rs: R, rt: R, rn: R) -> Wort {
        let groesse = if g == Grosse::X { 3u32 } else { 2 };
        let mut w: u32 = 0;
        w |= groesse << 30;
        w |= 0b001000 << 24;
        w |= rs.0 << 16;
        w |= 1 << 15;
        w |= 0b11111 << 10;
        w |= rn.0 << 5;
        w |= rt.0;
        w
    }

    // ---- Gleitkomma --------------------------------------------------------

    /// `fadd/fsub/fmul/fdiv` -- typ: 00 einfach, 01 doppelt.
    /// opc: 0010 fadd, 0011 fsub, 0000 fmul, 0001 fdiv.
    pub fn f_rechnen(typ: u32, opc: u32, rd: V, rn: V, rm: V) -> Wort {
        let mut w: u32 = 0;
        w |= 0b11110 << 24;
        w |= typ << 22;
        w |= 1 << 21;
        w |= rm.0 << 16;
        w |= opc << 12;
        w |= 0b10 << 10;
        w |= rn.0 << 5;
        w |= rd.0;
        w
    }

    /// `fcmp Dn, Dm`.
    pub fn fcmp(typ: u32, rn: V, rm: V) -> Wort {
        let mut w: u32 = 0;
        w |= 0b11110 << 24;
        w |= typ << 22;
        w |= 1 << 21;
        w |= rm.0 << 16;
        w |= 0b1000 << 10;
        w |= rn.0 << 5;
        w
    }

    /// `fcvt` zwischen einfacher und doppelter Genauigkeit.
    pub fn fcvt(von: u32, nach: u32, rd: V, rn: V) -> Wort {
        let mut w: u32 = 0;
        w |= 0b11110 << 24;
        w |= von << 22;
        w |= 1 << 21;
        w |= 0b0001 << 17;
        w |= nach << 15;
        w |= 0b10000 << 10;
        w |= rn.0 << 5;
        w |= rd.0;
        w
    }

    /// `scvtf` (ganz -> gleit) und `fcvtzs` (gleit -> ganz, abschneidend).
    pub fn cvt_ganz(g: Grosse, typ: u32, nach_gleit: bool, rd: u32, rn: u32) -> Wort {
        let mut w: u32 = 0;
        w |= g.sf() << 31;
        w |= 0b11110 << 24;
        w |= typ << 22;
        w |= 1 << 21;
        if nach_gleit {
            w |= 0b00 << 19;
            w |= 0b010 << 16;   // scvtf
        } else {
            w |= 0b11 << 19;
            w |= 0b000 << 16;   // fcvtzs
        }
        w |= rn << 5;
        w |= rd;
        w
    }

    /// `fmov` zwischen ganzzahligem und Gleitkommaregister.
    pub fn fmov_ganz(g: Grosse, typ: u32, nach_gleit: bool, rd: u32, rn: u32) -> Wort {
        let mut w: u32 = 0;
        w |= g.sf() << 31;
        w |= 0b11110 << 24;
        w |= typ << 22;
        w |= 1 << 21;
        // rmode ist 00 -- NICHT 11. Das ist der Unterschied zwischen fmov
        // (blosses Umschaufeln der Bits) und den Rundungsbefehlen.
        w |= 0b00 << 19;
        w |= (if nach_gleit { 0b111 } else { 0b110 }) << 16;
        w |= rn << 5;
        w |= rd;
        w
    }

    /// `mrs`/`msr` -- Systemregister lesen und schreiben.
    pub fn mrs(rt: R, o0: u32, op1: u32, crn: u32, crm: u32, op2: u32) -> Wort {
        let mut w: u32 = 0;
        w |= 0b1101010100 << 22;
        w |= 1 << 21;
        w |= (o0 & 1) << 19;
        w |= op1 << 16;
        w |= crn << 12;
        w |= crm << 8;
        w |= op2 << 5;
        w |= rt.0;
        w
    }

    pub fn msr(rt: R, o0: u32, op1: u32, crn: u32, crm: u32, op2: u32) -> Wort {
        let mut w: u32 = 0;
        w |= 0b1101010100 << 22;
        w |= (o0 & 1) << 19;
        w |= op1 << 16;
        w |= crn << 12;
        w |= crm << 8;
        w |= op2 << 5;
        w |= rt.0;
        w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmuster_grenzfaelle() {
        // 0 und lauter Einsen sind NICHT darstellbar -- das ist Absicht der
        // Architektur, nicht ein Fehler hier.
        assert!(A64::bitmuster(0, Grosse::X).is_none());
        assert!(A64::bitmuster(u64::MAX, Grosse::X).is_none());
        // 1, 3, 255, 4095 sind zusammenhaengende Einserblocke -> darstellbar
        assert!(A64::bitmuster(1, Grosse::X).is_some());
        assert!(A64::bitmuster(3, Grosse::X).is_some());
        assert!(A64::bitmuster(255, Grosse::X).is_some());
        assert!(A64::bitmuster(4095, Grosse::X).is_some());
        // 4097 = 0b1000000000001 ist kein zusammenhaengender Block
        assert!(A64::bitmuster(4097, Grosse::X).is_none());
    }

    #[test]
    fn konstante_zerlegen() {
        // Kleine Zahl -> ein einziges movz
        assert_eq!(A64::konstante(Grosse::X, R(0), 1).len(), 1);
        // Zahl ueber alle vier Stuecke -> movz + 3x movk
        assert_eq!(A64::konstante(Grosse::X, R(0), 0x1234_5678_9abc_def0).len(), 4);
    }
}
