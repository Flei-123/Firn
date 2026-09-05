//! **RUNDE KODIERER — vom Assemblertext zur Objektdatei, ohne `as`.**
//!
//! `x86enc.rs` kann einzelne Befehle in Oktette verwandeln. Diese Datei macht
//! daraus einen Assembler: sie zerteilt genau den Text, den `codegen_x86.rs`
//! schreibt, verwaltet Abschnitte, Marken und Symbole, löst Sprünge auf,
//! entscheidet zwischen Kurz- und Langform (Relaxation) und sammelt die
//! Umsetzungen (Relokationen), die der Binder braucht.
//!
//! ## Warum über den Text und nicht direkt aus FIR?
//!
//! Weil es dadurch eine **Gegenprobe mit Millionen echten Befehlen** gibt.
//! Der alte Weg schickt denselben Text durch `as`; wer beide Wege auf
//! dieselbe Eingabe loslässt, kann Oktett für Oktett vergleichen. Ein
//! Kodierer, der direkt aus FIR erzeugt, hätte diesen Prüfstand nicht — man
//! müsste ihm glauben. Der Umweg über den Text kostet wenig (der Zerteiler
//! ist klein, weil der Dialekt klein ist) und kauft dafür Gewissheit.
//!
//! Ist die Gewissheit da, kann `codegen_x86.rs` später ohne Umbau direkt
//! [`x86enc::Inst`] statt Text erzeugen — der Kodierer darunter bleibt
//! derselbe.
//!
//! ## Was hier `as` nachgebildet wird — und was nicht
//!
//! Nachgebildet, weil sonst die Oktette abweichen:
//!
//! * **Relaxation.** `jmp`/`jcc` auf eine Marke im selben Abschnitt werden
//!   kurz (rel8) kodiert, wenn die Weite reicht, sonst lang (rel32). `as`
//!   fängt kurz an und lässt nur wachsen; das ist ein Fixpunkt, und der wird
//!   hier genauso berechnet.
//! * **Die Asymmetrie zwischen `jmp` und `call`.** `jmp b` auf ein
//!   *globales* `b` im selben Abschnitt löst `as` direkt auf (`eb 0d`).
//!   `call b` auf dasselbe `b` **nicht** — dort entsteht immer
//!   `R_X86_64_PLT32`. Grund: der Sprung geht durch die Relaxation, der
//!   Aufruf durch die Umsetzung, und ein globales Symbol darf beim Binden
//!   ersetzt werden. Wer das übersieht, erzeugt Code, der erst beim
//!   dynamischen Binden auffällt.
//! * **Lokal gegen global.** Ein *lokales* Symbol (`.L…`) im selben
//!   Abschnitt wird überall direkt aufgelöst — auch bei `call` und bei
//!   `lea …[rip + .L…]`. Ein globales nie (außer beim Sprung).
//! * **Umsetzung auf Abschnittssymbole.** Zeigt eine Umsetzung auf eine
//!   *lokale* Marke in einem anderen Abschnitt, schreibt `as` sie gegen das
//!   **Abschnittssymbol** mit dem Versatz der Marke als Zusatz — nicht gegen
//!   die Marke selbst.
//!
//! Nicht nachgebildet (bewusst, siehe `docs/RUNDE-KODIERER.md`):
//!
//! * **DWARF.** `.loc`/`.file` werden gelesen und verworfen. Der interne Weg
//!   erzeugt also eine Objektdatei ohne Fehlersuchinformation — das
//!   entspricht einem Bau ohne `-g` und ist der Grund, warum der alte Weg
//!   Vorgabe bleibt.

#![allow(dead_code)]

use crate::x86enc::{self as E, Buf, Inst, Mem, Opnd};
use std::collections::HashMap;

// ===========================================================================
// Ergebnis
// ===========================================================================

pub const SEC_TEXT: usize = 0;
pub const SEC_DATA: usize = 1;
pub const SEC_RODATA: usize = 2;
pub const SEC_NOTE: usize = 3;
/// `.bss` — Inhalt null, belegt aber Platz im Abbild (ELF: `NOBITS`).
pub const SEC_BSS: usize = 4;
pub const N_SEC: usize = 5;

