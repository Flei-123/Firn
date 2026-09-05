//! **RUNDE KODIERER II — die DWARF-Zeilentabelle, selbst geschrieben.**
//!
//! Der Binärkodierer der Runde KODIERER konnte alles außer einem: die
//! Fehlersuchinformation. `.loc`/`.file` wurden gelesen und verworfen, und
//! genau deshalb blieb `as` die Vorgabe — ein Bau ohne Zeilentabelle wäre
//! ein Rückschritt gewesen. Dieses Stück schließt die Lücke: aus den
//! `.file`- und `.loc`-Angaben entsteht hier der **Zeilennummern-Automat**
//! nach DWARF, Oktett für Oktett.
//!
//! ## Warum DWARF 3 und nicht DWARF 5
//!
//! Der Maßstab ist `as`. `as` schreibt bei `.loc` ohne weitere Schalter
//! **DWARF 3** (`DWARF2_LINE_VERSION = dwarf_level > 3 ? dwarf_level : 3`,
//! `gas/dwarf2dbg.c`), und die Abnahme dieser Runde vergleicht Oktett für
//! Oktett gegen `as`. Eine andere Fassung wäre nicht falsch, aber sie
//! nähme uns den Prüfstand. DWARF 3 ist außerdem das, was `gdb`,
//! `addr2line` und `readelf` seit zwanzig Jahren lesen.
//!
//! ## Der Automat
//!
//! Eine Zeilentabelle ist keine Tabelle, sondern ein Programm, das eine
//! Tabelle *erzeugt*. Der Automat hat die Zustände Adresse, Datei, Zeile,
//! Spalte, `is_stmt`; die Befehle schieben sie weiter und `DW_LNS_copy`
//! (bzw. ein Sonderopcode) legt eine Zeile ab. Die Kunst ist die
//! Verdichtung: ein einziges Oktett kann Adresse **und** Zeile
//! weiterschieben und die Zeile ablegen. Das ist der Sonderopcode:
//!
//! ```text
//!   opcode = (Zeilenschritt - Zeilenbasis) + Zeilenspanne * Adressschritt
//!            + Opcodebasis
//! ```
//!
//! mit Zeilenbasis −5, Zeilenspanne 14, Opcodebasis 13. Passt das nicht in
//! ein Oktett, kommt `DW_LNS_const_add_pc` (schiebt um den größten
//! Adressschritt, den ein Sonderopcode kann: 17) davor, und erst wenn das
//! auch nicht reicht, `DW_LNS_advance_pc` mit einer LEB128-Zahl.
//!
//! Diese Reihenfolge ist **nicht** frei: sie ist genau die von
//! `emit_inc_line_addr()` in `gas/dwarf2dbg.c`, denn nur so ist die
//! Gegenprobe oktettweise möglich. Wo eine Entscheidung willkürlich
//! aussieht (warum erst `const_add_pc` versuchen und dann erst
//! `advance_pc`?), ist sie aus `as` übernommen und der Kommentar sagt es.
//!
//! ## Was hier sonst noch entsteht
//!
//! `as` schreibt bei `.loc` nicht nur `.debug_line`, sondern — solange das
//! Programm keine eigene `.debug_info` mitbringt — auch eine winzige
//! Übersetzungseinheit, die auf die Zeilentabelle zeigt
//! (`.debug_info`, `.debug_abbrev`, `.debug_aranges`, `.debug_str`).
//! Ohne die findet weder `gdb` noch `addr2line` die Tabelle: beide gehen
//! über `.debug_info` und `DW_AT_stmt_list`. Also schreiben wir sie auch.
//!
//! Genau **eine** Abweichung ist Absicht: `DW_AT_producer` sagt `firnc`
//! und nicht `GNU AS 2.40`. Der Übersetzer soll nicht behaupten, ein
//! anderes Programm zu sein. Die Zeichenkette steht am Ende von
//! `.debug_str`, also verschiebt sie nichts; `.debug_line`, `.debug_info`,
//! `.debug_abbrev` und `.debug_aranges` bleiben oktettgleich zu `as`.

