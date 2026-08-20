//! Lowering AST -> FIR.
//!
//! INTERFACE (fixed):
//!   `pub fn lower(prog: &ast::Program, info: &sema::TypeInfo, dg: &mut Diags)
//!        -> Option<fir::Module>`
//! Promise to the backend: every block has a real terminator
//! (no `Term::Unset`), all `alloca` stand at the entry block.
//!
//! Variable model (see docs/FIR.md): NO phi nodes. Every local variable and
//! every parameter gets one `alloca` slot; accesses are `load`/`store`.
//! Aggregates (structs, arrays) are never FIR values but always addresses
//! only; copies run through `copymem`.

use std::collections::HashMap;

use crate::ast::{self, Expr, ExprKind, Program, Stmt};
use crate::diag::{Diags, Span};
use crate::fir::{
    BinOp as FBin, BlockId, CmpOp, FTy, Func, Inst, Module, Op, Term, UnOp as FUn, Val,
};
use crate::abi::{self, ArgClass};
use crate::dwarf;
use crate::sema::TypeInfo;
use crate::types::Type;

/// Upper bound for nesting (protection against a recursion explosion).
const MAX_DEPTH: u32 = 200;

/// Scalar FIR type for a source type; `None` for aggregates and for
/// unresolved types, which after the type check should really be impossible.
fn scalar_fty(t: &Type) -> Option<FTy> {
    Some(match t {
        Type::F64 => FTy::F64,
        Type::I8 => FTy::I8,
        Type::I16 => FTy::I16,
        Type::I32 => FTy::I32,
        Type::I64 => FTy::I64,
        Type::Isize => FTy::I64,
        Type::U8 => FTy::U8,
        Type::U16 => FTy::U16,
        Type::U32 => FTy::U32,
        Type::U64 => FTy::U64,
        Type::Usize => FTy::U64,
        Type::Bool => FTy::Bool,
        Type::Ptr { .. } => FTy::Ptr,
        // Round 58: a function value is the pointer to its function record.
        Type::Fn { .. } => FTy::Ptr,
        Type::Void => FTy::Void,
        Type::Array(..) | Type::Struct(_) | Type::UntypedInt | Type::Error => return None,
    })
}

fn is_agg(t: &Type) -> bool {
    matches!(t, Type::Array(..) | Type::Struct(_))
}

pub(crate) struct Local {
    pub(crate) slot: Val,
    /// Round 58: the source type of the local. `lower_call` reads it to
    /// tell a CALL OF A FUNCTION VALUE apart from a direct call.
    pub(crate) ty: Type,
}

pub(crate) struct Lower<'a> {
    pub(crate) info: &'a TypeInfo,
    pub(crate) dg: &'a mut Diags,
    pub(crate) f: Func,
    pub(crate) cur: BlockId,
    pub(crate) scopes: Vec<HashMap<String, Local>>,
    pub(crate) depth: u32,
    /// Name of the function (key of the line table in `dwarf.rs`).
    pub(crate) fname: String,
    /// Targets of `break` / `continue` per loop (outermost first), plus the
    /// depth of the `defer` stack when the loop got entered: a `break` runs
    /// exactly those deferred statements that got declared INSIDE the
    /// loop.
    pub(crate) loops: Vec<(BlockId, BlockId, usize)>,
    /// Deferred statements per block level, in the order of their declaration;
    /// they are executed backwards (SPEC §5.1). The `bool` is `true` for
    /// `errdefer`: the statement then runs ONLY on the error path.
    pub(crate) defers: Vec<Vec<(Stmt, bool)>>,
    /// Hidden return pointer (`sret`), if the function yields an aggregate
    /// over 8 bytes (see `abi.rs`).
    pub(crate) sret: Option<Val>,
    /// Source line assigned to the next instruction produced.
    pub(crate) pending_line: Option<(u32, u32)>,
}

impl<'a> Lower<'a> {
    pub(crate) fn err<T>(&mut self, span: Span, msg: impl Into<String>) -> Option<T> {
        self.dg.error(span, msg);
        None
    }

    /// Internal error: reachable only when the type check violates its promise.
    /// Reported as a normal diagnostic, never as a panic.
    pub(crate) fn ice<T>(&mut self, span: Span, what: &str) -> Option<T> {
        self.dg.error(
            span,
            format!("internal error while lowering to FIR: {}", what),
        );
        None
    }

    // ---- helpers ------------------------------------------------------

    pub(crate) fn ty_of(&self, e: &Expr) -> Type {
        self.info.expr_ty(e.id).clone()
    }

    pub(crate) fn fty_of(&mut self, e: &Expr) -> Option<FTy> {
        let t = self.ty_of(e);
        match scalar_fty(&t) {
            Some(f) => Some(f),
            None => self.ice(e.span, "expression has no scalar type"),
        }
    }

    pub(crate) fn size_align(&self, t: &Type) -> (u64, u64) {
        (
            self.info.tcx.size_of(t).max(1),
            self.info.tcx.align_of(t).max(1),
        )
    }

    pub(crate) fn push(&mut self, ty: FTy, op: Op) -> Val {
        self.note_here();
        self.f.push(self.cur, ty, op)
    }

    pub(crate) fn push_void(&mut self, ty: FTy, op: Op) {
        self.note_here();
        self.f.push_void(self.cur, ty, op)
    }

    /// Assigns the source line of the running statement to the next
    /// instruction (for `.debug_line`, see `dwarf.rs`).
    fn note_here(&mut self) {
        if let Some((file, line)) = self.pending_line.take() {
            let idx = self.f.blocks[self.cur as usize].insts.len() as u32;
            dwarf::note(&self.fname, self.cur, idx, file, line);
        }
    }

    /// `alloca` in the entry block. The insertion place shifts the notes of the
    /// line table, which is why every alloca runs through this wrapper.
    pub(crate) fn alloca(&mut self, size: u64, align: u64) -> Val {
        let at = self.f.blocks[0]
            .insts
            .iter()
            .take_while(|i| matches!(i.op, Op::Alloca { .. }))
            .count() as u32;
        let v = self.f.alloca(size, align);
        dwarf::shift_after_insert(&self.fname, 0, at);
        v
    }

