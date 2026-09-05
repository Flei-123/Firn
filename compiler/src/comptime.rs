// SPDX-License-Identifier: GPL-2.0-only
//! **`comptime`** — evaluation at compile time.
//!
//! INTERFACE (fixed):
//!   `pub(crate) fn call_on(...) -> Result<i128, (Span, String)>`
//!
//! ## What for
//!
//! `FIRN-ANFORDERUNGEN.md` §6 lists compile time code generation as a **must**
//! for the browser: 697 Web IDL files, the HTML entity table, the CSS
//! properties and the Unicode data are generated code. Acceptance point 6
//! demands concretely a Unicode table that comes about at compile time out of
//! the UCD.
//!
//! The first step towards that is the ability to **execute functions of your
//! own at compile time** — with loops, branches and local variables. Exactly
//! that is what this interpreter does. The second step (`emit`, that is
//! generated source text) builds on it and additionally needs the reentrant
//! check phases that stand since the foundation work
//! (`sema::Checker::add_items`).
//!
//! ## What can be evaluated
//!
//! Integers and `bool`. Statements: `let`/`var`, assignment to a local
//! variable, `if`/`else`, `while`, `for`, `break`, `continue`, `return`,
//! blocks, expression statements. Expressions: literals, constants, local
//! names, all operators, conversions and **calls of further functions**
//! (recursive ones too).
//!
//! ## What deliberately does NOT work
//!
//! Pointers, arrays, structs, `syscall`, floating point, GC allocation. All of
//! that would need memory at compile time; that arrives with `emit`. An
//! attempt ends with a message plus source position, not with wrong code.
//!
//! ## Limits that get honoured
//!
//! A `comptime` run must not hang the compiler. Hence: at most `MAX_STEPS`
//! statements executed and `MAX_DEPTH` nested calls. Both end with a clear
//! message.

use crate::ast::{BinOp, Block, Expr, ExprKind, FnDecl, Program, Stmt, UnOp};
use crate::diag::Span;
use crate::types::Type;
use std::collections::HashMap;

/// Upper bound of statements executed per `comptime` evaluation.
const MAX_STEPS: u64 = 2_000_000;
/// Upper bound of nested calls.
const MAX_DEPTH: u32 = 64;

type Error = (Span, String);

/// Result of one executed statement.
enum Flow {
    /// carry on with the next statement
    Next,
    /// `return` with value (or 0 for `return` without value)
    Back(i128),
    Abort,
    Resume,
}

pub(crate) struct Execution<'a> {
    prog: &'a Program,
    /// Program wide constants, the way the type checker knows them.
    consts: &'a HashMap<String, (Type, i128)>,
    /// Type of every expression (for the width at conversions).
    expr_types: &'a [Type],
    steps: u64,
    /// Source text built up by `emit_*`.
    pub(crate) output: String,
    /// Directory of the root source file — the ONLY place `file_*` is
    /// allowed to read from.
    base: std::path::PathBuf,
    /// Files read once; a table is queried byte by byte.
    files: HashMap<String, Vec<u8>>,
}