/// Umsetzungsarten, die hier vorkommen (Werte nach der System-V-ABI, x86-64).
pub const R_X86_64_64: u32 = 1;
pub const R_X86_64_PC32: u32 = 2;
pub const R_X86_64_32: u32 = 10;
pub const R_X86_64_32S: u32 = 11;
pub const R_X86_64_PLT32: u32 = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reloc {
    pub offset: u64,
    /// Nummer in `Assembled::symbols`.
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
    /// `None` = nicht in dieser Datei definiert (extern).
    pub section: Option<usize>,
    pub value: u64,
    pub global: bool,
    /// Abschnittssymbol (`.text`, `.rodata`, …).
    pub is_section: bool,
}

pub struct Assembled {
    pub sections: Vec<Section>,
    pub symbols: Vec<Sym>,
}

// ===========================================================================
// Zwischendarstellung: Stücke
// ===========================================================================

/// Wohin ein Sprung/eine Adresse zeigt.
#[derive(Clone, Debug)]
struct Target {
    name: String,
}

#[derive(Clone, Debug)]
enum Piece {
    /// Fertig kodiert, Länge steht fest. `relocs` sind Umsetzungen relativ
    /// zum Anfang des Stücks.
    Fixed { bytes: Vec<u8>, relocs: Vec<PendReloc> },
    /// Ein Sprung, dessen Länge von der Weite abhängt.
    Branch { kind: BrKind, target: Target, long: bool },
    /// Ausrichtung: auffüllen bis zum Vielfachen von `n`.
    Align { n: u64 },
    /// Marke an dieser Stelle.
    Label { name: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BrKind {
    Jmp,
    Jcc(u8),
    /// `call` — nie kurz, aber gegenüber lokalen Marken auflösbar.
    Call,
}

/// Eine Umsetzung, deren Symbol erst am Ende bekannt ist.
#[derive(Clone, Debug)]
struct PendReloc {
    /// Versatz innerhalb des Stücks.
    at: usize,
    name: String,
    kind: u32,
    addend: i64,
}

// ===========================================================================
// Der Zerteiler
// ===========================================================================

pub struct Asm {
    pieces: [Vec<Piece>; N_SEC],
    cur: usize,
    globals: Vec<String>,
    aligns: [u64; N_SEC],
    /// Zähler für die zahligen Marken (`1:` … `jnz 1f`).
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
        format!("Zeile {}: {}", self.line_no, m.as_ref())
    }

    /// Nimmt den ganzen Assemblertext auf.
    pub fn feed(&mut self, text: &str) -> Result<(), String> {
        for (n, raw) in text.lines().enumerate() {
            self.line_no = n + 1;
            self.line(raw)?;
        }
        Ok(())
    }

    fn line(&mut self, raw: &str) -> Result<(), String> {
        // Kommentar abschneiden — aber nicht innerhalb einer Zeichenkette.
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

        // Marke? (`name:` am Anfang; danach darf noch ein Befehl folgen)
        let mut s = s;
        loop {
            let bytes = s.as_bytes();
            let mut i = 0;
            // eine zahlige Marke `1:`
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
            // eine gewöhnliche Marke
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
        // Direktiven, die trotzdem einen Doppelpunkt enthalten können
        // (`.section .note.GNU-stack,"",@progbits`) fangen wir oben ab,
        // weil sie mit `.` beginnen und keine Marke sind.
        if s.starts_with('.') {
            return self.directive(s);
        }
        self.instruction(s)
    }

    // -----------------------------------------------------------------
    // Direktiven
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
                    other => return Err(self.err(format!("unbekannter Abschnitt {}", other))),
                };
                Ok(())
            }
            ".globl" | ".global" => {
                self.globals.push(rest.to_string());
                Ok(())
            }
            ".align" | ".balign" => {
                let n: u64 = rest.parse().map_err(|_| self.err("Ausrichtungszahl"))?;
                if n > self.aligns[self.cur] {
                    self.aligns[self.cur] = n;
                }
                self.push(Piece::Align { n });
                Ok(())
            }
            ".p2align" => {
                let e: u32 = rest.split(',').next().unwrap_or("0").trim().parse()
                    .map_err(|_| self.err("Ausrichtungszahl"))?;
                let n = 1u64 << e;
                if n > self.aligns[self.cur] {
                    self.aligns[self.cur] = n;
                }
                self.push(Piece::Align { n });
                Ok(())
            }
            ".zero" | ".space" | ".skip" => {
                let n: usize = rest.split(',').next().unwrap_or("0").trim().parse()
                    .map_err(|_| self.err("Größe"))?;
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
                            // ein Symbol — Umsetzung voller Breite
                            let (name, add) = split_addend(item);
                            let kind = match w {
                                8 => R_X86_64_64,
                                4 => R_X86_64_32,
                                _ => return Err(self.err("Symbol in zu schmalem Feld")),
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
            other => Err(self.err(format!("unbekannte Direktive {}", other))),
        }
    }