#![allow(dead_code)]

// ---------------------------------------------------------------------------
// Die Zahlen des Automaten (DWARF 3, Abschnitt 6.2; Werte wie in `as`)
// ---------------------------------------------------------------------------

const LINE_VERSION: u16 = 3;
const INFO_VERSION: u16 = 3;
const ARANGES_VERSION: u16 = 2;

const LINE_BASE: i64 = -5;
const LINE_RANGE: i64 = 14;
const OPCODE_BASE: i64 = 13;
/// Der größte Adressschritt, den ein Sonderopcode noch schafft:
/// `(255 - Opcodebasis) / Zeilenspanne`.
const MAX_SPECIAL_ADDR_DELTA: u64 = ((255 - OPCODE_BASE) / LINE_RANGE) as u64;

// Die Standardbefehle.
const DW_LNS_COPY: u8 = 1;
const DW_LNS_ADVANCE_PC: u8 = 2;
const DW_LNS_ADVANCE_LINE: u8 = 3;
const DW_LNS_SET_FILE: u8 = 4;
const DW_LNS_SET_COLUMN: u8 = 5;
const DW_LNS_NEGATE_STMT: u8 = 6;
const DW_LNS_SET_BASIC_BLOCK: u8 = 7;
const DW_LNS_CONST_ADD_PC: u8 = 8;
const DW_LNS_FIXED_ADVANCE_PC: u8 = 9;
const DW_LNS_EXTENDED_OP: u8 = 0;

// Die erweiterten Befehle.
const DW_LNE_END_SEQUENCE: u8 = 1;
const DW_LNE_SET_ADDRESS: u8 = 2;

const ADDR_SIZE: u8 = 8;

// `.debug_abbrev`/`.debug_info`
const DW_TAG_COMPILE_UNIT: u64 = 0x11;
const DW_CHILDREN_NO: u8 = 0;
const DW_AT_NAME: u64 = 0x03;
const DW_AT_STMT_LIST: u64 = 0x10;
const DW_AT_LOW_PC: u64 = 0x11;
const DW_AT_HIGH_PC: u64 = 0x12;
const DW_AT_LANGUAGE: u64 = 0x13;
const DW_AT_COMP_DIR: u64 = 0x1b;
const DW_AT_PRODUCER: u64 = 0x25;
const DW_FORM_ADDR: u64 = 0x01;
const DW_FORM_DATA2: u64 = 0x05;
const DW_FORM_DATA4: u64 = 0x06;
const DW_FORM_STRP: u64 = 0x0e;
/// `as` schreibt „MIPS-Assembler" hin, weil DWARF keine Sprachnummer für
/// Assemblertext hat. Wir übernehmen den Wert, damit die Einheit für die
/// Werkzeuge genauso aussieht.
const DW_LANG_MIPS_ASSEMBLER: u16 = 0x8001;
const GAS_ABBREV_COMP_UNIT: u64 = 1;

// ---------------------------------------------------------------------------
// Eingabe und Ausgabe
// ---------------------------------------------------------------------------

/// Eine Zeile der Tabelle: an dieser Adresse im Abschnitt beginnt der Code
/// zu dieser Quellstelle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub addr: u64,
    /// 1-basiert, wie in `.loc`.
    pub file: u32,
    pub line: u32,
    pub col: u32,
}

/// Wogegen eine Umsetzung läuft. Der Aufrufer kennt die Abschnittsnummern,
/// dieses Stück nicht — es bleibt frei von ELF-Wissen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelTo {
    Text,
    Line,
    Info,
    Abbrev,
    Str,
}

#[derive(Clone, Copy, Debug)]
pub struct Rel {
    /// Versatz im eigenen Abschnitt.
    pub at: u64,
    pub to: RelTo,
    pub addend: i64,
    /// 4 oder 8 Oktette.
    pub width: u8,
}

