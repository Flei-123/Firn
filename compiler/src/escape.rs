//! **Round 79 — the escape analysis.** A raw pointer into a LOCAL must not
//! outlive the frame that local lives in (gap 9 of `docs/ROUND66.md`).
//!
//! ```firn
//! fn bad() -> *mut i64 {
//!     var x: i64 = 5
//!     return &x          // x is dead on return -- the pointer is not
//! }
//! ```
//!
//! Until this round the compiler said nothing about that. It is now an
//! error at COMPILE TIME, with the three places the reader needs: where the
//! address is taken, where the variable dies, and where the pointer gets
//! out.
//!
//! ## What this is NOT
//!
//! It is not a borrow checker and it grows no lifetime annotations
//! (`DESIGN_GOALS.md` §2 — Firn is not Rust). The rule of this file is:
//!
//! > catch what can be decided from the shape of the program alone, and
//! > where it cannot be decided, ALLOW.
//!
//! Every place where that costs a catch is written down in
//! `docs/ROUND79.md` §4 as a named gap. A false alarm would be much worse
//! than a missed case: it would make correct programs unbuildable and
//! teach everybody to switch the check off.
//!
//! ## The model: sources and sinks
//!
//! A pointer value carries a set of SOURCES (`Src`):
//!
//! * `Src::Local` — it points into the frame of the function being checked
//!   (a `let`/`var`, an array of it, a field of it, or a PARAMETER: a
//!   parameter slot lies in the frame too).
//! * `Src::Param(i)` — it is derived from the i-th parameter. Nothing is
//!   known about that frame here; the CALLER decides.
//!
//! Sources travel through address arithmetic, casts, aggregate literals and
//! assignments to locals. They do NOT travel through a LOAD out of memory
//! (`*p`, `(*p).f`, `p[i]`): what lies in the heap is data, not the address
//! of the pointer that led there. That distinction is what keeps
//! `vec_push` honest and `(*v).ptr` quiet.
//!
//! A source reaches a SINK (`Sink`) when it
//!
//! * is returned (`Sink::Return`),
//! * is written through a pointer that does not belong to this frame — a
//!   pointer parameter is `Sink::Out(j)`, anything else `Sink::Foreign`,
//! * is handed to a thread (`Sink::Thread`, the primitive
//!   `__thread_start`).
//!
//! For a `Src::Local` a sink is the error. For a `Src::Param(i)` it is not
//! an error but a FACT about the function, which is written into its
//! SUMMARY and used at every call site of it. That is how the check crosses
//! function boundaries without a single annotation:
//!
//! * `flow[i]` contains `Return` → the result of a call inherits the
//!   sources of argument `i` (`fn first(v: *mut V) -> *mut u8` hands its
//!   argument through; that is not a capture and must not be an error).
//! * `flow[i]` contains `Out(j)` → the sources of argument `i` land in what
//!   argument `j` points at (the out-parameter idiom).
//! * `flow[i]` contains `Foreign` → the function KEEPS the pointer
//!   somewhere that outlives it. Handing it the address of a local is the
//!   error, reported at the CALL.
//!
//! The summaries are computed to a fixed point over the whole program
//! before anything is reported, so the fact travels along chains of calls.
//!
//! ## The way out
//!
//! `#[allow_escape]` on a function switches the check off for that
//! function's body, and stays visible in the source while doing it — a
//! hardware address, a kernel page table or a stack a thread is handed do
//! sometimes have to leave the frame (SPEC §2). Nothing is switched off
//! silently and nothing is switched off globally.
//!
//! Wired up through the line `// HOOK escape` in `sema::Checker::run`. The
//! twin in Firn is `lib/firnc1/escape.fi`; both have to reject the same
//! programs with the same text, which `tools/escape/run.sh` checks.

use std::collections::HashMap;

use crate::ast::{BinOp, Block, Expr, ExprKind, FnDecl, Program, Stmt, UnOp};
use crate::diag::{Diag, Span};
use crate::sema::Checker;
use crate::types::Type;

/// The thread primitive of round 49 (`thread.rs`). Its first argument is
/// handed to a NEW stack that outlives this frame.
const THREAD_START: &str = "__thread_start";

/// Upper bound for the fixed point over the call graph. Every round can only
/// ADD sinks and the set of sinks per parameter is finite, so the loop ends
/// by itself; the bound is the second safeguard, exactly like `MAX_DEPTH`
/// in `nogc.rs`.
const MAX_ROUNDS: u32 = 12;

/// Deepest nesting of `match` bodies that is still walked.
const MAX_DEPTH: u32 = 256;