impl<'a> Execution<'a> {
    pub(crate) fn new(
        prog: &'a Program,
        consts: &'a HashMap<String, (Type, i128)>,
        expr_types: &'a [Type],
    ) -> Execution<'a> {
        Execution {
            prog,
            consts,
            expr_types,
            steps: 0,
            output: String::new(),
            base: std::path::PathBuf::from("."),
            files: HashMap::new(),
        }
    }

    /// Calls the given function with arguments that are already evaluated.
    pub(crate) fn call_on(
        &mut self,
        name: &str,
        args: &[i128],
        span: Span,
        depth: u32,
    ) -> Result<i128, Error> {
        if depth >= MAX_DEPTH {
            return Err((
                span,
                format!("comptime: more than {} nested calls", MAX_DEPTH),
            ));
        }
        let f: &FnDecl = match self.prog.funcs.iter().find(|f| f.name == name) {
            Some(f) => f,
            None => {
                return Err((
                    span,
                    format!("comptime: '{}' is not a function of this program", name),
                ))
            }
        };
        if f.params.len() != args.len() {
            return Err((
                span,
                format!(
                    "comptime: '{}' expects {} arguments, found {}",
                    name,
                    f.params.len(),
                    args.len()
                ),
            ));
        }
        let mut env: Vec<HashMap<String, i128>> = vec![HashMap::new()];
        for (p, v) in f.params.iter().zip(args.iter()) {
            env[0].insert(p.name.clone(), *v);
        }
        match self.block(&f.body, &mut env, depth)? {
            Flow::Back(v) => Ok(v),
            // A function without `return` yields 0 — the type checker made
            // sure beforehand that this happens with `-> void` only.
            _ => Ok(0),
        }
    }

    fn block(
        &mut self,
        b: &Block,
        env: &mut Vec<HashMap<String, i128>>,
        depth: u32,
    ) -> Result<Flow, Error> {
        env.push(HashMap::new());
        let mut r = Flow::Next;
        for s in &b.stmts {
            r = self.stmt(s, env, depth)?;
            if !matches!(r, Flow::Next) {
                break;
            }
        }
        env.pop();
        Ok(r)
    }

    fn stmt(
        &mut self,
        s: &Stmt,
        env: &mut Vec<HashMap<String, i128>>,
        depth: u32,
    ) -> Result<Flow, Error> {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return Err((
                s.span(),
                format!(
                    "comptime: more than {} steps — endless loop?",
                    MAX_STEPS
                ),
            ));
        }
        match s {
            Stmt::Error(_) => Ok(Flow::Next),
            Stmt::Let { name, init, .. } => {
                let v = self.expr(init, env, depth)?;
                if let Some(top) = env.last_mut() {
                    top.insert(name.clone(), v);
                }
                Ok(Flow::Next)
            }
            // ROUND 70: the comptime interpreter knows no compound
            // assignment. It refuses it with a message instead of
            // computing something wrong.
            Stmt::AssignOp { span, .. } | Stmt::Step { span, .. } => Err((
                *span,
                "comptime: '+=' and '++' are not available inside a comptime block".to_string(),
            )),
            Stmt::Assign { target, value, span } => {
                let v = self.expr(value, env, depth)?;
                let name = match &target.kind {
                    ExprKind::Ident(n) => n.clone(),
                    _ => {
                        return Err((
                            *span,
                            "comptime: only assignments to a local variable (no field, no index, no pointer)"
                                .to_string(),
                        ))
                    }
                };
                for level in env.iter_mut().rev() {
                    if let Some(slot) = level.get_mut(&name) {
                        *slot = v;
                        return Ok(Flow::Next);
                    }
                }
                Err((*span, format!("comptime: '{}' is not a local variable", name)))
            }
            Stmt::Expr(e) => {
                self.expr(e, env, depth)?;
                Ok(Flow::Next)
            }
            Stmt::Block(b) => self.block(b, env, depth),
            Stmt::Return { value, .. } => {
                let v = match value {
                    Some(e) => self.expr(e, env, depth)?,
                    None => 0,
                };
                Ok(Flow::Back(v))
            }
            Stmt::If { cond, then, els, .. } => {
                if self.expr(cond, env, depth)? != 0 {
                    self.block(then, env, depth)
                } else {
                    match els {
                        Some(e) => self.stmt(e, env, depth),
                        None => Ok(Flow::Next),
                    }
                }
            }
            Stmt::While { cond, body, .. } => {
                loop {
                    self.steps += 1;
                    if self.steps > MAX_STEPS {
                        return Err((
                            s.span(),
                            format!(
                                "comptime: more than {} steps — endless loop?",
                                MAX_STEPS
                            ),
                        ));
                    }
                    if self.expr(cond, env, depth)? == 0 {
                        return Ok(Flow::Next);
                    }
                    match self.block(body, env, depth)? {
                        Flow::Next | Flow::Resume => {}
                        Flow::Abort => return Ok(Flow::Next),
                        Flow::Back(v) => return Ok(Flow::Back(v)),
                    }
                }
            }
            Stmt::For { name, start, end, body, .. } => {
                let of = self.expr(start, env, depth)?;
                let to = self.expr(end, env, depth)?;
                let mut i = of;
                while i < to {
                    self.steps += 1;
                    if self.steps > MAX_STEPS {
                        return Err((
                            s.span(),
                            format!(
                                "comptime: more than {} steps — endless loop?",
                                MAX_STEPS
                            ),
                        ));
                    }
                    env.push(HashMap::new());
                    if let Some(top) = env.last_mut() {
                        top.insert(name.clone(), i);
                    }
                    let r = self.block(body, env, depth);
                    env.pop();
                    match r? {
                        Flow::Next | Flow::Resume => {}
                        Flow::Abort => return Ok(Flow::Next),
                        Flow::Back(v) => return Ok(Flow::Back(v)),
                    }
                    i += 1;
                }
                Ok(Flow::Next)
            }
            Stmt::Break(_) => Ok(Flow::Abort),
            Stmt::Continue(_) => Ok(Flow::Resume),
            // Deferred statements would have no effect in a pure computation;
            // they are rejected rather than silently passed over.
            Stmt::Defer(_, _, span) => Err((
                *span,
                "comptime: 'defer' and 'errdefer' are not allowed at compile time"
                    .to_string(),
            )),
        }
    }

    /// Reads a data file — ONCE, from the cache after that.
    ///
    /// SECURITY (DESIGN_GOALS §3): compile time file access is a gateway for
    /// supply chain attacks — a library dragged along could otherwise read
    /// `/etc/passwd` while building and write it into the generated code.
    /// Hence a hard rule holds here:
    ///
    ///   * RELATIVE to the root source file only,
    ///   * no `..` at any place,
    ///   * no absolute path, no drive or root prefix.
    ///
    /// That is deliberately tighter than needed. Once Firn gets a module
    /// system with capabilities (DESIGN_GOALS §3), it turns into a permission
    /// that a module has to request explicitly.
    fn read_file(&mut self, path: &str, span: Span) -> Result<&Vec<u8>, Error> {
        if !self.files.contains_key(path) {
            if path.is_empty() {
                return Err((span, "comptime: empty file name".to_string()));
            }
            let p = std::path::Path::new(path);
            if p.is_absolute() || path.starts_with('/') || path.starts_with('\\') {
                return Err((
                    span,
                    format!("comptime: '{}' is an absolute path — only paths relative to the source file are allowed", path),
                ));
            }
            if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                return Err((
                    span,
                    format!("comptime: '{}' contains '..' — access stays inside the directory of the source file", path),
                ));
            }
            let full = self.base.join(p);
            let content = std::fs::read(&full).map_err(|e| {
                (
                    span,
                    format!("comptime: '{}' is not readable: {}", full.display(), e),
                )
            })?;
            self.files.insert(path.to_string(), content);
        }
        Ok(&self.files[path])
    }

    fn expr(
        &mut self,
        e: &Expr,
        env: &mut Vec<HashMap<String, i128>>,
        depth: u32,
    ) -> Result<i128, Error> {
        let no = |msg: &str| Err((e.span, format!("comptime: {}", msg)));
        match &e.kind {
            ExprKind::Int(v) => Ok(*v),
            ExprKind::Bool(b) => Ok(if *b { 1 } else { 0 }),
            ExprKind::Float(..) => no("floating point is not yet possible at compile time"),
            ExprKind::Ident(n) => {
                for level in env.iter().rev() {
                    if let Some(v) = level.get(n) {
                        return Ok(*v);
                    }
                }
                match self.consts.get(n) {
                    // ROUND FIRN-LUECKEN: a float constant is a BIT PATTERN in
                    // that table. Compile time execution has no floating point
                    // (see the arm above), so it does not get one through the
                    // back door either.
                    Some((t, _)) if t.is_float() => Err((
                        e.span,
                        format!(
                            "comptime: '{}' is a floating point constant, and floating \
point is not yet possible at compile time",
                            n
                        ),
                    )),
                    Some((_, v)) => Ok(*v),
                    None => Err((e.span, format!("comptime: '{}' is not known here", n))),
                }
            }
            ExprKind::Unary(op, inner) => {
                let v = self.expr(inner, env, depth)?;
                match op {
                    UnOp::Neg => Ok(-v),
                    UnOp::Not => Ok(if v == 0 { 1 } else { 0 }),
                    UnOp::BitNot => Ok(!v),
                    _ => no("pointer operations do not exist at compile time"),
                }
            }
            ExprKind::Binary(op, l, r) => {
                let a = self.expr(l, env, depth)?;
                // Keep the short circuit: `false && f()` does not call `f`.
                if matches!(op, BinOp::LAnd) && a == 0 {
                    return Ok(0);
                }
                if matches!(op, BinOp::LOr) && a != 0 {
                    return Ok(1);
                }
                let b = self.expr(r, env, depth)?;
                compute(*op, a, b, e.span)
            }
            ExprKind::Cast(inner, _) => {
                let v = self.expr(inner, env, depth)?;
                let target = self
                    .expr_types
                    .get(e.id as usize)
                    .cloned()
                    .unwrap_or(Type::I64);
                Ok(crate::sema::comptime_wrap(v, &target))
            }
            ExprKind::Call(name, args, _) => {
                // EMIT: the only side effects that a `comptime` may have —
                // they write into the source text buffer (SPEC §6.4).
                if name == "emit_raw" {
                    let text = literal_text(args, e.span)?;
                    self.output.push_str(&text);
                    return Ok(0);
                }
                // DATA ACCESS AT COMPILE TIME (SPEC §6.4).
                //
                // Exactly for that, acceptance point 6 demands the Unicode
                // table "from the UCD": a data file is read and source
                // text comes about from it. The file is queried byte by
                // byte — that way the interpreter needs neither strings nor
                // arrays.
                if name == "file_size" {
                    let path = literal_text(args, e.span)?;
                    let content = self.read_file(&path, e.span)?;
                    return Ok(content.len() as i128);
                }
                if name == "file_byte" {
                    if args.len() != 2 {
                        return no("'file_byte' expects path and index");
                    }
                    let path = literal_text(&args[..1], e.span)?;
                    let idx = self.expr(&args[1], env, depth)?;
                    let content = self.read_file(&path, e.span)?;
                    if idx < 0 || idx >= content.len() as i128 {
                        return Ok(-1);
                    }
                    return Ok(content[idx as usize] as i128);
                }
                if name == "emit_number" {
                    if args.len() != 1 {
                        return no("'emit_number' expects exactly one argument");
                    }
                    let v = self.expr(&args[0], env, depth)?;
                    self.output.push_str(&v.to_string());
                    return Ok(v);
                }
                let mut values = Vec::with_capacity(args.len());
                for a in args {
                    values.push(self.expr(a, env, depth)?);
                }
                self.call_on(name, &values, e.span, depth + 1)
            }
            _ => no(
                "only literals, names, operators, conversions and calls are allowed here",
            ),
        }
    }
}

