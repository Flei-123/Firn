// SPDX-License-Identifier: GPL-2.0-only
//! Sum types (`enum`) and pattern matching (`match`) — SPEC §6.3, `L4`.
//!
//! This file belongs to the module `types`. It holds
//!  * the parser extensions (as `impl` on `parser::Parser`, wired up to the
//!    `// HOOK types` lines in `parser.rs`),
//!  * the registration of the enums at the type context,
//!  * the type check of the patterns and
//!  * the **exhaustiveness check at compile time**
//!    (`check_exhaustive`) — a missing case is a hard ERROR with line and
//!    column that states the missing variant, not a warning.
//!
//! ## Memory layout of one enum (binding)
//!
//! ```text
//! offset 0        : __tag : u32      (variant number, 0-based, declaration order)
//! offset payload_off : payload data of the respective variant
//! ```
//!
//! `payload_off = round_up(4, payload_align)`, where `payload_align` is the
//! largest alignment of all payload fields (at least 1). The payload fields
//! of one variant sit one after another with natural alignment, in
//! declaration order; the regions of **different** variants overlap (a true
//! union). Size of the enum =
//! `round_up(payload_off + max_variant_size, align)`,
//! `align = max(4, payload_align)`.
//!
//! Technically the enum is entered as a struct with the fields `__tag` and
//! `__v<tag>_<i>` in `types::TypeCtx`; the offsets are computed here, not by
//! `TypeCtx::set_fields`.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::ast::{Block, Expr, ExprKind, Stmt, TypeExpr};
use crate::diag::{Diag, Span};
use crate::lexer::TokKind;
use crate::parser::Parser;
use crate::sema::Checker;
use crate::types::{round_up, Field, StructDef, Type};

/// Prefix of the internal call names for `match` statements. It contains `#`,
/// so it can never be an identifier out of the source text.
pub(crate) const MATCH_PREFIX: &str = "__match#";

// ----------------------------------------------------------------- Data model

#[derive(Clone, Debug)]
pub(crate) struct VariantDef {
    pub(crate) name: String,
    pub(crate) tag: i128,
    /// payload types as written
    pub(crate) field_tys: Vec<TypeExpr>,
    /// resolved payload types (after `layout_enums`)
    pub(crate) fields: Vec<Type>,
    /// byte offsets of the payload fields (after `layout_enums`)
    pub(crate) offsets: Vec<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct EnumDef {
    pub(crate) name: String,
    pub(crate) span: Span,
    pub(crate) variants: Vec<VariantDef>,
    /// Index into `TypeCtx::structs`
    pub(crate) struct_idx: usize,
    pub(crate) size: u64,
    pub(crate) align: u64,
}

impl EnumDef {
    pub(crate) fn variant(&self, name: &str) -> Option<&VariantDef> {
        self.variants.iter().find(|v| v.name == name)
    }
}

/// Pattern (SPEC §6.3): variant with binding, literal, range, `_`, nested.
#[derive(Clone, Debug)]
pub(crate) enum Pattern {
    /// `_`
    Wild(Span),
    /// `x` — binds the whole value
    Bind(String, Span),
    Int(i128, Span),
    Bool(bool, Span),
    /// `lo..hi` (half open) or `lo..=hi` (inclusive)
    Range { lo: i128, hi: i128, inclusive: bool, span: Span },
    /// `Enum::Variant(sub, pattern)` — `ename` may be absent (`::Variant`)
    Variant { ename: Option<String>, vname: String, subs: Vec<Pattern>, span: Span },
}

impl Pattern {
    pub(crate) fn span(&self) -> Span {
        match self {
            Pattern::Wild(s) | Pattern::Bind(_, s) | Pattern::Int(_, s) | Pattern::Bool(_, s) => *s,
            Pattern::Range { span, .. } => *span,
            Pattern::Variant { span, .. } => *span,
        }
    }
    /// Does the pattern ALWAYS match (that is, does it only bind)?
    pub(crate) fn is_irrefutable(&self) -> bool {
        matches!(self, Pattern::Wild(_) | Pattern::Bind(..))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Arm {
    pub(crate) pat: Pattern,
    pub(crate) body: Block,
    pub(crate) span: Span,
}

#[derive(Clone, Debug)]
pub(crate) struct MatchInfo {
    pub(crate) subject: Expr,
    pub(crate) arms: Vec<Arm>,
    pub(crate) span: Span,
}

/// What the pattern match examines — basis of the exhaustiveness check.
#[derive(Clone, Debug)]
pub(crate) enum Subject {
    Enum(EnumDef),
    Bool,
    Int(Type),
    /// Faulty expression — it has already been reported.
    Bad,
}

#[derive(Default)]
struct Registry {
    enums: Vec<EnumDef>,
    by_name: HashMap<String, usize>,
    by_struct: HashMap<usize, usize>,
    matches: Vec<MatchInfo>,
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
}

/// Resets all registrations (one per compilation).
pub(crate) fn hook_reset() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
    crate::sema_generic::reset();
}

pub(crate) fn enum_count() -> usize {
    REG.with(|r| r.borrow().enums.len())
}

pub(crate) fn enum_at(i: usize) -> Option<EnumDef> {
    REG.with(|r| r.borrow().enums.get(i).cloned())
}

pub(crate) fn enum_by_name(name: &str) -> Option<EnumDef> {
    REG.with(|r| {
        let reg = r.borrow();
        reg.by_name.get(name).and_then(|i| reg.enums.get(*i)).cloned()
    })
}

pub(crate) fn enum_by_struct(idx: usize) -> Option<EnumDef> {
    REG.with(|r| {
        let reg = r.borrow();
        reg.by_struct.get(&idx).and_then(|i| reg.enums.get(*i)).cloned()
    })
}

pub(crate) fn is_enum_name(name: &str) -> bool {
    REG.with(|r| r.borrow().by_name.contains_key(name))
}

pub(crate) fn match_info(idx: usize) -> Option<MatchInfo> {
    REG.with(|r| r.borrow().matches.get(idx).cloned())
}

/// `// HOOK types` for the module system (`modules.rs`): the body blocks of
/// the `match` cases do NOT sit in the AST but in this registry. So that the
/// module system can rewrite the names in them just like in the rest of the
/// AST, an entry is taken out and put back after the rewrite.
pub(crate) fn take_match(idx: usize) -> Option<MatchInfo> {
    REG.with(|r| r.borrow().matches.get(idx).cloned())
}

/// Counterpart to `take_match`.
pub(crate) fn put_match(idx: usize, m: MatchInfo) {
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        if let Some(slot) = reg.matches.get_mut(idx) {
            *slot = m;
        }
    })
}

