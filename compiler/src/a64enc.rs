//! **RUNDE KODIERER — der ARM64-Binärkodierer.**
//!
//! ARM64 ist die freundlichere Hälfte der Aufgabe: **jeder Befehl ist genau
//! vier Oktette lang**, es gibt keine Präfixe, kein ModRM, kein SIB und
//! keine Relaxation — die Länge steht vor der Marke fest. Dafür sitzt die
//! Schwierigkeit woanders: in den **Sofortwerten**.
//!
//! Drei davon sind wirklich fies und stehen deshalb hier oben:
//!
//! 1. **Logische Sofortwerte** (`and x0, x0, #imm`) werden nicht als Zahl
//!    gespeichert, sondern als *Muster*: `N:immr:imms` beschreibt eine
//!    Folge von Einsen der Länge `s+1`, rotiert um `r`, wiederholt über eine
//!    Periode von 2, 4, 8, 16, 32 oder 64 Bit. Nur Zahlen, die so entstehen
//!    können, sind überhaupt kodierbar — `#1` ja, `#3` ja, `#5` **nein**.
//!    [`logical_imm`] rechnet das aus und sagt ehrlich `None`, wenn es nicht
//!    geht.
//! 2. **Verschobene Sofortwerte** (`add x0, x0, #imm`): 12 Bit, wahlweise um
//!    12 Stellen nach links geschoben. Also 0…4095 oder 4096…16773120 in
//!    Schritten von 4096 — dazwischen nichts.
//! 3. **Verschiebungen beim Laden** sind **skaliert**: `ldr x0, [x1, #16]`
//!    speichert die 2, nicht die 16. Passt die Verschiebung nicht ins
//!    Raster (negativ oder unausgerichtet), muss auf die *unskalierte* Form
//!    `ldur` gewechselt werden — mit anderem Opcode. `as` macht das
//!    stillschweigend; wer es nicht nachbildet, erzeugt einen Ladebefehl,
//!    der die falsche Speicherstelle liest.
//!
//! Wie beim x86-Kodierer ist jede Regel gegen `aarch64-linux-gnu-as`
//! geprüft, nicht aus dem Gedächtnis geschrieben.

#![allow(dead_code)]

// ===========================================================================
// Register
// ===========================================================================

/// Registernummer 0-31. 31 ist je nach Befehl `xzr`/`wzr` **oder** `sp` —
/// das entscheidet der Befehl, nicht die Nummer.
pub type Reg = u8;
pub const ZR: Reg = 31;
pub const SP_R: Reg = 31;
pub const LR: Reg = 30;

/// `x`/`w`-Register aus einem Namen; liefert (Nummer, ist_64bit, ist_sp).
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

/// Gleitkomma-/Vektorregister: `d0`, `s0`, `q0`, `v0`, `h0`, `b0`.
/// Liefert (Nummer, Breitenkennung).
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

/// Bedingungscodes.
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
// Sofortwert-Kodierungen
// ===========================================================================