    /// Loads an aggregate as `n` 8-byte words (System V INTEGER class).
    /// If the size is no multiple of 8, reading happens through a padded
    /// scratch buffer — otherwise the last `load` would reach partly beyond
    /// the object.
    fn load_words(&mut self, addr: Val, size: u64, n: usize) -> Option<Vec<Val>> {
        let src = if size % 8 != 0 {
            let t = self.alloca(n as u64 * 8, 8);
            self.push_void(FTy::Void, Op::CopyMem { dst: t, src: addr, size });
            t
        } else {
            addr
        };
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let a = self.ptradd_const(src, i as u64 * 8); // ABI-Wortkopie
            out.push(self.load(FTy::I64, a));
        }
        Some(out)
    }

    /// Counterpart to `load_words`: writes the words to `dst`.
    fn store_words(&mut self, dst: Val, size: u64, words: &[Val]) -> Option<()> {
        if size % 8 != 0 {
            let t = self.alloca(words.len() as u64 * 8, 8);
            for (i, w) in words.iter().enumerate() {
                let a = self.ptradd_const(t, i as u64 * 8); // ABI-Wortkopie
                self.store(FTy::I64, a, *w);
            }
            self.push_void(FTy::Void, Op::CopyMem { dst, src: t, size });
        } else {
            for (i, w) in words.iter().enumerate() {
                let a = self.ptradd_const(dst, i as u64 * 8); // ABI-Wortkopie
                self.store(FTy::I64, a, *w);
            }
        }
        Some(())
    }

    pub(crate) fn constant(&mut self, ty: FTy, v: i128) -> Val {
        self.push(ty, Op::Const(v))
    }

    pub(crate) fn load(&mut self, ty: FTy, addr: Val) -> Val {
        self.push(ty, Op::Load { addr })
    }

    pub(crate) fn store(&mut self, ty: FTy, addr: Val, val: Val) {
        self.push_void(ty, Op::Store { addr, val })
    }

    /// `base + off` bytes; a constant 0 is left out.
    /// Raw address arithmetic `base + off`.
    ///
    /// **Do not use for field accesses** — `layout.rs` exists for that
    /// (`field_addr`, `field_addr_at`, `elem_addr`, `elem_addr_const`).
    /// Direct calls are allowed for ABI word copies only and are marked with
    /// `// ABI-Wortkopie`; `tools/schichten/run.sh` checks that.
    pub(crate) fn ptradd_const(&mut self, base: Val, off: u64) -> Val {
        if off == 0 {
            return base;
        }
        let o = self.constant(FTy::I64, off as i128);
        self.push(FTy::Ptr, Op::PtrAdd { base, off: o })
    }

    pub(crate) fn new_block(&mut self) -> BlockId {
        self.f.add_block()
    }

    pub(crate) fn set_term(&mut self, t: Term) {
        let b = self.cur;
        self.f.set_term(b, t);
    }

    pub(crate) fn terminated(&self) -> bool {
        self.f.is_terminated(self.cur)
    }

    pub(crate) fn enter(&mut self) {
        self.scopes.push(HashMap::new());
    }
    pub(crate) fn leave(&mut self) {
        self.scopes.pop();
    }

    pub(crate) fn declare(&mut self, name: &str, slot: Val) {
        self.declare_ty(name, slot, Type::Error);
    }

    /// Round 58: declaration WITH the source type (see `Local::ty`).
    pub(crate) fn declare_ty(&mut self, name: &str, slot: Val, ty: Type) {
        if let Some(s) = self.scopes.last_mut() {
            s.insert(name.to_string(), Local { slot, ty });
        }
    }

    /// Round 58: the type of a local, if there is one under this name.
    pub(crate) fn local_ty(&self, name: &str) -> Option<Type> {
        for s in self.scopes.iter().rev() {
            if let Some(l) = s.get(name) {
                return Some(l.ty.clone());
            }
        }
        None
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<Val> {
        for s in self.scopes.iter().rev() {
            if let Some(l) = s.get(name) {
                return Some(l.slot);
            }
        }
        None
    }

    // ---- expressions: address (lvalue / aggregate) ---------------------

    pub(crate) fn lower_addr(&mut self, e: &Expr) -> Option<Val> {
        if self.depth > MAX_DEPTH {
            return self.err(e.span, "expression nested too deeply");
        }
        self.depth += 1;
        let r = self.lower_addr_inner(e);
        self.depth -= 1;
        r
    }

    fn lower_addr_inner(&mut self, e: &Expr) -> Option<Val> {
        // HOOK fehlerunionen: `try`/`catch`/error value as aggregate (lower_errors.rs)
        if let Some(r) = crate::lower_errors::hook_addr(self, e) {
            return r;
        }
        match &e.kind {
            ExprKind::Ident(name) => match self.lookup(name) {
                Some(slot) => Some(slot),
                None => {
                    if self.info.consts.contains_key(name) {
                        self.err(e.span, "a constant has no address")
                    } else {
                        self.ice(e.span, "unknown name in lowering")
                    }
                }
            },
            ExprKind::Unary(ast::UnOp::Deref, inner) => self.lower_expr(inner),
            ExprKind::Field(base, fname, fspan) => {
                let bt = self.ty_of(base);
                let (sidx, baddr) = match &bt {
                    Type::Struct(i) => (*i, self.lower_addr(base)?),
                    // `p.f` on a pointer to struct: dereference automatically
                    Type::Ptr { inner, .. } => match **inner {
                        Type::Struct(i) => (i, self.lower_expr(base)?),
                        _ => return self.ice(*fspan, "field access on a non-struct"),
                    },
                    _ => return self.ice(*fspan, "field access on a non-struct"),
                };
                // Layer field access <-> storage location (layout.rs, DESIGN_GOALS 8)
                self.field_addr(baddr, sidx, fname, *fspan)
            }
            ExprKind::Index(base, idx) => {
                let bt = self.ty_of(base);
                let (elem, baddr) = match &bt {
                    Type::Array(el, _) => ((**el).clone(), self.lower_addr(base)?),
                    Type::Ptr { inner, .. } => ((**inner).clone(), self.lower_expr(base)?),
                    _ => return self.ice(e.span, "index on a non-indexable type"),
                };
                let esz = self.info.tcx.size_of(&elem).max(1);
                let iv = self.lower_expr(idx)?;
                let ift = self.fty_of(idx)?;
                // Layer field access <-> storage location (layout.rs, DESIGN_GOALS 8)
                Some(self.elem_addr(baddr, esz, iv, ift))
            }
            ExprKind::StructLit(..) | ExprKind::ArrayLit(_) | ExprKind::ArrayRepeat(..) => {
                let t = self.ty_of(e);
                let (size, align) = self.size_align(&t);
                let slot = self.alloca(size, align);
                self.write_into(slot, e)?;
                Some(slot)
            }
            // HOOK iface: `((&x) as dyn I).m()` — the interface value gets
            // a scratch place whose address is here (iface.rs)
            ExprKind::Cast(..) if crate::iface::is_dyn(&self.info.tcx, &self.ty_of(e)) => {
                let t = self.ty_of(e);
                let (size, align) = self.size_align(&t);
                let slot = self.alloca(size, align);
                self.write_into(slot, e)?;
                Some(slot)
            }
            // HOOK types: `Enum::Variant(..)` yields one aggregate (lower_match.rs)
            ExprKind::Call(name, args, _) if crate::lower_match::is_ctor(name) => {
                crate::lower_match::lower_ctor_addr(self, e, name, args)
            }
            // A call that yields an aggregate writes into a scratch slot;
            // its address is the result (see abi.rs).
            ExprKind::Call(name, args, span) => {
                let t = self.ty_of(e);
                if !is_agg(&t) {
                    return self.err(e.span, "this expression has no address");
                }
                let (size, align) = self.size_align(&t);
                let slot = self.alloca(size, align);
                let name = name.clone();
                let args = args.clone();
                let span = *span;
                self.lower_call(&name, &args, Some(slot), span)?;
                Some(slot)
            }
            _ => self.err(e.span, "this expression has no address"),
        }
    }

    /// Writes the value of `e` to the address `addr` (scalar: `store`,
    /// literal: field or element wise, other aggregate: `copymem`).
    pub(crate) fn write_into(&mut self, addr: Val, e: &Expr) -> Option<()> {
        if self.depth > MAX_DEPTH {
            return self.err(e.span, "expression nested too deeply");
        }
        self.depth += 1;
        let r = self.write_into_inner(addr, e);
        self.depth -= 1;
        r
    }

    fn write_into_inner(&mut self, addr: Val, e: &Expr) -> Option<()> {
        // HOOK fehlerunionen: implicit conversion / `try` / `catch` (lower_errors.rs)
        if let Some(r) = crate::lower_errors::hook_write_into(self, addr, e) {
            return r;
        }
        let t = self.ty_of(e);
        match &e.kind {
            ExprKind::StructLit(_, fields, span) => {
                let sidx = match t {
                    Type::Struct(i) => i,
                    _ => return self.ice(*span, "struct literal without struct type"),
                };
                for (fname, fexpr, fspan) in fields {
                    // Layer field access <-> storage location (layout.rs, DESIGN_GOALS 8)
                    let fa = self.field_addr(addr, sidx, fname, *fspan)?;
                    self.write_into(fa, fexpr)?;
                }
                Some(())
            }
            ExprKind::ArrayLit(elems) => {
                let et = match &t {
                    Type::Array(el, _) => (**el).clone(),
                    _ => return self.ice(e.span, "array literal without array type"),
                };
                let esz = self.info.tcx.size_of(&et).max(1);
                for (i, el) in elems.iter().enumerate() {
                    let ea = self.elem_addr_const(addr, esz, i as u64);
                    self.write_into(ea, el)?;
                }
                Some(())
            }
            ExprKind::ArrayRepeat(val, _) => {
                let (et, n) = match &t {
                    Type::Array(el, n) => ((**el).clone(), *n),
                    _ => return self.ice(e.span, "repetition literal without array type"),
                };
                self.lower_repeat(addr, val, &et, n)
            }
            // HOOK iface: `p as dyn I` — data pointer and method table
            // (iface.rs, round 46)
            ExprKind::Cast(inner, _) if crate::iface::is_dyn(&self.info.tcx, &t) => {
                let inner = (**inner).clone();
                crate::iface::lower_cast_into(self, addr, &inner, &t, e.span)
            }
            // HOOK types: `Enum::Variant(..)` writes straight into the target (lower_match.rs)
            ExprKind::Call(name, args, _) if crate::lower_match::is_ctor(name) => {
                crate::lower_match::write_ctor_into(self, e, name, args, addr)
            }
            // `x = f()` with an aggregate result writes directly into the target.
            ExprKind::Call(name, args, span) if is_agg(&t) => {
                let name = name.clone();
                let args = args.clone();
                let span = *span;
                self.lower_call(&name, &args, Some(addr), span)?;
                Some(())
            }
            _ if is_agg(&t) => {
                let src = self.lower_addr(e)?;
                let size = self.info.tcx.size_of(&t);
                self.push_void(FTy::Void, Op::CopyMem { dst: addr, src, size });
                Some(())
            }
            _ => {
                let ft = self.fty_of(e)?;
                let v = self.lower_expr(e)?;
                self.store(ft, addr, v);
                Some(())
            }
        }
    }

    // ---- expressions: value -------------------------------------------

    pub(crate) fn lower_expr(&mut self, e: &Expr) -> Option<Val> {
        if self.depth > MAX_DEPTH {
            return self.err(e.span, "expression nested too deeply");
        }
        self.depth += 1;
        let r = self.lower_expr_inner(e);
        self.depth -= 1;
        r
    }

    fn lower_expr_inner(&mut self, e: &Expr) -> Option<Val> {
        // HOOK fehlerunionen: scalar result of `try`/`catch` (lower_errors.rs)
        if let Some(r) = crate::lower_errors::hook_value(self, e) {
            return r;
        }
        let t = self.ty_of(e);
        if is_agg(&t) {
            return self.ice(e.span, "aggregate as value (only addresses allowed)");
        }
        match &e.kind {
            ExprKind::Int(v) => {
                let ft = self.fty_of(e)?;
                Some(self.constant(ft, *v))
            }
            // The BIT PATTERN travels into FIR as a constant — there are no float
            // literals there, only bit patterns (fir::FTy::F64).
            ExprKind::Float(bits) => Some(self.constant(FTy::F64, *bits as i128)),
            ExprKind::Bool(b) => Some(self.constant(FTy::Bool, if *b { 1 } else { 0 })),
            ExprKind::Ident(name) => {
                if let Some(slot) = self.lookup(name) {
                    let ft = self.fty_of(e)?;
                    Some(self.load(ft, slot))
                } else if self.info.fns.contains_key(name)
                    && self.ty_of(e).is_fn()
                {
                    // ROUND 58 (fnval.rs): a named function AS A VALUE. The
                    // value is the address of its function record, one word
                    // in `.rodata` holding the code address.
                    let key = crate::fnval::record_of(name);
                    Some(self.push(FTy::Ptr, Op::FnRef { name: key }))
                } else if let Some((ct, cv)) = self.info.consts.get(name).cloned() {
                    let ft = match scalar_fty(&ct) {
                        Some(f) => f,
                        None => return self.ice(e.span, "constant with a non-scalar type"),
                    };
                    Some(self.constant(ft, cv))
                } else {
                    self.ice(e.span, "unknown name in lowering")
                }
            }
            ExprKind::Unary(op, inner) => self.lower_unary(e, *op, inner),
            ExprKind::Binary(op, a, b) => self.lower_binary(e, *op, a, b),
            ExprKind::Field(..) | ExprKind::Index(..) => {
                let addr = self.lower_addr(e)?;
                let ft = self.fty_of(e)?;
                Some(self.load(ft, addr))
            }
            // HOOK types: enum values are aggregates, no call (lower_match.rs)
            ExprKind::Call(name, _, span) if crate::lower_match::is_types_call(name) => {
                self.err(*span, "an enum value is an aggregate and not a scalar value")
            }
            ExprKind::Call(name, args, span) => {
                let ft = self.fty_of(e)?;
                if ft == FTy::Void {
                    return self.err(
                        *span,
                        "call without return value cannot be used as a value",
                    );
                }
                match self.lower_call(name, args, None, *span)? {
                    Some(v) => Some(v),
                    None => self.ice(*span, "call without value in a value position"),
                }
            }
            ExprKind::Syscall(args) => {
                let a = self.lower_syscall_args(args)?;
                Some(self.push(FTy::I64, Op::Syscall { args: a }))
            }
            ExprKind::Cast(inner, _) => {
                let from = self.fty_of(inner)?;
                let to = self.fty_of(e)?;
                let src = self.lower_expr(inner)?;
                if from == to {
                    return Some(src);
                }
                if to == FTy::Bool {
                    // `x as bool` is defined as `x != 0` (0/1, never 2).
                    let z = self.constant(from, 0);
                    return Some(self.push(
                        FTy::Bool,
                        Op::Cmp { op: CmpOp::Ne, ty: from, a: src, b: z },
                    ));
                }
                Some(self.push(to, Op::Cast { src, from }))
            }
            ExprKind::StructLit(..) | ExprKind::ArrayLit(_) | ExprKind::ArrayRepeat(..) => {
                self.ice(e.span, "literal of an aggregate as a value")
            }
        }
    }

    fn lower_unary(&mut self, e: &Expr, op: ast::UnOp, inner: &Expr) -> Option<Val> {
        match op {
            ast::UnOp::AddrOf => self.lower_addr(inner),
            ast::UnOp::Deref => {
                let addr = self.lower_expr(inner)?;
                let ft = self.fty_of(e)?;
                Some(self.load(ft, addr))
            }
            ast::UnOp::Neg => {
                let ft = self.fty_of(e)?;
                let v = self.lower_expr(inner)?;
                Some(self.push(ft, Op::Un(FUn::Neg, v)))
            }
            ast::UnOp::Not => {
                let ft = self.fty_of(e)?;
                let v = self.lower_expr(inner)?;
                Some(self.push(ft, Op::Un(FUn::Not, v)))
            }
        }
    }

    fn lower_binary(&mut self, e: &Expr, op: ast::BinOp, a: &Expr, b: &Expr) -> Option<Val> {
        use ast::BinOp as B;
        // HOOK fehlerunionen: comparison of two error values (lower_errors.rs)
        if let Some(r) = crate::lower_errors::hook_binary(self, op, a, b) {
            return r;
        }
        if op.is_logic() {
            return self.lower_short_circuit(op, a, b);
        }
        if op.is_cmp() {
            let ty = self.fty_of(a)?;
            let av = self.lower_expr(a)?;
            let bv = self.lower_expr(b)?;
            let c = match op {
                B::Eq => CmpOp::Eq,
                B::Ne => CmpOp::Ne,
                B::Lt => CmpOp::Lt,
                B::Le => CmpOp::Le,
                B::Gt => CmpOp::Gt,
                _ => CmpOp::Ge,
            };
            return Some(self.push(FTy::Bool, Op::Cmp { op: c, ty, a: av, b: bv }));
        }
        let ft = self.fty_of(e)?;
        let bop = match op {
            B::Add => FBin::Add,
            B::Sub => FBin::Sub,
            B::Mul => FBin::Mul,
            B::Div => FBin::Div,
            B::Rem => FBin::Rem,
            B::And => FBin::And,
            B::Or => FBin::Or,
            B::Xor => FBin::Xor,
            B::Shl => FBin::Shl,
            B::Shr => FBin::Shr,
            _ => return self.ice(e.span, "unknown binary operator"),
        };
        let av = self.lower_expr(a)?;
        let mut bv = self.lower_expr(b)?;
        if matches!(op, B::Shl | B::Shr) {
            // The shift amount is brought to the width of the left operand;
            // the kind of shift follows `ft`.
            let bt = self.fty_of(b)?;
            if bt != ft {
                bv = self.push(ft, Op::Cast { src: bv, from: bt });
            }
        }
        Some(self.push(ft, Op::Bin(bop, av, bv)))
    }

    /// `&&` / `||` short circuiting: result slot + branch, no arithmetic
    /// substitute operation.
    fn lower_short_circuit(&mut self, op: ast::BinOp, a: &Expr, b: &Expr) -> Option<Val> {
        let slot = self.alloca(1, 1);
        let av = self.lower_expr(a)?;
        self.store(FTy::Bool, slot, av);
        let rhs_bb = self.new_block();
        let join_bb = self.new_block();
        let is_and = matches!(op, ast::BinOp::LAnd);
        let (then_bb, else_bb) = if is_and { (rhs_bb, join_bb) } else { (join_bb, rhs_bb) };
        self.set_term(Term::BrCond { cond: av, then_bb, else_bb });

        self.cur = rhs_bb;
        let bv = self.lower_expr(b)?;
        self.store(FTy::Bool, slot, bv);
        if !self.terminated() {
            self.set_term(Term::Br(join_bb));
        }

        self.cur = join_bb;
        Some(self.load(FTy::Bool, slot))
    }

    /// A call following the calling convention of `abi.rs`.
    ///
    /// `dest` is the target address when the function yields one aggregate.
    /// Return: `Some(Some(v))` = scalar value, `Some(None)` = no value or
    /// result sits at `dest`, `None` = error (reported already).
    pub(crate) fn lower_call(
        &mut self,
        name: &str,
        args: &[Expr],
        dest: Option<Val>,
        span: Span,
    ) -> Option<Option<Val>> {
        // HOOK constant-time: select/barrier/secure_zero (ct.rs, SPEC §9.2/§9.3)
        // HOOK faden: the three thread primitives (thread.rs, round 49)
        if crate::thread::is_thread_call(name) && !self.info.fns.contains_key(name) {
            return crate::thread::lower_thread_call(self, name, args, span);
        }
        // HOOK atomar: the atomic primitive (atomic.rs, round 47)
        if crate::atomic::is_atomic_call(name) && !self.info.fns.contains_key(name) {
            return crate::atomic::lower_atomic_call(self, name, args, span);
        }
        if crate::ct::is_ct_call(name) && !self.info.fns.contains_key(name) {
            return crate::ct::lower_ct_call(self, name, args, span);
        }
        // HOOK sizeof: `size_of[T]()` is a constant — at run time nothing of
        // it is left (sizeof.rs)
        if let Some(g) = crate::sizeof::value(name) {
            return Some(Some(self.constant(FTy::U64, g)));
        }
        // HOOK kern: inline assembler and MMIO (core.rs, round 52)
        if let Some(r) = crate::core::lower_hook(self, name, args, span) {
            return r;
        }
        // HOOK gc: allocation `gc C{…}`, collector intrinsics, `x.as?[C]`
        // (gc_lower.rs, SPEC 3.5)
        if let Some(r) = crate::gc_lower::hook_call(self, name, args, dest, span) {
            return r;
        }
        // HOOK gc: `weak`/`strong` are runtime functions (gc_lower.rs)
        let name: &str = match crate::gc_lower::real_name(name) {
            Some(n) if !self.info.fns.contains_key(name) => n,
            _ => name,
        };
        // HOOK impl: `x.m(a)` appears as `"method m"` in the tree. The
        // resolution is derived AFRESH here — from the type of the receiver
        // and the method name, exactly as in `sema` (impls.rs, round 45). If
        // the method demands a pointer and the receiver is present as a
        // value, its ADDRESS is passed.
        let resolved;
        let mut receiver_address = false;
        // HOOK iface: `f.m(a)` on a `dyn I` — the dynamic dispatch
        // (iface.rs, round 46). `dispatch` carries the CALL TARGET (from the
        // method table) and the data pointer; everything else — aggregates,
        // hidden return pointer, stack arguments — then runs through
        // exactly the same code as any ordinary call.
        let mut dispatch: Option<(Val, Val)> = None;
        let mut dyn_sig: Option<crate::sema::FnSig> = None;
        if let Some(m) = crate::impls::method_name(name) {
            let et = match args.first() {
                Some(e) => self.ty_of(e),
                None => return self.ice(span, "method call without receiver"),
            };
            if let Some(iname) = crate::impls::dyn_interface(&self.info.tcx, &et) {
                let recv = match args.first() {
                    Some(e) => e,
                    None => return self.ice(span, "method call without receiver"),
                };
                let (target, data, sig) =
                    crate::iface::lower_dispatch(self, &iname, m, recv, span)?;
                dispatch = Some((target, data));
                dyn_sig = Some(sig);
            }
        }
        let name: &str = match crate::impls::method_name(name) {
            None => name,
            Some(_) if dispatch.is_some() => name,
            Some(m) => {
                let et = match args.first() {
                    Some(e) => self.ty_of(e),
                    None => return self.ice(span, "method call without receiver"),
                };
                match crate::impls::target(&self.info, m, &et) {
                    Some((full, addr)) => {
                        resolved = full;
                        receiver_address = addr;
                        &resolved
                    }
                    None => return self.ice(span, "unknown method in lowering"),
                }
            }
        };
        // ROUND 58 (fnval.rs): the callee sits in a VARIABLE of function
        // type. Word 0 of the function record is the code address; the
        // record itself goes in as the LAST argument, so that a closure body
        // finds its captured values there (a named function never reads it).
        let mut indirect: Option<(Val, Val)> = None;
        let sig = match dyn_sig {
            Some(s) => s,
            None => match self.local_ty(name) {
                Some(Type::Fn { params, ret }) => {
                    let slot = match self.lookup(name) {
                        Some(s) => s,
                        None => return self.ice(span, "function value without a slot"),
                    };
                    let rec = self.load(FTy::Ptr, slot);
                    let code = self.load(FTy::Ptr, rec);
                    indirect = Some((code, rec));
                    crate::sema::FnSig { params, ret: *ret }
                }
                _ => match self.info.fns.get(name) {
                    Some(s) => s.clone(),
                    None => return self.ice(span, "unknown function in lowering"),
                },
            },
        };
        let ret_agg = is_agg(&sig.ret);
        let sret = abi::ret_needs_sret(&sig.ret, &self.info.tcx);
        let mut vals: Vec<Val> = Vec::new();
        // target address for aggregate returns
        let target = if ret_agg {
            match dest {
                Some(d) => Some(d),
                None => {
                    let (size, align) = self.size_align(&sig.ret);
                    Some(self.alloca(size, align))
                }
            }
        } else {
            None
        };
        if sret {
            if let Some(t) = target {
                vals.push(t);
            }
        }
        for (i, a) in args.iter().enumerate() {
            // HOOK iface: the receiver is the data pointer from the fat
            // pointer — it was read above already (iface.rs)
            if i == 0 {
                if let Some((_, data)) = dispatch {
                    vals.push(data);
                    continue;
                }
            }
            // HOOK impl: the receiver goes in as an address (impls.rs)
            if i == 0 && receiver_address {
                let addr = self.lower_addr(a)?;
                vals.push(addr);
                continue;
            }
            // HOOK fehlerunionen: implicit conversion of one argument (lower_errors.rs)
            let t = match crate::lower_errors::hook_arg_type(a) {
                Some(t) => t,
                None => self.ty_of(a),
            };
            if !is_agg(&t) {
                vals.push(self.lower_expr(a)?);
                continue;
            }
            let (size, align) = self.size_align(&t);
            match abi::classify(&t, &self.info.tcx) {
                ArgClass::Integer(n) => {
                    let addr = self.lower_addr(a)?;
                    let mut ws = self.load_words(addr, size, n as usize)?;
                    vals.append(&mut ws);
                }
                // MEMORY: hidden pointer to a copy of the caller
                _ => {
                    let tmp = self.alloca(size, align);
                    self.write_into(tmp, a)?;
                    vals.push(tmp);
                }
            }
        }
        let op = match (dispatch, indirect) {
            (Some((target, _)), _) => Op::CallIndirect { target: target, args: vals },
            (None, Some((code, rec))) => {
                vals.push(rec);
                Op::CallIndirect { target: code, args: vals }
            }
            (None, None) => Op::Call { name: name.to_string(), args: vals },
        };
        if ret_agg {
            let d = match target {
                Some(d) => d,
                None => return self.ice(span, "aggregate return without target"),
            };
            let size = self.info.tcx.size_of(&sig.ret);
            if sret {
                let _ = self.push(FTy::Ptr, op);
            } else {
                let w = self.push(FTy::I64, op);
                self.store_words(d, size, &[w])?;
            }
            return Some(None);
        }
        match scalar_fty(&sig.ret) {
            Some(FTy::Void) | None => {
                self.push_void(FTy::Void, op);
                Some(None)
            }
            Some(ft) => Some(Some(self.push(ft, op))),
        }
    }

    /// Writes `[value; N]` to the address `addr`. The value is evaluated
    /// EXACTLY ONCE and then multiplied.
    fn lower_repeat(&mut self, addr: Val, val: &Expr, et: &Type, n: u64) -> Option<()> {
        let esz = self.info.tcx.size_of(et).max(1);
        let scalar = !is_agg(et);
        // evaluate the value once
        let (sv, saddr) = if scalar {
            let ft = match scalar_fty(et) {
                Some(f) => f,
                None => return self.ice(val.span, "element without scalar type"),
            };
            (Some((ft, self.lower_expr(val)?)), None)
        } else {
            (None, Some(self.lower_addr(val)?))
        };
        // small lengths without a loop
        if n <= 8 {
            for i in 0..n {
                let ea = self.elem_addr_const(addr, esz, i);
                match (sv, saddr) {
                    (Some((ft, v)), _) => self.store(ft, ea, v),
                    (None, Some(src)) => {
                        self.push_void(FTy::Void, Op::CopyMem { dst: ea, src, size: esz })
                    }
                    _ => return self.ice(val.span, "repetition literal without value"),
                }
            }
            return Some(());
        }
        // big lengths as a loop: i = 0; while i < n { .. ; i = i + 1 }
        let islot = self.alloca(8, 8);
        let zero = self.constant(FTy::U64, 0);
        self.store(FTy::U64, islot, zero);
        let head = self.new_block();
        let body = self.new_block();
        let end = self.new_block();
        self.set_term(Term::Br(head));

        self.cur = head;
        let iv = self.load(FTy::U64, islot);
        let nv = self.constant(FTy::U64, n as i128);
        let c = self.push(FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::U64, a: iv, b: nv });
        self.set_term(Term::BrCond { cond: c, then_bb: body, else_bb: end });

        self.cur = body;
        let iv2 = self.load(FTy::U64, islot);
        // Layer field access <-> storage location (layout.rs, DESIGN_GOALS 8)
        let ea = self.elem_addr(addr, esz, iv2, FTy::U64);
        match (sv, saddr) {
            (Some((ft, v)), _) => self.store(ft, ea, v),
            (None, Some(src)) => self.push_void(FTy::Void, Op::CopyMem { dst: ea, src, size: esz }),
            _ => return self.ice(val.span, "repetition literal without value"),
        }
        let one = self.constant(FTy::U64, 1);
        let inc = self.push(FTy::U64, Op::Bin(FBin::Add, iv2, one));
        self.store(FTy::U64, islot, inc);
        self.set_term(Term::Br(head));

        self.cur = end;
        Some(())
    }

    /// Widen all syscall arguments to `i64` (signed: sign extended, otherwise
    /// zero extended).
    fn lower_syscall_args(&mut self, args: &[Expr]) -> Option<Vec<Val>> {
        if args.is_empty() {
            return None;
        }
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            let ft = self.fty_of(a)?;
            let v = self.lower_expr(a)?;
            if ft == FTy::I64 {
                out.push(v);
            } else {
                out.push(self.push(FTy::I64, Op::Cast { src: v, from: ft }));
            }
        }
        Some(out)
    }

    // ---- statements ----------------------------------------------------

    pub(crate) fn lower_block(&mut self, b: &ast::Block) -> Option<()> {
        if self.depth > MAX_DEPTH {
            return self.err(b.span, "block nested too deeply");
        }
        self.depth += 1;
        self.enter();
        self.defers.push(Vec::new());
        let mut r = Some(());
        for s in &b.stmts {
            if r.is_none() {
                break;
            }
            r = self.lower_stmt(s);
        }
        // Deferred statements of this level, backwards. They run BEFORE
        // `leave()`, so that the names of the block are still visible.
        //
        // If the block was left through `return`/`break`/`continue`, they
        // have already been executed there; whatever is still produced here
        // lands in the unreachable block behind the jump and falls victim to
        // the code cleanup. So nothing runs twice.
        let list = self.defers.pop().unwrap_or_default();
        for (d, only_error) in list.iter().rev() {
            // `errdefer` does NOT run when leaving the ordinary way.
            if *only_error {
                continue;
            }
            if self.lower_stmt(d).is_none() {
                r = None;
            }
        }
        self.leave();
        self.depth -= 1;
        r
    }

    /// Return WITH cleanup: executes all deferred statements of the function
    /// and then sets the terminator.
    ///
    /// The return value has been computed by that point — that is deliberate
    /// and matches Zig and C++: a `defer` sees the finished value and can no
    /// longer replace it.
    pub(crate) fn ret_term(&mut self, v: Option<Val>) {
        self.lower_defers_to(0, false);
        self.set_term(Term::Ret(v));
    }

    /// Return on the ERROR PATH: the `errdefer` statements run here as well
    /// (SPEC §5.1).
    pub(crate) fn ret_term_error(&mut self, v: Option<Val>) {
        self.lower_defers_to(0, true);
        self.set_term(Term::Ret(v));
    }

    /// Executes the deferred statements of all levels above `depth` —
    /// innermost level first, backwards within one level.
    ///
    /// The stack stays UNCHANGED in the process: `lower_block` clears its own
    /// level. If cleanup happens once more further down after a `return`,
    /// that happens in an unreachable block.
    pub(crate) fn lower_defers_to(&mut self, depth: usize, with_error: bool) -> Option<()> {
        let mut r = Some(());
        let mut i = self.defers.len();
        while i > depth {
            i -= 1;
            let list = self.defers[i].clone();
            for (d, only_error) in list.iter().rev() {
                if *only_error && !with_error {
                    continue;
                }
                if self.lower_stmt(d).is_none() {
                    r = None;
                }
            }
        }
        r
    }

    /// Is there any active `errdefer` in this function at all?
    pub(crate) fn has_errdefer(&self) -> bool {
        self.defers.iter().any(|l| l.iter().any(|(_, only_error)| *only_error))
    }

    fn lower_stmt(&mut self, s: &Stmt) -> Option<()> {
        let sp = s.span();
        if !sp.is_none() {
            self.pending_line = Some((sp.file, sp.line));
        }
        match s {
            Stmt::Error(_) => Some(()),
            Stmt::Let { name, init, span, .. } => {
                // HOOK fehlerunionen: implicit conversion at 'let' (lower_errors.rs)
                if let Some(r) = crate::lower_errors::hook_let(self, name, init) {
                    return r;
                }
                let t = self.ty_of(init);
                if matches!(t, Type::Void) {
                    return self.err(*span, "a variable cannot have a value without a type");
                }
                let (size, align) = self.size_align(&t);
                let slot = self.alloca(size, align);
                self.write_into(slot, init)?;
                self.declare_ty(name, slot, t.clone());
                Some(())
            }
            Stmt::Assign { target, value, .. } => {
                let addr = self.lower_addr(target)?;
                self.write_into(addr, value)?;
                // HOOK gc: insertion barrier when writing a Gc pointer into
                // the heap (gc_lower.rs, SPEC 3.5.3)
                crate::gc_lower::hook_assign(self, target)
            }
            Stmt::Expr(e) => self.lower_expr_stmt(e),
            Stmt::Block(b) => self.lower_block(b),
            Stmt::Return { value, span } => {
                match value {
                    Some(v) => {
                        let t = self.ty_of(v);
                        // HONEST LIMIT (SPEC §14.1 F5): if a FINISHED error
                        // union is returned — neither `return E::Variant`
                        // nor a success value that is converted first —
                        // then only at run time is it settled whether this
                        // is the error path. Stage 0 does not decide that;
                        // instead of silently skipping `errdefer`, the case
                        // is rejected. With a conversion `t` is the SOURCE
                        // type and this condition does not apply.
                        if self.has_errdefer() && crate::errors::union_of(&t).is_some() {
                            return self.err(
                                *span,
                                "'errdefer' and passing on a finished error union do not go together in stage 0: here it is only known at run time whether the error path is taken — write 'return try …' or return the error with 'return E::Variant'",
                            );
                        }
                        // HOOK fehlerunionen: implicit conversion (lower_errors.rs)
                        if let Some(r) = crate::lower_errors::hook_return(self, v) {
                            r?;
                        } else if is_agg(&t) {
                            // Aggregate return per abi.rs: through the
                            // hidden pointer or as one word in rax.
                            let size = self.info.tcx.size_of(&t);
                            match self.sret {
                                Some(dst) => {
                                    self.write_into(dst, v)?;
                                    self.ret_term(Some(dst));
                                }
                                None => {
                                    let addr = self.lower_addr(v)?;
                                    let w = self.load_words(addr, size, 1)?;
                                    match w.first() {
                                        Some(w0) => self.ret_term(Some(*w0)),
                                        None => return self.ice(*span, "return without word"),
                                    }
                                }
                            }
                        } else {
                            let rv = self.lower_expr(v)?;
                            self.ret_term(Some(rv));
                        }
                    }
                    None => self.ret_term(None),
                }
                // Everything after it is unreachable: open a new block, so that
                // the invariant "one terminator per block" holds.
                let dead = self.new_block();
                self.cur = dead;
                Some(())
            }
            // Just note it down — it is executed when the block is left.
            Stmt::Defer(inner, only_error, span) => {
                match self.defers.last_mut() {
                    Some(list) => {
                        list.push(((**inner).clone(), *only_error));
                        Some(())
                    }
                    None => self.ice(*span, "'defer' outside a block"),
                }
            }
            Stmt::If { cond, then, els, .. } => self.lower_if(cond, then, els.as_deref()),
            Stmt::While { cond, body, .. } => self.lower_while(cond, body),
            Stmt::For { name, start, end, body, .. } => {
                self.lower_for(name, start, end, body)
            }
            Stmt::Break(span) => {
                let (target, depth) = match self.loops.last() {
                    Some((brk, _, t)) => (*brk, *t),
                    None => return self.ice(*span, "'break' outside a loop"),
                };
                // Clean up first, then jump.
                self.lower_defers_to(depth, false);
                self.set_term(Term::Br(target));
                let dead = self.new_block();
                self.cur = dead;
                Some(())
            }
            Stmt::Continue(span) => {
                let (target, depth) = match self.loops.last() {
                    Some((_, cont, t)) => (*cont, *t),
                    None => return self.ice(*span, "'continue' outside a loop"),
                };
                self.lower_defers_to(depth, false);
                self.set_term(Term::Br(target));
                let dead = self.new_block();
                self.cur = dead;
                Some(())
            }
        }
    }

    fn lower_expr_stmt(&mut self, e: &Expr) -> Option<()> {
        // HOOK fehlerunionen: `try`/`catch`/error value as statement (lower_errors.rs)
        if let Some(r) = crate::lower_errors::hook_stmt(self, e) {
            return r;
        }
        match &e.kind {
            // HOOK types: `match` and enum constructors (lower_match.rs)
            ExprKind::Call(name, args, _) if crate::lower_match::is_types_call(name) => {
                crate::lower_match::lower_types_stmt(self, e, name, args)
            }
            ExprKind::Call(name, args, span) => {
                let t = self.ty_of(e);
                let dest = if is_agg(&t) {
                    let (size, align) = self.size_align(&t);
                    Some(self.alloca(size, align))
                } else {
                    None
                };
                self.lower_call(name, args, dest, *span)?;
                Some(())
            }
            ExprKind::Syscall(args) => {
                let a = self.lower_syscall_args(args)?;
                let _ = self.push(FTy::I64, Op::Syscall { args: a });
                Some(())
            }
            _ => {
                self.lower_expr(e)?;
                Some(())
            }
        }
    }

    fn lower_if(&mut self, cond: &Expr, then: &ast::Block, els: Option<&Stmt>) -> Option<()> {
        let c = self.lower_expr(cond)?;
        let then_bb = self.new_block();
        let else_bb = self.new_block();
        let join_bb = self.new_block();
        self.set_term(Term::BrCond { cond: c, then_bb, else_bb });

        self.cur = then_bb;
        self.lower_block(then)?;
        if !self.terminated() {
            self.set_term(Term::Br(join_bb));
        }

        self.cur = else_bb;
        if let Some(e) = els {
            self.lower_stmt(e)?;
        }
        if !self.terminated() {
            self.set_term(Term::Br(join_bb));
        }

        self.cur = join_bb;
        Some(())
    }

    fn lower_while(&mut self, cond: &Expr, body: &ast::Block) -> Option<()> {
        let head_bb = self.new_block();
        let body_bb = self.new_block();
        let end_bb = self.new_block();
        self.set_term(Term::Br(head_bb));

        self.cur = head_bb;
        let c = self.lower_expr(cond)?;
        self.set_term(Term::BrCond { cond: c, then_bb: body_bb, else_bb: end_bb });

        self.cur = body_bb;
        self.loops.push((end_bb, head_bb, self.defers.len()));
        let r = self.lower_block(body);
        self.loops.pop();
        r?;
        if !self.terminated() {
            self.set_term(Term::Br(head_bb));
        }

        self.cur = end_bb;
        Some(())
    }

    /// The `for` loop over a range — desugared to a counting loop with its own
    /// step block, so that `continue` raises the counter.
    fn lower_for(
        &mut self,
        name: &str,
        start: &Expr,
        end: &Expr,
        body: &ast::Block,
    ) -> Option<()> {
        let ty = self.ty_of(start);
        let ft = match scalar_fty(&ty) {
            Some(f) if f != FTy::Void => f,
            _ => return self.ice(start.span, "range of 'for' without integer type"),
        };
        let bytes = ft.bytes().max(1);
        let islot = self.alloca(bytes, bytes);
        let sv = self.lower_expr(start)?;
        self.store(ft, islot, sv);
        // The upper bound is evaluated ONCE.
        let eslot = self.alloca(bytes, bytes);
        let ev = self.lower_expr(end)?;
        self.store(ft, eslot, ev);

        let head_bb = self.new_block();
        let body_bb = self.new_block();
        let step_bb = self.new_block();
        let end_bb = self.new_block();
        self.set_term(Term::Br(head_bb));

        self.cur = head_bb;
        let iv = self.load(ft, islot);
        let lim = self.load(ft, eslot);
        let c = self.push(FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: ft, a: iv, b: lim });
        self.set_term(Term::BrCond { cond: c, then_bb: body_bb, else_bb: end_bb });

        self.cur = body_bb;
        self.enter();
        self.declare(name, islot);
        self.loops.push((end_bb, step_bb, self.defers.len()));
        let r = self.lower_block(body);
        self.loops.pop();
        self.leave();
        r?;
        if !self.terminated() {
            self.set_term(Term::Br(step_bb));
        }

        self.cur = step_bb;
        let iv2 = self.load(ft, islot);
        let one = self.constant(ft, 1);
        let inc = self.push(ft, Op::Bin(FBin::Add, iv2, one));
        self.store(ft, islot, inc);
        self.set_term(Term::Br(head_bb));

        self.cur = end_bb;
        Some(())
    }

    /// Closes all blocks still open (unreachable), so that no `Term::Unset`
    /// is left over.
    fn finish(&mut self) {
        let ret = self.f.ret;
        for i in 0..self.f.blocks.len() {
            let b = i as BlockId;
            if self.f.is_terminated(b) {
                continue;
            }
            if ret == FTy::Void {
                self.f.set_term(b, Term::Ret(None));
            } else {
                let v = self.f.push(b, ret, Op::Const(0));
                self.f.set_term(b, Term::Ret(Some(v)));
            }
        }
    }
}