fn match_index_of(name: &str) -> Option<usize> {
    name.strip_prefix(MATCH_PREFIX).and_then(|s| s.parse::<usize>().ok())
}

// ------------------------------------------------------------- Parser hooks

impl<'a> Parser<'a> {
    fn tk(&self, off: usize) -> TokKind {
        match self.toks.get(self.pos + off) {
            Some(t) => t.kind.clone(),
            None => TokKind::Eof,
        }
    }

    fn tspan(&self, off: usize) -> Span {
        match self.toks.get(self.pos + off) {
            Some(t) => t.span,
            None => Span::none(),
        }
    }

    /// Do the tokens `off` and `off+1` stand right next to each other?
    fn adjacent(&self, off: usize) -> bool {
        let a = self.tspan(off);
        let b = self.tspan(off + 1);
        a.line == b.line && a.col + a.len == b.col
    }

    /// `::`
    pub(crate) fn types_at_colon2(&self, off: usize) -> bool {
        self.tk(off) == TokKind::Colon && self.tk(off + 1) == TokKind::Colon && self.adjacent(off)
    }

    /// `=>`
    pub(crate) fn types_at_fat_arrow(&self) -> bool {
        self.tk(0) == TokKind::Assign && self.tk(1) == TokKind::Gt && self.adjacent(0)
    }

    /// `..`
    fn types_at_dotdot(&self) -> bool {
        self.tk(0) == TokKind::DotDot
    }

    fn types_eat_colon2(&mut self) {
        self.bump();
        self.bump();
    }

