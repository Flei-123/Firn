//! **RUNDE KODIERER — vom Assemblertext zur Objektdatei, ARM64.**
//!
//! Dasselbe wie `asm_x86.rs`, nur für die andere Maschine — und an drei
//! Stellen einfacher:
//!
//! * **Keine Relaxation.** Jeder Befehl ist vier Oktette lang, egal wohin er
//!   springt. Die Versätze stehen nach einem Durchgang fest.
//! * **Keine Präfixe, kein ModRM.** Ein Befehl ist ein 32-Bit-Wort mit
//!   festen Feldern.
//! * **Kein Unterschied zwischen `b` und `bl`** bei der Auflösung: `as`
//!   setzt für **beide** eine Umsetzung, sobald das Ziel global ist
//!   (`R_AARCH64_JUMP26` bzw. `R_AARCH64_CALL26`) — anders als auf x86, wo
//!   der Sprung aufgelöst und der Aufruf umgesetzt wird.
//!
//! Dafür ist die **Adressbildung** aufwendiger. Eine Adresse in einem
//! anderen Abschnitt braucht immer zwei Befehle:
//!
//! ```text
//!     adrp x0, .Lstr          ; die 4-KiB-Seite, ±4 GiB weit
//!     add  x0, x0, #:lo12:.Lstr   ; die unteren zwölf Bit dazu
//! ```
//!
//! Beide bekommen eine eigene Umsetzung, und bei der Ladeform
//! (`ldr x1, [x0, #:lo12:sym]`) hängt die Umsetzungsart von der
//! **Zugriffsbreite** ab: `LDST8`, `LDST16`, `LDST32`, `LDST64` oder
//! `LDST128`. Wer hier die falsche nimmt, bekommt vom Binder eine falsch
//! skalierte Verschiebung — und der Fehler zeigt sich erst als falsch
//! gelesener Speicher.

#![allow(dead_code)]

use crate::a64enc::{self as A, Buf, FixKind};
use std::collections::HashMap;

pub const SEC_TEXT: usize = 0;
pub const SEC_DATA: usize = 1;
pub const SEC_RODATA: usize = 2;
pub const SEC_NOTE: usize = 3;
/// `.bss` — Inhalt null, belegt aber Platz im Abbild (ELF: `NOBITS`).
pub const SEC_BSS: usize = 4;
pub const N_SEC: usize = 5;

// Umsetzungsarten der AArch64-ABI.
pub const R_AARCH64_ABS64: u32 = 257;
pub const R_AARCH64_ABS32: u32 = 258;
pub const R_AARCH64_ADR_PREL_PG_HI21: u32 = 275;
pub const R_AARCH64_ADD_ABS_LO12_NC: u32 = 277;
pub const R_AARCH64_LDST8_ABS_LO12_NC: u32 = 278;
pub const R_AARCH64_TSTBR14: u32 = 279;
pub const R_AARCH64_CONDBR19: u32 = 280;
pub const R_AARCH64_JUMP26: u32 = 282;
pub const R_AARCH64_CALL26: u32 = 283;
pub const R_AARCH64_LDST16_ABS_LO12_NC: u32 = 284;
pub const R_AARCH64_LDST32_ABS_LO12_NC: u32 = 285;
pub const R_AARCH64_LDST64_ABS_LO12_NC: u32 = 286;
pub const R_AARCH64_LDST128_ABS_LO12_NC: u32 = 299;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reloc {
    pub offset: u64,
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
    pub section: Option<usize>,
    pub value: u64,
    pub global: bool,
    pub is_section: bool,
}

pub struct Assembled {
    pub sections: Vec<Section>,
    pub symbols: Vec<Sym>,
}

/// Eine Umsetzung, deren Symbol erst am Ende bekannt ist.
#[derive(Clone, Debug)]
struct PendReloc {
    at: usize,
    name: String,
    kind: u32,
}

#[derive(Clone, Debug)]
enum Piece {
    /// Fertig kodiert.
    Fixed { bytes: Vec<u8>, relocs: Vec<PendReloc> },
    /// Ein Sprung auf eine Marke — Länge steht fest (4), der Wert nicht.
    Branch { word: u32, kind: FixKind, target: String, reloc: u32 },
    Align { n: u64 },
    Label { name: String },
}

