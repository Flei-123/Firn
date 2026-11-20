// SPDX-License-Identifier: MPL-2.0
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
use crate::fx::HashMap;
use crate::comptime_value::{coerce, element_shape, follow, follow_mut, shape_of, to_int, Shape, Value, MAX_ARRAY_LEN};

/// Upper bound of statements executed per `comptime` evaluation.
const MAX_STEPS: u64 = 2_000_000;
/// Upper bound of nested calls.
const MAX_DEPTH: u32 = 64;

type Error = (Span, String);

struct Slot {
    value: Value,
    shape: Shape,
}

type Env = Vec<HashMap<String, Slot>>;

/// Result of one executed statement.
enum Flow {
    /// carry on with the next statement
    Next,
    /// `return` with value (or 0 for `return` without value)
    Back(Value),
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
    in_const_fn: bool,
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
            in_const_fn: false,
            output: String::new(),
            base: std::path::PathBuf::from("."),
            files: HashMap::default(),
        }
    }

    pub(crate) fn call_on(
        &mut self,
        name: &str,
        args: &[i128],
        span: Span,
        depth: u32,
    ) -> Result<i128, Error> {
        let values: Vec<Value> = args.iter().map(|a| Value::Int(*a)).collect();
        to_int(self.call_value(name, &values, span, depth)?, span)
    }

    pub(crate) fn call_value(
        &mut self,
        name: &str,
        args: &[Value],
        span: Span,
        depth: u32,
    ) -> Result<Value, Error> {
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
        if self.in_const_fn && !f.is_const {
            return Err((
                span,
                format!("comptime: a 'const fn' can only call 'const fn', '{}' is not one", name),
            ));
        }
        let mut env: Env = vec![HashMap::default()];
        for (p, v) in f.params.iter().zip(args.iter()) {
            let shape = shape_of(&p.ty);
            env[0].insert(p.name.clone(), Slot { value: coerce(v.clone(), &shape), shape });
        }
        let saved = std::mem::replace(&mut self.in_const_fn, f.is_const);
        let flow = self.block(&f.body, &mut env, depth);
        self.in_const_fn = saved;
        let ret = f.ret.as_ref().map(shape_of).unwrap_or(Shape::Any);
        match flow? {
            Flow::Back(v) => Ok(coerce(v, &ret)),
            _ => Ok(Value::Int(0)),
        }
    }

    fn block(
        &mut self,
        b: &Block,
        env: &mut Env,
        depth: u32,
    ) -> Result<Flow, Error> {
        env.push(HashMap::default());
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
        env: &mut Env,
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
            Stmt::Let { name, ty, init, .. } => {
                let shape = ty.as_ref().map(shape_of).unwrap_or(Shape::Any);
                let v = coerce(self.value(init, env, depth)?, &shape);
                if let Some(top) = env.last_mut() {
                    top.insert(name.clone(), Slot { value: v, shape });
                }
                Ok(Flow::Next)
            }
            Stmt::AssignOp { target, op, value, span } => {
                let rhs = self.expr(value, env, depth)?;
                self.update(target, env, depth, *span, |old| compute(*op, old, rhs, *span))?;
                Ok(Flow::Next)
            }
            Stmt::Step { target, up, span } => {
                let delta = if *up { 1 } else { -1 };
                self.update(target, env, depth, *span, |old| Ok(old + delta))?;
                Ok(Flow::Next)
            }
            Stmt::Assign { target, value, span } => {
                let v = self.value(value, env, depth)?;
                self.store(target, env, depth, *span, |_| Ok(v))?;
                Ok(Flow::Next)
            }
            Stmt::Expr(e) => {
                self.expr(e, env, depth)?;
                Ok(Flow::Next)
            }
            Stmt::Block(b) => self.block(b, env, depth),
            Stmt::Return { value, .. } => {
                let v = match value {
                    Some(e) => self.value(e, env, depth)?,
                    None => Value::Int(0),
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
            Stmt::For { name, start, end, body, inclusive, .. } => {
                let of = self.expr(start, env, depth)?;
                let to = self.expr(end, env, depth)?;
                let mut i = of;
                while if *inclusive { i <= to } else { i < to } {
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
                    env.push(HashMap::default());
                    if let Some(top) = env.last_mut() {
                        top.insert(name.clone(), Slot { value: Value::Int(i), shape: Shape::Any });
                    }
                    let r = self.block(body, env, depth);
                    env.pop();
                    match r? {
                        Flow::Next | Flow::Resume => {}
                        Flow::Abort => return Ok(Flow::Next),
                        Flow::Back(v) => return Ok(Flow::Back(v)),
                    }
                    // `..=` stops before incrementing past the end value.
                    if *inclusive && i == to {
                        break;
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
            crate::buildcache::taint();
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
        env: &mut Env,
        depth: u32,
    ) -> Result<i128, Error> {
        let no = |msg: &str| Err((e.span, format!("comptime: {}", msg)));
        match &e.kind {
            ExprKind::Int(v) => Ok(*v),
            ExprKind::Bool(b) => Ok(if *b { 1 } else { 0 }),
            ExprKind::Float(..) => no("floating point is not yet possible at compile time"),
            ExprKind::Ident(n) => {
                for level in env.iter().rev() {
                    if let Some(slot) = level.get(n) {
                        return to_int(slot.value.clone(), e.span);
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
            ExprKind::Index(..) => to_int(self.read_index(e, env, depth)?, e.span),
            ExprKind::Cast(inner, te) => {
                let v = self.expr(inner, env, depth)?;
                match shape_of(te) {
                    Shape::Int(t) => return Ok(crate::sema::comptime_wrap(v, &t)),
                    Shape::Bool => return Ok((v != 0) as i128),
                    _ => {}
                }
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
                    values.push(self.value(a, env, depth)?);
                }
                to_int(self.call_value(name, &values, e.span, depth + 1)?, e.span)
            }
            _ => no(
                "only literals, names, operators, conversions, array indexing and calls are allowed here",
            ),
        }
    }

    fn value(&mut self, e: &Expr, env: &mut Env, depth: u32) -> Result<Value, Error> {
        match &e.kind {
            ExprKind::ArrayLit(items) => {
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    out.push(self.value(it, env, depth)?);
                }
                Ok(Value::Arr(out))
            }
            ExprKind::ArrayRepeat(item, count) => {
                let n = self.expr(count, env, depth)?;
                if !(0..=MAX_ARRAY_LEN).contains(&n) {
                    return Err((count.span, format!("comptime: array length {} is not in 0..={}", n, MAX_ARRAY_LEN)));
                }
                let one = self.value(item, env, depth)?;
                Ok(Value::Arr(vec![one; n as usize]))
            }
            ExprKind::Text(_, inner) => self.value(inner, env, depth),
            ExprKind::Ident(n) => {
                for level in env.iter().rev() {
                    if let Some(slot) = level.get(n) {
                        return Ok(slot.value.clone());
                    }
                }
                Ok(Value::Int(self.expr(e, env, depth)?))
            }
            ExprKind::Index(..) => self.read_index(e, env, depth),
            ExprKind::Call(name, args, _) if self.prog.funcs.iter().any(|f| f.name == *name) => {
                let mut values = Vec::with_capacity(args.len());
                for a in args {
                    values.push(self.value(a, env, depth)?);
                }
                self.call_value(name, &values, e.span, depth + 1)
            }
            _ => Ok(Value::Int(self.expr(e, env, depth)?)),
        }
    }

    fn index_path<'e>(
        &mut self,
        e: &'e Expr,
        env: &mut Env,
        depth: u32,
    ) -> Result<(&'e Expr, Vec<i128>), Error> {
        let mut exprs: Vec<&Expr> = Vec::new();
        let mut cur = e;
        while let ExprKind::Index(base, idx) = &cur.kind {
            exprs.push(idx);
            cur = base;
        }
        exprs.reverse();
        let mut path = Vec::with_capacity(exprs.len());
        for ie in exprs {
            path.push(self.expr(ie, env, depth)?);
        }
        Ok((cur, path))
    }

    fn read_index(&mut self, e: &Expr, env: &mut Env, depth: u32) -> Result<Value, Error> {
        let (root, path) = self.index_path(e, env, depth)?;
        if let ExprKind::Ident(n) = &root.kind {
            for level in env.iter().rev() {
                if let Some(slot) = level.get(n) {
                    return follow(&slot.value, &path, e.span).cloned();
                }
            }
        }
        let base = self.value(root, env, depth)?;
        follow(&base, &path, e.span).cloned()
    }

    fn store(
        &mut self,
        target: &Expr,
        env: &mut Env,
        depth: u32,
        span: Span,
        make: impl FnOnce(&Value) -> Result<Value, Error>,
    ) -> Result<(), Error> {
        let (root, path) = self.index_path(target, env, depth)?;
        let ExprKind::Ident(name) = &root.kind else {
            return Err((
                span,
                "comptime: only assignments to a local variable or an element of one (no field, no pointer)".to_string(),
            ));
        };
        for level in env.iter_mut().rev() {
            if let Some(slot) = level.get_mut(name) {
                let shape = element_shape(&slot.shape, path.len());
                let cell = follow_mut(&mut slot.value, &path, span)?;
                let new = make(cell)?;
                *cell = coerce(new, &shape);
                return Ok(());
            }
        }
        Err((span, format!("comptime: '{}' is not a local variable", name)))
    }

    fn update(
        &mut self,
        target: &Expr,
        env: &mut Env,
        depth: u32,
        span: Span,
        change: impl FnOnce(i128) -> Result<i128, Error>,
    ) -> Result<(), Error> {
        self.store(target, env, depth, span, |old| {
            Ok(Value::Int(change(to_int(old.clone(), span)?)?))
        })
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
    let empty_consts: HashMap<String, (Type, i128)> = HashMap::default();
    let empty_types: Vec<Type> = Vec::new();
    let mut total = String::new();
    for (b, _span) in &prog.comptime_blocks {
        let mut run = Execution::new(prog, &empty_consts, &empty_types);
        run.base = base.to_path_buf();
        let mut env: Env = vec![HashMap::default()];
        match run.block(b, &mut env, 0) {
            Ok(_) => total.push_str(&run.output),
            Err((span, msg)) => dg.error(span, msg),
        }
    }
    total
}