/// **Der logische Sofortwert.** Aus einer Zahl wird `(N, immr, imms)`, oder
/// `None`, wenn die Zahl kein zulässiges Muster ist.
///
/// Das Verfahren (ARM ARM, „DecodeBitMasks" rückwärts):
///
/// 1. Die Zahl muss aus einer Periode `e` ∈ {2,4,8,16,32,64} bestehen, die
///    sich über die ganze Registerbreite wiederholt.
/// 2. Innerhalb der Periode müssen die Einsen **zusammenhängend** liegen
///    (rundlaufend). Weder 0 noch alles-1 sind zulässig.
/// 3. `imms` trägt die Länge (minus 1) und die Periode, `immr` die Rotation.
///
/// Beispiel: `0xFFFF_FFFF_FFFF_FFFC` = 62 Einsen, um 2 rotiert → kodierbar.
/// `0x5` (101 binär) ist es nicht — die Einsen sind nicht zusammenhängend.
pub fn logical_imm(value: u64, is64: bool) -> Option<(u32, u32, u32)> {
    let width = if is64 { 64u32 } else { 32 };
    let mut v = value;
    if !is64 {
        // bei 32 Bit muss die obere Hälfte eine Kopie sein
        if value >> 32 != 0 && value >> 32 != 0xFFFF_FFFF {
            // wir arbeiten unten mit der 32-Bit-Zahl, verdoppelt
        }
        v = (value & 0xFFFF_FFFF) | ((value & 0xFFFF_FFFF) << 32);
    }
    if v == 0 || v == u64::MAX {
        return None;
    }
    // 1. kleinste Periode finden
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
    // 2. rundlaufend zusammenhängende Einsen?
    //    Wir rotieren die Periode so, dass sie mit einer 0 endet und mit
    //    Einsen beginnt; dann muss part == (2^ones - 1) sein.
    let ones = part.count_ones();
    // Rotation suchen: die Stelle, an der ein 0->1-Übergang stattfindet.
    let mut rot = None;
    for r in 0..e {
        let rotated = rotate_right(part, r, e);
        if rotated == (if ones == e { mask } else { (1u64 << ones) - 1 }) {
            rot = Some(r);
            break;
        }
    }
    // `immr` ist die Rotation, mit der aus dem Grundmuster der WERT wird —
    // wir haben oben die umgekehrte Richtung gesucht. Also spiegeln.
    let r = (e - rot?) % e;
    // 3. zusammensetzen
    // imms: die oberen Bits kodieren die Periode als Maske
    //   e=64 -> 0b000000 (mit N=1), e=32 -> 0b011111 & ..., allgemein:
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

/// Der verschobene Sofortwert von `add`/`sub`: 12 Bit, wahlweise `lsl #12`.
/// Rückgabe: `(imm12, sh)`.
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
// Ausbesserungen
// ===========================================================================

/// Welche Art von relativem Feld eine Ausbesserung füllt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixKind {
    /// `b`/`bl`: 26 Bit, in Worten (×4).
    Br26,
    /// `b.cond`/`cbz`/`cbnz`: 19 Bit, in Worten.
    Br19,
    /// `tbz`/`tbnz`: 14 Bit, in Worten.
    Br14,
    /// `adrp`: 21 Bit Seitenversatz.
    Adrp,
    /// `add …, #:lo12:sym`: die unteren 12 Bit.
    Lo12,
    /// `ldr …, [x, #:lo12:sym]`: die unteren 12 Bit, skaliert.
    Lo12Scaled(u32),
}

#[derive(Clone, Copy, Debug)]
pub struct Fixup {
    pub id: u32,
    /// Versatz des Befehlsworts im Puffer.
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
    /// Setzt ein Feld in einem bereits geschriebenen Wort.
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

/// Trägt einen aufgelösten Wert in das Feld einer Ausbesserung ein.
///
/// `rel` ist bei den Sprüngen der Abstand in **Oktetten**, bei `adrp` der
/// Seitenabstand, bei `lo12` der absolute Wert.
pub fn apply_fix(buf: &mut Buf, at: usize, kind: FixKind, rel: i64) -> Result<(), String> {
    match kind {
        FixKind::Br26 => {
            if rel % 4 != 0 {
                return Err("Sprungziel nicht wortweise ausgerichtet".into());
            }
            let w = rel / 4;
            if !(-(1 << 25)..(1 << 25)).contains(&w) {
                return Err(format!("Sprungweite {} zu groß für b/bl", rel));
            }
            buf.patch(at, 0x03FF_FFFF, (w as u32) & 0x03FF_FFFF);
        }
        FixKind::Br19 => {
            if rel % 4 != 0 {
                return Err("Sprungziel nicht wortweise ausgerichtet".into());
            }
            let w = rel / 4;
            if !(-(1 << 18)..(1 << 18)).contains(&w) {
                return Err(format!("Sprungweite {} zu groß für b.cond/cbz", rel));
            }
            buf.patch(at, 0x00FF_FFE0, ((w as u32) & 0x7FFFF) << 5);
        }
        FixKind::Br14 => {
            if rel % 4 != 0 {
                return Err("Sprungziel nicht wortweise ausgerichtet".into());
            }
            let w = rel / 4;
            if !(-(1 << 13)..(1 << 13)).contains(&w) {
                return Err(format!("Sprungweite {} zu groß für tbz/tbnz", rel));
            }
            buf.patch(at, 0x0007_FFE0, ((w as u32) & 0x3FFF) << 5);
        }
        FixKind::Adrp => {
            let p = rel >> 12;
            if !(-(1 << 20)..(1 << 20)).contains(&p) {
                return Err("adrp-Abstand zu groß".into());
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
                return Err("lo12 nicht passend ausgerichtet".into());
            }
            let v = if sc > 0 { v / sc } else { v };
            buf.patch(at, 0x003F_FC00, (v & 0xFFF) << 10);
        }
    }
    Ok(())
}
