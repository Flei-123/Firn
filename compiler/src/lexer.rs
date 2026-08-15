//! Handgeschriebener Lexer (kein Generator).
//!
//! SCHNITTSTELLE (fest, wird von parser.rs benutzt):
//!   `pub fn lex(src: &str, dg: &mut Diags) -> Vec<Token>`
//! Der Tokenstrom endet IMMER mit genau einem `TokKind::Eof`.
//! Spalten zaehlen ZEICHEN (1-basiert), Zeilen 1-basiert.

use crate::diag::{Diags, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokKind {
    // Literale und Namen
    Int(i128),
    Ident(String),
    /// Zeichenkettenliteral: `"..."`, `b"..."` oder `u"..."`.
    /// Der Inhalt ist bereits entschluesselt (`compiler/src/strings.rs`).
    Str(crate::strings::LitKind, crate::strings::LitValue),
    // Schluesselwoerter
    KwFn,
    KwLet,
    KwVar,
    KwIf,
    KwElse,
    KwWhile,
    KwReturn,
    KwStruct,
    KwConst,
    KwProfile,
    KwAs,
    KwMut,
    KwTrue,
    KwFalse,
    KwSyscall,
    KwExtern,
    KwBreak,
    KwContinue,
    KwDefer,
    KwErrDefer,
    KwFor,
    KwIn,
    KwImport,
    KwExport,
    /// Summentypen (SPEC §6.3) — verdrahtet vom Modul `types`.
    KwEnum,
    KwMatch,
    /// Fehlerunionen (SPEC §5.1) — verdrahtet vom Modul `fehlerunionen`.
    KwError,
    KwTry,
    KwCatch,
    // Satzzeichen
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semi,
    Dot,
    DotDot,  // ..
    Arrow,   // ->
    Assign,  // =
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp,     // &
    Pipe,    // |
    Caret,   // ^
    Shl,     // <<
    Shr,     // >>
    AndAnd,  // &&
    OrOr,    // ||
    Not,     // !
    Question, // ? (gepruefte Abwaertsumwandlung `x.as?[T]`)
    Hash,    // # (Attribute)
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    Eof,
}

impl TokKind {
    /// Beschreibung fuer Fehlermeldungen ("erwartet ')' ...").
    pub fn text(&self) -> String {
        match self {
            TokKind::Int(v) => format!("{}", v),
            TokKind::Ident(s) => s.clone(),
            TokKind::Str(k, v) => format!("{}\"…\" ({} elemente)", k.prefix(), v.len()),
            TokKind::KwFn => "fn".into(),
            TokKind::KwLet => "let".into(),
            TokKind::KwVar => "var".into(),
            TokKind::KwIf => "if".into(),
            TokKind::KwElse => "else".into(),
            TokKind::KwWhile => "while".into(),
            TokKind::KwReturn => "return".into(),
            TokKind::KwStruct => "struct".into(),
            TokKind::KwConst => "const".into(),
            TokKind::KwProfile => "profile".into(),
            TokKind::KwAs => "as".into(),
            TokKind::KwMut => "mut".into(),
            TokKind::KwTrue => "true".into(),
            TokKind::KwFalse => "false".into(),
            TokKind::KwSyscall => "syscall".into(),
            TokKind::KwExtern => "extern".into(),
            TokKind::KwBreak => "break".into(),
            TokKind::KwDefer => "defer".into(),
            TokKind::KwErrDefer => "errdefer".into(),
            TokKind::KwContinue => "continue".into(),
            TokKind::KwFor => "for".into(),
            TokKind::KwIn => "in".into(),
            TokKind::KwImport => "import".into(),
            TokKind::KwExport => "export".into(),
            TokKind::KwEnum => "enum".into(),
            TokKind::KwMatch => "match".into(),
            TokKind::KwError => "error".into(),
            TokKind::KwTry => "try".into(),
            TokKind::KwCatch => "catch".into(),
            TokKind::LParen => "(".into(),
            TokKind::RParen => ")".into(),
            TokKind::LBrace => "{".into(),
            TokKind::RBrace => "}".into(),
            TokKind::LBracket => "[".into(),
            TokKind::RBracket => "]".into(),
            TokKind::Comma => ",".into(),
            TokKind::Colon => ":".into(),
            TokKind::Semi => ";".into(),
            TokKind::Dot => ".".into(),
            TokKind::DotDot => "..".into(),
            TokKind::Arrow => "->".into(),
            TokKind::Assign => "=".into(),
            TokKind::Plus => "+".into(),
            TokKind::Minus => "-".into(),
            TokKind::Star => "*".into(),
            TokKind::Slash => "/".into(),
            TokKind::Percent => "%".into(),
            TokKind::Amp => "&".into(),
            TokKind::Pipe => "|".into(),
            TokKind::Caret => "^".into(),
            TokKind::Hash => "#".into(),
            TokKind::Question => "?".into(),
            TokKind::Shl => "<<".into(),
            TokKind::Shr => ">>".into(),
            TokKind::AndAnd => "&&".into(),
            TokKind::OrOr => "||".into(),
            TokKind::Not => "!".into(),
            TokKind::EqEq => "==".into(),
            TokKind::NotEq => "!=".into(),
            TokKind::Lt => "<".into(),
            TokKind::Le => "<=".into(),
            TokKind::Gt => ">".into(),
            TokKind::Ge => ">=".into(),
            TokKind::Eof => "Dateiende".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokKind,
    pub span: Span,
}

/// Schluesselwort oder Bezeichner.
fn keyword(word: &str) -> Option<TokKind> {
    Some(match word {
        "fn" => TokKind::KwFn,
        "let" => TokKind::KwLet,
        "var" => TokKind::KwVar,
        "if" => TokKind::KwIf,
        "else" => TokKind::KwElse,
        "while" => TokKind::KwWhile,
        "return" => TokKind::KwReturn,
        "struct" => TokKind::KwStruct,
        "const" => TokKind::KwConst,
        "profile" => TokKind::KwProfile,
        "as" => TokKind::KwAs,
        "mut" => TokKind::KwMut,
        "true" => TokKind::KwTrue,
        "false" => TokKind::KwFalse,
        "syscall" => TokKind::KwSyscall,
        "extern" => TokKind::KwExtern,
        "break" => TokKind::KwBreak,
        "defer" => TokKind::KwDefer,
        "errdefer" => TokKind::KwErrDefer,
        "continue" => TokKind::KwContinue,
        "for" => TokKind::KwFor,
        "in" => TokKind::KwIn,
        "import" => TokKind::KwImport,
        "export" => TokKind::KwExport,
        "enum" => TokKind::KwEnum,
        "match" => TokKind::KwMatch,
        "error" => TokKind::KwError,
        "try" => TokKind::KwTry,
        "catch" => TokKind::KwCatch,
        _ => return None,
    })
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}
fn is_ident_cont(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

struct Lexer<'a> {
    chars: Vec<char>,
    pos: usize,
    /// Nummer der Quelldatei in der Karte der `Diags` (Modulsystem).
    file: u32,
    line: u32,
    col: u32,
    dg: &'a mut Diags,
    out: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }
    /// Ein Zeichen weiter; fuehrt Zeile/Spalte nach.
    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }
    fn push(&mut self, kind: TokKind, line: u32, col: u32, len: u32) {
        self.out.push(Token { kind, span: Span::in_file(self.file, line, col, len) });
    }

    fn sp(&self, line: u32, col: u32, len: u32) -> Span {
        Span::in_file(self.file, line, col, len)
    }

    /// Whitespace und Kommentare ueberspringen. Meldet nicht geschlossene
    /// Blockkommentare, lext danach aber weiter.
    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('/') if self.peek2() == Some('/') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                Some('/') if self.peek2() == Some('*') => {
                    let (sl, sc) = (self.line, self.col);
                    self.bump();
                    self.bump();
                    let mut depth = 1usize;
                    while depth > 0 {
                        match self.peek() {
                            None => {
                                self.dg.error(
                                    self.sp(sl, sc, 2),
                                    "blockkommentar wird nicht geschlossen ('*/' fehlt)",
                                );
                                break;
                            }
                            Some('/') if self.peek2() == Some('*') => {
                                self.bump();
                                self.bump();
                                depth += 1;
                            }
                            Some('*') if self.peek2() == Some('/') => {
                                self.bump();
                                self.bump();
                                depth -= 1;
                            }
                            Some(_) => {
                                self.bump();
                            }
                        }
                    }
                }
                _ => return,
            }
        }
    }

    /// Zahl ab der aktuellen Position (Dezimal, 0x, 0b, '_' als Trenner).
    fn number(&mut self) {
        let (line, col) = (self.line, self.col);
        let mut ncols = 0u32;
        let mut digits = String::new();
        let mut radix = 10u32;
        // Praefix erkennen
        if self.peek() == Some('0') {
            match self.peek2() {
                Some('x') | Some('X') => radix = 16,
                Some('b') | Some('B') => radix = 2,
                _ => {}
            }
            if radix != 10 {
                self.bump();
                self.bump();
                ncols += 2;
            }
        }
        let mut bad_digit: Option<(char, u32, u32)> = None;
        while let Some(c) = self.peek() {
            if c == '_' {
                self.bump();
                ncols += 1;
                continue;
            }
            if c.is_ascii_alphanumeric() {
                if c.is_digit(radix) {
                    digits.push(c);
                } else if bad_digit.is_none() {
                    bad_digit = Some((c, self.line, self.col));
                }
                self.bump();
                ncols += 1;
                continue;
            }
            break;
        }
        let len = ncols.max(1);
        if let Some((c, bl, bc)) = bad_digit {
            self.dg.error(
                self.sp(bl, bc, 1),
                format!("ungueltiges zeichen '{}' in einem ganzzahlliteral zur basis {}", c, radix),
            );
            self.push(TokKind::Int(0), line, col, len);
            return;
        }
        if digits.is_empty() {
            self.dg.error(
                self.sp(line, col, len),
                format!("ganzzahlliteral ohne ziffern (basis {})", radix),
            );
            self.push(TokKind::Int(0), line, col, len);
            return;
        }
        let mut val: i128 = 0;
        for ch in digits.chars() {
            let d = match ch.to_digit(radix) {
                Some(d) => d as i128,
                None => 0,
            };
            match val.checked_mul(radix as i128).and_then(|v| v.checked_add(d)) {
                Some(v) if v <= u64::MAX as i128 => val = v,
                _ => {
                    self.dg.error(
                        self.sp(line, col, len),
                        "ganzzahlliteral ist zu gross (mehr als 64 bit)",
                    );
                    self.push(TokKind::Int(0), line, col, len);
                    return;
                }
            }
        }
        self.push(TokKind::Int(val), line, col, len);
    }

    fn ident(&mut self) {
        let (line, col) = (self.line, self.col);
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if is_ident_cont(c) {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        let len = s.chars().count() as u32;
        let kind = keyword(&s).unwrap_or(TokKind::Ident(s));
        self.push(kind, line, col, len);
    }

    /// Ein Operator/Satzzeichen. Gibt false zurueck, wenn das Zeichen unbekannt ist.
    fn punct(&mut self) -> bool {
        let (line, col) = (self.line, self.col);
        let c = match self.peek() {
            Some(c) => c,
            None => return true,
        };
        let n = self.peek2();
        let (kind, width) = match (c, n) {
            ('-', Some('>')) => (TokKind::Arrow, 2),
            ('.', Some('.')) => (TokKind::DotDot, 2),
            ('<', Some('<')) => (TokKind::Shl, 2),
            ('>', Some('>')) => (TokKind::Shr, 2),
            ('&', Some('&')) => (TokKind::AndAnd, 2),
            ('|', Some('|')) => (TokKind::OrOr, 2),
            ('=', Some('=')) => (TokKind::EqEq, 2),
            ('!', Some('=')) => (TokKind::NotEq, 2),
            ('<', Some('=')) => (TokKind::Le, 2),
            ('>', Some('=')) => (TokKind::Ge, 2),
            ('(', _) => (TokKind::LParen, 1),
            (')', _) => (TokKind::RParen, 1),
            ('{', _) => (TokKind::LBrace, 1),
            ('}', _) => (TokKind::RBrace, 1),
            ('[', _) => (TokKind::LBracket, 1),
            (']', _) => (TokKind::RBracket, 1),
            (',', _) => (TokKind::Comma, 1),
            (':', _) => (TokKind::Colon, 1),
            (';', _) => (TokKind::Semi, 1),
            ('.', _) => (TokKind::Dot, 1),
            ('=', _) => (TokKind::Assign, 1),
            ('+', _) => (TokKind::Plus, 1),
            ('-', _) => (TokKind::Minus, 1),
            ('*', _) => (TokKind::Star, 1),
            ('/', _) => (TokKind::Slash, 1),
            ('%', _) => (TokKind::Percent, 1),
            ('&', _) => (TokKind::Amp, 1),
            ('|', _) => (TokKind::Pipe, 1),
            ('^', _) => (TokKind::Caret, 1),
            ('#', _) => (TokKind::Hash, 1),
            ('!', _) => (TokKind::Not, 1),
            ('?', _) => (TokKind::Question, 1),
            ('<', _) => (TokKind::Lt, 1),
            ('>', _) => (TokKind::Gt, 1),
            _ => return false,
        };
        for _ in 0..width {
            self.bump();
        }
        self.push(kind, line, col, width);
        true
    }

    /// Zeichenkettenliteral lexen. `false`, wenn an dieser Stelle keines steht.
    ///
    /// Die eigentliche Entschluesselung — Maskierungen, `\\uXXXX` samt
    /// ungepaarter Surrogate, UTF-8-Pruefung — macht `strings.rs`. Hier wird sie
    /// nur angebunden; genau diese Anbindung fehlte bis Runde 8 (SPEC §14.1.str,
    /// Punkt S1).
    ///
    /// WICHTIG: der Aufruf steht VOR der Bezeichnererkennung, sonst frisst
    /// `is_ident_start` das `b` bzw. `u` von `b"..."` und `u"..."`.
    fn string_literal(&mut self) -> bool {
        let (line, col) = (self.line, self.col);
        let (kind, res, verbraucht) =
            match crate::strings::lex_string_literal(&self.chars, self.pos) {
                Some(x) => x,
                None => return false,
            };
        for _ in 0..verbraucht {
            self.bump();
        }
        match res {
            Ok(val) => self.push(TokKind::Str(kind, val), line, col, verbraucht as u32),
            Err(e) => {
                // Die Spalte des Fehlers liegt `e.off` Zeichen hinter dem Anfang.
                self.dg.error(
                    self.sp(line, col + e.off, 1),
                    format!("in einem zeichenkettenliteral: {}", e.msg),
                );
                // Weiterlexen mit einem leeren Literal, damit Folgefehler
                // nicht auf eine kaputte Tokenfolge zurueckgehen.
                let leer = match kind {
                    crate::strings::LitKind::Str16 => {
                        crate::strings::LitValue::Units(Vec::new())
                    }
                    _ => crate::strings::LitValue::Octets(Vec::new()),
                };
                self.push(TokKind::Str(kind, leer), line, col, verbraucht as u32);
            }
        }
        true
    }

    fn run(&mut self) {
        loop {
            self.skip_trivia();
            let c = match self.peek() {
                Some(c) => c,
                None => break,
            };
            if self.string_literal() {
                continue;
            }
            if c.is_ascii_digit() {
                self.number();
            } else if is_ident_start(c) {
                self.ident();
            } else if !self.punct() {
                let (line, col) = (self.line, self.col);
                self.dg.error(
                    self.sp(line, col, 1),
                    format!("unbekanntes zeichen '{}' im quelltext", c),
                );
                // Weiterlexen: das stoerende Zeichen wird uebersprungen.
                self.bump();
            }
        }
        let (line, col) = (self.line, self.col);
        self.push(TokKind::Eof, line.max(1), col.max(1), 1);
    }
}