fn compute(op: BinOp, a: i128, b: i128, span: Span) -> Result<i128, Error> {
    let bit = |x: bool| if x { 1 } else { 0 };
    Ok(match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => {
            if b == 0 {
                return Err((span, "comptime: division by zero".to_string()));
            }
            a / b
        }
        BinOp::Rem => {
            if b == 0 {
                return Err((span, "comptime: remainder on division by zero".to_string()));
            }
            a % b
        }
        BinOp::And => a & b,
        BinOp::Or => a | b,
        BinOp::Xor => a ^ b,
        BinOp::Shl => {
            if !(0..128).contains(&b) {
                return Err((span, "comptime: shift amount outside 0..127".to_string()));
            }
            a << b
        }
        BinOp::Shr => {
            if !(0..128).contains(&b) {
                return Err((span, "comptime: shift amount outside 0..127".to_string()));
            }
            a >> b
        }
        BinOp::Eq => bit(a == b),
        BinOp::Ne => bit(a != b),
        BinOp::Lt => bit(a < b),
        BinOp::Le => bit(a <= b),
        BinOp::Gt => bit(a > b),
        BinOp::Ge => bit(a >= b),
        BinOp::LAnd => bit(a != 0 && b != 0),
        BinOp::LOr => bit(a != 0 || b != 0),
        // ROUND 72 -- explicit wrap/saturate (SPEC section 13, item L9).
        // `comptime` has no destination type of its own at this point (its
        // values are untyped i128 until a later cast narrows them, exactly
        // like every other operator here), so wrapping is the only one of
        // the two that has an honest answer: it is what the SAME narrowing
        // step every other arithmetic result already goes through already
        // means.
        BinOp::AddWrap => a + b,
        BinOp::SubWrap => a - b,
        BinOp::MulWrap => a * b,
        BinOp::AddSat | BinOp::SubSat | BinOp::MulSat => {
            return Err((
                span,
                "'+|'/'-|'/'*|' (saturating arithmetic) is not supported inside                  'comptime' yet -- use it at run time"
                    .to_string(),
            ));
        }
    })
}