pub struct Asm {
    pieces: [Vec<Piece>; N_SEC],
    cur: usize,
    globals: Vec<String>,
    aligns: [u64; N_SEC],
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
            aligns: [4, 1, 1, 1, 1],
            numeric: HashMap::new(),
            line_no: 0,
        }
    }

    fn push(&mut self, p: Piece) {
        self.pieces[self.cur].push(p);
    }
    fn word(&mut self, w: u32) {
        self.push(Piece::Fixed { bytes: w.to_le_bytes().to_vec(), relocs: Vec::new() });
    }
    fn err(&self, m: impl AsRef<str>) -> String {
        format!("Zeile {}: {}", self.line_no, m.as_ref())
    }

    pub fn feed(&mut self, text: &str) -> Result<(), String> {
        for (n, raw) in text.lines().enumerate() {
            self.line_no = n + 1;
            self.line(raw)?;
        }
        Ok(())
    }

    fn line(&mut self, raw: &str) -> Result<(), String> {
        // Kommentar: `//` und `#`, aber nicht in einer Zeichenkette.
        let mut cut = raw.len();
        let mut in_str = false;
        let mut esc = false;
        let b = raw.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if esc {
                esc = false;
                i += 1;
                continue;
            }
            match b[i] {
                b'\\' if in_str => esc = true,
                b'"' => in_str = !in_str,
                b'/' if !in_str && i + 1 < b.len() && b[i + 1] == b'/' => {
                    cut = i;
                    break;
                }
                b'#' if !in_str && (i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t') => {
                    // `#` leitet auf ARM64 einen Sofortwert ein — nur ein
                    // `#` am Zeilenanfang ist ein Kommentar.
                    if i == 0 {
                        cut = i;
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        let mut s = raw[..cut].trim();
        if s.is_empty() {
            return Ok(());
        }

        // Marken
        loop {
            let bytes = s.as_bytes();
            let mut i = 0;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i > 0 && i < bytes.len() && bytes[i] == b':' {
                let n: u32 = s[..i].parse().map_err(|_| self.err("Markenzahl"))?;
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
            i = 0;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'.' | b'$' | b'#'))
            {
                i += 1;
            }
            if i > 0 && i < bytes.len() && bytes[i] == b':' {
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

        if s.starts_with('.') {
            return self.directive(s);
        }
        self.instruction(s)
    }

    fn directive(&mut self, s: &str) -> Result<(), String> {
        let (d, rest) = match s.find(char::is_whitespace) {
            Some(i) => (&s[..i], s[i..].trim()),
            None => (s, ""),
        };
        match d {
            ".file" | ".loc" | ".ident" | ".type" | ".size" | ".arch" | ".cpu"
            | ".cfi_startproc" | ".cfi_endproc" | ".intel_syntax" => Ok(()),
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
                    o => return Err(self.err(format!("unbekannter Abschnitt {}", o))),
                };
                Ok(())
            }
            ".globl" | ".global" => {
                self.globals.push(rest.to_string());
                Ok(())
            }
            // `.balign` zählt Oktette, `.align` auf AArch64 Zweierpotenzen.
            ".balign" => {
                let n: u64 = rest.parse().map_err(|_| self.err("Ausrichtung"))?;
                self.set_align(n);
                Ok(())
            }
            ".align" | ".p2align" => {
                let e: u32 = rest
                    .split(',')
                    .next()
                    .unwrap_or("0")
                    .trim()
                    .parse()
                    .map_err(|_| self.err("Ausrichtung"))?;
                self.set_align(1u64 << e);
                Ok(())
            }
            ".zero" | ".space" | ".skip" => {
                let n: usize = rest
                    .split(',')
                    .next()
                    .unwrap_or("0")
                    .trim()
                    .parse()
                    .map_err(|_| self.err("Größe"))?;
                self.push(Piece::Fixed { bytes: vec![0u8; n], relocs: Vec::new() });
                Ok(())
            }
            ".byte" | ".word" | ".short" | ".hword" | ".long" | ".int" | ".quad" | ".xword" => {
                let w = match d {
                    ".byte" => 1usize,
                    ".short" | ".hword" => 2,
                    ".word" | ".long" | ".int" => 4,
                    _ => 8,
                };
                for item in rest.split(',') {
                    let item = item.trim();
                    if item.is_empty() {
                        continue;
                    }
                    match parse_int(item) {
                        Some(v) => {
                            let bs = (v as u64).to_le_bytes();
                            self.push(Piece::Fixed { bytes: bs[..w].to_vec(), relocs: Vec::new() });
                        }
                        None => {
                            let kind = match w {
                                8 => R_AARCH64_ABS64,
                                4 => R_AARCH64_ABS32,
                                _ => return Err(self.err("Symbol in zu schmalem Feld")),
                            };
                            self.push(Piece::Fixed {
                                bytes: vec![0u8; w],
                                relocs: vec![PendReloc { at: 0, name: item.to_string(), kind }],
                            });
                        }
                    }
                }
                Ok(())
            }
            ".ascii" | ".string" | ".asciz" => {
                let mut out = Vec::new();
                for item in split_strings(rest) {
                    out.extend_from_slice(&unescape(&item)?);
                    if d != ".ascii" {
                        out.push(0);
                    }
                }
                self.push(Piece::Fixed { bytes: out, relocs: Vec::new() });
                Ok(())
            }
            o => Err(self.err(format!("unbekannte Direktive {}", o))),
        }
    }

    fn set_align(&mut self, n: u64) {
        if n > self.aligns[self.cur] {
            self.aligns[self.cur] = n;
        }
        self.push(Piece::Align { n });
    }

    fn resolve_numeric(&self, t: &str) -> String {
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
}

// ===========================================================================
// Hilfsfunktionen zum Zerteilen
// ===========================================================================

pub(crate) fn parse_int(s: &str) -> Option<i64> {
    let s = s.trim().trim_start_matches('#').trim();
    let neg = s.starts_with('-');
    let s2 = s.trim_start_matches('-');
    let v = if let Some(h) = s2.strip_prefix("0x").or_else(|| s2.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()?
    } else {
        s2.parse::<u64>().ok()?
    };
    Some(if neg { -(v as i64) } else { v as i64 })
}

/// Zerlegt die Operanden an Kommas der obersten Ebene (`[…]` und `{…}` zählen).
pub(crate) fn split_operands(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut depth = 0;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '[' | '{' => {
                depth += 1;
                cur.push(c);
            }
            ']' | '}' => {
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
    // Nachgestellte Aktualisierung: `[sp], #16` ist EIN Operand, nicht zwei.
    // Das Komma steht dort ausserhalb der Klammer, also muss es hier wieder
    // zusammengefuegt werden -- sonst haette `ldp x29, x30, [sp], #16` vier
    // Operanden statt drei.
    let mut merged: Vec<String> = Vec::with_capacity(out.len());
    for o in out {
        if o.starts_with('#') {
            if let Some(last) = merged.last_mut() {
                if last.ends_with(']') {
                    last.push_str(", ");
                    last.push_str(&o);
                    continue;
                }
            }
        }
        merged.push(o);
    }
    merged
}

fn split_strings(s: &str) -> Vec<String> {
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
        .ok_or_else(|| format!("keine Zeichenkette: {}", s))?;
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

/// Der Gesamteinstieg (die Kodierung selbst steht in `asm_a64_inst.rs`).
pub fn assemble(text: &str) -> Result<Assembled, String> {
    let mut a = Asm::new();
    a.feed(text)?;
    a.finish()
}

fn elf_index(sec: usize) -> usize {
    match sec {
        SEC_TEXT => 0,
        SEC_DATA => 1,
        SEC_BSS => 2,
        SEC_RODATA => 3,
        _ => 4,
    }
}

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
    Ok(crate::elfobj::write(crate::elfobj::EM_AARCH64, &secs, &syms))
}

// ===========================================================================
// Auflösen
// ===========================================================================

impl Asm {
    pub fn finish(mut self) -> Result<Assembled, String> {
        let mut is_global: HashMap<&str, ()> = HashMap::new();
        for g in &self.globals {
            is_global.insert(g.as_str(), ());
        }

        // Versätze — ein Durchgang genügt, weil keine Länge veränderlich ist.
        let mut labels: HashMap<String, (usize, u64)> = HashMap::new();
        for sec in 0..N_SEC {
            let mut off = 0u64;
            for p in &self.pieces[sec] {
                match p {
                    Piece::Fixed { bytes, .. } => off += bytes.len() as u64,
                    Piece::Branch { .. } => off += 4,
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
        }

        let secnames: [&'static str; N_SEC] = [".text", ".data", ".rodata", ".note.GNU-stack", ".bss"];
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
                                // `as` füllt den Codeabschnitt mit `nop`.
                                let mut k = pad;
                                while k >= 4 {
                                    sections[sec].bytes.extend_from_slice(&0xD503201Fu32.to_le_bytes());
                                    k -= 4;
                                }
                                sections[sec].bytes.extend(std::iter::repeat(0u8).take(k));
                            } else {
                                sections[sec].bytes.extend(std::iter::repeat(0u8).take(pad));
                            }
                        }
                    }
                    Piece::Fixed { bytes, relocs } => {
                        sections[sec].bytes.extend_from_slice(&bytes);
                        for r in relocs {
                            let (si, extra) =
                                sym_for(&r.name, &labels, &is_global, &mut symbols, &mut symidx);
                            sections[sec].relocs.push(Reloc {
                                offset: base + r.at as u64,
                                sym: si,
                                kind: r.kind,
                                addend: extra,
                            });
                        }
                    }
                    Piece::Branch { word, kind, target, reloc } => {
                        let resolvable = match labels.get(&target) {
                            Some((tsec, _)) => *tsec == sec && !is_global.contains_key(target.as_str()),
                            None => false,
                        };
                        let mut b = Buf::new();
                        b.word(word);
                        if resolvable {
                            let (_, tgt) = labels[&target];
                            let rel = tgt as i64 - base as i64;
                            A::apply_fix(&mut b, 0, kind, rel)?;
                        } else {
                            let (si, extra) =
                                sym_for(&target, &labels, &is_global, &mut symbols, &mut symidx);
                            sections[sec].relocs.push(Reloc {
                                offset: base,
                                sym: si,
                                kind: reloc,
                                addend: extra,
                            });
                        }
                        sections[sec].bytes.extend_from_slice(&b.code);
                    }
                }
            }
        }

        Ok(Assembled { sections, symbols })
    }
}

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
        return (*symidx.get(name).unwrap(), 0);
    }
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

// ===========================================================================
// Operanden
// ===========================================================================

/// Eine Speicheradresse in ARM64-Schreibweise.
#[derive(Clone, Debug)]
struct MemOp {
    base: u8,
    /// Verschiebung als Zahl.
    disp: i64,
    /// Indexregister statt Verschiebung.
    index: Option<(u8, bool, String, u32)>, // (reg, ist64, Erweiterung, Verschiebebetrag)
    /// `[x, #…]!`
    pre: bool,
    /// `[x], #…`
    post: bool,
    /// `#:lo12:sym` als Verschiebung.
    lo12: Option<String>,
}

fn parse_mem(s: &str) -> Option<MemOp> {
    let t = s.trim();
    if !t.starts_with('[') {
        return None;
    }
    let close = t.find(']')?;
    let inner = &t[1..close];
    let after = t[close + 1..].trim();
    let pre = after == "!";
    let post = after.starts_with(',');
    let mut m = MemOp {
        base: 0,
        disp: 0,
        index: None,
        pre,
        post,
        lo12: None,
    };
    let parts: Vec<&str> = inner.split(',').map(|x| x.trim()).collect();
    let (b, _, _) = A::gpr_by_name(parts[0])?;
    m.base = b;
    if post {
        // `[xN], #imm`
        let d = after[1..].trim();
        m.disp = parse_int(d)?;
        return Some(m);
    }
    if parts.len() >= 2 {
        let p1 = parts[1];
        if let Some(sym) = p1.strip_prefix("#:lo12:").or_else(|| p1.strip_prefix(":lo12:")) {
            m.lo12 = Some(sym.trim().to_string());
        } else if p1.starts_with('#') || p1.starts_with('-') || p1.chars().next()?.is_ascii_digit() {
            m.disp = parse_int(p1)?;
        } else if let Some((r, w64, _)) = A::gpr_by_name(p1) {
            let mut ext = if w64 { "lsl".to_string() } else { "uxtw".to_string() };
            let mut amt = 0u32;
            if parts.len() >= 3 {
                let p2 = parts[2];
                let mut it = p2.split_whitespace();
                ext = it.next().unwrap_or("lsl").to_ascii_lowercase();
                if let Some(a) = it.next() {
                    amt = parse_int(a)? as u32;
                }
            }
            m.index = Some((r, w64, ext, amt));
        } else {
            return None;
        }
    }
    Some(m)
}

/// Ein Vektorregister mit Anordnung: `v3.16b`, `v3.4s`, `v3.d[1]`.
#[derive(Clone, Debug)]
struct VecOp {
    reg: u8,
    /// `b`, `h`, `s`, `d`, `q`
    elem: char,
    /// Anzahl der Elemente (16, 8, 4, 2, 1); 0 = mit Index
    count: u32,
    index: Option<u32>,
}

fn parse_vec(s: &str) -> Option<VecOp> {
    let t = s.trim().trim_start_matches('{').trim_end_matches('}').trim();
    let dot = t.find('.')?;
    let (r, rest) = t.split_at(dot);
    let (reg, _) = A::fpr_by_name(r)?;
    let rest = &rest[1..];
    if let Some(br) = rest.find('[') {
        let elem = rest.as_bytes()[0] as char;
        let idx: u32 = rest[br + 1..rest.find(']')?].trim().parse().ok()?;
        return Some(VecOp { reg, elem, count: 0, index: Some(idx) });
    }
    let (num, el) = rest.split_at(rest.len() - 1);
    let count: u32 = num.parse().ok()?;
    Some(VecOp { reg, elem: el.as_bytes()[0] as char, count, index: None })
}