    // -----------------------------------------------------------------
    // Befehle
    // -----------------------------------------------------------------

    fn instruction(&mut self, s: &str) -> Result<(), String> {
        let (mut m, mut rest) = match s.find(char::is_whitespace) {
            Some(i) => (s[..i].to_ascii_lowercase(), s[i..].trim()),
            None => (s.to_ascii_lowercase(), ""),
        };
        // Ein Praefix (`lock`, `rep`) gehoert zum Befehl und nicht zu den
        // Operanden: `lock cmpxchg qword ptr [rcx], rdx` hat ZWEI Operanden,
        // nicht `cmpxchg qword ptr [rcx]` und `rdx`.
        if matches!(m.as_str(), "lock" | "rep" | "repe" | "repne" | "repz" | "repnz") {
            let (m2, rest2) = match rest.find(char::is_whitespace) {
                Some(i) => (rest[..i].to_ascii_lowercase(), rest[i..].trim()),
                None => (rest.to_ascii_lowercase(), ""),
            };
            if m2.is_empty() {
                return Err(self.err("Präfix ohne Befehl"));
            }
            m = format!("{} {}", m, m2);
            rest = rest2;
        }
        let ops: Vec<String> = split_operands(rest);

        // --- Sprünge und Aufrufe zuerst: sie werden zu Branch-Stücken ---
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

        // --- alles andere wird sofort kodiert -------------------------
        let (inst, relocs) = self.build(&m, &ops)?;
        let mut buf = Buf::new();
        E::encode(&mut buf, &inst).map_err(|e| self.err(e))?;
        // Die Ausbesserungen des Kodierers auf Umsetzungen abbilden.
        let mut out = Vec::new();
        for f in &buf.fixups {
            let (name, add, kind) = relocs
                .get(f.id as usize)
                .ok_or_else(|| self.err("Ausbesserung ohne Symbol"))?
                .clone();
            let addend = if f.pcrel {
                // Bezugspunkt ist das Ende des Befehls: der Zusatz muss die
                // Oktette zwischen Feldanfang und Befehlsende abziehen.
                add - ((f.inst_end - f.at) as i64)
            } else {
                add
            };
            out.push(PendReloc { at: f.at, name, kind, addend });
        }
        self.push(Piece::Fixed { bytes: buf.code, relocs: out });
        Ok(())
    }