/// Where a pointer goes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Sink {
    /// out through the return value
    Return,
    /// written into what parameter `j` points at
    Out(usize),
    /// written through a pointer that belongs to nobody known here
    Foreign,
    /// handed to a new thread
    Thread,
}

/// Where a pointer came from.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Src {
    /// The frame of the function being checked. `name` is the local,
    /// `decl` where it is declared, `take` where its address is taken.
    Local { name: String, decl: Span, take: Span },
    /// Derived from the i-th parameter — somebody else's frame.
    Param(usize),
}

/// The text that names the way out, for the message.
#[derive(Clone, Debug)]
enum Route {
    Return,
    /// write through `*name`
    Ptr(String),
    /// write through a computed pointer
    ForeignPtr,
    /// handed to `callee` as argument `n` (1-based), which keeps it
    Call(String, usize),
    Thread,
}

impl Route {
    fn text(&self) -> String {
        match self {
            Route::Return => "through the return value".to_string(),
            Route::Ptr(n) => format!("through the pointer '{n}'"),
            Route::ForeignPtr => {
                "through a pointer that does not point into this frame".to_string()
            }
            Route::Call(f, n) => format!("into '{f}', which keeps its argument {n}"),
            Route::Thread => "into a new thread".to_string(),
        }
    }
    fn help(&self, who: &str) -> String {
        let tail = format!("'#[allow_escape]' on '{who}' switches this check off");
        match self {
            Route::Return => format!(
                "return the VALUE, or let the caller own the storage and pass a pointer in \
                 ('fn {who}(out: *mut T)'); {tail}"
            ),
            Route::Ptr(_) | Route::ForeignPtr => format!(
                "copy the value instead of its address, or put it on the GC heap with 'gc'; \
                 {tail}"
            ),
            Route::Call(f, _) => format!(
                "give '{f}' storage that outlives this frame -- the GC heap, or the caller's \
                 frame; {tail}"
            ),
            Route::Thread => format!(
                "the thread outlives this frame: hand it storage from the GC heap; {tail}"
            ),
        }
    }
}

/// Does the function carry `#[allow_escape]`?
pub(crate) fn has_allow_escape(f: &FnDecl) -> bool {
    f.attrs.iter().any(|a| a.name == "allow_escape")
}

/// A generated closure body (`__closure#N`, round 58). Deliberately NOT
/// checked — see `docs/ROUND79.md` §4, gap E4. It is skipped in BOTH
/// compilers, so that the two never disagree about it.
fn is_closure(name: &str) -> bool {
    name.starts_with("__closure#")
}

/// Call name produced by the compiler itself (`__match#N`, `__try#`,
/// `Enum::Variant`, `asm$N`) — no function of the source text.
fn is_internal(name: &str) -> bool {
    name.contains('#') || name.contains("::") || name.contains('$')
}

/// Write `helper__square` (module system, `modules.rs`) as `helper.square`
/// again — the message shall show the name that stands in the source. Same
/// rule as `nogc::readable`.
fn readable(name: &str) -> String {
    if name.starts_with('_') || is_internal(name) {
        return name.to_string();
    }
    let parts: Vec<&str> = name.split("__").collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        return format!("{}.{}", parts[0], parts[1]);
    }
    name.to_string()
}

/// `// HOOK escape` in `sema::Checker::run`.
pub(crate) fn hook_check(ck: &mut Checker, prog: &Program) {
    let findings = collect_findings(prog, &ck.expr_types);
    for (span, msg, note, help) in findings {
        ck.dg.report(Diag {
            msg,
            span,
            label: "here".to_string(),
            note: Some(note),
            help: Some(help),
        });
    }
}