fn is_sym(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    if b[0].is_ascii_digit() {
        return b.len() >= 2
            && matches!(b[b.len() - 1], b'f' | b'b')
            && b[..b.len() - 1].iter().all(|c| c.is_ascii_digit());
    }
    (b[0].is_ascii_alphabetic() || b[0] == b'_' || b[0] == b'.')
        && A::gpr_by_name(s).is_none()
        && A::fpr_by_name(s).is_none()
        && !s.contains('[')
}

// ===========================================================================
// Der Befehlssatz
// ===========================================================================

/// Kurzhand: Registernummer als Bitfeld.
#[inline]
fn rd(r: u8) -> u32 {
    r as u32
}
#[inline]
fn rn(r: u8) -> u32 {
    (r as u32) << 5
}
#[inline]
fn rm(r: u8) -> u32 {
    (r as u32) << 16
}
#[inline]
fn ra(r: u8) -> u32 {
    (r as u32) << 10
}

impl Asm {
    fn gpr(&self, s: &str) -> Result<(u8, bool, bool), String> {
        A::gpr_by_name(s.trim()).ok_or_else(|| self.err(format!("kein Register: {}", s)))
    }
    fn fpr(&self, s: &str) -> Result<(u8, char), String> {
        A::fpr_by_name(s.trim()).ok_or_else(|| self.err(format!("kein FP-Register: {}", s)))
    }
    fn imm(&self, s: &str) -> Result<i64, String> {
        parse_int(s).ok_or_else(|| self.err(format!("kein Sofortwert: {}", s)))
    }

    fn instruction(&mut self, s: &str) -> Result<(), String> {
        let (m, rest) = match s.find(char::is_whitespace) {
            Some(i) => (s[..i].to_ascii_lowercase(), s[i..].trim()),
            None => (s.to_ascii_lowercase(), ""),
        };
        let ops = split_operands(rest);
        self.build(&m, &ops)
    }

    fn build(&mut self, m: &str, ops: &[String]) -> Result<(), String> {
        // ---------- Sprünge ----------
        match m {
            "b" | "bl" if ops.len() == 1 && is_sym(&ops[0]) => {
                let t = self.resolve_numeric(&ops[0]);
                let word = if m == "bl" { 0x9400_0000 } else { 0x1400_0000 };
                let reloc = if m == "bl" { R_AARCH64_CALL26 } else { R_AARCH64_JUMP26 };
                self.push(Piece::Branch { word, kind: FixKind::Br26, target: t, reloc });
                return Ok(());
            }
            "ret" => {
                let r = if ops.is_empty() { 30 } else { self.gpr(&ops[0])?.0 };
                self.word(0xD65F_03C0 | rn(r));
                return Ok(());
            }
            "br" => {
                let r = self.gpr(&ops[0])?.0;
                self.word(0xD61F_0000 | rn(r));
                return Ok(());
            }
            "blr" => {
                let r = self.gpr(&ops[0])?.0;
                self.word(0xD63F_0000 | rn(r));
                return Ok(());
            }
            "nop" => {
                self.word(0xD503_201F);
                return Ok(());
            }
            "clrex" => {
                self.word(0xD503_3F5F);
                return Ok(());
            }
            "svc" => {
                let v = self.imm(&ops[0])? as u32;
                self.word(0xD400_0001 | ((v & 0xFFFF) << 5));
                return Ok(());
            }
            "brk" => {
                let v = self.imm(&ops[0])? as u32;
                self.word(0xD420_0000 | ((v & 0xFFFF) << 5));
                return Ok(());
            }
            _ => {}
        }
        if let Some(c) = m.strip_prefix("b.").and_then(A::cond_by_name) {
            let t = self.resolve_numeric(&ops[0]);
            self.push(Piece::Branch {
                word: 0x5400_0000 | c as u32,
                kind: FixKind::Br19,
                target: t,
                reloc: R_AARCH64_CONDBR19,
            });
            return Ok(());
        }
        if matches!(m, "cbz" | "cbnz") && ops.len() == 2 {
            let (r, w64, _) = self.gpr(&ops[0])?;
            let t = self.resolve_numeric(&ops[1]);
            let base = if m == "cbz" { 0x3400_0000u32 } else { 0x3500_0000 };
            self.push(Piece::Branch {
                word: base | ((w64 as u32) << 31) | rd(r),
                kind: FixKind::Br19,
                target: t,
                reloc: R_AARCH64_CONDBR19,
            });
            return Ok(());
        }
        if matches!(m, "tbz" | "tbnz") && ops.len() == 3 {
            let (r, _, _) = self.gpr(&ops[0])?;
            let bit = self.imm(&ops[1])? as u32;
            let t = self.resolve_numeric(&ops[2]);
            let base = if m == "tbz" { 0x3600_0000u32 } else { 0x3700_0000 };
            self.push(Piece::Branch {
                word: base | ((bit >> 5) << 31) | ((bit & 0x1F) << 19) | rd(r),
                kind: FixKind::Br14,
                target: t,
                reloc: R_AARCH64_TSTBR14,
            });
            return Ok(());
        }

        // ---------- adrp / adr ----------
        if matches!(m, "adrp" | "adr") && ops.len() == 2 {
            let (r, _, _) = self.gpr(&ops[0])?;
            let t = self.resolve_numeric(&ops[1]);
            let base = if m == "adrp" { 0x9000_0000u32 } else { 0x1000_0000 };
            self.push(Piece::Branch {
                word: base | rd(r),
                kind: FixKind::Adrp,
                target: t,
                reloc: R_AARCH64_ADR_PREL_PG_HI21,
            });
            return Ok(());
        }

        // ---------- Laden und Speichern ----------
        if let Some(r) = self.try_ldst(m, ops)? {
            let _ = r;
            return Ok(());
        }

        // ---------- Verschieben mit Sofortwert ----------
        if matches!(m, "movz" | "movk" | "movn") && ops.len() >= 2 {
            let (r, w64, _) = self.gpr(&ops[0])?;
            let v = self.imm(&ops[1])? as u64;
            let mut hw = 0u32;
            if ops.len() >= 3 {
                let sh = ops[2].trim().to_ascii_lowercase();
                let n = sh.trim_start_matches("lsl").trim().trim_start_matches('#').trim();
                hw = n.parse::<u32>().map_err(|_| self.err("lsl-Betrag"))? / 16;
            }
            let opc = match m {
                "movn" => 0u32,
                "movz" => 2,
                _ => 3,
            };
            self.word(
                ((w64 as u32) << 31) | (opc << 29) | 0x1280_0000 | (hw << 21)
                    | (((v & 0xFFFF) as u32) << 5)
                    | rd(r),
            );
            return Ok(());
        }
        // Vektorbefehle zuerst: `and v0.16b, …` teilt sich den Namen mit
        // dem Allzweckbefehl `and x0, …`, aber nicht die Kodierung. Die
        // Anordnung (`.16b`) ist das Erkennungszeichen.
        if ops.iter().any(|o| o.contains('.') && !o.contains(':')) {
            if let Some(()) = self.try_simd(m, ops)? {
                return Ok(());
            }
        }

        // ---------- add Xd, Xn, :lo12:sym ----------
        // Die zweite Hälfte der Adressbildung nach `adrp`. Der Wert steht
        // erst beim Binden fest, deshalb ein eigenes Stück mit Umsetzung.
        if m == "add" && ops.len() == 3 {
            let t3 = ops[2].trim();
            if let Some(sym) = t3.strip_prefix("#:lo12:").or_else(|| t3.strip_prefix(":lo12:")) {
                let (d, w64, _) = self.gpr(&ops[0])?;
                let (n, _, _) = self.gpr(&ops[1])?;
                let word = ((w64 as u32) << 31) | 0x1100_0000 | rn(n) | rd(d);
                let t = self.resolve_numeric(sym.trim());
                self.push(Piece::Branch {
                    word,
                    kind: FixKind::Lo12,
                    target: t,
                    reloc: R_AARCH64_ADD_ABS_LO12_NC,
                });
                return Ok(());
            }
        }

        // ---------- Arithmetik und Logik ----------
        if let Some(()) = self.try_alu(m, ops)? {
            return Ok(());
        }

        // ---------- Multiplizieren / Dividieren ----------
        if matches!(m, "mul" | "madd" | "msub" | "mneg" | "umulh" | "smulh" | "udiv" | "sdiv")
        {
            let (d, w64, _) = self.gpr(&ops[0])?;
            let (n, _, _) = self.gpr(&ops[1])?;
            let sf = (w64 as u32) << 31;
            match m {
                "umulh" | "smulh" => {
                    let (mm, _, _) = self.gpr(&ops[2])?;
                    let base = if m == "umulh" { 0x9BC0_7C00u32 } else { 0x9B40_7C00 };
                    self.word(base | rm(mm) | rn(n) | rd(d));
                }
                "udiv" | "sdiv" => {
                    let (mm, _, _) = self.gpr(&ops[2])?;
                    let o = if m == "udiv" { 0x0800u32 } else { 0x0C00 };
                    self.word(sf | 0x1AC0_0000 | rm(mm) | o | rn(n) | rd(d));
                }
                "mul" | "mneg" => {
                    let (mm, _, _) = self.gpr(&ops[2])?;
                    let o0 = if m == "mneg" { 1u32 << 15 } else { 0 };
                    self.word(sf | 0x1B00_0000 | rm(mm) | o0 | ra(A::ZR) | rn(n) | rd(d));
                }
                _ => {
                    let (mm, _, _) = self.gpr(&ops[2])?;
                    let (aa, _, _) = self.gpr(&ops[3])?;
                    let o0 = if m == "msub" { 1u32 << 15 } else { 0 };
                    self.word(sf | 0x1B00_0000 | rm(mm) | o0 | ra(aa) | rn(n) | rd(d));
                }
            }
            return Ok(());
        }

        // ---------- Bedingtes Setzen und Auswählen ----------
        if m == "cset" && ops.len() == 2 {
            let (d, w64, _) = self.gpr(&ops[0])?;
            let c = A::cond_by_name(ops[1].trim()).ok_or_else(|| self.err("Bedingung"))?;
            // cset Wd, cond = csinc Wd, WZR, WZR, invert(cond)
            self.word(
                ((w64 as u32) << 31) | 0x1A80_0400 | rm(A::ZR) | (((c ^ 1) as u32) << 12)
                    | rn(A::ZR)
                    | rd(d),
            );
            return Ok(());
        }
        if matches!(m, "csel" | "csinc" | "csinv" | "csneg") && ops.len() == 4 {
            let (d, w64, _) = self.gpr(&ops[0])?;
            let (n, _, _) = self.gpr(&ops[1])?;
            let (mm, _, _) = self.gpr(&ops[2])?;
            let c = A::cond_by_name(ops[3].trim()).ok_or_else(|| self.err("Bedingung"))?;
            let (op, o2) = match m {
                "csel" => (0u32, 0u32),
                "csinc" => (0, 1),
                "csinv" => (1, 0),
                _ => (1, 1),
            };
            self.word(
                ((w64 as u32) << 31) | (op << 30) | 0x1A80_0000 | rm(mm)
                    | ((c as u32) << 12)
                    | (o2 << 10)
                    | rn(n)
                    | rd(d),
            );
            return Ok(());
        }

        // ---------- Bitfelder und Schiebebefehle ----------
        if let Some(()) = self.try_bitfield(m, ops)? {
            return Ok(());
        }

        // ---------- Ausschließliche Zugriffe ----------
        if matches!(m, "ldaxr" | "ldxr") && ops.len() == 2 {
            let (t, w64, _) = self.gpr(&ops[0])?;
            let mem = parse_mem(&ops[1]).ok_or_else(|| self.err("Adresse"))?;
            let base = if w64 { 0xC85F_7C00u32 } else { 0x885F_7C00 };
            let acq = if m == "ldaxr" { 1u32 << 15 } else { 0 };
            self.word(base | acq | rn(mem.base) | rd(t));
            return Ok(());
        }
        if matches!(m, "stlxr" | "stxr") && ops.len() == 3 {
            let (st, _, _) = self.gpr(&ops[0])?;
            let (t, w64, _) = self.gpr(&ops[1])?;
            let mem = parse_mem(&ops[2]).ok_or_else(|| self.err("Adresse"))?;
            let base = if w64 { 0xC800_7C00u32 } else { 0x8800_7C00 };
            let rel = if m == "stlxr" { 1u32 << 15 } else { 0 };
            self.word(base | rm(st) | rel | rn(mem.base) | rd(t));
            return Ok(());
        }

        // ---------- Systemregister ----------
        if m == "mrs" && ops.len() == 2 {
            let (t, _, _) = self.gpr(&ops[0])?;
            let sr = sysreg(&ops[1]).ok_or_else(|| self.err(format!("Systemregister {}", ops[1])))?;
            self.word(0xD530_0000 | (sr << 5) | rd(t));
            return Ok(());
        }
        if m == "msr" && ops.len() == 2 {
            let sr = sysreg(&ops[0]).ok_or_else(|| self.err(format!("Systemregister {}", ops[0])))?;
            let (t, _, _) = self.gpr(&ops[1])?;
            self.word(0xD510_0000 | (sr << 5) | rd(t));
            return Ok(());
        }

        // ---------- Gleitkomma ----------
        if let Some(()) = self.try_fp(m, ops)? {
            return Ok(());
        }

        // ---------- Vektor und Krypto ----------
        if let Some(()) = self.try_simd(m, ops)? {
            return Ok(());
        }

        Err(self.err(format!("unbekannter Befehl '{}' mit {} Operanden", m, ops.len())))
    }
}

