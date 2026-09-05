// SPDX-License-Identifier: GPL-2.0-only
//! Type checker (SPEC §12).
//!
//! INTERFACE (fixed):
//!   `pub fn check(prog: &ast::Program, dg: &mut Diags) -> Option<TypeInfo>`
//! On success the GUARANTEE holds: for every `ExprId` ever handed out there
//! is a concrete type in `TypeInfo::expr_types` (never `Type::UntypedInt`,
//! never `Type::Error`). `lower.rs` relies on that.
//!
//! Structure:
//!  1. create the structs, resolve field types, detect recursion (direct and
//!     indirect), compute the layout in topological order.
//!  2. collect function signatures (scalar parameters/return only, SPEC §12.1).
//!  3. check `const` declarations and evaluate them at compile time.
//!  4. check the bodies: bidirectional (the context type is a hint for
//!     untyped integer literals), no implicit conversions, reachability
//!     analysis for `return`.
//!
//! As many errors as possible are collected: after an error the check carries
//! on with `Type::Error`, which is compatible with everything.

use std::collections::{HashMap, HashSet};

use crate::ast::{
    BinOp, Block, Expr, ExprId, ExprKind, FnDecl, Program, Stmt, TypeExpr, UnOp,
};
use crate::diag::{Diags, Span};
use crate::types::{Type, TypeCtx};

#[derive(Clone, Debug)]
pub struct FnSig {
    pub params: Vec<Type>,
    pub ret: Type,
}

#[derive(Clone, Debug, Default)]
pub struct TypeInfo {
    /// Struct table including the computed layout.
    pub tcx: TypeCtx,
    /// Type of every expression, indexed by `ExprId`.
    pub expr_types: Vec<Type>,
    /// Evaluated `const` declarations: name -> (type, value).
    pub consts: HashMap<String, (Type, i128)>,
    /// **ROUND 89** — `static` declarations: name -> (type, `mut`?).
    /// Unlike a `const`, a `static` has an ADDRESS; the initial value does
    /// not live here but in `statics.rs`, already turned into octets.
    pub statics: HashMap<String, (Type, bool)>,
    /// Signatures of all functions.
    pub fns: HashMap<String, FnSig>,
    /// **ROUND 71** — expressions of type `f32` that stand in a place where
    /// an `f64` is wanted. The type checker has already written `f64` into
    /// `expr_types` for them; the lowering puts the `cvtss2sd` in
    /// (`lower_expr`). That is the ONLY implicit conversion of the
    /// language, and it is lossless: every binary32 is a binary64
    /// (SPEC 8.6).
    pub widen_f32: HashSet<crate::ast::ExprId>,
}

impl TypeInfo {
    pub fn expr_ty(&self, id: crate::ast::ExprId) -> &Type {
        self.expr_types.get(id as usize).unwrap_or(&Type::Error)
    }
}

/// Why an lvalue is not writable.
#[derive(Clone, Debug)]
enum Mutability {
    Mutable,
    Fixed(String),
}

#[derive(Clone, Debug)]
pub(crate) struct VarInfo {
    pub(crate) ty: Type,
    pub(crate) mutable: bool,
}

/// Maximum nesting depth of expressions (guards against stack overflow).
const MAX_DEPTH: u32 = 200;

pub(crate) struct Checker<'a> {
    /// **ROUND 70** — the arguments of an interpolation that may be widened
    /// (`f"{x}"` with an i32 into `io.fmt_number(… i64)`). A set, not a
    /// single slot: the chain of an `f"…"` is itself made of such calls, so
    /// while one is being checked the next one is already being resolved.
    widen: std::collections::HashSet<crate::ast::ExprId>,
    /// **ROUND 71** — see `TypeInfo::widen_f32`.
    pub(crate) widen_f32: HashSet<crate::ast::ExprId>,
    pub(crate) dg: &'a mut Diags,
    pub(crate) tcx: TypeCtx,
    pub(crate) fns: HashMap<String, FnSig>,
    pub(crate) consts: HashMap<String, (Type, i128)>,
    /// **ROUND 89** — see `TypeInfo::statics`.
    pub(crate) statics: HashMap<String, (Type, bool)>,
    /// The program of the current pass — needed by `comptime`, which runs
    /// whole functions at compile time (`comptime.rs`).
    pub(crate) prog: Option<*const Program>,
    pub(crate) expr_types: Vec<Type>,
    pub(crate) scopes: Vec<HashMap<String, VarInfo>>,
    pub(crate) ret: Type,
    pub(crate) depth: u32,
    /// Functions marked `#[must_consume]` — their result must not be dropped
    /// as a statement (attrs.rs).
    pub(crate) must_consume_fns: HashSet<String>,
    /// **Round 58** (fnval.rs) — one entry per closure body currently being
    /// checked: the depth of the scope stack where the closure begins, and
    /// the values captured so far. A name found BELOW that depth is a
    /// capture.
    pub(crate) capture_frames: Vec<(usize, Vec<(String, Type)>)>,
}

pub fn check(prog: &Program, dg: &mut Diags) -> Option<TypeInfo> {
    let mut ck = Checker {
        widen: std::collections::HashSet::new(),
        widen_f32: HashSet::new(),
        dg,
        tcx: TypeCtx::new(),
        fns: HashMap::new(),
        consts: HashMap::new(),
        statics: HashMap::new(),
        prog: None,
        expr_types: vec![Type::Error; prog.expr_count as usize],
        scopes: Vec::new(),
        ret: Type::Void,
        depth: 0,
        must_consume_fns: HashSet::new(),
        capture_frames: Vec::new(),
    };
    ck.run(prog);
    if ck.dg.has_errors() {
        return None;
    }
    // ExprIds handed out by the parser but then dropped (error recovery) get a
    // concrete filler value so that the guarantee holds. They are reachable
    // from no AST node, and lowering never sees them.
    for t in ck.expr_types.iter_mut() {
        if !matches!(t, Type::Error) {
            continue;
        }
        *t = Type::I64;
    }
    Some(TypeInfo {
        tcx: ck.tcx,
        expr_types: ck.expr_types,
        consts: ck.consts,
        statics: ck.statics,
        fns: ck.fns,
        widen_f32: ck.widen_f32,
    })
}

impl<'a> Checker<'a> {
    fn run(&mut self, prog: &Program) {
        // SAFETY: the pointer refers to the program that `check` received as
        // a reference; the checker lives entirely inside this call and never
        // outlives it. A field with a lifetime would be cleaner, but it would
        // have burdened `check` and every one of its callers with one more
        // lifetime parameter.
        self.prog = Some(prog as *const Program);
        self.check_profile(prog);
        // HOOK str: the builtin type `str` — declared BEFORE everything else,
        // so that its index is the same in every program (strtype.rs).
        crate::strtype::declare(&mut self.tcx);
        // HOOK types: register the enum names (sema_match.rs)
        crate::sema_match::declare_enums(self);
        // HOOK fehlerunionen: register the error sets (errors.rs)
        crate::errors::declare_error_sets(self);
        // HOOK gc: register `gc class` as a struct with prefix layout, compute
        // the type tag and the ancestor chain (gc.rs, SPEC 3.5.1)
        crate::gc::declare_classes(self);
        self.add_items_inner(prog, true);
        // HOOK nogc: check `#[no_gc]` transitively (nogc.rs, SPEC 3.5.4).
        // Runs AFTER the type check, because rule 3 (writing into a
        // Gc[T] field) needs the type table.
        crate::nogc::hook_check(self, prog);
        // HOOK escape: a raw pointer into a local must not outlive its frame
        // (escape.rs, round 79, gap 9 of docs/ROUND66.md). Runs AFTER the
        // type check as well: it has to tell `a[i]` on an ARRAY from `p[i]`
        // on a pointer, and only the type table knows which is which.
        crate::escape::hook_check(self, prog);
        // HOOK kern: `#[interrupt]` — check the form and forbid calls
        // (core.rs, round 52).
        crate::core::check_interrupts(self, prog);
        // Whole program check: runs exactly once, not per addendum.
        self.check_main(prog);
    }