/// How a source parameter crosses the function boundary (see `abi.rs`).
enum ParamKind {
    /// scalar: exactly one FIR parameter
    Scalar(FTy),
    /// aggregate as `n` integer words
    Words(usize),
    /// aggregate through memory: a pointer to the copy of the caller
    Ref,
}

fn lower_fn(d: &ast::FnDecl, info: &TypeInfo, dg: &mut Diags) -> Option<Func> {
    let sig = match info.fns.get(&d.name) {
        Some(s) => s.clone(),
        None => {
            dg.error(
                d.span,
                format!("internal error while lowering to FIR: signature of '{}' is missing", d.name),
            );
            return None;
        }
    };

    // --- translate the calling convention of abi.rs into FIR parameters ---
    let sret = abi::ret_needs_sret(&sig.ret, &info.tcx);
    let mut pf: Vec<FTy> = Vec::new();
    if sret {
        pf.push(FTy::Ptr); // hidden return pointer at rdi
    }
    let mut kinds: Vec<ParamKind> = Vec::with_capacity(sig.params.len());
    for (i, p) in sig.params.iter().enumerate() {
        let span = d.params.get(i).map(|p| p.span).unwrap_or(d.span);
        if is_agg(p) {
            match abi::classify(p, &info.tcx) {
                ArgClass::Integer(n) => {
                    for _ in 0..n {
                        pf.push(FTy::I64);
                    }
                    kinds.push(ParamKind::Words(n as usize));
                }
                _ => {
                    pf.push(FTy::Ptr);
                    kinds.push(ParamKind::Ref);
                }
            }
            continue;
        }
        match scalar_fty(p) {
            Some(FTy::Void) | None => {
                dg.error(span, "a parameter of this type is not compilable");
                return None;
            }
            Some(f) => {
                pf.push(f);
                kinds.push(ParamKind::Scalar(f));
            }
        }
    }
    let rf = if is_agg(&sig.ret) {
        // aggregate: either a pointer (sret) or one word in rax
        if sret {
            FTy::Ptr
        } else {
            FTy::I64
        }
    } else {
        match scalar_fty(&sig.ret) {
            Some(f) => f,
            None => {
                dg.error(d.span, "a return type of this kind is not compilable");
                return None;
            }
        }
    };

    let mut f = Func::new(&d.name, pf.clone(), rf);
    // HOOK kern: `#[interrupt]` — its own calling convention in the code
    // generator (core.rs/codegen_x86.rs, round 52).
    f.interrupt = crate::core::has_interrupt(d);
    dwarf::set_fn(&d.name, d.span.file, d.span.line);
    let mut lo = Lower {
        info,
        dg,
        f,
        cur: 0,
        scopes: Vec::new(),
        depth: 0,
        fname: d.name.clone(),
        loops: Vec::new(),
            defers: Vec::new(),
        sret: None,
        pending_line: None,
    };
    let mut next = 0usize;
    if sret {
        lo.sret = Some(lo.f.param_val(0));
        next = 1;
    }
    lo.enter();
    // Parameters get a slot and are saved in the entry block.
    for (i, p) in d.params.iter().enumerate() {
        let ty = match sig.params.get(i) {
            Some(t) => t.clone(),
            None => break,
        };
        match kinds.get(i) {
            Some(ParamKind::Scalar(ft)) => {
                let ft = *ft;
                let slot = lo.alloca(ft.bytes().max(1), ft.bytes().max(1));
                let pv = lo.f.param_val(next);
                next += 1;
                lo.store(ft, slot, pv);
                lo.declare_ty(&p.name, slot, ty.clone());
            }
            Some(ParamKind::Words(n)) => {
                let n = *n;
                let (size, align) = lo.size_align(&ty);
                // The slot is padded to full words, so that the stores of
                // the words lie entirely inside the object.
                let slot = lo.alloca(size.max(n as u64 * 8), align.max(8));
                let ws: Vec<Val> = (0..n)
                    .map(|k| {
                        let v = lo.f.param_val(next + k);
                        v
                    })
                    .collect();
                next += n;
                for (k, w) in ws.iter().enumerate() {
                    let a = lo.ptradd_const(slot, k as u64 * 8); // ABI-Wortkopie
                    lo.store(FTy::I64, a, *w);
                }
                lo.declare(&p.name, slot);
            }
            Some(ParamKind::Ref) => {
                // The caller has created a copy already; its address
                // is the slot of the parameter.
                let pv = lo.f.param_val(next);
                next += 1;
                lo.declare(&p.name, pv);
            }
            None => break,
        }
    }
    let ok = lo.lower_block(&d.body).is_some();
    lo.leave();
    if !ok {
        return None;
    }
    lo.finish();
    Some(lo.f)
}