    /// Enum declaration: `enum E { A, B(i32), C(Point, bool) }`
    fn types_enum_decl(&mut self) {
        let start = self.bump(); // 'enum'
        let (name, nspan) = match self.ident("after 'enum'") {
            Some(x) => x,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        if !self.expect(TokKind::LBrace, "after the name of the enum") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        let mut variants: Vec<VariantDef> = Vec::new();
        loop {
            while self.eat(&TokKind::Comma) {}
            if self.at(&TokKind::RBrace) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let (vname, vspan) = match self.ident("for a variant of the enum") {
                Some(x) => x,
                None => break,
            };
            let mut field_tys = Vec::new();
            if self.eat(&TokKind::LParen) {
                loop {
                    if self.at(&TokKind::RParen) || self.at_eof() {
                        break;
                    }
                    let b2 = self.pos;
                    match self.parse_type() {
                        Some(t) => field_tys.push(t),
                        None => break,
                    }
                    if !self.eat(&TokKind::Comma) {
                        break;
                    }
                    if self.pos == b2 {
                        self.bump();
                    }
                }
                self.close(TokKind::RParen, "after the payload of a variant");
            }
            if variants.iter().any(|v| v.name == vname) {
                self.dg.error(
                    vspan,
                    format!("variant '{}' is already declared in enum '{}'", vname, name),
                );
            } else {
                let tag = variants.len() as i128;
                variants.push(VariantDef {
                    name: vname,
                    tag,
                    field_tys,
                    fields: Vec::new(),
                    offsets: Vec::new(),
                });
            }
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.span();
        self.close(TokKind::RBrace, "at the end of the enum");
        self.recovering = false;
        if variants.is_empty() {
            self.dg.error(
                nspan,
                format!("enum '{}' has no variant", name),
            );
            return;
        }
        let def = EnumDef {
            name: name.clone(),
            span: Parser::join(start, end),
            variants,
            struct_idx: usize::MAX,
            size: 0,
            align: 1,
        };
        let duplicate = REG.with(|r| {
            let mut reg = r.borrow_mut();
            if reg.by_name.contains_key(&name) {
                return true;
            }
            let i = reg.enums.len();
            reg.enums.push(def);
            reg.by_name.insert(name.clone(), i);
            false
        });
        if duplicate {
            self.dg
                .error(nspan, format!("enum '{}' is already declared", name));
        }
    }

    /// `match subject { pattern => { .. } .. }` as a statement.
    fn types_match_stmt(&mut self) -> Stmt {
        let start = self.bump(); // 'match'
        if crate::sema_generic::in_template() {
            // The body blocks of the cases sit in the registry, not in the
            // AST — a template could not replace them per instantiation.
            self.dg.error_note(
                start,
                "'match' inside a generic template is not supported in this stage",
                "move the pattern match into a non-generic function",
            );
        }
        let saved = self.no_struct_lit;
        self.no_struct_lit = true;
        let subject = self.expr();
        self.no_struct_lit = saved;
        if !self.expect(TokKind::LBrace, "after the expression of 'match'") {
            self.recovering = false;
            self.sync_item();
            return Stmt::Error(start);
        }
        let mut arms: Vec<Arm> = Vec::new();
        loop {
            while self.eat(&TokKind::Comma) || self.eat(&TokKind::Semi) {}
            if self.at(&TokKind::RBrace) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let pat = match self.types_pattern(0) {
                Some(p) => p,
                None => break,
            };
            if !self.types_at_fat_arrow() {
                self.error_here(format!(
                    "expected '=>' after the pattern, found '{}'",
                    self.kind().text()
                ));
                self.recovering = false;
                break;
            }
            self.bump();
            self.bump();
            if !self.at(&TokKind::LBrace) {
                self.error_here(format!(
                    "expected '{{' after '=>' (the body of an arm is a block), found '{}'",
                    self.kind().text()
                ));
                self.recovering = false;
                break;
            }
            let body = self.block("at the start of a match arm");
            self.recovering = false;
            let span = Parser::join(pat.span(), body.span);
            arms.push(Arm { pat, body, span });
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.span();
        self.close(TokKind::RBrace, "at the end of 'match'");
        self.recovering = false;
        let span = Parser::join(start, end);
        if arms.is_empty() {
            self.dg.error(span, "'match' needs at least one arm");
            return Stmt::Error(span);
        }
        let idx = REG.with(|r| {
            let mut reg = r.borrow_mut();
            reg.matches.push(MatchInfo { subject, arms, span });
            reg.matches.len() - 1
        });
        let name = format!("{}{}", MATCH_PREFIX, idx);
        let e = self.mk(span, ExprKind::Call(name, Vec::new(), span));
        Stmt::Expr(e)
    }

    fn types_pattern(&mut self, depth: u32) -> Option<Pattern> {
        if depth > 32 {
            self.error_here("pattern is nested too deeply (more than 32 levels)");
            self.recovering = false;
            return None;
        }
        let sp = self.span();
        match self.kind().clone() {
            TokKind::KwTrue => {
                self.bump();
                Some(Pattern::Bool(true, sp))
            }
            TokKind::KwFalse => {
                self.bump();
                Some(Pattern::Bool(false, sp))
            }
            TokKind::Int(_) | TokKind::Minus => {
                let lo = self.types_pattern_int()?;
                if self.types_at_dotdot() {
                    self.bump();
                    let inclusive = self.eat(&TokKind::Assign);
                    let hisp = self.span();
                    let hi = self.types_pattern_int()?;
                    let span = Parser::join(sp, hisp);
                    if (inclusive && hi < lo) || (!inclusive && hi <= lo) {
                        self.dg
                            .error(span, "the range in the pattern is empty (upper bound too small)");
                        return None;
                    }
                    return Some(Pattern::Range { lo, hi, inclusive, span });
                }
                Some(Pattern::Int(lo, sp))
            }
            TokKind::Ident(name) => {
                if name == "_" {
                    self.bump();
                    return Some(Pattern::Wild(sp));
                }
                self.bump();
                if self.types_at_colon2(0) {
                    self.types_eat_colon2();
                    let (vname, vspan) = self.ident("after '::' in the pattern")?;
                    let mut subs = Vec::new();
                    let mut end = vspan;
                    if self.eat(&TokKind::LParen) {
                        loop {
                            if self.at(&TokKind::RParen) || self.at_eof() {
                                break;
                            }
                            let before = self.pos;
                            let p = self.types_pattern(depth + 1)?;
                            subs.push(p);
                            if !self.eat(&TokKind::Comma) {
                                break;
                            }
                            if self.pos == before {
                                self.bump();
                            }
                        }
                        end = self.span();
                        self.close(TokKind::RParen, "after the subpatterns");
                    }
                    return Some(Pattern::Variant {
                        ename: Some(name),
                        vname,
                        subs,
                        span: Parser::join(sp, end),
                    });
                }
                Some(Pattern::Bind(name, sp))
            }
            other => {
                self.error_here(format!("expected a pattern, found '{}'", other.text()));
                self.recovering = false;
                None
            }
        }
    }

    fn types_pattern_int(&mut self) -> Option<i128> {
        let neg = self.eat(&TokKind::Minus);
        match self.kind().clone() {
            TokKind::Int(v) => {
                self.bump();
                Some(if neg { -v } else { v })
            }
            other => {
                self.error_here(format!(
                    "expected an integer in the pattern, found '{}'",
                    other.text()
                ));
                self.recovering = false;
                None
            }
        }
    }
}

/// `// HOOK types` in `parser.rs::program` — enums and generic
/// templates at top level.
pub(crate) fn hook_item(p: &mut Parser) -> bool {
    if matches!(p.kind(), TokKind::KwEnum) {
        p.types_enum_decl();
        return true;
    }
    crate::sema_generic::hook_item(p)
}

/// `// HOOK types` in `parser.rs::stmt_inner` — `match` statement.
pub(crate) fn hook_stmt(p: &mut Parser) -> Option<Stmt> {
    if matches!(p.kind(), TokKind::KwMatch) {
        return Some(p.types_match_stmt());
    }
    None
}

/// `// HOOK types` in `parser.rs::primary` — `Enum::Variant(..)`.
pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr> {
    if let TokKind::Ident(name) = p.kind().clone() {
        if p.types_at_colon2(1) && matches!(p.tk(3), TokKind::Ident(_)) {
            let sp = p.bump();
            p.types_eat_colon2();
            let (vname, vspan) = p.ident("after '::'")?;
            let full = format!("{}::{}", name, vname);
            let mut args = Vec::new();
            let mut end = vspan;
            if p.at(&TokKind::LParen) {
                p.bump();
                let (a, e) = p.call_args("after the payload of the variant");
                args = a;
                end = e;
            }
            let span = Parser::join(sp, end);
            return Some(p.mk(span, ExprKind::Call(full, args, span)));
        }
    }
    crate::sema_generic::hook_primary(p)
}

// ------------------------------------------- Registration at the type context

/// `// HOOK types` in `sema::run` (before `collect_structs`): registers
/// the names of all enums, so that structs and functions can refer to them.
pub(crate) fn declare_enums(ck: &mut Checker) {
    let n = enum_count();
    for i in 0..n {
        let def = match enum_at(i) {
            Some(d) => d,
            None => continue,
        };
        if ck.tcx.lookup(&def.name).is_some() {
            ck.dg
                .error(def.span, format!("type '{}' is already declared", def.name));
            continue;
        }
        let idx = ck.tcx.declare(&def.name);
        REG.with(|r| {
            let mut reg = r.borrow_mut();
            if let Some(d) = reg.enums.get_mut(i) {
                d.struct_idx = idx;
            }
            reg.by_struct.insert(idx, i);
        });
    }
}

/// `// HOOK types` in `sema::run` (after `collect_structs`): computes the
/// layout of every enum and enters it as a struct layout.
pub(crate) fn layout_enums(ck: &mut Checker, prog: &crate::ast::Program) {
    if enum_count() == 0 {
        return;
    }
    // Enums may (as yet) not sit by value inside a struct:
    // the struct layouts are settled by that point already.
    for s in &prog.structs {
        for (fname, te, span) in &s.fields {
            if let Some(n) = value_named(te) {
                if is_enum_name(&n) {
                    ck.dg.error_note(
                        *span,
                        format!(
                            "field '{}' has the enum type '{}' — that is not supported in this stage",
                            fname, n
                        ),
                        "use a pointer ('*mut T') to the enum",
                    );
                }
            }
        }
    }

    // Ordered by dependency (one enum can contain another by value).
    // Cycles are an error.
    let n = enum_count();
    let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut defs: Vec<EnumDef> = Vec::with_capacity(n);
    for i in 0..n {
        let d = match enum_at(i) {
            Some(d) => d,
            None => continue,
        };
        for v in &d.variants {
            for te in &v.field_tys {
                if let Some(name) = value_named(te) {
                    if let Some(j) = REG.with(|r| r.borrow().by_name.get(&name).copied()) {
                        deps[i].push(j);
                    }
                }
            }
        }
        defs.push(d);
    }
    let mut state = vec![0u8; n];
    let mut order: Vec<usize> = Vec::new();
    let mut bad: Vec<usize> = Vec::new();
    for i in 0..n {
        toposort(i, &deps, &mut state, &mut order, &mut bad);
    }
    for i in &bad {
        if let Some(d) = defs.get(*i) {
            ck.dg.error_note(
                d.span,
                format!("enum '{}' contains itself (directly or indirectly)", d.name),
                "use a pointer there, e.g. '*mut T'",
            );
        }
    }

    for i in order {
        if bad.contains(&i) {
            continue;
        }
        let mut d = match defs.get(i).cloned() {
            Some(d) => d,
            None => continue,
        };
        if d.struct_idx == usize::MAX {
            continue;
        }
        // 1. resolve payload types, determine alignment
        let mut payload_align: u64 = 1;
        let mut resolved: Vec<Vec<Type>> = Vec::new();
        for v in &d.variants {
            let mut tys = Vec::new();
            for te in &v.field_tys {
                let t = ck.resolve_ty(te);
                if matches!(t, Type::Void) {
                    ck.dg.error(te.span(), "the payload of a variant cannot have the type '()'");
                }
                let a = ck.tcx.align_of(&t).max(1);
                if a > payload_align {
                    payload_align = a;
                }
                tys.push(t);
            }
            resolved.push(tys);
        }
        let payload_off = round_up(4, payload_align);
        // 2. offsets per variant (variants overlap)
        let mut max_end = payload_off;
        let mut fields: Vec<Field> = vec![Field {
            name: "__tag".to_string(),
            ty: Type::U32,
            offset: 0,
        }];
        for (vi, tys) in resolved.iter().enumerate() {
            let mut off = payload_off;
            let mut offsets = Vec::new();
            for (fi, t) in tys.iter().enumerate() {
                let a = ck.tcx.align_of(t).max(1);
                let sz = ck.tcx.size_of(t);
                off = round_up(off, a);
                offsets.push(off);
                fields.push(Field {
                    name: format!("__v{}_{}", vi, fi),
                    ty: t.clone(),
                    offset: off,
                });
                off += sz;
            }
            if off > max_end {
                max_end = off;
            }
            if let Some(v) = d.variants.get_mut(vi) {
                v.fields = tys.clone();
                v.offsets = offsets;
            }
        }
        let align = payload_align.max(4);
        let size = round_up(max_end, align).max(align);
        d.size = size;
        d.align = align;
        if let Some(sd) = ck.tcx.structs.get_mut(d.struct_idx) {
            *sd = StructDef {
                name: d.name.clone(),
                fields,
                size,
                align,
                            must_consume: false,
            };
        }
        REG.with(|r| {
            let mut reg = r.borrow_mut();
            if let Some(slot) = reg.enums.get_mut(i) {
                *slot = d.clone();
            }
        });
    }
}

/// Name of a type, if it is contained BY VALUE (pointers are not).
fn value_named(te: &TypeExpr) -> Option<String> {
    match te {
        TypeExpr::Named(n, _) => Some(n.clone()),
        TypeExpr::Array { elem, .. } => value_named(elem),
        TypeExpr::Ptr { .. } => None,
        // Round 58: a function value is one word, not a struct by value.
        TypeExpr::Fn { .. } => None,
    }
}

fn toposort(i: usize, deps: &[Vec<usize>], state: &mut Vec<u8>, order: &mut Vec<usize>, bad: &mut Vec<usize>) {
    match state.get(i) {
        Some(2) | None => return,
        Some(1) => {
            if !bad.contains(&i) {
                bad.push(i);
            }
            return;
        }
        _ => {}
    }
    state[i] = 1;
    for j in deps.get(i).map(|v| v.as_slice()).unwrap_or(&[]) {
        toposort(*j, deps, state, order, bad);
        if bad.contains(j) && !bad.contains(&i) {
            bad.push(i);
        }
    }
    state[i] = 2;
    order.push(i);
}

// -------------------------------------------------------------- Type check

/// `// HOOK types` in `sema::stmt_returns`: does a `match` return on
/// every path? The pattern match has been checked by that point, so it is
/// exhaustive — it suffices that every case returns.
pub(crate) fn match_returns(e: &Expr) -> bool {
    let idx = match &e.kind {
        ExprKind::Call(name, _, _) => match match_index_of(name) {
            Some(i) => i,
            None => return false,
        },
        _ => return false,
    };
    let mi = match match_info(idx) {
        Some(m) => m,
        None => return false,
    };
    !mi.arms.is_empty() && mi.arms.iter().all(|a| block_returns(&a.body))
}

fn block_returns(b: &Block) -> bool {
    b.stmts.iter().any(stmt_returns)
}

fn stmt_returns(s: &Stmt) -> bool {
    match s {
        Stmt::Return { .. } => true,
        Stmt::Block(b) => block_returns(b),
        Stmt::If { then, els, .. } => match els {
            Some(e) => block_returns(then) && stmt_returns(e),
            None => false,
        },
        Stmt::Expr(e) => match_returns(e),
        _ => false,
    }
}

/// `// HOOK types` in `sema::call`: catches `Enum::Variant(..)` and `match`.
pub(crate) fn hook_call(
    ck: &mut Checker,
    name: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Option<Type> {
    if let Some(idx) = match_index_of(name) {
        check_match(ck, idx, espan);
        return Some(Type::Void);
    }
    if let Some((ename, vname)) = name.split_once("::") {
        return Some(check_ctor(ck, ename, vname, args, nspan, espan));
    }
    None
}

fn check_ctor(
    ck: &mut Checker,
    ename: &str,
    vname: &str,
    args: &[Expr],
    nspan: Span,
    espan: Span,
) -> Type {
    let def = match enum_by_name(ename) {
        Some(d) => d,
        None => {
            for a in args {
                ck.type_out_expr(a);
            }
            ck.dg
                .error(nspan, format!("unknown enum '{}'", ename));
            return Type::Error;
        }
    };
    let v = match def.variant(vname) {
        Some(v) => v.clone(),
        None => {
            for a in args {
                ck.type_out_expr(a);
            }
            ck.dg.error_note(
                nspan,
                format!("enum '{}' has no variant '{}'", ename, vname),
                format!("known are: {}", variant_list(&def)),
            );
            return Type::Error;
        }
    };
    if args.len() != v.fields.len() {
        ck.dg.error(
            espan,
            format!(
                "variant '{}::{}' expects {} payload value(s), found {}",
                ename,
                vname,
                v.fields.len(),
                args.len()
            ),
        );
    }
    for (i, a) in args.iter().enumerate() {
        match v.fields.get(i) {
            Some(want) => {
                let got = ck.expr(a, Some(want));
                if !assignable(&got, want) {
                    ck.dg.error(
                        a.span,
                        format!(
                            "payload value {} of '{}::{}' has type {}, expected {}",
                            i + 1,
                            ename,
                            vname,
                            ck.tcx.name_of(&got),
                            ck.tcx.name_of(want)
                        ),
                    );
                }
            }
            None => ck.type_out_expr(a),
        }
    }
    Type::Struct(def.struct_idx)
}

fn variant_list(def: &EnumDef) -> String {
    def.variants
        .iter()
        .map(|v| v.name.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

fn assignable(got: &Type, want: &Type) -> bool {
    if got.is_error() || want.is_error() {
        return true;
    }
    match (got, want) {
        (Type::Ptr { inner: x, .. }, Type::Ptr { inner: y, .. }) => assignable(x, y),
        _ => got == want,
    }
}

fn check_match(ck: &mut Checker, idx: usize, espan: Span) {
    let mi = match match_info(idx) {
        Some(m) => m,
        None => {
            ck.dg
                .error(espan, "internal error: unknown pattern match");
            return;
        }
    };
    let sty = ck.expr(&mi.subject, None);
    let subject = classify_subject(ck, &sty, mi.subject.span);

    // 1. check patterns, create bindings, check the body
    for arm in &mi.arms {
        ck.scopes.push(HashMap::new());
        check_pattern(ck, &arm.pat, &subject_type(&subject, &sty), &subject, true);
        ck.check_block(&arm.body, true);
        ck.scopes.pop();
    }

    // 2. reachability: after a case that always matches nothing follows
    let mut catchall: Option<Span> = None;
    for arm in &mi.arms {
        if let Some(prev) = catchall {
            ck.dg.error_note(
                arm.span,
                "this arm is unreachable",
                format!(
                    "an earlier arm in line {} always matches",
                    prev.line
                ),
            );
            break;
        }
        if arm.pat.is_irrefutable() {
            catchall = Some(arm.pat.span());
        }
    }

    // 3. exhaustiveness — ERROR, not a warning (SPEC §6.3)
    if let Err(d) = check_exhaustive(&subject, &mi.arms, mi.span) {
        match d.note {
            Some(note) => ck.dg.error_note(d.span, d.msg, note),
            None => ck.dg.error(d.span, d.msg),
        }
    }
}

fn subject_type(s: &Subject, fallback: &Type) -> Type {
    match s {
        Subject::Enum(d) => Type::Struct(d.struct_idx),
        Subject::Bool => Type::Bool,
        Subject::Int(t) => t.clone(),
        Subject::Bad => fallback.clone(),
    }
}

fn classify_subject(ck: &mut Checker, ty: &Type, span: Span) -> Subject {
    if ty.is_error() {
        return Subject::Bad;
    }
    if let Type::Struct(i) = ty {
        if let Some(d) = enum_by_struct(*i) {
            return Subject::Enum(d);
        }
    }
    if *ty == Type::Bool {
        return Subject::Bool;
    }
    if ty.is_concrete_int() {
        return Subject::Int(ty.clone());
    }
    if *ty == Type::UntypedInt {
        ck.dg.error_note(
            span,
            "the type of the integer expression in 'match' cannot be inferred",
            "write e.g. 'x as i32'",
        );
        return Subject::Bad;
    }
    ck.dg.error(
        span,
        format!(
            "'match' works on enums, integers and bool, not on {}",
            ck.tcx.name_of(ty)
        ),
    );
    Subject::Bad
}

/// Checks a pattern against the expected type and creates its bindings.
fn check_pattern(ck: &mut Checker, pat: &Pattern, ty: &Type, subject: &Subject, top: bool) {
    match pat {
        Pattern::Wild(_) => {}
        Pattern::Bind(name, span) => {
            ck.declare_var(name, ty.clone(), false, *span);
        }
        Pattern::Bool(_, span) => {
            if !matches!(ty, Type::Bool | Type::Error) {
                ck.dg.error(
                    *span,
                    format!(
                        "pattern 'true'/'false' does not fit the type {}",
                        ck.tcx.name_of(ty)
                    ),
                );
            }
        }
        Pattern::Int(v, span) => {
            if !ty.is_concrete_int() && !ty.is_error() {
                ck.dg.error(
                    *span,
                    format!("number pattern does not fit the type {}", ck.tcx.name_of(ty)),
                );
            } else if !fits(*v, ty) {
                ck.dg.error(
                    *span,
                    format!("number {} does not fit into the type {}", v, ck.tcx.name_of(ty)),
                );
            }
        }
        Pattern::Range { lo, hi, span, .. } => {
            if !ty.is_concrete_int() && !ty.is_error() {
                ck.dg.error(
                    *span,
                    format!("range pattern does not fit the type {}", ck.tcx.name_of(ty)),
                );
            } else if !fits(*lo, ty) || !fits(*hi, ty) {
                ck.dg.error(
                    *span,
                    format!("the range bounds do not fit into the type {}", ck.tcx.name_of(ty)),
                );
            }
        }
        Pattern::Variant { ename, vname, subs, span } => {
            // determine the enum of the pattern
            let def = match ename {
                Some(n) => match enum_by_name(n) {
                    Some(d) => Some(d),
                    None => {
                        ck.dg.error(*span, format!("unknown enum '{}'", n));
                        None
                    }
                },
                None => match ty {
                    Type::Struct(i) => enum_by_struct(*i),
                    _ => None,
                },
            };
            let def = match def {
                Some(d) => d,
                None => return,
            };
            let want = Type::Struct(def.struct_idx);
            if !ty.is_error() && *ty != want {
                ck.dg.error(
                    *span,
                    format!(
                        "pattern of the enum '{}' does not fit the type {}",
                        def.name,
                        ck.tcx.name_of(ty)
                    ),
                );
                return;
            }
            if top {
                if let Subject::Enum(sd) = subject {
                    if sd.name != def.name {
                        ck.dg.error(
                            *span,
                            format!(
                                "pattern of the enum '{}' does not fit the type '{}'",
                                def.name, sd.name
                            ),
                        );
                        return;
                    }
                }
            }
            let v = match def.variant(vname) {
                Some(v) => v.clone(),
                None => {
                    ck.dg.error_note(
                        *span,
                        format!("enum '{}' has no variant '{}'", def.name, vname),
                        format!("known are: {}", variant_list(&def)),
                    );
                    return;
                }
            };
            if subs.len() != v.fields.len() {
                ck.dg.error(
                    *span,
                    format!(
                        "pattern '{}::{}' expects {} subpatterns, found {}",
                        def.name,
                        vname,
                        v.fields.len(),
                        subs.len()
                    ),
                );
            }
            for (i, sp) in subs.iter().enumerate() {
                if let Some(ft) = v.fields.get(i) {
                    check_pattern(ck, sp, ft, subject, false);
                }
            }
        }
    }
}

fn fits(v: i128, t: &Type) -> bool {
    let (lo, hi): (i128, i128) = match t {
        Type::I8 => (-128, 127),
        Type::I16 => (-32768, 32767),
        Type::I32 => (-2147483648, 2147483647),
        Type::I64 | Type::Isize => (i64::MIN as i128, i64::MAX as i128),
        Type::U8 => (0, 255),
        Type::U16 => (0, 65535),
        Type::U32 => (0, 4294967295),
        Type::U64 | Type::Usize => (0, u64::MAX as i128),
        _ => return true,
    };
    v >= lo && v <= hi
}

/// **Exhaustiveness check at compile time** (SPEC §6.3).
///
/// Yields `Err(Diag)` with line/column and the name of the missing variant
/// when the pattern match leaves a case uncovered. That is a hard error, not
/// a warning — the caller reports it through `Diags`.
pub fn check_exhaustive(subject: &Subject, arms: &[Arm], span: Span) -> Result<(), Diag> {
    let has_catchall = arms.iter().any(|a| a.pat.is_irrefutable());
    match subject {
        Subject::Bad => Ok(()),
        Subject::Enum(def) => {
            if has_catchall {
                return Ok(());
            }
            let mut missing: Vec<String> = Vec::new();
            for v in &def.variants {
                let covered = arms.iter().any(|a| match &a.pat {
                    Pattern::Variant { vname, subs, .. } => {
                        *vname == v.name && subs.iter().all(|s| s.is_irrefutable())
                    }
                    _ => false,
                });
                if !covered {
                    missing.push(format!("{}::{}", def.name, v.name));
                }
            }
            if missing.is_empty() {
                return Ok(());
            }
            let list = missing.join(", ");
            Err(Diag {
                msg: format!(
                    "'match' is not exhaustive: {} not covered",
                    if missing.len() == 1 {
                        format!("the variant {} is", list)
                    } else {
                        format!("the variants {} are", list)
                    }
                ),
                span,
                label: "here".to_string(),
                note: Some(format!(
                    "add an arm '{} => {{ }}' or '_ => {{ }}'",
                    missing[0]
                )),
                help: None,
            })
        }
        Subject::Bool => {
            if has_catchall {
                return Ok(());
            }
            let has = |b: bool| {
                arms.iter()
                    .any(|a| matches!(&a.pat, Pattern::Bool(x, _) if *x == b))
            };
            let mut missing = Vec::new();
            if !has(true) {
                missing.push("true");
            }
            if !has(false) {
                missing.push("false");
            }
            if missing.is_empty() {
                return Ok(());
            }
            Err(Diag {
                msg: format!(
                    "'match' is not exhaustive: the arm {} is missing",
                    missing.join(" and ")
                ),
                span,
                label: "here".to_string(),
                note: Some("add the missing arm or '_ => { }'".to_string()),
                help: None,
            })
        }
        Subject::Int(t) => {
            if has_catchall {
                return Ok(());
            }
            Err(Diag {
                msg: format!(
                    "'match' over {} is not exhaustive: an arm for all remaining values is missing",
                    type_name(t)
                ),
                span,
                label: "here".to_string(),
                note: Some("add '_ => { }'".to_string()),
                help: None,
            })
        }
    }
}

fn type_name(t: &Type) -> &'static str {
    match t {
        Type::I8 => "i8",
        Type::I16 => "i16",
        Type::I32 => "i32",
        Type::I64 => "i64",
        Type::U8 => "u8",
        Type::U16 => "u16",
        Type::U32 => "u32",
        Type::U64 => "u64",
        Type::Usize => "usize",
        Type::Isize => "isize",
        _ => "integers",
    }
}

#[cfg(test)]
mod tests {
    use crate::diag::Diags;

    fn compile(src: &str) -> (String, bool) {
        let mut dg = Diags::new("test.fi", src);
        let toks = crate::lexer::lex(src, &mut dg);
        let mut prog = crate::parser::parse(&toks, &mut dg);
        crate::mono::expand(&mut prog, &mut dg);
        if dg.has_errors() {
            return (dg.render(), false);
        }
        let info = match crate::sema::check(&prog, &mut dg) {
            Some(i) => i,
            None => return (dg.render(), false),
        };
        if dg.has_errors() {
            return (dg.render(), false);
        }
        match crate::lower::lower(&prog, &info, &mut dg) {
            Some(_) => (dg.render(), !dg.has_errors()),
            None => (dg.render(), false),
        }
    }

    #[test]
    fn missing_variant_is_in_error_with_names() {
        let src = "\
enum T { A, B(i32), C }
fn main() -> i32 {
    let t = T::B(3 as i32)
    match t {
        T::A => { }
        T::B(x) => { }
    }
    return 0 as i32
}
";
        let (out, ok) = compile(src);
        assert!(!ok, "an inexhaustive match must be an error:\n{}", out);
        assert!(out.contains("not exhaustive"), "{}", out);
        assert!(out.contains("T::C"), "{}", out);
    }

    #[test]
    fn complete_match_compiled() {
        let src = "\
enum T { A, B(i32) }
fn main() -> i32 {
    var r: i32 = 0 as i32
    let t = T::B(7 as i32)
    match t {
        T::A => { r = 1 as i32 }
        T::B(x) => { r = x }
    }
    return r
}
";
        let (out, ok) = compile(src);
        assert!(ok, "{}", out);
    }

    #[test]
    fn int_match_needs_a_catch_arm() {
        let src = "\
fn main() -> i32 {
    let n: i32 = 3 as i32
    match n {
        0 => { }
        1 => { }
    }
    return 0 as i32
}
";
        let (out, ok) = compile(src);
        assert!(!ok, "{}", out);
        assert!(out.contains("not exhaustive"), "{}", out);
    }

    #[test]
    fn layout_tag_and_payload() {
        let src = "\
enum T { A, B(i64) }
fn main() -> i32 { return 0 as i32 }
";
        let mut dg = Diags::new("test.fi", src);
        let toks = crate::lexer::lex(src, &mut dg);
        let prog = crate::parser::parse(&toks, &mut dg);
        let info = crate::sema::check(&prog, &mut dg).expect("type check");
        let idx = info.tcx.lookup("T").expect("enum T");
        let sd = &info.tcx.structs[idx];
        assert_eq!(sd.field("__tag").expect("tag").offset, 0);
        assert_eq!(sd.field("__v1_0").expect("payload").offset, 8);
        assert_eq!(sd.size, 16);
        assert_eq!(sd.align, 8);
    }
}