/// The pass proper, without `Checker` — testable on its own that way.
fn collect_findings(prog: &Program, tys: &[Type]) -> Vec<(Span, String, String, String)> {
    // --- phase 1: the summaries, to a fixed point ---------------------------
    let mut sums: HashMap<String, Vec<Vec<Sink>>> = HashMap::new();
    for f in &prog.funcs {
        // Declared twice is an error of the type check; the first entry wins
        // here and the run stays deterministic.
        sums.entry(f.name.clone())
            .or_insert_with(|| vec![Vec::new(); f.params.len()]);
    }
    for _round in 0..MAX_ROUNDS {
        let mut fresh: Vec<(String, Vec<Vec<Sink>>)> = Vec::new();
        for f in &prog.funcs {
            // `#[allow_escape]` vouches for the whole function, its callers
            // included: its summary stays EMPTY, so no call of it is blamed
            // for what it does with the pointers it is given. Anything else
            // would make the way out useless -- the author would silence the
            // function and the caller would go red instead.
            if f.extern_info.is_some() || is_closure(&f.name) || has_allow_escape(f) {
                continue;
            }
            let mut p = Pass::new(prog, &sums, tys, f, false);
            p.run(f);
            if sums.get(&f.name) != Some(&p.found) {
                fresh.push((f.name.clone(), p.found));
            }
        }
        if fresh.is_empty() {
            break;
        }
        for (n, v) in fresh {
            sums.insert(n, v);
        }
    }
    // --- phase 2: the report ------------------------------------------------
    let mut out: Vec<(Span, String, String, String)> = Vec::new();
    for f in &prog.funcs {
        if f.extern_info.is_some() || is_closure(&f.name) || has_allow_escape(f) {
            continue;
        }
        let mut p = Pass::new(prog, &sums, tys, f, true);
        p.run(f);
        out.append(&mut p.out);
    }
    out.sort_by_key(|(s, _, _, _)| (s.file, s.line, s.col));
    out.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    out
}

struct Pass<'a> {
    sums: &'a HashMap<String, Vec<Vec<Sink>>>,
    tys: &'a [Type],
    /// Name of the function being checked, as it stands in the source.
    who: String,
    /// Position of the closing brace of its body — where the frame dies.
    fend: Span,
    /// Parameter names in order; the index is the `Src::Param`.
    params: Vec<String>,
    /// Every local of the function (parameters included) with the place it
    /// is declared at.
    locals: HashMap<String, Span>,
    /// Which local currently holds an address, and where that address is from.
    taint: HashMap<String, Vec<Src>>,
    /// What the parameters of THIS function do — the summary being built.
    found: Vec<Vec<Sink>>,
    report: bool,
    depth: u32,
    out: Vec<(Span, String, String, String)>,
    _prog: &'a Program,
}