    /// **Re-entry into the check phases** (DESIGN_GOALS.md §7, a foundation
    /// point from §10.4).
    ///
    /// Checks ADDITIONAL declarations against the state that is already built
    /// up — the same name table, the same type table, the same diagnostics.
    /// That answers the question "can the compiler still check a function
    /// that has only just come into being?" with **yes**.
    ///
    /// `comptime`/`emit` needs this (SPEC §6.4): there, items come into being
    /// *during* compilation — Web IDL bindings, CSS property tables and
    /// Unicode tables. A type checker built as a single pass over a fixed AST
    /// cannot learn about them after the fact; which is why the ability sits
    /// here already, before there is any producer in the compiler that would
    /// make use of it.
    ///
    /// **Honest scope:** addenda may contain structs, functions and constants.
    /// Enums are laid out in the first pass only (`layout_enums`), because
    /// they are registered in the parser; `enum`s produced after the fact
    /// arrive together with `comptime` itself.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn add_items(&mut self, prog: &Program) {
        self.add_items_inner(prog, false);
    }

    fn add_items_inner(&mut self, prog: &Program, layout_enums: bool) {
        self.prog = Some(prog as *const Program);
        // Addenda bring their own expression ids along; the table grows with them.
        if prog.expr_count as usize > self.expr_types.len() {
            self.expr_types.resize(prog.expr_count as usize, Type::Error);
        }
        // HOOK iface: register `dyn I` as a struct — BEFORE the structs of the
        // program, so that a field of type `dyn I` finds its layout (iface.rs)
        crate::iface::declare_interfaces(self);
        self.collect_structs(prog);
        // HOOK str: note every view of the shape `{ *mut u8, usize }` —
        // `str` may be used for those and vice versa (strtype.rs).
        crate::strtype::remember_views(&self.tcx);
        if layout_enums {
            // HOOK types: lay out the enums (sema_match.rs)
            crate::sema_match::layout_enums(self, prog);
        }
        // HOOK gc: field layout of the gc classes (gc.rs). Only at this point
        // are the structs and enums of the program known — that way a struct
        // field in a gc class gets the right message.
        crate::gc::layout_classes(self);
        self.collect_fns(prog);
        // HOOK extfn: register `extern fn` / `#[export_c]` link names
        // (extfn.rs, SPEC §14.5, round 75). Needs to run once collect_fns
        // has resolved names, so it runs on the SAME `FnDecl.name` values
        // that lowering and codegen see.
        crate::extfn::register(prog);
        // HOOK iface: check `impl I for T` completely — all methods present,
        // all signatures matching (iface.rs, round 46). Runs BEFORE the bodies,
        // so that the message about the implementation comes before the one
        // about the call site.
        crate::iface::hook_check_impls(self);
        // Check and apply the attributes (attrs.rs)
        self.check_attrs(prog);
        self.check_consts(prog);
        // ROUND 89: after the constants, because a `static`'s initial value
        // may name a `const` (`static mut BUF: [u8; SIZE] = [0; SIZE]`),
        // and never the other way round -- a `const` is a number, a
        // `static` is a place, and a place has no value at compile time.
        self.check_statics(prog);
        for f in &prog.funcs {
            self.check_fn(f);
        }
    }

    /// Is a value being discarded here that must not be discarded?
    ///
    /// **Scope at stage 0, stated honestly:** what is checked is the case "the
    /// result of a call is discarded as a statement". The full form from
    /// SPEC §3.3 — *the value must be passed on to a consuming function* —
    /// needs the move checker and arrives together with it (ROADMAP phase 2).
    /// What is checked here is the subset that can be decided without move
    /// tracking.
    fn check_discard(&mut self, e: &Expr, t: &Type) {
        let name = match &e.kind {
            ExprKind::Call(n, _, _) => n.clone(),
            _ => return,
        };
        let basic = if self.must_consume_fns.contains(&name) {
            format!("'{}' is marked with #[must_consume]", name)
        } else if let Type::Struct(i) = t {
            match self.tcx.structs.get(*i) {
                Some(d) if d.must_consume => {
                    format!("the type '{}' is marked with #[must_consume]", d.name)
                }
                _ => return,
            }
        } else {
            return;
        };
        self.dg.error_note(
            e.span,
            format!("the result must not be discarded: {}", basic),
            "bind it to a variable or pass it on".to_string(),
        );
    }

    // ------------------------------------------------------------ Attributes

    /// Checks all attributes against the register in `attrs.rs` and applies
    /// those that really do something at stage 0.
    ///
    /// Three kinds of error, all with line and column:
    ///  * **unknown** — with a suggestion, in case it is a typo
    ///  * **wrong target** — say `#[packed]` in front of a function
    ///  * **known, but not implemented at stage 0** — an explicit rejection
    ///    instead of silently ignoring it. A `#[constant_time]` passed over
    ///    in silence would be the most dangerous bug in this language.
    fn check_attrs(&mut self, prog: &Program) {
        for f in &prog.funcs {
            let attrs = f.attrs.clone();
            for a in &attrs {
                if self.check_one_attr(a, true) && a.name == "must_consume" {
                    self.must_consume_fns.insert(f.name.clone());
                }
            }
            // **Round 75** (SPEC §14.5) — `#[link_name(...)]` only makes
            // sense on a body-less declaration; `#[export_c]` only on one
            // WITH a body (there has to be something to export). Checked
            // here rather than in `attrs.rs`, because it needs to know
            // whether THIS declaration is `extern`.
            // ROUND 89 (SPEC 13): `#[panic_handler]`. The signature is
            // FIXED, because the trampoline calls it with five values in
            // five registers and cannot negotiate. Checked here, where the
            // signature is known, rather than at the call site, which is
            // hand written assembly and has no type checker of its own.
            if attrs.iter().any(|a| a.name == "panic_handler") {
                self.check_panic_handler(f);
            }
            // ROUND 94: a test takes nothing and gives nothing back. It says
            // what it thinks by RUNNING -- it panics, it crashes or it comes
            // back (`testrun.rs`). A return value would be a second, silent
            // channel that the runner does not read, so it is refused here
            // rather than ignored.
            if attrs.iter().any(|a| a.name == "test") {
                if f.extern_info.is_some() {
                    self.dg.error(f.span, "'#[test]' needs a body ('extern fn' has none)");
                } else if !f.params.is_empty() || f.ret.is_some() {
                    self.dg.error_note(
                        f.span,
                        format!("'#[test]' takes no parameters and returns nothing, '{}' does", f.name),
                        format!("write 'fn {}()'", f.name),
                    );
                }
            }
            let has_link_name = attrs.iter().any(|a| a.name == "link_name");
            let has_export_c = attrs.iter().any(|a| a.name == "export_c");
            if has_link_name && f.extern_info.is_none() {
                self.dg.error(
                    f.span,
                    "'#[link_name(...)]' only belongs on an 'extern fn' declaration",
                );
            }
            if has_export_c && f.extern_info.is_some() {
                self.dg.error(
                    f.span,
                    "'#[export_c]' does not belong on 'extern fn' (it has no body to export)",
                );
            }
        }
        for sd in &prog.structs {
            let attrs = sd.attrs.clone();
            for a in &attrs {
                if self.check_one_attr(a, false) && a.name == "must_consume" {
                    if let Some(i) = self.tcx.lookup(&sd.name) {
                        if let Some(def) = self.tcx.structs.get_mut(i) {
                            def.must_consume = true;
                        }
                    }
                }
            }
        }
    }

    /// ROUND 89 — the one function a program may mark `#[panic_handler]`,
    /// and what its signature has to be.
    fn check_panic_handler(&mut self, f: &crate::ast::FnDecl) {
        if f.extern_info.is_some() {
            self.dg.error(
                f.span,
                "'#[panic_handler]' needs a body ('extern fn' has none)",
            );
            return;
        }
        let want: [Type; 5] = [
            Type::ptr(Type::U8, false),
            Type::U64,
            Type::I64,
            Type::I64,
            Type::U64,
        ];
        let ok = f.ret.is_none()
            && f.params.len() == want.len()
            && f
                .params
                .iter()
                .zip(want.iter())
                .all(|(p, w)| self.resolve_ty_quiet(&p.ty).as_ref() == Some(w));
        if !ok {
            self.dg.error_note(
                f.span,
                format!(
                    "'#[panic_handler]' has a fixed signature, '{}' does not match it",
                    f.name
                ),
                format!("write '{}'", crate::panic_rt::HANDLER_SIG),
            );
            return;
        }
        if let Some(other) = crate::panic_rt::handler() {
            if other != f.name {
                self.dg.error(
                    f.span,
                    format!(
                        "'#[panic_handler]' is already taken by '{}' — a program has exactly one ending",
                        other
                    ),
                );
                return;
            }
        }
        crate::panic_rt::set_handler(Some(f.name.clone()));
    }

    /// `true` = the attribute is valid AND implemented at stage 0.
    fn check_one_attr(&mut self, a: &crate::ast::Attr, on_func: bool) -> bool {
        let info = match crate::attrs::search(&a.name) {
            Some(i) => i,
            None => {
                let msg = format!("unknown attribute '{}'", a.name);
                match crate::attrs::proposal(&a.name) {
                    Some(v) => self.dg.error_note(
                        a.span,
                        msg,
                        format!("did you mean '{}'? '--list-attrs' shows all", v),
                    ),
                    None => self.dg.error_note(
                        a.span,
                        msg,
                        "'--list-attrs' shows all known attributes".to_string(),
                    ),
                }
                return false;
            }
        };
        if !crate::attrs::fits(info, on_func) {
            self.dg.error(
                a.span,
                format!(
                    "attribute '{}' does not belong before {}",
                    a.name,
                    if on_func { "a function" } else { "a struct" }
                ),
            );
            return false;
        }
        if a.args.len() != info.args {
            self.dg.error(
                a.span,
                format!(
                    "attribute '{}' expects {} argument(s), found {}",
                    a.name,
                    info.args,
                    a.args.len()
                ),
            );
            return false;
        }
        if !info.implemented {
            self.dg.error_note(
                a.span,
                format!("attribute '{}' is not implemented in stage 0", a.name),
                format!("geplant: {}", info.what),
            );
            return false;
        }
        true
    }

    // --------------------------------------------------------------- Profile

    fn check_profile(&mut self, prog: &Program) {
        // HOOK profil (prof.rs, round 52): fix the profile AND enforce its
        // rules. Up to round 51 only the name check stood here — the
        // declaration had no effect at all (SPEC §14, point 6).
        crate::prof::hook_check(self.dg, prog);
    }

    // --------------------------------------------------------------- Structs

    fn collect_structs(&mut self, prog: &Program) {
        // HOOK fehlerunionen: report the layout phase (errors.rs)
        crate::errors::hook_struct_phase(true);
        // 1. create all names (allows mutual pointer references)
        let mut idx_of: Vec<usize> = Vec::with_capacity(prog.structs.len());
        for s in &prog.structs {
            if self.tcx.lookup(&s.name).is_some() {
                self.dg
                    .error(s.span, format!("struct '{}' is already declared", s.name));
                // Declared twice: point at the first entry.
                idx_of.push(self.tcx.lookup(&s.name).unwrap_or(0));
                continue;
            }
            idx_of.push(self.tcx.declare(&s.name));
        }

        // 2. resolve the field types
        let mut resolved: Vec<Vec<(String, Type)>> = Vec::with_capacity(prog.structs.len());
        for s in &prog.structs {
            let mut seen: HashSet<String> = HashSet::new();
            let mut fields: Vec<(String, Type)> = Vec::new();
            for (name, te, span) in &s.fields {
                let ty = self.resolve_ty(te);
                if !seen.insert(name.clone()) {
                    self.dg.error(
                        *span,
                        format!("field '{}' is already declared in struct '{}'", name, s.name),
                    );
                    continue;
                }
                if matches!(ty, Type::Void) {
                    self.dg
                        .error(te.span(), "a field cannot have the type '()'");
                    continue;
                }
                fields.push((name.clone(), ty));
            }
            resolved.push(fields);
        }

        // 3. detect recursion (value containment; pointers break the cycle)
        let n = self.tcx.structs.len();
        let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, fields) in resolved.iter().enumerate() {
            let target = match idx_of.get(i) {
                Some(t) => *t,
                None => continue,
            };
            for (_, ty) in fields {
                collect_value_deps(ty, &mut deps[target]);
            }
        }
        let mut state = vec![0u8; n]; // 0 = new, 1 = on the path, 2 = done
        let mut order: Vec<usize> = Vec::new();
        let mut bad: HashSet<usize> = HashSet::new();
        for i in 0..n {
            find_cycles(i, &deps, &mut state, &mut order, &mut bad);
        }
        for (i, s) in prog.structs.iter().enumerate() {
            let target = match idx_of.get(i) {
                Some(t) => *t,
                None => continue,
            };
            if bad.contains(&target) {
                self.dg.error_note(
                    s.span,
                    format!("struct '{}' contains itself (directly or indirectly)", s.name),
                    "use a pointer there, e.g. '*mut T'",
                );
            }
        }

        // 4. compute the layout in topological order
        let mut fields_by_idx: Vec<Option<Vec<(String, Type)>>> = vec![None; n];
        for (i, fields) in resolved.into_iter().enumerate() {
            if let Some(t) = idx_of.get(i) {
                if fields_by_idx[*t].is_none() {
                    fields_by_idx[*t] = Some(fields);
                }
            }
        }
        for idx in order {
            let fields = match fields_by_idx.get_mut(idx).and_then(|f| f.take()) {
                Some(f) => f,
                None => continue,
            };
            if bad.contains(&idx) {
                // Cyclic structs get no layout (size 0), so that size_of does
                // not run forever. The error has already been reported.
                self.tcx.set_fields(idx, Vec::new());
                continue;
            }
            self.tcx.set_fields(idx, fields);
        }
        // HOOK fehlerunionen: the layout phase has ended (errors.rs)
        crate::errors::hook_struct_phase(false);
    }

    // -------------------------------------------------------------- Functions

    fn collect_fns(&mut self, prog: &Program) {
        for f in &prog.funcs {
            let mut params = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();
            for p in &f.params {
                let ty = self.resolve_ty(&p.ty);
                if !seen.insert(p.name.clone()) {
                    self.dg.error(
                        p.span,
                        format!("parameter '{}' is already declared", p.name),
                    );
                }
                if matches!(ty, Type::Void) {
                    self.dg.error(p.ty.span(), "a parameter cannot have the type '()'");
                    params.push(Type::Error);
                    continue;
                }
                params.push(ty);
            }
            let ret = match &f.ret {
                None => Type::Void,
                Some(te) => self.resolve_ty(te),
            };
            if self.fns.contains_key(&f.name) {
                self.dg.error(
                    f.span,
                    format!("function '{}' is already declared", f.name),
                );
                continue;
            }
            self.fns.insert(f.name.clone(), FnSig { params, ret });
        }
    }

    fn check_main(&mut self, prog: &Program) {
        match self.fns.get("main") {
            None => {
                // ROUND 52 (SPEC §2): the kernel profile has no entry
                // point at all. There is no `_start`, no runtime prologue
                // and nobody who would take an exit code — only an object
                // file that a boot loader or a linker script pulls into
                // place.
                if crate::prof::is_kernel() {
                    return;
                }
                self.dg.error_note(
                    Span::none(),
                    "the program has no function 'main'",
                    "'fn main() -> i32' is expected",
                );
            }
            Some(sig) => {
                // TWO allowed forms:
                //   fn main() -> i32
                //   fn main(start: u64) -> i32
                // The second one gets the START BLOCK of the process
                // ([argc][argv..][0][envp..]); for that `_start` puts `rsp`
                // into `rdi` (codegen_x86.rs). Without it a program written
                // in Firn cannot read its own arguments — and `firnc1`
                // needs a file name.
                let params_ok = sig.params.is_empty()
                    || (sig.params.len() == 1
                        && matches!(sig.params[0], Type::U64 | Type::Usize));
                let bad = !params_ok || sig.ret != Type::I32;
                if bad {
                    let span = prog
                        .funcs
                        .iter()
                        .find(|f| f.name == "main")
                        .map(|f| f.span)
                        .unwrap_or_else(Span::none);
                    self.dg.error_note(
                        span,
                        "'main' must be declared without parameters or with exactly one 'u64' and must return 'i32'",
                        "'fn main() -> i32' or 'fn main(start: u64) -> i32' is expected (start block: argc, argv, envp)",
                    );
                }
            }
        }
    }

    fn check_fn(&mut self, f: &FnDecl) {
        // **Round 75** (SPEC §14.5) — `extern fn` has no body: nothing to
        // type-check and no "reaches the end without return" to report
        // (there IS no end here, the function is defined elsewhere).
        if f.extern_info.is_some() {
            return;
        }
        let sig = match self.fns.get(&f.name) {
            Some(s) => s.clone(),
            None => return, // label declared twice, reported already
        };
        self.ret = sig.ret.clone();
        self.scopes.clear();
        self.scopes.push(HashMap::new());
        for (p, ty) in f.params.iter().zip(sig.params.iter()) {
            self.declare_var(&p.name, ty.clone(), false, p.span);
        }
        self.check_block(&f.body, true);
        self.scopes.pop();
        if sig.ret != Type::Void && !sig.ret.is_error() && !block_returns(&f.body) {
            self.dg.error_note(
                f.span,
                format!(
                    "function '{}' reaches the end without 'return' (return type {})",
                    f.name,
                    self.tcx.name_of(&sig.ret)
                ),
                "every path must end with 'return <value>'",
            );
        }
    }

    // -------------------------------------------------------------- Constants

    fn check_consts(&mut self, prog: &Program) {
        for c in &prog.consts {
            let ty = self.resolve_ty(&c.ty);
            if !ty.is_error() && !(ty.is_concrete_int() || ty == Type::Bool) {
                self.dg.error(
                    c.ty.span(),
                    "'const' supports only integer and bool types in stage 0",
                );
                self.type_out_expr(&c.value);
                continue;
            }
            let t = self.expr(&c.value, Some(&ty));
            if !assignable(&t, &ty) {
                self.dg.error(
                    c.value.span,
                    format!(
                        "constant '{}' has type {}, the value is of type {}",
                        c.name,
                        self.tcx.name_of(&ty),
                        self.tcx.name_of(&t)
                    ),
                );
                continue;
            }
            if self.consts.contains_key(&c.name) {
                self.dg.error(
                    c.span,
                    format!("constant '{}' is already declared", c.name),
                );
                continue;
            }
            match self.eval_const(&c.value) {
                Ok(v) => {
                    self.consts.insert(c.name.clone(), (ty.clone(), wrap(v, &ty)));
                }
                Err((span, msg)) => {
                    self.dg.error(span, msg);
                    // So that later uses do not report an unknown name.
                    self.consts.insert(c.name.clone(), (ty.clone(), 0));
                }
            }
        }
    }

    // -------------------------------------------------- Global variables

    /// **ROUND 89** — `static` / `static mut` (SPEC §14.1.statics).
    ///
    /// Three things happen per declaration, in this order, and each one can
    /// stop the next:
    ///
    /// 1. The TYPE is resolved and refused if the collector could not see
    ///    through it (`Gc[T]`, a `gc class` pointer). The reasoning is in
    ///    `statics.rs`; the message says it out loud rather than producing a
    ///    root the collector never scans.
    /// 2. The VALUE is type checked against that type, exactly as an
    ///    initialiser of a local would be -- so a text literal becomes the
    ///    array of its octets and `[0; 256]` gets its element type from the
    ///    left hand side, both for free.
    /// 3. The value is EVALUATED to a finished sequence of octets
    ///    (`static_bytes`). Anything that is not evaluable at compile time
    ///    is an error HERE, with the position of the offending
    ///    subexpression -- that is what makes an initialisation order
    ///    unnecessary instead of merely undocumented.
    fn check_statics(&mut self, prog: &Program) {
        for d in &prog.statics {
            let ty = self.resolve_ty(&d.ty);
            if ty.is_error() {
                self.type_out_expr(&d.value);
                continue;
            }
            if let Some(what) = self.gc_reachable(&ty) {
                self.dg.error_note(
                    d.ty.span(),
                    format!(
                        "a 'static' must not hold a collected value ({} in '{}')",
                        what,
                        self.tcx.name_of(&ty)
                    ),
                    "the root set of the collector is the stack and the callee-saved \
registers (SPEC 3.5.3); a data section entry is neither, so the collector \
would free an object this 'static' still points at. Keep the handle in a \
local, or use 'profile kernel', which has no collector at all",
                );
                self.type_out_expr(&d.value);
                continue;
            }
            if self.tcx.size_of(&ty) == 0 {
                self.dg.error(
                    d.ty.span(),
                    format!(
                        "a 'static' of type {} has no storage",
                        self.tcx.name_of(&ty)
                    ),
                );
                self.type_out_expr(&d.value);
                continue;
            }
            let t = self.expr(&d.value, Some(&ty));
            if !t.is_error() && !assignable(&t, &ty) {
                self.dg.error(
                    d.value.span,
                    format!(
                        "the global variable '{}' has type {}, the value is of type {}",
                        d.name,
                        self.tcx.name_of(&ty),
                        self.tcx.name_of(&t)
                    ),
                );
                continue;
            }
            if self.statics.contains_key(&d.name) || self.consts.contains_key(&d.name) {
                self.dg.error(
                    d.span,
                    format!("'{}' is already declared", d.name),
                );
                continue;
            }
            let mut bytes: Vec<u8> = Vec::new();
            match self.static_bytes(&d.value, &ty, &mut bytes) {
                Ok(()) => {
                    crate::statics::register(
                        &d.name,
                        d.mutable,
                        bytes,
                        self.tcx.align_of(&ty).max(1),
                    );
                }
                Err((span, msg)) => {
                    self.dg.error_note(
                        span,
                        msg,
                        "the initial value of a 'static' is written into the object file, \
so it has to be known while compiling -- there is no code that runs before \
'main' to compute it (SPEC 14.1.statics)",
                    );
                }
            }
            // Even after a failed evaluation the NAME is known, so that a
            // use of it further down does not report a second, misleading
            // "unknown name".
            self.statics.insert(d.name.clone(), (ty, d.mutable));
        }
    }

    /// Does a value of this type contain something the collector owns?
    /// Returns the offending spelling for the message, or `None`.
    fn gc_reachable(&self, t: &Type) -> Option<String> {
        match t {
            Type::Ptr { inner, .. } => match &**inner {
                Type::Struct(i) => {
                    let n = self.tcx.structs.get(*i).map(|s| s.name.as_str()).unwrap_or("");
                    if let Some(c) = n.strip_prefix("gc ") {
                        Some(format!("Gc[{}]", c))
                    } else {
                        self.gc_reachable(inner)
                    }
                }
                other => self.gc_reachable(other),
            },
            Type::Array(e, _) => self.gc_reachable(e),
            Type::Struct(i) => {
                let d = self.tcx.structs.get(*i)?;
                if d.name.starts_with("gc ") {
                    return Some(format!("gc class {}", &d.name[3..]));
                }
                for f in &d.fields {
                    if let Some(w) = self.gc_reachable(&f.ty) {
                        return Some(w);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// **ROUND 89** — the initial value of a `static`, as the exact octets
    /// that go into the object file (little endian, `size_of(ty)` of them).
    ///
    /// Deliberately its own walk and not a second use of `eval_const`:
    /// `eval_const` answers "which NUMBER is this", which is the wrong
    /// question for an array of 256 octets or a struct with three fields.
    /// The two meet at the leaves -- every scalar in here is evaluated by
    /// `eval_const` and by nothing else, so a `const` and a `static` can
    /// never disagree about what `1 << 12` is.
    fn static_bytes(
        &self,
        e: &Expr,
        ty: &Type,
        out: &mut Vec<u8>,
    ) -> Result<(), (Span, String)> {
        let want = self.tcx.size_of(ty) as usize;
        let start = out.len();
        match ty {
            Type::Array(el, n) => {
                let esz = self.tcx.size_of(el) as usize;
                match &e.kind {
                    // A text literal carries the array literal of its
                    // octets inside (ast.rs::Text, round 70).
                    ExprKind::Text(_, inner) => {
                        self.static_bytes(inner, ty, out)?;
                    }
                    ExprKind::ArrayLit(items) => {
                        if items.len() as u64 != *n {
                            return Err((
                                e.span,
                                format!(
                                    "the array has {} elements, the value has {}",
                                    n,
                                    items.len()
                                ),
                            ));
                        }
                        for it in items {
                            self.static_bytes(it, el, out)?;
                        }
                    }
                    ExprKind::ArrayRepeat(v, cnt) => {
                        let c = self.eval_const(cnt)?;
                        if c < 0 || c as u64 != *n {
                            return Err((
                                cnt.span,
                                format!("the array has {} elements, the repetition says {}", n, c),
                            ));
                        }
                        let mut one: Vec<u8> = Vec::new();
                        self.static_bytes(v, el, &mut one)?;
                        for _ in 0..*n {
                            out.extend_from_slice(&one);
                        }
                    }
                    _ => {
                        return Err((
                            e.span,
                            "the initial value of an array 'static' must be an array literal \
('[a, b, c]', '[0; n]' or a text literal)"
                                .to_string(),
                        ))
                    }
                }
                let _ = esz;
            }
            Type::Struct(i) => {
                let def = match self.tcx.structs.get(*i) {
                    Some(d) => d.clone(),
                    None => return Err((e.span, "unknown struct type".to_string())),
                };
                let (lit_fields, lspan) = match &e.kind {
                    ExprKind::StructLit(_, fs, sp) => (fs, *sp),
                    _ => {
                        return Err((
                            e.span,
                            "the initial value of a struct 'static' must be a struct literal"
                                .to_string(),
                        ))
                    }
                };
                // Padding is zero, not whatever was on the compiler's heap.
                out.resize(start + want, 0);
                for f in &def.fields {
                    let given = match lit_fields.iter().find(|(n, _, _)| *n == f.name) {
                        Some((_, v, _)) => v,
                        None => {
                            return Err((
                                lspan,
                                format!("the field '{}' is missing", f.name),
                            ))
                        }
                    };
                    let mut fb: Vec<u8> = Vec::new();
                    self.static_bytes(given, &f.ty, &mut fb)?;
                    let at = start + f.offset as usize;
                    out[at..at + fb.len()].copy_from_slice(&fb);
                }
            }
            Type::F64 => {
                let bits = self.eval_static_float(e, false)?;
                out.extend_from_slice(&bits.to_le_bytes());
            }
            Type::F32 => {
                let bits = self.eval_static_float(e, true)? as u32;
                out.extend_from_slice(&bits.to_le_bytes());
            }
            Type::Ptr { .. } | Type::Fn { .. } => {
                // A pointer that is known at compile time can only be the
                // null pointer -- every other address is decided by the
                // linker or by `mmap`, neither of which has happened yet.
                let v = self.eval_const(e)?;
                if v != 0 {
                    return Err((
                        e.span,
                        "a pointer 'static' can only start as 0 (there is no address at \
compile time)"
                            .to_string(),
                    ));
                }
                out.extend_from_slice(&0u64.to_le_bytes());
            }
            t if t.is_concrete_int() || *t == Type::Bool => {
                let v = wrap(self.eval_const(e)?, t);
                let b = (v as u128).to_le_bytes();
                out.extend_from_slice(&b[..want.min(16)]);
            }
            other => {
                return Err((
                    e.span,
                    format!(
                        "a 'static' of type {} is not supported in stage 0",
                        self.tcx.name_of(other)
                    ),
                ))
            }
        }
        if out.len() - start != want {
            out.resize(start + want, 0);
        }
        Ok(())
    }

    /// The bit pattern of a floating point initial value. Only a literal
    /// (with an optional minus in front) -- `0.1 + 0.2` at compile time
    /// would need a second, exactly IEEE-754 conforming evaluator, and one
    /// that is only ALMOST right is worse than none.
    fn eval_static_float(&self, e: &Expr, single: bool) -> Result<u64, (Span, String)> {
        match &e.kind {
            ExprKind::Float(bits, s32) => {
                Ok(if single { *s32 as u64 } else { *bits })
            }
            ExprKind::FloatF32(b) => Ok(*b as u64),
            ExprKind::Unary(UnOp::Neg, inner) => {
                let v = self.eval_static_float(inner, single)?;
                Ok(if single {
                    (v as u32 ^ 0x8000_0000) as u64
                } else {
                    v ^ 0x8000_0000_0000_0000
                })
            }
            _ => Err((
                e.span,
                "a floating point 'static' starts from a literal ('1.5', '-0.25')".to_string(),
            )),
        }
    }

    // ---------------------------------------------------------------- Ranges

    pub(crate) fn declare_var(&mut self, name: &str, ty: Type, mutable: bool, span: Span) {
        if let Some(top) = self.scopes.last() {
            if top.contains_key(name) {
                self.dg.error(
                    span,
                    format!("'{}' is already declared in this block", name),
                );
            }
        }
        if let Some(top) = self.scopes.last_mut() {
            top.insert(name.to_string(), VarInfo { ty, mutable });
        }
    }

    /// Round 64 -- every name that could stand at the place of a VALUE:
    /// variables in scope, constants and functions (functions are values
    /// since round 58).
    pub(crate) fn value_names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in &self.scopes {
            out.extend(s.keys().cloned());
        }
        out.extend(self.consts.keys().cloned());
        out.extend(self.statics.keys().cloned());
        out.extend(self.fns.keys().cloned());
        out
    }

    /// Round 64 -- the suggestion for a misspelled value name.
    pub(crate) fn value_hint(&self, name: &str) -> Option<String> {
        let all = self.value_names();
        crate::diag::nearest(name, all.iter().map(|s| s.as_str()))
            .map(|n| crate::diag::did_you_mean(&n))
    }

    /// Round 64 -- the suggestion for a misspelled type or struct name.
    pub(crate) fn type_hint(&self, name: &str) -> Option<String> {
        let all: Vec<String> = self.tcx.structs.iter().map(|s| s.name.clone()).collect();
        crate::diag::nearest(name, all.iter().map(|s| s.as_str()))
            .map(|n| crate::diag::did_you_mean(&n))
    }

    /// Round 64 -- the suggestion for a misspelled field of struct `idx`.
    pub(crate) fn field_hint(&self, idx: usize, name: &str) -> Option<String> {
        let all: Vec<String> = self
            .tcx
            .structs
            .get(idx)
            .map(|s| s.fields.iter().map(|f| f.name.clone()).collect())
            .unwrap_or_default();
        crate::diag::nearest(name, all.iter().map(|s| s.as_str()))
            .map(|n| crate::diag::did_you_mean(&n))
    }

    pub(crate) fn lookup_var(&self, name: &str) -> Option<&VarInfo> {
        for s in self.scopes.iter().rev() {
            if let Some(v) = s.get(name) {
                return Some(v);
            }
        }
        None
    }

    /// Round 58: like `lookup_var`, but with the depth at which the name was
    /// found — `note_use` decides on that whether it is a capture.
    fn lookup_var_at(&self, name: &str) -> Option<(usize, VarInfo)> {
        for (i, s) in self.scopes.iter().enumerate().rev() {
            if let Some(v) = s.get(name) {
                return Some((i, v.clone()));
            }
        }
        None
    }

    /// **Round 58** (fnval.rs) — records the USE of a name inside a closure
    /// body. Everything that lies below the closure's own scopes is
    /// captured, ONCE, in the order of first use.
    pub(crate) fn note_use(&mut self, name: &str) {
        if self.capture_frames.is_empty() {
            return;
        }
        let (depth, ty) = match self.lookup_var_at(name) {
            Some((d, v)) => (d, v.ty),
            None => return,
        };
        for f in self.capture_frames.iter_mut() {
            if depth < f.0 && !f.1.iter().any(|(n, _)| n == name) {
                f.1.push((name.to_string(), ty.clone()));
            }
        }
    }

    /// Round 58: is this name captured by the closure being checked?
    pub(crate) fn is_captured(&self, name: &str) -> bool {
        match (self.capture_frames.last(), self.lookup_var_at(name)) {
            (Some(f), Some((d, _))) => d < f.0,
            _ => false,
        }
    }

    /// Round 58 (fnval.rs): the quiet type resolution, usable from outside.
    pub(crate) fn resolve_ty_quiet_pub(&self, te: &TypeExpr) -> Option<Type> {
        self.resolve_ty_quiet(te)
    }

    // ------------------------------------------------------------ Statements

    pub(crate) fn check_block(&mut self, b: &Block, reuse_scope: bool) {
        if !reuse_scope {
            self.scopes.push(HashMap::new());
        }
        for s in &b.stmts {
            self.check_stmt(s);
        }
        if !reuse_scope {
            self.scopes.pop();
        }
    }

    fn check_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Error(_) => {}
            // `defer <statement>` (SPEC §5.1). The body is checked entirely
            // normally — it sees the same names as the place where it
            // stands, and it runs in the same frame.
            //
            // FORBIDDEN is a jump out of the body: `return`, `break` and
            // `continue` would tear apart the order of the remaining
            // deferred statements and overwrite the return value. Zig
            // forbids it for the same reason.
            Stmt::Defer(inner, only_error, span) => {
                let kind = if *only_error { "errdefer" } else { "defer" };
                if let Some((bad, word)) = crate::sema::defer_jump(inner) {
                    self.dg.error_note(
                        bad,
                        format!("'{}' is not allowed in a '{}'", word, kind),
                        "the deferred body must end normally; otherwise it would be undefined what happens to the remaining deferred statements",
                    );
                    let _ = span;
                }
                self.check_stmt(inner);
            }
            Stmt::Block(b) => self.check_block(b, false),
            Stmt::Expr(e) => {
                let t = self.expr(e, None);
                self.check_discard(e, &t);
            }
            Stmt::Let { name, mutable, ty, init, span } => {
                let declared = ty.as_ref().map(|te| self.resolve_ty(te));
                let t = match &declared {
                    // HOOK fehlerunionen: implicit conversion (errors.rs)
                    Some(d) if crate::errors::hook_coerce(self, init, d) => d.clone(),
                    Some(d) => {
                        let got = self.expr(init, Some(d));
                        if !assignable(&got, d) {
                            self.dg.error(
                                init.span,
                                format!(
                                    "expected type {}, found {}",
                                    self.tcx.name_of(d),
                                    self.tcx.name_of(&got)
                                ),
                            );
                        }
                        d.clone()
                    }
                    None => {
                        let got = self.expr(init, None);
                        if matches!(got, Type::Void) {
                            self.dg.error(
                                init.span,
                                "the expression yields no value and cannot be bound",
                            );
                            Type::Error
                        } else {
                            got
                        }
                    }
                };
                self.declare_var(name, t, *mutable, *span);
            }
            Stmt::Assign { target, value, span } => {
                let (ty, mutability) = match self.lvalue(target) {
                    Some(x) => x,
                    None => {
                        self.expr(value, Some(&Type::I64));
                        return;
                    }
                };
                if let Mutability::Fixed(reason) = mutability {
                    let note = fixed_note(&reason);
                    self.dg.error_note(*span, reason, note);
                }
                // HOOK fehlerunionen: implicit conversion (errors.rs)
                if crate::errors::hook_coerce(self, value, &ty) {
                    return;
                }
                let got = self.expr(value, Some(&ty));
                if !assignable(&got, &ty) {
                    self.dg.error(
                        value.span,
                        format!(
                            "assignment expects type {}, found {}",
                            self.tcx.name_of(&ty),
                            self.tcx.name_of(&got)
                        ),
                    );
                }
            }
            // ROUND 70: `x op= e` - the same lvalue check as an ordinary
            // assignment, and afterwards EXACTLY the rules of `x = x op e`
            // (`binop_type`). `let` stays immutable.
            Stmt::AssignOp { target, op, value, span } => {
                let (ty, mutability) = match self.lvalue(target) {
                    Some(x) => x,
                    None => {
                        self.expr(value, Some(&Type::I64));
                        return;
                    }
                };
                if let Mutability::Fixed(reason) = mutability {
                    let note = fixed_note(&reason);
                    self.dg.error_note(*span, reason, note);
                }
                // The right side gets the type of the left one as its hint -
                // exactly what `binary` would give it (`probe(l)`).
                let rt = self.expr(value, Some(&ty));
                if ty.is_error() || rt.is_error() {
                    return;
                }
                let got = self.binop_type(*op, &ty, &rt, *span);
                if !assignable(&got, &ty) {
                    self.dg.error(
                        value.span,
                        format!(
                            "assignment expects type {}, found {}",
                            self.tcx.name_of(&ty),
                            self.tcx.name_of(&got)
                        ),
                    );
                }
            }
            // ROUND 70: `x++` / `x--`. It is exactly `x = x + 1`, so it
            // works on the types on which `x + 1` works - the integers.
            Stmt::Step { target, up, span } => {
                let (ty, mutability) = match self.lvalue(target) {
                    Some(x) => x,
                    None => return,
                };
                if let Mutability::Fixed(reason) = mutability {
                    let note = fixed_note(&reason);
                    self.dg.error_note(*span, reason, note);
                }
                if !ty.is_error() && !ty.is_concrete_int() {
                    self.dg.error_note(
                        *span,
                        format!(
                            "'{}' expects an integer type, found {}",
                            if *up { "++" } else { "--" },
                            self.tcx.name_of(&ty)
                        ),
                        "'x++' is exactly 'x = x + 1', and that needs an integer type",
                    );
                }
            }
            Stmt::If { cond, then, els, .. } => {
                self.check_cond(cond, "if");
                self.check_block(then, false);
                if let Some(e) = els {
                    self.check_stmt(e);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.check_cond(cond, "while");
                self.check_block(body, false);
            }
            Stmt::Break(_) | Stmt::Continue(_) => {
                // The parser already checks the position inside a loop.
            }
            Stmt::For { name, start, end, body, name_span, .. } => {
                let want = self.probe(start).or_else(|| self.probe(end));
                let st = self.expr(start, want.as_ref());
                let et = self.expr(end, want.as_ref().or(Some(&st)));
                let ty = if st.is_error() || et.is_error() {
                    Type::Error
                } else if !st.is_concrete_int() || !et.is_concrete_int() || st != et {
                    self.dg.error_note(
                        start.span,
                        format!(
                            "the range of 'for' needs two values of the same integer type, found {} and {}",
                            self.tcx.name_of(&st),
                            self.tcx.name_of(&et)
                        ),
                        "write e.g. 'for i in 0 as usize..n'",
                    );
                    Type::Error
                } else {
                    st
                };
                self.scopes.push(HashMap::new());
                self.declare_var(name, ty, false, *name_span);
                self.check_block(body, true);
                self.scopes.pop();
            }
            Stmt::Return { value, span } => {
                let want = self.ret.clone();
                match value {
                    None => {
                        if want != Type::Void && !want.is_error() {
                            self.dg.error(
                                *span,
                                format!(
                                    "'return' without value, a value of type {} is expected",
                                    self.tcx.name_of(&want)
                                ),
                            );
                        }
                    }
                    Some(e) => {
                        if want == Type::Void {
                            self.expr(e, Some(&Type::I64));
                            self.dg.error(
                                e.span,
                                "this function has no return type, 'return' must not carry a value",
                            );
                            return;
                        }
                        // HOOK fehlerunionen: implicit conversion (errors.rs)
                        if crate::errors::hook_coerce(self, e, &want) {
                            return;
                        }
                        let got = self.expr(e, Some(&want));
                        if !assignable(&got, &want) {
                            self.dg.error(
                                e.span,
                                format!(
                                    "'return' expects type {}, found {}",
                                    self.tcx.name_of(&want),
                                    self.tcx.name_of(&got)
                                ),
                            );
                        }
                    }
                }
            }
        }
    }

    fn check_cond(&mut self, e: &Expr, kw: &str) {
        let t = self.expr(e, Some(&Type::Bool));
        if !t.is_error() && t != Type::Bool {
            self.dg.error_note(
                e.span,
                format!(
                    "condition of '{}' must be of type bool, found {}",
                    kw,
                    self.tcx.name_of(&t)
                ),
                "there is no implicit conversion, write e.g. 'x != 0'",
            );
        }
    }

    // -------------------------------------------------------------- lvalues

    /// Checks an assignable expression. `None` means: not an lvalue (reported).
    fn lvalue(&mut self, e: &Expr) -> Option<(Type, Mutability)> {
        match &e.kind {
            ExprKind::Ident(name) => {
                // HOOK fnval: a captured value is a COPY — writing to it
                // would look like an effect on the outside and be none
                // (fnval.rs, round 58).
                if self.is_captured(name) {
                    self.dg.error_note(
                        e.span,
                        format!("'{}' is captured by value and cannot be assigned inside the closure", name),
                        "capture a pointer or a Gc[T] if the change is meant to be visible outside",
                    );
                    let ty = self.lookup_var(name).map(|v| v.ty.clone()).unwrap_or(Type::Error);
                    self.record(e.id, ty.clone());
                    return Some((ty, Mutability::Mutable));
                }
                if let Some(v) = self.lookup_var(name) {
                    let ty = v.ty.clone();
                    let m = if v.mutable {
                        Mutability::Mutable
                    } else {
                        Mutability::Fixed(format!(
                            "'{}' is bound with 'let' and cannot be modified",
                            name
                        ))
                    };
                    self.record(e.id, ty.clone());
                    Some((ty, m))
                } else if let Some((ty, _)) = self.consts.get(name) {
                    let ty = ty.clone();
                    self.record(e.id, ty.clone());
                    Some((
                        ty,
                        Mutability::Fixed(format!(
                            "'{}' is a constant and cannot be modified",
                            name
                        )),
                    ))
                } else if let Some((ty, mutable)) = self.statics.get(name) {
                    // ROUND 89: a `static` is a PLACE, so it can stand on
                    // the left of an assignment -- but only with `mut`.
                    let (ty, mutable) = (ty.clone(), *mutable);
                    self.record(e.id, ty.clone());
                    let m = if mutable {
                        Mutability::Mutable
                    } else {
                        Mutability::Fixed(format!(
                            "'{}' is a 'static' without 'mut' and cannot be modified",
                            name
                        ))
                    };
                    Some((ty, m))
                } else {
                    let hint = self.value_hint(name);
                    self.dg
                        .error_maybe_help(e.span, format!("unknown name '{}'", name), hint);
                    self.record(e.id, Type::Error);
                    Some((Type::Error, Mutability::Mutable))
                }
            }
            ExprKind::Field(base, name, nspan) => {
                // HOOK gc: writing THROUGH a `Gc[T]` (gc.rs).
                // `let a: Gc[Node]` binds the HANDLE immutably — the object
                // at the other end stays writable, exactly as with
                // `let p: *mut T` and `(*p).field = …`.
                if let Some(bt) = self.probe(base).filter(crate::gc::is_gc_ptr) {
                    let _ = self.expr(base, None);
                    let ty = self.field_type(&bt, name, *nspan, base.span);
                    self.record(e.id, ty.clone());
                    return Some((ty, Mutability::Mutable));
                }
                let (bt, m) = self.lvalue(base)?;
                let ty = self.field_type(&bt, name, *nspan, base.span);
                self.record(e.id, ty.clone());
                Some((ty, m))
            }
            ExprKind::Index(base, idx) => {
                let (bt, m) = self.lvalue(base)?;
                let ty = self.index_type(&bt, idx, base.span);
                self.record(e.id, ty.clone());
                Some((ty, m))
            }
            ExprKind::Unary(UnOp::Deref, inner) => {
                let t = self.expr(inner, None);
                let ty = match &t {
                    Type::Ptr { inner: i, .. } => (**i).clone(),
                    Type::Error => Type::Error,
                    other => {
                        self.dg.error(
                            e.span,
                            format!(
                                "dereference expects a pointer, found {}",
                                self.tcx.name_of(other)
                            ),
                        );
                        Type::Error
                    }
                };
                self.record(e.id, ty.clone());
                Some((ty, Mutability::Mutable))
            }
            _ => {
                self.expr(e, Some(&Type::I64));
                self.dg.error(
                    e.span,
                    "left side is not an assignable expression (variable, field, index or '*pointer')",
                );
                None
            }
        }
    }

    fn field_type(&mut self, base: &Type, name: &str, nspan: Span, bspan: Span) -> Type {
        // HOOK gc: field access through a `Gc[T]` (gc.rs, SPEC 3.5.1). A Gc
        // pointer is followed without `(*p).field` — it is first class.
        if let Some(i) = crate::gc::hook_field_base(base) {
            return self.field_type(&Type::Struct(i), name, nspan, bspan);
        }
        match base {
            Type::Struct(i) => match self.tcx.structs.get(*i).and_then(|s| s.field(name)) {
                Some(f) => f.ty.clone(),
                None => {
                    let sname = self
                        .tcx
                        .structs
                        .get(*i)
                        .map(|s| s.name.clone())
                        .unwrap_or_else(|| "<struct>".to_string());
                    let hint = self.field_hint(*i, name);
                    self.dg.error_maybe_help(
                        nspan,
                        format!("struct '{}' has no field '{}'", sname, name),
                        hint,
                    );
                    Type::Error
                }
            },
            Type::Error => Type::Error,
            Type::Ptr { inner, .. } if matches!(**inner, Type::Struct(_)) => {
                self.dg.error_note(
                    bspan,
                    format!(
                        "field access on the pointer type {}",
                        self.tcx.name_of(base)
                    ),
                    "there is no automatic dereference, write '(*p).field'",
                );
                Type::Error
            }
            other => {
                self.dg.error(
                    bspan,
                    format!(
                        "field access on the non-struct type {}",
                        self.tcx.name_of(other)
                    ),
                );
                Type::Error
            }
        }
    }

    fn index_type(&mut self, base: &Type, idx: &Expr, bspan: Span) -> Type {
        let it = self.expr(idx, Some(&Type::Usize));
        if !it.is_error() && it != Type::Usize {
            self.dg.error_note(
                idx.span,
                format!(
                    "index must be of type usize, found {}",
                    self.tcx.name_of(&it)
                ),
                "write e.g. 'a[i as usize]'",
            );
        }
        match base {
            Type::Array(e, n) => {
                // ROUND 89 (SPEC 13, item L9): an index that is a NUMBER
                // in the source and a length that is a number in the type
                // is a question with an answer right here. Reporting it at
                // compile time is not an optimisation of the run time
                // check -- it holds at EVERY build level, `release-fast`
                // included, where there is no run time check at all.
                if let Some(v) = self.literal_index(idx) {
                    if v < 0 || v as u64 >= *n {
                        self.dg.error(
                            idx.span,
                            format!(
                                "index {} is outside '{}' (valid: 0 to {})",
                                v,
                                self.tcx.name_of(base),
                                n.saturating_sub(1)
                            ),
                        );
                    }
                }
                (**e).clone()
            }
            Type::Error => Type::Error,
            other => {
                self.dg.error(
                    bspan,
                    format!("index on the non-array type {}", self.tcx.name_of(other)),
                );
                Type::Error
            }
        }
    }

    /// ROUND 89 — the index as a number, if it IS one in the source text.
    ///
    /// Deliberately NOT `eval_const`: that one runs whole functions at
    /// compile time (`comptime.rs`), and asking it about every index
    /// expression of a program would make the type checker run the program.
    /// What is recognised here is what a reader would call a constant index:
    /// a literal, a `const`, a cast or a negation of one.
    fn literal_index(&self, e: &Expr) -> Option<i128> {
        match &e.kind {
            ExprKind::Int(v) => Some(*v),
            ExprKind::Cast(inner, _) => self.literal_index(inner),
            ExprKind::Unary(UnOp::Neg, inner) => self.literal_index(inner).map(|v| -v),
            ExprKind::Ident(n) => self.consts.get(n).map(|(_, v)| *v),
            _ => None,
        }
    }

    // ------------------------------------------------------------ Expressions

    pub(crate) fn record(&mut self, id: ExprId, ty: Type) {
        if let Some(slot) = self.expr_types.get_mut(id as usize) {
            *slot = ty;
        }
    }

    /// Gives every subexpression a concrete type without checking it any
    /// further — used after an error has already been reported, so that no
    /// avalanche of follow-on errors ("type of the literal ...") arises.
    pub(crate) fn type_out_expr(&mut self, e: &Expr) {
        self.expr(e, Some(&Type::I64));
    }

    pub(crate) fn expr(&mut self, e: &Expr, hint: Option<&Type>) -> Type {
        if self.depth >= MAX_DEPTH {
            self.dg.error(
                e.span,
                "expression is nested too deeply (more than 200 levels)",
            );
            self.record(e.id, Type::Error);
            return Type::Error;
        }
        self.depth += 1;
        let t = self.expr_inner(e, hint);
        self.depth -= 1;
        // ROUND 71 — THE ONE IMPLICIT CONVERSION.
        //
        // `f32` -> `f64` loses nothing: every binary32 is exactly
        // representable as a binary64. The other direction throws digits
        // away and therefore needs `as f32` (`tests/neg/f32_no_narrowing.fi`).
        //
        // It sits HERE and nowhere else. `expr` is the single funnel through
        // which every expression with a wanted type passes -- declaration,
        // assignment, argument, `return`, struct field, array element. That
        // is why one place is enough, and why no context can be forgotten.
        if t == Type::F32 && matches!(hint, Some(Type::F64)) {
            self.widen_f32.insert(e.id);
            // `expr_types` keeps the NATURAL type. The lowering builds the
            // value in `f32` first and only then converts -- reading an
            // `f32` variable as if it were eight bytes wide would reach
            // beyond its storage.
            self.record(e.id, t);
            return Type::F64;
        }
        // The same expression can be checked twice with different context
        // types (probe, then the real run). Whoever does NOT widen has to
        // take the mark back, otherwise a conversion would be left standing
        // that nobody asked for.
        self.widen_f32.remove(&e.id);
        self.record(e.id, t.clone());
        t
    }

    fn expr_inner(&mut self, e: &Expr, hint: Option<&Type>) -> Type {
        match &e.kind {
            // ROUND 71 — the float literal is UNTYPED, like the integer
            // literal since round 70. Where the context says `f32`, it is an
            // `f32`; where nothing says anything, `f64` holds -- the default
            // type, as in C#. The suffix `1.5f` is therefore only needed
            // where there is no context at all (SPEC 8.6).
            ExprKind::Float(..) => match hint {
                Some(Type::F32) => Type::F32,
                _ => Type::F64,
            },
            // The suffix decides on its own and lets no context talk it out.
            ExprKind::FloatF32(_) => Type::F32,
            ExprKind::Int(v) => match hint {
                Some(t) if t.is_concrete_int() => {
                    if !lit_fits(*v, t) {
                        self.dg.error(
                            e.span,
                            format!(
                                "integer literal {} does not fit into the type {}",
                                v,
                                self.tcx.name_of(t)
                            ),
                        );
                    }
                    t.clone()
                }
                Some(Type::Error) => Type::Error,
                Some(Type::Bool) => {
                    self.dg.error_note(
                        e.span,
                        "a truth value of type bool is expected here, found an integer literal",
                        "there is no implicit conversion, write e.g. 'x != 0'",
                    );
                    Type::Error
                }
                // ROUND 70 -- THE DEFAULT TYPE. Whoever writes `let x = 5`
                // gets `i32`. The rule stands BEHIND the context: as soon as
                // the surroundings prescribe a type, that one holds
                // (`let y: i64 = 5` is unchanged). Only when nothing at all
                // says anything does the literal fall back to `i32`, exactly
                // as in C#, Java and Go.
                //
                // The overflow check does not soften: a literal that does not
                // fit into `i32` is reported here just as it would be at an
                // explicit `i32` -- with the note that the wider type has to
                // be written down.
                None => {
                    if !lit_fits(*v, &Type::I32) {
                        self.dg.error_note(
                            e.span,
                            format!("integer literal {} does not fit into the type i32", v),
                            "i32 is the default type of an integer literal; write the wider type down, e.g. 'let x: i64 = ...'",
                        );
                        return Type::Error;
                    }
                    Type::I32
                }
                _ => {
                    self.dg.error_note(
                        e.span,
                        "the type of the integer literal cannot be inferred",
                        "give the type, e.g. '5 as i32' or 'let x: i32 = 5'",
                    );
                    Type::Error
                }
            },
            ExprKind::Bool(_) => Type::Bool,
            // HOOK str: a text literal — array literal or `str`, the context
            // decides (strtype.rs, round 70)
            ExprKind::Text(wide, inner) => {
                crate::strtype::check_text(self, e, *wide, inner, hint)
            }
            // HOOK fnval: the closure literal (fnval.rs, round 58)
            ExprKind::Lambda(d) => crate::fnval::check_lambda(self, d),
            ExprKind::Ident(name) => {
                // HOOK fnval: a name out of the enclosing function is a
                // capture (fnval.rs, round 58)
                self.note_use(name);
                if let Some(v) = self.lookup_var(name) {
                    v.ty.clone()
                } else if let Some((t, _)) = self.consts.get(name) {
                    t.clone()
                } else if let Some((t, _)) = self.statics.get(name) {
                    // ROUND 89 -- a global variable read as a value.
                    t.clone()
                } else if let Some(sig) = self.fns.get(name).cloned() {
                    // ROUND 58 (fnval.rs): a named function AS A VALUE. The
                    // value is the address of its function record; the type
                    // is read straight off the signature.
                    Type::Fn { params: sig.params, ret: Box::new(sig.ret) }
                } else {
                    let hint = self.value_hint(name);
                    self.dg
                        .error_maybe_help(e.span, format!("unknown name '{}'", name), hint);
                    Type::Error
                }
            }
            ExprKind::Unary(op, inner) => self.unary(e, *op, inner, hint),
            ExprKind::Binary(op, l, r) => self.binary(e, *op, l, r, hint),
            ExprKind::Field(base, name, nspan) => {
                let bt = self.expr(base, None);
                self.field_type(&bt, name, *nspan, base.span)
            }
            ExprKind::Index(base, idx) => {
                let bt = self.expr(base, None);
                self.index_type(&bt, idx, base.span)
            }
            ExprKind::Call(name, args, nspan) => {
                // HOOK fehlerunionen: `try`, `catch`, `ErrorSet::Variant` (errors.rs)
                if let Some(t) = crate::errors::hook_call(self, e.id, name, args, *nspan, e.span) {
                    return t;
                }
                self.call(name, args, *nspan, e.span)
            }
            ExprKind::Syscall(args) => {
                if args.is_empty() || args.len() > 7 {
                    self.dg.error_note(
                        e.span,
                        format!(
                            "'syscall' expects 1 to 7 arguments (number and up to 6 values), found {}",
                            args.len()
                        ),
                        "call: syscall(nr, a1, ..., a6)",
                    );
                }
                for a in args {
                    let t = self.expr(a, Some(&Type::I64));
                    if !t.is_error() && !t.is_concrete_int() && !t.is_ptr() {
                        self.dg.error(
                            a.span,
                            format!(
                                "'syscall' argument must be an integer or pointer type, found {}",
                                self.tcx.name_of(&t)
                            ),
                        );
                    }
                }
                Type::I64
            }
            ExprKind::Cast(inner, te) => {
                let dst = self.resolve_ty(te);
                let inner_hint = if dst.is_concrete_int() {
                    Some(dst.clone())
                } else if dst == Type::Bool {
                    Some(Type::I64)
                } else if dst.is_ptr() {
                    Some(Type::Usize)
                } else {
                    None
                };
                let src = self.expr(inner, inner_hint.as_ref());
                if src.is_error() || dst.is_error() {
                    return dst;
                }
                // HOOK iface: `x as dyn I` — the interface value (iface.rs)
                if let Some(t) = crate::iface::hook_cast(self, e.span, &src, &dst) {
                    return t;
                }
                // **Round 75** (SPEC §14.5) — `name as *T` / `name as *mut T`
                // where `name` is a DIRECTLY NAMED top-level function: the
                // result is the RAW CODE ADDRESS, so it can be handed to
                // `extern fn` as a C callback (`qsort`'s `compar`, and
                // similar). Restricted on purpose to a bare, directly named
                // function — never a closure, never a value that merely
                // HAPPENS to hold `Type::Fn` (a variable, a struct field, the
                // result of a call): only a directly named function is
                // guaranteed to be a one-word record with NO capture payload
                // (SPEC/ROUND58 — closures without captures share that
                // shape, but there is no source-level way to name one other
                // than through the very literal this rule does not match).
                // A variable of function type still cannot be cast to a raw
                // pointer at all — that would silently hand out a captured
                // closure's record as if it were a bare code pointer, which
                // is exactly the unsound case this restriction rules out.
                if let (Type::Fn { .. }, true) = (&src, dst.is_ptr()) {
                    if let ExprKind::Ident(name) = &inner.kind {
                        if self.fns.contains_key(name) && self.lookup_var(name).is_none() {
                            return dst;
                        }
                    }
                    self.dg.error_note(
                        e.span,
                        format!(
                            "conversion from {} to {} is not allowed",
                            self.tcx.name_of(&src),
                            self.tcx.name_of(&dst)
                        ),
                        "only a directly named function ('name as *T') may be cast to a raw pointer, for use as a C callback (SPEC §14.5) — a value merely of a function type may be a closure with captures and has no bare code address",
                    );
                    return Type::Error;
                }
                let ok = cast_kind(&src) && cast_kind(&dst);
                if !ok {
                    self.dg.error(
                        e.span,
                        format!(
                            "conversion from {} to {} is not allowed",
                            self.tcx.name_of(&src),
                            self.tcx.name_of(&dst)
                        ),
                    );
                    return Type::Error;
                }
                dst
            }
            ExprKind::StructLit(name, fields, nspan) => self.struct_lit(name, fields, *nspan),
            ExprKind::ArrayRepeat(val, count) => {
                let ct = self.expr(count, Some(&Type::Usize));
                let n = if ct.is_error() || !ct.is_concrete_int() {
                    if !ct.is_error() {
                        self.dg.error(
                            count.span,
                            format!(
                                "the length of a repetition literal must be an integer, found {}",
                                self.tcx.name_of(&ct)
                            ),
                        );
                    }
                    0
                } else {
                    match self.eval_const(count) {
                        Ok(v) if v > 0 => v as u64,
                        Ok(_) => {
                            self.dg.error(
                                count.span,
                                "the length of a repetition literal must be greater than zero",
                            );
                            0
                        }
                        Err((sp, msg)) => {
                            self.dg.error(sp, msg);
                            0
                        }
                    }
                };
                let elem_hint = match hint {
                    Some(Type::Array(et, _)) => Some((**et).clone()),
                    _ => None,
                };
                let vt = self.expr(val, elem_hint.as_ref());
                if n == 0 || vt.is_error() {
                    return Type::Error;
                }
                if let Some(Type::Array(et, want_n)) = hint {
                    if !assignable(&vt, et) {
                        self.dg.error(
                            val.span,
                            format!(
                                "element has type {}, expected {}",
                                self.tcx.name_of(&vt),
                                self.tcx.name_of(et)
                            ),
                        );
                    }
                    if *want_n != n {
                        self.dg.error(
                            count.span,
                            format!("{} elements are expected, the literal has {}", want_n, n),
                        );
                    }
                }
                Type::Array(Box::new(vt), n)
            }
            ExprKind::ArrayLit(elems) => match hint {
                Some(Type::Array(et, n)) => {
                    if elems.len() as u64 != *n {
                        self.dg.error(
                            e.span,
                            format!(
                                "array literal has {} elements, {} are expected",
                                elems.len(),
                                n
                            ),
                        );
                    }
                    for el in elems {
                        let t = self.expr(el, Some(et));
                        if !assignable(&t, et) {
                            self.dg.error(
                                el.span,
                                format!(
                                    "element has type {}, expected {}",
                                    self.tcx.name_of(&t),
                                    self.tcx.name_of(et)
                                ),
                            );
                        }
                    }
                    Type::Array(et.clone(), *n)
                }
                _ => {
                    for el in elems {
                        self.type_out_expr(el);
                    }
                    self.dg.error_note(
                        e.span,
                        "the type of the array literal cannot be inferred",
                        "give the type, e.g. 'var a: [i32; 3] = [1, 2, 3]'",
                    );
                    Type::Error
                }
            },
        }
    }

    fn unary(&mut self, e: &Expr, op: UnOp, inner: &Expr, hint: Option<&Type>) -> Type {
        match op {
            UnOp::Neg => {
                // ROUND 68: `f64` counts as well. SPEC §14.1.f64 named "the
                // sign `-x`" as implemented and it was not
                // (docs/ROUND63.md, gap 1). The code generator could do it
                // all along — it flips bit 63 instead of running `neg`,
                // which is what makes `-0.0` come out right.
                let h = self
                    .probe(inner)
                    .or_else(|| hint.filter(|t| t.is_concrete_int() || t.is_float()).cloned());
                let t = self.expr(inner, h.as_ref());
                if t.is_error() {
                    return Type::Error;
                }
                if !(t.is_concrete_int() || t.is_float()) {
                    self.dg.error(
                        e.span,
                        format!(
                            "unary '-' expects an integer or floating point type, found {}",
                            self.tcx.name_of(&t)
                        ),
                    );
                    return Type::Error;
                }
                t
            }
            UnOp::BitNot => {
                let h = self
                    .probe(inner)
                    .or_else(|| hint.filter(|t| t.is_concrete_int()).cloned());
                let t = self.expr(inner, h.as_ref());
                if t.is_error() {
                    return Type::Error;
                }
                if !t.is_concrete_int() {
                    self.dg.error_note(
                        e.span,
                        format!(
                            "unary '~' expects an integer type, found {}",
                            self.tcx.name_of(&t)
                        ),
                        "'~' flips every bit; the logical negation of a bool is '!'",
                    );
                    return Type::Error;
                }
                t
            }
            UnOp::Not => {
                let t = self.expr(inner, Some(&Type::Bool));
                if t.is_error() {
                    return Type::Error;
                }
                if t != Type::Bool {
                    self.dg.error_note(
                        e.span,
                        format!(
                            "unary '!' expects the type bool, found {}",
                            self.tcx.name_of(&t)
                        ),
                        "'!' is the logical negation of a bool; the bitwise one is '~'",
                    );
                    return Type::Error;
                }
                Type::Bool
            }
            UnOp::AddrOf => {
                let (t, _m) = match self.lvalue(inner) {
                    Some(x) => x,
                    None => return Type::Error,
                };
                if t.is_error() {
                    return Type::Error;
                }
                Type::ptr(t, true)
            }
            UnOp::Deref => {
                let t = self.expr(inner, None);
                match &t {
                    Type::Ptr { inner: i, .. } => (**i).clone(),
                    Type::Error => Type::Error,
                    other => {
                        self.dg.error(
                            e.span,
                            format!(
                                "dereference expects a pointer, found {}",
                                self.tcx.name_of(other)
                            ),
                        );
                        Type::Error
                    }
                }
            }
        }
    }

    fn binary(&mut self, e: &Expr, op: BinOp, l: &Expr, r: &Expr, hint: Option<&Type>) -> Type {
        // HOOK fehlerunionen: comparison of two error values (errors.rs)
        if let Some(t) = crate::errors::hook_binary(self, op, l, r, e.span) {
            return t;
        }
        if op.is_logic() {
            let lt = self.expr(l, Some(&Type::Bool));
            let rt = self.expr(r, Some(&Type::Bool));
            for (t, sp) in [(lt, l.span), (rt, r.span)] {
                if !t.is_error() && t != Type::Bool {
                    self.dg.error(
                        sp,
                        format!(
                            "operator '{}' expects operands of type bool, found {}",
                            op.text(),
                            self.tcx.name_of(&t)
                        ),
                    );
                }
            }
            return Type::Bool;
        }
        if op.is_cmp() {
            let want = float_want(self.probe(l), self.probe(r))
                .or_else(|| hint.filter(|t| t.is_float()).cloned());
            let lt = self.expr(l, want.as_ref());
            let rt = self.expr(r, want.as_ref());
            if lt.is_error() || rt.is_error() {
                return Type::Bool;
            }
            // HOOK gc: identity comparison of two related Gc pointers (gc.rs)
            let same = compatible(&lt, &rt) || crate::gc::is_related(&lt, &rt);
            if !same {
                self.dg.error(
                    e.span,
                    format!(
                        "comparison between different types {} and {}",
                        self.tcx.name_of(&lt),
                        self.tcx.name_of(&rt)
                    ),
                );
                return Type::Bool;
            }
            let eq_only = matches!(op, BinOp::Eq | BinOp::Ne);
            // `f64` compares with all six operators. NaN follows IEEE-754
            // in doing so: every comparison except `!=` is false.
            // ROUND 58: two function values compare like two pointers —
            // `==` means "the same function record". Ordering does not
            // exist for them; addresses have no meaningful order.
            // HOOK str: `==`/`!=` compare the CONTENT (strtype.rs, round 70)
            if let Some(t) = crate::strtype::hook_binary(self, op, &lt, &rt, e.span) {
                return t;
            }
            let ok = lt.is_concrete_int()
                || lt.is_float()
                || (eq_only && (lt == Type::Bool || lt.is_ptr() || lt.is_fn()));
            if !ok {
                self.dg.error(
                    e.span,
                    format!(
                        "operator '{}' is not defined for the type {}",
                        op.text(),
                        self.tcx.name_of(&lt)
                    ),
                );
            }
            return Type::Bool;
        }
        if matches!(op, BinOp::Shl | BinOp::Shr) {
            let h = self
                .probe(l)
                .or_else(|| hint.filter(|t| t.is_concrete_int()).cloned());
            let lt = self.expr(l, h.as_ref());
            let rh = self.probe(r).or_else(|| {
                if lt.is_concrete_int() {
                    Some(lt.clone())
                } else {
                    Some(Type::I64)
                }
            });
            let rt = self.expr(r, rh.as_ref());
            if lt.is_error() || rt.is_error() {
                return if lt.is_error() { Type::Error } else { lt };
            }
            return self.binop_type(op, &lt, &rt, e.span);
        }
        // Arithmetic and bit operations: the same integer type on both sides
        // The type of an operand that is already typed takes precedence over the
        // context hint — that way `let x: i64 = a + 1` (a: i32) reports the real
        // error at the assignment instead of a confusing operand error.
        // ROUND 71: with two floating point operands the WIDER one wins, so
        // that `a_f32 + b_f64` and `b_f64 + a_f32` mean the same thing. The
        // hint may name a floating point type as well -- that is what makes
        // `let x: f32 = 1.5 + 2.5` compute in f32.
        let want = float_want(self.probe(l), self.probe(r))
            .or_else(|| hint.filter(|t| t.is_concrete_int() || t.is_float()).cloned());
        let lt = self.expr(l, want.as_ref());
        let rt = self.expr(r, want.as_ref());
        if lt.is_error() || rt.is_error() {
            return Type::Error;
        }
        self.binop_type(op, &lt, &rt, e.span)
    }

    /// **ROUND 70** - the type rules of `a op b` with both operand types
    /// already known.
    ///
    /// `binary` computes with it, and so does the compound assignment
    /// (`x op= e`). That is the point: `x += e` has to mean EXACTLY
    /// `x = x + e`, down to the wording of the message - and it does,
    /// because there is only ONE place where the rules stand.
    pub(crate) fn binop_type(&mut self, op: BinOp, lt: &Type, rt: &Type, span: Span) -> Type {
        let lt = lt.clone();
        let rt = rt.clone();
        if matches!(op, BinOp::Shl | BinOp::Shr) {
            if !lt.is_concrete_int() || !rt.is_concrete_int() {
                self.dg.error(
                    span,
                    format!(
                        "operator '{}' expects integer types, found {} and {}",
                        op.text(),
                        self.tcx.name_of(&lt),
                        self.tcx.name_of(&rt)
                    ),
                );
                return Type::Error;
            }
            return lt;
        }
        // HOOK str: `+` concatenates (strtype.rs, round 70)
        if let Some(t) = crate::strtype::hook_binary(self, op, &lt, &rt, span) {
            return t;
        }
        let e = SpanHolder { span };
        // FLOATING POINT: `+ - * /` are allowed, `%` and the bit operations are
        // not. `%` would be `fmod` and needs a library function; the bit
        // operations would have no sensible meaning on a bit pattern (anyone
        // who needs them converts to `u64` explicitly).
        if lt.is_float() || rt.is_float() {
            let allowed = matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div);
            if !allowed {
                self.dg.error_note(
                    e.span,
                    format!(
                        "operator '{}' is not defined for {}",
                        op.text(),
                        self.tcx.name_of(if lt.is_float() { &lt } else { &rt })
                    ),
                    "allowed are '+', '-', '*', '/' and the comparisons; for the rest convert explicitly",
                );
                return Type::Error;
            }
            if lt != rt {
                self.dg.error_note(
                    e.span,
                    format!(
                        "operator '{}' expects two operands of the same type, found {} and {}",
                        op.text(),
                        self.tcx.name_of(&lt),
                        self.tcx.name_of(&rt)
                    ),
                    "the only implicit conversion is f32 -> f64; the other way round use 'as f32'",
                );
                return Type::Error;
            }
            return lt;
        }
        if !lt.is_concrete_int() || !rt.is_concrete_int() || lt != rt {
            self.dg.error_note(
                e.span,
                format!(
                    "operator '{}' expects two operands of the same integer type, found {} and {}",
                    op.text(),
                    self.tcx.name_of(&lt),
                    self.tcx.name_of(&rt)
                ),
                "there is no implicit conversion, use 'as'",
            );
            return Type::Error;
        }
        lt
    }

    /// **ROUND 70** — the target of one interpolation step.
    ///
    /// The parser writes `io.fmt_value(chain, x)` because at parse time
    /// nobody knows what `x` is. Here the type decides:
    ///
    /// | type of `x` | step |
    /// |---|---|
    /// | integer | `fmt_number` (widened to i64 by the lowering) |
    /// | `bool` | `fmt_bool` |
    /// | `f64` | `fmt_f64` |
    /// | `str` | `fmt_str` |
    ///
    /// Only the LAST name segment is replaced, so that the module prefix
    /// (`io__`, whatever the program called it) stays untouched. The
    /// lowering derives the very same target from the very same material
    /// (`lower::fmt_target`) — no side table between the phases.
    pub(crate) fn fmt_target(&mut self, name: &str, args: &[Expr], span: Span) -> Option<String> {
        let head = fmt_value_head(name)?;
        if args.len() != 2 {
            return None;
        }
        let t = self.expr(&args[1], None);
        // `io.fmt_number` takes an i64. An i32 fits into it WITHOUT loss —
        // the lowering widens it (lower_call). The exception holds for
        // exactly this one argument of exactly this one call, so no general
        // implicit conversion comes into being through the back door.
        if t.is_concrete_int() {
            self.widen.insert(args[1].id);
        }
        let step = match &t {
            _ if t.is_error() => return None,
            // Signed goes to `fmt_number` (i64), unsigned to `fmt_u64`.
            // That is not decoration: a u64 above i64::MAX would wrap around
            // in `fmt_number` and print a negative number.
            _ if t.is_concrete_int() && t.is_signed() => "fmt_number",
            _ if t.is_concrete_int() => "fmt_u64",
            Type::Bool => "fmt_bool",
            Type::F64 => "fmt_f64",
            Type::F32 => "fmt_f32",
            _ if crate::strtype::is_str_like(&t) => "fmt_str",
            other => {
                self.dg.error_note(
                    span,
                    format!(
                        "an interpolation f\"{{…}}\" cannot show a value of type {}",
                        self.tcx.name_of(other)
                    ),
                    "integers, bool, f32, f64 and str work; for everything else turn it into text yourself",
                );
                return None;
            }
        };
        Some(format!("{}{}", head, step))
    }

    fn call(&mut self, name: &str, args: &[Expr], nspan: Span, espan: Span) -> Type {
        // HOOK str: `f"{x}"` — which builder step is right depends on the
        // TYPE of x (strtype.rs / fmt_target, round 70). The name is resolved
        // here and the ordinary check runs on the resolved one.
        if let Some(real) = self.fmt_target(name, args, espan) {
            return self.call(&real, args, nspan, espan);
        }
        // HOOK impl: `x.m(args)` — method call (impls.rs, round 45)
        if let Some(t) = crate::impls::hook_call(self, name, args, nspan, espan) {
            return t;
        }
        // HOOK types: `Enum::Variant(..)` and `match` (sema_match.rs)
        if let Some(t) = crate::sema_match::hook_call(self, name, args, nspan, espan) {
            return t;
        }
        // HOOK constant-time: select/barrier/secure_zero (ct.rs, SPEC §9.2/§9.3)
        // HOOK faden: the three thread primitives (thread.rs, round 49)
        if let Some(t) = crate::thread::hook_call(self, name, args, nspan, espan) {
            return t;
        }
        // HOOK atomar: the atomic primitive (atomic.rs, round 47)
        if let Some(t) = crate::atomic::hook_call(self, name, args, nspan, espan) {
            return t;
        }
        // HOOK simd: the vector and crypto instructions (simd.rs, round 82)
        if let Some(t) = crate::simd::hook_call(self, name, args, nspan, espan) {
            return t;
        }
        if let Some(t) = crate::ct::hook_call(self, name, args, nspan, espan) {
            return t;
        }
        // HOOK sizeof: `size_of[T]()` (sizeof.rs)
        if let Some(t) = crate::sizeof::hook_call(self, name, args, nspan) {
            return t;
        }
        // HOOK kern: `asm(…)` and the eight MMIO names (core.rs, round 52)
        if let Some(t) = crate::core::hook_call(self, name, args, nspan, espan) {
            return t;
        }
        // HOOK gc: `gc C{…}`, `weak(g)`, `strong(w)`, `x.as?[C]` and the
        // collector intrinsics (gc.rs, SPEC 3.5)
        if let Some(t) = crate::gc::hook_call(self, name, args, nspan, espan) {
            return t;
        }
        // ROUND 58 (fnval.rs): the name belongs to a VARIABLE that holds a
        // function. A variable of function type wins over a function of the
        // same name — that is ordinary scoping, and only that way does a
        // parameter named like a global function stay callable.
        self.note_use(name);
        if let Some(v) = self.lookup_var(name) {
            if let Type::Fn { params, ret } = v.ty.clone() {
                let shown = self.tcx.name_of(&Type::Fn {
                    params: params.clone(),
                    ret: ret.clone(),
                });
                if args.len() != params.len() {
                    self.dg.error(
                        espan,
                        format!(
                            "the function value '{}' of type {} expects {} argument(s), found {}",
                            name,
                            shown,
                            params.len(),
                            args.len()
                        ),
                    );
                }
                for (i, a) in args.iter().enumerate() {
                    match params.get(i) {
                        Some(p) => self.check_argument(name, i + 1, a, p),
                        None => self.type_out_expr(a),
                    }
                }
                return *ret;
            }
        }
        let sig = match self.fns.get(name) {
            Some(s) => s.clone(),
            None => {
                for a in args {
                    self.type_out_expr(a);
                }
                if self.lookup_var(name).is_some()
                    || self.consts.contains_key(name)
                    || self.statics.contains_key(name)
                {
                    self.dg.error(
                        nspan,
                        format!("'{}' is not a function and cannot be called", name),
                    );
                } else {
                    let hint = self.value_hint(name);
                    self.dg
                        .error_maybe_help(nspan, format!("unknown function '{}'", name), hint);
                }
                return Type::Error;
            }
        };
        if args.len() != sig.params.len() {
            self.dg.error(
                espan,
                format!(
                    "function '{}' expects {} argument(s), found {}",
                    name,
                    sig.params.len(),
                    args.len()
                ),
            );
        }
        for (i, a) in args.iter().enumerate() {
            match sig.params.get(i) {
                Some(p) => self.check_argument(name, i + 1, a, p),
                None => {
                    self.type_out_expr(a);
                }
            }
        }
        sig.ret
    }

    /// One argument against its parameter type. `nr` is the number that
    /// appears in the message (1-based). For a method call it counts WITHOUT
    /// the receiver — `v.push(x)` has one argument, not two
    /// (impls.rs).
    pub(crate) fn check_argument(&mut self, who: &str, nr: usize, a: &Expr, p: &Type) {
        // HOOK fehlerunionen: implicit conversion (errors.rs)
        if crate::errors::hook_coerce(self, a, p) {
            return;
        }
        let t = self.expr(a, Some(p));
        // ROUND 70: the ONE widening of the interpolation (see `fmt_target`).
        if self.widen.contains(&a.id)
            && t.is_concrete_int()
            && p.is_concrete_int()
            && p.bits() >= t.bits()
        {
            return;
        }
        if !assignable(&t, p) {
            self.dg.error(
                a.span,
                format!(
                    "argument {} of '{}' has type {}, expected {}",
                    nr,
                    who,
                    self.tcx.name_of(&t),
                    self.tcx.name_of(p)
                ),
            );
        }
    }

    fn struct_lit(&mut self, name: &str, fields: &[(String, Expr, Span)], nspan: Span) -> Type {
        let idx = match self.tcx.lookup(name) {
            Some(i) => i,
            None => {
                for (_, e, _) in fields {
                    self.type_out_expr(e);
                }
                let hint = self.type_hint(name);
                self.dg
                    .error_maybe_help(nspan, format!("unknown struct type '{}'", name), hint);
                return Type::Error;
            }
        };
        let def_fields: Vec<(String, Type)> = self
            .tcx
            .structs
            .get(idx)
            .map(|s| s.fields.iter().map(|f| (f.name.clone(), f.ty.clone())).collect())
            .unwrap_or_default();
        let mut seen: HashSet<String> = HashSet::new();
        for (fname, fexpr, fspan) in fields {
            match def_fields.iter().find(|(n, _)| n == fname) {
                Some((_, ft)) => {
                    if !seen.insert(fname.clone()) {
                        self.dg.error(
                            *fspan,
                            format!("field '{}' is given more than once", fname),
                        );
                    }
                    // HOOK fehlerunionen: implicit conversion (errors.rs)
                    if crate::errors::hook_coerce(self, fexpr, ft) {
                        continue;
                    }
                    let t = self.expr(fexpr, Some(ft));
                    if !assignable(&t, ft) {
                        self.dg.error(
                            fexpr.span,
                            format!(
                                "field '{}' has type {}, expected {}",
                                fname,
                                self.tcx.name_of(&t),
                                self.tcx.name_of(ft)
                            ),
                        );
                    }
                }
                None => {
                    self.type_out_expr(fexpr);
                    let hint = self.field_hint(idx, fname);
                    self.dg.error_maybe_help(
                        *fspan,
                        format!("struct '{}' has no field '{}'", name, fname),
                        hint,
                    );
                }
            }
        }
        let missing: Vec<String> = def_fields
            .iter()
            .filter(|(n, _)| !seen.contains(n))
            .map(|(n, _)| n.clone())
            .collect();
        if !missing.is_empty() {
            self.dg.error(
                nspan,
                format!(
                    "struct literal '{}' is missing the field '{}'",
                    name,
                    missing.join("', '")
                ),
            );
        }
        Type::Struct(idx)
    }

    /// Determines the type of an expression without reporting errors and
    /// without writing to the type table. Needed to obtain the type of the
    /// literal in `a + 1` from the other operand.
    fn probe(&self, e: &Expr) -> Option<Type> {
        self.probe_d(e, 0)
    }

    fn probe_d(&self, e: &Expr, d: u32) -> Option<Type> {
        if d >= MAX_DEPTH {
            return None;
        }
        match &e.kind {
            ExprKind::Int(_) => None,
            // ROUND 70: a text literal probes as `str` — that is how the
            // other side of `text == "quit"` gets its type (strtype.rs).
            ExprKind::Text(wide, _) => {
                if *wide {
                    None
                } else {
                    Some(crate::strtype::ty())
                }
            }
            // ROUND 71: an unsuffixed float literal probes as NOTHING -- it
            // adapts, exactly like the integer literal. Only the suffix
            // makes it speak.
            ExprKind::Float(..) => None,
            ExprKind::FloatF32(_) => Some(Type::F32),
            ExprKind::Bool(_) => Some(Type::Bool),
            ExprKind::Ident(n) => {
                if let Some(v) = self.lookup_var(n) {
                    Some(v.ty.clone())
                } else if let Some((t, _)) = self.consts.get(n) {
                    Some(t.clone())
                } else {
                    self.statics.get(n).map(|(t, _)| t.clone())
                }
            }
            ExprKind::Unary(op, inner) => match op {
                UnOp::Neg => self.probe_d(inner, d + 1),
                UnOp::BitNot => self.probe_d(inner, d + 1),
                UnOp::Not => Some(Type::Bool),
                UnOp::AddrOf => self.probe_d(inner, d + 1).map(|t| Type::ptr(t, true)),
                UnOp::Deref => match self.probe_d(inner, d + 1) {
                    Some(Type::Ptr { inner: i, .. }) => Some((*i).clone()),
                    _ => None,
                },
            },
            ExprKind::Binary(op, l, r) => {
                if op.is_cmp() || op.is_logic() {
                    Some(Type::Bool)
                } else if matches!(op, BinOp::Shl | BinOp::Shr) {
                    self.probe_d(l, d + 1)
                } else {
                    // ROUND 71: the same rule as in `binary` -- with two
                    // floating point operands of different width the wider
                    // one is what comes out. Otherwise `a_f32 + b_f64` would
                    // probe as `f32` and the literal next to it would get
                    // the wrong type.
                    float_want(self.probe_d(l, d + 1), self.probe_d(r, d + 1))
                }
            }
            ExprKind::Field(base, name, _) => {
                let bt = self.probe_d(base, d + 1)?;
                // HOOK gc: field access through `Gc[T]` (gc.rs)
                let idx = match bt {
                    Type::Struct(i) => Some(i),
                    ref t => crate::gc::hook_field_base(t),
                };
                idx.and_then(|i| self.tcx.structs.get(i))
                    .and_then(|s| s.field(name))
                    .map(|f| f.ty.clone())
            }
            ExprKind::Index(base, _) => match self.probe_d(base, d + 1) {
                Some(Type::Array(el, _)) => Some((*el).clone()),
                _ => None,
            },
            ExprKind::Call(name, args, _) => {
                // HOOK fnval: a call THROUGH a function value yields the
                // result type of the signature. Without that a literal
                // beside it would get no type — `f(1, 2) != 13`
                // (fnval.rs, round 58).
                if let Some(Type::Fn { ret, .. }) = self.lookup_var(name).map(|v| v.ty.clone()) {
                    return Some(*ret);
                }
                // HOOK fehlerunionen: `try a`/`a catch b` yield the
                // success type of the error union (errors.rs)
                if crate::errors::is_result_call(name) {
                    let inner = args.first().and_then(|a| self.probe_d(a, d + 1))?;
                    return crate::errors::success_type(&inner);
                }
                // HOOK impl: `x.m(..)` yields the return type of the method.
                // Without that, a literal beside it would get no type
                // (`p.sum() != 42`) — the same resolution as in `call`,
                // only without reporting and without writing (impls.rs)
                if let Some(m) = crate::impls::method_name(name) {
                    let et = args.first().and_then(|a| self.probe_d(a, d + 1))?;
                    // HOOK iface: on a `dyn I` the type sits in the
                    // interface, not in the function table (iface.rs)
                    if let Some(iname) = crate::impls::dyn_interface(&self.tcx, &et) {
                        return crate::iface::ret_of(&iname, m);
                    }
                    if let Some((full, _)) =
                        crate::impls::target_of(&self.tcx, &self.fns, m, &et)
                    {
                        return self.fns.get(&full).map(|s| s.ret.clone());
                    }
                    // HOOK fnval (ROUND 68): a FIELD holding a function
                    // value — `c.hook(3, 4) != 12` needs the result type
                    // here too, otherwise the literal beside it gets none.
                    return crate::impls::field_fn(&self.tcx, &et, m).map(|(_, _, r)| r);
                }
                // HOOK sizeof: `size_of[T]()` is always `usize` — without that
                // a literal beside it gets no type (`size_of[u8]() != 1`)
                if crate::sizeof::value(name).is_some() || name.starts_with("size_of$") {
                    return Some(Type::Usize);
                }
                // HOOK gc: type of `weak(g)`, `strong(w)` and `x.as?[C]` WITHOUT
                // a check, so that a literal beside it gets its type (gc.rs)
                let arg0 = args.first().and_then(|a| self.probe_d(a, d + 1));
                if let Some(t) = crate::gc::probe_ty(name, arg0.as_ref()) {
                    return Some(t);
                }
                self.fns.get(name).map(|s| s.ret.clone())
            }
            ExprKind::Syscall(_) => Some(Type::I64),
            ExprKind::Cast(_, te) => self.resolve_ty_quiet(te),
            ExprKind::StructLit(name, _, _) => self.tcx.lookup(name).map(Type::Struct),
            ExprKind::ArrayRepeat(..) => None,
            ExprKind::Lambda(d) => crate::fnval::probe_lambda(self, d),
            ExprKind::ArrayLit(els) => {
                let first = els.first()?;
                let et = self.probe_d(first, d + 1)?;
                Some(Type::Array(Box::new(et), els.len() as u64))
            }
        }
    }

    // ------------------------------------------------------------------ Types

    pub(crate) fn resolve_ty(&mut self, te: &TypeExpr) -> Type {
        self.resolve_ty_d(te, 0)
    }

    fn resolve_ty_d(&mut self, te: &TypeExpr, d: u32) -> Type {
        if d >= MAX_DEPTH {
            self.dg
                .error(te.span(), "type is nested too deeply (more than 200 levels)");
            return Type::Error;
        }
        // HOOK fehlerunionen: error union `E!T` (errors.rs)
        if let Some(t) = crate::errors::hook_resolve_ty(self, te) {
            return t;
        }
        // HOOK gc: `Gc[C]`, `GcWeak[C]` and the forbidden use of a
        // `gc class` name where an ordinary value belongs (gc.rs)
        if let Some(t) = crate::gc::hook_resolve_ty(self, te) {
            return t;
        }
        // HOOK iface: `dyn I` with unknown `I` (iface.rs)
        if let Some(t) = crate::iface::hook_resolve_ty(self, te) {
            return t;
        }
        match te {
            // ROUND 88: the struct lookup asks for the CANONICAL name, so
            // that `string` finds the one builtin `str` (types.rs::alias_of).
            // For the primitive aliases nothing changes -- `prim_type` has
            // already caught `int` and friends one line above.
            TypeExpr::Named(name, span) => match prim_type(name) {
                Some(t) => t,
                None => match self.tcx.lookup(crate::types::canon_name(name)) {
                    Some(i) => Type::Struct(i),
                    None => {
                        let hint = self.type_hint(name);
                        self.dg.error_maybe_help(
                            *span,
                            format!("unknown type '{}'", name),
                            hint,
                        );
                        Type::Error
                    }
                },
            },
            TypeExpr::Ptr { mutable, inner, .. } => {
                let t = self.resolve_ty_d(inner, d + 1);
                if t.is_error() {
                    return Type::Error;
                }
                Type::ptr(t, *mutable)
            }
            TypeExpr::Array { elem, len, span } => {
                let t = self.resolve_ty_d(elem, d + 1);
                if t.is_error() {
                    return Type::Error;
                }
                if *len == 0 {
                    self.dg.error(*span, "array length must be greater than zero");
                    return Type::Error;
                }
                // ROUND 79: `_` as the length only works where there is an
                // initializer to take it from -- in a `let`/`var`, where the
                // parser has already filled it in. Anywhere else (a
                // parameter, a field, a `const`) it has to be caught, or a
                // type of 2^64 elements would go into the layout.
                if *len == crate::ast::LEN_INFER {
                    self.dg.error_note(
                        *span,
                        "the length '_' needs an initializer to be taken from",
                        "it works in a 'let'/'var' with a literal; a parameter, a field and a 'const' have to write the number out",
                    );
                    return Type::Error;
                }
                Type::Array(Box::new(t), *len)
            }
            // Round 58: `fn(T1, T2) -> R` — a function as a value.
            TypeExpr::Fn { params, ret, .. } => {
                let mut ps = Vec::new();
                for p in params {
                    let t = self.resolve_ty_d(p, d + 1);
                    if t.is_error() {
                        return Type::Error;
                    }
                    ps.push(t);
                }
                let r = match ret {
                    Some(t) => {
                        let t = self.resolve_ty_d(t, d + 1);
                        if t.is_error() {
                            return Type::Error;
                        }
                        t
                    }
                    None => Type::Void,
                };
                Type::Fn { params: ps, ret: Box::new(r) }
            }
        }
    }

    /// Type resolution without error messages (for `probe`).
    fn resolve_ty_quiet(&self, te: &TypeExpr) -> Option<Type> {
        match te {
            TypeExpr::Named(name, _) => match prim_type(name) {
                Some(t) => Some(t),
                None => {
                    self.tcx.lookup(crate::types::canon_name(name)).map(Type::Struct)
                }
            },
            TypeExpr::Ptr { mutable, inner, .. } => {
                self.resolve_ty_quiet(inner).map(|t| Type::ptr(t, *mutable))
            }
            TypeExpr::Array { elem, len, .. } => self
                .resolve_ty_quiet(elem)
                .map(|t| Type::Array(Box::new(t), *len)),
            TypeExpr::Fn { params, ret, .. } => {
                let mut ps = Vec::new();
                for p in params {
                    ps.push(self.resolve_ty_quiet(p)?);
                }
                let r = match ret {
                    Some(t) => self.resolve_ty_quiet(t)?,
                    None => Type::Void,
                };
                Some(Type::Fn { params: ps, ret: Box::new(r) })
            }
        }
    }

    // ------------------------------------------------------- Constant values

    fn eval_const(&self, e: &Expr) -> Result<i128, (Span, String)> {
        self.eval_const_d(e, 0)
    }

    fn eval_const_d(&self, e: &Expr, d: u32) -> Result<i128, (Span, String)> {
        let nope = |msg: &str| Err((e.span, msg.to_string()));
        if d >= MAX_DEPTH {
            return nope("constant expression is nested too deeply");
        }
        match &e.kind {
            ExprKind::Int(v) => Ok(*v),
            ExprKind::Bool(b) => Ok(if *b { 1 } else { 0 }),
            ExprKind::Ident(n) => match self.consts.get(n) {
                Some((_, v)) => Ok(*v),
                None => Err((
                    e.span,
                    format!("'{}' is not an already declared constant", n),
                )),
            },
            ExprKind::Unary(op, inner) => {
                let v = self.eval_const_d(inner, d + 1)?;
                match op {
                    UnOp::Neg => Ok(-v),
                    UnOp::Not => Ok(if v == 0 { 1 } else { 0 }),
                    UnOp::BitNot => Ok(!v),
                    _ => nope("a constant expression must not use pointers"),
                }
            }
            ExprKind::Binary(op, l, r) => {
                let a = self.eval_const_d(l, d + 1)?;
                if matches!(op, BinOp::LAnd) && a == 0 {
                    return Ok(0);
                }
                if matches!(op, BinOp::LOr) && a != 0 {
                    return Ok(1);
                }
                let b = self.eval_const_d(r, d + 1)?;
                let lty = self
                    .expr_types
                    .get(l.id as usize)
                    .cloned()
                    .unwrap_or(Type::I64);
                let signed = lty.is_signed();
                let v = match op {
                    BinOp::Add => a + b,
                    BinOp::Sub => a - b,
                    BinOp::Mul => a * b,
                    BinOp::Div | BinOp::Rem => {
                        if b == 0 {
                            return Err((
                                e.span,
                                "division by zero in the constant expression".to_string(),
                            ));
                        }
                        if *op == BinOp::Div {
                            a / b
                        } else {
                            a % b
                        }
                    }
                    BinOp::And => a & b,
                    BinOp::Or => a | b,
                    BinOp::Xor => a ^ b,
                    BinOp::Shl => {
                        if !(0..128).contains(&b) {
                            return Err((
                                e.span,
                                "shift amount in the constant expression is too large".to_string(),
                            ));
                        }
                        a << b
                    }
                    BinOp::Shr => {
                        if !(0..128).contains(&b) {
                            return Err((
                                e.span,
                                "shift amount in the constant expression is too large".to_string(),
                            ));
                        }
                        if signed {
                            a >> b
                        } else {
                            ((a as u128) >> b) as i128
                        }
                    }
                    BinOp::Eq => bit(a == b),
                    BinOp::Ne => bit(a != b),
                    BinOp::Lt => bit(a < b),
                    BinOp::Le => bit(a <= b),
                    BinOp::Gt => bit(a > b),
                    BinOp::Ge => bit(a >= b),
                    BinOp::LAnd => bit(b != 0),
                    BinOp::LOr => bit(b != 0),
                    // ROUND 72 -- explicit wrap/saturate (SPEC section 13,
                    // item L9). Wrapping needs nothing beyond plain
                    // arithmetic: the `wrap(v, &rty)` call right below this
                    // match already narrows to the destination type, which
                    // IS two's complement wrapping.
                    BinOp::AddWrap => a + b,
                    BinOp::SubWrap => a - b,
                    BinOp::MulWrap => a * b,
                    BinOp::AddSat | BinOp::SubSat | BinOp::MulSat => {
                        return Err((
                            e.span,
                            "'+|'/'-|'/'*|' (saturating arithmetic) is not                              supported in a constant expression yet -- use it                              at run time"
                                .to_string(),
                        ));
                    }
                };
                let rty = self
                    .expr_types
                    .get(e.id as usize)
                    .cloned()
                    .unwrap_or(Type::I64);
                Ok(wrap(v, &rty))
            }
            ExprKind::Cast(inner, _) => {
                let v = self.eval_const_d(inner, d + 1)?;
                let dst = self
                    .expr_types
                    .get(e.id as usize)
                    .cloned()
                    .unwrap_or(Type::I64);
                if dst == Type::Bool {
                    return Ok(bit(v != 0));
                }
                if dst.is_ptr() {
                    return nope("pointers are not allowed in a constant expression");
                }
                Ok(wrap(v, &dst))
            }
            // COMPTIME: a call is EXECUTED at compile time — with loops,
            // branches and recursion (comptime.rs). That makes table sizes
            // and key figures computable instead of working them out by hand
            // and writing them down as a literal.
            ExprKind::Call(name, args, _) => {
                let mut values = Vec::with_capacity(args.len());
                for a in args {
                    values.push(self.eval_const_d(a, d + 1)?);
                }
                let prog = match self.prog {
                    // SAFE: set in `run`/`add_items_inner`, valid for the
                    // duration of this pass.
                    Some(p) => unsafe { &*p },
                    None => return nope("comptime: the program is not available here"),
                };
                let mut run =
                    crate::comptime::Execution::new(prog, &self.consts, &self.expr_types);
                run.call_on(name, &values, e.span, 0)
            }
            _ => nope("a constant expression must be evaluable at compile time (only literals, constants, operators and calls)"),
        }
    }
}

/// Round 70: only so that the body of `binop_type`, which was moved over
/// from `binary`, can keep writing `e.span`.
struct SpanHolder {
    span: Span,
}

fn bit(b: bool) -> i128 {
    if b {
        1
    } else {
        0
    }
}

/// **ROUND 70** — is this the interpolation placeholder, and what stands in
/// front of its last segment? `io__fmt_value` -> `Some("io__")`.
pub(crate) fn fmt_value_head(name: &str) -> Option<&str> {
    let rest = name.strip_suffix("fmt_value")?;
    if rest.is_empty() || rest.ends_with('_') || rest.ends_with('.') {
        Some(rest)
    } else {
        None
    }
}

fn prim_type(name: &str) -> Option<Type> {
    // ROUND 70: `int`/`long`/`byte`/... are only a second spelling; they
    // are folded onto the canonical name here and are therefore THE SAME
    // type (types.rs::canon_name).
    Some(match crate::types::canon_name(name) {
        "i8" => Type::I8,
        "i16" => Type::I16,
        "i32" => Type::I32,
        "i64" => Type::I64,
        "u8" => Type::U8,
        "u16" => Type::U16,
        "u32" => Type::U32,
        "u64" => Type::U64,
        "usize" => Type::Usize,
        "isize" => Type::Isize,
        "bool" => Type::Bool,
        "f64" => Type::F64,
        "f32" => Type::F32,
        // ROUND 82 (simd.rs, SPEC 8.7): the 128-bit vector register.
        "v128" => Type::V128,
        _ => return None,
    })
}

/// Cut a value to the width/signedness of the target type — for the
/// `comptime` interpreter as well (`comptime.rs`).
pub(crate) fn comptime_wrap(v: i128, t: &Type) -> i128 {
    wrap(v, t)
}

/// Cut a value to the width/signedness of the target type.
/// ROUND 89 — which advice fits the refusal. `let`/`var` is the right
/// answer for a local and a nonsense one for a global: a `static` is never
/// a `var`, it grows a `mut` instead. One place decides, so the three call
/// sites of `Mutability::Fixed` cannot drift apart.
fn fixed_note(reason: &str) -> &'static str {
    if reason.contains("'static' without 'mut'") {
        "write 'static mut NAME: T = ...' if it is meant to change (SPEC 14.1.statics)"
    } else if reason.contains("is a constant") {
        "a 'const' is a number folded into every use site; use 'static mut' for a place that changes"
    } else {
        "use 'var' instead of 'let'"
    }
}