/// Die Systemregister, die Firn benutzt.
fn sysreg(s: &str) -> Option<u32> {
    // Kodierung: op0(2):op1(3):CRn(4):CRm(4):op2(3), zusammen 15 Bit.
    let enc = |op0: u32, op1: u32, crn: u32, crm: u32, op2: u32| {
        ((op0 & 3) << 14) | (op1 << 11) | (crn << 7) | (crm << 3) | op2
    };
    Some(match s.trim().to_ascii_lowercase().as_str() {
        "tpidr_el0" => enc(3, 3, 13, 0, 2),
        "tpidrro_el0" => enc(3, 3, 13, 0, 3),
        "nzcv" => enc(3, 3, 4, 2, 0),
        "fpcr" => enc(3, 3, 4, 4, 0),
        "fpsr" => enc(3, 3, 4, 4, 1),
        "cntvct_el0" => enc(3, 3, 14, 0, 2),
        "cntfrq_el0" => enc(3, 3, 14, 0, 0),
        "midr_el1" => enc(3, 0, 0, 0, 0),
        _ => return None,
    })
}

// ===========================================================================
// Laden und Speichern
// ===========================================================================

impl Asm {
    /// Alle `ldr`/`str`-Formen. Rückgabe `None`, wenn `m` keiner ist.
    fn try_ldst(&mut self, m: &str, ops: &[String]) -> Result<Option<()>, String> {
        // Paare zuerst
        if matches!(m, "ldp" | "stp") && ops.len() == 3 {
            let (t1, w64, _) = self.gpr(&ops[0])?;
            let (t2, _, _) = self.gpr(&ops[1])?;
            let mem = parse_mem(&ops[2]).ok_or_else(|| self.err("Adresse"))?;
            let scale: i64 = if w64 { 8 } else { 4 };
            if mem.disp % scale != 0 {
                return Err(self.err("ldp/stp: Verschiebung nicht passend"));
            }
            let imm7 = ((mem.disp / scale) as u32) & 0x7F;
            let l = if m == "ldp" { 1u32 } else { 0 };
            // Bits 24-23: 01 = nachgestellt, 10 = einfach, 11 = vorgestellt
            let mode = if mem.post { 1u32 } else if mem.pre { 3 } else { 2 };
            let opc = if w64 { 2u32 } else { 0 };
            self.word(
                (opc << 30) | 0x2800_0000 | (mode << 23) | (l << 22) | (imm7 << 15)
                    | ((t2 as u32) << 10)
                    | rn(mem.base)
                    | rd(t1),
            );
            return Ok(Some(()));
        }

        // Einzelzugriffe
        let (is_load, size, v, opc_extra) = match m {
            "ldr" => (true, None, false, 0u32),
            "str" => (false, None, false, 0),
            "ldrb" => (true, Some(0u32), false, 0),
            "strb" => (false, Some(0), false, 0),
            "ldrh" => (true, Some(1), false, 0),
            "strh" => (false, Some(1), false, 0),
            "ldrsw" => (true, Some(2), false, 2),
            "ldrsb" => (true, Some(0), false, 2),
            "ldrsh" => (true, Some(1), false, 2),
            "ldur" | "stur" => (m == "ldur", None, false, 0),
            _ => return Ok(None),
        };
        if ops.len() != 2 {
            return Ok(None);
        }
        let _ = v;

        // Zielregister: Allzweck oder Gleitkomma
        let (treg, size2, isv) = if let Some((r, w64, _)) = A::gpr_by_name(ops[0].trim()) {
            (r, size.unwrap_or(if w64 { 3 } else { 2 }), false)
        } else {
            let (r, c) = self.fpr(&ops[0])?;
            let sz = match c {
                'b' => 0u32,
                'h' => 1,
                's' => 2,
                'd' => 3,
                'q' => 0, // Sonderfall: size=00 mit opc=1x
                _ => return Err(self.err("FP-Breite")),
            };
            (r, sz, true)
        };
        let is_q = isv && ops[0].trim().starts_with('q');
        let mem = parse_mem(&ops[1]).ok_or_else(|| self.err(format!("Adresse: {}", ops[1])))?;

        // opc: 00 = speichern, 01 = laden, 10/11 = vorzeichenerweitert
        //      bei V=1 und Q: 10 = speichern, 11 = laden
        let opc: u32 = if is_q {
            if is_load { 3 } else { 2 }
        } else if opc_extra != 0 {
            // ldrsw/ldrsb/ldrsh nach X
            if ops[0].trim().starts_with('x') { 2 } else { 3 }
        } else if is_load {
            1
        } else {
            0
        };

        // --- Umsetzung auf ein Symbol (#:lo12:) ---
        if let Some(sym) = mem.lo12.clone() {
            let scale = 1u32 << size2;
            let word = (size2 << 30) | ((isv as u32) << 26) | 0x3900_0000 | (opc << 22)
                | rn(mem.base)
                | rd(treg);
            let kind = if is_q {
                R_AARCH64_LDST128_ABS_LO12_NC
            } else {
                match size2 {
                    0 => R_AARCH64_LDST8_ABS_LO12_NC,
                    1 => R_AARCH64_LDST16_ABS_LO12_NC,
                    2 => R_AARCH64_LDST32_ABS_LO12_NC,
                    _ => R_AARCH64_LDST64_ABS_LO12_NC,
                }
            };
            let _ = scale;
            let t = self.resolve_numeric(&sym);
            self.push(Piece::Branch {
                word,
                kind: FixKind::Lo12Scaled(if is_q { 16 } else { 1 << size2 }),
                target: t,
                reloc: kind,
            });
            return Ok(Some(()));
        }

        // --- Registerindex ---
        if let Some((ir, i64r, ext, amt)) = mem.index.clone() {
            let option: u32 = match ext.as_str() {
                "uxtw" => 2,
                "lsl" => 3,
                "sxtw" => 6,
                "sxtx" => 7,
                _ => {
                    if i64r {
                        3
                    } else {
                        2
                    }
                }
            };
            let sbit = if amt != 0 { 1u32 } else { 0 };
            self.word(
                (size2 << 30) | ((isv as u32) << 26) | 0x3800_0800 | (opc << 22) | (1 << 21)
                    | rm(ir)
                    | (option << 13)
                    | (sbit << 12)
                    | rn(mem.base)
                    | rd(treg),
            );
            return Ok(Some(()));
        }

        // --- Vor-/nachgestellte Aktualisierung ---
        if mem.pre || mem.post {
            let imm9 = (mem.disp as u32) & 0x1FF;
            let mode = if mem.pre { 3u32 } else { 1 };
            self.word(
                (size2 << 30) | ((isv as u32) << 26) | 0x3800_0000 | (opc << 22) | (imm9 << 12)
                    | (mode << 10)
                    | rn(mem.base)
                    | rd(treg),
            );
            return Ok(Some(()));
        }

        // --- Skalierte Verschiebung, sonst unskaliert ---
        // ACHTUNG bei `q`: im Befehl steht size=00, die Verschiebung wird
        // aber mit SECHZEHN skaliert (die Breite ergibt sich aus
        // `opc<1>:size`, nicht aus `size` allein). Wer hier 1 nimmt, hält
        // `ldr q0, [sp, #8672]` für unkodierbar -- oder, schlimmer, kodiert
        // eine falsche Adresse.
        let scale = if is_q { 16i64 } else { 1i64 << size2 };
        let unscaled_forced = m == "ldur" || m == "stur";
        if !unscaled_forced && mem.disp >= 0 && mem.disp % scale == 0 && (mem.disp / scale) <= 0xFFF
        {
            let imm12 = (mem.disp / scale) as u32;
            self.word(
                (size2 << 30) | ((isv as u32) << 26) | 0x3900_0000 | (opc << 22) | (imm12 << 10)
                    | rn(mem.base)
                    | rd(treg),
            );
        } else {
            // ldur/stur: unskaliert, 9 Bit mit Vorzeichen
            if !(-256..=255).contains(&mem.disp) {
                return Err(self.err(format!("Verschiebung {} passt in keine Ladeform", mem.disp)));
            }
            let imm9 = (mem.disp as u32) & 0x1FF;
            self.word(
                (size2 << 30) | ((isv as u32) << 26) | 0x3800_0000 | (opc << 22) | (imm9 << 12)
                    | rn(mem.base)
                    | rd(treg),
            );
        }
        Ok(Some(()))
    }
}