#[derive(Default, Debug)]
pub struct Sections {
    pub line: Vec<u8>,
    pub line_rel: Vec<Rel>,
    pub info: Vec<u8>,
    pub info_rel: Vec<Rel>,
    pub abbrev: Vec<u8>,
    pub aranges: Vec<u8>,
    pub aranges_rel: Vec<Rel>,
    pub dstr: Vec<u8>,
}

/// Alles, was der Kodierer über den Bau wissen muss.
pub struct Params<'a> {
    /// Die Quelldateien in der Reihenfolge ihrer `.file`-Nummern;
    /// `files[0]` ist `.file 1`.
    pub files: &'a [String],
    /// Größe des `.text`-Abschnitts — die Endmarke der Folge.
    pub text_size: u64,
    /// 1 auf x86-64, 4 auf ARM64. Adressschritte werden damit geteilt.
    pub min_insn: u64,
    /// Das Verzeichnis, das `as` als `--debug-prefix-map <cwd>=.` bekommt.
    /// Leer = keine Abbildung.
    pub cwd: &'a str,
    /// Arbeitsverzeichnis für `DW_AT_comp_dir` (vor der Abbildung).
    pub pwd: &'a str,
    pub producer: &'a str,
    /// Schreibt das Programm eine eigene `.debug_info`? Dann legt `as` die
    /// kleine Einheit NICHT an — und wir auch nicht.
    pub own_debug_info: bool,
}

// ---------------------------------------------------------------------------
// Schreibhilfen
// ---------------------------------------------------------------------------

#[derive(Default)]
struct W {
    b: Vec<u8>,
    rel: Vec<Rel>,
}

impl W {
    fn u8(&mut self, v: u8) {
        self.b.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn bytes(&mut self, v: &[u8]) {
        self.b.extend_from_slice(v);
    }
    fn cstr(&mut self, s: &str) {
        self.b.extend_from_slice(s.as_bytes());
        self.b.push(0);
    }
    fn uleb(&mut self, mut v: u64) {
        loop {
            let mut byte = (v & 0x7f) as u8;
            v >>= 7;
            if v != 0 {
                byte |= 0x80;
            }
            self.b.push(byte);
            if v == 0 {
                break;
            }
        }
    }
    fn sleb(&mut self, mut v: i64) {
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            let sign = byte & 0x40 != 0;
            if (v == 0 && !sign) || (v == -1 && sign) {
                self.b.push(byte);
                break;
            }
            self.b.push(byte | 0x80);
        }
    }
    /// Ein Feld, das erst der Binder füllt.
    fn reloc(&mut self, to: RelTo, addend: i64, width: u8) {
        self.rel.push(Rel {
            at: self.b.len() as u64,
            to,
            addend,
            width,
        });
        self.b.extend(std::iter::repeat(0u8).take(width as usize));
    }
    fn len(&self) -> u64 {
        self.b.len() as u64
    }
}

// ---------------------------------------------------------------------------
// Datei- und Verzeichnistabelle
// ---------------------------------------------------------------------------

/// `lbasename` mit der Sonderregel aus `gas/dwarf2dbg.c::get_basename`:
/// steht der letzte Schrägstrich ganz vorn (`/a.fi`), gilt der ganze Pfad
/// als Name und es gibt kein Verzeichnis. Sonst wäre das Verzeichnis die
/// leere Zeichenkette, und die ist keine.
fn basename_at(path: &str) -> usize {
    match path.rfind('/') {
        Some(i) if i + 1 == 1 => 0,
        Some(i) => i + 1,
        None => 0,
    }
}

