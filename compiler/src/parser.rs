// SPDX-License-Identifier: GPL-2.0-only
//! Hand-written, recursive descent parser (no generator).
//!
//! INTERFACE (fixed):
//!   `pub fn parse(toks: &[Token], dg: &mut Diags) -> ast::Program`
//! The parser hands out consecutive `ExprId`s from 0 upwards and sets
//! `Program::expr_count`. It MUST recover after an error at statement or
//! item level and report further errors.
//!
//! Grammar: SPEC §10.1. Precedence (weak -> strong):
//!   `||` , `&&` , comparison (not associative) , `+ - | ^` , `* / % & << >>` ,
//!   unary `- ! & *` , postfix `. [] () as`.
//! The semicolon is optional: a line break ends a statement.

use crate::ast::{
    Attr, Block, ConstDecl, Expr, ExprKind, BinOp, ExternInfo, FnDecl, ImportDecl, Param, Program, Stmt,
    StructDecl, TypeExpr, UnOp,
};
use std::collections::HashSet;
use crate::diag::{Diags, Span};
use crate::lexer::{TokKind, Token};

/// Maximum nesting depth (expressions, blocks, types). Beyond it there is a
/// clean error rather than a stack overflow.
const MAX_DEPTH: u32 = 200;

pub(crate) struct Parser<'a> {
    pub(crate) toks: &'a [Token],
    pub(crate) pos: usize,
    pub(crate) dg: &'a mut Diags,
    pub(crate) next_id: u32,
    pub(crate) depth: u32,
    /// An error has already been reported in the current statement.
    pub(crate) recovering: bool,
    /// `ident {` is NO struct literal in conditions but a name + block.
    pub(crate) no_struct_lit: bool,
    /// Bracket depth: inside `(...)`, `[...]`, `{...}` of an expression the
    /// expression may run across lines, outside it may not.
    pub(crate) paren_depth: u32,
    /// Number of the source file (module system, `modules.rs`).
    pub(crate) file: u32,
    /// Known module names from `import`: only with those is `alias.item` a
    /// qualified name and not a field access.
    pub(crate) modules: HashSet<String>,
    /// Nesting depth of the loops — `break`/`continue` need it.
    pub(crate) loop_depth: u32,
    /// Attributes that stood immediately in front of the next declaration
    /// (`attrs.rs`). Taken over by `fn_decl`/`struct_decl`.
    pub(crate) pending_attrs: Vec<crate::ast::Attr>,
    /// Hidden `var _fseg<N>` of the string interpolation (round 39): they get
    /// hoisted AHEAD of the next statement (`block` empties the list).
    pub(crate) hoist: Vec<Stmt>,
    /// > 0: an interpolation is already running — nesting does not exist yet.
    pub(crate) interp_depth: u32,
    /// **ROUND 79** — `[T; _]` is only allowed where an initializer follows
    /// that the length can be taken from: the type of a `let`/`var`.
    /// `let_stmt` switches it on around exactly that call; everywhere else a
    /// `_` is refused in the parser, so `ast::LEN_INFER` never leaves it.
    pub(crate) infer_len_ok: bool,
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