// ===========================================================================
// Arithmetik und Logik
// ===========================================================================

impl Asm {
    fn try_alu(&mut self, m: &str, ops: &[String]) -> Result<Option<()>, String> {
        // `mov` ist ein Deckname: zwischen Registern `orr Xd, XZR, Xm`,
        // mit `sp` dagegen `add Xd, Xn, #0`, und mit Sofortwert `movz`.
        if m == "mov" && ops.len() == 2 {
            if let (Ok((d, w64, dsp)), Ok((s, _, ssp))) =
                (self.gpr(&ops[0]), self.gpr(&ops[1]))
            {
                if dsp || ssp {
                    self.word(((w64 as u32) << 31) | 0x1100_0000 | rn(s) | rd(d));
                } else {
                    self.word(((w64 as u32) << 31) | 0x2A00_0000 | rm(s) | rn(A::ZR) | rd(d));
                }
                return Ok(Some(()));
            }
            // mov Xd, #imm
            if let Ok((d, w64, _)) = self.gpr(&ops[0]) {
                if let Some(v) = parse_int(&ops[1]) {
                    let uv = if w64 { v as u64 } else { (v as u32) as u64 };
                    // movz, wenn nur ein 16-Bit-Feld gesetzt ist
                    for hw in 0..if w64 { 4 } else { 2 } {
                        let part = (uv >> (16 * hw)) & 0xFFFF;
                        if uv == part << (16 * hw) {
                            self.word(
                                ((w64 as u32) << 31) | 0x5280_0000 | ((hw as u32) << 21)
                                    | ((part as u32) << 5)
                                    | rd(d),
                            );
                            return Ok(Some(()));
                        }
                    }
                    // sonst movn, wenn das Komplement passt
                    let nv = !uv & if w64 { u64::MAX } else { 0xFFFF_FFFF };
                    for hw in 0..if w64 { 4 } else { 2 } {
                        let part = (nv >> (16 * hw)) & 0xFFFF;
                        if nv == part << (16 * hw) {
                            self.word(
                                ((w64 as u32) << 31) | 0x1280_0000 | ((hw as u32) << 21)
                                    | ((part as u32) << 5)
                                    | rd(d),
                            );
                            return Ok(Some(()));
                        }
                    }
                    // sonst als logischer Sofortwert
                    if let Some((n, immr, imms)) = A::logical_imm(uv, w64) {
                        self.word(
                            ((w64 as u32) << 31) | 0x3200_0000 | (n << 22) | (immr << 16)
                                | (imms << 10)
                                | rn(A::ZR)
                                | rd(d),
                        );
                        return Ok(Some(()));
                    }
                    return Err(self.err(format!("mov mit #{} nicht kodierbar", v)));
                }
            }
            return Ok(None);
        }

        let (kind, sf_op, s_bit) = match m {
            "add" => (0u8, 0u32, 0u32),
            "adds" => (0, 0, 1),
            "sub" => (0, 1, 0),
            "subs" => (0, 1, 1),
            "cmp" => (0, 1, 1),
            "cmn" => (0, 0, 1),
            "tst" => (1, 0, 1),
            "neg" => (0, 1, 0),
            "and" => (1, 0, 0),
            "ands" => (1, 0, 1),
            "orr" => (1, 1, 0),
            "eor" => (1, 2, 0),
            "bic" => (1, 4, 0),
            "orn" => (1, 5, 0),
            "eon" => (1, 6, 0),
            "mvn" => (1, 5, 0),
            _ => return Ok(None),
        };

        // Operanden ordnen: cmp/cmn/neg/mvn haben ein verstecktes Register
        let (dstr, nstr, mstr, shstr): (&str, &str, &str, Option<&str>) = match m {
            // `cmp`/`cmn`/`tst` schreiben ihr Ergebnis nach XZR -- sie sind
            // Decknamen von `subs`/`adds`/`ands` mit verworfenem Ziel.
            "cmp" | "cmn" | "tst" => {
                ("xzr", ops[0].as_str(), ops[1].as_str(), ops.get(2).map(|x| x.as_str()))
            }
            "neg" | "mvn" => ("", ops[0].as_str(), ops[1].as_str(), ops.get(2).map(|x| x.as_str())),
            _ => {
                if ops.len() < 3 {
                    return Ok(None);
                }
                (ops[0].as_str(), ops[1].as_str(), ops[2].as_str(), ops.get(3).map(|x| x.as_str()))
            }
        };

        let (d, w64, dsp) = if matches!(m, "neg" | "mvn") {
            let (r, w, sp) = self.gpr(ops[0].trim())?;
            (r, w, sp)
        } else if dstr == "xzr" && matches!(m, "cmp" | "cmn" | "tst") {
            let (_, w, _) = self.gpr(nstr)?;
            (A::ZR, w, false)
        } else {
            self.gpr(dstr)?
        };
        let (n, _, nsp) = if matches!(m, "neg" | "mvn") {
            (A::ZR, w64, false)
        } else {
            self.gpr(nstr)?
        };
        let sf = (w64 as u32) << 31;

        // --- zweiter Operand: Sofortwert ---
        if let Some(v) = parse_int(mstr) {
            if kind == 0 {
                // `add x12, sp, #22, lsl #12` -- die Verschiebung um zwoelf
                // Stellen ist ein EIGENES Bit im Befehl, kein Rechenschritt.
                // Steht sie im Text, ist der Sofortwert schon der geschobene.
                let explicit_lsl12 = shstr
                    .map(|x| x.trim().to_ascii_lowercase().starts_with("lsl"))
                    .unwrap_or(false);
                let (imm12, sh) = if explicit_lsl12 {
                    let amt: u32 = shstr
                        .and_then(|x| parse_int(x.trim().trim_start_matches("lsl")))
                        .unwrap_or(0) as u32;
                    if amt != 12 {
                        return Err(self.err("add/sub: nur 'lsl #12' ist zulässig"));
                    }
                    (v.unsigned_abs() as u32 & 0xFFF, 1u32)
                } else {
                    A::addsub_imm(v.unsigned_abs())
                        .ok_or_else(|| self.err(format!("add/sub #{} nicht kodierbar", v)))?
                };
                let neg = v < 0;
                let opb = if neg { 1 - sf_op } else { sf_op };
                self.word(
                    sf | (opb << 30) | (s_bit << 29) | 0x1100_0000 | (sh << 22) | (imm12 << 10)
                        | rn(n)
                        | rd(d),
                );
            } else {
                let uv = if w64 { v as u64 } else { (v as u32) as u64 };
                let uv = if matches!(m, "bic" | "orn" | "eon" | "mvn") { !uv } else { uv };
                let uv = if !w64 { uv & 0xFFFF_FFFF } else { uv };
                let (nn, immr, imms) = A::logical_imm(uv, w64)
                    .ok_or_else(|| self.err(format!("logischer Sofortwert #{} nicht kodierbar", v)))?;
                let opc = match m {
                    "and" | "bic" => 0u32,
                    "orr" | "orn" | "mvn" => 1,
                    "eor" | "eon" => 2,
                    _ => 3, // ands, tst
                };
                self.word(
                    sf | (opc << 29) | 0x1200_0000 | (nn << 22) | (immr << 16) | (imms << 10)
                        | rn(n)
                        | rd(d),
                );
            }
            let _ = (dsp, nsp);
            return Ok(Some(()));
        }

        // --- zweiter Operand: Register ---
        let (mm, mw64, _) = self.gpr(mstr)?;
        // Verschiebung oder Erweiterung
        let mut shift_type = 0u32;
        let mut shift_amt = 0u32;
        let mut ext: Option<(u32, u32)> = None;
        if let Some(sh) = shstr {
            let sh = sh.trim().to_ascii_lowercase();
            let mut it = sh.split_whitespace();
            let word = it.next().unwrap_or("");
            let amt = it
                .next()
                .and_then(parse_int)
                .unwrap_or(0) as u32;
            match word {
                "lsl" => {
                    shift_type = 0;
                    shift_amt = amt;
                }
                "lsr" => {
                    shift_type = 1;
                    shift_amt = amt;
                }
                "asr" => {
                    shift_type = 2;
                    shift_amt = amt;
                }
                "ror" => {
                    shift_type = 3;
                    shift_amt = amt;
                }
                "uxtb" => ext = Some((0, amt)),
                "uxth" => ext = Some((1, amt)),
                "uxtw" => ext = Some((2, amt)),
                "uxtx" => ext = Some((3, amt)),
                "sxtb" => ext = Some((4, amt)),
                "sxth" => ext = Some((5, amt)),
                "sxtw" => ext = Some((6, amt)),
                "sxtx" => ext = Some((7, amt)),
                o => return Err(self.err(format!("unbekannte Verschiebung {}", o))),
            }
        }
        // `add x0, sp, x1` verlangt die erweiterte Form -- `sp` ist im
        // geschobenen Register-Format nicht darstellbar.
        if kind == 0 && (dsp || nsp) && ext.is_none() {
            ext = Some((if mw64 { 3 } else { 2 }, shift_amt));
        }
        if kind == 0 {
            if let Some((option, amt)) = ext {
                self.word(
                    sf | (sf_op << 30) | (s_bit << 29) | 0x0B20_0000 | rm(mm) | (option << 13)
                        | (amt << 10)
                        | rn(n)
                        | rd(d),
                );
            } else {
                self.word(
                    sf | (sf_op << 30) | (s_bit << 29) | 0x0B00_0000 | (shift_type << 22)
                        | rm(mm)
                        | (shift_amt << 10)
                        | rn(n)
                        | rd(d),
                );
            }
        } else {
            let (opc, nbit) = match m {
                "tst" | "ands" => (3u32, 0u32),
                "and" => (0u32, 0u32),
                "bic" => (0, 1),
                "orr" => (1, 0),
                "orn" | "mvn" => (1, 1),
                "eor" => (2, 0),
                "eon" => (2, 1),
                _ => (3, 0),
            };
            self.word(
                sf | (opc << 29) | 0x0A00_0000 | (shift_type << 22) | (nbit << 21) | rm(mm)
                    | (shift_amt << 10)
                    | rn(n)
                    | rd(d),
            );
        }
        Ok(Some(()))
    }
}

