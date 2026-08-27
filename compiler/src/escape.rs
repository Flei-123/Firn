// SPDX-License-Identifier: GPL-2.0-only
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
//! A pointer value carries a `Taint` — where it can have come from:
//!
//! * `Taint::local` — it points into the frame of the function being
//!   checked (a `let`/`var`, an array or a field of one, or a PARAMETER: a
//!   parameter slot lies in the frame too). One is kept, the first found;
//!   it is what the message names.
//! * `Taint::params` — a bit per parameter it is derived from. Nothing is
//!   known about that frame here; the CALLER decides.
//!
//! A taint travels through address arithmetic, casts, aggregate literals and
//! assignments to locals. It does NOT travel through a LOAD out of memory
//! (`*p`, `(*p).f`, `p[i]`): what lies in the heap is data, not the address
//! of the pointer that led there. That distinction is what keeps
//! `vec_push` honest and `(*v).ptr` quiet.
//!
//! A taint reaches a SINK when it
//!
//! * is returned (`SINK_RETURN`),
//! * is written through a pointer that does not belong to this frame — a
//!   pointer parameter is `out_bit(j)`, anything else `SINK_FOREIGN`,
//! * is handed to a thread (`SINK_THREAD`, the primitive `__thread_start`).
//!
//! For a local source a sink is the error. For a parameter it is not an
//! error but a FACT about the function, which is written into its SUMMARY
//! and used at every call site of it. That is how the check crosses
//! function boundaries without a single annotation:
//!
//! * `SINK_RETURN` → the result of a call inherits the taint of that
//!   argument (`fn first(v: *mut V) -> *mut u8` hands its argument through;
//!   that is not a capture and must not be an error).
//! * `out_bit(j)` → the taint of argument `i` lands in what argument `j`
//!   points at (the out-parameter idiom).
//! * `SINK_FOREIGN` → the function KEEPS the pointer somewhere that
//!   outlives it. Handing it the address of a local is the error, reported
//!   at the CALL.
//!
//! The summaries are computed to a fixed point over the whole program
//! before anything is reported, so the fact travels along chains of calls.
//!
//! ## The way out
//!
//! `#[allow_escape]` on a function switches the check off for that
//! function's body, and stays visible in the source while doing it — a
//! hardware address, a stack a thread is handed or the stack pointer the
//! collector needs (SPEC §3.5.3) do sometimes have to leave the frame. It
//! empties the function's SUMMARY too, so a vouched-for function does not
//! send its callers red instead. Nothing is switched off silently and
//! nothing is switched off globally.
//!
//! Wired up through the line `// HOOK escape` in `sema::Checker::run`. The
//! twin in Firn is `lib/firnc1/escape.fi`; both have to reject the same
//! programs with the same text, which `tools/escape/run.sh` checks. That is
//! also why the state here is kept in INSERTION ORDERED lists and not in a
//! `HashMap`: the two compilers have to walk the same marks in the same
//! order, or a program with two escaping locals would be blamed for
//! different ones.

use std::collections::HashMap;

use crate::ast::{BinOp, Block, Expr, ExprKind, FnDecl, Program, Stmt, UnOp};
use crate::diag::{Diag, Span};
use crate::sema::Checker;
use crate::types::Type;

/// The thread primitive of round 49 (`thread.rs`). Its first argument is
/// handed to a NEW stack that outlives this frame.
const THREAD_START: &str = "__thread_start";

/// Upper bound for the fixed point over the call graph. Every round can only
/// ADD sink bits and there are finitely many, so the loop ends by itself;
/// the bound is the second safeguard, exactly like `MAX_DEPTH` in `nogc.rs`.
const MAX_ROUNDS: u32 = 12;

/// Deepest nesting of `match` bodies that is still walked.
const MAX_DEPTH: u32 = 256;

/// Out through the return value.
const SINK_RETURN: u64 = 1;
/// Written through a pointer that belongs to nobody known here.
const SINK_FOREIGN: u64 = 2;
/// Handed to a new thread.
const SINK_THREAD: u64 = 4;

/// Written into what parameter `j` points at. Parameters from 56 on count as
/// `SINK_FOREIGN` — no signature of this language has that many, and the
/// coarser answer is the safe one.
fn out_bit(j: usize) -> u64 {
    if j < 56 {
        1u64 << (8 + j)
    } else {
        SINK_FOREIGN
    }
}

/// The local a pointer points into.
#[derive(Clone, PartialEq, Eq, Debug)]
struct LocalSrc {
    name: String,
    /// where it is declared
    decl: Span,
    /// where its address is taken
    take: Span,
}