/// Bildet `path` ab, wenn er mit `cwd` anfängt — die Nachbildung von
/// `--debug-prefix-map <cwd>=.`. `as` vergleicht nur den Anfang, ohne auf
/// eine Trennstelle zu achten (`gas/remap.c`), und genauso machen wir es.
fn remap(path: &str, cwd: &str) -> String {
    if !cwd.is_empty() && path.starts_with(cwd) {
        format!(".{}", &path[cwd.len()..])
    } else {
        path.to_string()
    }
}

/// Die Verzeichnistabelle, aufgebaut wie `get_directory_table_entry`:
/// Steckplatz 0 bleibt frei (den benutzt erst DWARF 5), die erste echte
/// Angabe landet auf 1.
fn dir_entry(dirs: &mut Vec<Option<String>>, path: &str, mut dirlen: usize) -> usize {
    if dirlen == 0 {
        return 0;
    }
    if path.as_bytes()[dirlen - 1] == b'/' {
        dirlen -= 1;
        if dirlen == 0 {
            return 0;
        }
    }
    let want = &path[..dirlen];
    for (d, e) in dirs.iter().enumerate() {
        if e.as_deref() == Some(want) {
            return d;
        }
    }
    let mut d = dirs.len();
    if d == 0 {
        d = 1;
    }
    if dirs.len() <= d {
        dirs.resize(d + 1, None);
    }
    dirs[d] = Some(want.to_string());
    d
}

// ---------------------------------------------------------------------------
// Der Zeilennummern-Automat
// ---------------------------------------------------------------------------

/// Ein Adress-/Zeilenschritt, verdichtet — die Übersetzung von
/// `emit_inc_line_addr()` aus `gas/dwarf2dbg.c`.
///
/// `line_delta == None` bedeutet das Ende der Folge (`INT_MAX` bei `as`):
/// dort darf kein Sonderopcode stehen, weil `DW_LNE_end_sequence` selbst
/// die Zeile ablegt.
fn inc_line_addr(w: &mut W, line_delta: Option<i64>, mut addr_delta: u64, min_insn: u64) {
    // Adressschritte zählen in Befehlen, nicht in Oktetten — auf ARM64 ist
    // jeder Befehl 4 Oktett lang, also passt viermal so viel in denselben
    // Sonderopcode.
    if min_insn > 1 {
        addr_delta /= min_insn;
    }

    let Some(mut line_delta) = line_delta else {
        if addr_delta == MAX_SPECIAL_ADDR_DELTA {
            w.u8(DW_LNS_CONST_ADD_PC);
        } else if addr_delta != 0 {
            w.u8(DW_LNS_ADVANCE_PC);
            w.uleb(addr_delta);
        }
        w.u8(DW_LNS_EXTENDED_OP);
        w.u8(1);
        w.u8(DW_LNE_END_SEQUENCE);
        return;
    };

    let mut tmp = line_delta - LINE_BASE;
    let mut need_copy = false;
    // Passt der Zeilenschritt nicht in die Spanne, muss er einzeln
    // geschrieben werden; danach ist der Schritt 0.
    if !(0..LINE_RANGE).contains(&tmp) {
        w.u8(DW_LNS_ADVANCE_LINE);
        w.sleb(line_delta);
        line_delta = 0;
        tmp = -LINE_BASE;
        need_copy = true;
    }

    // `as`: „hübscher, finde ich, als ein Sonderopcode mit Zeile +0 und
    // Adresse +0". Das Ergebnis ist dasselbe, die Oktette sind es nicht —
    // also übernehmen wir es.
    if line_delta == 0 && addr_delta == 0 {
        w.u8(DW_LNS_COPY);
        return;
    }

    tmp += OPCODE_BASE;

    // Der Überlaufschutz von `as`: bei sehr großen Adressschritten würde
    // `tmp + addr_delta * LINE_RANGE` überlaufen, statt „zu groß" zu sein.
    if addr_delta < 256 + MAX_SPECIAL_ADDR_DELTA {
        let opcode = tmp + addr_delta as i64 * LINE_RANGE;
        if opcode <= 255 {
            w.u8(opcode as u8);
            return;
        }
        let opcode = tmp + (addr_delta - MAX_SPECIAL_ADDR_DELTA) as i64 * LINE_RANGE;
        if opcode <= 255 {
            w.u8(DW_LNS_CONST_ADD_PC);
            w.u8(opcode as u8);
            return;
        }
    }

    w.u8(DW_LNS_ADVANCE_PC);
    w.uleb(addr_delta);
    if need_copy {
        w.u8(DW_LNS_COPY);
    } else {
        w.u8(tmp as u8);
    }
}