// ===========================================================================
// Bitfelder und Schiebebefehle
// ===========================================================================

impl Asm {
    fn try_bitfield(&mut self, m: &str, ops: &[String]) -> Result<Option<()>, String> {
        // Schieben um ein REGISTER ist ein eigener Befehl (lslv/lsrv/…),
        // Schieben um einen SOFORTWERT dagegen ein Deckname fuer ubfm/sbfm.
        if matches!(m, "lsl" | "lsr" | "asr" | "ror") && ops.len() == 3 {
            let (d, w64, _) = self.gpr(&ops[0])?;
            let (n, _, _) = self.gpr(&ops[1])?;
            let sf = (w64 as u32) << 31;
            let width = if w64 { 64u32 } else { 32 };
            if let Some(v) = parse_int(&ops[2]) {
                let sh = (v as u32) % width;
                let (immr, imms, is_sbfm) = match m {
                    "lsl" => ((width - sh) % width, width - 1 - sh, false),
                    "lsr" => (sh, width - 1, false),
                    "asr" => (sh, width - 1, true),
                    _ => {
                        // ror Xd, Xn, #sh = extr Xd, Xn, Xn, #sh
                        let nb = if w64 { 1u32 } else { 0 };
                        self.word(
                            sf | 0x1380_0000 | (nb << 22) | rm(n) | (sh << 10) | rn(n) | rd(d),
                        );
                        return Ok(Some(()));
                    }
                };
                let nb = if w64 { 1u32 } else { 0 };
                let base = if is_sbfm { 0x1300_0000u32 } else { 0x5300_0000 };
                self.word(sf | base | (nb << 22) | (immr << 16) | (imms << 10) | rn(n) | rd(d));
                return Ok(Some(()));
            }
            let (mm, _, _) = self.gpr(&ops[2])?;
            let opc = match m {
                "lsl" => 8u32,
                "lsr" => 9,
                "asr" => 10,
                _ => 11,
            };
            self.word(sf | 0x1AC0_0000 | rm(mm) | (opc << 10) | rn(n) | rd(d));
            return Ok(Some(()));
        }

        if matches!(m, "ubfx" | "sbfx" | "ubfiz" | "sbfiz" | "ubfm" | "sbfm" | "bfm" | "bfi" | "bfxil")
            && ops.len() == 4
        {
            let (d, w64, _) = self.gpr(&ops[0])?;
            let (n, _, _) = self.gpr(&ops[1])?;
            let a = self.imm(&ops[2])? as u32;
            let b = self.imm(&ops[3])? as u32;
            let width = if w64 { 64u32 } else { 32 };
            let (immr, imms) = match m {
                "ubfx" | "sbfx" | "bfxil" => (a, a + b - 1),
                "ubfiz" | "sbfiz" | "bfi" => ((width - a) % width, b - 1),
                _ => (a, b),
            };
            let nb = if w64 { 1u32 } else { 0 };
            let base = match m {
                "sbfx" | "sbfiz" | "sbfm" => 0x1300_0000u32,
                "bfm" | "bfi" | "bfxil" => 0x3300_0000,
                _ => 0x5300_0000,
            };
            self.word(
                ((w64 as u32) << 31) | base | (nb << 22) | (immr << 16) | (imms << 10) | rn(n)
                    | rd(d),
            );
            return Ok(Some(()));
        }

        // Vorzeichen-/Nullerweiterung: allesamt Decknamen von ubfm/sbfm
        if matches!(m, "uxtb" | "uxth" | "sxtb" | "sxth" | "sxtw" | "uxtw") && ops.len() == 2 {
            let (d, w64, _) = self.gpr(&ops[0])?;
            let (n, _, _) = self.gpr(&ops[1])?;
            let (base, imms, sf, nb) = match m {
                "uxtb" => (0x5300_0000u32, 7u32, 0u32, 0u32),
                "uxth" => (0x5300_0000, 15, 0, 0),
                "sxtb" => (0x1300_0000, 7, (w64 as u32) << 31, w64 as u32),
                "sxth" => (0x1300_0000, 15, (w64 as u32) << 31, w64 as u32),
                "sxtw" => (0x1300_0000, 31, 1 << 31, 1),
                _ => {
                    // uxtw Xd, Wn = mov Wd, Wn
                    self.word(0x2A00_0000 | rm(n) | rn(A::ZR) | rd(d));
                    return Ok(Some(()));
                }
            };
            self.word(sf | base | (nb << 22) | (imms << 10) | rn(n) | rd(d));
            return Ok(Some(()));
        }

        if m == "extr" && ops.len() == 4 {
            let (d, w64, _) = self.gpr(&ops[0])?;
            let (n, _, _) = self.gpr(&ops[1])?;
            let (mm, _, _) = self.gpr(&ops[2])?;
            let sh = self.imm(&ops[3])? as u32;
            let nb = if w64 { 1u32 } else { 0 };
            self.word(
                ((w64 as u32) << 31) | 0x1380_0000 | (nb << 22) | rm(mm) | (sh << 10) | rn(n)
                    | rd(d),
            );
            return Ok(Some(()));
        }

        if matches!(m, "rev" | "rev16" | "rev32" | "clz" | "rbit") && ops.len() == 2 {
            let (d, w64, _) = self.gpr(&ops[0])?;
            let (n, _, _) = self.gpr(&ops[1])?;
            let opc = match m {
                "rbit" => 0u32,
                "rev16" => 1,
                "rev32" => 2,
                "rev" => {
                    if w64 {
                        3
                    } else {
                        2
                    }
                }
                _ => 4,
            };
            let o = if m == "clz" { 0x1000u32 } else { opc << 10 };
            self.word(((w64 as u32) << 31) | 0x5AC0_0000 | o | rn(n) | rd(d));
            return Ok(Some(()));
        }

        if matches!(m, "crc32b" | "crc32h" | "crc32w" | "crc32x" | "crc32cb" | "crc32ch"
            | "crc32cw" | "crc32cx")
            && ops.len() == 3
        {
            let (d, _, _) = self.gpr(&ops[0])?;
            let (n, _, _) = self.gpr(&ops[1])?;
            let (mm, _, _) = self.gpr(&ops[2])?;
            let c = m.starts_with("crc32c");
            let sz = match m.chars().last().unwrap() {
                'b' => 0u32,
                'h' => 1,
                'w' => 2,
                _ => 3,
            };
            let sf = if sz == 3 { 1u32 << 31 } else { 0 };
            self.word(
                sf | 0x1AC0_4000 | rm(mm) | ((c as u32) << 12) | (sz << 10) | rn(n) | rd(d),
            );
            return Ok(Some(()));
        }

        Ok(None)
    }
}