fn wrap(v: i128, t: &Type) -> i128 {
    let bits = t.bits();
    if bits == 0 {
        return v;
    }
    if bits >= 64 {
        return if t.is_signed() {
            v as i64 as i128
        } else {
            (v as u64) as i128
        };
    }
    let m = (1i128 << bits) - 1;
    let x = v & m;
    if t.is_signed() && (x >> (bits - 1)) & 1 == 1 {
        x - (1i128 << bits)
    } else {
        x
    }
}

/// Does the literal fit into the type (read signed OR unsigned)?
fn lit_fits(v: i128, t: &Type) -> bool {
    let bits = t.bits() as i128;
    if bits >= 64 {
        return v >= i64::MIN as i128 && v <= u64::MAX as i128;
    }
    let ub = 1i128 << bits;
    v >= -(ub / 2) && v < ub
}

/// May this type take part in an `as` conversion?
fn cast_kind(t: &Type) -> bool {
    t.is_concrete_int() || *t == Type::Bool || t.is_ptr() || t.is_float()
}

/// **ROUND 71** — the wanted type of a binary operation out of the two
/// probes.
///
/// The rule is the same for arithmetic and for comparisons and it is
/// SYMMETRIC: if both sides are floating point but of different width, the
/// wider one wins (`f32 + f64` is an `f64` computation, and so is
/// `f64 + f32`). Without that the meaning would depend on the order of
/// writing, which is exactly the sort of surprise this language does not
/// want.
fn float_want(pl: Option<Type>, pr: Option<Type>) -> Option<Type> {
    match (&pl, &pr) {
        (Some(a), Some(b)) if a.is_float() && b.is_float() && a != b => Some(Type::F64),
        _ => pl.or(pr),
    }
}

