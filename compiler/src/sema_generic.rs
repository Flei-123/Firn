// SPDX-License-Identifier: MPL-2.0
//! Generic templates (`L5`): capture at the parser, naming scheme, bounds.
//!
//! This file belongs to the module `types`. Generics get **monomorphized**
//! (see `mono.rs`): for every type combination used, a separate, fully
//! concrete function or a separate struct comes about. The naming scheme is
//! contract (debugger, inlining, tests):
//!
//! ```text
//! name__T1_T2      e.g.   vec_push__i32, Vec__ptr_u8, Map__u32_i64
//! ```
//!
//! Syntax (square brackets, so `<` stays unambiguously comparison — SPEC §12):
//!
//! ```firn
//! struct Vec[T] { data: *mut T, len: usize, cap: usize }
//! fn sum[T: Int](a: T, b: T) -> T { return a + b }
//! let s = sum[i32](1 as i32, 2 as i32)
//! var v: Vec[i32] = Vec[i32]{ data: p, len: 0 as usize, cap: 0 as usize }
//! ```
//!
//! Bounds (`T: Int`) are checked during monomorphization; a `T` left
//! unsatisfied is a plain error with line and column.

use std::cell::RefCell;
use crate::fx::HashMap;

use crate::ast::{Expr, ExprKind, FnDecl, StructDecl, TypeExpr};
use crate::diag::Span;
use crate::lexer::{TokKind, Token};
use crate::parser::Parser;

/// Bound on a type parameter — `T: Int`, `T: Ord`, `T: Int + Ord`.
///
/// `Any`, `Int` and `Scalar` are the three BUILTIN bounds (round 30). Any
/// other name is the name of an INTERFACE (round 50). Whether that interface
/// exists is not yet settled while parsing: `interface Ord` may stand
/// further down or in another file. That is why the check happens at
/// INSTANTIATION (`mono.rs`) — where the concrete type is known and the
/// message can say which method is missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Bound {
    /// no bound
    Any,
    /// integer type
    Int,
    /// integer, bool or pointer
    Scalar,
    /// `T: I` — the type must implement the interface `I`
    Iface(String),
}