// ===========================================================================
// Gleitkomma
// ===========================================================================

impl Asm {
    fn try_fp(&mut self, m: &str, ops: &[String]) -> Result<Option<()>, String> {
        // ftype: 00 = einfach (s), 01 = doppelt (d), 11 = halb (h)
        let ftype = |c: char| -> u32 {
            match c {
                's' => 0,
                'd' => 1,
                _ => 3,
            }
        };
        if matches!(m, "fadd" | "fsub" | "fmul" | "fdiv" | "fmax" | "fmin" | "fnmul")
            && ops.len() == 3
        {
            let (d, c) = self.fpr(&ops[0])?;
            let (n, _) = self.fpr(&ops[1])?;
            let (mm, _) = self.fpr(&ops[2])?;
            let opc = match m {
                "fmul" => 0u32,
                "fdiv" => 1,
                "fadd" => 2,
                "fsub" => 3,
                "fmax" => 4,
                "fmin" => 5,
                _ => 8,
            };
            self.word(0x1E20_0800 | (ftype(c) << 22) | rm(mm) | (opc << 12) | rn(n) | rd(d));
            return Ok(Some(()));
        }
        if matches!(m, "fcmp" | "fcmpe") && ops.len() == 2 {
            let (n, c) = self.fpr(&ops[0])?;
            if let Ok((mm, _)) = self.fpr(&ops[1]) {
                self.word(0x1E20_2000 | (ftype(c) << 22) | rm(mm) | rn(n));
            } else {
                // fcmp Dn, #0.0
                self.word(0x1E20_2008 | (ftype(c) << 22) | rn(n));
            }
            return Ok(Some(()));
        }
        if matches!(m, "fneg" | "fabs" | "fsqrt" | "fmov" | "fcvt") && ops.len() == 2 {
            // fmov zwischen Allzweck- und FP-Register
            if m == "fmov" {
                let a_is_gpr = A::gpr_by_name(ops[0].trim()).is_some();
                let b_is_gpr = A::gpr_by_name(ops[1].trim()).is_some();
                if a_is_gpr || b_is_gpr {
                    let (g, w64, _) = self.gpr(if a_is_gpr { &ops[0] } else { &ops[1] })?;
                    let (f, c) = self.fpr(if a_is_gpr { &ops[1] } else { &ops[0] })?;
                    // rmode=00, opcode: 110 = FP->GPR, 111 = GPR->FP
                    let opcode = if a_is_gpr { 6u32 } else { 7 };
                    self.word(
                        ((w64 as u32) << 31) | 0x1E20_0000 | (ftype(c) << 22) | (opcode << 16)
                            | rn(if a_is_gpr { f } else { g })
                            | rd(if a_is_gpr { g } else { f }),
                    );
                    return Ok(Some(()));
                }
            }
            let (d, dc) = self.fpr(&ops[0])?;
            let (n, nc) = self.fpr(&ops[1])?;
            if m == "fcvt" {
                let to = ftype(dc);
                self.word(0x1E22_4000 | (ftype(nc) << 22) | (to << 15) | rn(n) | rd(d));
                return Ok(Some(()));
            }
            let opc = match m {
                "fmov" => 0u32,
                "fabs" => 1,
                "fneg" => 2,
                _ => 3,
            };
            self.word(0x1E20_4000 | (ftype(dc) << 22) | (opc << 15) | rn(n) | rd(d));
            return Ok(Some(()));
        }
        if matches!(m, "scvtf" | "ucvtf" | "fcvtzs" | "fcvtzu" | "fcvtms" | "fcvtps" | "fcvtas")
            && ops.len() == 2
        {
            let a_is_gpr = A::gpr_by_name(ops[0].trim()).is_some();
            if a_is_gpr {
                // fcvtzs Xd, Dn
                let (d, w64, _) = self.gpr(&ops[0])?;
                let (n, c) = self.fpr(&ops[1])?;
                let (rmode, opcode) = match m {
                    "fcvtzs" => (3u32, 0u32),
                    "fcvtzu" => (3, 1),
                    "fcvtms" => (2, 0),
                    "fcvtps" => (1, 0),
                    _ => (0, 4),
                };
                self.word(
                    ((w64 as u32) << 31) | 0x1E20_0000 | (ftype(c) << 22) | (rmode << 19)
                        | (opcode << 16)
                        | rn(n)
                        | rd(d),
                );
            } else {
                // scvtf Dd, Xn
                let (d, c) = self.fpr(&ops[0])?;
                let (n, w64, _) = self.gpr(&ops[1])?;
                let opcode = if m == "scvtf" { 2u32 } else { 3 };
                self.word(
                    ((w64 as u32) << 31) | 0x1E20_0000 | (ftype(c) << 22) | (opcode << 16)
                        | rn(n)
                        | rd(d),
                );
            }
            return Ok(Some(()));
        }
        Ok(None)
    }
}

// ===========================================================================
// Vektor und Krypto
// ===========================================================================