    /// Baut einen Befehl aus dem `0F`-Raum. Hier faellt die Entscheidung,
    /// welcher Operand ins `reg`-Feld und welcher ins `r/m`-Feld kommt.
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
                _ => return Err(self.err(format!("{} braucht einen Sofortwert", m))),
            },
            None => None,
        };
        let (pfx, opc, digit, w, reg, rm) = match form {
            SseForm::Rm | SseForm::RmI => {
                let d = parse_operand(a, 128, syms)?;
                let dx = match d {
                    Opnd::Xmm(x) => x,
                    Opnd::Reg(r, _) => r, // pinsrd nimmt ein xmm links, aber
                    _ => return Err(self.err(format!("{} braucht xmm links", m))),
                };
                let sw = hint_width(b).unwrap_or(128);
                let src = parse_operand(b, if sw == 128 { 128 } else { sw }, syms)?;
                // pinsrq/pextrq brauchen REX.W
                let w = m.ends_with('q') && (m.starts_with("pinsr") || m.starts_with("pextr"));
                (pfx, op, None::<u8>, w, dx, src)
            }
            SseForm::MrI => {
                // `pextrd r/m32, xmm, imm8` -- das xmm ist die QUELLE.
                let sw = hint_width(a).unwrap_or(32);
                let dst = parse_operand(a, sw, syms)?;
                let sx = match parse_operand(b, 128, syms)? {
                    Opnd::Xmm(x) => x,
                    _ => return Err(self.err(format!("{} braucht xmm rechts", m))),
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
                        _ => return Err(self.err(format!("{} braucht xmm als Quelle", m))),
                    };
                    (pfx, store_op, None, false, sx, dst)
                } else {
                    let dx = match parse_operand(a, 128, syms)? {
                        Opnd::Xmm(x) => x,
                        _ => return Err(self.err(format!("{} braucht xmm als Ziel", m))),
                    };
                    let src = parse_operand(b, 128, syms)?;
                    (pfx, op, None, false, dx, src)
                }
            }
            SseForm::Digit(d) => {
                let dx = match parse_operand(a, 128, syms)? {
                    Opnd::Xmm(x) => x,
                    _ => return Err(self.err(format!("{} braucht xmm", m))),
                };
                let v = match parse_operand(b, 8, syms)? {
                    Opnd::Imm(v) => v as u8,
                    // `psrlq xmm, xmm` gibt es auch -- dann die Nicht-Ziffer-Form
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
        // `1f` = nächste Marke `1:` vorwärts, `1b` = die letzte rückwärts.
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

    /// Baut aus Mnemonik und Operandentexten einen [`Inst`].
    ///
    /// Zweiter Rückgabewert: die Symbole, auf die sich die Ausbesserungen
    /// im Befehl beziehen (Nummer = Reihenfolge).
    #[allow(clippy::type_complexity)]
    fn build(
        &self,
        m: &str,
        ops: &[String],
    ) -> Result<(Inst, Vec<(String, i64, u32)>), String> {
        let mut syms: Vec<(String, i64, u32)> = Vec::new();
        let mut p = |o: &str, want: u16| -> Result<Opnd, String> { parse_operand(o, want, &mut syms) };

        // Praefixbefehle ohne Operanden: `rep movsb`, `rep stosb`
        if let Some(r) = m.strip_prefix("rep ") {
            let i = match r {
                "movsb" => Inst::RepMovsb,
                "stosb" => Inst::RepStosb,
                other => return Err(self.err(format!("rep {}", other))),
            };
            return Ok((i, syms));
        }

        // Ohne Operanden
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
                other => return Err(self.err(format!("unbekannter Befehl '{}'", other))),
            };
            return Ok((i, syms));
        }

        // Ein Operand
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
                    let (r, b) = reg_of(&d).ok_or_else(|| self.err("bswap braucht ein Register"))?;
                    Inst::Bswap { reg: r, bits: b }
                }
                "inc" => Inst::IncDec { dec: false, dst: p(o0, 64)? },
                "dec" => Inst::IncDec { dec: true, dst: p(o0, 64)? },
                other => return Err(self.err(format!("unbekannter Befehl '{}' mit 1 Operanden", other))),
            };
            return Ok((i, syms));
        }

        // Zwei Operanden
        if ops.len() == 2 {
            let (a, b) = (&ops[0], &ops[1]);
            // Präfixbefehle
            if let Some(rest) = m.strip_prefix("lock ") {
                let dst = p(a, 64)?;
                let src = p(b, 64)?;
                let (mem, bits) = match dst {
                    Opnd::Mem(mm) => (mm, mm.bits),
                    _ => return Err(self.err("lock braucht Speicher")),
                };
                let sr = match src {
                    Opnd::Reg(r, _) => r,
                    _ => return Err(self.err("lock braucht ein Register")),
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
                let (dr, bits) = reg_of(&d).ok_or_else(|| self.err("cmov braucht Register"))?;
                return Ok((Inst::Cmovcc { cc, dst: dr, bits, src: p(b, bits)? }, syms));
            }
            // Alles aus dem 0F-Raum
            if let Some((pfx, esc, op, form)) = sse_table(m) {
                return self.sse2(m, a, b, None, pfx, esc, op, form, &mut syms);
            }
            // crc32 r32/r64, r/m8|16|32|64 -- die Breite steckt im Opcode
            if m == "crc32" {
                let d = parse_operand(a, 64, &mut syms)?;
                let (dr, dbits) = reg_of(&d).ok_or_else(|| self.err("crc32 braucht ein Register"))?;
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
                    .ok_or_else(|| self.err(format!("{} braucht ein Register", m)))?;
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
                    // Die Breite kommt vom Operanden, der sie kennt.
                    let (wa, wb) = (hint_width(a), hint_width(b));
                    let w = wa.or(wb).unwrap_or(64);
                    let dst = p(a, w)?;
                    let w2 = width_hint_of(&dst).or(wb).unwrap_or(w);
                    let src = p(b, w2)?;
                    // movq/movd zwischen xmm und Allzweckregister
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
                    let (dr, bits) = reg_of(&d).ok_or_else(|| self.err("lea braucht Register"))?;
                    match p(b, 64)? {
                        Opnd::Mem(mm) => Inst::Lea { dst: dr, dst_bits: bits, src: mm },
                        _ => return Err(self.err("lea braucht eine Adresse")),
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
                    let (dr, bits) = reg_of(&d).ok_or_else(|| self.err("imul braucht Register"))?;
                    Inst::Imul2 { dst: dr, bits, src: p(b, bits)? }
                }
                "movzx" | "movsx" => {
                    let d = p(a, 64)?;
                    let (dr, dbits) = reg_of(&d).ok_or_else(|| self.err("movzx braucht Register"))?;
                    let sw = hint_width(b).ok_or_else(|| self.err("Quellbreite unbekannt"))?;
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
                    let (dr, _) = reg_of(&d).ok_or_else(|| self.err("movsxd braucht Register"))?;
                    Inst::Movsxd { dst: dr, src: p(b, 32)? }
                }
                "cvtsi2sd" | "cvtsi2ss" => {
                    let d = p(a, 128)?;
                    let dx = match d {
                        Opnd::Xmm(x) => x,
                        _ => return Err(self.err("cvtsi2 braucht xmm")),
                    };
                    let sw = hint_width(b).unwrap_or(64);
                    Inst::CvtSi2F { double: m == "cvtsi2sd", dst: dx, src: p(b, sw)?, src_bits: sw }
                }
                "cvttsd2si" | "cvttss2si" => {
                    let d = p(a, 64)?;
                    let (dr, bits) = reg_of(&d).ok_or_else(|| self.err("cvtt braucht Register"))?;
                    let sx = match p(b, 128)? {
                        Opnd::Xmm(x) => x,
                        _ => return Err(self.err("cvtt braucht xmm")),
                    };
                    Inst::CvtF2Si { double: m == "cvttsd2si", dst: dr, dst_bits: bits, src: sx }
                }
                other => return Err(self.err(format!("unbekannter Befehl '{}' mit 2 Operanden", other))),
            };
            return Ok((i, syms));
        }

        // Drei Operanden: SSE mit Sofortwert
        if ops.len() == 3 {
            if let Some((pfx, esc, op, form)) = sse_table(m) {
                // `sha256rnds2 xmm1, xmm2, xmm0` -- der dritte Operand ist
                // die vorgeschriebene xmm0 und steht NICHT in der Kodierung.
                // `as` nimmt beide Schreibweisen; die Oktette sind dieselben.
                if m == "sha256rnds2" {
                    if ops[2].trim() != "xmm0" {
                        return Err(self.err("sha256rnds2 verlangt xmm0 als dritten Operanden"));
                    }
                    return self.sse2(m, &ops[0], &ops[1], None, pfx, esc, op, form, &mut syms);
                }
                return self.sse2(m, &ops[0], &ops[1], Some(&ops[2]), pfx, esc, op, form, &mut syms);
            }
        }

        // Drei Operanden: nur `imul r, r/m, imm`
        if ops.len() == 3 && m == "imul" {
            let d = p(&ops[0], 64)?;
            let (dr, bits) = reg_of(&d).ok_or_else(|| self.err("imul braucht Register"))?;
            let src = p(&ops[1], bits)?;
            let imm = match p(&ops[2], bits)? {
                Opnd::Imm(v) => v,
                _ => return Err(self.err("imul braucht einen Sofortwert")),
            };
            return Ok((Inst::Imul3 { dst: dr, bits, src, imm }, syms));
        }

        Err(self.err(format!("'{}' mit {} Operanden", m, ops.len())))
    }

    // -----------------------------------------------------------------
    // Auflösen: Marken, Relaxation, Umsetzungen
    // -----------------------------------------------------------------

    /// Berechnet Längen und Versätze, bis sich nichts mehr ändert, und
    /// baut daraus die fertigen Abschnitte.
    pub fn finish(mut self) -> Result<Assembled, String> {
        // Welche Namen sind global?
        let mut is_global: HashMap<&str, ()> = HashMap::new();
        for g in &self.globals {
            is_global.insert(g.as_str(), ());
        }

        // --- Fixpunkt der Relaxation ---------------------------------
        // `as` fängt kurz an und lässt Sprünge nur wachsen. Damit ist der
        // Fixpunkt eindeutig und wird in wenigen Durchgängen erreicht.
        let mut offsets: [Vec<u64>; N_SEC] = Default::default();
        let mut labels: HashMap<String, (usize, u64)> = HashMap::new();
        for _round in 0..64 {
            // 1. Versätze aus den aktuellen Längen
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
            // 2. Sprünge prüfen
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
                        // extern/global → immer lang
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

        // --- Symboltabelle -------------------------------------------
        // `as` schreibt zuerst die Abschnittssymbole, dann die lokalen,
        // dann die globalen. Die Abschnittssymbole brauchen wir sowieso:
        // Umsetzungen auf lokale Marken laufen über sie.
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

        // --- Oktette schreiben ---------------------------------------
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
                            // Sprung/Aufruf auf ein Symbol: PLT32, Zusatz -4.
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

/// Findet (oder legt an) das Symbol, gegen das eine Umsetzung läuft.
///
/// Hier steckt die Regel, die `as` anwendet: eine **lokale** Marke bekommt
/// keine eigene Zeile in der Symboltabelle, sondern die Umsetzung läuft
/// gegen das **Abschnittssymbol** und der Versatz der Marke wandert in den
/// Zusatz. Ein **globales** Symbol steht für sich selbst.
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
    // unbekannt → externes Symbol
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

/// Auffüllen im Codeabschnitt: `as` schreibt Mehr-Oktett-`nop`s, keine 0x90.
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
// Operanden zerteilen
// ===========================================================================

/// Ist dieser Operandentext eine Marke/ein Symbol (und kein Register,
/// keine Adresse, keine Zahl)? Nur dann wird aus `jmp x` ein Sprungstueck.
fn is_symbolish(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    if b[0].is_ascii_digit() {
        // die zahligen Marken `1f` / `1b`
        return b.len() >= 2
            && matches!(b[b.len() - 1], b'f' | b'b')
            && b[..b.len() - 1].iter().all(|c| c.is_ascii_digit());
    }
    if !(b[0].is_ascii_alphabetic() || matches!(b[0], b'_' | b'.')) {
        return false;
    }
    // ein Registername ist kein Symbol
    if E::gpr_by_name(s).is_some() || E::xmm_by_name(s).is_some() {
        return false;
    }
    // ein Groessenwort leitet eine Adresse ein
    let low = s.to_ascii_lowercase();
    for w in ["byte ptr", "word ptr", "dword ptr", "qword ptr", "xmmword ptr"] {
        if low.starts_with(w) {
            return false;
        }
    }
    !s.contains('[')
}

/// Zerlegt die Operandenliste an Kommas der obersten Ebene.
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

/// Wie `split_commas`, aber Kommas in Anführungszeichen trennen nicht.
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
                    // Oktal, bis zu drei Ziffern
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

/// Welche Breite verrät der Operandentext von sich aus?
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

/// Zerteilt einen einzelnen Operanden.
///
/// `want` ist die Breite, die der Befehl erwartet, falls der Text selbst
/// keine nennt (Sofortwerte, Marken).
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
    // Speicher: [..] mit oder ohne Größenwort, mit oder ohne Segment
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
    // Segmentpräfix `fs:0` (Firn benutzt es fuer den Faden-eigenen Bereich)
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
        let d = parse_int(a).ok_or_else(|| format!("Segmentadresse: {}", rest))?;
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
    // Ein Symbol als Sofortwert.
    let (name, add) = split_addend(s);
    let id = syms.len() as u32;
    syms.push((name, add, R_X86_64_32S));
    Ok(Opnd::SymImm { addend: add, fix: Some(id) })
}

/// Der Inhalt von `[...]`.
fn parse_mem(
    inner: &str,
    bits: u16,
    syms: &mut Vec<(String, i64, u32)>,
) -> Result<Opnd, String> {
    let t = inner.trim();
    // RIP-relativ: `rip + name` oder `rip - 4`
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

    // In Stücke zerlegen: an + und - trennen, das Vorzeichen behalten.
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
                .ok_or_else(|| format!("kein Register: {}", r))?;
            index = Some(rn);
            scale = sc.trim().parse::<u8>().map_err(|_| format!("Skalierung: {}", sc))?;
            continue;
        }
        if let Some((rn, _)) = E::gpr_by_name(term.trim()) {
            if base.is_none() && sg > 0 {
                base = Some(rn);
            } else if index.is_none() {
                index = Some(rn);
                scale = 1;
            } else {
                return Err(format!("zu viele Register in [{}]", inner));
            }
            continue;
        }
        if let Some(v) = parse_int(&term) {
            disp += sg * v;
            continue;
        }
        return Err(format!("unverstandener Teil '{}' in [{}]", term, inner));
    }

    Ok(Opnd::Mem(Mem { base, index, scale, disp, rip: false, bits, disp_fix: None, seg: 0 }))
}