/// Where a pointer value can have come from.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
struct Taint {
    /// bit `i` — derived from parameter `i`
    params: u64,
    /// the FIRST local source found; the message names this one.
    local: Option<LocalSrc>,
}

impl Taint {
    fn is_empty(&self) -> bool {
        self.params == 0 && self.local.is_none()
    }
    fn param(i: usize) -> Taint {
        Taint {
            params: if i < 64 { 1u64 << i } else { 0 },
            local: None,
        }
    }
    fn unite(&mut self, other: Taint) {
        self.params |= other.params;
        if self.local.is_none() {
            self.local = other.local;
        }
    }
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
    let mut sums: HashMap<String, Vec<u64>> = HashMap::new();
    for f in &prog.funcs {
        // Declared twice is an error of the type check; the first entry wins
        // here and the run stays deterministic.
        sums.entry(f.name.clone())
            .or_insert_with(|| vec![0u64; f.params.len()]);
    }
    for _round in 0..MAX_ROUNDS {
        let mut fresh: Vec<(String, Vec<u64>)> = Vec::new();
        for f in &prog.funcs {
            // `#[allow_escape]` vouches for the whole function, its callers
            // included: its summary stays EMPTY, so no call of it is blamed
            // for what it does with the pointers it is given. Anything else
            // would make the way out useless — the author would silence the
            // function and the caller would go red instead.
            if f.extern_info.is_some() || is_closure(&f.name) || has_allow_escape(f) {
                continue;
            }
            let mut p = Pass::new(&sums, tys, f, false);
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
        let mut p = Pass::new(&sums, tys, f, true);
        p.run(f);
        out.append(&mut p.out);
    }
    out.sort_by_key(|(s, _, _, _)| (s.file, s.line, s.col));
    out.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    out
}

struct Pass<'a> {
    sums: &'a HashMap<String, Vec<u64>>,
    tys: &'a [Type],
    /// Name of the function being checked, as it stands in the source.
    who: String,
    /// Position of the closing brace of its body — where the frame dies.
    fend: Span,
    /// Parameter names in order; the index is the parameter bit.
    params: Vec<String>,
    /// Every local of the function (parameters included) with the place it
    /// is declared at, in the order they turn up.
    locals: Vec<(String, Span)>,
    /// Which PLACE of this frame currently holds an address and where from.
    /// Insertion ordered — see the file comment.
    taint: Vec<(String, Taint)>,
    /// What the parameters of THIS function do — the summary being built.
    found: Vec<u64>,
    report: bool,
    depth: u32,
    out: Vec<(Span, String, String, String)>,
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
        sums: &'a HashMap<String, Vec<u64>>,
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
            locals: Vec::new(),
            taint: Vec::new(),
            found: vec![0u64; f.params.len()],
            report,
            depth: 0,
            out: Vec::new(),
        }
    }

    fn run(&mut self, f: &FnDecl) {
        for p in &f.params {
            self.declare(&p.name, p.span);
        }
        collect_locals(&f.body, &mut self.locals);
        for (i, p) in f.params.iter().enumerate() {
            self.taint.push((p.name.clone(), Taint::param(i)));
        }
        self.block(&f.body);
    }

    fn declare(&mut self, name: &str, sp: Span) {
        if !self.locals.iter().any(|(n, _)| n == name) {
            self.locals.push((name.to_string(), sp));
        }
    }

    // --------------------------------------------------------------- helpers

    fn is_ptr(&self, e: &Expr) -> bool {
        matches!(self.tys.get(e.id as usize), Some(Type::Ptr { .. }))
    }

    fn decl_of(&self, name: &str) -> Option<Span> {
        self.locals.iter().find(|(n, _)| n == name).map(|(_, s)| *s)
    }

    fn is_local(&self, name: &str) -> bool {
        self.decl_of(name).is_some()
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

    fn is_under(key: &str, path: &str) -> bool {
        key == path
            || (key.len() > path.len()
                && key.starts_with(path)
                && (key.as_bytes()[path.len()] == b'.' || key.as_bytes()[path.len()] == b'['))
    }

    /// Everything marked at this place, BELOW it or ABOVE it.
    ///
    /// Below, because reading the whole struct reads its fields with it.
    /// Above, because a mark at `a` says "somewhere in `a` there is an
    /// address of this frame" and reading `a[0]` may well be the way it
    /// comes back out (`tools/escape/reject/22_array_of_addresses.fi`).
    /// Both directions are the conservative reading; only SIBLINGS stay
    /// apart, and that is the case that keeps `lib/firnc1/parser.fi` and
    /// `lib/js/regexp.fi` buildable.
    fn taint_under(&self, path: &str) -> Taint {
        let mut out = Taint::default();
        for (k, v) in &self.taint {
            if Pass::is_under(k, path) || Pass::is_under(path, k) {
                out.unite(v.clone());
            }
        }
        out
    }

    /// Writing into a place of this frame: it replaces exactly that place and
    /// everything below it.
    fn write_local(&mut self, path: &str, t: Taint) {
        self.taint.retain(|(k, _)| !Pass::is_under(k, path));
        if !t.is_empty() {
            self.taint.push((path.to_string(), t));
        }
    }

    /// Put the value of `e` into the place `path` — FIELD FOR FIELD when it
    /// is an aggregate literal.
    ///
    /// `M { p: &x, flag: true }` marks `m.p` and not `m`. The difference is
    /// not cosmetic: a mark on the whole struct would be inherited by every
    /// field read out of it afterwards, and `lib/js/regexp.fi` builds
    /// exactly such a struct (`konts: &konts[0]`) and reads a DIFFERENT
    /// field out of it (`st.over`) one line later. Marking the whole thing
    /// would reject that correct program.
    fn store_value(&mut self, path: &str, e: &Expr) {
        match &e.kind {
            ExprKind::StructLit(_, fields, _) => {
                self.write_local(path, Taint::default());
                for (n, v, _) in fields {
                    let inner = format!("{path}.{n}");
                    self.store_value(&inner, v);
                }
            }
            // An array is index INsensitive: everything that goes in lands
            // under one `[]`. Nothing else would be honest -- the index is a
            // value at run time.
            ExprKind::ArrayLit(xs) => {
                self.write_local(path, Taint::default());
                let mut t = Taint::default();
                for x in xs {
                    t.unite(self.value(x));
                }
                let inner = format!("{path}[]");
                self.write_local(&inner, t);
            }
            ExprKind::ArrayRepeat(v, _) => {
                self.write_local(path, Taint::default());
                let t = self.value(v);
                let inner = format!("{path}[]");
                self.write_local(&inner, t);
            }
            ExprKind::Text(_, inner) => self.store_value(path, inner),
            _ => {
                let t = self.value(e);
                self.write_local(path, t);
            }
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

    /// The taint of the VALUE of `e`.
    fn value(&self, e: &Expr) -> Taint {
        match &e.kind {
            ExprKind::Ident(n) => self.taint_under(n),
            ExprKind::Unary(UnOp::AddrOf, inner) => self.address_of(inner, e.span),
            // A LOAD out of memory yields data, not the address that led
            // there. Without this line the analysis would call every
            // `(*v).ptr` a pointer into the caller's frame and `vec_push`
            // would never be recognised as storing.
            ExprKind::Unary(UnOp::Deref, _) => Taint::default(),
            ExprKind::Unary(_, a) => self.value(a),
            ExprKind::Cast(a, _) => self.value(a),
            // Pointer arithmetic keeps the source; `&a[0] + 3` still points
            // into `a`.
            ExprKind::Binary(BinOp::Add, a, b) => {
                let mut v = self.value(a);
                v.unite(self.value(b));
                v
            }
            // The DIFFERENCE of two addresses is a distance and no address at
            // all -- `(&p.b) as usize - (&p.a) as usize` is how
            // `lib/std/rc.fi` measures the stride of a type. Only
            // `address - number` stays an address.
            ExprKind::Binary(BinOp::Sub, a, b) => {
                if self.value(b).is_empty() {
                    self.value(a)
                } else {
                    Taint::default()
                }
            }
            // Reading a field/element of a LOCAL aggregate: whatever was put
            // into that very field is what comes out.
            ExprKind::Field(..) | ExprKind::Index(..) => match self.path(e) {
                Some(p) => self.taint_under(&p),
                None => Taint::default(),
            },
            ExprKind::StructLit(_, fields, _) => {
                let mut v = Taint::default();
                for (_, x, _) in fields {
                    v.unite(self.value(x));
                }
                v
            }
            ExprKind::ArrayLit(xs) => {
                let mut v = Taint::default();
                for x in xs {
                    v.unite(self.value(x));
                }
                v
            }
            ExprKind::ArrayRepeat(x, _) => self.value(x),
            ExprKind::Text(_, inner) => self.value(inner),
            ExprKind::Call(name, args, _) => self.call_result(name, args),
            _ => Taint::default(),
        }
    }

    /// The taint of the ADDRESS of the place `e` (`&e`). `at` is where the
    /// `&` stands, for the message.
    fn address_of(&self, e: &Expr, at: Span) -> Taint {
        match &e.kind {
            ExprKind::Ident(n) => match self.decl_of(n) {
                Some(decl) => Taint {
                    params: 0,
                    local: Some(LocalSrc {
                        name: n.clone(),
                        decl,
                        take: at,
                    }),
                },
                // A constant lies in `.rodata` and outlives everything.
                None => Taint::default(),
            },
            // `&(*p).f` and `&p[i]` are `p` plus an offset — the frame of the
            // local `p` has nothing to do with it.
            ExprKind::Unary(UnOp::Deref, p) => self.value(p),
            ExprKind::Field(b, _, _) | ExprKind::Index(b, _) => {
                if self.is_ptr(b) {
                    self.value(b)
                } else {
                    self.address_of(b, at)
                }
            }
            _ => Taint::default(),
        }
    }

    /// The summary of a callee, if there is one.
    fn summary(&self, name: &str) -> Option<&'a Vec<u64>> {
        self.sums.get(name)
    }

    /// What a call yields: the taint of every argument that the callee passes
    /// through to its result.
    fn call_result(&self, name: &str, args: &[Expr]) -> Taint {
        let flows = match self.summary(name) {
            Some(f) => f,
            None => return Taint::default(),
        };
        let mut v = Taint::default();
        for (i, fl) in flows.iter().enumerate() {
            if fl & SINK_RETURN != 0 {
                if let Some(a) = args.get(i) {
                    v.unite(self.value(a));
                }
            }
        }
        v
    }

    // ----------------------------------------------------------------- sinks

    /// A taint reaches a sink. A parameter bit is a fact for the summary, a
    /// local source is the error.
    fn flow(&mut self, t: &Taint, sink: u64, route: &Route, at: Span) {
        if t.params != 0 {
            for i in 0..self.found.len().min(64) {
                if t.params & (1u64 << i) != 0 {
                    self.found[i] |= sink;
                }
            }
        }
        if let Some(l) = &t.local {
            if self.report {
                let (name, decl, take) = (l.name.clone(), l.decl, l.take);
                self.blame(&name, decl, take, route, at);
            }
        }
    }

    fn blame(&mut self, name: &str, decl: Span, take: Span, route: &Route, at: Span) {
        let who = self.who.clone();
        let msg = format!("the address of the local '{name}' escapes {}", route.text());
        let note = format!(
            "'&{name}' at {}:{} points into the frame of '{who}'; '{name}' is declared at \
             {}:{} and dies at {}:{}, where '{who}' returns",
            take.line, take.col, decl.line, decl.col, self.fend.line, self.fend.col
        );
        let help = route.help(&who);
        let at = if at.is_none() { take } else { at };
        // ONE caret, not the width of the statement. `lib/firnc1/escape.fi`
        // has no token lengths in its tree -- it keeps a line and a column
        // per node and nothing more. A marker of the same width would be the
        // one thing the two compilers could not agree on, and it says
        // nothing the position does not: the place is the beginning of the
        // construct that lets the pointer out.
        let at = Span::in_file(at.file, at.line, at.col, 1);
        self.out.push((at, msg, note, help));
    }

    /// The taint `t` is written into the place `target`.
    ///
    /// Three outcomes: it stays in this frame (then the place takes it over),
    /// it goes into what a parameter points at (`out_bit`), or it goes
    /// somewhere unknown (`SINK_FOREIGN`).
    fn store(&mut self, target: &Expr, t: Taint, at: Span) {
        if let Some(p) = self.path(target) {
            self.write_local(&p, t);
            return;
        }
        if t.is_empty() {
            return;
        }
        match self.root(target) {
            // A place of this frame the path did not catch (an index behind a
            // cast and the like): mark the root local, coarsely but safely.
            Root::Local(n) => {
                let n = n.to_string();
                let mut cur = self.taint_under(&n);
                cur.unite(t);
                self.write_local(&n, cur);
            }
            Root::Behind(p) => {
                let pt = self.value(p);
                // Into the frame itself: `var b: Box; let q = &b; (*q).p = &x`
                // writes into `b`, and `b` lives exactly as long as `x`.
                if pt.local.is_some() {
                    return;
                }
                let (sink, route) = self.through(&pt);
                self.flow(&t, sink, &route, at);
            }
            Root::Other => {
                self.flow(&t, SINK_FOREIGN, &Route::ForeignPtr, at);
            }
        }
    }

    /// A write goes through the pointer whose taint is `pt`: into a
    /// parameter's storage, or out of sight.
    fn through(&self, pt: &Taint) -> (u64, Route) {
        for i in 0..self.params.len().min(64) {
            if pt.params & (1u64 << i) != 0 {
                return (out_bit(i), Route::Ptr(self.params[i].clone()));
            }
        }
        (SINK_FOREIGN, Route::ForeignPtr)
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
                let name = name.clone();
                self.store_value(&name, init);
            }
            Stmt::Assign { target, value, span } => {
                self.expr(value);
                self.expr(target);
                if let Some(p) = self.path(target) {
                    self.store_value(&p, value);
                    return;
                }
                let v = self.value(value);
                self.store(target, v, *span);
            }
            Stmt::AssignOp { target, value, span, .. } => {
                self.expr(value);
                self.expr(target);
                // `p += n` keeps whatever `p` already pointed into.
                let mut v = self.value(target);
                v.unite(self.value(value));
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
                self.declare(name, *name_span);
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
                    let t = self.value(v);
                    self.flow(&t, SINK_RETURN, &Route::Return, *span);
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
        // Round 49: the first argument of the thread primitive is handed to a
        // stack of its own that keeps running when this frame is gone.
        if name == THREAD_START {
            if let Some(a) = args.first() {
                let t = self.value(a);
                self.flow(&t, SINK_THREAD, &Route::Thread, sp);
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
            let t = self.value(a);
            if t.is_empty() {
                continue;
            }
            if fl & (SINK_FOREIGN | SINK_THREAD) != 0 {
                let route = Route::Call(readable(name), i + 1);
                self.flow(&t, SINK_FOREIGN, &route, sp);
            }
            for j in 0..args.len().min(56) {
                if fl & out_bit(j) != 0 && j < 56 {
                    // Argument `i` lands in what argument `j` points at.
                    self.store_through(&args[j], t.clone(), sp, name, i + 1);
                }
            }
        }
    }

    /// The callee writes into `*dst`. `dst` is an ARGUMENT here, so the place
    /// is one indirection further out than in `store`.
    fn store_through(&mut self, dst: &Expr, t: Taint, sp: Span, callee: &str, argno: usize) {
        // `f(&out, &x)`: what lands in `out` stays in this frame.
        if let ExprKind::Unary(UnOp::AddrOf, place) = &dst.kind {
            if let Some(p) = self.path(place) {
                let mut cur = self.taint_under(&p);
                cur.unite(t);
                self.write_local(&p, cur);
                return;
            }
        }
        let dt = self.value(dst);
        if dt.local.is_some() {
            return;
        }
        let (sink, route) = match self.through(&dt) {
            (s, Route::ForeignPtr) => (s, Route::Call(readable(callee), argno)),
            other => other,
        };
        self.flow(&t, sink, &route, sp);
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

/// Union of two markings — what one branch marks counts afterwards. `other`
/// is the state that was there BEFORE, so its entries come first and the
/// order stays the same as in the Firn twin.
fn merge(dst: &mut Vec<(String, Taint)>, other: Vec<(String, Taint)>) {
    for (k, v) in other {
        match dst.iter_mut().find(|(n, _)| *n == k) {
            Some(slot) => slot.1.unite(v),
            None => dst.push((k, v)),
        }
    }
}

/// Every `let`/`var` of the body, with the place it is declared at. Names
/// declared twice (shadowing in a nested block) keep the FIRST place; the
/// analysis knows one slot per name, which is written down as gap E5.
fn collect_locals(b: &Block, out: &mut Vec<(String, Span)>) {
    for s in &b.stmts {
        collect_stmt(s, out);
    }
}

fn put(out: &mut Vec<(String, Span)>, name: &str, sp: Span) {
    if !out.iter().any(|(n, _)| n == name) {
        out.push((name.to_string(), sp));
    }
}

fn collect_stmt(s: &Stmt, out: &mut Vec<(String, Span)>) {
    match s {
        Stmt::Let { name, span, .. } => put(out, name, *span),
        Stmt::If { then, els, .. } => {
            collect_locals(then, out);
            if let Some(e) = els {
                collect_stmt(e, out);
            }
        }
        Stmt::While { body, .. } => collect_locals(body, out),
        Stmt::For { name, name_span, body, .. } => {
            put(out, name, *name_span);
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
            statics: Vec::new(),
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

    /// A pointer PARAMETER may be handed through — that is the counter-check
    /// against over-strictness.
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

    /// Two functions: `keep` stores its argument in the heap, `caller` hands
    /// it the address of a local. The error belongs at the CALL.
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