/// Assignment compatibility — there are NO implicit conversions. The only
/// leniency: the `mut` marking of a pointer is not checked (stage 0 has no
/// mutability checker for pointer targets).
fn assignable(got: &Type, want: &Type) -> bool {
    if got.is_error() || want.is_error() {
        return true;
    }
    // HOOK gc: free upcast `Gc[Derived]` -> `Gc[Base]`
    // (gc.rs, SPEC 4.4). ONLY in that direction; downwards goes exclusively
    // through the checked `x.as?[C]`.
    if crate::gc::is_upward(got, want) {
        return true;
    }
    compatible(got, want)
}

fn compatible(a: &Type, b: &Type) -> bool {
    if a.is_error() || b.is_error() {
        return true;
    }
    // HOOK str: the language type `str` and a view of the same shape
    // (`str.Span`) may be used for each other — same two words, same ABI,
    // no copy of the octets (strtype.rs, round 70).
    if crate::strtype::same_view(a, b) {
        return true;
    }
    match (a, b) {
        (Type::Ptr { inner: x, .. }, Type::Ptr { inner: y, .. }) => compatible(x, y),
        // ROUND 58: function types are compared EXACTLY — same arity, same
        // parameter types, same result. A pointer compares leniently here
        // (see above); for a call target that would be wrong, because the
        // callee reads the memory behind it.
        (Type::Fn { .. }, Type::Fn { .. }) => a == b,
        _ => a == b,
    }
}

