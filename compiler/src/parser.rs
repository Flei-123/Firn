//! Handgeschriebener, rekursiv absteigender Parser (kein Generator).
//!
//! SCHNITTSTELLE (fest):
//!   `pub fn parse(toks: &[Token], dg: &mut Diags) -> ast::Program`
//! Der Parser vergibt fortlaufende `ExprId`s ab 0 und setzt
//! `Program::expr_count`. Er MUSS sich nach einem Fehler auf Anweisungs-
//! bzw. Item-Ebene erholen und weitere Fehler melden.
//!
//! Grammatik: SPEC §10.1. Praezedenz (schwach -> stark):
//!   `||` , `&&` , Vergleich (nicht assoziativ) , `+ - | ^` , `* / % & << >>` ,
//!   unaeres `- ! & *` , postfix `. [] () as`.
//! Semikolon ist optional: ein Zeilenwechsel beendet eine Anweisung.

use crate::ast::{
    Attr, Block, ConstDecl, Expr, ExprKind, BinOp, FnDecl, ImportDecl, Param, Program, Stmt,
    StructDecl, TypeExpr, UnOp,
};
use std::collections::HashSet;
use crate::diag::{Diags, Span};
use crate::lexer::{TokKind, Token};

/// Maximale Verschachtelungstiefe (Ausdruecke, Bloecke, Typen). Darueber gibt es
/// einen sauberen Fehler statt eines Stapelueberlaufs.
const MAX_DEPTH: u32 = 200;

pub(crate) struct Parser<'a> {
    pub(crate) toks: &'a [Token],
    pub(crate) pos: usize,
    pub(crate) dg: &'a mut Diags,
    pub(crate) next_id: u32,
    pub(crate) depth: u32,
    /// Innerhalb der aktuellen Anweisung wurde bereits ein Fehler gemeldet.
    pub(crate) recovering: bool,
    /// `ident {` ist in Bedingungen KEIN Struct-Literal, sondern Name + Block.
    pub(crate) no_struct_lit: bool,
    /// Klammertiefe: innerhalb von `(...)`, `[...]`, `{...}` eines Ausdrucks
    /// darf ein Ausdruck ueber Zeilen laufen, ausserhalb nicht.
    pub(crate) paren_depth: u32,
    /// Nummer der Quelldatei (Modulsystem, `modules.rs`).
    pub(crate) file: u32,
    /// Bekannte Modulnamen aus `import`: nur damit ist `alias.name` ein
    /// qualifizierter Name und kein Feldzugriff.
    pub(crate) modules: HashSet<String>,
    /// Verschachtelungstiefe der Schleifen — `break`/`continue` brauchen sie.
    pub(crate) loop_depth: u32,
    /// Attribute, die unmittelbar vor der naechsten Deklaration standen
    /// (`attrs.rs`). Werden von `fn_decl`/`struct_decl` uebernommen.
    pub(crate) pending_attrs: Vec<crate::ast::Attr>,
}

fn starts_stmt(k: &TokKind) -> bool {
    matches!(
        k,
        TokKind::KwLet
            | TokKind::KwVar
            | TokKind::KwIf
            | TokKind::KwWhile
            | TokKind::KwReturn
            | TokKind::KwFn
            | TokKind::KwStruct
            | TokKind::KwConst
            | TokKind::KwFor
            | TokKind::KwBreak
            | TokKind::KwContinue
            | TokKind::KwDefer
            | TokKind::KwErrDefer
            | TokKind::KwMatch
    )
}

fn starts_item(k: &TokKind) -> bool {
    matches!(
        k,
        TokKind::KwFn
            | TokKind::KwStruct
            | TokKind::KwConst
            | TokKind::KwProfile
            | TokKind::KwExtern
            | TokKind::KwImport
            | TokKind::KwExport
            | TokKind::KwEnum
            | TokKind::KwError
    )
}

impl<'a> Parser<'a> {
    // ---------------------------------------------------------------- Grundlagen

    pub(crate) fn kind(&self) -> &TokKind {
        // Der Strom endet immer mit Eof; der Index wird nie darueber hinaus erhoeht.
        match self.toks.get(self.pos) {
            Some(t) => &t.kind,
            None => &TokKind::Eof,
        }
    }

    pub(crate) fn span(&self) -> Span {
        match self.toks.get(self.pos) {
            Some(t) => t.span,
            None => match self.toks.last() {
                Some(t) => t.span,
                None => Span::in_file(self.file, 1, 1, 1),
            },
        }
    }

    pub(crate) fn at(&self, k: &TokKind) -> bool {
        self.kind() == k
    }

    pub(crate) fn at_eof(&self) -> bool {
        matches!(self.kind(), TokKind::Eof)
    }

    /// Steht das aktuelle Token als erstes auf seiner Zeile?
    fn at_line_start(&self) -> bool {
        if self.pos == 0 {
            return true;
        }
        match (self.toks.get(self.pos), self.toks.get(self.pos - 1)) {
            (Some(cur), Some(prev)) => cur.span.line > prev.span.line,
            _ => true,
        }
    }

    /// Darf der Ausdruck mit dem aktuellen Token fortgesetzt werden?
    /// Ein Operator am ZEILENANFANG beendet ausserhalb von Klammern die
    /// Anweisung (SPEC §10: Semikolon optional, Zeilenende beendet sie).
    fn cont(&self) -> bool {
        self.paren_depth > 0 || !self.at_line_start()
    }

    pub(crate) fn bump(&mut self) -> Span {
        let s = self.span();
        if !self.at_eof() {
            self.pos += 1;
        }
        s
    }

    pub(crate) fn eat(&mut self, k: &TokKind) -> bool {
        if self.at(k) {
            self.bump();
            true
        } else {
            false
        }
    }

    pub(crate) fn error_here(&mut self, msg: impl Into<String>) {
        let sp = self.span();
        self.dg.error(sp, msg);
        self.recovering = true;
    }

    /// Erwartet ein bestimmtes Token; sonst Fehler am STOERENDEN Token.
    pub(crate) fn expect(&mut self, k: TokKind, ctx: &str) -> bool {
        if self.eat(&k) {
            return true;
        }
        if !self.recovering {
            self.error_here(format!(
                "erwartet '{}' {}, gefunden '{}'",
                k.text(),
                ctx,
                self.kind().text()
            ));
        }
        false
    }

    /// Wie `expect`, meldet aber nichts, wenn die Anweisung schon kaputt ist.
    pub(crate) fn close(&mut self, k: TokKind, ctx: &str) -> bool {
        if self.eat(&k) {
            return true;
        }
        if self.recovering {
            return false;
        }
        self.error_here(format!(
            "erwartet '{}' {}, gefunden '{}'",
            k.text(),
            ctx,
            self.kind().text()
        ));
        false
    }

    pub(crate) fn ident(&mut self, ctx: &str) -> Option<(String, Span)> {
        if let TokKind::Ident(name) = self.kind() {
            let name = name.clone();
            let sp = self.bump();
            return Some((name, sp));
        }
        if !self.recovering {
            self.error_here(format!(
                "erwartet einen namen {}, gefunden '{}'",
                ctx,
                self.kind().text()
            ));
        }
        None
    }