pub fn lex(src: &str, dg: &mut Diags) -> Vec<Token> {
    lex_file(src, 0, dg)
}

/// Wie `lex`, aber fuer eine bestimmte Quelldatei der Karte (Modulsystem).
pub fn lex_file(src: &str, file: u32, dg: &mut Diags) -> Vec<Token> {
    let mut lx = Lexer {
        chars: src.chars().collect(),
        pos: 0,
        file,
        line: 1,
        col: 1,
        dg,
        out: Vec::new(),
    };
    lx.run();
    lx.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> (Vec<TokKind>, usize) {
        let mut dg = Diags::new("test", src);
        let toks = lex(src, &mut dg);
        (toks.into_iter().map(|t| t.kind).collect(), dg.count())
    }

    #[test]
    fn zahlen_und_trenner() {
        let (k, n) = kinds("1_000 0xFF 0b1010 0");
        assert_eq!(n, 0);
        assert_eq!(
            k,
            vec![
                TokKind::Int(1000),
                TokKind::Int(255),
                TokKind::Int(10),
                TokKind::Int(0),
                TokKind::Eof
            ]
        );
    }

    #[test]
    fn operatoren_maximal_lang() {
        let (k, n) = kinds("<< <= < >> >= > && & || | == = != ! ->");
        assert_eq!(n, 0);
        assert_eq!(k[0], TokKind::Shl);
        assert_eq!(k[1], TokKind::Le);
        assert_eq!(k[2], TokKind::Lt);
        assert_eq!(k[3], TokKind::Shr);
        assert_eq!(k[4], TokKind::Ge);
        assert_eq!(k[5], TokKind::Gt);
        assert_eq!(k[6], TokKind::AndAnd);
        assert_eq!(k[14], TokKind::Arrow);
    }

    #[test]
    fn kommentare_verschachtelt() {
        let (k, n) = kinds("1 /* a /* b */ c */ 2 // weg\n3");
        assert_eq!(n, 0);
        assert_eq!(k, vec![TokKind::Int(1), TokKind::Int(2), TokKind::Int(3), TokKind::Eof]);
    }

    #[test]
    fn positionen_sind_zeichenbasiert() {
        let src = "let a\n  bb = 1";
        let mut dg = Diags::new("test", src);
        let toks = lex(src, &mut dg);
        assert_eq!((toks[1].span.line, toks[1].span.col, toks[1].span.len), (1, 5, 1));
        assert_eq!((toks[2].span.line, toks[2].span.col, toks[2].span.len), (2, 3, 2));
    }

    #[test]
    fn fehler_dann_weiterlexen() {
        let (k, n) = kinds("1 § 2");
        assert_eq!(n, 1);
        assert_eq!(k, vec![TokKind::Int(1), TokKind::Int(2), TokKind::Eof]);
    }

    #[test]
    fn offener_blockkommentar_meldet_und_endet() {
        let (k, n) = kinds("1 /* offen");
        assert_eq!(n, 1);
        assert_eq!(k, vec![TokKind::Int(1), TokKind::Eof]);
    }

    #[test]
    fn zu_grosse_zahl() {
        let (_, n) = kinds("99999999999999999999999999");
        assert_eq!(n, 1);
    }
}