/// Collects all structs that `ty` contains BY VALUE (pointers do not count).
fn collect_value_deps(ty: &Type, out: &mut Vec<usize>) {
    match ty {
        Type::Struct(i) => out.push(*i),
        Type::Array(e, _) => collect_value_deps(e, out),
        _ => {}
    }
}

/// Depth first search: detects cycles and yields a topological order
/// (dependencies first) for the layout computation.
fn find_cycles(
    i: usize,
    deps: &[Vec<usize>],
    state: &mut Vec<u8>,
    order: &mut Vec<usize>,
    bad: &mut HashSet<usize>,
) {
    match state.get(i) {
        Some(2) => return,
        Some(1) => {
            bad.insert(i);
            return;
        }
        None => return,
        _ => {}
    }
    state[i] = 1;
    if let Some(list) = deps.get(i) {
        for d in list.clone() {
            find_cycles(d, deps, state, order, bad);
            if bad.contains(&d) {
                bad.insert(i);
            }
        }
    }
    state[i] = 2;
    order.push(i);
}

/// Reachability analysis: does EVERY path in the block end with 'return'?
pub(crate) fn block_returns(b: &Block) -> bool {
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
        // A 'while true' loop WITHOUT 'break' never leaves the body.
        Stmt::While { cond, body, .. } => {
            matches!(cond.kind, ExprKind::Bool(true)) && !block_breaks(body)
        }
        // HOOK types: a complete 'match' whose cases all return
        // returns itself (sema_match.rs).
        Stmt::Expr(e) => crate::sema_match::match_returns(e),
        _ => false,
    }
}

