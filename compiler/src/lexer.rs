// SPDX-License-Identifier: GPL-2.0-only
//! Hand-written lexer (no generator).
//!
//! INTERFACE (fixed, used by parser.rs):
//!   `pub fn lex(src: &str, dg: &mut Diags) -> Vec<Token>`
//! The token stream ALWAYS ends with exactly one `TokKind::Eof`.
//! Columns count CHARACTERS (1-based), lines are 1-based.

use crate::diag::{Diags, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokKind {
    // literals and names
    Int(i128),
    Ident(String),
    /// Float literal (`1.5`, `2e10`, `1_000.25`) as the **bit pattern** of an
    /// IEEE-754 binary64. No `f64`, because `TokKind` derives `Eq` and floats
    /// carry no equivalence relation (NaN != NaN) — and because FIR knows the
    /// bit pattern only anyway.
    ///
    /// **ROUND 71** — the token carries the binary32 bit pattern ALONGSIDE.
    /// Not out of thrift but out of necessity: the way decimal -> binary64
    /// -> binary32 is NOT correctly rounded (measured: 27% deviation from C
    /// `strtof` at the exact middles between two binary32). The narrowing
    /// has to happen on the TEXT, and the text only exists here.
    Float(u64, u32),
    /// **ROUND 71** — float literal with the suffix `f` (`1.5f`, `2f`,
    /// `1e3f`) as the **bit pattern** of an IEEE-754 binary32. The suffix is
    /// only needed where NO context says what is wanted; `let y: f32 = 2.5`
    /// gets by without it (SPEC §8.6).
    FloatF32(u32),
    /// String literal: `"..."`, `b"..."` or `u"..."`.
    /// The content is already decoded (`compiler/src/strings.rs`).
    Str(crate::strings::LitKind, crate::strings::LitValue),
    /// `f"..."` — string interpolation (round 39). The content is the RAW
    /// body; splitting it into text and expression segments is the parser's job.
    FStr(String),
    // keywords
    KwFn,
    KwLet,
    KwVar,
    KwIf,
    KwElse,
    KwWhile,
    KwReturn,
    KwStruct,
    KwConst,
    /// **ROUND 89** — `static` / `static mut` at top level (SPEC §14.1.statics).
    KwStatic,
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
    KwComptime,
    KwFor,
    KwIn,
    KwImport,
    KwExport,
    /// Sum types (SPEC §6.3) — wired up by the module `types`.
    KwEnum,
    KwMatch,
    /// Error unions (SPEC §5.1) — wired up by the module `fehlerunionen`.
    KwError,
    KwTry,
    KwCatch,
    // punctuation
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
    /// `~` — the bitwise complement (round 68).
    Tilde,   // ~
    Question, // ? (checked downcast `x.as?[T]`)
    Hash,    // # (attributes)
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    /// **ROUND 70** - the compound assignments `+= -= *= /= %=` and the bit
    /// variants `&= |= ^= <<= >>=`. They open a STATEMENT, not an operator:
    /// `x += e` is exactly `x = x + e` (SPEC 12.7).
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    AmpEq,
    PipeEq,
    CaretEq,
    ShlEq,
    ShrEq,
    /// **ROUND 70** - `++` and `--`. Only as a statement, never inside an
    /// expression; the prefix/postfix difference of C does not exist here.
    PlusPlus,
    MinusMinus,
    /// **ROUND 72** — explicit wrapping arithmetic (SPEC §13, `L9`):
    /// `+% -% *%`. Never checked, regardless of the build level.
    PlusPercent,
    MinusPercent,
    StarPercent,
    /// **ROUND 72** — explicit saturating arithmetic: `+| -| *|`.
    PlusPipe,
    MinusPipe,
    StarPipe,
    Eof,
}

impl TokKind {
    /// Description for error messages ("expected ')' ...").
    pub fn text(&self) -> String {
        match self {
            TokKind::Int(v) => format!("{}", v),
            TokKind::Float(bits, _) => format!("{}", f64::from_bits(*bits)),
            TokKind::FloatF32(bits) => format!("{}f", f32::from_bits(*bits)),
            TokKind::Ident(s) => s.clone(),
            TokKind::Str(k, v) => format!("{}\"…\" ({} elements)", k.prefix(), v.len()),
            TokKind::FStr(_) => "f\"…\" (interpolation)".into(),
            TokKind::KwFn => "fn".into(),
            TokKind::KwLet => "let".into(),
            TokKind::KwVar => "var".into(),
            TokKind::KwIf => "if".into(),
            TokKind::KwElse => "else".into(),
            TokKind::KwWhile => "while".into(),
            TokKind::KwReturn => "return".into(),
            TokKind::KwStruct => "struct".into(),
            TokKind::KwConst => "const".into(),
            TokKind::KwStatic => "static".into(),
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
            TokKind::KwComptime => "comptime".into(),
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
            TokKind::Tilde => "~".into(),
            TokKind::PlusEq => "+=".into(),
            TokKind::MinusEq => "-=".into(),
            TokKind::StarEq => "*=".into(),
            TokKind::SlashEq => "/=".into(),
            TokKind::PercentEq => "%=".into(),
            TokKind::AmpEq => "&=".into(),
            TokKind::PipeEq => "|=".into(),
            TokKind::CaretEq => "^=".into(),
            TokKind::ShlEq => "<<=".into(),
            TokKind::ShrEq => ">>=".into(),
            TokKind::PlusPlus => "++".into(),
            TokKind::MinusMinus => "--".into(),
            TokKind::PlusPercent => "+%".into(),
            TokKind::MinusPercent => "-%".into(),
            TokKind::StarPercent => "*%".into(),
            TokKind::PlusPipe => "+|".into(),
            TokKind::MinusPipe => "-|".into(),
            TokKind::StarPipe => "*|".into(),
            TokKind::EqEq => "==".into(),
            TokKind::NotEq => "!=".into(),
            TokKind::Lt => "<".into(),
            TokKind::Le => "<=".into(),
            TokKind::Gt => ">".into(),
            TokKind::Ge => ">=".into(),
            TokKind::Eof => "end of file".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokKind,
    pub span: Span,
}

/// Keyword or identifier.
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
        "static" => TokKind::KwStatic,
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
        "comptime" => TokKind::KwComptime,
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

/// **ROUND 71** — decimal text -> `f32` bit pattern, CORRECTLY ROUNDED.
///
/// `parse::<f32>` rounds the text directly, as `strtof` does. The detour
/// through the binary64 next to it looks harmless and is not: for the exact
/// middle between two binary32 values the intermediate rounding lands ON
/// the middle, the tie-to-even then decides in the wrong direction, and the
/// result is one ulp off. Measured against glibc `strtof`: 63568 of 239064
/// such cases. Figueroa's theorem does not carry here -- it holds for the
/// results of ARITHMETIC, not for an arbitrary decimal.
pub fn narrow_text(text: &str) -> u32 {
    let v: f32 = text.parse().unwrap_or(0.0);
    v.to_bits()
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
    /// Number of the source file within the map of `Diags` (module system).
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
    /// ROUND 70: needed for `<<=` and `>>=`.
    fn peek3(&self) -> Option<char> {
        self.chars.get(self.pos + 2).copied()
    }
    /// One character onwards; keeps line/column up to date.
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

    /// Skip whitespace and comments. Reports unclosed block comments, but
    /// keeps lexing afterwards.
    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                // ROUND 64: only ASCII blanks count, exactly as in
                // `lib/firnc1/lexer.fi::is_empty`. `char::is_whitespace`
                // additionally swallows U+00A0, U+2007, U+202F, U+3000 --
                // then the two lexers disagreed, and a non-breaking blank
                // pasted in from a document vanished without a trace. It is
                // now what it really is: an unknown character, with a
                // suggestion (`char_hint`).
                Some(c) if is_ascii_blank(c) => {
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
                                    "block comment is not closed ('*/' missing)",
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

    /// Number from the current position (decimal, 0x, 0b, '_' as separator).
    /// Rest of a float literal from the point or the exponent onwards.
    /// `front` holds the digits before the point already read (without `_`).
    fn float_rest(&mut self, line: u32, col: u32, mut ncols: u32, front: String) {
        let mut text = front;
        if self.peek() == Some('.') {
            text.push('.');
            self.bump();
            ncols += 1;
            while let Some(c) = self.peek() {
                if c == '_' {
                    self.bump();
                    ncols += 1;
                    continue;
                }
                if !c.is_ascii_digit() {
                    break;
                }
                text.push(c);
                self.bump();
                ncols += 1;
            }
        }
        if matches!(self.peek(), Some('e') | Some('E')) {
            text.push('e');
            self.bump();
            ncols += 1;
            if matches!(self.peek(), Some('+') | Some('-')) {
                if let Some(c) = self.peek() {
                    text.push(c);
                }
                self.bump();
                ncols += 1;
            }
            let mut digits = 0;
            while let Some(c) = self.peek() {
                if c == '_' {
                    self.bump();
                    ncols += 1;
                    continue;
                }
                if !c.is_ascii_digit() {
                    break;
                }
                text.push(c);
                digits += 1;
                self.bump();
                ncols += 1;
            }
            if digits == 0 {
                self.dg.error(
                    self.sp(line, col, ncols.max(1)),
                    "floating point literal: the digits of the exponent are missing after 'e'",
                );
                self.push(TokKind::Float(0, 0), line, col, ncols.max(1));
                return;
            }
        }
        // `parse::<f64>` rounds correctly (Rust uses the same algorithm for that
        // as `strtod`); an overflow yields `inf`, which is wanted.
        let v: f64 = text.parse().unwrap_or(0.0);
        // ROUND 71: the suffix `f` makes an `f32` out of it. A LETTER may not
        // follow, otherwise `1.5foo` would silently become `1.5f` plus `oo`.
        let single = narrow_text(&text);
        if self.peek() == Some('f') && !self.peek2().map(is_ident_cont).unwrap_or(false) {
            self.bump();
            ncols += 1;
            self.push(TokKind::FloatF32(single), line, col, ncols.max(1));
            return;
        }
        self.push(TokKind::Float(v.to_bits(), single), line, col, ncols.max(1));
    }

    fn number(&mut self) {
        let (line, col) = (self.line, self.col);
        let mut ncols = 0u32;
        let mut digits = String::new();
        let mut radix = 10u32;
        // spot the prefix
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
                // FLOAT EXPONENT: at base 10 an `e`/`E` followed by a digit
                // or a sign ends the integer — otherwise it would be reported
                // here as an invalid digit before the float check even gets
                // its turn (`1e3`).
                if radix == 10
                    && (c == 'e' || c == 'E')
                    && (self.peek2().map(|n| n.is_ascii_digit()) == Some(true)
                        || matches!(self.peek2(), Some('+') | Some('-')))
                {
                    break;
                }
                // ROUND 71: `2f` — the f32 suffix on a literal without a
                // point and without an exponent. Only at base 10 (at base 16
                // `f` is a digit) and only when no letter follows.
                if radix == 10
                    && c == 'f'
                    && !digits.is_empty()
                    && bad_digit.is_none()
                    && !self.peek2().map(is_ident_cont).unwrap_or(false)
                {
                    self.bump();
                    ncols += 1;
                    self.push(TokKind::FloatF32(narrow_text(&digits)), line, col, ncols.max(1));
                    return;
                }
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
        // FLOAT (base 10 only): a point counts as part of it only when a
        // DIGIT follows — `0..10` stays the range of a `for` loop and is
        // not read as `0.`.
        if radix == 10 && bad_digit.is_none() && !digits.is_empty() {
            let dot = self.peek() == Some('.') && self.peek2().map(|c| c.is_ascii_digit()) == Some(true);
            let expo = matches!(self.peek(), Some('e') | Some('E'))
                && (self.peek2().map(|c| c.is_ascii_digit()) == Some(true)
                    || matches!(self.peek2(), Some('+') | Some('-')));
            if dot || expo {
                return self.float_rest(line, col, ncols, digits);
            }
        }
        let len = ncols.max(1);
        if let Some((c, bl, bc)) = bad_digit {
            self.dg.error(
                self.sp(bl, bc, 1),
                format!("invalid character '{}' in an integer literal to base {}", c, radix),
            );
            self.push(TokKind::Int(0), line, col, len);
            return;
        }
        if digits.is_empty() {
            self.dg.error(
                self.sp(line, col, len),
                format!("integer literal without digits (base {})", radix),
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
                        "integer literal is too large (more than 64 bit)",
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

    /// An operator/punctuation mark. Returns false when the character is unknown.
    fn punct(&mut self) -> bool {
        let (line, col) = (self.line, self.col);
        let c = match self.peek() {
            Some(c) => c,
            None => return true,
        };
        let n = self.peek2();
        // ROUND 70: THREE characters first - `<<=` and `>>=` would otherwise
        // be read as `<<` plus `=`, and the compound assignment would fall
        // apart into two tokens.
        let three = match (c, n, self.peek3()) {
            ('<', Some('<'), Some('=')) => Some(TokKind::ShlEq),
            ('>', Some('>'), Some('=')) => Some(TokKind::ShrEq),
            _ => None,
        };
        if let Some(k) = three {
            self.bump();
            self.bump();
            self.bump();
            self.push(k, line, col, 3);
            return true;
        }
        let (kind, width) = match (c, n) {
            ('-', Some('>')) => (TokKind::Arrow, 2),
            // ROUND 70: the compound assignments and the two step operators.
            // `++`/`--` are read GREEDILY; `a - -b` stays two tokens because
            // of the blank between them, `a --b` becomes `a`, `--`, `b`.
            ('+', Some('=')) => (TokKind::PlusEq, 2),
            ('-', Some('=')) => (TokKind::MinusEq, 2),
            ('*', Some('=')) => (TokKind::StarEq, 2),
            ('/', Some('=')) => (TokKind::SlashEq, 2),
            ('%', Some('=')) => (TokKind::PercentEq, 2),
            ('&', Some('=')) => (TokKind::AmpEq, 2),
            ('|', Some('=')) => (TokKind::PipeEq, 2),
            ('^', Some('=')) => (TokKind::CaretEq, 2),
            ('+', Some('+')) => (TokKind::PlusPlus, 2),
            ('-', Some('-')) => (TokKind::MinusMinus, 2),
            // ROUND 72: explicit wrap/saturate arithmetic (SPEC section 13,
            // item L9) -- read GREEDILY like the pairs above them, before
            // the one-character '+'/'-'/'*' fallback further down.
            ('+', Some('%')) => (TokKind::PlusPercent, 2),
            ('-', Some('%')) => (TokKind::MinusPercent, 2),
            ('*', Some('%')) => (TokKind::StarPercent, 2),
            ('+', Some('|')) => (TokKind::PlusPipe, 2),
            ('-', Some('|')) => (TokKind::MinusPipe, 2),
            ('*', Some('|')) => (TokKind::StarPipe, 2),
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
            ('~', _) => (TokKind::Tilde, 1),
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

    /// Lex a string literal. `false` when none stands at this place.
    ///
    /// The decoding proper — escapes, `\uXXXX` including unpaired surrogates,
    /// UTF-8 check — is done by `strings.rs`. Here it is only wired up;
    /// exactly that wiring was missing until round 8 (SPEC §14.1.str,
    /// point S1).
    ///
    /// IMPORTANT: the call stands BEFORE identifier recognition, otherwise
    /// `is_ident_start` eats the `b` or `u` of `b"..."` and `u"..."`.
    fn string_literal(&mut self) -> bool {
        let (line, col) = (self.line, self.col);
        // FIRST the interpolation: `f"` would otherwise be identifier + literal.
        if let Some((res, consumed)) = crate::strings::lex_fstring_literal(&self.chars, self.pos) {
            for _ in 0..consumed {
                self.bump();
            }
            match res {
                Ok(raw) => self.push(TokKind::FStr(raw), line, col, consumed as u32),
                Err(e) => {
                    self.dg.error(
                        self.sp(line, col + e.off, 1),
                        format!("in a string literal: {}", e.msg),
                    );
                    // Keep lexing with an empty body — as with the other literals.
                    self.push(TokKind::FStr(String::new()), line, col, consumed as u32);
                }
            }
            return true;
        }
        let (kind, res, consumed) =
            match crate::strings::lex_string_literal(&self.chars, self.pos) {
                Some(x) => x,
                None => return false,
            };
        for _ in 0..consumed {
            self.bump();
        }
        match res {
            Ok(val) => self.push(TokKind::Str(kind, val), line, col, consumed as u32),
            Err(e) => {
                // The column of the error sits `e.off` characters past the start.
                self.dg.error(
                    self.sp(line, col + e.off, 1),
                    format!("in a string literal: {}", e.msg),
                );
                // Keep lexing with an empty literal, so that follow-up errors
                // do not trace back to a broken token sequence.
                let empty = match kind {
                    crate::strings::LitKind::Str16 => {
                        crate::strings::LitValue::Units(Vec::new())
                    }
                    _ => crate::strings::LitValue::Octets(Vec::new()),
                };
                self.push(TokKind::Str(kind, empty), line, col, consumed as u32);
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
                let msg = format!("unknown character '{}' in the source text", c);
                match char_hint(c) {
                    Some(h) => self.dg.error_help(self.sp(line, col, 1), msg, h),
                    None => self.dg.error(self.sp(line, col, 1), msg),
                }
                // Keep lexing: the offending character is skipped.
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

/// Like `lex`, but for a particular source file of the map (module system).
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
    fn numbers_and_sep() {
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
    fn operators_max_long() {
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
    fn comments_nested() {
        let (k, n) = kinds("1 /* a /* b */ c */ 2 // gone\n3");
        assert_eq!(n, 0);
        assert_eq!(k, vec![TokKind::Int(1), TokKind::Int(2), TokKind::Int(3), TokKind::Eof]);
    }

    #[test]
    fn positions_are_char_based() {
        let src = "let a\n  bb = 1";
        let mut dg = Diags::new("test", src);
        let toks = lex(src, &mut dg);
        assert_eq!((toks[1].span.line, toks[1].span.col, toks[1].span.len), (1, 5, 1));
        assert_eq!((toks[2].span.line, toks[2].span.col, toks[2].span.len), (2, 3, 2));
    }

    #[test]
    fn error_then_relex() {
        let (k, n) = kinds("1 § 2");
        assert_eq!(n, 1);
        assert_eq!(k, vec![TokKind::Int(1), TokKind::Int(2), TokKind::Eof]);
    }

    #[test]
    fn open_block_comment_reports_and_ends() {
        let (k, n) = kinds("1 /* open");
        assert_eq!(n, 1);
        assert_eq!(k, vec![TokKind::Int(1), TokKind::Eof]);
    }

    #[test]
    fn too_big_number() {
        let (_, n) = kinds("99999999999999999999999999");
        assert_eq!(n, 1);
    }
}

// -------------------------------------------------- suggestions (round 64)
//
// "unknown character" was the message that helped the least: it named the
// character and left the reader alone with it. Yet exactly these characters
// come out of copying from a document or a web page -- typographic quotation
// marks, an en dash instead of a minus, a non-breaking blank. The lexer knows
// them and now says what to write instead.
//
// The twin in Firn is `lib/firnc1/lexer.fi::char_hint`; the texts have to be
// identical octet for octet, and `tools/lex_compare.sh` compares the whole
// error output of both lexers.
/// The blanks of the language: ASCII, nothing else. The counterpart in Firn
/// is `lib/firnc1/lexer.fi::is_empty`.
pub fn is_ascii_blank(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\u{000B}' || c == '\u{000C}'
}

pub fn char_hint(c: char) -> Option<String> {
    match c {
        // apostrophe and single typographic quotation marks
        '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => Some(
            "Firn has no character literals -- write a text literal with '\"'".to_string(),
        ),
        // double typographic quotation marks, guillemets, backtick
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{00AB}' | '\u{00BB}' | '`' => {
            Some("did you mean the quotation mark '\"'?".to_string())
        }
        // en dash, em dash, the real minus sign
        '\u{2013}' | '\u{2014}' | '\u{2212}' => {
            Some("did you mean the minus sign '-'?".to_string())
        }
        // blanks that do not look like blanks
        '\u{00A0}' | '\u{2007}' | '\u{202F}' | '\u{3000}' => Some(format!(
            "that is the blank U+{:04X}, not the blank U+0020",
            c as u32
        )),
        '@' => Some(
            "Firn has no annotations with '@' -- an attribute is written '#[name]'".to_string(),
        ),
        _ => None,
    }
}