/// Wie ein Befehl aus dem `0F`-Raum seine Operanden verteilt.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SseForm {
    /// `op xmm_ziel, xmm/m_quelle` — reg = Ziel.
    Rm,
    /// `op xmm_ziel, xmm/m_quelle, imm8`.
    RmI,
    /// `op r/m_ziel, xmm_quelle, imm8` — reg = QUELLE (pextrd/pextrq).
    MrI,
    /// Lade-/Speicherform: `mov… xmm, xmm/m` **oder** `mov… m, xmm`.
    /// Der zweite Opcode ist der Speicherfall.
    Mov(u8),
    /// `op xmm, imm8` mit fester Ziffer im reg-Feld (psrlq, pslldq …).
    Digit(u8),
}

/// Die Befehle aus dem `0F`-Raum, die Firns Codeerzeuger ausgibt.
///
/// Erhoben aus `compiler/src/simd.rs` und dem Auszaehlen des Baums; jede
/// Zeile ist gegen `as` geprueft (`tools/kodierer/run.sh`).
/// Rueckgabe: (Pflichtpraefix, zweites Opcode-Oktett, Opcode, Form).
fn sse_table(m: &str) -> Option<(u8, u8, u8, SseForm)> {
    use SseForm::*;
    Some(match m {
        // --- Gleitkomma, skalar ------------------------------------
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
        // --- Bewegen (Lade- und Speicherform) ----------------------
        "movsd" => (0xF2, 0, 0x10, Mov(0x11)),
        "movss" => (0xF3, 0, 0x10, Mov(0x11)),
        "movaps" => (0x00, 0, 0x28, Mov(0x29)),
        "movapd" => (0x66, 0, 0x28, Mov(0x29)),
        "movups" => (0x00, 0, 0x10, Mov(0x11)),
        "movupd" => (0x66, 0, 0x10, Mov(0x11)),
        "movdqa" => (0x66, 0, 0x6F, Mov(0x7F)),
        "movdqu" => (0xF3, 0, 0x6F, Mov(0x7F)),
        // --- bitweise, ganzzahlig ----------------------------------
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
        // --- mit Sofortwert ----------------------------------------
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
        // --- Schiebebefehle mit fester Ziffer ----------------------
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
        // --- AES und SHA -------------------------------------------
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

/// Der bequeme Gesamteinstieg: Text rein, Objektinhalt raus.
pub fn assemble(text: &str) -> Result<Assembled, String> {
    let mut a = Asm::new();
    a.feed(text)?;
    a.finish()
}

// ===========================================================================
// Übergang zum ELF-Schreiber
// ===========================================================================

/// Die Abschnittsnummern, die `elfobj` benutzt: `.text .data .bss .rodata
/// .note.GNU-stack`. `.bss` steht dazwischen, weil `as` es auch so anlegt.
fn elf_index(sec: usize) -> usize {
    match sec {
        SEC_TEXT => 0,
        SEC_DATA => 1,
        SEC_BSS => 2,
        SEC_RODATA => 3,
        _ => 4,
    }
}

/// Der ganze Weg: Assemblertext → Oktette einer ELF-Objektdatei.
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