/// Does the block contain a `break` that leaves THIS loop (so not one from
/// an inner loop)?
fn block_breaks(b: &Block) -> bool {
    b.stmts.iter().any(stmt_breaks)
}

fn stmt_breaks(s: &Stmt) -> bool {
    match s {
        Stmt::Break(_) => true,
        Stmt::Block(b) => block_breaks(b),
        Stmt::If { then, els, .. } => {
            block_breaks(then) || els.as_deref().map(stmt_breaks).unwrap_or(false)
        }
        // A 'break' in an inner loop leaves only that one.
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Self check
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        Block, ConstDecl, Expr, ExprKind, FnDecl, Param, Program, Stmt, StructDecl, TypeExpr,
    };

    fn sp() -> Span {
        Span::new(1, 1, 1)
    }

    struct B {
        next: u32,
    }

    impl B {
        fn new() -> B {
            B { next: 0 }
        }
        fn e(&mut self, k: ExprKind) -> Expr {
            let id = self.next;
            self.next += 1;
            Expr { id, span: sp(), kind: k }
        }
        fn int(&mut self, v: i128) -> Expr {
            self.e(ExprKind::Int(v))
        }
        fn id(&mut self, n: &str) -> Expr {
            self.e(ExprKind::Ident(n.to_string()))
        }
    }

    // ------------------------------------------------------------------
    // Re-entry into the check phases (DESIGN_GOALS.md §7)
    // ------------------------------------------------------------------

    /// Builds a checker in the state *after* the first pass.
    fn checker_after_first_run<'d>(
        dg: &'d mut Diags,
        first: &Program,
    ) -> Checker<'d> {
        let mut ck = Checker {
        widen: std::collections::HashSet::new(),
        widen_f32: HashSet::new(),
            dg,
            tcx: TypeCtx::new(),
            fns: HashMap::new(),
            consts: HashMap::new(),
            statics: HashMap::new(),
            expr_types: vec![Type::Error; first.expr_count as usize],
            scopes: Vec::new(),
            ret: Type::Void,
            depth: 0,
            must_consume_fns: HashSet::new(),
            capture_frames: Vec::new(),
            prog: None,
        };
        ck.run(first);
        ck
    }

    #[test]
    fn check_phases_take_later_generated_elems_an() {
        // First pass: `main` and `basis` only.
        let mut b = B::new();
        let ret_base = b.int(7);
        let base = FnDecl {
            name: "base".to_string(),
            params: Vec::new(),
            ret: Some(named("i32")),
            body: blk(vec![Stmt::Return { value: Some(ret_base), span: sp() }]),
            span: sp(),
            attrs: Vec::new(),
            extern_info: None,
        };
        let ret_main = b.int(0);
        let first = Program {
            funcs: vec![base, main_fn(vec![Stmt::Return { value: Some(ret_main), span: sp() }])],
            expr_count: b.next,
            ..Default::default()
        };

        let mut dg = Diags::new("test.fi", "");
        let mut ck = checker_after_first_run(&mut dg, &first);
        assert!(!ck.dg.has_errors(), "first run must be free of errors");
        assert!(ck.fns.contains_key("base"));
        assert!(!ck.fns.contains_key("later"));

        // Second pass: a function that did not exist the first time and that
        // reaches into a function of the first pass.
        // That is exactly what `comptime emit` has to do later.
        let call = b.e(ExprKind::Call("base".to_string(), Vec::new(), sp()));
        let addendum = Program {
            funcs: vec![FnDecl {
                name: "later".to_string(),
                params: Vec::new(),
                ret: Some(named("i32")),
                body: blk(vec![Stmt::Return { value: Some(call), span: sp() }]),
                span: sp(),
                attrs: Vec::new(),
                extern_info: None,
            }],
            expr_count: b.next,
            ..Default::default()
        };
        ck.add_items(&addendum);

        assert!(!ck.dg.has_errors(), "addendum must run through without errors");
        assert!(ck.fns.contains_key("later"), "the new function is missing");
        // The call really did get a type — the table grew along with it.
        assert_eq!(ck.expr_types.len(), b.next as usize);
        assert_eq!(ck.fns["later"].ret, Type::I32);
    }

    #[test]
    fn addendum_becomes_likewise_strict_checked() {
        let mut b = B::new();
        let ret_main = b.int(0);
        let first = Program {
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret_main), span: sp() }])],
            expr_count: b.next,
            ..Default::default()
        };
        let mut dg = Diags::new("test.fi", "");
        let mut ck = checker_after_first_run(&mut dg, &first);
        assert!(!ck.dg.has_errors());

        // The addendum calls something that does not exist -> the same error as
        // in the first pass. An addendum must not be a back door.
        let call = b.e(ExprKind::Call("does_not_exist".to_string(), Vec::new(), sp()));
        let addendum = Program {
            funcs: vec![FnDecl {
                name: "broken".to_string(),
                params: Vec::new(),
                ret: Some(named("i32")),
                body: blk(vec![Stmt::Return { value: Some(call), span: sp() }]),
                span: sp(),
                attrs: Vec::new(),
                extern_info: None,
            }],
            expr_count: b.next,
            ..Default::default()
        };
        ck.add_items(&addendum);
        assert!(ck.dg.has_errors(), "unknown name in the addendum must be noticed");
    }

    #[test]
    fn addendum_reports_duplicate_decl() {
        let mut b = B::new();
        let ret_main = b.int(0);
        let first = Program {
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret_main), span: sp() }])],
            expr_count: b.next,
            ..Default::default()
        };
        let mut dg = Diags::new("test.fi", "");
        let mut ck = checker_after_first_run(&mut dg, &first);

        let ret2 = b.int(1);
        let addendum = Program {
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret2), span: sp() }])],
            expr_count: b.next,
            ..Default::default()
        };
        ck.add_items(&addendum);
        assert!(ck.dg.has_errors(), "'main' twice must be an error");
    }

    fn named(n: &str) -> TypeExpr {
        TypeExpr::Named(n.to_string(), sp())
    }

    fn blk(stmts: Vec<Stmt>) -> Block {
        Block { stmts, span: sp(), end: sp() }
    }

    /// main function with the given body and `return <ret>`.
    fn main_fn(body: Vec<Stmt>) -> FnDecl {
        FnDecl {
            name: "main".to_string(),
            params: Vec::new(),
            ret: Some(named("i32")),
            body: blk(body),
            span: sp(), attrs: Vec::new(), extern_info: None,
        }
    }

    fn run(prog: Program, src: &str) -> (Option<TypeInfo>, String) {
        let mut dg = Diags::new("test.x", src);
        let info = check(&prog, &mut dg);
        (info, dg.render())
    }

    /// Builds a program whose main contains nothing but `return <e>`.
    fn prog_with(b: &mut B, ret: Expr) -> Program {
        Program {
            profile: None,
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret), span: sp() }])],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            expr_count: b.next,
            comptime_blocks: Vec::new(),
        }
    }

    fn expect_err(prog: Program, needle: &str) {
        let (info, out) = run(prog, "");
        assert!(info.is_none(), "expected error '{}' did not appear", needle);
        assert!(
            out.contains(needle),
            "message '{}' is missing in:\n{}",
            needle,
            out
        );
    }

    // ---- struct layout (explicitly demanded) ------------------------------

    fn layout_of(fields: &[(&str, &str)]) -> (Vec<u64>, u64, u64) {
        let mut b = B::new();
        let ret = b.int(0);
        let sd = StructDecl {
            name: "S".to_string(),
            fields: fields
                .iter()
                .map(|(n, t)| (n.to_string(), named(t), sp()))
                .collect(),
            span: sp(), attrs: Vec::new(),
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret), span: sp() }])],
            structs: vec![sd],
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        let (info, out) = run(prog, "");
        let info = info.unwrap_or_else(|| panic!("layout program faulty:\n{}", out));
        // ROUND 70: index 0 is the builtin `str` — look it up by name.
        let s = &info.tcx.structs[info.tcx.lookup("S").unwrap_or_default()];
        (s.fields.iter().map(|f| f.offset).collect(), s.size, s.align)
    }

    #[test]
    fn layout_u8_u32_u8() {
        let (offs, size, align) = layout_of(&[("a", "u8"), ("b", "u32"), ("c", "u8")]);
        assert_eq!(offs, vec![0, 4, 8]);
        assert_eq!(size, 12);
        assert_eq!(align, 4);
    }

    #[test]
    fn layout_packed_order() {
        let (offs, size, align) = layout_of(&[("a", "i64"), ("b", "i8"), ("c", "i16")]);
        assert_eq!(offs, vec![0, 8, 10]);
        assert_eq!(size, 16);
        assert_eq!(align, 8);
    }

    #[test]
    fn layout_bool_and_ptr() {
        let mut b = B::new();
        let ret = b.int(0);
        let sd = StructDecl {
            name: "S".to_string(),
            fields: vec![
                ("f".to_string(), named("bool"), sp()),
                (
                    "p".to_string(),
                    TypeExpr::Ptr { mutable: true, inner: Box::new(named("u8")), span: sp() },
                    sp(),
                ),
                (
                    "a".to_string(),
                    TypeExpr::Array { elem: Box::new(named("u16")), len: 3, span: sp() },
                    sp(),
                ),
            ],
            span: sp(),
            attrs: Vec::new(),
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret), span: sp() }])],
            structs: vec![sd],
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        let (info, out) = run(prog, "");
        let info = info.unwrap_or_else(|| panic!("error:\n{}", out));
        // ROUND 70: index 0 is the builtin `str` — look it up by name.
        let s = &info.tcx.structs[info.tcx.lookup("S").unwrap_or_default()];
        assert_eq!(s.fields[0].offset, 0);
        assert_eq!(s.fields[1].offset, 8);
        assert_eq!(s.fields[2].offset, 16);
        assert_eq!(s.size, 24);
        assert_eq!(s.align, 8);
    }

    #[test]
    fn layout_nested_struct() {
        let mut b = B::new();
        let ret = b.int(0);
        let inner = StructDecl {
            name: "Inner".to_string(),
            fields: vec![
                ("x".to_string(), named("u8"), sp()),
                ("y".to_string(), named("u32"), sp()),
            ],
            span: sp(), attrs: Vec::new(),
        };
        // The outer struct comes BEFORE the inner one -> topological order needed.
        let outer = StructDecl {
            name: "Outer".to_string(),
            fields: vec![
                ("a".to_string(), named("u8"), sp()),
                ("i".to_string(), named("Inner"), sp()),
            ],
            span: sp(), attrs: Vec::new(),
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret), span: sp() }])],
            structs: vec![outer, inner],
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        let (info, out) = run(prog, "");
        let info = info.unwrap_or_else(|| panic!("error:\n{}", out));
        // ROUND 70: index 0 is the builtin `str` — look it up by name.
        let o = &info.tcx.structs[info.tcx.lookup("Outer").unwrap_or_default()];
        assert_eq!(o.fields[0].offset, 0);
        assert_eq!(o.fields[1].offset, 4);
        assert_eq!(o.size, 12);
        assert_eq!(o.align, 4);
    }

    #[test]
    fn recursive_struct_is_error() {
        let mut b = B::new();
        let ret = b.int(0);
        let a = StructDecl {
            name: "A".to_string(),
            fields: vec![("b".to_string(), named("B"), sp())],
            span: sp(), attrs: Vec::new(),
        };
        let bs = StructDecl {
            name: "B".to_string(),
            fields: vec![("a".to_string(), named("A"), sp())],
            span: sp(), attrs: Vec::new(),
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret), span: sp() }])],
            structs: vec![a, bs],
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "contains itself");
    }

    // ---- one error case per check -----------------------------------------

    #[test]
    fn ok_program_types_everything() {
        let mut b = B::new();
        let lit = b.int(7);
        let x = b.id("x");
        let one = b.int(1);
        let sum = b.e(ExprKind::Binary(BinOp::Add, Box::new(x), Box::new(one)));
        let prog = Program {
            profile: Some(("app".to_string(), sp())),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: false,
                    ty: Some(named("i32")),
                    init: lit,
                    span: sp(),
                },
                Stmt::Return { value: Some(sum), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            expr_count: b.next,
            comptime_blocks: Vec::new(),
        };
        let (info, out) = run(prog, "");
        let info = info.unwrap_or_else(|| panic!("unexpected error:\n{}", out));
        assert_eq!(info.expr_types.len(), 4);
        for t in &info.expr_types {
            assert!(!t.is_error() && *t != Type::UntypedInt, "type {:?}", t);
        }
        assert_eq!(info.expr_types[0], Type::I32);
    }

    /// **ROUND 70** — `let x = 5` is no longer an error: without any
    /// context the literal has the default type `i32`.
    #[test]
    fn untyped_literal_becomes_i32() {
        let mut b = B::new();
        let lit = b.int(5);
        let ret = b.int(0);
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: false,
                    ty: None,
                    init: lit,
                    span: sp(),
                },
                Stmt::Return { value: Some(ret), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        let (info, out) = run(prog, "");
        let info = info.unwrap_or_else(|| panic!("unexpected error:\n{}", out));
        assert_eq!(info.expr_types[0], Type::I32);
    }

    /// The literal still cannot be inferred where the context is no integer
    /// type at all — there the default must NOT jump in.
    #[test]
    fn untyped_literal_at_a_pointer_is_error() {
        let mut b = B::new();
        let lit = b.int(5);
        let ret = b.int(0);
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "p".to_string(),
                    mutable: false,
                    ty: Some(TypeExpr::Ptr {
                        mutable: true,
                        inner: Box::new(named("u8")),
                        span: sp(),
                    }),
                    init: lit,
                    span: sp(),
                },
                Stmt::Return { value: Some(ret), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "the type of the integer literal cannot be inferred");
    }

    /// **ROUND 70** — the overflow check does not soften at the default type.
    #[test]
    fn untyped_literal_over_i32_is_error() {
        let mut b = B::new();
        let lit = b.int(5_000_000_000);
        let ret = b.int(0);
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: false,
                    ty: None,
                    init: lit,
                    span: sp(),
                },
                Stmt::Return { value: Some(ret), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "does not fit into the type i32");
    }

    /// **ROUND 70** — `int` and `i32` are THE SAME type, not two.
    #[test]
    fn the_alias_is_the_same_type() {
        for (alias, canonical) in [
            ("sbyte", "i8"),
            ("short", "i16"),
            ("int", "i32"),
            ("long", "i64"),
            ("byte", "u8"),
            ("ushort", "u16"),
            ("uint", "u32"),
            ("ulong", "u64"),
            ("double", "f64"),
            // ROUND 71: `float` is given out now, and it means `f32`.
            ("float", "f32"),
        ] {
            assert_eq!(prim_type(alias), prim_type(canonical), "{}", alias);
        }
        assert_eq!(prim_type("float"), Some(Type::F32));
        // ROUND 88: `string` belongs to the same family, but it cannot be
        // checked with `prim_type` -- `str` is the builtin STRUCT, not a
        // primitive. So the pair is checked one level lower, on the name;
        // `resolve_ty` looks the struct up under exactly that name, and
        // `tools/firstrun/run.sh` case 08 proves end to end that a function
        // taking a `str` accepts a `string` and the other way round.
        assert_eq!(crate::types::canon_name("string"), "str");
        assert_eq!(prim_type("string"), None);
        assert_eq!(prim_type("str"), None);
    }

    #[test]
    fn unknown_name_is_error() {
        let mut b = B::new();
        let x = b.id("nix");
        let prog = prog_with(&mut b, x);
        expect_err(prog, "unknown name 'nix'");
    }

    #[test]
    fn wrong_arg_count_is_error() {
        let mut b = B::new();
        let a1 = b.int(1);
        let call = b.e(ExprKind::Call("f".to_string(), vec![a1], sp()));
        let fret = b.int(0);
        let f = FnDecl {
            name: "f".to_string(),
            params: vec![
                Param { name: "a".to_string(), ty: named("i32"), span: sp() },
                Param { name: "b".to_string(), ty: named("i32"), span: sp() },
            ],
            ret: Some(named("i32")),
            body: blk(vec![Stmt::Return { value: Some(fret), span: sp() }]),
            span: sp(),
            attrs: Vec::new(),
            extern_info: None,
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(call), span: sp() }]), f],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "expects 2 argument(s), found 1");
    }

    #[test]
    fn missing_return_is_error() {
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(Vec::new())],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: 0,
        };
        expect_err(prog, "reaches the end without 'return'");
    }

    #[test]
    fn assign_to_let_is_error() {
        let mut b = B::new();
        let init = b.int(1);
        let tgt = b.id("x");
        let val = b.int(2);
        let ret = b.int(0);
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: false,
                    ty: Some(named("i32")),
                    init,
                    span: sp(),
                },
                Stmt::Assign { target: tgt, value: val, span: sp() },
                Stmt::Return { value: Some(ret), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "bound with 'let'");
    }

    #[test]
    fn index_on_non_array_is_error() {
        let mut b = B::new();
        let init = b.int(1);
        let base = b.id("x");
        let idx = b.int(0);
        let ix = b.e(ExprKind::Index(Box::new(base), Box::new(idx)));
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: false,
                    ty: Some(named("i32")),
                    init,
                    span: sp(),
                },
                Stmt::Return { value: Some(ix), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "index on the non-array type i32");
    }

    #[test]
    fn field_on_non_struct_is_error() {
        let mut b = B::new();
        let init = b.int(1);
        let base = b.id("x");
        let f = b.e(ExprKind::Field(Box::new(base), "y".to_string(), sp()));
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: false,
                    ty: Some(named("i32")),
                    init,
                    span: sp(),
                },
                Stmt::Return { value: Some(f), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "field access on the non-struct type i32");
    }

    #[test]
    fn deref_non_pointer_is_error() {
        let mut b = B::new();
        let init = b.int(1);
        let base = b.id("x");
        let d = b.e(ExprKind::Unary(UnOp::Deref, Box::new(base)));
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: false,
                    ty: Some(named("i32")),
                    init,
                    span: sp(),
                },
                Stmt::Return { value: Some(d), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "dereference expects a pointer");
    }

    #[test]
    fn condition_must_be_bool() {
        let mut b = B::new();
        let c = b.int(1);
        let ret = b.int(0);
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::If { cond: c, then: blk(Vec::new()), els: None, span: sp() },
                Stmt::Return { value: Some(ret), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "a truth value of type bool is expected here");
    }

    #[test]
    fn mixed_int_types_are_error() {
        let mut b = B::new();
        let ia = b.int(1);
        let ib = b.int(2);
        let x = b.id("x");
        let y = b.id("y");
        let sum = b.e(ExprKind::Binary(BinOp::Add, Box::new(x), Box::new(y)));
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let { name: "x".into(), mutable: false, ty: Some(named("i32")), init: ia, span: sp() },
                Stmt::Let { name: "y".into(), mutable: false, ty: Some(named("i64")), init: ib, span: sp() },
                Stmt::Return { value: Some(sum), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "of the same integer type, found i32 and i64");
    }

    #[test]
    /// Round 2: aggregates at function boundaries are ALLOWED (SPEC §14.1,
    /// point 1 dropped). The type checker accepts them, `abi.rs` classifies them.
    fn aggregate_parameter_is_allowed() {
        let mut b = B::new();
        let ret = b.int(0);
        let f = FnDecl {
            name: "f".to_string(),
            params: vec![Param {
                name: "a".to_string(),
                ty: TypeExpr::Array { elem: Box::new(named("i32")), len: 4, span: sp() },
                span: sp(),
            }],
            ret: None,
            body: blk(Vec::new()),
            span: sp(),
            attrs: Vec::new(),
            extern_info: None,
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret), span: sp() }]), f],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        let (info, out) = run(prog, "");
        let info = info.unwrap_or_else(|| panic!("aggregate parameter rejected:\n{}", out));
        let sig = info.fns.get("f").expect("signature of f");
        assert_eq!(sig.params[0], Type::Array(Box::new(Type::I32), 4));
        assert_eq!(
            crate::abi::classify(&sig.params[0], &info.tcx),
            crate::abi::ArgClass::ints(2)
        );
    }

    #[test]
    fn missing_main_is_error() {
        let prog = Program::default();
        expect_err(prog, "no function 'main'");
    }

    #[test]
    fn bad_cast_is_error() {
        let mut b = B::new();
        let lit = b.int(1);
        let s = b.e(ExprKind::StructLit(
            "P".to_string(),
            vec![("x".to_string(), lit, sp())],
            sp(),
        ));
        let c = b.e(ExprKind::Cast(Box::new(s), named("i32")));
        let sd = StructDecl {
            name: "P".to_string(),
            fields: vec![("x".to_string(), named("i32"), sp())],
            span: sp(), attrs: Vec::new(),
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(c), span: sp() }])],
            structs: vec![sd],
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "conversion from P to i32 is not allowed");
    }

    #[test]
    fn struct_literal_missing_field() {
        let mut b = B::new();
        let lit = b.int(1);
        let s = b.e(ExprKind::StructLit(
            "P".to_string(),
            vec![("x".to_string(), lit, sp())],
            sp(),
        ));
        let ret = b.int(0);
        let sd = StructDecl {
            name: "P".to_string(),
            fields: vec![
                ("x".to_string(), named("i32"), sp()),
                ("y".to_string(), named("i32"), sp()),
            ],
            span: sp(), attrs: Vec::new(),
        };
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let { name: "p".into(), mutable: false, ty: Some(named("P")), init: s, span: sp() },
                Stmt::Return { value: Some(ret), span: sp() },
            ])],
            structs: vec![sd],
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "is missing the field 'y'");
    }

    #[test]
    fn const_is_evaluated() {
        let mut b = B::new();
        let a = b.int(6);
        let bb = b.int(7);
        let mul = b.e(ExprKind::Binary(BinOp::Mul, Box::new(a), Box::new(bb)));
        let k = b.id("K");
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(k), span: sp() }])],
            structs: Vec::new(),
            statics: Vec::new(),
            consts: vec![ConstDecl {
                name: "K".to_string(),
                ty: named("i32"),
                value: mul,
                span: sp(),
            }],
            expr_count: b.next,
            comptime_blocks: Vec::new(),
        };
        let (info, out) = run(prog, "");
        let info = info.unwrap_or_else(|| panic!("error:\n{}", out));
        assert_eq!(info.consts.get("K"), Some(&(Type::I32, 42)));
    }

    #[test]
    fn const_division_by_zero_is_error() {
        let mut b = B::new();
        let a = b.int(1);
        let z = b.int(0);
        let div = b.e(ExprKind::Binary(BinOp::Div, Box::new(a), Box::new(z)));
        let ret = b.int(0);
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![Stmt::Return { value: Some(ret), span: sp() }])],
            structs: Vec::new(),
            statics: Vec::new(),
            consts: vec![ConstDecl {
                name: "K".to_string(),
                ty: named("i32"),
                value: div,
                span: sp(),
            }],
            expr_count: b.next,
            comptime_blocks: Vec::new(),
        };
        expect_err(prog, "division by zero");
    }

    #[test]
    fn syscall_and_pointer_flow() {
        // var b: u8 = 65; syscall(1, 1, &b, 1); return 0
        let mut b = B::new();
        let init = b.int(65);
        let n1 = b.int(1);
        let n2 = b.int(1);
        let bid = b.id("b");
        let addr = b.e(ExprKind::Unary(UnOp::AddrOf, Box::new(bid)));
        let n3 = b.int(1);
        let sc = b.e(ExprKind::Syscall(vec![n1, n2, addr, n3]));
        let ret = b.int(0);
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let { name: "b".into(), mutable: true, ty: Some(named("u8")), init, span: sp() },
                Stmt::Expr(sc),
                Stmt::Return { value: Some(ret), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        let (info, out) = run(prog, "");
        let info = info.unwrap_or_else(|| panic!("error:\n{}", out));
        for t in &info.expr_types {
            assert!(!t.is_error());
        }
    }

    #[test]
    fn addr_of_non_lvalue_is_error() {
        let mut b = B::new();
        let lit = b.int(1);
        let a = b.e(ExprKind::Unary(UnOp::AddrOf, Box::new(lit)));
        let p = b.e(ExprKind::Unary(UnOp::Deref, Box::new(a)));
        let prog = prog_with(&mut b, p);
        expect_err(prog, "not an assignable expression");
    }

    #[test]
    fn index_must_be_usize() {
        let mut b = B::new();
        let i0 = b.int(0);
        let i1 = b.int(1);
        let i2 = b.int(2);
        let arr = b.e(ExprKind::ArrayLit(vec![i0, i1, i2]));
        let base = b.id("a");
        let iexpr = b.id("i");
        let ix = b.e(ExprKind::Index(Box::new(base), Box::new(iexpr)));
        let iinit = b.int(0);
        let prog = Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs: vec![main_fn(vec![
                Stmt::Let {
                    name: "a".into(),
                    mutable: false,
                    ty: Some(TypeExpr::Array { elem: Box::new(named("i32")), len: 3, span: sp() }),
                    init: arr,
                    span: sp(),
                },
                Stmt::Let { name: "i".into(), mutable: false, ty: Some(named("i32")), init: iinit, span: sp() },
                Stmt::Return { value: Some(ix), span: sp() },
            ])],
            structs: Vec::new(),
            consts: Vec::new(),
            statics: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: b.next,
        };
        expect_err(prog, "index must be of type usize");
    }
}