/// Setzt die Adresse absolut — das einzige Mal, dass der Binder ins
/// Zeilenprogramm eingreifen muss.
fn set_addr(w: &mut W, addr: u64) {
    w.u8(DW_LNS_EXTENDED_OP);
    w.uleb(ADDR_SIZE as u64 + 1);
    w.u8(DW_LNE_SET_ADDRESS);
    w.reloc(RelTo::Text, addr as i64, 8);
}

// ---------------------------------------------------------------------------
// Der Gesamtbau
// ---------------------------------------------------------------------------

/// Baut alle Fehlersuchabschnitte, die aus `.file`/`.loc` folgen.
///
/// `rows` müssen nach Adresse aufsteigend sortiert sein — sie kommen in
/// Textreihenfolge aus dem Zerteiler, und Code wächst nur nach vorn.
pub fn build(p: &Params, rows: &[Row]) -> Sections {
    let mut out = Sections::default();
    if p.files.is_empty() || rows.is_empty() {
        return out;
    }

    // --- Datei- und Verzeichnistabelle ------------------------------
    let mut dirs: Vec<Option<String>> = Vec::new();
    // (Grundname, Verzeichnisnummer) je `.file`-Nummer, 1-basiert.
    let mut ftab: Vec<(String, usize)> = Vec::new();
    for path in p.files {
        let cut = basename_at(path);
        let d = dir_entry(&mut dirs, path, cut);
        ftab.push((path[cut..].to_string(), d));
    }

    // --- der Kopf ----------------------------------------------------
    let mut w = W::default();
    w.u32(0); // Gesamtlänge, kommt am Ende
    w.u16(LINE_VERSION);
    w.u32(0); // Kopflänge, kommt gleich
    let after_hdrlen = w.len();

    w.u8(p.min_insn as u8);
    // (DWARF 4 hätte hier `maximum_operations_per_instruction`.)
    w.u8(1); // default_is_stmt
    w.u8(LINE_BASE as i8 as u8);
    w.u8(LINE_RANGE as u8);
    w.u8(OPCODE_BASE as u8);
    // Wie viele Zahlen jeder Standardbefehl mitbringt.
    for n in [0u8, 1, 1, 1, 1, 0, 0, 0, 1, 0, 0, 1] {
        w.u8(n);
    }

    // Verzeichnisse (Steckplatz 0 wird nicht geschrieben), dann die
    // Abschlussnull.
    for d in dirs.iter().skip(1) {
        w.cstr(&remap(d.as_deref().unwrap_or(""), p.cwd));
    }
    w.u8(0);
    // Dateien: Name, Verzeichnisnummer, Zeitstempel, Größe. Zeit und Größe
    // schreibt `as` als 0, weil es die Datei nicht anfasst.
    for (name, d) in &ftab {
        w.cstr(name);
        w.uleb(*d as u64);
        w.uleb(0);
        w.uleb(0);
    }
    w.u8(0);

    let hdrlen = w.len() - after_hdrlen;
    w.b[6..10].copy_from_slice(&(hdrlen as u32).to_le_bytes());

    // --- das Programm ------------------------------------------------
    let mut filenum: u32 = 1;
    let mut line: i64 = 1;
    let mut col: u32 = 0;
    let mut last_addr: u64 = 0;
    let mut first = true;
    for r in rows {
        // Die Reihenfolge ist die von `process_entries`: erst die Datei,
        // dann die Spalte. Andersherum käme dieselbe Tabelle heraus, aber
        // andere Oktette.
        if filenum != r.file {
            filenum = r.file;
            w.u8(DW_LNS_SET_FILE);
            w.uleb(filenum as u64);
        }
        if col != r.col {
            col = r.col;
            w.u8(DW_LNS_SET_COLUMN);
            w.uleb(col as u64);
        }
        let line_delta = r.line as i64 - line;
        if first {
            set_addr(&mut w, r.addr);
            inc_line_addr(&mut w, Some(line_delta), 0, p.min_insn);
            first = false;
        } else {
            inc_line_addr(&mut w, Some(line_delta), r.addr - last_addr, p.min_insn);
        }
        line = r.line as i64;
        last_addr = r.addr;
    }
    // Das Ende der Folge liegt am Ende des Abschnitts, nicht am letzten
    // `.loc` — sonst gälte die letzte Zeile bis in alle Ewigkeit.
    inc_line_addr(
        &mut w,
        None,
        p.text_size.saturating_sub(last_addr),
        p.min_insn,
    );

    let total = w.len() - 4;
    w.b[0..4].copy_from_slice(&(total as u32).to_le_bytes());
    out.line = w.b;
    out.line_rel = w.rel;

    if p.own_debug_info {
        return out;
    }

    // --- die kleine Übersetzungseinheit ------------------------------
    // Ohne sie zeigt nichts auf die Zeilentabelle, und `gdb` wie
    // `addr2line` gehen leer aus.

    // .debug_str: Name, Arbeitsverzeichnis, Erzeuger — in dieser Folge,
    // die Versätze stehen unten in `.debug_info`.
    let mut s = W::default();
    let (name, d) = &ftab[0];
    let full = if *d != 0 {
        format!("{}/{}", remap(dirs[*d].as_deref().unwrap_or(""), p.cwd), name)
    } else {
        name.clone()
    };
    let off_name = s.len();
    s.cstr(&full);
    let off_comp_dir = s.len();
    s.cstr(&remap(p.pwd, p.cwd));
    let off_producer = s.len();
    s.cstr(p.producer);
    out.dstr = s.b;

    // .debug_abbrev: eine einzige Abkürzung, kinderlos.
    let mut a = W::default();
    a.uleb(GAS_ABBREV_COMP_UNIT);
    a.uleb(DW_TAG_COMPILE_UNIT);
    a.u8(DW_CHILDREN_NO);
    for (at, form) in [
        (DW_AT_STMT_LIST, DW_FORM_DATA4),
        (DW_AT_LOW_PC, DW_FORM_ADDR),
        (DW_AT_HIGH_PC, DW_FORM_ADDR),
        (DW_AT_NAME, DW_FORM_STRP),
        (DW_AT_COMP_DIR, DW_FORM_STRP),
        (DW_AT_PRODUCER, DW_FORM_STRP),
        (DW_AT_LANGUAGE, DW_FORM_DATA2),
    ] {
        a.uleb(at);
        a.uleb(form);
    }
    a.uleb(0);
    a.uleb(0);
    a.u8(0);
    out.abbrev = a.b;

    // .debug_info
    let mut i = W::default();
    i.u32(0); // Länge, kommt am Ende
    i.u16(INFO_VERSION);
    i.reloc(RelTo::Abbrev, 0, 4);
    i.u8(ADDR_SIZE);
    i.uleb(GAS_ABBREV_COMP_UNIT);
    i.reloc(RelTo::Line, 0, 4); // DW_AT_stmt_list
    i.reloc(RelTo::Text, 0, 8); // DW_AT_low_pc
    i.reloc(RelTo::Text, p.text_size as i64, 8); // DW_AT_high_pc
    i.reloc(RelTo::Str, off_name as i64, 4);
    i.reloc(RelTo::Str, off_comp_dir as i64, 4);
    i.reloc(RelTo::Str, off_producer as i64, 4);
    i.u16(DW_LANG_MIPS_ASSEMBLER);
    let total = i.len() - 4;
    i.b[0..4].copy_from_slice(&(total as u32).to_le_bytes());
    out.info = i.b;
    out.info_rel = i.rel;

    // .debug_aranges: die Adressbereiche der Einheit, für Werkzeuge, die
    // erst grob suchen und dann erst die Einheit aufschlagen.
    let mut r = W::default();
    r.u32(0);
    r.u16(ARANGES_VERSION);
    r.reloc(RelTo::Info, 0, 4);
    r.u8(ADDR_SIZE);
    r.u8(0); // Segmentkennung
    // Der Kopf wird auf das Doppelte einer Adresse ausgerichtet, damit die
    // Paare darunter gerade liegen.
    while r.len() % (2 * ADDR_SIZE as u64) != 0 {
        r.u8(0);
    }
    r.reloc(RelTo::Text, 0, 8);
    r.u64(p.text_size);
    r.u64(0);
    r.u64(0);
    let total = r.len() - 4;
    r.b[0..4].copy_from_slice(&(total as u32).to_le_bytes());
    out.aranges = r.b;
    out.aranges_rel = r.rel;

    out
}