/// The text of a string literal. The parser has already turned `"abc"` into
/// an array literal of octets (SPEC §14.1.str) — here it is read back.
/// That way `emit_raw` needs no string support inside the
/// interpreter.
fn literal_text(args: &[Expr], span: Span) -> Result<String, Error> {
    if args.len() != 1 {
        return Err((span, "comptime: 'emit_raw' expects exactly one argument".to_string()));
    }
    // ROUND 70 (strtype.rs): the text literal is its own node now and
    // carries the array literal of its octets inside.
    let inner = match &args[0].kind {
        ExprKind::Text(_, inner) => inner,
        _ => &args[0],
    };
    let elems = match &inner.kind {
        ExprKind::ArrayLit(v) => v,
        _ => {
            return Err((
                args[0].span,
                "comptime: 'emit_raw' expects a string literal".to_string(),
            ))
        }
    };
    let mut bytes = Vec::with_capacity(elems.len());
    for el in elems {
        match &el.kind {
            ExprKind::Int(v) if (0..256).contains(v) => bytes.push(*v as u8),
            _ => {
                return Err((
                    args[0].span,
                    "comptime: 'emit_raw' expects a string literal".to_string(),
                ))
            }
        }
    }
    String::from_utf8(bytes)
        .map_err(|_| (args[0].span, "comptime: 'emit_raw' needs valid UTF-8".to_string()))
}

/// Executes all `comptime { … }` blocks of the program and yields the source
/// text produced along the way.
///
/// The run happens BEFORE the type check: the blocks may therefore use no
/// program wide constants, but they may call every function of the program.
/// Honestly stated at SPEC §14.1.comptime.
pub(crate) fn run_blocks_out(
    prog: &Program,
    dg: &mut crate::diag::Diags,
    base: &std::path::Path,
) -> String {
    if prog.comptime_blocks.is_empty() {
        return String::new();
    }
    let empty_consts: HashMap<String, (Type, i128)> = HashMap::new();
    let empty_types: Vec<Type> = Vec::new();
    let mut total = String::new();
    for (b, _span) in &prog.comptime_blocks {
        let mut run = Execution::new(prog, &empty_consts, &empty_types);
        run.base = base.to_path_buf();
        let mut env: Vec<HashMap<String, i128>> = vec![HashMap::new()];
        match run.block(b, &mut env, 0) {
            Ok(_) => total.push_str(&run.output),
            Err((span, msg)) => dg.error(span, msg),
        }
    }
    total
}