/// **ROUND 68** — the tokens that carry an expression over a line break.
/// Every one of them is INFIX ONLY: no statement of the language may begin
/// with it, so the reading is unambiguous. See `Parser::cont`.
fn continues_line(k: &TokKind) -> bool {
    matches!(
        k,
        TokKind::Plus
            | TokKind::Minus
            | TokKind::Slash
            | TokKind::Percent
            | TokKind::Amp
            | TokKind::Pipe
            | TokKind::Caret
            | TokKind::Shl
            | TokKind::Shr
            | TokKind::AndAnd
            | TokKind::OrOr
            | TokKind::EqEq
            | TokKind::NotEq
            | TokKind::Lt
            | TokKind::Le
            | TokKind::Gt
            | TokKind::Ge
            | TokKind::Dot
            | TokKind::KwAs
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
    // -------------------------------------------------------------------- Basics

    pub(crate) fn kind(&self) -> &TokKind {
        // The stream always ends with Eof; the index is never raised beyond it.
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

    /// Does the current token stand first on its line?
    fn at_line_start(&self) -> bool {
        if self.pos == 0 {
            return true;
        }
        match (self.toks.get(self.pos), self.toks.get(self.pos - 1)) {
            (Some(cur), Some(prev)) => cur.span.line > prev.span.line,
            _ => true,
        }
    }

    /// May the expression be continued with the current token?
    ///
    /// Outside brackets a line break closes the statement (SPEC §10:
    /// semicolon optional). **ROUND 68** adds the LINE CONTINUATION: a token
    /// that can only ever CONTINUE an expression — a binary operator, the
    /// dot of a field access, the `as` of a conversion — carries the
    /// expression across the line break, so a long condition or a long mask
    /// may be broken with the operator at the START of the following line.
    ///
    /// `*`, `(` and `[` are deliberately NOT in that set, and that is the
    /// whole point of the list: a line may legitimately begin with
    /// `*p = 0`, with `(*p).f = 0` or with an index, and none of those may
    /// silently become a multiplication, a call or an index belonging to
    /// the line before (`tests/1233_no_continuation.fi`).
    pub(crate) fn cont(&self) -> bool {
        self.paren_depth > 0 || !self.at_line_start() || continues_line(self.kind())
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

    /// Expects a particular token; otherwise the error sits at the OFFENDER.
    pub(crate) fn expect(&mut self, k: TokKind, ctx: &str) -> bool {
        if self.eat(&k) {
            return true;
        }
        if !self.recovering {
            self.error_here(format!(
                "expected '{}' {}, found '{}'",
                k.text(),
                ctx,
                self.kind().text()
            ));
        }
        false
    }

    /// Like `expect`, but reports nothing when the statement is broken already.
    pub(crate) fn close(&mut self, k: TokKind, ctx: &str) -> bool {
        if self.eat(&k) {
            return true;
        }
        if self.recovering {
            return false;
        }
        self.error_here(format!(
            "expected '{}' {}, found '{}'",
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
                "expected a name {}, found '{}'",
                ctx,
                self.kind().text()
            ));
        }
        None
    }

    /// Qualified name `module.name`: only when `module` is known through
    /// `import` is the dot read as a module access — otherwise it stays a
    /// field access. The name is passed on as "module.name";
    /// `modules.rs` resolves it while merging.
    pub(crate) fn qualify(&mut self, name: String, sp: Span) -> (String, Span) {
        if !self.modules.contains(&name) || !self.at(&TokKind::Dot) {
            return (name, sp);
        }
        let member = match self.toks.get(self.pos + 1).map(|t| t.kind.clone()) {
            Some(TokKind::Ident(m)) => m,
            _ => return (name, sp),
        };
        self.bump(); // '.'
        let msp = self.bump(); // label
        (format!("{}.{}", name, member), Parser::join(sp, msp))
    }

    pub(crate) fn mk(&mut self, span: Span, kind: ExprKind) -> Expr {
        let id = self.next_id;
        self.next_id += 1;
        Expr { id, span, kind }
    }

    /// Placeholder for a broken expression (the error has already been reported).
    pub(crate) fn broken_expr(&mut self, span: Span) -> Expr {
        self.mk(span, ExprKind::Int(0))
    }

    /// One span over both. **ROUND 79** — `Span::in_file` and no longer
    /// `Span::new`: the latter sets the file number to 0, so every joined
    /// span of a MODULE pointed into the root file. Nothing noticed for a
    /// long time because the joined spans (statements, blocks) were used by
    /// no message that a module can produce; the escape analysis of this
    /// round is the first one, and it showed a `return` of `lib/gc/gc.fi`
    /// under a line number of the test file.
    pub(crate) fn join(a: Span, b: Span) -> Span {
        // ROUND 72 FOUND THIS: `Span::new` always sets `file: 0` (the doc
        // comment on it says as much -- `Span::in_file` is the one that
        // takes a real file number, `diag.rs`). `a`/`b` are on the same
        // line here BY DEFINITION (the condition below requires it), which
        // for a module-system program means the same FILE too -- but
        // building the joined span with `Span::new` silently threw `a`'s
        // own `file` away and replaced it with 0 regardless, so every
        // compound expression (`a op b`, a call, anything `join` merges)
        // inside an IMPORTED file reported itself as living in the ROOT
        // file at whatever line/column happened to share this span's
        // shape. Nothing printed that number to a human before this round
        // -- `firnc0`'s own compile-time diagnostics apparently never hit
        // this exact path with a genuinely multi-file span, or always hit
        // the `else` branch instead; a CHECKED PANIC MESSAGE (SPEC section
        // 13, `L9`) is the first thing that put `span.file` in front of a
        // user for a RUNTIME event, and it printed the wrong file the
        // first time it had more than one to choose from (found running
        // `tests/1180_layout_position.fi`, a multi-module program).
        if a.line == b.line && b.col + b.len > a.col {
            Span::in_file(a.file, a.line, a.col, b.col + b.len - a.col)
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
                "nested too deeply (more than {} levels)",
                MAX_DEPTH
            ));
        }
        // Force progress, so that no endless loop arises.
        self.bump();
        true
    }

    // ---------------------------------------------------------------- Error recovery

    /// Advance to the end of the statement: swallow ';', stop in front of '}'
    /// or of a statement keyword at the start of a line.
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

    /// Advance to the next item at top level.
    pub(crate) fn sync_item(&mut self) {
        while !self.at_eof() && !starts_item(self.kind()) {
            self.bump();
        }
    }

    // ------------------------------------------------------------------ Types

    /// See `not_implemented_ty` at the end of the file.
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
            // ROUND 58 — `fn(T1, T2) -> R`, the type of a function VALUE.
            // Unambiguous: in a type position the keyword `fn` can mean
            // nothing else, and a declaration always carries a name after it.
            TokKind::KwFn => {
                let start = self.bump();
                if !self.expect(TokKind::LParen, "after 'fn' in a function type") {
                    return None;
                }
                let mut params = Vec::new();
                if !self.at(&TokKind::RParen) {
                    loop {
                        params.push(self.parse_type()?);
                        if self.eat(&TokKind::Comma) {
                            if self.at(&TokKind::RParen) {
                                break;
                            }
                            continue;
                        }
                        break;
                    }
                }
                let mut end = self.span();
                if !self.expect(TokKind::RParen, "after the parameters of a function type") {
                    return None;
                }
                let mut ret = None;
                if self.at(&TokKind::Arrow) {
                    self.bump();
                    let r = self.parse_type()?;
                    end = r.span();
                    ret = Some(Box::new(r));
                }
                Some(TypeExpr::Fn { params, ret, span: Parser::join(start, end) })
            }
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
                if !self.expect(TokKind::Semi, "after the element type of an array type") {
                    return None;
                }
                let len = match self.kind().clone() {
                    TokKind::Int(v) if v >= 0 => {
                        self.bump();
                        v as u64
                    }
                    // ROUND 79 (gap 10 of docs/ROUND66.md): `[u8; _]` — the
                    // length comes out of the initializer. `let_stmt` fills
                    // it in as soon as it has read the initializer, so
                    // `LEN_INFER` never reaches the type checker.
                    TokKind::Ident(n) if n == "_" => {
                        let sp = self.span();
                        self.bump();
                        if !self.infer_len_ok {
                            self.dg.error_note(
                                sp,
                                "the length '_' needs an initializer to be taken from",
                                "it works in a 'let'/'var' with a literal; a parameter, a field and a 'const' have to write the number out",
                            );
                            // No follow-up message for the same construct:
                            // `expected ')' after the parameter list` would
                            // say nothing the line above does not.
                            self.recovering = true;
                            return None;
                        }
                        crate::ast::LEN_INFER
                    }
                    _ => {
                        self.error_here(format!(
                            "expected an integer array length, found '{}'",
                            self.kind().text()
                        ));
                        return None;
                    }
                };
                let end = self.span();
                if !self.expect(TokKind::RBracket, "after the array length") {
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
                // Integration: type constructors that the SPEC describes but
                // stage 0 does NOT implement report a clear error here instead
                // of a clueless syntax error (SPEC §14 "not contained").
                // HOOK gc: `Gc[C]` and `GcWeak[C]` (gc.rs)
                if let Some(t) = crate::gc::hook_type(self, &name, sp) {
                    return Some(t);
                }
                // HOOK iface: `dyn I` — the interface value (iface.rs)
                if let Some(t) = crate::iface::hook_type(self, &name, sp) {
                    return Some(t);
                }
                if self.kind() == &TokKind::LBracket
                    && !crate::sema_generic::is_generic_struct(&name)
                {
                    if let Some(basic) = not_implemented_ty(&name) {
                        self.dg.error_note(
                            sp,
                            format!("'{}[T]' is not implemented in stage 0", name),
                            basic,
                        );
                        self.recovering = true;
                        return None;
                    }
                }
                // HOOK fehlerunionen: error union `E!T` (errors.rs)
                if let Some(t) = crate::errors::hook_type(self, &name, sp) {
                    return Some(t);
                }
                // HOOK types: generic type `Vec[i32]` (sema_generic.rs)
                if let Some(t) = crate::sema_generic::hook_generic_type(self, &name, sp) {
                    return Some(t);
                }
                let (name, sp) = self.qualify(name, sp);
                Some(TypeExpr::Named(name, sp))
            }
            other => {
                if !self.recovering {
                    self.error_here(format!("expected a type, found '{}'", other.text()));
                }
                None
            }
        }
    }

    // ------------------------------------------------------------ Expressions

    pub(crate) fn expr(&mut self) -> Expr {
        if self.too_deep() {
            let sp = self.span();
            return self.broken_expr(sp);
        }
        self.depth += 1;
        let e = self.or_expr();
        // HOOK fehlerunionen: `expression catch alternative` (errors.rs)
        let e = crate::errors::hook_catch(self, e);
        self.depth -= 1;
        e
    }

    /// Expression without a struct literal at top level (conditions).
    fn cond_expr(&mut self) -> Expr {
        let saved = self.no_struct_lit;
        self.no_struct_lit = true;
        let e = self.expr();
        self.no_struct_lit = saved;
        e
    }

    /// Expression inside brackets/arguments: struct literals are allowed there.
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
        // Comparisons are not associative (SPEC §10.1: exactly one comparison).
        while Parser::cmp_op(self.kind()).is_some() && self.cont() {
            let op2 = match Parser::cmp_op(self.kind()) {
                Some(o) => o,
                None => break,
            };
            self.error_here(format!(
                "comparisons are not chainable, put parentheses around the first comparison"
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
            // ROUND 72: explicit wrap/saturate, same precedence as '+'/'-'
            // (SPEC section 13, item L9).
            TokKind::PlusPercent => BinOp::AddWrap,
            TokKind::MinusPercent => BinOp::SubWrap,
            TokKind::PlusPipe => BinOp::AddSat,
            TokKind::MinusPipe => BinOp::SubSat,
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
            // ROUND 72: explicit wrap/saturate multiply, same precedence
            // as '*' (SPEC section 13, item L9).
            TokKind::StarPercent => BinOp::MulWrap,
            TokKind::StarPipe => BinOp::MulSat,
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
            TokKind::Tilde => Some(UnOp::BitNot),
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
                    // HOOK gc: checked downcast `x.as?[C]` (gc.rs)
                    if let Some(g) = crate::gc::hook_postfix(self, &e) {
                        e = g;
                        continue;
                    }
                    match self.ident("after '.' in the field access") {
                        Some((name, sp)) => {
                            // HOOK impl: `x.m(args)` is a method call,
                            // no field access (impls.rs, round 45)
                            if let Some(m) = crate::impls::hook_method_call(self, &e, &name, sp) {
                                e = m;
                                continue;
                            }
                            let full = Parser::join(e.span, sp);
                            e = self.mk(full, ExprKind::Field(Box::new(e), name, sp));
                        }
                        None => return e,
                    }
                }
                TokKind::LBracket => {
                    // HOOK types: generic call `foo[i32](..)` (sema_generic.rs)
                    if let Some(g) = crate::sema_generic::hook_generic_call(self, &e) {
                        e = g;
                        continue;
                    }
                    let start = self.bump();
                    let idx = self.nested_expr();
                    let end = self.span();
                    if !self.close(TokKind::RBracket, "after the index") {
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
                                "only direct function names can be called (pointer calls are not supported in stage 0)",
                            );
                            return e;
                        }
                    };
                    self.bump();
                    let (args, end) = self.call_args("after the argument list");
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

    /// Argument list after the '(' has been consumed. Yields the arguments and
    /// the position of the closing bracket (or of the offending token).
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
        // HOOK fehlerunionen: `try expression` (errors.rs)
        if let Some(e) = crate::errors::hook_primary(self) {
            return e;
        }
        // HOOK types: `Enum::Variant(..)` and `Vec[i32]{..}` (sema_match.rs)
        if let Some(e) = crate::sema_match::hook_primary(self) {
            return e;
        }
        // HOOK sizeof: `size_of[T]()` (sizeof.rs)
        if let Some(e) = crate::sizeof::hook_primary(self) {
            return e;
        }
        // HOOK kern: `asm("…", <asm_op>, out("rax"), clobber("memory"))`
        // (core.rs, round 52). Only when `asm` is followed immediately by `(`
        // and a string literal — otherwise `asm` stays an identifier.
        if let Some(e) = crate::core::hook_primary(self) {
            return e;
        }
        // HOOK gc: `gc C{…}`, `gc_null[C]()`, `weak_null[C]()` (gc.rs)
        if let Some(e) = crate::gc::hook_primary(self) {
            return e;
        }
        // HOOK fnval: the closure literal `fn(…) { … }` / `gc fn(…) { … }`
        // (fnval.rs, round 58)
        if let Some(e) = crate::fnval::hook_primary(self) {
            return e;
        }
        // ROUND 70: `++`/`--` inside an expression. Instead of a clueless
        // "expected an expression" the message says what the language does
        // and does not have.
        if matches!(self.kind(), TokKind::PlusPlus | TokKind::MinusMinus) {
            let word = self.kind().text();
            let sp = self.bump();
            self.dg.error_note(
                sp,
                format!("'{}' is a statement, not an expression", word),
                "write it on a line of its own; there is no 'y = x++' here, because prefix and postfix inside an expression are a source of error",
            );
            return self.broken_expr(sp);
        }
        match self.kind().clone() {
            TokKind::Int(v) => {
                let sp = self.bump();
                self.mk(sp, ExprKind::Int(v))
            }
            TokKind::Float(bits, single) => {
                let sp = self.bump();
                self.mk(sp, ExprKind::Float(bits, single))
            }
            TokKind::FloatF32(bits) => {
                let sp = self.bump();
                self.mk(sp, ExprKind::FloatF32(bits))
            }
            TokKind::FStr(raw) => {
                let sp = self.bump();
                let raw = raw.clone();
                self.interpolation(sp, &raw)
            }
            // STRING LITERAL -> array literal.
            //
            // `"abc"` becomes `[97, 98, 99]`, `u"abc"` the UTF-16 code
            // units. The type is therefore `[u8; N]` or `[u16; N]`, and
            // everything further — type check, lowering, code generation — is
            // there already. The price is stated honestly (SPEC §14.1.str,
            // S8): the data land as a sequence of single store instructions
            // in the frame, not in `.rodata`. For the source text the gain is
            // big nonetheless: `var m: [u8; 12] = "firn-gc: …"` instead of an
            // octet list computed by hand.
            TokKind::Str(_, val) => {
                let sp = self.bump();
                let mut elems: Vec<Expr> = Vec::new();
                let mut wide = false;
                match val {
                    crate::strings::LitValue::Octets(v) => {
                        for b in v {
                            elems.push(self.mk(sp, ExprKind::Int(b as i128)));
                        }
                    }
                    crate::strings::LitValue::Units(v) => {
                        wide = true;
                        for u in v {
                            elems.push(self.mk(sp, ExprKind::Int(u as i128)));
                        }
                    }
                }
                // ROUND 70: the empty literal is no longer an error HERE —
                // `""` is a valid, empty `str`. Where an array is wanted the
                // same message comes from the type check, which is the only
                // place that knows the context (strtype.rs::check_text).
                let lit = self.mk(sp, ExprKind::ArrayLit(elems));
                self.mk(sp, ExprKind::Text(wide, Box::new(lit)))
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
                self.close(TokKind::RParen, "after the parenthesized expression");
                e
            }
            TokKind::LBracket => {
                let start = self.bump();
                let mut elems = Vec::new();
                // `[value; N]` — repeat literal
                if !self.at(&TokKind::RBracket) && !self.at_eof() {
                    let first = self.nested_expr();
                    if self.at(&TokKind::Semi) && !self.recovering {
                        self.bump();
                        let count = self.nested_expr();
                        let end = self.span();
                        self.close(TokKind::RBracket, "after the length of the repetition literal");
                        let sp = Parser::join(start, end);
                        return self
                            .mk(sp, ExprKind::ArrayRepeat(Box::new(first), Box::new(count)));
                    }
                    let done = self.recovering || !self.eat(&TokKind::Comma);
                    elems.push(first);
                    if done {
                        let end = self.span();
                        self.close(TokKind::RBracket, "after the elements of the array literal");
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
                self.close(TokKind::RBracket, "after the elements of the array literal");
                let sp = Parser::join(start, end);
                self.mk(sp, ExprKind::ArrayLit(elems))
            }
            TokKind::KwSyscall => {
                let start = self.bump();
                self.expect(TokKind::LParen, "after 'syscall'");
                let (args, end) = self.call_args("after the arguments of 'syscall'");
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
                        "expected an expression, found '{}'",
                        other.text()
                    ));
                }
                let sp = self.span();
                self.broken_expr(sp)
            }
        }
    }

    /// `T{ field: value, ... }` — '{' is still ahead.
    pub(crate) fn struct_lit(&mut self, name: String, name_span: Span) -> Expr {
        self.bump(); // '{'
        let mut fields = Vec::new();
        loop {
            if self.at(&TokKind::RBrace) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let (fname, fspan) = match self.ident("for a field in the struct literal") {
                Some(x) => x,
                None => break,
            };
            if !self.expect(TokKind::Colon, "after the field name in the struct literal") {
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
        self.close(TokKind::RBrace, "at the end of the struct literal");
        let sp = Parser::join(name_span, end);
        self.mk(sp, ExprKind::StructLit(name, fields, name_span))
    }

    // ------------------------------------------------------------- Statements

    pub(crate) fn block(&mut self, ctx: &str) -> Block {
        let start = self.span();
        if self.too_deep() {
            return Block { stmts: Vec::new(), span: start, end: start };
        }
        if !self.expect(TokKind::LBrace, ctx) {
            self.recovering = false;
            return Block { stmts: Vec::new(), span: start, end: start };
        }
        self.depth += 1;
        let mut stmts = Vec::new();
        loop {
            if self.at(&TokKind::RBrace) {
                break;
            }
            if self.at_eof() {
                if !self.recovering {
                    self.error_here("expected '}' at the end of the block, found 'end of file'");
                }
                break;
            }
            if self.eat(&TokKind::Semi) {
                continue;
            }
            if self.dg.is_full() {
                // Avoid a flood of errors: skip the rest of the block.
                while !self.at(&TokKind::RBrace) && !self.at_eof() {
                    self.bump();
                }
                break;
            }
            let before = self.pos;
            let s = self.stmt();
            // Hidden text segments of the interpolation stand BEFORE the
            // statement that contains their interpolation.
            if !self.hoist.is_empty() {
                stmts.append(&mut self.hoist);
            }
            stmts.push(s);
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.span();
        self.eat(&TokKind::RBrace);
        self.depth -= 1;
        Block { stmts, span: Parser::join(start, end), end }
    }

    /// End of a statement: ';' or line break or '}'.
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
        // ROUND 70: `let y = x++`. The statement is over, and a `++` follows
        // - that is the postfix form inside an expression, and it does not
        // exist here. The message says so instead of only complaining about
        // the missing end of line.
        if matches!(self.kind(), TokKind::PlusPlus | TokKind::MinusMinus) {
            let word = self.kind().text();
            let sp = self.span();
            self.dg.error_note(
                sp,
                format!("'{}' is a statement, not an expression", word),
                "write it on a line of its own; there is no 'y = x++' here, because prefix and postfix inside an expression are a source of error",
            );
            self.recovering = false;
            self.sync_stmt();
            return;
        }
        self.error_here(format!(
            "expected ';' or an end of line after the statement, found '{}'",
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

    /// `defer <statement>` — the statement runs when the enclosing block is
    /// left (SPEC §5.1). Allowed is both a block (`defer { … }`) and a single
    /// statement (`defer close(fd)`).
    fn defer_stmt(&mut self, only_error: bool) -> Stmt {
        let word = if only_error { "errdefer" } else { "defer" };
        let start = self.bump();
        if self.at_eof() {
            self.error_here(format!("the deferred statement is missing after '{}'", word));
            return Stmt::Error(start);
        }
        let inner = self.stmt();
        let sp = Parser::join(start, inner.span());
        Stmt::Defer(Box::new(inner), only_error, sp)
    }

    fn stmt_inner(&mut self, start: Span) -> Stmt {
        // HOOK types: `match` statement (sema_match.rs)
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
            TokKind::LBrace => Stmt::Block(self.block("at the start of a block")),
            TokKind::KwFn | TokKind::KwStruct | TokKind::KwConst | TokKind::KwExtern => {
                self.error_here(format!(
                    "'{}' is only allowed at top level, not inside a function body",
                    self.kind().text()
                ));
                self.recovering = false;
                self.bump();
                self.sync_stmt();
                Stmt::Error(start)
            }
            _ => {
                let e = self.expr();
                // ROUND 70: `x += e` and `x++`. Both are STATEMENTS - the
                // parser sees them only here, and that is exactly why there
                // is no `y = x++`.
                if let Some(op) = compound_op(self.kind()) {
                    if !self.recovering {
                        self.bump();
                        let v = self.expr();
                        let sp = Parser::join(start, v.span);
                        self.end_stmt();
                        return Stmt::AssignOp { target: e, op, value: v, span: sp };
                    }
                }
                if matches!(self.kind(), TokKind::PlusPlus | TokKind::MinusMinus)
                    && !self.recovering
                {
                    let up = matches!(self.kind(), TokKind::PlusPlus);
                    let end = self.bump();
                    let sp = Parser::join(start, end);
                    self.end_stmt();
                    return Stmt::Step { target: e, up, span: sp };
                }
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
        let name = match self.ident(&format!("after '{}'", kw)) {
            Some((n, _)) => n,
            None => {
                self.recovering = false;
                self.sync_stmt();
                return Stmt::Error(start);
            }
        };
        let ty = if self.eat(&TokKind::Colon) {
            // ROUND 79: `[T; _]` is allowed HERE and only here -- an
            // initializer is coming to take the length from.
            self.infer_len_ok = true;
            let parsed = self.parse_type();
            self.infer_len_ok = false;
            match parsed {
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
        if !self.expect(TokKind::Assign, &format!("after the name in a '{}' statement", kw)) {
            self.recovering = false;
            self.sync_stmt();
            return Stmt::Error(start);
        }
        let init = self.expr();
        let broken = self.recovering;
        let sp = Parser::join(start, init.span);
        self.end_stmt();
        // ROUND 79: `[T; _]` — now that the initializer has been read, the
        // length is known.
        let ty = match ty {
            Some(t) => match self.fill_in_length(t, &init) {
                Some(t) => Some(t),
                None => return Stmt::Error(start),
            },
            None => None,
        };
        if broken {
            Stmt::Error(start)
        } else {
            Stmt::Let { name, mutable, ty, init, span: sp }
        }
    }

    /// **ROUND 79** — replaces the `_` of an array length with the number of
    /// elements of the initializer (gap 10 of `docs/ROUND66.md`).
    ///
    /// Only the OUTERMOST length: `[[u8; _]; 3]` stays an error, because the
    /// inner one would have to come out of the elements of the elements and
    /// nothing in this language writes that down today.
    fn fill_in_length(&mut self, t: TypeExpr, init: &Expr) -> Option<TypeExpr> {
        let (elem, len, span) = match t {
            TypeExpr::Array { elem, len, span } => (elem, len, span),
            other => return Some(other),
        };
        if len != crate::ast::LEN_INFER {
            return Some(TypeExpr::Array { elem, len, span });
        }
        match Parser::literal_length(init) {
            Some(n) => Some(TypeExpr::Array { elem, len: n, span }),
            None => {
                self.dg.error_note(
                    span,
                    "the length '_' can only be taken from a literal",
                    "write the number out, or initialise with a text literal, an array literal or '[v; n]'",
                );
                self.recovering = false;
                self.sync_stmt();
                None
            }
        }
    }

    /// How many elements does this initializer have? `None` = not a literal.
    fn literal_length(e: &Expr) -> Option<u64> {
        match &e.kind {
            // A text literal carries its array literal of octets inside
            // (round 70); its length is the one that counts, including a
            // written `\0`.
            ExprKind::Text(_, inner) => Parser::literal_length(inner),
            ExprKind::ArrayLit(v) => Some(v.len() as u64),
            ExprKind::ArrayRepeat(_, n) => match &n.kind {
                ExprKind::Int(v) if *v >= 0 => Some(*v as u64),
                _ => None,
            },
            _ => None,
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
        let then = self.block("after the condition of 'if'");
        let els = if self.at(&TokKind::KwElse) {
            self.bump();
            if self.at(&TokKind::KwIf) {
                Some(Box::new(self.stmt()))
            } else {
                Some(Box::new(Stmt::Block(self.block("after 'else'"))))
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
        let body = self.block("after the condition of 'while'");
        self.loop_depth -= 1;
        Stmt::While { cond, body, span: start }
    }

    /// The `for` loop over a half-open, ascending range `start..end`.
    fn for_stmt(&mut self) -> Stmt {
        let start = self.bump(); // 'for'
        let (name, name_span) = match self.ident("after 'for'") {
            Some(x) => x,
            None => {
                self.recovering = false;
                self.sync_stmt();
                return Stmt::Error(start);
            }
        };
        if !self.expect(TokKind::KwIn, "after the loop name") {
            self.recovering = false;
            self.sync_stmt();
            return Stmt::Error(start);
        }
        let from = self.cond_expr();
        if !self.expect(TokKind::DotDot, "between start and end of the range") {
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
        let body = self.block("after the range of 'for'");
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
                .error(sp, format!("'{}' is outside a loop", word));
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

    // ------------------------------------------------------------------- Items

    pub(crate) fn params(&mut self) -> Vec<Param> {
        let mut out = Vec::new();
        loop {
            if self.at(&TokKind::RParen) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let (name, sp) = match self.ident("for a parameter") {
                Some(x) => x,
                None => break,
            };
            if !self.expect(TokKind::Colon, "after the parameter name") {
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
        }
        if !self.expect(TokKind::KwFn, "at the start of a function declaration") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        let name = match self.ident("after 'fn'") {
            Some((n, _)) => n,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        if !self.expect(TokKind::LParen, "after the function name") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        let params = self.params();
        self.close(TokKind::RParen, "after the parameter list");
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
        let attrs = std::mem::take(&mut self.pending_attrs);
        // **Round 75** — `extern fn` (SPEC §14.5): a declaration WITHOUT a
        // body, closed with ';' instead of a block. The link name comes
        // from `#[link_name(c_symbol)]` if present, otherwise the bare
        // Firn name (no `_F0.` mangling — `modules::symbol`).
        if is_extern {
            if self.at(&TokKind::LBrace) {
                self.error_here(
                    "'extern fn' has no body; end the declaration with ';' instead of '{'"
                        .to_string(),
                );
                self.recovering = false;
                self.sync_item();
                return;
            }
            if !self.expect(TokKind::Semi, "after an 'extern fn' declaration") {
                self.recovering = false;
                self.sync_item();
                return;
            }
            self.recovering = false;
            let link_name = attrs.iter().find(|a| a.name == "link_name")
                .and_then(|a| a.args.first().cloned());
            let body = Block { stmts: Vec::new(), span: start, end: start };
            prog.funcs.push(FnDecl {
                name,
                params,
                ret,
                body,
                span: start,
                attrs,
                extern_info: Some(ExternInfo { link_name }),
            });
            return;
        }
        if !self.at(&TokKind::LBrace) {
            self.error_here(format!(
                "expected '{{' at the start of the function body, found '{}'",
                self.kind().text()
            ));
            self.recovering = false;
            self.sync_item();
            return;
        }
        let body = self.block("at the start of the function body");
        self.recovering = false;
        prog.funcs.push(FnDecl { name, params, ret, body, span: start, attrs, extern_info: None });
    }

    fn struct_decl(&mut self, prog: &mut Program) {
        let start = self.bump(); // 'struct'
        let name = match self.ident("after 'struct'") {
            Some((n, _)) => n,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        if !self.expect(TokKind::LBrace, "after the struct name") {
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
            let (fname, fspan) = match self.ident("for a struct field") {
                Some(x) => x,
                None => break,
            };
            if !self.expect(TokKind::Colon, "after the field name") {
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
        if !self.close(TokKind::RBrace, "at the end of the struct declaration") {
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
        let name = match self.ident("after 'const'") {
            Some((n, _)) => n,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        if !self.expect(TokKind::Colon, "after the name of a constant") {
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
        if !self.expect(TokKind::Assign, "after the type of a constant") {
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

    /// **ROUND 89** — `static NAME: T = value` / `static mut NAME: T = value`
    /// (SPEC §14.1.statics). Same shape as `const_decl` right above, with
    /// exactly one extra token; the difference is not in the syntax but in
    /// what the back end does with it (a place in `.bss`/`.data`/`.rodata`
    /// instead of a number folded into every use site).
    fn static_decl(&mut self, prog: &mut Program) {
        let start = self.bump(); // 'static'
        let mutable = self.eat(&TokKind::KwMut);
        let name = match self.ident("after 'static'") {
            Some((n, _)) => n,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        if !self.expect(TokKind::Colon, "after the name of a global variable") {
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
        if !self.expect(TokKind::Assign, "after the type of a global variable") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        let value = self.expr();
        let broken = self.recovering;
        self.end_stmt();
        self.recovering = false;
        if !broken {
            prog.statics.push(crate::ast::StaticDecl {
                name,
                ty,
                value,
                mutable,
                span: start,
            });
        }
    }

    /// `import path.module`
    fn import_decl(&mut self, prog: &mut Program) {
        let start = self.bump(); // 'import'
        let mut path: Vec<String> = Vec::new();
        loop {
            match self.ident("in a module path after 'import'") {
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
                .error(start, format!("module '{}' is imported more than once", alias));
        }
        self.modules.insert(alias.clone());
        prog.imports.push(ImportDecl { path, alias, span: start });
        self.end_stmt();
        self.recovering = false;
    }

    /// `export { a, b }`
    fn export_decl(&mut self, prog: &mut Program) {
        self.bump(); // 'export'
        if !self.expect(TokKind::LBrace, "after 'export'") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        loop {
            if self.at(&TokKind::RBrace) || self.at_eof() {
                break;
            }
            let before = self.pos;
            match self.ident("in the export list") {
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
        self.close(TokKind::RBrace, "at the end of the export list");
        self.recovering = false;
    }

    fn profile_decl(&mut self, prog: &mut Program) {
        let start = self.bump(); // 'profile'
        match self.ident("after 'profile'") {
            Some((n, _)) => {
                if prog.profile.is_some() {
                    self.dg.error(start, "more than one 'profile' declaration");
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

    /// `#[attr]` or `#[attr(arg, ...)]`, as often as you like one after another.
    ///
    /// The parser checks the form here ONLY. Whether the name exists, where it
    /// belongs and whether it does anything at stage 0 is decided by `sema.rs`
    /// per the register in `attrs.rs` — with line, column and a suggestion.
    fn attributes(&mut self) -> Vec<Attr> {
        let mut out = Vec::new();
        while self.at(&TokKind::Hash) {
            let start = self.bump(); // '#'
            if !self.expect(TokKind::LBracket, "after '#'") {
                self.recovering = false;
                self.sync_item();
                return out;
            }
            let name = match self.ident("as attribute name after '#['") {
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
                                "expected a name or a number as attribute argument, found '{}'",
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
                if !self.expect(TokKind::RParen, "after the attribute arguments") {
                    self.recovering = false;
                    self.sync_item();
                    return out;
                }
            }
            if !self.expect(TokKind::RBracket, "after the attribute") {
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
            // Attributes belong to the next declaration (attrs.rs).
            if self.at(&TokKind::Hash) {
                self.pending_attrs = self.attributes();
                while self.eat(&TokKind::Semi) {}
                if self.at_eof() {
                    if !self.pending_attrs.is_empty() {
                        let sp = self.pending_attrs[0].span;
                        self.dg.error(sp, "attribute without a declaration after it".to_string());
                    }
                    break;
                }
                if self.pos == before {
                    self.bump();
                }
                continue;
            }
            // HOOK fehlerunionen: `error` declaration (errors.rs)
            if crate::errors::hook_item(self) {
                if self.pos == before {
                    self.bump();
                }
                continue;
            }
            // HOOK gc: `gc class C { … }` (gc.rs, SPEC 3.5.1)
            if crate::gc::hook_item(self) {
                if self.pos == before {
                    self.bump();
                }
                continue;
            }
            // HOOK iface: `interface I { fn … }` (iface.rs, round 46)
            if crate::iface::hook_item(self) {
                if self.pos == before {
                    self.bump();
                }
                continue;
            }
            // HOOK impl: `impl T { fn … }`, `impl I for T { fn … }`
            // (impls.rs, round 45; the interface form round 46)
            if crate::impls::hook_item(self, &mut prog) {
                if self.pos == before {
                    self.bump();
                }
                continue;
            }
            // HOOK types: enum declaration and generic templates (sema_match.rs)
            if crate::sema_match::hook_item(self) {
                if self.pos == before {
                    self.bump();
                }
                continue;
            }
            match self.kind() {
                TokKind::KwComptime => {
                    let start = self.bump();
                    let b = self.block("after 'comptime'");
                    let sp = Parser::join(start, b.span);
                    prog.comptime_blocks.push((b, sp));
                }
                TokKind::KwFn | TokKind::KwExtern => self.fn_decl(&mut prog),
                TokKind::KwStruct => self.struct_decl(&mut prog),
                TokKind::KwConst => self.const_decl(&mut prog),
                TokKind::KwStatic => self.static_decl(&mut prog),
                TokKind::KwProfile => self.profile_decl(&mut prog),
                TokKind::KwImport => self.import_decl(&mut prog),
                TokKind::KwExport => self.export_decl(&mut prog),
                other => {
                    let msg = format!(
                        "expected 'fn', 'struct', 'const', 'static', 'comptime', 'import', 'export' or 'profile' at top level, found '{}'",
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

/// Resets the registrations of the neighbouring modules for ONE compilation.
/// With several files `modules.rs` calls that exactly once — otherwise every
/// file would lose the enums of the previous one.
pub fn reset_hooks() {
    // HOOK types: reset the registrations of this compilation (sema_match.rs)
    crate::sema_match::hook_reset();
    // HOOK fehlerunionen: the same for error sets/error unions (errors.rs)
    crate::errors::hook_reset();
    // HOOK sizeof: empty the size table of this compilation (sizeof.rs)
    crate::sizeof::hook_reset();
    // HOOK gc: the same for the gc classes (gc.rs)
    crate::gc::hook_reset();
    // HOOK iface: the same for interfaces and their implementations (iface.rs)
    crate::iface::hook_reset();
    crate::fnval::hook_reset();
    crate::fnval::closure_reset();
    // HOOK str: the builtin type of round 70 (strtype.rs)
    crate::strtype::hook_reset();
}

/// Like `parse`, but for a file of the source map: `file` is its number,
/// `base_id` the first `ExprId` still free. Afterwards `Program::expr_count`
/// is the first free id behind this file (absolute) — that is how
/// `modules.rs` lines the files up without overlap.
impl<'a> Parser<'a> {
    /// `f"..."` — the string interpolation (round 39).
    ///
    /// The parser splits the body AT COMPILE TIME into a chain of calls on
    /// the Fmt builder from `std.io` — no varargs, no parsing at run time.
    /// `f"x = {x}!"` becomes:
    ///
    /// ```text
    /// io.fmt_text(io.fmt_number(io.fmt_text(io.fmt_new(), &_fseg0[0], 6),
    ///                           (x) as i64),
    ///             &_fseg1[0], 1)
    /// ```
    ///
    /// The text segments cannot be addressed flatly (a bare array literal has
    /// no derivable type at this place) — they are hoisted as hidden
    /// `let _fseg<N>: [u8; N]` in front of the surrounding statement (`block`
    /// empties `self.hoist`). Whoever writes `f"..."` needs `import std.io` —
    /// otherwise the resolution reports `io` as not included, exactly as with
    /// a hand written `io.`.
    fn interpolation(&mut self, sp: Span, raw: &str) -> Expr {
        if self.interp_depth > 0 {
            self.dg.error(
                sp,
                "nested interpolation: an f\"...\" inside the expression of an f\"...\" is not yet".to_string(),
            );
            return self.broken_expr(sp);
        }
        let chars: Vec<char> = raw.chars().collect();
        let mut chain = self.mk(sp, ExprKind::Call("io.fmt_new".to_string(), Vec::new(), sp));
        let mut i: usize = 0;
        let mut text_of: usize = 0;
        let mut broken = false;
        while i < chars.len() {
            let c = chars[i];
            if c == '}' {
                self.dg.error(
                    sp,
                    "unbalanced brace in an interpolation: '}' without '{'".to_string(),
                );
                broken = true;
                break;
            }
            if c == '\\' {
                // The escape belongs to the text segment — the bracket scan
                // must not misread a \" as the end of the literal (the quotes
                // are already gone here); '{')}' does not appear escaped
                // in the core version (docs/ROUND39.md).
                i += 2;
                continue;
            }
            if c != '{' {
                i += 1;
                continue;
            }
            // Close the text segment in front of the bracket.
            if i > text_of {
                chain = self.interp_text(chain, sp, &chars[text_of..i], text_of);
            }
            // Look for the closing '}'; nesting of '{' counts as an error.
            let mut j = i + 1;
            let mut too = false;
            while j < chars.len() {
                if chars[j] == '{' {
                    self.dg.error(
                        sp,
                        "unbalanced brace in an interpolation: '{' inside the expression".to_string(),
                    );
                    broken = true;
                    break;
                }
                if chars[j] == '}' {
                    too = true;
                    break;
                }
                j += 1;
            }
            if broken {
                break;
            }
            if !too {
                self.dg.error(
                    sp,
                    "unbalanced brace in an interpolation: '{' without '}'".to_string(),
                );
                broken = true;
                break;
            }
            chain = self.interp_expr(chain, sp, &chars[i + 1..j], i + 1);
            i = j + 1;
            text_of = i;
        }
        if !broken && text_of < chars.len() {
            chain = self.interp_text(chain, sp, &chars[text_of..], text_of);
        }
        chain
    }

    /// One text segment: decode it, register it as a hidden `let _fseg<N>`
    /// and append `io.fmt_text(chain, &name[0] as u64, N)` to the chain.
    fn interp_text(&mut self, chain: Expr, sp: Span, raw: &[char], of: usize) -> Expr {
        let bytes = match crate::strings::decode_literal(crate::strings::LitKind::Str, raw) {
            Ok(crate::strings::LitValue::Octets(v)) => v,
            Ok(_) => Vec::new(),
            Err(e) => {
                self.dg.error(
                    self.spanned(sp.line, sp.col + 2 + of as u32 + e.off, 1),
                    format!("in a text segment of an interpolation: {}", e.msg),
                );
                return chain;
            }
        };
        if bytes.is_empty() {
            return chain;
        }
        let n = bytes.len();
        let name = format!("_fseg{}", self.next_id);
        let elems: Vec<Expr> = bytes
            .iter()
            .map(|b| self.mk(sp, ExprKind::Int(*b as i128)))
            .collect();
        let init = self.mk(sp, ExprKind::ArrayLit(elems));
        self.hoist.push(Stmt::Let {
            name: name.clone(),
            mutable: false,
            ty: Some(TypeExpr::Array {
                elem: Box::new(TypeExpr::Named("u8".to_string(), sp)),
                len: n as u64,
                span: sp,
            }),
            init,
            span: sp,
        });
        let ident = self.mk(sp, ExprKind::Ident(name));
        let null = self.mk(sp, ExprKind::Int(0));
        let idx = self.mk(sp, ExprKind::Index(Box::new(ident), Box::new(null)));
        let addr = self.mk(sp, ExprKind::Unary(UnOp::AddrOf, Box::new(idx)));
        let ptr = self.mk(
            sp,
            ExprKind::Cast(Box::new(addr), TypeExpr::Named("u64".to_string(), sp)),
        );
        let len = self.mk(sp, ExprKind::Int(n as i128));
        self.mk(
            sp,
            ExprKind::Call("io.fmt_text".to_string(), vec![chain, ptr, len], sp),
        )
    }

    /// One expression segment: lex the fragment again with padded positions,
    /// parse ONE expression out of it and append it to the chain as
    /// `io.fmt_number(chain, (expression) as i64)`.
    fn interp_expr(&mut self, chain: Expr, sp: Span, raw: &[char], of: usize) -> Expr {
        // The positions are right when the fragment stands at its real place:
        // pad lines and columns beforehand (strings are single line, so ONE
        // line suffices).
        let mut source = String::new();
        for _ in 1..sp.line {
            source.push('\n');
        }
        for _ in 0..sp.col + 1 + of as u32 {
            source.push(' ');
        }
        source.extend(raw.iter());
        let toks = crate::lexer::lex_file(&source, self.file, self.dg);
        let expr = in_expr(&toks, self.dg, self.file, &self.modules, &mut self.next_id);
        match expr {
            // ROUND 70 — NO `as i64` any more. Which builder step is right
            // depends on the TYPE of the expression, and only the type check
            // knows that. `io.fmt_value` is the placeholder the type check
            // and the lowering both resolve — out of the same material
            // (`strtype`/`sema::call`/`lower::lower_call`), so that they
            // cannot drift apart.
            Some(e) => self.mk(
                sp,
                ExprKind::Call("io.fmt_value".to_string(), vec![chain, e], sp),
            ),
            None => chain,
        }
    }

    /// Help for the error message above: a span built from single parts.
    fn spanned(&self, line: u32, col: u32, len: u32) -> Span {
        Span { line, col, len, file: self.file }
    }
}

/// Parses exactly ONE expression out of an interpolation segment.
///
/// The sub-parser shares diagnostics and module knowledge with the calling
/// parser; the `ExprId`s carry on seamlessly through `next_id`.
/// `interp_depth = 1` bars nesting: an `f"..."` in the fragment becomes a
/// clean error rather than silent hoist leaks.
fn in_expr(
    toks: &[Token],
    dg: &mut Diags,
    file: u32,
    modules: &HashSet<String>,
    next_id: &mut u32,
) -> Option<Expr> {
    let mut p = Parser {
        toks,
        pos: 0,
        dg,
        next_id: *next_id,
        depth: 1,
        recovering: false,
        no_struct_lit: false,
        paren_depth: 0,
        file,
        modules: modules.clone(),
        loop_depth: 0,
        pending_attrs: Vec::new(),
        hoist: Vec::new(),
        interp_depth: 1,
        infer_len_ok: false,
    };
    let e = p.nested_expr();
    *next_id = p.next_id;
    if p.dg.count() > 0 {
        return None;
    }
    if !p.at_eof() {
        p.error_here("expected exactly one expression in an interpolation");
        return None;
    }
    Some(e)
}

/// **ROUND 70** - which arithmetic operator does this compound assignment
/// carry? `None` for every other token.
///
/// The table is the whole definition: `x op= e` is exactly `x = x op e`,
/// with the same type rules and the same overflow behaviour, because both
/// end up in the very same check (`sema::binop_type`) and in the very same
/// FIR operation.
fn compound_op(k: &TokKind) -> Option<BinOp> {
    Some(match k {
        TokKind::PlusEq => BinOp::Add,
        TokKind::MinusEq => BinOp::Sub,
        TokKind::StarEq => BinOp::Mul,
        TokKind::SlashEq => BinOp::Div,
        TokKind::PercentEq => BinOp::Rem,
        TokKind::AmpEq => BinOp::And,
        TokKind::PipeEq => BinOp::Or,
        TokKind::CaretEq => BinOp::Xor,
        TokKind::ShlEq => BinOp::Shl,
        TokKind::ShrEq => BinOp::Shr,
        _ => return None,
    })
}

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
        hoist: Vec::new(),
        interp_depth: 0,
        infer_len_ok: false,
    };
    let prog = p.program();
    if !p.hoist.is_empty() {
        // f"..." at ITEM level (say in a `const`): there is no
        // statement in front of which the text segments could be hoisted.
        p.dg.error(
            Span::none(),
            "interpolation f\"...\" outside a statement (for example in a const) cannot be expressed".to_string(),
        );
    }
    prog
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
        assert_eq!(n, 0, "unexpected errors:\n{}", t);
        p
    }

    #[test]
    fn empty_main_program() {
        let p = ok("fn main() -> i32 { return 0 }");
        assert_eq!(p.funcs.len(), 1);
        assert_eq!(p.funcs[0].name, "main");
        assert!(p.expr_count > 0);
    }

    #[test]
    fn expr_ids_are_continuous() {
        let p = ok("fn main() -> i32 { let a: i32 = 1 + 2 * 3\n return a }");
        // 1, 2, 3, 2*3, 1+..., a  => 6 expressions
        assert_eq!(p.expr_count, 6);
    }

    fn dump(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(v) => format!("{}", v),
            ExprKind::Float(bits, _) => format!("{}", f64::from_bits(*bits)),
            ExprKind::FloatF32(bits) => format!("{}f", f32::from_bits(*bits)),
            ExprKind::Bool(b) => format!("{}", b),
            ExprKind::Ident(n) => n.clone(),
            // ROUND 70: the text literal carries its array literal inside.
            ExprKind::Text(_, inner) => dump(inner),
            ExprKind::Lambda(d) => format!("fn#{}", d.id),
            ExprKind::Unary(op, a) => format!(
                "({}{})",
                match op {
                    UnOp::Neg => "-",
                    UnOp::Not => "!",
                    UnOp::BitNot => "~",
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
            other => panic!("expected let, {:?}", other),
        }
    }

    #[test]
    fn precedence_after_ebnf() {
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
    fn semicolon_is_optional() {
        let p = ok("fn main() -> i32 {\n let a: i32 = 1;;\n var b: i32 = 2\n b = a + b\n return b\n}");
        assert_eq!(p.funcs[0].body.stmts.len(), 4);
    }

    #[test]
    fn line_end_finished_the_stmt() {
        // `*p = ...` on the next line is NO multiplication.
        let p = ok("fn main() -> i32 {\n var a: i32 = 1\n var p: *mut i32 = &a\n *p = 2\n return a\n}");
        match &p.funcs[0].body.stmts[2] {
            Stmt::Assign { target, .. } => match &target.kind {
                ExprKind::Unary(UnOp::Deref, _) => {}
                other => panic!("{:?}", other),
            },
            other => panic!("expected assignment, {:?}", other),
        }
        // An operator at the END OF A LINE continues the expression, though,
        // and inside brackets a line break does so too.
        let p2 = ok("fn f(a: i32, b: i32) -> i32 { return a }\nfn main() -> i32 {\n let s: i32 = 1 +\n 2\n let t: i32 = f(1,\n 2)\n return s + t\n}");
        assert_eq!(p2.funcs[1].body.stmts.len(), 3);
        // `a` and `(b)` on two lines are two statements, no call.
        let p3 = ok("fn main() -> i32 {\n let a: i32 = 1\n a\n (a)\n return a\n}");
        assert_eq!(p3.funcs[0].body.stmts.len(), 4);
    }

    #[test]
    fn struct_and_const_and_profile() {
        let p = ok("profile app\nstruct P { x: i32, y: i32 }\nconst M: i32 = 7\nfn main() -> i32 { let p: P = P{ x: 1, y: 2 }\n return p.x }");
        assert_eq!(p.structs.len(), 1);
        assert_eq!(p.structs[0].fields.len(), 2);
        assert_eq!(p.consts.len(), 1);
        assert_eq!(p.profile.as_ref().map(|x| x.0.clone()), Some("app".to_string()));
    }

    #[test]
    fn cond_without_struct_literal() {
        let p = ok("fn main() -> i32 { var x: i32 = 0\n while x < 3 { x = x + 1 }\n if x == 3 { return 0 } else { return 1 } }");
        assert_eq!(p.funcs[0].body.stmts.len(), 3);
    }

    #[test]
    fn types_and_arrays() {
        let p = ok("fn f(p: *mut u8, a: [i32; 4]) -> *u8 { return p as *u8 }\nfn main() -> i32 { return 0 }");
        assert_eq!(p.funcs.len(), 2);
        assert_eq!(p.funcs[0].params.len(), 2);
    }

    #[test]
    fn else_if_chain() {
        let p = ok("fn main() -> i32 { if false { return 1 } else if true { return 2 } else { return 3 } }");
        match &p.funcs[0].body.stmts[0] {
            Stmt::If { els: Some(b), .. } => match b.as_ref() {
                Stmt::If { .. } => {}
                other => panic!("expected else-if, {:?}", other),
            },
            other => panic!("expected if, {:?}", other),
        }
    }

    #[test]
    fn several_error_become_reported() {
        let src = "fn main() -> i32 {\n    let x = add(1, 2 ;\n    let = 3\n    return 0\n}\n";
        let (_, n, text) = parse_src(src);
        assert!(n >= 2, "expected several errors, got {}:\n{}", n, text);
        assert!(text.contains("2:22"), "position missing:\n{}", text);
        assert!(text.contains("expected ')'"), "message missing:\n{}", text);
    }

    #[test]
    fn error_in_two_funcs() {
        let src = "fn a() -> i32 { return ) }\nfn b() -> i32 { return * }\n";
        let (_, n, text) = parse_src(src);
        assert!(n >= 2, "{}", text);
    }

    #[test]
    fn no_hanger_at_abort() {
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
            assert!(n >= 1, "expected error for {:?}", src);
        }
    }

    #[test]
    fn depth_nesting_breaks_clean_ab() {
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
    fn compare_is_not_associative() {
        let (_, n, text) = parse_src("fn main() -> i32 { let b: bool = 1 < 2 < 3\n return 0 }");
        assert!(n >= 1);
        assert!(text.contains("not chainable"), "{}", text);
    }

    #[test]
    fn extern_with_body_is_rejected() {
        // Round 75: `extern fn` no longer means "unsupported" — but it still
        // MUST NOT have a body (that would defeat the point of a declaration
        // without one).
        let (_p, n, text) = parse_src("extern fn write(fd: i32) -> i32 { return 0 }\nfn main() -> i32 { return 0 }");
        assert!(n >= 1);
        assert!(text.contains("no body"), "{}", text);
    }

    #[test]
    fn extern_fn_is_parsed_without_a_body() {
        // Round 75 (SPEC §14.5): `extern fn` IS supported now — a
        // declaration terminated with ';', no block, no FIR body.
        let p = ok("extern fn write(fd: i32, buf: *u8, n: usize) -> i64;\nfn main() -> i32 { return 0 }");
        assert_eq!(p.funcs.len(), 2);
        let ext = &p.funcs[0];
        assert_eq!(ext.name, "write");
        assert!(ext.body.stmts.is_empty());
        assert!(ext.extern_info.is_some());
        assert_eq!(ext.extern_info.as_ref().unwrap().link_name, None);
    }

    #[test]
    fn extern_fn_with_link_name() {
        let p = ok("#[link_name(exit)]\nextern fn c_exit(code: i32) -> i32;\nfn main() -> i32 { return 0 }");
        let ext = &p.funcs[0];
        assert_eq!(ext.name, "c_exit");
        assert_eq!(ext.extern_info.as_ref().unwrap().link_name.as_deref(), Some("exit"));
    }

    #[test]
    fn syscall_becomes_recognized() {
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

/// Type constructors that `SPEC.md` describes but stage 0 does not build.
/// They get a clear error of their own rather than a syntax error —
/// `SPEC.md` §14 lists them under "not contained".
fn not_implemented_ty(name: &str) -> Option<&'static str> {
    match name {
        "secret" => Some("secret[T] and the constant-time primitives (SPEC §9) are not implemented; see ACCEPTANCE.md"),
        "Rc" | "Arc" | "Weak" => Some("Rc/Arc/Weak (SPEC §3.4) are not implemented; see ACCEPTANCE.md"),
        _ => None,
    }
}
