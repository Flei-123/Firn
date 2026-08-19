//! Zeichenketten im Compiler (SPEC §8) — Modul `str`.
//!
//! Diese Datei enthaelt die **Compilerseite** der vier Zeichenkettentypen:
//!
//! | Typ | Inhalt | geprueft | Layout |
//! |---|---|---|---|
//! | `Bytes` | rohe Oktette | nein | `{ ptr: *mut u8, len: usize, cap: usize }` |
//! | `Str` | UTF-8 | ja (beim Literal) | wie `Bytes` |
//! | `Str16` | WTF-16-Codeeinheiten | **nichts** | `{ ptr: *mut u16, len: usize, cap: usize }` |
//! | `Atom` | interniert | — | `u32` |
//!
//! `Str16` prueft und normalisiert **nichts**: ein einzelnes `\uD800` bleibt als
//! ungepaartes Surrogat erhalten (SPEC §8.2). `to_utf8` scheitert daran,
//! `to_utf8_lossy` ersetzt es durch U+FFFD, WTF-8 haelt es verlustfrei.
//!
//! Die Funktionen sind bewusst frei von Compiler-Innereien (kein `Diags`,
//! kein `Span`): der Lexer braucht genau einen Aufruf von
//! [`lex_string_literal`] und macht aus [`LitError`] eine Meldung mit
//! Zeile/Spalte. Diese Anbindung im Lexer gehoert dem Modul `kern` und ist in
//! dieser Runde noch nicht gesetzt (siehe `ACCEPTANCE.md`, Abschnitt `str`);
//! ueber `firnc --strlit <literal>` ist der gesamte Pfad trotzdem
//! nachpruefbar.

// ---------------------------------------------------------------------------
// Speicherlayout (Vertrag mit lib/str/*.fi und dem Modul tok)
// ---------------------------------------------------------------------------

/// Offset des Datenzeigers in `Bytes`/`Str`/`Str16`.
pub const SLICE_PTR_OFF: u64 = 0;
/// Offset der Laenge (in Elementen, nicht Bytes).
pub const SLICE_LEN_OFF: u64 = 8;
/// Offset der Kapazitaet (in Elementen).
pub const SLICE_CAP_OFF: u64 = 16;
/// Gesamtgroesse von `Bytes`/`Str`/`Str16`.
pub const SLICE_SIZE: u64 = 24;

/// Ersatzzeichen U+FFFD.
pub const REPLACEMENT: u32 = 0xFFFD;

// ---------------------------------------------------------------------------
// Literale
// ---------------------------------------------------------------------------

/// Welche der drei Literalformen vorliegt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LitKind {
    /// `b"..."` — rohe Oktette.
    Bytes,
    /// `"..."` — UTF-8, geprueft.
    Str,
    /// `u"..."` — WTF-16, prueft nichts.
    Str16,
}

impl LitKind {
    pub fn type_name(self) -> &'static str {
        match self {
            LitKind::Bytes => "Bytes",
            LitKind::Str => "Str",
            LitKind::Str16 => "Str16",
        }
    }
    /// Praefix vor dem Anfuehrungszeichen.
    pub fn prefix(self) -> &'static str {
        match self {
            LitKind::Bytes => "b",
            LitKind::Str => "",
            LitKind::Str16 => "u",
        }
    }
}

/// Der entschluesselte Inhalt eines Literals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LitValue {
    /// Oktette (`Bytes`) bzw. geprueftes UTF-8 (`Str`).
    Octets(Vec<u8>),
    /// `u16`-Codeeinheiten, ungepruef (`Str16`).
    Units(Vec<u16>),
}