impl Bound {
    pub(crate) fn name(&self) -> &str {
        match self {
            Bound::Any => "Any",
            Bound::Int => "Int",
            Bound::Scalar => "Scalar",
            Bound::Iface(n) => n.as_str(),
        }
    }
    /// No `Option`: an unknown name is no faulty input but the name of an
    /// interface. A typo is reported at instantiation as
    /// "unknown interface" — together with the list of the known ones.
    fn parse(name: &str) -> Bound {
        match name {
            "Any" => Bound::Any,
            "Int" => Bound::Int,
            "Scalar" => Bound::Scalar,
            _ => Bound::Iface(name.to_string()),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TyParam {
    pub(crate) name: String,
    /// empty list = no bound (`[T]`)
    pub(crate) bounds: Vec<Bound>,
}

#[derive(Clone, Debug)]
pub(crate) struct FnTemplate {
    pub(crate) params: Vec<TyParam>,
    pub(crate) decl: FnDecl,
}

#[derive(Clone, Debug)]
pub(crate) struct StructTemplate {
    pub(crate) params: Vec<TyParam>,
    pub(crate) decl: StructDecl,
}

/// One type combination that was used (`Vec[i32]`, `sum[u8]`).
#[derive(Clone, Debug)]
pub(crate) struct Instantiation {
    pub(crate) base: String,
    pub(crate) args: Vec<TypeExpr>,
    pub(crate) span: Span,
    /// written down inside a template (may contain type parameters)
    pub(crate) is_abstract: bool,
    pub(crate) is_fn: bool,
}

#[derive(Default)]
struct Registry {
    fn_names: Vec<String>,
    struct_names: Vec<String>,
    fns: HashMap<String, FnTemplate>,
    structs: HashMap<String, StructTemplate>,
    /// Round MODGEN: instantiation names the module renamer replaced by a
    /// qualified one (`Vec__Block` -> `Vec__refdes__Block`) ...
    superseded: crate::fx::HashSet<String>,
    /// ... and names it saw in use unchanged. A superseded name that no file
    /// still uses is not instantiated: its argument names a type that exists
    /// only under the qualified name.
    kept: crate::fx::HashSet<String>,
    insts: HashMap<String, Instantiation>,
    order: Vec<String>,
    /// nesting depth while parsing a template
    in_template: u32,
    /// qualified generic calls `mod.fn[T](..)`, checked by `modules.rs`
    qual_uses: Vec<QualUse>,
}

/// One qualified generic call as written in a source file.
#[derive(Clone, Debug)]
pub(crate) struct QualUse {
    pub(crate) file: u32,
    pub(crate) alias: String,
    pub(crate) name: String,
    pub(crate) span: Span,
    /// r707: a generic STRUCT (`mod.Box[i32]`), not a function
    pub(crate) is_struct: bool,
}

pub(crate) fn record_qual_use(u: QualUse) {
    REG.with(|r| r.borrow_mut().qual_uses.push(u));
}

pub(crate) fn qual_uses() -> Vec<QualUse> {
    REG.with(|r| r.borrow().qual_uses.clone())
}

/// Does file `file` declare a generic function template `name`?
pub(crate) fn file_has_fn_template(file: u32, name: &str) -> bool {
    REG.with(|r| {
        r.borrow()
            .fns
            .get(name)
            .map(|t| t.decl.span.file == file)
            .unwrap_or(false)
    })
}

/// Does file `file` declare a generic struct template `name`?
pub(crate) fn file_has_struct_template(file: u32, name: &str) -> bool {
    REG.with(|r| {
        r.borrow()
            .structs
            .get(name)
            .map(|t| t.decl.span.file == file)
            .unwrap_or(false)
    })
}

thread_local! {
    static REG: RefCell<Registry> = RefCell::new(Registry::default());
}

pub(crate) fn reset() {
    REG.with(|r| *r.borrow_mut() = Registry::default());
}

pub(crate) fn fn_template(name: &str) -> Option<FnTemplate> {
    REG.with(|r| r.borrow().fns.get(name).cloned())
}

/// Names of all generic templates declared in file `file`.
///
/// Needed by `modules.rs`: the templates do NOT sit in `Program::funcs`
/// but here — module rewriting therefore never reached them, and a template
/// saw the names of the root file only
/// (docs/SELF_HOSTING.md §7, blocker B2).
pub(crate) fn fn_templates_the_file(file: u32) -> Vec<String> {
    REG.with(|r| {
        r.borrow()
            .fns
            .iter()
            .filter(|(_, t)| t.decl.span.file == file)
            .map(|(n, _)| n.clone())
            .collect()
    })
}

pub(crate) fn struct_templates_the_file(file: u32) -> Vec<String> {
    REG.with(|r| {
        r.borrow()
            .structs
            .iter()
            .filter(|(_, t)| t.decl.span.file == file)
            .map(|(n, _)| n.clone())
            .collect()
    })
}

/// Changes a function template in place.
///
/// The template is taken OUT of the registry while `f` runs, so that `f`
/// may itself consult the registry (the module renamer looks up and records
/// instantiations, round MODGEN) without a double borrow.
pub(crate) fn with_fn_template<F: FnOnce(&mut crate::ast::FnDecl)>(name: &str, f: F) {
    let taken = REG.with(|r| r.borrow_mut().fns.remove(name));
    if let Some(mut t) = taken {
        f(&mut t.decl);
        REG.with(|r| {
            r.borrow_mut().fns.insert(name.to_string(), t);
        });
    }
}

/// Changes a struct template in place.
pub(crate) fn with_struct_template<F: FnOnce(&mut crate::ast::StructDecl)>(name: &str, f: F) {
    let taken = REG.with(|r| r.borrow_mut().structs.remove(name));
    if let Some(mut t) = taken {
        f(&mut t.decl);
        REG.with(|r| {
            r.borrow_mut().structs.insert(name.to_string(), t);
        });
    }
}

pub(crate) fn struct_template(name: &str) -> Option<StructTemplate> {
    REG.with(|r| r.borrow().structs.get(name).cloned())
}

/// Is the body of a generic template being parsed right now?
pub(crate) fn in_template() -> bool {
    REG.with(|r| r.borrow().in_template > 0)
}

pub(crate) fn is_generic_fn(name: &str) -> bool {
    REG.with(|r| r.borrow().fn_names.iter().any(|n| n == name))
}

pub(crate) fn is_generic_struct(name: &str) -> bool {
    REG.with(|r| r.borrow().struct_names.iter().any(|n| n == name))
}

/// All uses captured while parsing, in order of their appearance.
pub(crate) fn instantiations() -> Vec<(String, Instantiation)> {
    REG.with(|r| {
        let reg = r.borrow();
        reg.order
            .iter()
            .filter(|k| !reg.superseded.contains(*k) || reg.kept.contains(*k))
            .filter_map(|k| reg.insts.get(k).map(|i| (k.clone(), i.clone())))
            .collect()
    })
}

/// Round MODGEN: see `Registry::superseded`.
pub(crate) fn mark_superseded(mangled: &str) {
    REG.with(|r| {
        r.borrow_mut().superseded.insert(mangled.to_string());
    });
}

/// Round MODGEN: see `Registry::kept`.
pub(crate) fn mark_kept(mangled: &str) {
    REG.with(|r| {
        r.borrow_mut().kept.insert(mangled.to_string());
    });
}

pub(crate) fn instantiation(mangled: &str) -> Option<Instantiation> {
    REG.with(|r| r.borrow().insts.get(mangled).cloned())
}

pub(crate) fn record_inst(mangled: &str, inst: Instantiation) {
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        if !reg.insts.contains_key(mangled) {
            reg.order.push(mangled.to_string());
            reg.insts.insert(mangled.to_string(), inst);
        } else if !inst.is_abstract {
            // The first sighting may have been written inside a generic
            // template (`vec_set[u8]` in a module imported earlier) and
            // is flagged abstract, so the expansion skips it. A later
            // sighting outside any template (concrete use) must win,
            // else the result depends on the import order.
            if let Some(old) = reg.insts.get_mut(mangled) {
                old.is_abstract = false;
            }
        }
    });
}

// ----------------------------------------------------------- Naming scheme

/// Text form of a type for the naming scheme `name__T1_T2`.
pub(crate) fn type_tag(te: &TypeExpr) -> String {
    match te {
        TypeExpr::Named(n, _) => n.clone(),
        TypeExpr::Ptr { mutable, inner, .. } => {
            format!("{}{}", if *mutable { "ptrmut_" } else { "ptr_" }, type_tag(inner))
        }
        TypeExpr::Array { elem, len, .. } => format!("arr{}_{}", len, type_tag(elem)),
        // Round 58: a function type in the naming scheme. Round brackets and
        // arrows are no identifier characters, so the shape gets spelled out:
        // `fn2_i32_i32_to_bool`.
        TypeExpr::Fn { params, ret, .. } => {
            let mut s = format!("fn{}", params.len());
            for p in params {
                s.push('_');
                s.push_str(&type_tag(p));
            }
            s.push_str("_to_");
            match ret {
                Some(t) => s.push_str(&type_tag(t)),
                None => s.push_str("void"),
            }
            s
        }
        // Round TUPLES: `(i32, u8)` is `tup2_i32_u8` in the naming scheme.
        TypeExpr::Tuple(elems, _) => {
            let mut s = format!("tup{}", elems.len());
            for e in elems {
                s.push('_');
                s.push_str(&type_tag(e));
            }
            s
        }
    }
}

/// `name__T1_T2` (contract).
pub(crate) fn mangle(base: &str, args: &[TypeExpr]) -> String {
    let mut s = String::from(base);
    s.push_str("__");
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            s.push('_');
        }
        s.push_str(&type_tag(a));
    }
    s
}

