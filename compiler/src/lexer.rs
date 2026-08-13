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

/// STUB — wird von Modul "frontend" implementiert.
pub fn lex(src: &str, dg: &mut Diags) -> Vec<Token> {
    let _ = src;
    dg.error(Span::none(), "Lexer ist in diesem Baustand noch nicht implementiert");
    vec![Token { kind: TokKind::Eof, span: Span::new(1, 1, 1) }]
}