pub fn lower(prog: &Program, info: &TypeInfo, dg: &mut Diags) -> Option<Module> {
    let mut m = Module::new();
    let mut ok = true;
    for d in &prog.funcs {
        match lower_fn(d, info, dg) {
            Some(f) => m.funcs.push(f),
            None => ok = false,
        }
    }
    if !ok {
        return None;
    }
    // Check the invariant instead of merely claiming it.
    for f in &m.funcs {
        for b in &f.blocks {
            if matches!(b.term, Term::Unset) {
                dg.error(
                    Span::none(),
                    format!(
                        "internal error while lowering to FIR: block bb{} in '{}' without terminator",
                        b.id, f.name
                    ),
                );
                return None;
            }
            if b.id != 0 && b.insts.iter().any(|i: &Inst| matches!(i.op, Op::Alloca { .. })) {
                dg.error(
                    Span::none(),
                    format!(
                        "internal error while lowering to FIR: alloca outside the entry block in '{}'",
                        f.name
                    ),
                );
                return None;
            }
        }
    }
    Some(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::FnSig;
    use crate::types::TypeCtx;

    /// A small AST construction kit, so that the lowering can be checked
    /// independently of parser and type checker.
    struct B {
        next: u32,
        types: Vec<Type>,
    }

    impl B {
        fn new() -> B {
            B { next: 0, types: Vec::new() }
        }
        fn e(&mut self, kind: ExprKind, t: Type) -> Expr {
            let id = self.next;
            self.next += 1;
            self.types.push(t);
            Expr { id, span: Span::new(1, 1, 1), kind }
        }
        fn int(&mut self, v: i128, t: Type) -> Expr {
            self.e(ExprKind::Int(v), t)
        }
        fn id(&mut self, n: &str, t: Type) -> Expr {
            self.e(ExprKind::Ident(n.to_string()), t)
        }
        fn bin(&mut self, op: ast::BinOp, a: Expr, b: Expr, t: Type) -> Expr {
            self.e(ExprKind::Binary(op, Box::new(a), Box::new(b)), t)
        }
    }

    fn blk(stmts: Vec<Stmt>) -> ast::Block {
        ast::Block { stmts, span: Span::new(1, 1, 1) }
    }

    fn info_of(b: &B, tcx: TypeCtx, fns: Vec<(&str, FnSig)>) -> TypeInfo {
        let mut ti = TypeInfo {
            tcx,
            expr_types: b.types.clone(),
            consts: HashMap::new(),
            fns: HashMap::new(),
        };
        for (n, s) in fns {
            ti.fns.insert(n.to_string(), s);
        }
        ti
    }

    fn run(prog: &Program, info: &TypeInfo) -> Module {
        let mut dg = Diags::new("test", "");
        let m = lower(prog, info, &mut dg);
        assert!(!dg.has_errors(), "{}", dg.render());
        assert!(m.is_some(), "lowering yielded no module");
        m.unwrap_or_default()
    }

    #[test]
    fn simple_return() {
        let mut b = B::new();
        let a = b.int(2, Type::I32);
        let c = b.int(3, Type::I32);
        let sum = b.bin(ast::BinOp::Add, a, c, Type::I32);
        let f = ast::FnDecl {
            name: "main".into(),
            params: vec![],
            ret: None,
            body: blk(vec![Stmt::Return { value: Some(sum), span: Span::new(1, 1, 1) }]),
            span: Span::new(1, 1, 1),
            attrs: Vec::new(),
        };
        let prog = Program { funcs: vec![f], expr_count: b.next, ..Default::default() };
        let info = info_of(
            &b,
            TypeCtx::new(),
            vec![("main", FnSig { params: vec![], ret: Type::I32 })],
        );
        let m = run(&prog, &info);
        let t = m.to_text();
        assert!(t.contains("add.i32"), "{}", t);
        assert!(t.contains("ret %"), "{}", t);
        assert!(!t.contains("<unset>"), "{}", t);
    }

    #[test]
    fn short_circuit_generated_branch() {
        let mut b = B::new();
        let x = b.id("p", Type::Bool);
        let y = b.id("q", Type::Bool);
        let and = b.bin(ast::BinOp::LAnd, x, y, Type::Bool);
        let r = b.int(1, Type::I32);
        let f = ast::FnDecl {
            name: "f".into(),
            params: vec![
                ast::Param { name: "p".into(), ty: ast::TypeExpr::Named("bool".into(), Span::none()), span: Span::none() },
                ast::Param { name: "q".into(), ty: ast::TypeExpr::Named("bool".into(), Span::none()), span: Span::none() },
            ],
            ret: None,
            body: blk(vec![
                Stmt::If {
                    cond: and,
                    then: blk(vec![Stmt::Return { value: Some(r), span: Span::none() }]),
                    els: None,
                    span: Span::none(),
                },
            ]),
            span: Span::none(),
            attrs: Vec::new(),
        };
        let prog = Program { funcs: vec![f], expr_count: b.next, ..Default::default() };
        let info = info_of(
            &b,
            TypeCtx::new(),
            vec![(
                "f",
                FnSig { params: vec![Type::Bool, Type::Bool], ret: Type::I32 },
            )],
        );
        let m = run(&prog, &info);
        let t = m.to_text();
        assert!(t.matches("brcond").count() >= 2, "{}", t);
        assert!(!t.contains("<unset>"), "{}", t);
        // The short circuit runs over a bool slot, not over `and`.
        assert!(!t.contains("and.bool"), "{}", t);
    }

    /// Builds the AST of the example program of `docs/FIR.md` (see the source
    /// text there) and yields program + type information.
    fn doc_program() -> (Program, TypeInfo) {
        let mut tcx = TypeCtx::new();
        let pi = tcx.declare("Point");
        tcx.set_fields(pi, vec![("x".into(), Type::I32), ("y".into(), Type::I32)]);
        let pt = Type::Struct(pi);
        let i32t = Type::I32;
        let mut b = B::new();

        // --- fn sum(n: i32) -> i32 ---
        let s_init = b.int(0, i32t.clone());
        let i_init = b.int(1, i32t.clone());
        let ci = b.id("i", i32t.clone());
        let cn = b.id("n", i32t.clone());
        let cond = b.bin(ast::BinOp::Le, ci, cn, Type::Bool);
        let ts = b.id("s", i32t.clone());
        let vs = b.id("s", i32t.clone());
        let vi = b.id("i", i32t.clone());
        let sum = b.bin(ast::BinOp::Add, vs, vi, i32t.clone());
        let ti = b.id("i", i32t.clone());
        let vi2 = b.id("i", i32t.clone());
        let one = b.int(1, i32t.clone());
        let inc = b.bin(ast::BinOp::Add, vi2, one, i32t.clone());
        let rs = b.id("s", i32t.clone());
        let sum_decl = ast::FnDecl {
            name: "sum".into(),
            params: vec![ast::Param {
                name: "n".into(),
                ty: ast::TypeExpr::Named("i32".into(), Span::none()),
                span: Span::none(),
            }],
            ret: Some(ast::TypeExpr::Named("i32".into(), Span::none())),
            body: blk(vec![
                Stmt::Let { name: "s".into(), mutable: true, ty: None, init: s_init, span: Span::none() },
                Stmt::Let { name: "i".into(), mutable: true, ty: None, init: i_init, span: Span::none() },
                Stmt::While {
                    cond,
                    body: blk(vec![
                        Stmt::Assign { target: ts, value: sum, span: Span::none() },
                        Stmt::Assign { target: ti, value: inc, span: Span::none() },
                    ]),
                    span: Span::none(),
                },
                Stmt::Return { value: Some(rs), span: Span::none() },
            ]),
            span: Span::none(),
            attrs: Vec::new(),
        };

        // --- fn main() -> i32 ---
        let fx = b.int(3, i32t.clone());
        let fy = b.int(4, i32t.clone());
        let lit = b.e(
            ExprKind::StructLit(
                "Point".into(),
                vec![("x".into(), fx, Span::none()), ("y".into(), fy, Span::none())],
                Span::none(),
            ),
            pt.clone(),
        );
        let p1 = b.id("p", pt.clone());
        let py = b.e(ExprKind::Field(Box::new(p1), "y".into(), Span::none()), i32t.clone());
        let lim = b.id("LIMIT", i32t.clone());
        let call = b.e(ExprKind::Call("sum".into(), vec![lim], Span::none()), i32t.clone());
        let p2 = b.id("p", pt.clone());
        let px = b.e(ExprKind::Field(Box::new(p2), "x".into(), Span::none()), i32t.clone());
        let z0 = b.int(0, i32t.clone());
        let c1 = b.bin(ast::BinOp::Gt, px, z0, Type::Bool);
        let p3 = b.id("p", pt.clone());
        let py2 = b.e(ExprKind::Field(Box::new(p3), "y".into(), Span::none()), i32t.clone());
        let z1 = b.int(0, i32t.clone());
        let c2 = b.bin(ast::BinOp::Gt, py2, z1, Type::Bool);
        let land = b.bin(ast::BinOp::LAnd, c1, c2, Type::Bool);
        let p4 = b.id("p", pt.clone());
        let px2 = b.e(ExprKind::Field(Box::new(p4), "x".into(), Span::none()), i32t.clone());
        let p5 = b.id("p", pt.clone());
        let py3 = b.e(ExprKind::Field(Box::new(p5), "y".into(), Span::none()), i32t.clone());
        let sum2 = b.bin(ast::BinOp::Add, px2, py3, i32t.clone());
        let zero = b.int(0, i32t.clone());
        let mainf = ast::FnDecl {
            name: "main".into(),
            params: vec![],
            ret: Some(ast::TypeExpr::Named("i32".into(), Span::none())),
            body: blk(vec![
                Stmt::Let { name: "p".into(), mutable: true, ty: None, init: lit, span: Span::none() },
                Stmt::Assign { target: py, value: call, span: Span::none() },
                Stmt::If {
                    cond: land,
                    then: blk(vec![Stmt::Return { value: Some(sum2), span: Span::none() }]),
                    els: None,
                    span: Span::none(),
                },
                Stmt::Return { value: Some(zero), span: Span::none() },
            ]),
            span: Span::none(),
            attrs: Vec::new(),
        };

        let prog = Program { funcs: vec![sum_decl, mainf], expr_count: b.next, ..Default::default() };
        let mut info = info_of(
            &b,
            tcx,
            vec![
                ("sum", FnSig { params: vec![Type::I32], ret: Type::I32 }),
                ("main", FnSig { params: vec![], ret: Type::I32 }),
            ],
        );
        info.consts.insert("LIMIT".into(), (Type::I32, 10));
        (prog, info)
    }

    /// The dump printed at `docs/FIR.md` must match what the lowering really
    /// produces.
    #[test]
    fn doc_example_matches() {
        let (prog, info) = doc_program();
        let m = run(&prog, &info);
        let got = m.to_text();
        if std::env::var(format!("{}_DUMP_DOC", crate::config::compiler_name().to_uppercase())).is_ok() {
            println!("{}", got);
        }
        let doc = include_str!("../../docs/FIR.md");
        let mut want = String::new();
        let mut inside = false;
        for line in doc.lines() {
            if line.starts_with("```") {
                if inside {
                    break;
                }
                if line.contains("firdump") {
                    inside = true;
                }
                continue;
            }
            if inside {
                want.push_str(line);
                want.push('\n');
            }
        }
        assert!(!want.is_empty(), "no ```firdump block found in docs/FIR.md");
        assert_eq!(want, got);
    }

    #[test]
    fn while_and_index() {
        let mut b = B::new();
        let arr_ty = Type::Array(Box::new(Type::I32), 4);
        let lit_elems: Vec<Expr> = (0..4).map(|i| b.int(i, Type::I32)).collect();
        let lit = b.e(ExprKind::ArrayLit(lit_elems), arr_ty.clone());
        let a1 = b.id("a", arr_ty.clone());
        let idx = b.int(2, Type::Usize);
        let ix = b.e(ExprKind::Index(Box::new(a1), Box::new(idx)), Type::I32);
        let f = ast::FnDecl {
            name: "main".into(),
            params: vec![],
            ret: None,
            body: blk(vec![
                Stmt::Let {
                    name: "a".into(),
                    mutable: true,
                    ty: None,
                    init: lit,
                    span: Span::none(),
                },
                Stmt::Return { value: Some(ix), span: Span::none() },
            ]),
            span: Span::none(),
            attrs: Vec::new(),
        };
        let prog = Program { funcs: vec![f], expr_count: b.next, ..Default::default() };
        let info = info_of(
            &b,
            TypeCtx::new(),
            vec![("main", FnSig { params: vec![], ret: Type::I32 })],
        );
        let m = run(&prog, &info);
        let t = m.to_text();
        assert!(t.contains("alloca.ptr size=16 align=4"), "{}", t);
        assert!(t.contains("ptradd.ptr"), "{}", t);
        assert!(t.contains("mul.u64"), "{}", t);
        // all allocas in the entry block
        let f0 = &m.funcs[0];
        for b2 in &f0.blocks[1..] {
            assert!(!b2.insts.iter().any(|i| matches!(i.op, Op::Alloca { .. })));
        }
    }
}