// ------------------------------------------------------------- Parser hooks

impl<'a> Parser<'a> {
    /// `[T, U: Int]` — type parameter list of a template.
    fn generic_params(&mut self) -> Option<Vec<TyParam>> {
        if !self.expect(TokKind::LBracket, "at the start of the type parameter list") {
            return None;
        }
        let mut out: Vec<TyParam> = Vec::new();
        loop {
            if self.at(&TokKind::RBracket) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let (name, span) = self.ident("for a type parameter")?;
            // `T: A + B + C` — the bounds stand one after another with `+` and
            // ALL hold at the same time (round 50).
            let mut bounds: Vec<Bound> = Vec::new();
            if self.eat(&TokKind::Colon) {
                loop {
                    let (bname, bspan) = self.ident("for a bound on the type parameter")?;
                    let b = Bound::parse(&bname);
                    if bounds.contains(&b) {
                        self.dg.error_note(
                            bspan,
                            format!("the bound '{}' appears twice on '{}'", bname, name),
                            "each bound is named at most once",
                        );
                    } else {
                        bounds.push(b);
                    }
                    if !self.eat(&TokKind::Plus) {
                        break;
                    }
                }
            }
            if out.iter().any(|p| p.name == name) {
                self.dg
                    .error(span, format!("type parameter '{}' is already declared", name));
            } else {
                let _ = span;
                out.push(TyParam { name, bounds });
            }
            if !self.eat(&TokKind::Comma) {
                break;
            }
            if self.pos == before {
                self.bump();
            }
        }
        self.close(TokKind::RBracket, "after the type parameter list");
        if out.is_empty() {
            self.error_here("a type parameter list needs at least one parameter");
            self.recovering = false;
            return None;
        }
        Some(out)
    }