/// The root of a place expression (`x`, `a[i]`, `s.f`, `(*p).f`).
enum Root<'a> {
    /// a local of this frame
    Local(&'a str),
    /// behind this pointer expression
    Behind(&'a Expr),
    /// something the analysis cannot name
    Other,
}

impl<'a> Pass<'a> {
    fn new(
        prog: &'a Program,
        sums: &'a HashMap<String, Vec<Vec<Sink>>>,
        tys: &'a [Type],
        f: &FnDecl,
        report: bool,
    ) -> Pass<'a> {
        Pass {
            sums,
            tys,
            who: readable(&f.name),
            fend: f.body.end,
            params: f.params.iter().map(|p| p.name.clone()).collect(),
            locals: HashMap::new(),
            taint: HashMap::new(),
            found: vec![Vec::new(); f.params.len()],
            report,
            depth: 0,
            out: Vec::new(),
            _prog: prog,
        }
    }

    fn run(&mut self, f: &FnDecl) {
        for p in &f.params {
            self.locals.insert(p.name.clone(), p.span);
        }
        collect_locals(&f.body, &mut self.locals);
        for (i, p) in f.params.iter().enumerate() {
            self.taint.insert(p.name.clone(), vec![Src::Param(i)]);
        }
        self.block(&f.body);
    }

    // --------------------------------------------------------------- helpers

    fn is_ptr(&self, e: &Expr) -> bool {
        matches!(self.tys.get(e.id as usize), Some(Type::Ptr { .. }))
    }

    fn is_local(&self, name: &str) -> bool {
        self.locals.contains_key(name)
    }

    /// The sources of the VALUE of `e`.
    fn value(&self, e: &Expr) -> Vec<Src> {
        match &e.kind {
            ExprKind::Ident(n) => self.taint_under(n),
            ExprKind::Unary(UnOp::AddrOf, inner) => self.address_of(inner, e.span),
            // A LOAD out of memory yields data, not the address that led
            // there. Without this line the analysis would call every
            // `(*v).ptr` a pointer into the caller's frame and `vec_push`
            // would never be recognised as storing.
            ExprKind::Unary(UnOp::Deref, _) => Vec::new(),
            ExprKind::Unary(_, a) => self.value(a),
            ExprKind::Cast(a, _) => self.value(a),
            // Pointer arithmetic keeps the source; `&a[0] + 3` still points
            // into `a`.
            ExprKind::Binary(BinOp::Add, a, b) => {
                let mut v = self.value(a);
                add_all(&mut v, self.value(b));
                v
            }
            // The DIFFERENCE of two addresses is a distance and no address at
            // all -- `(&p.b) as usize - (&p.a) as usize` is how `lib/std/rc.fi`
            // measures the stride of a type. Only `address - number` stays an
            // address.
            ExprKind::Binary(BinOp::Sub, a, b) => {
                if self.value(b).is_empty() {
                    self.value(a)
                } else {
                    Vec::new()
                }
            }
            // Reading a field/element of a LOCAL aggregate: whatever was put
            // into that very field is what comes out.
            ExprKind::Field(..) | ExprKind::Index(..) => match self.path(e) {
                Some(p) => self.taint_under(&p),
                None => Vec::new(),
            },
            ExprKind::StructLit(_, fields, _) => {
                let mut v = Vec::new();
                for (_, x, _) in fields {
                    add_all(&mut v, self.value(x));
                }
                v
            }
            ExprKind::ArrayLit(xs) => {
                let mut v = Vec::new();
                for x in xs {
                    add_all(&mut v, self.value(x));
                }
                v
            }
            ExprKind::ArrayRepeat(x, _) => self.value(x),
            ExprKind::Text(_, inner) => self.value(inner),
            ExprKind::Call(name, args, _) => self.call_result(name, args),
            _ => Vec::new(),
        }
    }

    /// The sources of the ADDRESS of the place `e` (`&e`). `at` is where
    /// the `&` stands, for the message.
    fn address_of(&self, e: &Expr, at: Span) -> Vec<Src> {
        match &e.kind {
            ExprKind::Ident(n) => match self.locals.get(n) {
                Some(decl) => vec![Src::Local {
                    name: n.clone(),
                    decl: *decl,
                    take: at,
                }],
                // A constant lies in `.rodata` and outlives everything.
                None => Vec::new(),
            },
            // `&(*p).f` and `&p[i]` are `p` plus an offset — the frame of
            // the local `p` has nothing to do with it.
            ExprKind::Unary(UnOp::Deref, p) => self.value(p),
            ExprKind::Field(b, _, _) | ExprKind::Index(b, _) => {
                if self.is_ptr(b) {
                    self.value(b)
                } else {
                    self.address_of(b, at)
                }
            }
            _ => Vec::new(),
        }
    }

    /// The PLACE PATH of an expression, if it is a place of this frame that
    /// no pointer leads to: `sub`, `sub.lx`, `st.konts`, `a[]`.
    ///
    /// Field paths and not just the root local: `lib/firnc1/parser.fi` copies
    /// a whole `Parser`, puts the address of a local lexer into ONE field and
    /// afterwards reads a DIFFERENT field out of it. Rooted at the local
    /// alone, the second read would drag the first field's address along and
    /// the compiler would reject a correct program.
    fn path(&self, e: &Expr) -> Option<String> {
        match &e.kind {
            ExprKind::Ident(n) => {
                if self.is_local(n) {
                    Some(n.clone())
                } else {
                    None
                }
            }
            ExprKind::Field(b, f, _) => {
                if self.is_ptr(b) {
                    None
                } else {
                    self.path(b).map(|p| format!("{p}.{f}"))
                }
            }
            ExprKind::Index(b, _) => {
                if self.is_ptr(b) {
                    None
                } else {
                    self.path(b).map(|p| format!("{p}[]"))
                }
            }
            _ => None,
        }
    }

    /// Everything marked at this place or BELOW it: reading the whole struct
    /// reads its fields with it.
    fn taint_under(&self, path: &str) -> Vec<Src> {
        let mut out = Vec::new();
        for (k, v) in &self.taint {
            if k == path || k.starts_with(&format!("{path}.")) || k.starts_with(&format!("{path}["))
            {
                add_all(&mut out, v.clone());
            }
        }
        out
    }

    /// Writing into a place of this frame: it replaces exactly that place and
    /// everything below it.
    fn write_local(&mut self, path: &str, srcs: Vec<Src>) {
        let under: Vec<String> = self
            .taint
            .keys()
            .filter(|k| {
                k.as_str() == path
                    || k.starts_with(&format!("{path}."))
                    || k.starts_with(&format!("{path}["))
            })
            .cloned()
            .collect();
        for k in under {
            self.taint.remove(&k);
        }
        if !srcs.is_empty() {
            self.taint.insert(path.to_string(), srcs);
        }
    }

    /// Where a place expression is rooted.
    fn root<'e>(&self, e: &'e Expr) -> Root<'e> {
        match &e.kind {
            ExprKind::Ident(n) => {
                if self.is_local(n) {
                    Root::Local(n)
                } else {
                    Root::Other
                }
            }
            ExprKind::Unary(UnOp::Deref, p) => Root::Behind(p),
            ExprKind::Field(b, _, _) | ExprKind::Index(b, _) => {
                if self.is_ptr(b) {
                    Root::Behind(b)
                } else {
                    self.root(b)
                }
            }
            _ => Root::Other,
        }
    }

    /// The summary of a callee, if there is one.
    fn summary(&self, name: &str) -> Option<&'a Vec<Vec<Sink>>> {
        self.sums.get(name)
    }

    /// What a call yields: the sources of every argument that the callee
    /// passes through to its result.
    fn call_result(&self, name: &str, args: &[Expr]) -> Vec<Src> {
        let flows = match self.summary(name) {
            Some(f) => f,
            None => return Vec::new(),
        };
        let mut v = Vec::new();
        for (i, fl) in flows.iter().enumerate() {
            if fl.contains(&Sink::Return) {
                if let Some(a) = args.get(i) {
                    add_all(&mut v, self.value(a));
                }
            }
        }
        v
    }

    // ----------------------------------------------------------------- sinks

    /// A set of sources reaches a sink. A `Param` is a fact for the summary,
    /// a `Local` is the error.
    fn flow(&mut self, srcs: &[Src], sink: Sink, route: &Route, at: Span) {
        for s in srcs {
            match s {
                Src::Param(i) => {
                    if let Some(slot) = self.found.get_mut(*i) {
                        if !slot.contains(&sink) {
                            slot.push(sink);
                            slot.sort();
                        }
                    }
                }
                Src::Local { name, decl, take } => {
                    if self.report {
                        self.blame(name, *decl, *take, route, at);
                    }
                }
            }
        }
    }

    fn blame(&mut self, name: &str, decl: Span, take: Span, route: &Route, at: Span) {
        let who = self.who.clone();
        let msg = format!(
            "the address of the local '{name}' escapes {}",
            route.text()
        );
        let note = format!(
            "'&{name}' at {}:{} points into the frame of '{who}'; '{name}' is declared at \
             {}:{} and dies at {}:{}, where '{who}' returns",
            take.line, take.col, decl.line, decl.col, self.fend.line, self.fend.col
        );
        let help = route.help(&who);
        let at = if at.is_none() { take } else { at };
        self.out.push((at, msg, note, help));
    }

    /// The sources `srcs` are written into the place `target`.
    ///
    /// Three outcomes: it stays in this frame (then the local that owns the
    /// place takes the sources over), it goes into what a parameter points
    /// at (`Out(j)`), or it goes somewhere unknown (`Foreign`).
    fn store(&mut self, target: &Expr, srcs: Vec<Src>, at: Span) {
        if let Some(p) = self.path(target) {
            self.write_local(&p, srcs);
            return;
        }
        if srcs.is_empty() {
            return;
        }
        match self.root(target) {
            // A place of this frame the path did not catch (an index behind a
            // cast and the like): mark the root local, coarsely but safely.
            Root::Local(n) => {
                let n = n.to_string();
                let cur = self.taint.entry(n).or_default();
                for s in srcs {
                    if !cur.contains(&s) {
                        cur.push(s);
                    }
                }
            }
            Root::Behind(p) => {
                let pt = self.value(p);
                // Into the frame itself: `var b: Box; let q = &b; (*q).p = &x`
                // writes into `b`, and `b` lives exactly as long as `x`.
                if pt.iter().any(|s| matches!(s, Src::Local { .. })) {
                    return;
                }
                let (sink, route) = match pt.iter().find_map(|s| match s {
                    Src::Param(i) => Some(*i),
                    _ => None,
                }) {
                    Some(i) => (
                        Sink::Out(i),
                        Route::Ptr(self.params.get(i).cloned().unwrap_or_default()),
                    ),
                    None => (Sink::Foreign, Route::ForeignPtr),
                };
                self.flow(&srcs, sink, &route, at);
            }
            Root::Other => {
                self.flow(&srcs, Sink::Foreign, &Route::ForeignPtr, at);
            }
        }
    }

    // ------------------------------------------------------------ statements

    fn block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            // A deferred statement runs in the same frame, under the same rules.
            Stmt::Defer(inner, _, _) => self.stmt(inner),
            Stmt::Let { name, init, .. } => {
                self.expr(init);
                let v = self.value(init);
                self.write_local(name, v);
            }
            Stmt::Assign { target, value, span } => {
                self.expr(value);
                self.expr(target);
                let v = self.value(value);
                self.store(target, v, *span);
            }
            Stmt::AssignOp { target, value, span, .. } => {
                self.expr(value);
                self.expr(target);
                // `p += n` keeps whatever `p` already pointed into.
                let mut v = self.value(target);
                add_all(&mut v, self.value(value));
                self.store(target, v, *span);
            }
            // `p++` changes no source.
            Stmt::Step { target, .. } => self.expr(target),
            Stmt::If { cond, then, els, .. } => {
                self.expr(cond);
                let before = self.taint.clone();
                self.block(then);
                let after_then = std::mem::replace(&mut self.taint, before);
                if let Some(e) = els {
                    self.stmt(e);
                }
                merge(&mut self.taint, after_then);
            }
            Stmt::While { cond, body, .. } => {
                self.expr(cond);
                // Twice, so that what the end of the body marks is visible at
                // its beginning. A third round can add nothing that the
                // second did not already carry into the merged state.
                let before = self.taint.clone();
                self.block(body);
                merge(&mut self.taint, before);
                let before = self.taint.clone();
                self.block(body);
                merge(&mut self.taint, before);
            }
            Stmt::For { name, start, end, body, name_span, .. } => {
                self.expr(start);
                self.expr(end);
                self.locals.entry(name.clone()).or_insert(*name_span);
                let before = self.taint.clone();
                self.block(body);
                merge(&mut self.taint, before);
                let before = self.taint.clone();
                self.block(body);
                merge(&mut self.taint, before);
            }
            Stmt::Return { value, span } => {
                if let Some(v) = value {
                    self.expr(v);
                    let srcs = self.value(v);
                    self.flow(&srcs, Sink::Return, &Route::Return, *span);
                }
            }
            Stmt::Expr(e) => self.expr(e),
            Stmt::Block(b) => self.block(b),
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Error(_) => {}
        }
    }

    // ----------------------------------------------------------- expressions

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Call(name, args, sp) => {
                for a in args {
                    self.expr(a);
                }
                let sp = if sp.is_none() { e.span } else { *sp };
                self.call(name, args, sp);
                self.match_cases(name);
            }
            // Round 58: a closure body is its own function and is checked
            // nowhere — see gap E4 in docs/ROUND79.md.
            ExprKind::Lambda(_) => {}
            ExprKind::Unary(_, a) => self.expr(a),
            ExprKind::Binary(_, a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::Field(b, _, _) => self.expr(b),
            ExprKind::Index(b, i) => {
                self.expr(b);
                self.expr(i);
            }
            ExprKind::Syscall(args) | ExprKind::ArrayLit(args) => {
                for a in args {
                    self.expr(a);
                }
            }
            ExprKind::Cast(a, _) => self.expr(a),
            ExprKind::Text(_, inner) => self.expr(inner),
            ExprKind::StructLit(_, fields, _) => {
                for (_, v, _) in fields {
                    self.expr(v);
                }
            }
            ExprKind::ArrayRepeat(v, n) => {
                self.expr(v);
                self.expr(n);
            }
            _ => {}
        }
    }

    fn call(&mut self, name: &str, args: &[Expr], sp: Span) {
        // Round 49: the first argument of the thread primitive is handed to
        // a stack of its own that keeps running when this frame is gone.
        if name == THREAD_START {
            if let Some(a) = args.first() {
                let srcs = self.value(a);
                self.flow(&srcs, Sink::Thread, &Route::Thread, sp);
            }
            return;
        }
        if is_internal(name) {
            return;
        }
        let flows = match self.summary(name) {
            Some(f) => f.clone(),
            // Unknown name (the type check reports it), or an `extern fn`
            // whose body is not here: gap E3.
            None => return,
        };
        for (i, fl) in flows.iter().enumerate() {
            let a = match args.get(i) {
                Some(a) => a,
                None => break,
            };
            let srcs = self.value(a);
            if srcs.is_empty() {
                continue;
            }
            if fl.contains(&Sink::Foreign) || fl.contains(&Sink::Thread) {
                let route = Route::Call(readable(name), i + 1);
                self.flow(&srcs, Sink::Foreign, &route, sp);
            }
            for s in fl {
                if let Sink::Out(j) = s {
                    // Argument `i` lands in what argument `j` points at.
                    if let Some(dst) = args.get(*j) {
                        self.store_through(dst, srcs.clone(), sp, name, i + 1);
                    }
                }
            }
        }
    }

    /// The callee writes into `*dst`. `dst` is an ARGUMENT here, so the
    /// place is one indirection further out than in `store`.
    fn store_through(
        &mut self,
        dst: &Expr,
        srcs: Vec<Src>,
        sp: Span,
        callee: &str,
        argno: usize,
    ) {
        // `f(&out, &x)`: what lands in `out` stays in this frame.
        if let ExprKind::Unary(UnOp::AddrOf, place) = &dst.kind {
            if let Some(p) = self.path(place) {
                let mut cur = self.taint_under(&p);
                add_all(&mut cur, srcs);
                self.write_local(&p, cur);
                return;
            }
        }
        let dt = self.value(dst);
        if dt.iter().any(|s| matches!(s, Src::Local { .. })) {
            return;
        }
        let (sink, route) = match dt.iter().find_map(|s| match s {
            Src::Param(i) => Some(*i),
            _ => None,
        }) {
            Some(i) => (
                Sink::Out(i),
                Route::Ptr(self.params.get(i).cloned().unwrap_or_default()),
            ),
            None => (Sink::Foreign, Route::Call(readable(callee), argno)),
        };
        self.flow(&srcs, sink, &route, sp);
    }

    /// `match` stands in the tree as the call `__match#N`; the bodies of its
    /// arms live in the registry of `sema_match.rs`. Without this descent
    /// every state machine would be a blind spot (same reasoning as
    /// `nogc.rs`).
    fn match_cases(&mut self, name: &str) {
        let idx = match name
            .strip_prefix(crate::sema_match::MATCH_PREFIX)
            .and_then(|s| s.parse::<usize>().ok())
        {
            Some(i) => i,
            None => return,
        };
        let mi = match crate::sema_match::match_info(idx) {
            Some(m) => m,
            None => return,
        };
        if self.depth >= MAX_DEPTH {
            return;
        }
        self.depth += 1;
        self.expr(&mi.subject);
        for arm in &mi.arms {
            let before = self.taint.clone();
            self.block(&arm.body);
            merge(&mut self.taint, before);
        }
        self.depth -= 1;
    }
}