    /// Qualifizierter Name `modul.name`: nur wenn `modul` per `import`
    /// bekannt ist, wird der Punkt als Modulzugriff gelesen — sonst bleibt es
    /// ein Feldzugriff. Der Name wird als "modul.name" weitergereicht;
    /// `modules.rs` loest ihn beim Zusammenfuehren auf.
    pub(crate) fn qualify(&mut self, name: String, sp: Span) -> (String, Span) {
        if !self.modules.contains(&name) || !self.at(&TokKind::Dot) {
            return (name, sp);
        }
        let member = match self.toks.get(self.pos + 1).map(|t| t.kind.clone()) {
            Some(TokKind::Ident(m)) => m,
            _ => return (name, sp),
        };
        self.bump(); // '.'
        let msp = self.bump(); // name
        (format!("{}.{}", name, member), Parser::join(sp, msp))
    }

    pub(crate) fn mk(&mut self, span: Span, kind: ExprKind) -> Expr {
        let id = self.next_id;
        self.next_id += 1;
        Expr { id, span, kind }
    }

    /// Platzhalter fuer einen kaputten Ausdruck (Fehler ist bereits gemeldet).
    fn broken_expr(&mut self, span: Span) -> Expr {
        self.mk(span, ExprKind::Int(0))
    }

    pub(crate) fn join(a: Span, b: Span) -> Span {
        if a.line == b.line && b.col + b.len > a.col {
            Span::new(a.line, a.col, b.col + b.len - a.col)
        } else {
            a
        }
    }

    pub(crate) fn too_deep(&mut self) -> bool {
        if self.depth < MAX_DEPTH {
            return false;
        }
        if !self.recovering {
            self.error_here(format!(
                "zu tief verschachtelt (mehr als {} ebenen)",
                MAX_DEPTH
            ));
        }
        // Fortschritt erzwingen, damit keine Endlosschleife entsteht.
        self.bump();
        true
    }

    // ------------------------------------------------------- Fehlerwiederherstellung