impl LitValue {
    /// Anzahl der Elemente (Oktette bzw. Codeeinheiten).
    pub fn len(&self) -> usize {
        match self {
            LitValue::Octets(v) => v.len(),
            LitValue::Units(v) => v.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Assembler-Darstellung der Daten (`.byte`/`.short`), ohne Beschriftung.
    pub fn asm_data(&self) -> String {
        match self {
            LitValue::Octets(v) => data_line(".byte", v.iter().map(|b| *b as u64)),
            LitValue::Units(v) => data_line(".short", v.iter().map(|u| *u as u64)),
        }
    }
}

fn data_line(dir: &str, it: impl Iterator<Item = u64>) -> String {
    let parts: Vec<String> = it.map(|v| v.to_string()).collect();
    if parts.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for chunk in parts.chunks(16) {
        out.push_str("    ");
        out.push_str(dir);
        out.push(' ');
        out.push_str(&chunk.join(", "));
        out.push('\n');
    }
    out
}

/// Erkennt `f"..."` — die String-Interpolation (Runde 39).
///
/// Der Rumpf bleibt ROH (Klammern und Maskierungen unangetastet): das
/// Zerlegen in Text- und Ausdruckssegmente macht der Parser
/// (`parser.rs::interpolation`), das Entschluesseln der Textsegmente laeuft
/// danach ueber [`decode_literal`] wie bei jedem anderen Literal.
/// Rueckgabe wie bei [`lex_string_literal`]: `(Inhalt-oder-Fehler,
/// verbrauchte Zeichen)`, `None`, wenn hier kein `f"` steht.
pub fn lex_fstring_literal(src: &[char], pos: usize) -> Option<(Result<String, LitError>, usize)> {
    if src.get(pos) != Some(&'f') || src.get(pos + 1) != Some(&'"') {
        return None;
    }
    // Rumpf bis zum unmaskierten Anfuehrungszeichen — dieselbe Schleife wie
    // bei `lex_string_literal`, nur ohne Entschluesselung.
    let mut body: Vec<char> = Vec::new();
    let mut i = pos + 2;
    let mut closed = false;
    while let Some(&c) = src.get(i) {
        if c == '\\' {
            body.push(c);
            i += 1;
            match src.get(i) {
                Some(&n) if n != '\n' => {
                    body.push(n);
                    i += 1;
                }
                _ => break,
            }
            continue;
        }
        if c == '"' {
            closed = true;
            i += 1;
            break;
        }
        if c == '\n' {
            break;
        }
        body.push(c);
        i += 1;
    }
    let used = i - pos;
    if !closed {
        let e = LitError::new(body.len() + 2, "string literal without a closing \"");
        return Some((Err(e), used));
    }
    Some((Ok(body.into_iter().collect()), used))
}

/// Fehler beim Entschluesseln eines Literals.
///
/// `off` ist der Abstand in **Zeichen** vom oeffnenden Anfuehrungszeichen;
/// der Lexer addiert ihn auf die Spalte des Literalanfangs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LitError {
    pub off: u32,
    pub msg: String,
}

impl LitError {
    fn new(off: usize, msg: impl Into<String>) -> LitError {
        LitError { off: off as u32, msg: msg.into() }
    }
}

/// Erkennt am Zeichenstrom `src` ab `pos` ein Zeichenkettenliteral.
///
/// Rueckgabe: `(Art, Inhalt-oder-Fehler, Anzahl verbrauchter Zeichen)`.
/// `None`, wenn an dieser Stelle kein Literal beginnt. Der Lexer braucht genau
/// diesen einen Aufruf; die verbrauchte Zeichenzahl ist zugleich die Breite der
/// Markierung in der Fehlerausgabe.
pub fn lex_string_literal(
    src: &[char],
    pos: usize,
) -> Option<(LitKind, Result<LitValue, LitError>, usize)> {
    let (kind, quote_at) = match src.get(pos)? {
        '"' => (LitKind::Str, pos),
        'b' if src.get(pos + 1) == Some(&'"') => (LitKind::Bytes, pos + 1),
        'u' if src.get(pos + 1) == Some(&'"') => (LitKind::Str16, pos + 1),
        _ => return None,
    };
    // Rumpf bis zum unmaskierten Anfuehrungszeichen einsammeln.
    let mut body: Vec<char> = Vec::new();
    let mut i = quote_at + 1;
    let mut closed = false;
    while let Some(&c) = src.get(i) {
        if c == '\\' {
            body.push(c);
            i += 1;
            match src.get(i) {
                Some(&n) if n != '\n' => {
                    body.push(n);
                    i += 1;
                }
                _ => break,
            }
            continue;
        }
        if c == '"' {
            closed = true;
            i += 1;
            break;
        }
        if c == '\n' {
            break;
        }
        body.push(c);
        i += 1;
    }
    let used = i - pos;
    if !closed {
        let off = (quote_at - pos) + 1 + body.len();
        return Some((
            kind,
            Err(LitError::new(off, "string literal without a closing \"")),
            used,
        ));
    }
    let lead = (quote_at - pos) + 1; // Zeichen vor dem Rumpf
    let res = decode_literal(kind, &body).map_err(|e| LitError { off: e.off + lead as u32, ..e });
    Some((kind, res, used))
}

/// Entschluesselt den Rumpf eines Literals (ohne Anfuehrungszeichen).
pub fn decode_literal(kind: LitKind, body: &[char]) -> Result<LitValue, LitError> {
    let mut units: Vec<u16> = Vec::new(); // fuer Str16
    let mut octets: Vec<u8> = Vec::new(); // fuer Bytes/Str
    let mut i = 0usize;
    while i < body.len() {
        let c = body[i];
        if c != '\\' {
            i += 1;
            match kind {
                LitKind::Bytes => {
                    if (c as u32) > 0x7F {
                        return Err(LitError::new(
                            i - 1,
                            format!(
                                "character '{}' is not allowed in a Bytes literal, \
                                 write the octets as \\xNN",
                                c
                            ),
                        ));
                    }
                    octets.push(c as u8);
                }
                LitKind::Str => push_utf8(&mut octets, c as u32),
                LitKind::Str16 => push_utf16(&mut units, c as u32),
            }
            continue;
        }
        // Maskierung
        let start = i;
        i += 1;
        let e = match body.get(i) {
            Some(&e) => e,
            None => return Err(LitError::new(start, "escape at the end of the literal")),
        };
        i += 1;
        let simple = match e {
            'n' => Some(0x0A),
            'r' => Some(0x0D),
            't' => Some(0x09),
            '0' => Some(0x00),
            '\\' => Some(0x5C),
            '"' => Some(0x22),
            '\'' => Some(0x27),
            _ => None,
        };
        if let Some(v) = simple {
            match kind {
                LitKind::Bytes => octets.push(v as u8),
                LitKind::Str => push_utf8(&mut octets, v),
                LitKind::Str16 => units.push(v as u16),
            }
            continue;
        }
        match e {
            'x' => {
                let v = hex_fixed(body, i, 2).ok_or_else(|| {
                    LitError::new(start, "\\x expects exactly two hexadecimal digits")
                })?;
                i += 2;
                match kind {
                    LitKind::Bytes => octets.push(v as u8),
                    LitKind::Str => {
                        if v > 0x7F {
                            return Err(LitError::new(
                                start,
                                "\\xNN above 0x7F does not yield valid UTF-8, \
                                 write \\u{...}",
                            ));
                        }
                        octets.push(v as u8);
                    }
                    LitKind::Str16 => units.push(v as u16),
                }
            }
            'u' => {
                let (cp, width) = if body.get(i) == Some(&'{') {
                    let mut j = i + 1;
                    let mut val: u32 = 0;
                    let mut n = 0;
                    while let Some(&d) = body.get(j) {
                        if d == '}' {
                            break;
                        }
                        let h = d.to_digit(16).ok_or_else(|| {
                            LitError::new(start, "\\u{...} expects hexadecimal digits")
                        })?;
                        val = val * 16 + h;
                        if val > 0x10FFFF {
                            return Err(LitError::new(
                                start,
                                "code point above U+10FFFF is not representable",
                            ));
                        }
                        n += 1;
                        j += 1;
                    }
                    if n == 0 || body.get(j) != Some(&'}') {
                        return Err(LitError::new(start, "\\u{...} is not terminated"));
                    }
                    (val, j + 1 - i)
                } else {
                    let v = hex_fixed(body, i, 4).ok_or_else(|| {
                        LitError::new(start, "\\u expects exactly four hexadecimal digits")
                    })?;
                    (v, 4)
                };
                i += width;
                match kind {
                    LitKind::Bytes => {
                        return Err(LitError::new(
                            start,
                            "\\u is not allowed in a Bytes literal (Bytes is not text)",
                        ))
                    }
                    LitKind::Str16 => {
                        // Str16 prueft NICHTS: einzelne Surrogate bleiben erhalten.
                        if cp > 0xFFFF {
                            push_utf16(&mut units, cp);
                        } else {
                            units.push(cp as u16);
                        }
                    }
                    LitKind::Str => {
                        // In Str ist ein ungepaartes Surrogat ein Fehler; ein
                        // Paar 😀 wird zu einem Codepunkt vereinigt.
                        if is_high_surrogate(cp) {
                            if let Some((lo, w)) = peek_escaped_low(body, i) {
                                i += w;
                                push_utf8(&mut octets, combine(cp, lo));
                            } else {
                                return Err(LitError::new(
                                    start,
                                    format!(
                                        "unpaired surrogate U+{:04X} in a Str literal; \
                                         Str is checked UTF-8 — use u\"...\" (Str16)",
                                        cp
                                    ),
                                ));
                            }
                        } else if is_low_surrogate(cp) {
                            return Err(LitError::new(
                                start,
                                format!(
                                    "unpaired surrogate U+{:04X} in a Str literal; \
                                     Str is checked UTF-8 — use u\"...\" (Str16)",
                                    cp
                                ),
                            ));
                        } else {
                            push_utf8(&mut octets, cp);
                        }
                    }
                }
            }
            other => {
                return Err(LitError::new(
                    start,
                    format!("unknown escape '\\{}'", other),
                ))
            }
        }
    }
    Ok(match kind {
        LitKind::Str16 => LitValue::Units(units),
        _ => LitValue::Octets(octets),
    })
}

fn hex_fixed(body: &[char], at: usize, n: usize) -> Option<u32> {
    let mut v = 0u32;
    for k in 0..n {
        v = v * 16 + body.get(at + k)?.to_digit(16)?;
    }
    Some(v)
}

/// Sucht direkt hinter Position `i` ein `\uXXXX` mit tiefem Surrogat.
fn peek_escaped_low(body: &[char], i: usize) -> Option<(u32, usize)> {
    if body.get(i) != Some(&'\\') || body.get(i + 1) != Some(&'u') {
        return None;
    }
    let (cp, w) = if body.get(i + 2) == Some(&'{') {
        let mut j = i + 3;
        let mut val = 0u32;
        let mut n = 0;
        while let Some(&d) = body.get(j) {
            if d == '}' {
                break;
            }
            val = val * 16 + d.to_digit(16)?;
            n += 1;
            j += 1;
        }
        if n == 0 || body.get(j) != Some(&'}') {
            return None;
        }
        (val, j + 1 - i)
    } else {
        (hex_fixed(body, i + 2, 4)?, 6)
    };
    if is_low_surrogate(cp) {
        Some((cp, w))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// WTF-16 / UTF-8 / WTF-8
// ---------------------------------------------------------------------------

pub fn is_high_surrogate(cp: u32) -> bool {
    (0xD800..0xDC00).contains(&cp)
}
pub fn is_low_surrogate(cp: u32) -> bool {
    (0xDC00..0xE000).contains(&cp)
}
pub fn is_surrogate(cp: u32) -> bool {
    (0xD800..0xE000).contains(&cp)
}
fn combine(hi: u32, lo: u32) -> u32 {
    0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
}

/// Haengt einen Codepunkt als UTF-8 an (Surrogate werden WTF-8-artig kodiert).
pub fn push_utf8(out: &mut Vec<u8>, cp: u32) {
    if cp < 0x80 {
        out.push(cp as u8);
    } else if cp < 0x800 {
        out.push(0xC0 | (cp >> 6) as u8);
        out.push(0x80 | (cp & 0x3F) as u8);
    } else if cp < 0x10000 {
        out.push(0xE0 | (cp >> 12) as u8);
        out.push(0x80 | ((cp >> 6) & 0x3F) as u8);
        out.push(0x80 | (cp & 0x3F) as u8);
    } else {
        out.push(0xF0 | (cp >> 18) as u8);
        out.push(0x80 | ((cp >> 12) & 0x3F) as u8);
        out.push(0x80 | ((cp >> 6) & 0x3F) as u8);
        out.push(0x80 | (cp & 0x3F) as u8);
    }
}

/// Haengt einen Codepunkt als UTF-16 an (Werte > 0xFFFF werden zum Paar).
pub fn push_utf16(out: &mut Vec<u16>, cp: u32) {
    if cp < 0x10000 {
        out.push(cp as u16);
    } else {
        let v = cp - 0x10000;
        out.push(0xD800 + (v >> 10) as u16);
        out.push(0xDC00 + (v & 0x3FF) as u16);
    }
}

/// `Str -> Str16`: gelingt immer.
pub fn utf8_to_utf16(bytes: &[u8]) -> Vec<u16> {
    let mut out = Vec::new();
    for cp in (Utf8Iter { b: bytes, i: 0 }) {
        push_utf16(&mut out, cp);
    }
    out
}

/// `Str16 -> Str`, fehlbar: `None` bei einem ungepaarten Surrogat (SPEC §8.2).
pub fn to_utf8(units: &[u16]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < units.len() {
        let u = units[i] as u32;
        if is_high_surrogate(u) {
            let lo = *units.get(i + 1)? as u32;
            if !is_low_surrogate(lo) {
                return None;
            }
            push_utf8(&mut out, combine(u, lo));
            i += 2;
        } else if is_low_surrogate(u) {
            return None;
        } else {
            push_utf8(&mut out, u);
            i += 1;
        }
    }
    Some(out)
}

/// `Str16 -> Str`, ersetzend: ungepaarte Surrogate werden U+FFFD.
pub fn to_utf8_lossy(units: &[u16]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < units.len() {
        let u = units[i] as u32;
        if is_high_surrogate(u) {
            match units.get(i + 1).map(|v| *v as u32) {
                Some(lo) if is_low_surrogate(lo) => {
                    push_utf8(&mut out, combine(u, lo));
                    i += 2;
                }
                _ => {
                    push_utf8(&mut out, REPLACEMENT);
                    i += 1;
                }
            }
        } else if is_low_surrogate(u) {
            push_utf8(&mut out, REPLACEMENT);
            i += 1;
        } else {
            push_utf8(&mut out, u);
            i += 1;
        }
    }
    out
}

/// WTF-8: verlustfreie Bruecke, haelt ungepaarte Surrogate (SPEC §8.2).
pub fn to_wtf8(units: &[u16]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < units.len() {
        let u = units[i] as u32;
        if is_high_surrogate(u) {
            match units.get(i + 1).map(|v| *v as u32) {
                Some(lo) if is_low_surrogate(lo) => {
                    push_utf8(&mut out, combine(u, lo));
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        push_utf8(&mut out, u); // Surrogat bleibt als 3-Byte-Folge erhalten
        i += 1;
    }
    out
}

/// WTF-8 -> WTF-16, verlustfrei.
pub fn from_wtf8(bytes: &[u8]) -> Vec<u16> {
    let mut out = Vec::new();
    for cp in (Utf8Iter { b: bytes, i: 0 }) {
        if is_surrogate(cp) {
            out.push(cp as u16);
        } else {
            push_utf16(&mut out, cp);
        }
    }
    out
}

/// Prueft, ob `bytes` wohlgeformtes UTF-8 ist (ohne Surrogate, ohne Ueberlang).
pub fn is_valid_utf8(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
}

struct Utf8Iter<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Iterator for Utf8Iter<'a> {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        let b0 = *self.b.get(self.i)? as u32;
        let n = if b0 < 0x80 {
            1
        } else if b0 >= 0xF0 {
            4
        } else if b0 >= 0xE0 {
            3
        } else if b0 >= 0xC0 {
            2
        } else {
            1 // fortsetzungsbyte allein: als Ersatzzeichen behandeln
        };
        if n == 1 && b0 >= 0x80 {
            self.i += 1;
            return Some(REPLACEMENT);
        }
        let mut cp = match n {
            1 => b0,
            2 => b0 & 0x1F,
            3 => b0 & 0x0F,
            _ => b0 & 0x07,
        };
        for k in 1..n {
            match self.b.get(self.i + k) {
                Some(&c) if c & 0xC0 == 0x80 => cp = (cp << 6) | (c as u32 & 0x3F),
                _ => {
                    self.i += k;
                    return Some(REPLACEMENT);
                }
            }
        }
        self.i += n;
        Some(cp)
    }
}

// ---------------------------------------------------------------------------
// Atome (SPEC §8.3)
// ---------------------------------------------------------------------------

/// Haeufige Namen bekommen zur **Bauzeit** feste, kleine Nummern (SPEC §8.3),
/// damit `match` ueber Tag-Namen eine Sprungtabelle werden kann.
pub const STATIC_ATOMS: &[&str] = &[
    "", "a", "b", "br", "div", "em", "form", "h1", "h2", "h3", "head", "hr", "html", "i", "img",
    "input", "li", "link", "meta", "ol", "p", "script", "span", "strong", "style", "table", "tbody",
    "td", "th", "title", "tr", "ul", "body", "class", "id", "href", "src", "type", "value", "name",
    "style-attr", "alt", "width", "height", "rel", "content", "charset", "lang", "title-attr",
    "data",
];

/// Interniertabelle: Text -> `u32`. Vergleich zweier `Atom` ist ein
/// Ganzzahlvergleich.
#[derive(Clone, Debug)]
pub struct AtomTable {
    names: Vec<Vec<u8>>,
    map: std::collections::HashMap<Vec<u8>, u32>,
}

impl Default for AtomTable {
    fn default() -> AtomTable {
        AtomTable::new()
    }
}

impl AtomTable {
    pub fn new() -> AtomTable {
        let mut t = AtomTable { names: Vec::new(), map: std::collections::HashMap::new() };
        for s in STATIC_ATOMS {
            t.intern(s.as_bytes());
        }
        t
    }
    /// Liefert die Nummer; gleicher Text ergibt immer dieselbe Nummer.
    pub fn intern(&mut self, text: &[u8]) -> u32 {
        if let Some(&id) = self.map.get(text) {
            return id;
        }
        let id = self.names.len() as u32;
        self.names.push(text.to_vec());
        self.map.insert(text.to_vec(), id);
        id
    }
    pub fn text(&self, id: u32) -> Option<&[u8]> {
        self.names.get(id as usize).map(|v| v.as_slice())
    }
    pub fn len(&self) -> usize {
        self.names.len()
    }
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

// ---------------------------------------------------------------------------
// `--strlit`: Pruefwerkzeug fuer den gesamten Literalpfad
// ---------------------------------------------------------------------------

/// Entschluesselt ein Literal von der Befehlszeile und beschreibt das Ergebnis.
/// Rueckgabe `Err(text)` bei ungueltigem Literal (Spalte im Text enthalten).
pub fn strlit_report(lit: &str) -> Result<String, String> {
    let src: Vec<char> = lit.chars().collect();
    let (kind, res, used) = match lex_string_literal(&src, 0) {
        Some(t) => t,
        None => {
            return Err(format!(
                "not a string literal: expected \"...\", b\"...\" or u\"...\", found {}",
                lit
            ))
        }
    };
    let val = match res {
        Ok(v) => v,
        Err(e) => return Err(format!("column {}: {}", e.off + 1, e.msg)),
    };
    let mut out = String::new();
    out.push_str(&format!(
        "type      {}\nprefix    {}\"\nchars     {}\nelements  {}\nempty     {}\n\
         layout    ptr@{} len@{} cap@{} size {}\n",
        kind.type_name(),
        kind.prefix(),
        used,
        val.len(),
        if val.is_empty() { "ja" } else { "no" },
        SLICE_PTR_OFF,
        SLICE_LEN_OFF,
        SLICE_CAP_OFF,
        SLICE_SIZE
    ));
    match &val {
        LitValue::Octets(v) => {
            out.push_str("octets    ");
            out.push_str(&hex_list(v.iter().map(|b| *b as u32), 2));
            out.push('\n');
            out.push_str(&format!("utf8_ok   {}\n", if is_valid_utf8(v) { "ja" } else { "no" }));
            let units = utf8_to_utf16(v);
            out.push_str("as_str16 ");
            out.push_str(&hex_list(units.iter().map(|u| *u as u32), 4));
            out.push('\n');
        }
        LitValue::Units(v) => {
            out.push_str("units ");
            out.push_str(&hex_list(v.iter().map(|u| *u as u32), 4));
            out.push('\n');
            match to_utf8(v) {
                Some(b) => {
                    out.push_str("to_utf8   ");
                    out.push_str(&hex_list(b.iter().map(|x| *x as u32), 2));
                    out.push('\n');
                }
                None => out.push_str("to_utf8   nothing (unpaired surrogate)\n"),
            }
            let lossy = to_utf8_lossy(v);
            out.push_str("to_lossy  ");
            out.push_str(&hex_list(lossy.iter().map(|x| *x as u32), 2));
            out.push('\n');
            let w = to_wtf8(v);
            out.push_str("wtf8      ");
            out.push_str(&hex_list(w.iter().map(|x| *x as u32), 2));
            out.push('\n');
            let back = from_wtf8(&w);
            out.push_str(&format!(
                "wtf8_rt   {}\n",
                if back == *v { "bit identical" } else { "MISMATCH" }
            ));
        }
    }
    let mut atoms = AtomTable::new();
    let key: Vec<u8> = match &val {
        LitValue::Octets(v) => v.clone(),
        LitValue::Units(v) => to_wtf8(v),
    };
    let id = atoms.intern(&key);
    let back = atoms.text(id).map(|t| t.to_vec()).unwrap_or_default();
    out.push_str(&format!(
        "atom      {} (table {} entries, empty {}, wtf8 {})\n",
        id,
        atoms.len(),
        if atoms.is_empty() { "ja" } else { "no" },
        hex_list(back.iter().map(|b| *b as u32), 2)
    ));
    out.push_str(&format!("asm\n{}", val.asm_data()));
    Ok(out)
}

fn hex_list(it: impl Iterator<Item = u32>, w: usize) -> String {
    let v: Vec<String> = it.map(|x| format!("{:01$X}", x, w)).collect();
    if v.is_empty() {
        "(empty)".to_string()
    } else {
        v.join(" ")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(kind: LitKind, s: &str) -> LitValue {
        let body: Vec<char> = s.chars().collect();
        decode_literal(kind, &body).expect("literal should be valid")
    }
    fn err(kind: LitKind, s: &str) -> String {
        let body: Vec<char> = s.chars().collect();
        decode_literal(kind, &body).expect_err("literal should be faulty").msg
    }

    #[test]
    fn str_is_utf8() {
        assert_eq!(dec(LitKind::Str, "Hi"), LitValue::Octets(b"Hi".to_vec()));
        assert_eq!(dec(LitKind::Str, "\\u00e4"), LitValue::Octets(vec![0xC3, 0xA4]));
        assert_eq!(dec(LitKind::Str, "ä"), LitValue::Octets(vec![0xC3, 0xA4]));
        // Surrogatpaar wird zu einem Codepunkt
        assert_eq!(
            dec(LitKind::Str, "\\uD83D\\uDE00"),
            LitValue::Octets(vec![0xF0, 0x9F, 0x98, 0x80])
        );
        assert_eq!(dec(LitKind::Str, "\\u{1F600}"), LitValue::Octets(vec![0xF0, 0x9F, 0x98, 0x80]));
    }

    #[test]
    fn str_rejects_unpaired_surrogate_ab() {
        assert!(err(LitKind::Str, "\\uD800").contains("unpaired surrogate U+D800"));
        assert!(err(LitKind::Str, "\\uDC00x").contains("unpaired surrogate U+DC00"));
    }

    #[test]
    fn str16_holds_unpaired_surrogate() {
        let v = dec(LitKind::Str16, "a\\uD800b");
        assert_eq!(v, LitValue::Units(vec![0x61, 0xD800, 0x62]));
        let u = match v {
            LitValue::Units(u) => u,
            LitValue::Octets(_) => Vec::new(), // Str16 liefert immer Einheiten
        };
        assert!(!u.is_empty(), "Str16 must yield units");
        assert_eq!(to_utf8(&u), None, "to_utf8 must fail");
        assert_eq!(to_utf8_lossy(&u), vec![0x61, 0xEF, 0xBF, 0xBD, 0x62], "U+FFFD expected");
        // WTF-8 ist verlustfrei
        assert_eq!(from_wtf8(&to_wtf8(&u)), u);
    }

    #[test]
    fn str16_normalized_nothing() {
        // Ein Paar bleibt ein Paar, ein Nicht-BMP-Escape wird zum Paar.
        assert_eq!(dec(LitKind::Str16, "\\uD83D\\uDE00"), LitValue::Units(vec![0xD83D, 0xDE00]));
        assert_eq!(dec(LitKind::Str16, "\\u{1F600}"), LitValue::Units(vec![0xD83D, 0xDE00]));
        // Verdrehte Reihenfolge (low vor high) bleibt exakt erhalten.
        assert_eq!(dec(LitKind::Str16, "\\uDC00\\uD800"), LitValue::Units(vec![0xDC00, 0xD800]));
    }

    #[test]
    fn bytes_is_no_text() {
        assert_eq!(dec(LitKind::Bytes, "AB\\xff"), LitValue::Octets(vec![65, 66, 255]));
        assert!(err(LitKind::Bytes, "\\u0041").contains("Bytes is not text"));
        assert!(err(LitKind::Bytes, "ä").contains("not allowed"));
    }

    #[test]
    fn maskings() {
        assert_eq!(dec(LitKind::Str, "\\n\\r\\t\\0\\\\\\\""), LitValue::Octets(vec![10, 13, 9, 0, 92, 34]));
        assert!(err(LitKind::Str, "\\q").contains("unknown escape"));
        assert!(err(LitKind::Str, "\\u12").contains("four hexadecimal digits"));
        assert!(err(LitKind::Str, "\\u{").contains("not terminated"));
        assert!(err(LitKind::Str, "\\u{110000}").contains("U+10FFFF"));
        assert!(err(LitKind::Str, "\\xff").contains("does not yield valid UTF-8"));
    }

    #[test]
    fn lexer_hook() {
        let src: Vec<char> = r#"u"a\uD800" rest"#.chars().collect();
        let (kind, res, used) = lex_string_literal(&src, 0).expect("literal expected");
        assert_eq!(kind, LitKind::Str16);
        assert_eq!(used, 10);
        assert_eq!(res.unwrap(), LitValue::Units(vec![0x61, 0xD800]));

        let src: Vec<char> = r#""abc"#.chars().collect();
        let (_, res, _) = lex_string_literal(&src, 0).expect("literal expected");
        assert!(res.unwrap_err().msg.contains("without a closing"));

        assert!(lex_string_literal(&"abc".chars().collect::<Vec<_>>(), 0).is_none());
        // Spalte des Fehlers zeigt auf die Maskierung im Quelltext.
        let src: Vec<char> = r#""a\q""#.chars().collect();
        let (_, res, _) = lex_string_literal(&src, 0).unwrap();
        assert_eq!(res.unwrap_err().off, 2);
    }

    #[test]
    fn utf16_to_and_back() {
        let s = "Hallo, Wüste 🏜!";
        let u = utf8_to_utf16(s.as_bytes());
        assert_eq!(to_utf8(&u).unwrap(), s.as_bytes());
        assert_eq!(to_utf8_lossy(&u), s.as_bytes());
    }

    #[test]
    fn atoms_are_ints() {
        let mut t = AtomTable::new();
        let div = t.intern(b"div");
        assert_eq!(div, t.intern(b"div"));
        assert!(div < 64, "frequent atoms have small numbers");
        assert_ne!(div, t.intern(b"span"));
        let new = t.intern(b"karstos");
        assert_eq!(t.text(new), Some(&b"karstos"[..]));
        assert_eq!(t.len(), STATIC_ATOMS.len() + 1);
    }

    #[test]
    fn layout_is_contract() {
        assert_eq!((SLICE_PTR_OFF, SLICE_LEN_OFF, SLICE_CAP_OFF, SLICE_SIZE), (0, 8, 16, 24));
    }

    #[test]
    fn report_shows_surrogate() {
        let r = strlit_report(r#"u"\uD800""#).unwrap();
        assert!(r.contains("D800"), "{}", r);
        assert!(r.contains("to_utf8   nothing"), "{}", r);
        assert!(r.contains("EF BF BD"), "{}", r);
        assert!(r.contains("wtf8_rt   bit identical"), "{}", r);
        assert!(strlit_report("42").is_err());
    }
}
