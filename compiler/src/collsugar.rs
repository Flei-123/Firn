// SPDX-License-Identifier: MPL-2.0
//! Python-level collection sugar (wf-py): list/dict/set literals and comprehensions
//! for `Vec[T]`, `Dict[V]` and `Set[K]`. Pure parser desugar, docs/FIRN-VS-PYTHON.md G22b.
//!
//! The element type cannot be inferred in the parser, so the sugar needs a
//! DECLARED target type: the initializer of `let/var x: Vec[T] = ...` or the
//! value of `return ...` in a function declared `-> Vec[T]` / `-> Dict[V]`.
//!
//! ```firn
//! let sq: Vec[i64] = [x * x for x in xs if x > 0]      // comprehension
//! let v: Vec[i64] = [1, 2, 3]                          // list literal
//! let d: Dict[i64] = {"a": 1, "b": 2}                  // dict literal
//! let m: Dict[i64] = {name(i): i for i in 0..n}        // dict comprehension
//! let u: Set[i64] = {x % 10 for x in xs}               // set comprehension (set literal `{1, 2}` too)
//! ```
//! becomes
//! ```firn
//! var sq: Vec[i64] = vec_new[i64]()
//! for __each#L_C in 0 as usize..__each_len(xs) { let x = xs[__each#L_C]; if x > 0 { vec_push[i64](&sq, x * x) } }
//! ```
//! Several `for`/`if` clauses nest in source order. `return [..]` uses a hidden
//! `var __lc#L_C`. The iterables follow the rules of `for x in v` (a place, or a
//! range `a..b`). Twin: `collection_sugar` in `lib/firnc1/parser.fi`.

use crate::ast::{Block, Expr, ExprKind, Stmt, TypeExpr, UnOp};
use crate::diag::Span;
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::sema_generic::{coll_fn_name, coll_instance};

/// Where the finished collection goes.
pub(crate) enum CollDest {
    /// `let/var name: T = <literal>`: the statement itself is the declaration.
    Let { name: String, span: Span },
    /// `return <literal>`.
    Return { span: Span },
}

/// The innermost statement of a comprehension (and what must run before it).
pub(crate) struct CollLeaf {
    pre: Vec<Stmt>,
    stmt: Stmt,
}

impl<'a> Parser<'a> {
    /// Looks ahead over the bracket group at the current token: (has a top-level
    /// `for`, has a top-level `:`).
    fn coll_scan(&self) -> Option<(bool, bool)> {
        let mut depth: i32 = 0;
        let mut has_for = false;
        let mut colon = false;
        let mut i = self.pos;
        loop {
            let k = &self.toks.get(i)?.kind;
            match k {
                TokKind::LParen | TokKind::LBracket | TokKind::LBrace => depth += 1,
                TokKind::RParen | TokKind::RBracket | TokKind::RBrace => {
                    depth -= 1;
                    if depth <= 0 {
                        break;
                    }
                }
                TokKind::KwFor if depth == 1 => has_for = true,
                TokKind::Colon if depth == 1 => colon = true,
                TokKind::Eof => return None,
                _ => {}
            }
            i += 1;
        }
        Some((has_for, colon))
    }

    fn coll_ident(&mut self, name: &str, at: Span) -> Expr {
        self.mk(at, ExprKind::Ident(name.to_string()))
    }

    fn coll_addr(&mut self, name: &str, at: Span) -> Expr {
        let id = self.coll_ident(name, at);
        self.mk(at, ExprKind::Unary(UnOp::AddrOf, Box::new(id)))
    }

    /// `push` statement: `vec_push[T](&target, e)` / `dict_set[V](&target, k, e)`.
    fn coll_add(&mut self, target: &str, push_fn: &str, mut args: Vec<Expr>, at: Span) -> Stmt {
        let addr = self.coll_addr(target, at);
        args.insert(0, addr);
        let call = self.mk(at, ExprKind::Call(push_fn.to_string(), args, at));
        Stmt::Expr(call)
    }