// ------------------------------------------------------------------ utility

fn add_all(dst: &mut Vec<Src>, src: Vec<Src>) {
    for s in src {
        if !dst.contains(&s) {
            dst.push(s);
        }
    }
}

/// Union of two markings — what one branch marks counts afterwards.
fn merge(dst: &mut HashMap<String, Vec<Src>>, other: HashMap<String, Vec<Src>>) {
    for (k, v) in other {
        let slot = dst.entry(k).or_default();
        add_all(slot, v);
    }
}

/// Every `let`/`var` of the body, with the place it is declared at. Names
/// declared twice (shadowing in a nested block) keep the FIRST place; the
/// analysis knows one slot per name, which is written down as gap E5.
fn collect_locals(b: &Block, out: &mut HashMap<String, Span>) {
    for s in &b.stmts {
        collect_stmt(s, out);
    }
}

fn collect_stmt(s: &Stmt, out: &mut HashMap<String, Span>) {
    match s {
        Stmt::Let { name, span, .. } => {
            out.entry(name.clone()).or_insert(*span);
        }
        Stmt::If { then, els, .. } => {
            collect_locals(then, out);
            if let Some(e) = els {
                collect_stmt(e, out);
            }
        }
        Stmt::While { body, .. } => collect_locals(body, out),
        Stmt::For { name, name_span, body, .. } => {
            out.entry(name.clone()).or_insert(*name_span);
            collect_locals(body, out);
        }
        Stmt::Block(b) => collect_locals(b, out),
        Stmt::Defer(inner, _, _) => collect_stmt(inner, out),
        _ => {}
    }
}

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Attr, ExprId, Param, TypeExpr};

    fn sp(line: u32, col: u32) -> Span {
        Span { file: 0, line, col, len: 1 }
    }

    struct B {
        next: ExprId,
    }

    impl B {
        fn new() -> B {
            B { next: 0 }
        }
        fn e(&mut self, s: Span, k: ExprKind) -> Expr {
            let id = self.next;
            self.next += 1;
            Expr { id, span: s, kind: k }
        }
        fn ident(&mut self, n: &str, s: Span) -> Expr {
            self.e(s, ExprKind::Ident(n.to_string()))
        }
        fn addr(&mut self, inner: Expr, s: Span) -> Expr {
            self.e(s, ExprKind::Unary(UnOp::AddrOf, Box::new(inner)))
        }
    }

    fn func(name: &str, params: Vec<&str>, stmts: Vec<Stmt>, attrs: Vec<Attr>) -> FnDecl {
        FnDecl {
            name: name.to_string(),
            params: params
                .iter()
                .map(|p| Param {
                    name: p.to_string(),
                    ty: TypeExpr::Named("i64".to_string(), sp(1, 1)),
                    span: sp(1, 10),
                })
                .collect(),
            ret: None,
            body: Block { stmts, span: sp(1, 1), end: sp(9, 1) },
            span: sp(1, 1),
            attrs,
            extern_info: None,
        }
    }

    fn program(funcs: Vec<FnDecl>, n: u32) -> Program {
        Program {
            profile: None,
            imports: Vec::new(),
            exports: Vec::new(),
            funcs,
            structs: Vec::new(),
            consts: Vec::new(),
            comptime_blocks: Vec::new(),
            expr_count: n,
        }
    }

    /// `var x: i64 = 0; return &x` has to strike.
    #[test]
    fn return_of_a_local_address() {
        let mut b = B::new();
        let zero = b.e(sp(2, 18), ExprKind::Int(0));
        let x = b.ident("x", sp(3, 13));
        let a = b.addr(x, sp(3, 12));
        let f = func(
            "bad",
            Vec::new(),
            vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: true,
                    ty: None,
                    init: zero,
                    span: sp(2, 5),
                },
                Stmt::Return { value: Some(a), span: sp(3, 5) },
            ],
            Vec::new(),
        );
        let prog = program(vec![f], b.next);
        let tys = vec![Type::I64; b.next as usize];
        let out = collect_findings(&prog, &tys);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(
            out[0].1,
            "the address of the local 'x' escapes through the return value"
        );
        assert!(out[0].2.contains("declared at 2:5"), "{}", out[0].2);
        assert!(out[0].2.contains("dies at 9:1"), "{}", out[0].2);
    }

    /// `#[allow_escape]` switches it off, and stays visible while doing it.
    #[test]
    fn the_way_out_works() {
        let mut b = B::new();
        let zero = b.e(sp(2, 18), ExprKind::Int(0));
        let x = b.ident("x", sp(3, 13));
        let a = b.addr(x, sp(3, 12));
        let f = func(
            "bad",
            Vec::new(),
            vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: true,
                    ty: None,
                    init: zero,
                    span: sp(2, 5),
                },
                Stmt::Return { value: Some(a), span: sp(3, 5) },
            ],
            vec![Attr {
                name: "allow_escape".to_string(),
                args: Vec::new(),
                span: sp(1, 1),
            }],
        );
        let prog = program(vec![f], b.next);
        let tys = vec![Type::I64; b.next as usize];
        assert!(collect_findings(&prog, &tys).is_empty());
    }

    /// The address of a PARAMETER is not the address of this frame — it may
    /// be returned. That is the counter-check against over-strictness.
    #[test]
    fn a_pointer_parameter_may_be_handed_through() {
        let mut b = B::new();
        let p = b.ident("p", sp(2, 12));
        let f = func(
            "pass",
            vec!["p"],
            vec![Stmt::Return { value: Some(p), span: sp(2, 5) }],
            Vec::new(),
        );
        let prog = program(vec![f], b.next);
        let tys = vec![Type::ptr(Type::I64, true); b.next as usize];
        assert!(collect_findings(&prog, &tys).is_empty());
    }

    /// Two functions: `keep` stores its argument in the heap, `caller`
    /// hands it the address of a local. The error belongs at the CALL.
    #[test]
    fn a_capture_is_seen_across_the_call() {
        let mut b = B::new();
        // fn keep(p) { *(0 as *mut i64) = p }  -- a foreign target
        let pv = b.ident("p", sp(2, 20));
        let zero = b.e(sp(2, 6), ExprKind::Int(0));
        let cast = b.e(
            sp(2, 6),
            ExprKind::Cast(
                Box::new(zero),
                TypeExpr::Named("i64".to_string(), sp(2, 6)),
            ),
        );
        let target = b.e(sp(2, 5), ExprKind::Unary(UnOp::Deref, Box::new(cast)));
        let keep = func(
            "keep",
            vec!["p"],
            vec![Stmt::Assign { target, value: pv, span: sp(2, 5) }],
            Vec::new(),
        );
        // fn caller() { var x: i64 = 0; keep(&x) }
        let zero2 = b.e(sp(6, 18), ExprKind::Int(0));
        let x = b.ident("x", sp(7, 11));
        let a = b.addr(x, sp(7, 10));
        let call = b.e(
            sp(7, 5),
            ExprKind::Call("keep".to_string(), vec![a], sp(7, 5)),
        );
        let caller = func(
            "caller",
            Vec::new(),
            vec![
                Stmt::Let {
                    name: "x".to_string(),
                    mutable: true,
                    ty: None,
                    init: zero2,
                    span: sp(6, 5),
                },
                Stmt::Expr(call),
            ],
            Vec::new(),
        );
        let prog = program(vec![keep, caller], b.next);
        let tys = vec![Type::ptr(Type::I64, true); b.next as usize];
        let out = collect_findings(&prog, &tys);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(
            out[0].1,
            "the address of the local 'x' escapes into 'keep', which keeps its argument 1"
        );
    }
}