    /// Bis zum Ende der Anweisung vorruecken: ';' schlucken, vor '}' oder einem
    /// Anweisungsschluesselwort am Zeilenanfang stehenbleiben.
    fn sync_stmt(&mut self) {
        loop {
            match self.kind() {
                TokKind::Eof | TokKind::RBrace => return,
                TokKind::Semi => {
                    self.bump();
                    return;
                }
                k if starts_stmt(k) && self.at_line_start() => return,
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// Bis zum naechsten Element auf oberster Ebene vorruecken.
    pub(crate) fn sync_item(&mut self) {
        while !self.at_eof() && !starts_item(self.kind()) {
            self.bump();
        }
    }

    // ------------------------------------------------------------------ Typen

    /// Siehe `nicht_umgesetzter_typ` am Dateiende.
    pub(crate) fn parse_type(&mut self) -> Option<TypeExpr> {
        if self.too_deep() {
            return None;
        }
        self.depth += 1;
        let t = self.parse_type_inner();
        self.depth -= 1;
        t
    }

    fn parse_type_inner(&mut self) -> Option<TypeExpr> {
        match self.kind().clone() {
            TokKind::Star => {
                let start = self.bump();
                let mutable = self.eat(&TokKind::KwMut);
                let inner = self.parse_type()?;
                let span = Parser::join(start, inner.span());
                Some(TypeExpr::Ptr { mutable, inner: Box::new(inner), span })
            }
            TokKind::LBracket => {
                let start = self.bump();
                let elem = self.parse_type()?;
                if !self.expect(TokKind::Semi, "nach dem elementtyp eines arraytyps") {
                    return None;
                }
                let len = match self.kind().clone() {
                    TokKind::Int(v) if v >= 0 => {
                        self.bump();
                        v as u64
                    }
                    _ => {
                        self.error_here(format!(
                            "erwartet eine ganzzahlige arraylaenge, gefunden '{}'",
                            self.kind().text()
                        ));
                        return None;
                    }
                };
                let end = self.span();
                if !self.expect(TokKind::RBracket, "nach der arraylaenge") {
                    return None;
                }
                Some(TypeExpr::Array {
                    elem: Box::new(elem),
                    len,
                    span: Parser::join(start, end),
                })
            }
            TokKind::Ident(name) => {
                let sp = self.bump();
                // Integration: Typkonstruktoren, die die SPEC beschreibt, die
                // Stufe 0 aber NICHT umsetzt, melden hier einen klaren Fehler
                // statt eines ratlosen Syntaxfehlers (SPEC §14 "Nicht enthalten").
                // HOOK gc: `Gc[C]` und `GcWeak[C]` (gc.rs)
                if let Some(t) = crate::gc::hook_type(self, &name, sp) {
                    return Some(t);
                }
                if self.kind() == &TokKind::LBracket
                    && !crate::sema_generic::is_generic_struct(&name)
                {
                    if let Some(grund) = nicht_umgesetzter_typ(&name) {
                        self.dg.error_note(
                            sp,
                            format!("'{}[T]' ist in Stufe 0 nicht umgesetzt", name),
                            grund,
                        );
                        self.recovering = true;
                        return None;
                    }
                }
                // HOOK fehlerunionen: Fehlerunion `E!T` (errors.rs)
                if let Some(t) = crate::errors::hook_type(self, &name, sp) {
                    return Some(t);
                }
                // HOOK types: generischer Typ `Vec[i32]` (sema_generic.rs)
                if let Some(t) = crate::sema_generic::hook_generic_type(self, &name, sp) {
                    return Some(t);
                }
                let (name, sp) = self.qualify(name, sp);
                Some(TypeExpr::Named(name, sp))
            }
            other => {
                if !self.recovering {
                    self.error_here(format!("erwartet einen typ, gefunden '{}'", other.text()));
                }
                None
            }
        }
    }

    // ------------------------------------------------------------- Ausdruecke

    pub(crate) fn expr(&mut self) -> Expr {
        if self.too_deep() {
            let sp = self.span();
            return self.broken_expr(sp);
        }
        self.depth += 1;
        let e = self.or_expr();
        // HOOK fehlerunionen: `ausdruck catch ersatzwert` (errors.rs)
        let e = crate::errors::hook_catch(self, e);
        self.depth -= 1;
        e
    }

    /// Ausdruck ohne Struct-Literal auf oberster Ebene (Bedingungen).
    fn cond_expr(&mut self) -> Expr {
        let saved = self.no_struct_lit;
        self.no_struct_lit = true;
        let e = self.expr();
        self.no_struct_lit = saved;
        e
    }

    /// Ausdruck in Klammern/Argumenten: Struct-Literale sind dort erlaubt.
    pub(crate) fn nested_expr(&mut self) -> Expr {
        let saved = self.no_struct_lit;
        self.no_struct_lit = false;
        self.paren_depth += 1;
        let e = self.expr();
        self.paren_depth -= 1;
        self.no_struct_lit = saved;
        e
    }

    pub(crate) fn or_expr(&mut self) -> Expr {
        let mut lhs = self.and_expr();
        while self.at(&TokKind::OrOr) && self.cont() {
            self.bump();
            let rhs = self.and_expr();
            let sp = Parser::join(lhs.span, rhs.span);
            lhs = self.mk(sp, ExprKind::Binary(BinOp::LOr, Box::new(lhs), Box::new(rhs)));
        }
        lhs
    }

    fn and_expr(&mut self) -> Expr {
        let mut lhs = self.cmp_expr();
        while self.at(&TokKind::AndAnd) && self.cont() {
            self.bump();
            let rhs = self.cmp_expr();
            let sp = Parser::join(lhs.span, rhs.span);
            lhs = self.mk(sp, ExprKind::Binary(BinOp::LAnd, Box::new(lhs), Box::new(rhs)));
        }
        lhs
    }

    fn cmp_op(k: &TokKind) -> Option<BinOp> {
        Some(match k {
            TokKind::EqEq => BinOp::Eq,
            TokKind::NotEq => BinOp::Ne,
            TokKind::Lt => BinOp::Lt,
            TokKind::Le => BinOp::Le,
            TokKind::Gt => BinOp::Gt,
            TokKind::Ge => BinOp::Ge,
            _ => return None,
        })
    }

    fn cmp_expr(&mut self) -> Expr {
        let lhs = self.add_expr();
        let op = match Parser::cmp_op(self.kind()) {
            Some(op) if self.cont() => op,
            _ => return lhs,
        };
        self.bump();
        let rhs = self.add_expr();
        let sp = Parser::join(lhs.span, rhs.span);
        let mut res = self.mk(sp, ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)));
        // Vergleiche sind nicht assoziativ (SPEC §10.1: genau ein Vergleich).
        while Parser::cmp_op(self.kind()).is_some() && self.cont() {
            let op2 = match Parser::cmp_op(self.kind()) {
                Some(o) => o,
                None => break,
            };
            self.error_here(format!(
                "vergleiche sind nicht verkettbar, setze klammern um den ersten vergleich"
            ));
            self.bump();
            let rhs2 = self.add_expr();
            let sp2 = Parser::join(res.span, rhs2.span);
            res = self.mk(sp2, ExprKind::Binary(op2, Box::new(res), Box::new(rhs2)));
        }
        res
    }

    fn add_op(k: &TokKind) -> Option<BinOp> {
        Some(match k {
            TokKind::Plus => BinOp::Add,
            TokKind::Minus => BinOp::Sub,
            TokKind::Pipe => BinOp::Or,
            TokKind::Caret => BinOp::Xor,
            _ => return None,
        })
    }

    fn add_expr(&mut self) -> Expr {
        let mut lhs = self.mul_expr();
        while let Some(op) = Parser::add_op(self.kind()).filter(|_| self.cont()) {
            self.bump();
            let rhs = self.mul_expr();
            let sp = Parser::join(lhs.span, rhs.span);
            lhs = self.mk(sp, ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)));
        }
        lhs
    }

    fn mul_op(k: &TokKind) -> Option<BinOp> {
        Some(match k {
            TokKind::Star => BinOp::Mul,
            TokKind::Slash => BinOp::Div,
            TokKind::Percent => BinOp::Rem,
            TokKind::Amp => BinOp::And,
            TokKind::Shl => BinOp::Shl,
            TokKind::Shr => BinOp::Shr,
            _ => return None,
        })
    }

    fn mul_expr(&mut self) -> Expr {
        let mut lhs = self.unary();
        while let Some(op) = Parser::mul_op(self.kind()).filter(|_| self.cont()) {
            self.bump();
            let rhs = self.unary();
            let sp = Parser::join(lhs.span, rhs.span);
            lhs = self.mk(sp, ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)));
        }
        lhs
    }

    pub(crate) fn unary(&mut self) -> Expr {
        let op = match self.kind() {
            TokKind::Minus => Some(UnOp::Neg),
            TokKind::Not => Some(UnOp::Not),
            TokKind::Amp => Some(UnOp::AddrOf),
            TokKind::Star => Some(UnOp::Deref),
            _ => None,
        };
        match op {
            None => self.postfix(),
            Some(op) => {
                if self.too_deep() {
                    let sp = self.span();
                    return self.broken_expr(sp);
                }
                let start = self.bump();
                self.depth += 1;
                let inner = self.unary();
                self.depth -= 1;
                let sp = Parser::join(start, inner.span);
                self.mk(sp, ExprKind::Unary(op, Box::new(inner)))
            }
        }
    }

    fn postfix(&mut self) -> Expr {
        let mut e = self.primary();
        loop {
            if !self.cont() {
                return e;
            }
            match self.kind() {
                TokKind::Dot => {
                    self.bump();
                    // HOOK gc: gepruefte Abwaertsumwandlung `x.as?[C]` (gc.rs)
                    if let Some(g) = crate::gc::hook_postfix(self, &e) {
                        e = g;
                        continue;
                    }
                    match self.ident("nach '.' beim feldzugriff") {
                        Some((name, sp)) => {
                            let full = Parser::join(e.span, sp);
                            e = self.mk(full, ExprKind::Field(Box::new(e), name, sp));
                        }
                        None => return e,
                    }
                }
                TokKind::LBracket => {
                    // HOOK types: generischer Aufruf `foo[i32](..)` (sema_generic.rs)
                    if let Some(g) = crate::sema_generic::hook_generic_call(self, &e) {
                        e = g;
                        continue;
                    }
                    let start = self.bump();
                    let idx = self.nested_expr();
                    let end = self.span();
                    if !self.close(TokKind::RBracket, "nach dem index") {
                        let sp = Parser::join(e.span, start);
                        return self.mk(sp, ExprKind::Index(Box::new(e), Box::new(idx)));
                    }
                    let sp = Parser::join(e.span, end);
                    e = self.mk(sp, ExprKind::Index(Box::new(e), Box::new(idx)));
                }
                TokKind::LParen => {
                    let lp = self.span();
                    let name = match &e.kind {
                        ExprKind::Ident(n) => n.clone(),
                        _ => {
                            self.error_here(
                                "nur direkte funktionsnamen koennen aufgerufen werden (zeigeraufrufe werden in Stufe 0 nicht unterstuetzt)",
                            );
                            return e;
                        }
                    };
                    self.bump();
                    let (args, end) = self.call_args("nach der argumentliste");
                    let sp = Parser::join(e.span, end);
                    let _ = lp;
                    e = self.mk(sp, ExprKind::Call(name, args, e.span));
                }
                TokKind::KwAs => {
                    self.bump();
                    match self.parse_type() {
                        Some(t) => {
                            let sp = Parser::join(e.span, t.span());
                            e = self.mk(sp, ExprKind::Cast(Box::new(e), t));
                        }
                        None => return e,
                    }
                }
                _ => return e,
            }
        }
    }

    /// Argumentliste nach bereits verbrauchtem '('. Liefert Argumente und die
    /// Position der schliessenden Klammer (bzw. des stoerenden Tokens).
    pub(crate) fn call_args(&mut self, ctx: &str) -> (Vec<Expr>, Span) {
        let mut args = Vec::new();
        loop {
            if self.at(&TokKind::RParen) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let a = self.nested_expr();
            args.push(a);
            if self.recovering {
                break;
            }
            if !self.eat(&TokKind::Comma) {
                break;
            }
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.span();
        self.close(TokKind::RParen, ctx);
        (args, end)
    }

    pub(crate) fn primary(&mut self) -> Expr {
        // HOOK fehlerunionen: `try ausdruck` (errors.rs)
        if let Some(e) = crate::errors::hook_primary(self) {
            return e;
        }
        // HOOK types: `Enum::Variante(..)` und `Vec[i32]{..}` (sema_match.rs)
        if let Some(e) = crate::sema_match::hook_primary(self) {
            return e;
        }
        // HOOK gc: `gc C{…}`, `gc_null[C]()`, `weak_null[C]()` (gc.rs)
        if let Some(e) = crate::gc::hook_primary(self) {
            return e;
        }
        match self.kind().clone() {
            TokKind::Int(v) => {
                let sp = self.bump();
                self.mk(sp, ExprKind::Int(v))
            }
            TokKind::Float(bits) => {
                let sp = self.bump();
                self.mk(sp, ExprKind::Float(bits))
            }
            // ZEICHENKETTENLITERAL -> Array-Literal.
            //
            // `"abc"` wird zu `[97, 98, 99]`, `u"abc"` zu den UTF-16-
            // Codeeinheiten. Damit ist der Typ `[u8; N]` bzw. `[u16; N]`, und
            // alles Weitere — Typpruefung, Lowering, Codegenerierung — ist
            // schon da. Der Preis ist ehrlich benannt (SPEC §14.1.str, S8):
            // die Daten landen als Folge einzelner Speicherbefehle im Rahmen,
            // nicht in `.rodata`. Fuer den Quelltext ist der Gewinn trotzdem
            // gross: `var m: [u8; 12] = "firn-gc: …"` statt einer von Hand
            // ausgerechneten Oktettliste.
            TokKind::Str(_, val) => {
                let sp = self.bump();
                let mut elems: Vec<Expr> = Vec::new();
                match val {
                    crate::strings::LitValue::Octets(v) => {
                        for b in v {
                            elems.push(self.mk(sp, ExprKind::Int(b as i128)));
                        }
                    }
                    crate::strings::LitValue::Units(v) => {
                        for u in v {
                            elems.push(self.mk(sp, ExprKind::Int(u as i128)));
                        }
                    }
                }
                if elems.is_empty() {
                    self.dg.error_note(
                        sp,
                        "leeres zeichenkettenliteral".to_string(),
                        "ein array braucht mindestens ein element; schreibe ein feld der gewuenschten laenge, z. B. '[0 as u8; 8]'",
                    );
                    return self.broken_expr(sp);
                }
                self.mk(sp, ExprKind::ArrayLit(elems))
            }
            TokKind::KwTrue => {
                let sp = self.bump();
                self.mk(sp, ExprKind::Bool(true))
            }
            TokKind::KwFalse => {
                let sp = self.bump();
                self.mk(sp, ExprKind::Bool(false))
            }
            TokKind::LParen => {
                self.bump();
                let e = self.nested_expr();
                self.close(TokKind::RParen, "nach dem geklammerten ausdruck");
                e
            }
            TokKind::LBracket => {
                let start = self.bump();
                let mut elems = Vec::new();
                // `[wert; N]` — Wiederholungsliteral
                if !self.at(&TokKind::RBracket) && !self.at_eof() {
                    let first = self.nested_expr();
                    if self.at(&TokKind::Semi) && !self.recovering {
                        self.bump();
                        let count = self.nested_expr();
                        let end = self.span();
                        self.close(TokKind::RBracket, "nach der laenge des wiederholungsliterals");
                        let sp = Parser::join(start, end);
                        return self
                            .mk(sp, ExprKind::ArrayRepeat(Box::new(first), Box::new(count)));
                    }
                    let done = self.recovering || !self.eat(&TokKind::Comma);
                    elems.push(first);
                    if done {
                        let end = self.span();
                        self.close(TokKind::RBracket, "nach den elementen des arrayliterals");
                        let sp = Parser::join(start, end);
                        return self.mk(sp, ExprKind::ArrayLit(elems));
                    }
                }
                loop {
                    if self.at(&TokKind::RBracket) || self.at_eof() {
                        break;
                    }
                    let before = self.pos;
                    let e = self.nested_expr();
                    elems.push(e);
                    if self.recovering {
                        break;
                    }
                    if !self.eat(&TokKind::Comma) {
                        break;
                    }
                    if self.pos == before {
                        self.bump();
                    }
                }
                let end = self.span();
                self.close(TokKind::RBracket, "nach den elementen des arrayliterals");
                let sp = Parser::join(start, end);
                self.mk(sp, ExprKind::ArrayLit(elems))
            }
            TokKind::KwSyscall => {
                let start = self.bump();
                self.expect(TokKind::LParen, "nach 'syscall'");
                let (args, end) = self.call_args("nach den argumenten von 'syscall'");
                let sp = Parser::join(start, end);
                self.mk(sp, ExprKind::Syscall(args))
            }
            TokKind::Ident(name) => {
                let sp = self.bump();
                let (name, sp) = self.qualify(name, sp);
                if self.at(&TokKind::LBrace) && !self.no_struct_lit {
                    return self.struct_lit(name, sp);
                }
                self.mk(sp, ExprKind::Ident(name))
            }
            other => {
                if !self.recovering {
                    self.error_here(format!(
                        "erwartet einen ausdruck, gefunden '{}'",
                        other.text()
                    ));
                }
                let sp = self.span();
                self.broken_expr(sp)
            }
        }
    }

    /// `Name{ feld: wert, ... }` — '{' steht noch an.
    pub(crate) fn struct_lit(&mut self, name: String, name_span: Span) -> Expr {
        self.bump(); // '{'
        let mut fields = Vec::new();
        loop {
            if self.at(&TokKind::RBrace) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let (fname, fspan) = match self.ident("fuer ein feld im struct-literal") {
                Some(x) => x,
                None => break,
            };
            if !self.expect(TokKind::Colon, "nach dem feldnamen im struct-literal") {
                break;
            }
            let val = self.nested_expr();
            fields.push((fname, val, fspan));
            if self.recovering {
                break;
            }
            if !self.eat(&TokKind::Comma) {
                break;
            }
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.span();
        self.close(TokKind::RBrace, "am ende des struct-literals");
        let sp = Parser::join(name_span, end);
        self.mk(sp, ExprKind::StructLit(name, fields, name_span))
    }

    // ------------------------------------------------------------ Anweisungen

    pub(crate) fn block(&mut self, ctx: &str) -> Block {
        let start = self.span();
        if self.too_deep() {
            return Block { stmts: Vec::new(), span: start };
        }
        if !self.expect(TokKind::LBrace, ctx) {
            self.recovering = false;
            return Block { stmts: Vec::new(), span: start };
        }
        self.depth += 1;
        let mut stmts = Vec::new();
        loop {
            if self.at(&TokKind::RBrace) {
                break;
            }
            if self.at_eof() {
                if !self.recovering {
                    self.error_here("erwartet '}' am ende des blocks, gefunden 'Dateiende'");
                }
                break;
            }
            if self.eat(&TokKind::Semi) {
                continue;
            }
            if self.dg.is_full() {
                // Fehlerlawine vermeiden: Rest des Blocks ueberspringen.
                while !self.at(&TokKind::RBrace) && !self.at_eof() {
                    self.bump();
                }
                break;
            }
            let before = self.pos;
            let s = self.stmt();
            stmts.push(s);
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.span();
        self.eat(&TokKind::RBrace);
        self.depth -= 1;
        Block { stmts, span: Parser::join(start, end) }
    }

    /// Anweisungsende: ';' oder Zeilenwechsel oder '}'.
    pub(crate) fn end_stmt(&mut self) {
        if self.recovering {
            self.recovering = false;
            self.sync_stmt();
            return;
        }
        let mut got = false;
        while self.at(&TokKind::Semi) {
            self.bump();
            got = true;
        }
        if got || self.at(&TokKind::RBrace) || self.at_eof() || self.at_line_start() {
            return;
        }
        self.error_here(format!(
            "erwartet ';' oder ein zeilenende nach der anweisung, gefunden '{}'",
            self.kind().text()
        ));
        self.recovering = false;
        self.sync_stmt();
    }

    fn stmt(&mut self) -> Stmt {
        let start = self.span();
        if self.too_deep() {
            return Stmt::Error(start);
        }
        self.depth += 1;
        let s = self.stmt_inner(start);
        self.depth -= 1;
        s
    }

    /// `defer <anweisung>` — die Anweisung laeuft beim Verlassen des
    /// umschliessenden Blocks (SPEC §5.1). Erlaubt ist sowohl ein Block
    /// (`defer { … }`) als auch eine einzelne Anweisung (`defer close(fd)`).
    fn defer_stmt(&mut self, nur_fehler: bool) -> Stmt {
        let wort = if nur_fehler { "errdefer" } else { "defer" };
        let start = self.bump();
        if self.at_eof() {
            self.error_here(format!("nach '{}' fehlt die aufgeschobene anweisung", wort));
            return Stmt::Error(start);
        }
        let inner = self.stmt();
        let sp = Parser::join(start, inner.span());
        Stmt::Defer(Box::new(inner), nur_fehler, sp)
    }

    fn stmt_inner(&mut self, start: Span) -> Stmt {
        // HOOK types: `match`-Anweisung (sema_match.rs)
        if let Some(s) = crate::sema_match::hook_stmt(self) {
            return s;
        }
        match self.kind().clone() {
            TokKind::KwLet | TokKind::KwVar => self.let_stmt(),
            TokKind::KwIf => self.if_stmt(),
            TokKind::KwWhile => self.while_stmt(),
            TokKind::KwFor => self.for_stmt(),
            TokKind::KwBreak | TokKind::KwContinue => self.jump_stmt(),
            TokKind::KwDefer => self.defer_stmt(false),
            TokKind::KwErrDefer => self.defer_stmt(true),
            TokKind::KwReturn => self.return_stmt(),
            TokKind::LBrace => Stmt::Block(self.block("am anfang eines blocks")),
            TokKind::KwFn | TokKind::KwStruct | TokKind::KwConst | TokKind::KwExtern => {
                self.error_here(format!(
                    "'{}' ist nur auf oberster ebene erlaubt, nicht in einem funktionsrumpf",
                    self.kind().text()
                ));
                self.recovering = false;
                self.bump();
                self.sync_stmt();
                Stmt::Error(start)
            }
            _ => {
                let e = self.expr();
                if self.at(&TokKind::Assign) && !self.recovering {
                    self.bump();
                    let v = self.expr();
                    let sp = Parser::join(start, v.span);
                    self.end_stmt();
                    Stmt::Assign { target: e, value: v, span: sp }
                } else {
                    let broken = self.recovering;
                    self.end_stmt();
                    if broken {
                        Stmt::Error(start)
                    } else {
                        Stmt::Expr(e)
                    }
                }
            }
        }
    }

    fn let_stmt(&mut self) -> Stmt {
        let start = self.span();
        let mutable = self.at(&TokKind::KwVar);
        let kw = self.kind().text();
        self.bump();
        let name = match self.ident(&format!("nach '{}'", kw)) {
            Some((n, _)) => n,
            None => {
                self.recovering = false;
                self.sync_stmt();
                return Stmt::Error(start);
            }
        };
        let ty = if self.eat(&TokKind::Colon) {
            match self.parse_type() {
                Some(t) => Some(t),
                None => {
                    self.recovering = false;
                    self.sync_stmt();
                    return Stmt::Error(start);
                }
            }
        } else {
            None
        };
        if !self.expect(TokKind::Assign, &format!("nach dem namen in einer '{}'-anweisung", kw)) {
            self.recovering = false;
            self.sync_stmt();
            return Stmt::Error(start);
        }
        let init = self.expr();
        let broken = self.recovering;
        let sp = Parser::join(start, init.span);
        self.end_stmt();
        if broken {
            Stmt::Error(start)
        } else {
            Stmt::Let { name, mutable, ty, init, span: sp }
        }
    }

    fn if_stmt(&mut self) -> Stmt {
        let start = self.bump(); // 'if'
        let cond = self.cond_expr();
        if self.recovering {
            self.recovering = false;
            if !self.at(&TokKind::LBrace) {
                self.sync_stmt();
                return Stmt::Error(start);
            }
        }
        let then = self.block("nach der bedingung von 'if'");
        let els = if self.at(&TokKind::KwElse) {
            self.bump();
            if self.at(&TokKind::KwIf) {
                Some(Box::new(self.stmt()))
            } else {
                Some(Box::new(Stmt::Block(self.block("nach 'else'"))))
            }
        } else {
            None
        };
        Stmt::If { cond, then, els, span: start }
    }

    fn while_stmt(&mut self) -> Stmt {
        let start = self.bump(); // 'while'
        let cond = self.cond_expr();
        if self.recovering {
            self.recovering = false;
            if !self.at(&TokKind::LBrace) {
                self.sync_stmt();
                return Stmt::Error(start);
            }
        }
        self.loop_depth += 1;
        let body = self.block("nach der bedingung von 'while'");
        self.loop_depth -= 1;
        Stmt::While { cond, body, span: start }
    }

    /// `for name in start..end { }` — halboffener, aufsteigender Bereich.
    fn for_stmt(&mut self) -> Stmt {
        let start = self.bump(); // 'for'
        let (name, name_span) = match self.ident("nach 'for'") {
            Some(x) => x,
            None => {
                self.recovering = false;
                self.sync_stmt();
                return Stmt::Error(start);
            }
        };
        if !self.expect(TokKind::KwIn, "nach dem schleifennamen") {
            self.recovering = false;
            self.sync_stmt();
            return Stmt::Error(start);
        }
        let from = self.cond_expr();
        if !self.expect(TokKind::DotDot, "zwischen anfang und ende des bereichs") {
            self.recovering = false;
            self.sync_stmt();
            return Stmt::Error(start);
        }
        let to = self.cond_expr();
        if self.recovering {
            self.recovering = false;
            if !self.at(&TokKind::LBrace) {
                self.sync_stmt();
                return Stmt::Error(start);
            }
        }
        self.loop_depth += 1;
        let body = self.block("nach dem bereich von 'for'");
        self.loop_depth -= 1;
        Stmt::For { name, start: from, end: to, body, name_span, span: start }
    }

    /// `break` / `continue`
    fn jump_stmt(&mut self) -> Stmt {
        let is_break = self.at(&TokKind::KwBreak);
        let word = if is_break { "break" } else { "continue" };
        let sp = self.bump();
        if self.loop_depth == 0 {
            self.dg
                .error(sp, format!("'{}' steht ausserhalb einer schleife", word));
            self.recovering = true;
        }
        self.end_stmt();
        if is_break {
            Stmt::Break(sp)
        } else {
            Stmt::Continue(sp)
        }
    }

    fn return_stmt(&mut self) -> Stmt {
        let start = self.bump(); // 'return'
        let has_value = !(self.at(&TokKind::Semi)
            || self.at(&TokKind::RBrace)
            || self.at_eof()
            || self.at_line_start());
        let value = if has_value {
            let e = self.expr();
            Some(e)
        } else {
            None
        };
        let broken = self.recovering;
        let sp = match &value {
            Some(e) => Parser::join(start, e.span),
            None => start,
        };
        self.end_stmt();
        if broken {
            Stmt::Error(start)
        } else {
            Stmt::Return { value, span: sp }
        }
    }

    // ---------------------------------------------------------------- Elemente

    pub(crate) fn params(&mut self) -> Vec<Param> {
        let mut out = Vec::new();
        loop {
            if self.at(&TokKind::RParen) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let (name, sp) = match self.ident("fuer einen parameter") {
                Some(x) => x,
                None => break,
            };
            if !self.expect(TokKind::Colon, "nach dem parameternamen") {
                break;
            }
            let ty = match self.parse_type() {
                Some(t) => t,
                None => break,
            };
            out.push(Param { name, ty, span: sp });
            if !self.eat(&TokKind::Comma) {
                break;
            }
            if self.pos == before {
                self.bump();
            }
        }
        out
    }

    fn fn_decl(&mut self, prog: &mut Program) {
        let start = self.span();
        let is_extern = self.at(&TokKind::KwExtern);
        if is_extern {
            self.bump();
            self.dg.error(
                start,
                "'extern fn' wird in Stufe 0 nicht unterstuetzt",
            );
        }
        if !self.expect(TokKind::KwFn, "am anfang einer funktionsdeklaration") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        let name = match self.ident("nach 'fn'") {
            Some((n, _)) => n,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        if !self.expect(TokKind::LParen, "nach dem funktionsnamen") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        let params = self.params();
        self.close(TokKind::RParen, "nach der parameterliste");
        self.recovering = false;
        let ret = if self.eat(&TokKind::Arrow) {
            match self.parse_type() {
                Some(t) => Some(t),
                None => {
                    self.recovering = false;
                    self.sync_item();
                    return;
                }
            }
        } else {
            None
        };
        if !self.at(&TokKind::LBrace) {
            self.error_here(format!(
                "erwartet '{{' am anfang des funktionsrumpfes, gefunden '{}'",
                self.kind().text()
            ));
            self.recovering = false;
            self.sync_item();
            return;
        }
        let body = self.block("am anfang des funktionsrumpfes");
        self.recovering = false;
        if !is_extern {
            let attrs = std::mem::take(&mut self.pending_attrs);
            prog.funcs.push(FnDecl { name, params, ret, body, span: start, attrs });
        }
    }

    fn struct_decl(&mut self, prog: &mut Program) {
        let start = self.bump(); // 'struct'
        let name = match self.ident("nach 'struct'") {
            Some((n, _)) => n,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        if !self.expect(TokKind::LBrace, "nach dem structnamen") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        let mut fields = Vec::new();
        loop {
            if self.at(&TokKind::RBrace) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let (fname, fspan) = match self.ident("fuer ein structfeld") {
                Some(x) => x,
                None => break,
            };
            if !self.expect(TokKind::Colon, "nach dem feldnamen") {
                break;
            }
            let ty = match self.parse_type() {
                Some(t) => t,
                None => break,
            };
            fields.push((fname, ty, fspan));
            self.eat(&TokKind::Comma);
            if self.pos == before {
                self.bump();
            }
        }
        if !self.close(TokKind::RBrace, "am ende der structdeklaration") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        self.recovering = false;
        let attrs = std::mem::take(&mut self.pending_attrs);
        prog.structs.push(StructDecl { name, fields, span: start, attrs });
    }

    fn const_decl(&mut self, prog: &mut Program) {
        let start = self.bump(); // 'const'
        let name = match self.ident("nach 'const'") {
            Some((n, _)) => n,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        if !self.expect(TokKind::Colon, "nach dem namen einer konstanten") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        let ty = match self.parse_type() {
            Some(t) => t,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        if !self.expect(TokKind::Assign, "nach dem typ einer konstanten") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        let value = self.expr();
        let broken = self.recovering;
        self.end_stmt();
        self.recovering = false;
        if !broken {
            prog.consts.push(ConstDecl { name, ty, value, span: start });
        }
    }

    /// `import pfad.modul`
    fn import_decl(&mut self, prog: &mut Program) {
        let start = self.bump(); // 'import'
        let mut path: Vec<String> = Vec::new();
        loop {
            match self.ident("in einem modulpfad nach 'import'") {
                Some((n, _)) => path.push(n),
                None => {
                    self.recovering = false;
                    self.sync_item();
                    return;
                }
            }
            if !self.eat(&TokKind::Dot) {
                break;
            }
        }
        let alias = match path.last() {
            Some(a) => a.clone(),
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        if prog.imports.iter().any(|i| i.alias == alias) {
            self.dg
                .error(start, format!("modul '{}' wird mehrfach eingebunden", alias));
        }
        self.modules.insert(alias.clone());
        prog.imports.push(ImportDecl { path, alias, span: start });
        self.end_stmt();
        self.recovering = false;
    }

    /// `export { a, b }`
    fn export_decl(&mut self, prog: &mut Program) {
        self.bump(); // 'export'
        if !self.expect(TokKind::LBrace, "nach 'export'") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        loop {
            if self.at(&TokKind::RBrace) || self.at_eof() {
                break;
            }
            let before = self.pos;
            match self.ident("in der export-liste") {
                Some((n, sp)) => prog.exports.push((n, sp)),
                None => break,
            }
            if !self.eat(&TokKind::Comma) {
                break;
            }
            if self.pos == before {
                self.bump();
            }
        }
        self.close(TokKind::RBrace, "am ende der export-liste");
        self.recovering = false;
    }

    fn profile_decl(&mut self, prog: &mut Program) {
        let start = self.bump(); // 'profile'
        match self.ident("nach 'profile'") {
            Some((n, _)) => {
                if prog.profile.is_some() {
                    self.dg.error(start, "mehr als eine 'profile'-deklaration");
                } else {
                    prog.profile = Some((n, start));
                }
                self.end_stmt();
            }
            None => {
                self.recovering = false;
                self.sync_item();
            }
        }
        self.recovering = false;
    }

    /// `#[name]` oder `#[name(arg, ...)]`, beliebig oft hintereinander.
    ///
    /// Der Parser prueft hier NUR die Form. Ob es den Namen gibt, wohin er
    /// gehoert und ob er in Stufe 0 etwas tut, entscheidet `sema.rs` anhand
    /// des Registers in `attrs.rs` — mit Zeile, Spalte und Vorschlag.
    fn attributes(&mut self) -> Vec<Attr> {
        let mut out = Vec::new();
        while self.at(&TokKind::Hash) {
            let start = self.bump(); // '#'
            if !self.expect(TokKind::LBracket, "nach '#'") {
                self.recovering = false;
                self.sync_item();
                return out;
            }
            let name = match self.ident("als attributname nach '#['") {
                Some((n, _)) => n,
                None => {
                    self.recovering = false;
                    self.sync_item();
                    return out;
                }
            };
            let mut args = Vec::new();
            if self.eat(&TokKind::LParen) {
                loop {
                    if self.at(&TokKind::RParen) || self.at_eof() {
                        break;
                    }
                    match self.kind() {
                        TokKind::Ident(t) => {
                            args.push(t.clone());
                            self.bump();
                        }
                        TokKind::Int(v) => {
                            args.push(v.to_string());
                            self.bump();
                        }
                        other => {
                            let msg = format!(
                                "erwartet einen namen oder eine zahl als attributargument, gefunden '{}'",
                                other.text()
                            );
                            self.error_here(msg);
                            self.recovering = false;
                            self.sync_item();
                            return out;
                        }
                    }
                    if !self.eat(&TokKind::Comma) {
                        break;
                    }
                }
                if !self.expect(TokKind::RParen, "nach den attributargumenten") {
                    self.recovering = false;
                    self.sync_item();
                    return out;
                }
            }
            if !self.expect(TokKind::RBracket, "nach dem attribut") {
                self.recovering = false;
                self.sync_item();
                return out;
            }
            out.push(Attr { name, args, span: start });
        }
        out
    }

    fn program(&mut self) -> Program {
        let mut prog = Program::default();
        loop {
            while self.eat(&TokKind::Semi) {}
            if self.at_eof() {
                break;
            }
            if self.dg.is_full() {
                break;
            }
            let before = self.pos;
            // Attribute gehoeren zur naechsten Deklaration (attrs.rs).
            if self.at(&TokKind::Hash) {
                self.pending_attrs = self.attributes();
                while self.eat(&TokKind::Semi) {}
                if self.at_eof() {
                    if !self.pending_attrs.is_empty() {
                        let sp = self.pending_attrs[0].span;
                        self.dg.error(sp, "attribut ohne deklaration dahinter".to_string());
                    }
                    break;
                }
                if self.pos == before {
                    self.bump();
                }
                continue;
            }
            // HOOK fehlerunionen: `error`-Deklaration (errors.rs)
            if crate::errors::hook_item(self) {
                if self.pos == before {
                    self.bump();
                }
                continue;
            }
            // HOOK gc: `gc class Name { … }` (gc.rs, SPEC 3.5.1)
            if crate::gc::hook_item(self) {
                if self.pos == before {
                    self.bump();
                }
                continue;
            }
            // HOOK types: enum-Deklaration und generische Vorlagen (sema_match.rs)
            if crate::sema_match::hook_item(self) {
                if self.pos == before {
                    self.bump();
                }
                continue;
            }
            match self.kind() {
                TokKind::KwFn | TokKind::KwExtern => self.fn_decl(&mut prog),
                TokKind::KwStruct => self.struct_decl(&mut prog),
                TokKind::KwConst => self.const_decl(&mut prog),
                TokKind::KwProfile => self.profile_decl(&mut prog),
                TokKind::KwImport => self.import_decl(&mut prog),
                TokKind::KwExport => self.export_decl(&mut prog),
                other => {
                    let msg = format!(
                        "erwartet 'fn', 'struct', 'const', 'import', 'export' oder 'profile' auf oberster ebene, gefunden '{}'",
                        other.text()
                    );
                    self.error_here(msg);
                    self.recovering = false;
                    self.sync_item();
                }
            }
            if self.pos == before {
                self.bump();
            }
        }
        prog.expr_count = self.next_id;
        prog
    }
}

pub fn parse(toks: &[Token], dg: &mut Diags) -> Program {
    reset_hooks();
    parse_module(toks, dg, 0, 0)
}

/// Setzt die Registrierungen der Nachbarmodule fuer EINE Uebersetzung zurueck.
/// Bei mehreren Dateien ruft `modules.rs` das genau einmal auf — sonst
/// verloere jede Datei die Aufzaehlungen der vorherigen.
pub fn reset_hooks() {
    // HOOK types: Registrierungen dieser Uebersetzung zuruecksetzen (sema_match.rs)
    crate::sema_match::hook_reset();
    // HOOK fehlerunionen: dasselbe fuer Fehlermengen/Fehlerunionen (errors.rs)
    crate::errors::hook_reset();
    // HOOK gc: dasselbe fuer die gc-Klassen (gc.rs)
    crate::gc::hook_reset();
}

/// Wie `parse`, aber fuer eine Datei der Quelltextkarte: `file` ist ihre
/// Nummer, `base_id` die erste noch freie `ExprId`. `Program::expr_count` ist
/// danach die erste hinter dieser Datei freie Id (absolut) — `modules.rs`
/// reiht die Dateien so ohne Ueberschneidung aneinander.
pub fn parse_module(toks: &[Token], dg: &mut Diags, file: u32, base_id: u32) -> Program {
    crate::sema_generic::hook_prescan(toks);
    let mut p = Parser {
        toks,
        pos: 0,
        dg,
        next_id: base_id,
        depth: 0,
        recovering: false,
        no_struct_lit: false,
        paren_depth: 0,
        file,
        modules: HashSet::new(),
        loop_depth: 0,
        pending_attrs: Vec::new(),
    };
    p.program()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn parse_src(src: &str) -> (Program, usize, String) {
        let mut dg = Diags::new("test", src);
        let toks = lex(src, &mut dg);
        let prog = parse(&toks, &mut dg);
        let text = dg.render();
        (prog, dg.count(), text)
    }

    fn ok(src: &str) -> Program {
        let (p, n, t) = parse_src(src);
        assert_eq!(n, 0, "unerwartete fehler:\n{}", t);
        p
    }

    #[test]
    fn leeres_hauptprogramm() {
        let p = ok("fn main() -> i32 { return 0 }");
        assert_eq!(p.funcs.len(), 1);
        assert_eq!(p.funcs[0].name, "main");
        assert!(p.expr_count > 0);
    }

    #[test]
    fn expr_ids_sind_fortlaufend() {
        let p = ok("fn main() -> i32 { let a: i32 = 1 + 2 * 3\n return a }");
        // 1, 2, 3, 2*3, 1+..., a  => 6 Ausdruecke
        assert_eq!(p.expr_count, 6);
    }

    fn dump(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(v) => format!("{}", v),
            ExprKind::Float(bits) => format!("{}", f64::from_bits(*bits)),
            ExprKind::Bool(b) => format!("{}", b),
            ExprKind::Ident(n) => n.clone(),
            ExprKind::Unary(op, a) => format!(
                "({}{})",
                match op {
                    UnOp::Neg => "-",
                    UnOp::Not => "!",
                    UnOp::AddrOf => "&",
                    UnOp::Deref => "*",
                },
                dump(a)
            ),
            ExprKind::Binary(op, a, b) => format!("({} {} {})", dump(a), op.text(), dump(b)),
            ExprKind::Field(b, n, _) => format!("({}.{})", dump(b), n),
            ExprKind::Index(b, i) => format!("({}[{}])", dump(b), dump(i)),
            ExprKind::Call(n, a, _) => {
                let args: Vec<String> = a.iter().map(dump).collect();
                format!("{}({})", n, args.join(","))
            }
            ExprKind::Syscall(a) => {
                let args: Vec<String> = a.iter().map(dump).collect();
                format!("syscall({})", args.join(","))
            }
            ExprKind::Cast(b, _) => format!("({} as T)", dump(b)),
            ExprKind::StructLit(n, f, _) => {
                let fs: Vec<String> = f.iter().map(|(k, v, _)| format!("{}:{}", k, dump(v))).collect();
                format!("{}{{{}}}", n, fs.join(","))
            }
            ExprKind::ArrayLit(e) => {
                let es: Vec<String> = e.iter().map(dump).collect();
                format!("[{}]", es.join(","))
            }
            ExprKind::ArrayRepeat(v, n) => format!("[{}; {}]", dump(v), dump(n)),
        }
    }

    fn first_expr(src: &str) -> String {
        let p = ok(&format!("fn main() -> i32 {{ let t: i32 = {}\n return 0 }}", src));
        match &p.funcs[0].body.stmts[0] {
            Stmt::Let { init, .. } => dump(init),
            other => panic!("erwartet let, {:?}", other),
        }
    }

    #[test]
    fn praezedenz_nach_ebnf() {
        assert_eq!(first_expr("1 + 2 * 3"), "(1 + (2 * 3))");
        assert_eq!(first_expr("1 | 2 & 3"), "(1 | (2 & 3))");
        assert_eq!(first_expr("1 + 2 << 3"), "(1 + (2 << 3))");
        assert_eq!(first_expr("a && b || c && d"), "((a && b) || (c && d))");
        assert_eq!(first_expr("1 < 2 && 3 > 4"), "((1 < 2) && (3 > 4))");
        assert_eq!(first_expr("-a * !b"), "((-a) * (!b))");
        assert_eq!(first_expr("*p + 1"), "((*p) + 1)");
        assert_eq!(first_expr("&a"), "(&a)");
        assert_eq!(first_expr("a.b[1].c"), "(((a.b)[1]).c)");
        assert_eq!(first_expr("f(1, 2) + g()"), "(f(1,2) + g())");
        assert_eq!(first_expr("1 - 2 - 3"), "((1 - 2) - 3)");
        assert_eq!(first_expr("x as i32 + 1"), "((x as T) + 1)");
        assert_eq!(first_expr("(1 + 2) * 3"), "((1 + 2) * 3)");
    }

    #[test]
    fn semikolon_ist_optional() {
        let p = ok("fn main() -> i32 {\n let a: i32 = 1;;\n var b: i32 = 2\n b = a + b\n return b\n}");
        assert_eq!(p.funcs[0].body.stmts.len(), 4);
    }

    #[test]
    fn zeilenende_beendet_die_anweisung() {
        // `*p = ...` in der naechsten Zeile ist KEINE Multiplikation.
        let p = ok("fn main() -> i32 {\n var a: i32 = 1\n var p: *mut i32 = &a\n *p = 2\n return a\n}");
        match &p.funcs[0].body.stmts[2] {
            Stmt::Assign { target, .. } => match &target.kind {
                ExprKind::Unary(UnOp::Deref, _) => {}
                other => panic!("{:?}", other),
            },
            other => panic!("erwartet zuweisung, {:?}", other),
        }
        // Ein Operator am ZEILENENDE setzt den Ausdruck dagegen fort,
        // innerhalb von Klammern auch ein Zeilenumbruch.
        let p2 = ok("fn f(a: i32, b: i32) -> i32 { return a }\nfn main() -> i32 {\n let s: i32 = 1 +\n 2\n let t: i32 = f(1,\n 2)\n return s + t\n}");
        assert_eq!(p2.funcs[1].body.stmts.len(), 3);
        // `a` und `(b)` in zwei Zeilen sind zwei Anweisungen, kein Aufruf.
        let p3 = ok("fn main() -> i32 {\n let a: i32 = 1\n a\n (a)\n return a\n}");
        assert_eq!(p3.funcs[0].body.stmts.len(), 4);
    }

    #[test]
    fn struct_und_const_und_profile() {
        let p = ok("profile app\nstruct P { x: i32, y: i32 }\nconst M: i32 = 7\nfn main() -> i32 { let p: P = P{ x: 1, y: 2 }\n return p.x }");
        assert_eq!(p.structs.len(), 1);
        assert_eq!(p.structs[0].fields.len(), 2);
        assert_eq!(p.consts.len(), 1);
        assert_eq!(p.profile.as_ref().map(|x| x.0.clone()), Some("app".to_string()));
    }

    #[test]
    fn bedingung_ohne_struct_literal() {
        let p = ok("fn main() -> i32 { var x: i32 = 0\n while x < 3 { x = x + 1 }\n if x == 3 { return 0 } else { return 1 } }");
        assert_eq!(p.funcs[0].body.stmts.len(), 3);
    }

    #[test]
    fn typen_und_arrays() {
        let p = ok("fn f(p: *mut u8, a: [i32; 4]) -> *u8 { return p as *u8 }\nfn main() -> i32 { return 0 }");
        assert_eq!(p.funcs.len(), 2);
        assert_eq!(p.funcs[0].params.len(), 2);
    }

    #[test]
    fn else_if_kette() {
        let p = ok("fn main() -> i32 { if false { return 1 } else if true { return 2 } else { return 3 } }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::If { els: Some(b), .. } => match b.as_ref() {
                Stmt::If { .. } => {}
                other => panic!("erwartet else-if, {:?}", other),
            },
            other => panic!("erwartet if, {:?}", other),
        }
    }

    #[test]
    fn mehrere_fehler_werden_gemeldet() {
        let src = "fn main() -> i32 {\n    let x = add(1, 2 ;\n    let = 3\n    return 0\n}\n";
        let (_, n, text) = parse_src(src);
        assert!(n >= 2, "erwartet mehrere fehler, bekam {}:\n{}", n, text);
        assert!(text.contains("2:22"), "position fehlt:\n{}", text);
        assert!(text.contains("erwartet ')'"), "meldung fehlt:\n{}", text);
    }

    #[test]
    fn fehler_in_zwei_funktionen() {
        let src = "fn a() -> i32 { return ) }\nfn b() -> i32 { return * }\n";
        let (_, n, text) = parse_src(src);
        assert!(n >= 2, "{}", text);
    }

    #[test]
    fn kein_haenger_bei_abbruch() {
        for src in [
            "fn",
            "fn main(",
            "fn main() -> {",
            "fn main() -> i32 { let",
            "fn main() -> i32 { return 1 +",
            "struct",
            "struct P { x:",
            "const",
            "profile",
            "}",
            "fn main() -> i32 { if { } }",
            "fn main() -> i32 { a[ }",
            "fn main() -> i32 { P{ x: } }",
            "fn main() -> i32 { syscall( }",
        ] {
            let (_, n, _) = parse_src(src);
            assert!(n >= 1, "erwartet fehler fuer {:?}", src);
        }
    }

    #[test]
    fn tiefe_verschachtelung_bricht_sauber_ab() {
        let deep = format!("fn main() -> i32 {{ return {}1{} }}", "(".repeat(500), ")".repeat(500));
        let (_, n, _) = parse_src(&deep);
        assert!(n >= 1);
        let deep2 = format!("fn main() -> i32 {{ return {}1 }}", "-".repeat(500));
        let (_, n2, _) = parse_src(&deep2);
        assert!(n2 >= 1);
        let deep3 = format!(
            "fn main() -> i32 {{ {} return 0 {} }}",
            "{".repeat(400),
            "}".repeat(400)
        );
        let (_, n3, _) = parse_src(&deep3);
        assert!(n3 >= 1);
    }

    #[test]
    fn vergleich_ist_nicht_assoziativ() {
        let (_, n, text) = parse_src("fn main() -> i32 { let b: bool = 1 < 2 < 3\n return 0 }");
        assert!(n >= 1);
        assert!(text.contains("nicht verkettbar"), "{}", text);
    }

    #[test]
    fn extern_wird_abgelehnt() {
        let (p, n, text) = parse_src("extern fn write(fd: i32) -> i32 { return 0 }\nfn main() -> i32 { return 0 }");
        assert!(n >= 1);
        assert!(text.contains("Stufe 0"), "{}", text);
        assert_eq!(p.funcs.len(), 1);
    }

    #[test]
    fn syscall_wird_erkannt() {
        let p = ok("fn main() -> i32 { let r: i64 = syscall(1 as i64, 1 as i64, 0 as i64, 0 as i64)\n return 0 }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::Let { init, .. } => match &init.kind {
                ExprKind::Syscall(a) => assert_eq!(a.len(), 4),
                other => panic!("{:?}", other),
            },
            other => panic!("{:?}", other),
        }
    }
}

/// Typkonstruktoren, die `SPEC.md` beschreibt, die Stufe 0 aber nicht umsetzt.
/// Sie bekommen einen eigenen, klaren Fehler statt eines Syntaxfehlers —
/// `SPEC.md` §14 fuehrt sie unter "Nicht enthalten".
fn nicht_umgesetzter_typ(name: &str) -> Option<&'static str> {
    match name {
        "secret" => Some("secret[T] und die Constant-Time-Primitive (SPEC §9) sind nicht umgesetzt; siehe ABNAHME.md"),
        "Rc" | "Arc" | "Weak" => Some("Rc/Arc/Weak (SPEC §3.4) sind nicht umgesetzt; siehe ABNAHME.md"),
        _ => None,
    }
}