    /// Entry point, called with the current token right behind `=` / `return`.
    /// `Some(stmt)`: the literal was handled (its extra statements are queued in
    /// `pre` / `after`); `None`: nothing consumed, parse as usual.
    pub(crate) fn coll_init(&mut self, ty: &TypeExpr, dest: CollDest) -> Option<Stmt> {
        let (base, targs) = coll_instance(ty)?;
        let (is_dict, is_set) = match (base.as_str(), targs.len()) {
            ("Vec", 1) => (false, false),
            ("Dict", 1) => (true, false),
            ("Set", 1) => (false, true),
            _ => return None,
        };
        let (open, close, close_txt) = if is_dict {
            (TokKind::LBrace, TokKind::RBrace, "after the entries of the dict literal")
        } else if is_set {
            (TokKind::LBrace, TokKind::RBrace, "after the elements of the set literal")
        } else {
            (TokKind::LBracket, TokKind::RBracket, "after the elements of the list literal")
        };
        if !self.at(&open) {
            return None;
        }
        let (has_for, colon) = self.coll_scan()?;
        // `[x; N]` is an array repeat literal; `{ a: .. }` without a type is a block.
        if is_dict && !colon && !has_for && !self.next_is(&TokKind::RBrace) {
            return None;
        }
        // `{ k: v }` is no set literal
        if is_set && colon {
            return None;
        }
        let (new_name, add_name) = if is_dict {
            ("dict_new", "dict_set")
        } else if is_set {
            ("set_new", "set_put")
        } else {
            ("vec_new", "vec_push")
        };
        let open_sp = self.span();
        let new_fn = coll_fn_name(self, new_name, &targs, open_sp)?;
        let add_fn = coll_fn_name(self, add_name, &targs, open_sp)?;
        self.bump(); // open
        let target = match &dest {
            CollDest::Let { name, .. } => name.clone(),
            CollDest::Return { .. } => format!("__lc#{}_{}", open_sp.line, open_sp.col),
        };
        let init = self.mk(open_sp, ExprKind::Call(new_fn, Vec::new(), open_sp));
        self.paren_depth += 1;
        self.no_q += 1;
        let saved_sl = self.no_struct_lit;
        self.no_struct_lit = false;
        let mut body: Vec<Stmt> = Vec::new();
        if !self.at(&close) {
            let h0 = self.hoist.len();
            let first = self.expr();
            let first_val = if is_dict {
                if !self.expect(TokKind::Colon, "between the key and the value of a dict entry") {
                    None
                } else {
                    Some(self.expr())
                }
            } else {
                None
            };
            if self.recovering {
                // fall through to the closing code
            } else if self.at(&TokKind::KwFor) {
                // comprehension: the hoisted f-string parts of the element run per iteration
                let hs = self.hoist.split_off(h0);
                let mut args = Vec::new();
                args.push(first);
                if let Some(v) = first_val {
                    args.push(v);
                }
                let stmt = self.coll_add(&target, &add_fn, args, open_sp);
                self.comp_leaf = Some(CollLeaf { pre: hs, stmt });
                let blk = self.coll_clauses();
                body = blk.stmts;
            } else {
                let mut items = vec![(first, first_val)];
                while self.eat(&TokKind::Comma) && !self.at(&close) && !self.recovering {
                    let k = self.expr();
                    let v = if is_dict {
                        if !self.expect(TokKind::Colon, "between the key and the value of a dict entry") {
                            break;
                        }
                        Some(self.expr())
                    } else {
                        None
                    };
                    items.push((k, v));
                }
                for (k, v) in items {
                    let mut args = vec![k];
                    if let Some(v) = v {
                        args.push(v);
                    }
                    let stmt = self.coll_add(&target, &add_fn, args, open_sp);
                    body.push(stmt);
                }
            }
        }
        self.no_struct_lit = saved_sl;
        self.no_q -= 1;
        self.paren_depth -= 1;
        let end = self.span();
        self.close(close, close_txt);
        let broken = self.recovering;
        self.end_stmt();
        if broken {
            return Some(Stmt::Error(open_sp));
        }
        let sp = Parser::join(open_sp, end);
        match dest {
            CollDest::Let { name, span } => {
                self.after.append(&mut body);
                Some(Stmt::Let { name, mutable: true, ty: Some(ty.clone()), init, span })
            }
            CollDest::Return { span } => {
                self.pre.push(Stmt::Let { name: target.clone(), mutable: true, ty: Some(ty.clone()), init, span: sp });
                self.pre.append(&mut body);
                let value = self.coll_ident(&target, sp);
                Some(Stmt::Return { value: Some(value), span })
            }
        }
    }

    fn next_is(&self, k: &TokKind) -> bool {
        matches!(self.toks.get(self.pos + 1), Some(t) if &t.kind == k)
    }

    /// The clauses of a comprehension, innermost last: `for PAT in IT`, `if COND`.
    /// Ends in the stored leaf statement. Called with the current token at the
    /// next clause, or at the closing bracket.
    pub(crate) fn coll_clauses(&mut self) -> Block {
        let start = self.span();
        let mut stmts: Vec<Stmt> = Vec::new();
        if self.recovering || self.at_eof() {
            return Block { stmts, span: start, end: start };
        }
        if self.at(&TokKind::KwFor) {
            let h0 = self.hoist.len();
            let st = self.for_stmt_c(true);
            let mut hs = self.hoist.split_off(h0);
            stmts.append(&mut hs);
            stmts.push(st);
        } else if self.at(&TokKind::KwIf) {
            let isp = self.bump();
            let h0 = self.hoist.len();
            let cond = self.cond_expr();
            let mut hs = self.hoist.split_off(h0);
            let then = self.coll_clauses();
            stmts.append(&mut hs);
            stmts.push(Stmt::If { cond, then, els: None, span: isp });
        } else {
            match self.comp_leaf.take() {
                Some(leaf) => {
                    stmts.extend(leaf.pre);
                    stmts.push(leaf.stmt);
                }
                None => {
                    self.error_here("this clause cannot follow a comprehension");
                    self.recovering = true;
                }
            }
        }
        Block { stmts, span: start, end: self.span() }
    }
}