    /// `[i32, u8]` — type arguments at a use site.
    fn generic_args(&mut self) -> Option<Vec<TypeExpr>> {
        if !self.expect(TokKind::LBracket, "at the start of the type arguments") {
            return None;
        }
        let mut out = Vec::new();
        loop {
            if self.at(&TokKind::RBracket) || self.at_eof() {
                break;
            }
            let before = self.pos;
            out.push(self.parse_type()?);
            if !self.eat(&TokKind::Comma) {
                break;
            }
            if self.pos == before {
                self.bump();
            }
        }
        self.close(TokKind::RBracket, "after the type arguments");
        if out.is_empty() {
            self.error_here("the type arguments are missing");
            self.recovering = false;
            return None;
        }
        Some(out)
    }

    fn generic_fn_template(&mut self) {
        let start = self.span();
        self.bump(); // 'fn'
        let (name, nspan) = match self.ident("after 'fn'") {
            Some(x) => x,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        let tparams = match self.generic_params() {
            Some(p) => p,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        REG.with(|r| r.borrow_mut().in_template += 1);
        let decl = self.rest_of_fn(name.clone(), start);
        REG.with(|r| r.borrow_mut().in_template -= 1);
        let decl = match decl {
            Some(d) => d,
            None => return,
        };
        let duplicate = REG.with(|r| {
            let mut reg = r.borrow_mut();
            if reg.fns.contains_key(&name) {
                return true;
            }
            reg.fns.insert(name.clone(), FnTemplate { params: tparams, decl });
            if !reg.fn_names.contains(&name) {
                reg.fn_names.push(name.clone());
            }
            false
        });
        if duplicate {
            self.dg.error(
                nspan,
                format!("generic function '{}' is already declared", name),
            );
        }
    }

    fn rest_of_fn(&mut self, name: String, start: Span) -> Option<FnDecl> {
        if !self.expect(TokKind::LParen, "after the function name") {
            self.recovering = false;
            self.sync_item();
            return None;
        }
        self.allow_defaults = true; // r704: generic templates may have defaults
        let (params, ref_params) = self.params_ref();
        self.allow_defaults = false;
        self.close(TokKind::RParen, "after the parameter list");
        self.recovering = false;
        let ret = if self.eat(&TokKind::Arrow) {
            match self.parse_type() {
                Some(t) => Some(t),
                None => {
                    self.recovering = false;
                    self.sync_item();
                    return None;
                }
            }
        } else {
            None
        };
        if !self.at(&TokKind::LBrace) {
            self.error_here(format!(
                "expected '{{' at the start of the function body, found '{}'",
                self.kind().text()
            ));
            self.recovering = false;
            self.sync_item();
            return None;
        }
        let saved_ret = std::mem::replace(&mut self.cur_ret, ret.clone());
        let mut body = self.block("at the start of the function body");
        self.cur_ret = saved_ret;
        self.recovering = false;
        self.finish_body(&ref_params, &mut body);
        Some(FnDecl { name, params, ret, body, span: start, attrs: Vec::new(), extern_info: None })
    }

    fn generic_struct_template(&mut self) {
        let start = self.span();
        self.bump(); // 'struct'
        let (name, nspan) = match self.ident("after 'struct'") {
            Some(x) => x,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        let tparams = match self.generic_params() {
            Some(p) => p,
            None => {
                self.recovering = false;
                self.sync_item();
                return;
            }
        };
        if !self.expect(TokKind::LBrace, "after the structure name") {
            self.recovering = false;
            self.sync_item();
            return;
        }
        REG.with(|r| r.borrow_mut().in_template += 1);
        let mut fields: Vec<(String, TypeExpr, Span)> = Vec::new();
        loop {
            while self.eat(&TokKind::Comma) || self.eat(&TokKind::Semi) {}
            if self.at(&TokKind::RBrace) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let (fname, fspan) = match self.ident("for a field") {
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
            if self.pos == before {
                self.bump();
            }
        }
        let end = self.span();
        self.close(TokKind::RBrace, "at the end of the structure declaration");
        self.recovering = false;
        REG.with(|r| r.borrow_mut().in_template -= 1);
        let decl =
            StructDecl { name: name.clone(), fields, span: Parser::join(start, end), attrs: Vec::new() };
        let duplicate = REG.with(|r| {
            let mut reg = r.borrow_mut();
            if reg.structs.contains_key(&name) {
                return true;
            }
            reg.structs
                .insert(name.clone(), StructTemplate { params: tparams, decl });
            if !reg.struct_names.contains(&name) {
                reg.struct_names.push(name.clone());
            }
            false
        });
        if duplicate {
            self.dg.error(
                nspan,
                format!("generic struct '{}' is already declared", name),
            );
        }
    }

    /// Lookahead: at `[`, is the bracket group followed by `(`? Tells a
    /// generic call `m.f[T](..)` from an index `m.arr[i]`.
    fn bracket_then_paren(&self) -> bool {
        if !self.at(&TokKind::LBracket) {
            return false;
        }
        let mut depth = 0i32;
        let mut i = self.pos;
        while let Some(t) = self.toks.get(i) {
            match t.kind {
                TokKind::LBracket => depth += 1,
                TokKind::RBracket => {
                    depth -= 1;
                    if depth == 0 {
                        // r729: the `(` must belong to the same line (or
                        // sit inside round brackets): `m.TABLE[i]` followed
                        // by a line that starts with `(*p).f = 0` is an index.
                        let same_line = match (self.toks.get(i), self.toks.get(i + 1)) {
                            (Some(a), Some(b)) => b.span.line == a.span.line,
                            _ => false,
                        };
                        return matches!(
                            self.toks.get(i + 1).map(|t| &t.kind),
                            Some(TokKind::LParen)
                        ) && (same_line || self.paren_depth > 0);
                    }
                }
                TokKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    fn note_inst(&mut self, base: &str, args: Vec<TypeExpr>, span: Span, is_fn: bool) -> String {
        let mangled = mangle(base, &args);
        let is_abstract = REG.with(|r| r.borrow().in_template > 0);
        record_inst(
            &mangled,
            Instantiation { base: base.to_string(), args, span, is_abstract, is_fn },
        );
        mangled
    }
}

/// Advance search through the token stream: which names are generic
/// templates? That makes use BEFORE the declaration work too.
pub(crate) fn hook_prescan(toks: &[Token]) {
    let mut fns: Vec<String> = Vec::new();
    let mut sts: Vec<String> = Vec::new();
    for i in 0..toks.len() {
        let k = &toks[i].kind;
        let is_fn = matches!(k, TokKind::KwFn);
        let is_st = matches!(k, TokKind::KwStruct);
        if !is_fn && !is_st {
            continue;
        }
        let name = match toks.get(i + 1).map(|t| &t.kind) {
            Some(TokKind::Ident(n)) => n.clone(),
            _ => continue,
        };
        if !matches!(toks.get(i + 2).map(|t| &t.kind), Some(TokKind::LBracket)) {
            continue;
        }
        if is_fn {
            fns.push(name);
        } else {
            sts.push(name);
        }
    }
    // Additive: with several source files (modules.rs) the names of every
    // file join; the reset happens once per compilation only.
    REG.with(|r| {
        let mut reg = r.borrow_mut();
        for n in fns {
            if !reg.fn_names.contains(&n) {
                reg.fn_names.push(n);
            }
        }
        for n in sts {
            if !reg.struct_names.contains(&n) {
                reg.struct_names.push(n);
            }
        }
    });
}

/// `// HOOK types` in `parser.rs::program` (via `sema_match::hook_item`).
pub(crate) fn hook_item(p: &mut Parser) -> bool {
    let name = match p.toks.get(p.pos + 1).map(|t| t.kind.clone()) {
        Some(TokKind::Ident(n)) => n,
        _ => return false,
    };
    let _ = name;
    if !matches!(p.toks.get(p.pos + 2).map(|t| &t.kind), Some(TokKind::LBracket)) {
        return false;
    }
    match p.kind() {
        TokKind::KwFn => {
            p.generic_fn_template();
            true
        }
        TokKind::KwStruct => {
            p.generic_struct_template();
            true
        }
        _ => false,
    }
}

/// `// HOOK types` in `parser.rs::parse_type_inner` — `Vec[i32]`.
pub(crate) fn hook_generic_type(p: &mut Parser, name: &str, sp: Span) -> Option<TypeExpr> {
    if !p.at(&TokKind::LBracket) || !is_generic_struct(name) {
        return None;
    }
    let args = p.generic_args()?;
    let mangled = p.note_inst(name, args, sp, false);
    Some(TypeExpr::Named(mangled, sp))
}

/// r707: `mod.Box[..]` ahead? Yields `(member, span of "mod.Box")` and records
/// the use for `modules.rs`; the cursor is left AT the `[`.
fn qual_struct_head(p: &mut Parser, module: &str, sp: Span) -> Option<(String, Span)> {
    if !p.modules.contains(module) || !p.at(&TokKind::Dot) {
        return None;
    }
    let member = match p.toks.get(p.pos + 1).map(|t| t.kind.clone()) {
        Some(TokKind::Ident(m)) => m,
        _ => return None,
    };
    if !matches!(p.toks.get(p.pos + 2).map(|t| &t.kind), Some(TokKind::LBracket))
        || !is_generic_struct(&member)
    {
        return None;
    }
    p.bump(); // '.'
    let msp = p.bump();
    let full = Parser::join(sp, msp);
    record_qual_use(QualUse {
        file: p.file,
        alias: module.to_string(),
        name: member.clone(),
        span: full,
        is_struct: true,
    });
    Some((member, full))
}

/// `// HOOK types` in `parser.rs::parse_type_inner` — `mod.Vec[i32]` (r707).
pub(crate) fn hook_qual_generic_type(p: &mut Parser, module: &str, sp: Span) -> Option<TypeExpr> {
    let (member, full) = qual_struct_head(p, module, sp)?;
    let args = p.generic_args()?;
    let mangled = p.note_inst(&member, args, full, false);
    Some(TypeExpr::Named(mangled, full))
}

/// `// HOOK types` in `parser.rs::postfix` — `sum[i32](a, b)`.
pub(crate) fn hook_generic_call(p: &mut Parser, base: &Expr) -> Option<Expr> {
    // `fn[T](..)` or the qualified `mod.fn[T](..)` (the parser reads the
    // latter as the identifier "mod.fn" when `mod` is an imported module).
    let (qual, name) = match &base.kind {
        ExprKind::Ident(n) if is_generic_fn(n) => (None, n.clone()),
        ExprKind::Ident(n) => match n.split_once('.') {
            Some((m, f)) if p.modules.contains(m) && (p.bracket_then_paren() || is_generic_fn(f)) => {
                (Some(m.to_string()), f.to_string())
            }
            _ => return None,
        },
        // `x.f[T](..)` with `x` no module and `f` a generic function name:
        // a call through something that is not imported.
        ExprKind::Field(inner, f, _)
            if is_generic_fn(f) && p.bracket_then_paren() && matches!(inner.kind, ExprKind::Ident(_)) =>
        {
            if let ExprKind::Ident(m) = &inner.kind {
                p.dg.error(inner.span, format!("module '{}' is not imported", m));
            }
            (None, f.clone())
        }
        _ => return None,
    };
    if let Some(m) = &qual {
        // The generic names hold program wide; modules.rs checks that the
        // module really offers the template (and exports it).
        record_qual_use(QualUse {
            file: p.file,
            alias: m.clone(),
            name: name.clone(),
            span: base.span,
            is_struct: false,
        });
    }
    let args = p.generic_args()?;
    let mangled = p.note_inst(&name, args, base.span, true);
    if !p.at(&TokKind::LParen) {
        // r718: `id[i32]` without a call is the instance as a function value.
        let end = p.toks.get(p.pos.wrapping_sub(1)).map(|t| t.span).unwrap_or(base.span);
        let span = Parser::join(base.span, end);
        return Some(p.mk(span, ExprKind::Ident(mangled)));
    }
    p.bump();
    let (cargs, end) = p.call_args("after the argument list");
    let span = Parser::join(base.span, end);
    Some(p.mk(span, ExprKind::Call(mangled, cargs, base.span)))
}

/// r742 -- unify a template type (mentions the type parameters) with a concrete
/// type. `false` = the shapes differ (the type checker reports that later).
fn unify_ty(t: &TypeExpr, c: &TypeExpr, tps: &[TyParam], map: &mut HashMap<String, TypeExpr>) -> bool {
    match (t, c) {
        (TypeExpr::Named(n, _), _) if tps.iter().any(|p| &p.name == n) => match map.get(n) {
            Some(old) => type_tag(old) == type_tag(c),
            None => {
                map.insert(n.clone(), c.clone());
                true
            }
        },
        (TypeExpr::Named(n, _), TypeExpr::Named(m, _)) => {
            if n == m {
                return true;
            }
            // `Vec__T` (abstract instance written in the template) against `Vec__i32`
            match (instantiation(n), instantiation(m)) {
                (Some(a), Some(b)) if a.base == b.base && a.args.len() == b.args.len() && a.is_fn == b.is_fn => {
                    a.args.iter().zip(b.args.iter()).all(|(x, y)| unify_ty(x, y, tps, map))
                }
                _ => false,
            }
        }
        (TypeExpr::Ptr { inner: a, .. }, TypeExpr::Ptr { inner: b, .. }) => unify_ty(a, b, tps, map),
        (TypeExpr::Array { elem: a, len: la, .. }, TypeExpr::Array { elem: b, len: lb, .. }) => {
            la == lb && unify_ty(a, b, tps, map)
        }
        (TypeExpr::Fn { params: pa, ret: ra, .. }, TypeExpr::Fn { params: pb, ret: rb, .. }) => {
            pa.len() == pb.len()
                && pa.iter().zip(pb.iter()).all(|(x, y)| unify_ty(x, y, tps, map))
                && match (ra, rb) {
                    (Some(x), Some(y)) => unify_ty(x, y, tps, map),
                    (None, None) => true,
                    _ => false,
                }
        }
        (TypeExpr::Tuple(a, _), TypeExpr::Tuple(b, _)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| unify_ty(x, y, tps, map))
        }
        _ => false,
    }
}

/// r742 -- `apply(id, 4)` / `let f: fn(i32) -> i32 = id`: the type arguments of
/// the generic function `name` come from the expected `fn(..)` type. Records the
/// instance (the caller runs `mono::expand_late`) and returns its mangled name.
/// `Err` = the message for the user.
pub(crate) fn infer_fn_value(name: &str, expected: &TypeExpr, at: Span) -> Result<String, String> {
    let tpl = match fn_template(name) {
        Some(t) => t,
        None => return Err(format!("unknown generic function '{}'", name)),
    };
    let (eparams, eret) = match expected {
        TypeExpr::Fn { params, ret, .. } => (params, ret),
        _ => return Err(format!("'{}' is a generic function: write the type arguments, e.g. '{}[i32]'", name, name)),
    };
    let mut map: HashMap<String, TypeExpr> = HashMap::default();
    let ok = tpl.decl.params.len() == eparams.len()
        && tpl.decl.params.iter().zip(eparams.iter()).all(|(p, e)| unify_ty(&p.ty, e, &tpl.params, &mut map))
        && match (&tpl.decl.ret, eret) {
            (Some(x), Some(y)) => unify_ty(x, y, &tpl.params, &mut map),
            (None, None) => true,
            _ => false,
        };
    if !ok {
        return Err(format!(
            "generic function '{}' does not fit the expected function type (write the type arguments, e.g. '{}[i32]')",
            name, name
        ));
    }
    let mut args: Vec<TypeExpr> = Vec::new();
    for p in &tpl.params {
        match map.get(&p.name) {
            Some(t) => args.push(t.clone()),
            None => {
                return Err(format!(
                    "cannot infer the type parameter '{}' of '{}' from the expected function type (write the type arguments, e.g. '{}[i32]')",
                    p.name, name, name
                ))
            }
        }
    }
    let mangled = mangle(name, &args);
    record_inst(
        &mangled,
        Instantiation { base: name.to_string(), args, span: at, is_abstract: false, is_fn: true },
    );
    Ok(mangled)
}

/// r742b -- `apply_g(id, 4)`: the type arguments of the generic function `name`
/// come from its VALUE arguments. `strong[i]` = the evident type of argument i,
/// `weak[i]` = a literal default (`4` -> i32); weak types only fill parameters
/// the strong ones left open. Records the instance (the caller runs
/// `mono::expand_late`) and returns (mangled name, parameter types, result type)
/// with the type arguments substituted.
pub(crate) fn infer_call(
    name: &str,
    strong: &[Option<TypeExpr>],
    weak: &[Option<TypeExpr>],
    at: Span,
) -> Result<(String, Vec<TypeExpr>, Option<TypeExpr>), String> {
    let tpl = match fn_template(name) {
        Some(t) => t,
        None => return Err(format!("unknown generic function '{}'", name)),
    };
    if tpl.decl.params.len() != strong.len() {
        return Err(format!(
            "generic function '{}' expects {} argument(s), found {}",
            name,
            tpl.decl.params.len(),
            strong.len()
        ));
    }
    let mut map: HashMap<String, TypeExpr> = HashMap::default();
    for (p, t) in tpl.decl.params.iter().zip(strong.iter()) {
        if let Some(t) = t {
            let mut trial = map.clone();
            if unify_ty(&p.ty, t, &tpl.params, &mut trial) {
                map = trial;
            }
        }
    }
    for (p, t) in tpl.decl.params.iter().zip(weak.iter()) {
        if let Some(t) = t {
            let mut trial = map.clone();
            if unify_ty(&p.ty, t, &tpl.params, &mut trial) {
                map = trial;
            }
        }
    }
    let mut args: Vec<TypeExpr> = Vec::new();
    for p in &tpl.params {
        match map.get(&p.name) {
            Some(t) => args.push(t.clone()),
            None => {
                return Err(format!(
                    "cannot infer the type parameter '{}' of '{}' from the arguments (write the type arguments, e.g. '{}[i32]')",
                    p.name, name, name
                ))
            }
        }
    }
    let mangled = mangle(name, &args);
    record_inst(
        &mangled,
        Instantiation { base: name.to_string(), args, span: at, is_abstract: false, is_fn: true },
    );
    let ptys = tpl.decl.params.iter().map(|p| crate::mono::subst_for_infer(&p.ty, &map)).collect();
    let ret = tpl.decl.ret.as_ref().map(|r| crate::mono::subst_for_infer(r, &map));
    Ok((mangled, ptys, ret))
}

/// `// HOOK types` in `parser.rs::primary` — `Vec[i32]{ .. }`.
pub(crate) fn hook_primary(p: &mut Parser) -> Option<Expr> {
    // r707: `mod.Vec[i32]{ .. }`
    if let TokKind::Ident(m) = p.kind().clone() {
        if p.modules.contains(&m) && matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::Dot)) {
            let sp0 = p.span();
            let save = p.pos;
            p.bump();
            if let Some((member, full)) = qual_struct_head(p, &m, sp0) {
                let args = p.generic_args()?;
                let mangled = p.note_inst(&member, args, full, false);
                if p.at(&TokKind::LBrace) && !p.no_struct_lit {
                    return Some(p.struct_lit(mangled, full));
                }
                p.error_here(format!(
                    "expected '{{' after '{}.{}[..]', found '{}'",
                    m,
                    member,
                    p.kind().text()
                ));
                p.recovering = false;
                return None;
            }
            p.pos = save;
        }
    }
    let name = match p.kind().clone() {
        TokKind::Ident(n) if is_generic_struct(&n) => n,
        _ => return None,
    };
    if !matches!(p.toks.get(p.pos + 1).map(|t| &t.kind), Some(TokKind::LBracket)) {
        return None;
    }
    let sp = p.span();
    p.bump();
    let args = p.generic_args()?;
    let mangled = p.note_inst(&name, args, sp, false);
    if p.at(&TokKind::LBrace) && !p.no_struct_lit {
        return Some(p.struct_lit(mangled, sp));
    }
    p.error_here(format!(
        "expected '{{' after '{}[..]', found '{}'",
        name,
        p.kind().text()
    ));
    p.recovering = false;
    None
}

/// r725 -- `opt_none[U]()` for a declared result type `Option[U]` (`None` when the
/// type is not an Option instance or `opt_none` is unknown).
pub(crate) fn option_none_call(p: &mut Parser, ret: &TypeExpr, at: Span) -> Option<Expr> {
    let name = match ret {
        TypeExpr::Named(n, _) => n.clone(),
        _ => return None,
    };
    let inst = instantiation(&name)?;
    if inst.is_fn || inst.base != "Option" || !is_generic_fn("opt_none") {
        return None;
    }
    let mangled = p.note_inst("opt_none", inst.args.clone(), at, true);
    Some(p.mk(at, ExprKind::Call(mangled, Vec::new(), at)))
}

/// Python-level collections sugar -- the instance `base[args]` named by a
/// declared type (`Vec[i64]` -> `("Vec", [i64])`); `None` for anything that
/// is not a generic struct instance.
pub(crate) fn coll_instance(ty: &TypeExpr) -> Option<(String, Vec<TypeExpr>)> {
    let name = match ty {
        TypeExpr::Named(n, _) => n.clone(),
        _ => return None,
    };
    let inst = instantiation(&name)?;
    if inst.is_fn {
        return None;
    }
    Some((inst.base.clone(), inst.args.clone()))
}

/// Python-level collections sugar -- the expanded name of the generic function
/// `base[args]` (`vec_push[i64]`), `None` when the library is not imported.
pub(crate) fn coll_fn_name(p: &mut Parser, base: &str, args: &[TypeExpr], at: Span) -> Option<String> {
    if !is_generic_fn(base) {
        return None;
    }
    Some(p.note_inst(base, args.to_vec(), at, true))
}