impl Asm {
    fn try_simd(&mut self, m: &str, ops: &[String]) -> Result<Option<()>, String> {
        // AES/SHA: `aese v0.16b, v1.16b`
        if let Some(base) = match m {
            "aese" => Some(0x4E28_4800u32),
            "aesd" => Some(0x4E28_5800),
            "aesmc" => Some(0x4E28_6800),
            "aesimc" => Some(0x4E28_7800),
            "sha256su0" => Some(0x5E28_2800),
            "sha1h" => Some(0x5E28_0800),
            "sha1su1" => Some(0x5E28_1800),
            _ => None,
        } {
            let a = parse_vec(&ops[0]).ok_or_else(|| self.err("Vektorregister"))?;
            let b = parse_vec(&ops[1]).ok_or_else(|| self.err("Vektorregister"))?;
            self.word(base | rn(b.reg) | rd(a.reg));
            return Ok(Some(()));
        }

        // `ins v0.s[1], w2`  /  `ins v0.d[0], x1`
        if m == "ins" && ops.len() == 2 {
            let v = parse_vec(&ops[0]).ok_or_else(|| self.err("Vektorregister"))?;
            let idx = v.index.ok_or_else(|| self.err("ins braucht einen Index"))?;
            let (g, _, _) = self.gpr(&ops[1])?;
            let (sz, imm5) = match v.elem {
                'b' => (0u32, (idx << 1) | 1),
                'h' => (1, (idx << 2) | 2),
                's' => (2, (idx << 3) | 4),
                _ => (3, (idx << 4) | 8),
            };
            let _ = sz;
            self.word(0x4E00_1C00 | (imm5 << 16) | rn(g) | rd(v.reg));
            return Ok(Some(()));
        }
        // `umov w0, v1.s[2]` / `umov x0, v1.d[1]`
        if matches!(m, "umov" | "mov") && ops.len() == 2 && A::gpr_by_name(ops[0].trim()).is_some()
        {
            if let Some(v) = parse_vec(&ops[1]) {
                let idx = v.index.ok_or_else(|| self.err("umov braucht einen Index"))?;
                let (g, _, _) = self.gpr(&ops[0])?;
                let (q, imm5) = match v.elem {
                    'b' => (0u32, (idx << 1) | 1),
                    'h' => (0, (idx << 2) | 2),
                    's' => (0, (idx << 3) | 4),
                    _ => (1, (idx << 4) | 8),
                };
                // Das Vektorregister steht im `Rn`-Feld -- ohne das liest der
                // Befehl aus v0 statt aus dem gemeinten Register.
                self.word(0x0E00_3C00 | (q << 30) | (imm5 << 16) | rn(v.reg) | rd(g));
                return Ok(Some(()));
            }
        }
        // `mov v0.16b, v1.16b` = orr
        if m == "mov" && ops.len() == 2 {
            if let (Some(a), Some(b)) = (parse_vec(&ops[0]), parse_vec(&ops[1])) {
                self.word(0x4EA0_1C00 | rm(b.reg) | rn(b.reg) | rd(a.reg));
                return Ok(Some(()));
            }
        }
        // Bitweise auf 16b
        if let Some((base, three)) = match m {
            "and" => Some((0x4E20_1C00u32, true)),
            "bic" => Some((0x4E60_1C00, true)),
            "orr" => Some((0x4EA0_1C00, true)),
            "orn" => Some((0x4EE0_1C00, true)),
            "eor" => Some((0x6E20_1C00, true)),
            "bsl" => Some((0x6E60_1C00, true)),
            "bit" => Some((0x6EA0_1C00, true)),
            "bif" => Some((0x6EE0_1C00, true)),
            _ => None,
        } {
            if three && ops.len() == 3 {
                if let (Some(a), Some(b), Some(c)) =
                    (parse_vec(&ops[0]), parse_vec(&ops[1]), parse_vec(&ops[2]))
                {
                    self.word(base | rm(c.reg) | rn(b.reg) | rd(a.reg));
                    return Ok(Some(()));
                }
            }
        }
        // add/sub auf Vektoren
        if matches!(m, "add" | "sub") && ops.len() == 3 {
            if let (Some(a), Some(b), Some(c)) =
                (parse_vec(&ops[0]), parse_vec(&ops[1]), parse_vec(&ops[2]))
            {
                let size = match a.elem {
                    'b' => 0u32,
                    'h' => 1,
                    's' => 2,
                    _ => 3,
                };
                let q = if a.count * elem_bits(a.elem) == 128 { 1u32 } else { 0 };
                let base = if m == "add" { 0x0E20_8400u32 } else { 0x2E20_8400 };
                self.word(base | (q << 30) | (size << 22) | rm(c.reg) | rn(b.reg) | rd(a.reg));
                return Ok(Some(()));
            }
        }
        // movi v0.16b, #imm
        if m == "movi" && ops.len() == 2 {
            let a = parse_vec(&ops[0]).ok_or_else(|| self.err("Vektorregister"))?;
            let v = self.imm(&ops[1])? as u32 & 0xFF;
            let q = if a.count * elem_bits(a.elem) == 128 { 1u32 } else { 0 };
            // cmode=1110 (8-Bit-Wiederholung), op=0
            self.word(
                0x0F00_0400 | (q << 30) | (((v >> 5) & 7) << 16) | (0xE << 12) | ((v & 0x1F) << 5)
                    | rd(a.reg),
            );
            return Ok(Some(()));
        }
        // shl / ushr / sshr mit Sofortwert
        if matches!(m, "shl" | "ushr" | "sshr") && ops.len() == 3 {
            let a = parse_vec(&ops[0]).ok_or_else(|| self.err("Vektorregister"))?;
            let b = parse_vec(&ops[1]).ok_or_else(|| self.err("Vektorregister"))?;
            let sh = self.imm(&ops[2])? as u32;
            let eb = elem_bits(a.elem);
            let q = if a.count * eb == 128 { 1u32 } else { 0 };
            let immh_immb = if m == "shl" { eb + sh } else { 2 * eb - sh };
            let base = if m == "shl" {
                0x0F00_5400u32
            } else if m == "ushr" {
                0x2F00_0400
            } else {
                0x0F00_0400
            };
            self.word(base | (q << 30) | (immh_immb << 16) | rn(b.reg) | rd(a.reg));
            return Ok(Some(()));
        }
        // zip1 / zip2
        if matches!(m, "zip1" | "zip2" | "uzp1" | "uzp2" | "trn1" | "trn2") && ops.len() == 3 {
            let a = parse_vec(&ops[0]).ok_or_else(|| self.err("Vektorregister"))?;
            let b = parse_vec(&ops[1]).ok_or_else(|| self.err("Vektorregister"))?;
            let c = parse_vec(&ops[2]).ok_or_else(|| self.err("Vektorregister"))?;
            let size = match a.elem {
                'b' => 0u32,
                'h' => 1,
                's' => 2,
                _ => 3,
            };
            let q = if a.count * elem_bits(a.elem) == 128 { 1u32 } else { 0 };
            let opc = match m {
                "uzp1" => 1u32,
                "trn1" => 2,
                "zip1" => 3,
                "uzp2" => 5,
                "trn2" => 6,
                _ => 7,
            };
            self.word(
                0x0E00_0800 | (q << 30) | (size << 22) | rm(c.reg) | (opc << 12) | rn(b.reg)
                    | rd(a.reg),
            );
            return Ok(Some(()));
        }
        // ext v0.16b, v1.16b, v2.16b, #n
        if m == "ext" && ops.len() == 4 {
            let a = parse_vec(&ops[0]).ok_or_else(|| self.err("Vektorregister"))?;
            let b = parse_vec(&ops[1]).ok_or_else(|| self.err("Vektorregister"))?;
            let c = parse_vec(&ops[2]).ok_or_else(|| self.err("Vektorregister"))?;
            let n = self.imm(&ops[3])? as u32;
            let q = if a.count * elem_bits(a.elem) == 128 { 1u32 } else { 0 };
            self.word(0x2E00_0000 | (q << 30) | rm(c.reg) | (n << 11) | rn(b.reg) | rd(a.reg));
            return Ok(Some(()));
        }
        // tbl v0.16b, {v1.16b}, v2.16b
        if m == "tbl" && ops.len() == 3 {
            let a = parse_vec(&ops[0]).ok_or_else(|| self.err("Vektorregister"))?;
            let b = parse_vec(&ops[1]).ok_or_else(|| self.err("Vektorliste"))?;
            let c = parse_vec(&ops[2]).ok_or_else(|| self.err("Vektorregister"))?;
            let q = if a.count * elem_bits(a.elem) == 128 { 1u32 } else { 0 };
            self.word(0x0E00_0000 | (q << 30) | rm(c.reg) | rn(b.reg) | rd(a.reg));
            return Ok(Some(()));
        }
        // pmull v0.1q, v1.1d, v2.1d
        if matches!(m, "pmull" | "pmull2") && ops.len() == 3 {
            let a = parse_vec(&ops[0]).ok_or_else(|| self.err("Vektorregister"))?;
            let b = parse_vec(&ops[1]).ok_or_else(|| self.err("Vektorregister"))?;
            let c = parse_vec(&ops[2]).ok_or_else(|| self.err("Vektorregister"))?;
            let size = if a.elem == 'q' { 3u32 } else { 0 };
            let q = if m == "pmull2" { 1u32 } else { 0 };
            self.word(0x0E20_E000 | (q << 30) | (size << 22) | rm(c.reg) | rn(b.reg) | rd(a.reg));
            return Ok(Some(()));
        }
        Ok(None)
    }
}

fn elem_bits(c: char) -> u32 {
    match c {
        'b' => 8,
        'h' => 16,
        's' => 32,
        'd' => 64,
        _ => 128,
    }
}