// ---------------------------------------------------------------------------
// Prüfungen
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn params<'a>(files: &'a [String], cwd: &'a str) -> Params<'a> {
        Params {
            files,
            text_size: 0x199,
            min_insn: 1,
            cwd,
            pwd: cwd,
            producer: "firnc 0.1.0",
            own_debug_info: false,
        }
    }

    #[test]
    fn leb128_wie_im_buch() {
        let mut w = W::default();
        w.uleb(0);
        w.uleb(127);
        w.uleb(128);
        w.uleb(624485);
        assert_eq!(w.b, vec![0, 127, 0x80, 0x01, 0xE5, 0x8E, 0x26]);
        let mut w = W::default();
        w.sleb(-2);
        w.sleb(2);
        w.sleb(-129);
        assert_eq!(w.b, vec![0x7e, 0x02, 0xFF, 0x7E]);
    }

    /// Der Grundnamen-Sonderfall: `/a.fi` hat kein Verzeichnis.
    #[test]
    fn grundname_und_verzeichnis() {
        assert_eq!(basename_at("a.fi"), 0);
        assert_eq!(basename_at("/a.fi"), 0);
        assert_eq!(basename_at("./a.fi"), 2);
        assert_eq!(basename_at("/root/x/y.fi"), 8);
    }

    #[test]
    fn abbildung_des_arbeitsverzeichnisses() {
        assert_eq!(remap("/tmp/dl/a.fi", "/tmp/dl"), "./a.fi");
        assert_eq!(remap("/anders/a.fi", "/tmp/dl"), "/anders/a.fi");
        assert_eq!(remap("/tmp/dl", "/tmp/dl"), ".");
    }

    /// Die Zahlen aus dem Kopf, gegen die Hand nachgerechnet: 17 Oktette
    /// Zustandsangaben + 3 für die Verzeichnisse + 9 für die Datei.
    #[test]
    fn kopflaenge_stimmt() {
        let files = vec!["./a.fi".to_string()];
        let p = params(&files, "");
        let s = build(&p, &[Row { addr: 0x19, file: 1, line: 1, col: 0 }]);
        let hdrlen = u32::from_le_bytes(s.line[6..10].try_into().unwrap());
        assert_eq!(hdrlen, 29);
        assert_eq!(s.line[4..6], [3, 0]); // DWARF 3
        // Das Programm fängt hinter dem Kopf an.
        assert_eq!(4 + 2 + 4 + hdrlen as usize, 39);
        assert_eq!(s.line[39], DW_LNS_EXTENDED_OP);
        assert_eq!(s.line[41], DW_LNE_SET_ADDRESS);
        assert_eq!(s.line_rel[0].at, 42);
        assert_eq!(s.line_rel[0].addend, 0x19);
    }

    /// Der Sonderopcode, nachgerechnet: Adresse +14, Zeile +1 ergibt
    /// (1+5) + 14*14 + 13 = 215.
    #[test]
    fn sonderopcode_verdichtet() {
        let mut w = W::default();
        inc_line_addr(&mut w, Some(1), 14, 1);
        assert_eq!(w.b, vec![215]);
        // Zu weit für einen Opcode, aber nach `const_add_pc` passt es.
        let mut w = W::default();
        inc_line_addr(&mut w, Some(1), 22, 1);
        assert_eq!(w.b, vec![DW_LNS_CONST_ADD_PC, 89]);
        // Zu weit auch dafür: `advance_pc` und dann der Opcode mit
        // Adressschritt 0.
        let mut w = W::default();
        inc_line_addr(&mut w, Some(4), 39, 1);
        assert_eq!(w.b, vec![DW_LNS_ADVANCE_PC, 39, 22]);
        // Nichts bewegt sich: `copy`.
        let mut w = W::default();
        inc_line_addr(&mut w, Some(0), 0, 1);
        assert_eq!(w.b, vec![DW_LNS_COPY]);
        // Zeilensprung außerhalb der Spanne.
        let mut w = W::default();
        inc_line_addr(&mut w, Some(100), 0, 1);
        assert_eq!(w.b, vec![DW_LNS_ADVANCE_LINE, 0xE4, 0x00, DW_LNS_COPY]);
    }

    /// ARM64 zählt Adressschritte in Befehlen: 4 Oktett = 1 Schritt.
    #[test]
    fn adressschritt_wird_geteilt() {
        let mut w = W::default();
        inc_line_addr(&mut w, Some(1), 16, 4);
        // (1+5) + 4*14 + 13 = 75
        assert_eq!(w.b, vec![75]);
    }

    /// Die Endmarke legt selbst eine Zeile ab, also nie ein Sonderopcode.
    #[test]
    fn endmarke() {
        let mut w = W::default();
        inc_line_addr(&mut w, None, 267, 1);
        assert_eq!(w.b, vec![DW_LNS_ADVANCE_PC, 0x8B, 0x02, 0, 1, 1]);
        let mut w = W::default();
        inc_line_addr(&mut w, None, 17, 1);
        assert_eq!(w.b, vec![DW_LNS_CONST_ADD_PC, 0, 1, 1]);
        let mut w = W::default();
        inc_line_addr(&mut w, None, 0, 1);
        assert_eq!(w.b, vec![0, 1, 1]);
    }

    /// Die kleine Übersetzungseinheit hat die Größen, die `as` ihr gibt.
    #[test]
    fn kleine_einheit() {
        let files = vec!["./a.fi".to_string()];
        let p = params(&files, "");
        let s = build(&p, &[Row { addr: 0x19, file: 1, line: 1, col: 0 }]);
        assert_eq!(s.abbrev.len(), 20);
        assert_eq!(s.info.len(), 46);
        assert_eq!(s.aranges.len(), 48);
        assert_eq!(s.info_rel.len(), 7);
        assert_eq!(s.aranges_rel.len(), 2);
        // Name, Arbeitsverzeichnis, Erzeuger.
        assert!(s.dstr.starts_with(b"./a.fi\0"));
    }

    /// Bringt das Programm eine eigene `.debug_info` mit, legt `as` keine
    /// an — und wir auch nicht.
    #[test]
    fn eigene_einheit_verdraengt_die_kleine() {
        let files = vec!["./a.fi".to_string()];
        let mut p = params(&files, "");
        p.own_debug_info = true;
        let s = build(&p, &[Row { addr: 0, file: 1, line: 1, col: 0 }]);
        assert!(!s.line.is_empty());
        assert!(s.info.is_empty() && s.abbrev.is_empty() && s.dstr.is_empty());
    }
}