/// Finds a jump (`return`/`break`/`continue`) that leads OUT of a `defer`
/// body. Jumps inside a loop that begins within the body itself are
/// allowed — they do not leave the body.
pub(crate) fn defer_jump(s: &Stmt) -> Option<(Span, &'static str)> {
    match s {
        Stmt::Return { span, .. } => Some((*span, "return")),
        Stmt::Break(span) => Some((*span, "break")),
        Stmt::Continue(span) => Some((*span, "continue")),
        Stmt::Block(b) => b.stmts.iter().find_map(defer_jump),
        Stmt::Defer(inner, _, _) => defer_jump(inner),
        Stmt::If { then, els, .. } => then
            .stmts
            .iter()
            .find_map(defer_jump)
            .or_else(|| els.as_deref().and_then(defer_jump)),
        // Inside `while`/`for` a `break`/`continue` may appear: they belong to
        // this loop and do not leave the body. A `return` does.
        Stmt::While { body, .. } | Stmt::For { body, .. } => {
            body.stmts.iter().find_map(defer_return_only)
        }
        _ => None,
    }
}

/// Like `defer_jump`, but `return` only — for bodies inside a loop that
/// begins within the `defer` itself.
fn defer_return_only(s: &Stmt) -> Option<(Span, &'static str)> {
    match s {
        Stmt::Return { span, .. } => Some((*span, "return")),
        Stmt::Block(b) => b.stmts.iter().find_map(defer_return_only),
        Stmt::Defer(inner, _, _) => defer_return_only(inner),
        Stmt::If { then, els, .. } => then
            .stmts
            .iter()
            .find_map(defer_return_only)
            .or_else(|| els.as_deref().and_then(defer_return_only)),
        Stmt::While { body, .. } | Stmt::For { body, .. } => {
            body.stmts.iter().find_map(defer_return_only)
        }
        _ => None,
    }
}
