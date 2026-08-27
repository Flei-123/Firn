// SPDX-License-Identifier: GPL-2.0-only
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
pub(crate) fn scalar_fty_pub(t: &Type) -> Option<FTy> {
    scalar_fty(t)
}

fn scalar_fty(t: &Type) -> Option<FTy> {
    Some(match t {
        Type::F64 => FTy::F64,
        Type::F32 => FTy::F32,
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
        // ROUND 82 (simd.rs): the vector register is a SCALAR value in FIR —
        // one value, one slot, no fields. That it is sixteen octets wide only
        // the frame layout and the code generator care about.
        Type::V128 => FTy::V128,
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

/// **ROUND 72** -- `ast::BinOp` -> `fir::BinOp` for exactly the five
/// operators `Op::CheckedBin`/`Op::CheckedDiv` ever carry (`+ - * / %`).
/// A separate, narrow function rather than reusing a general-purpose
/// mapping: the panic message building code above must never be handed an
/// operator it was not written for, and this function's return type says
/// nothing else is possible without a caller reading the source.
fn bin_of(op: ast::BinOp) -> FBin {
    match op {
        ast::BinOp::Add => FBin::Add,
        ast::BinOp::Sub => FBin::Sub,
        ast::BinOp::Mul => FBin::Mul,
        ast::BinOp::Div => FBin::Div,
        ast::BinOp::Rem => FBin::Rem,
        _ => unreachable!("bin_of only ever sees Add/Sub/Mul/Div/Rem"),
    }
}

/// **ROUND 71** — the FIR type of ONE ABI eightbyte. `f64` is not the truth
/// about the content (an SSE eightbyte can hold two `f32`), it is the truth
/// about the REGISTER: everything the code generator needs to know is
/// "integer register or xmm register", and it reads exactly that off here.
fn word_fty(w: abi::Word) -> FTy {
    match w {
        abi::Word::Int => FTy::I64,
        abi::Word::Sse => FTy::F64,
    }
}

pub(crate) struct Local {
    pub(crate) slot: Val,
    /// Round 58: the source type of the local. `lower_call` reads it to
    /// tell a CALL OF A FUNCTION VALUE apart from a direct call.
    pub(crate) ty: Type,
}

pub(crate) struct Lower<'a> {
    /// **ROUND 70** - lvalues whose address is already computed
    /// (`lower_addr`). The compound assignment on `str` puts its target in
    /// here so that the address is computed exactly once.
    pub(crate) pinned: HashMap<crate::ast::ExprId, Val>,
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
    /// Round 64: are the parameters through? Everything declared after that
    /// is a local variable and gets `DW_TAG_variable` instead of
    /// `DW_TAG_formal_parameter`.
    pub(crate) params_done: bool,
    /// Round 64: the line of the statement being lowered -- for the
    /// `DW_AT_decl_line` of a declared name.
    pub(crate) decl_line: Option<(u32, u32)>,
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

    /// **ROUND 71** — the type an expression HANDS OUT.
    ///
    /// That is its own type, except where the one implicit conversion
    /// applies: an `f32` in a place that wants an `f64` hands out an `f64`
    /// (`sema::expr`). `ty_of` deliberately keeps giving the OWN type --
    /// the value is built at its own width first and only converted after.
    /// Whoever asks what comes OUT asks here.
    pub(crate) fn out_ty(&self, e: &Expr) -> Type {
        if self.info.widen_f32.contains(&e.id) {
            return Type::F64;
        }
        self.ty_of(e)
    }

    pub(crate) fn out_fty(&mut self, e: &Expr) -> Option<FTy> {
        let t = self.out_ty(e);
        match scalar_fty(&t) {
            Some(f) => Some(f),
            None => self.ice(e.span, "expression has no scalar type"),
        }
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
        self.f.push(self.cur, ty, op)
    }

    pub(crate) fn push_void(&mut self, ty: FTy, op: Op) {
        self.f.push_void(self.cur, ty, op)
    }

    /// ROUND 94 -- the source position every instruction pushed from here on
    /// carries (`fir::Loc`). Set per statement and per expression; the
    /// instruction takes it along wherever the optimizer later moves it.
    pub(crate) fn set_loc(&mut self, sp: Span) {
        if !sp.is_none() {
            self.f.loc_stamp = crate::fir::Loc { file: sp.file, line: sp.line, col: sp.col };
        }
    }

    /// `alloca` in the entry block.
    ///
    /// ROUND 94: an `alloca` is bookkeeping of the frame, not a statement of
    /// the program. It gets NO position -- otherwise the whole prologue would
    /// claim the line of whatever statement happened to need a slot, and a
    /// breakpoint on that line would stop in the prologue.
    pub(crate) fn alloca(&mut self, size: u64, align: u64) -> Val {
        let keep = self.f.loc_stamp;
        self.f.loc_stamp = crate::fir::Loc::NONE;
        let v = self.f.alloca(size, align);
        self.f.loc_stamp = keep;
        v
    }

    /// Loads an aggregate as `n` 8-byte words (System V INTEGER class).
    /// If the size is no multiple of 8, reading happens through a padded
    /// scratch buffer — otherwise the last `load` would reach partly beyond
    /// the object.
    /// **ROUND 71** — `words` carries the CLASS of every eightbyte. A word
    /// of the SSE class is loaded as an `f64`; that is not decoration but
    /// the whole point -- the code generator reads the register class off
    /// the FIR type of the argument and therefore puts a `{ f32, f32 }`
    /// into `xmm0` rather than into `rdi`.
    fn load_words(&mut self, addr: Val, size: u64, words: &[abi::Word]) -> Option<Vec<Val>> {
        let n = words.len();
        let src = if size % 8 != 0 {
            let t = self.alloca(n as u64 * 8, 8);
            self.push_void(FTy::Void, Op::CopyMem { dst: t, src: addr, size });
            t
        } else {
            addr
        };
        let mut out = Vec::with_capacity(n);
        for (i, w) in words.iter().enumerate() {
            let a = self.ptradd_const(src, i as u64 * 8); // ABI-Wortkopie
            out.push(self.load(word_fty(*w), a));
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
        // ROUND 64 -- the hook for the debugger. This is the ONE place where
        // a name out of the source text is bound to storage; everything
        // `gdb` later shows about variables comes from here. The line is the
        // one of the statement being lowered (`pending_line`).
        if dwarf::with_variables() {
            let (file, line) = self.decl_line.unwrap_or((0, 0));
            dwarf::declare_var(
                &self.fname,
                name,
                slot,
                crate::dwarf_info::dtype_of(&self.info.tcx, &ty),
                file,
                line,
                !self.params_done,
            );
        }
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

    /// **ROUND 70** - the address of an lvalue that has already been
    /// computed.
    ///
    /// The compound assignment on `str` passes its target as an ARGUMENT
    /// (`__str_concat(s, x)`) and writes the result back into the same
    /// place. Without this pin the address would be computed a second time
    /// for the argument - and with `a[f()] += "x"` `f()` would run twice.
    pub(crate) fn lower_addr(&mut self, e: &Expr) -> Option<Val> {
        if let Some(v) = self.pinned.get(&e.id) {
            return Some(*v);
        }
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
                // ROUND 89 (statics.rs): a `static` HAS an address, and it
                // is the same one everywhere -- a link time constant, not a
                // stack slot. From here on every path that already worked
                // for a local (field, index, assignment, `&x`) works for a
                // global without knowing the difference.
                None if self.info.statics.contains_key(name) => {
                    Some(self.push(FTy::Ptr, Op::GlobalAddr { name: name.clone() }))
                }
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
                let mut iv = self.lower_expr(idx)?;
                let ift = self.fty_of(idx)?;
                // ROUND 89 -- the checked index (SPEC section 13, item L9).
                // Only a FIXED SIZE ARRAY gets one: `*T` carries no length,
                // so there is nothing to compare against and this compiler
                // does not invent one. The check sits HERE and not in
                // `elem_addr`, so that the array literals and struct copies
                // the compiler generates for itself (whose indices it just
                // computed and knows to be inside) do not pay for it.
                if let Type::Array(_, n) = &bt {
                    if crate::checkmode::is_checked() {
                        let msg = self.index_msg(idx.span, &bt);
                        iv = self.push(ift, Op::CheckedIdx { idx: iv, len: *n, msg });
                    }
                }
                // Layer field access <-> storage location (layout.rs, DESIGN_GOALS 8)
                Some(self.elem_addr(baddr, esz, iv, ift))
            }
            ExprKind::StructLit(..)
            | ExprKind::ArrayLit(_)
            | ExprKind::ArrayRepeat(..)
            // HOOK str: a text literal and `a + b` on `str` are aggregates
            // and therefore need a place of their own (strtype.rs, round 70)
            | ExprKind::Text(..) => {
                let t = self.ty_of(e);
                let (size, align) = self.size_align(&t);
                let slot = self.alloca(size, align);
                self.write_into(slot, e)?;
                Some(slot)
            }
            ExprKind::Binary(ast::BinOp::Add, _, _)
                if crate::strtype::is_str_like(&self.ty_of(e)) =>
            {
                let t = self.ty_of(e);
                let (size, align) = self.size_align(&t);
                let slot = self.alloca(size, align);
                self.write_into(slot, e)?;
                Some(slot)
            }
            // HOOK fnval: `gc fn(…)` yields an error union — an aggregate,
            // so it needs a place of its own (fnval.rs, round 58).
            ExprKind::Lambda(_) if is_agg(&self.ty_of(e)) => {
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
            // HOOK fnval: `gc fn(…)` — allocate the record and write the
            // captured values into it (fnval.rs, round 58).
            ExprKind::Lambda(d) if crate::errors::union_of(&t).is_some() => {
                crate::fnval::lower_closure(self, addr, d, &t, e.span)
            }
            // HOOK str: THE text literal (strtype.rs, round 70).
            //
            // Where an array is wanted, the inner array literal is written —
            // literally the code of round 39. Where a `str` is wanted, the
            // octets get a place in the frame and the two words `p`/`n` are
            // written into the target. The empty literal `""` needs no place
            // at all: p = 0, n = 0.
            ExprKind::Text(_, inner) => {
                if matches!(t, Type::Array(..)) {
                    return self.write_into(addr, inner);
                }
                let n = crate::strtype::literal_len(inner);
                let sidx = match t {
                    Type::Struct(i) => i,
                    _ => return self.ice(e.span, "text literal without str type"),
                };
                let pa = self.field_addr(addr, sidx, crate::strtype::F_PTR, e.span)?;
                let na = self.field_addr(addr, sidx, crate::strtype::F_LEN, e.span)?;
                let p = if n == 0 {
                    self.constant(FTy::Ptr, 0)
                } else {
                    let bytes = self.alloca(n, 1);
                    self.write_into(bytes, inner)?;
                    bytes
                };
                self.store(FTy::Ptr, pa, p);
                let len = self.constant(FTy::U64, n as i128);
                self.store(FTy::U64, na, len);
                Some(())
            }
            // HOOK str: `a + b` on `str` — the result comes from the
            // collector (strtype.rs, round 70).
            ExprKind::Binary(ast::BinOp::Add, a, b) if crate::strtype::is_str_like(&t) => {
                let a = (**a).clone();
                let b = (**b).clone();
                self.lower_call(crate::strtype::FN_CONCAT, &[a, b], Some(addr), e.span)?;
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
                let ft = self.out_fty(e)?;
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
        // ROUND 94: the position of the EXPRESSION, not only of the
        // statement. That is what makes the line table agree with the panic
        // messages, which have carried file:line:column since round 72 --
        // `tools/dwarf/run.sh` measures exactly that agreement.
        let keep = self.f.loc_stamp;
        self.set_loc(e.span);
        let r = self.lower_expr_inner(e);
        self.f.loc_stamp = keep;
        self.depth -= 1;
        // ROUND 71 — the counterpart to the widening in `sema::expr`. The
        // type checker has noted THAT it happens, here it really happens:
        // `cvtss2sd`. One place for it, so that no context can lose it.
        match r {
            Some(v) if self.info.widen_f32.contains(&e.id) => {
                Some(self.push(FTy::F64, Op::Cast { src: v, from: FTy::F32 }))
            }
            other => other,
        }
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
            // ROUND 70: a text literal is an aggregate in every case (array
            // or `str`) — `is_agg` above has already caught it. The arm
            // exists so that the case split stays complete.
            ExprKind::Text(..) => self.ice(e.span, "text literal as value"),
            // The BIT PATTERN travels into FIR as a constant — there are no float
            // literals there, only bit patterns (fir::FTy::F64/F32).
            //
            // ROUND 71: WHICH bit pattern is decided by the type checker. In
            // an `f32` context the binary64 of the lexer is narrowed here,
            // once, correctly rounded -- and that is exactly the same value
            // that the suffix `1.5f` would have produced.
            ExprKind::Float(bits, single) => {
                let ft = self.fty_of(e)?;
                let v = if ft == FTy::F32 { *single as i128 } else { *bits as i128 };
                Some(self.constant(ft, v))
            }
            ExprKind::FloatF32(bits) => Some(self.constant(FTy::F32, *bits as i128)),
            ExprKind::Bool(b) => Some(self.constant(FTy::Bool, if *b { 1 } else { 0 })),
            // HOOK fnval: a closure WITHOUT captures — its record is one word
            // in `.rodata`, exactly like that of a named function
            // (fnval.rs, round 58).
            ExprKind::Lambda(d) => {
                let key = crate::fnval::record_of(&crate::fnval::closure_fn(d.id));
                Some(self.push(FTy::Ptr, Op::FnRef { name: key }))
            }
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
                } else if self.info.statics.contains_key(name) {
                    // ROUND 89: read a global variable -- its address, then
                    // a load, exactly as for a local.
                    let ft = self.fty_of(e)?;
                    let addr = self.push(FTy::Ptr, Op::GlobalAddr { name: name.clone() });
                    Some(self.load(ft, addr))
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
                // **Round 75** (SPEC §14.5) — `name as *T` where `name` is a
                // directly named function (checked in `sema.rs`, which is
                // the only place allowing `Type::Fn` -> pointer at all): the
                // VALUE of `name` alone is the address of its one-word
                // `.rodata` record (`Op::FnRef`, see the `Ident` arm above);
                // what a C callback needs is the CODE ADDRESS stored INSIDE
                // that word. So this reads through the record once more —
                // the same `load` a `*fn` dereference would do — instead of
                // handing out the record's own address.
                if self.ty_of(inner).is_fn() {
                    let record_addr = self.lower_expr(inner)?;
                    return Some(self.load(FTy::Ptr, record_addr));
                }
                let from = self.out_fty(inner)?;
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
                // ROUND 72 -- checked narrowing `as` (SPEC section 13, item
                // L9). Only a SOURCE-visible narrowing between two INTEGER
                // types is a checked cast: `to` holding fewer bits than
                // `from` can already lose the value outright, and an EQUAL
                // width with a signedness flip can too (`i32 as u32` on a
                // negative value, `u32 as i32` above `i32::MAX`) -- the
                // shared test `panic_rt::emit_checked_cast` runs (narrow to
                // `to`, widen back to `from`, compare) catches both shapes
                // the same way. A WIDENING cast (`to.bits() > from.bits()`)
                // is always lossless and stays `Op::Cast`; so does every
                // float on either side, which SPEC section 14.1 already
                // defines as truncating, not checked, and `bool`/`Ptr`,
                // neither of which a narrowing `as` between them exists for.
                let narrowing_int_cast = crate::checkmode::is_checked()
                    && !from.is_float()
                    && !to.is_float()
                    && from != FTy::Bool
                    && to != FTy::Bool
                    && from != FTy::Ptr
                    && to != FTy::Ptr
                    && to.bits() <= from.bits();
                if narrowing_int_cast {
                    let msg = self.cast_msg(e.span, from, to);
                    return Some(self.push(to, Op::CheckedCast { src, from, msg }));
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
            // `!b` (bool) and `~x` (integer) become the SAME FIR operation;
            // `FUn::Not` reads the instruction type and emits `xor eax, 1`
            // for `bool` and `not` for an integer (codegen_x86.rs).
            ast::UnOp::Not | ast::UnOp::BitNot => {
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
        // HOOK str: `==`/`!=` compare the CONTENT — one call into the
        // collector runtime, the same shape in both compilers
        // (strtype.rs, round 70).
        if matches!(op, B::Eq | B::Ne) && crate::strtype::is_str_like(&self.ty_of(a)) {
            let x = a.clone();
            let y = b.clone();
            let v = match self.lower_call(crate::strtype::FN_EQ, &[x, y], None, e.span)? {
                Some(v) => v,
                None => return self.ice(e.span, "__str_eq without a result"),
            };
            if op == B::Ne {
                return Some(self.push(FTy::Bool, Op::Un(FUn::Not, v)));
            }
            return Some(v);
        }
        if op.is_cmp() {
            // ROUND 71: after the widening BOTH operands are `f64`. The
            // comparison type has to be the one the values really carry,
            // not the one the left side started out with.
            let ty = self.out_fty(a)?;
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
        // ROUND 72 -- explicit "+% -% *%" / "+| -| *|" (SPEC section 13,
        // item L9). Never checked, regardless of the build level: these
        // spellings exist exactly so overflow can be OPTED INTO where it is
        // the point (hashes, checksums, timestamps), rather than forcing a
        // program to switch off checking everywhere else to get there.
        if let Some((kind, fbop)) = op.wrap_sat() {
            let av = self.lower_expr(a)?;
            let bv = self.lower_expr(b)?;
            return Some(self.push(ft, Op::BinWrapSat { kind, op: fbop, a: av, b: bv }));
        }
        // ROUND 72 -- checked "+ - * /" (SPEC section 13, item L9). Only
        // integer types are checked (bool/pointer never reach `Add`/`Sub`/
        // `Mul`/`Div`/`Rem` through the parser in the first place; floating
        // point overflow is a defined IEEE-754 value -- infinity -- and
        // stays exactly as SPEC section 14.1.f64 always described it).
        if crate::checkmode::is_checked() && ft != FTy::Bool && ft != FTy::Ptr && !ft.is_float() {
            match op {
                B::Add | B::Sub | B::Mul => {
                    let av = self.lower_expr(a)?;
                    let bv = self.lower_expr(b)?;
                    let msg = self.overflow_msg(e.span, op, ft);
                    return Some(self.push(ft, Op::CheckedBin { op: bin_of(op), a: av, b: bv, msg }));
                }
                B::Div | B::Rem => {
                    let av = self.lower_expr(a)?;
                    let bv = self.lower_expr(b)?;
                    let msg_zero = self.div_zero_msg(e.span, op, ft);
                    let msg_range = self.div_range_msg(e.span, op, ft);
                    return Some(self.push(
                        ft,
                        Op::CheckedDiv { op: bin_of(op), a: av, b: bv, msg_zero, msg_range },
                    ));
                }
                _ => {}
            }
        }
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

    /// The message text baked into a checked `+ - *` at LOWERING time (SPEC
    /// §13, `L9`): file, line, the type, the operator — everything that is
    /// known here and nowhere later, because FIR carries no source
    /// positions (`dwarf.rs`). The two operand VALUES are appended by the
    /// backend at the panic site, where they sit in registers.
    fn overflow_msg(&self, span: Span, op: ast::BinOp, ft: FTy) -> String {
        format!(
            "panic: integer overflow in '{} {} {}' at {}:{}:{}",
            ft.name(),
            op.text(),
            ft.name(),
            self.dg.file_name(span.file),
            span.line,
            span.col,
        )
    }

    /// ROUND 89 -- the message baked into a checked index at LOWERING
    /// time, in the same three-part shape as the arithmetic messages
    /// above: what went wrong, what the source program actually wrote,
    /// where. The two NUMBERS (the index and the length) are appended by
    /// the backend at the panic site, where it holds them in registers --
    /// under the words `index=` and `len=`, not `a=`/`b=`
    /// (`panic_rt.rs::TRAMPOLINE_INDEX`).
    fn index_msg(&self, span: Span, arr: &Type) -> String {
        format!(
            "panic: index out of bounds in '{}' at {}:{}:{}",
            self.info.tcx.name_of(arr),
            self.dg.file_name(span.file),
            span.line,
            span.col,
        )
    }

    fn div_zero_msg(&self, span: Span, op: ast::BinOp, ft: FTy) -> String {
        format!(
            "panic: division by zero in '{} {} {}' at {}:{}:{}",
            ft.name(),
            op.text(),
            ft.name(),
            self.dg.file_name(span.file),
            span.line,
            span.col,
        )
    }

    fn div_range_msg(&self, span: Span, op: ast::BinOp, ft: FTy) -> String {
        format!(
            "panic: integer overflow ({} MIN {} -1) at {}:{}:{}",
            ft.name(),
            op.text(),
            self.dg.file_name(span.file),
            span.line,
            span.col,
        )
    }

    /// ROUND 72 -- message for a checked narrowing `as` (SPEC section 13,
    /// item L9). Same three-part shape as the arithmetic messages above:
    /// what went wrong, the two types the SOURCE program actually wrote,
    /// where. `from`/`to` (not `ft`) because a cast is the one checked
    /// operation with two DIFFERENT types on either side of the operator.
    fn cast_msg(&self, span: Span, from: FTy, to: FTy) -> String {
        format!(
            "panic: integer overflow casting '{} as {}' at {}:{}:{}",
            from.name(),
            to.name(),
            self.dg.file_name(span.file),
            span.line,
            span.col,
        )
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

    /// **ROUND 70** — the same resolution as `sema::fmt_target`, derived
    /// from the same material: the type of the interpolated expression.
    fn fmt_target(&mut self, name: &str, args: &[Expr]) -> Option<String> {
        let head = crate::sema::fmt_value_head(name)?;
        if args.len() != 2 {
            return None;
        }
        let t = self.ty_of(&args[1]);
        let step = if crate::strtype::is_str_like(&t) {
            "fmt_str"
        } else if t.is_concrete_int() && t.is_signed() {
            "fmt_number"
        } else if t.is_concrete_int() {
            "fmt_u64"
        } else if t == Type::Bool {
            "fmt_bool"
        } else if t == Type::F64 {
            "fmt_f64"
        } else if t == Type::F32 {
            "fmt_f32"
        } else {
            return None;
        };
        Some(format!("{}{}", head, step))
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
        // HOOK str: `f"{x}"` — the step is derived from the type of the
        // argument, exactly as in the type check (sema::fmt_target, round 70).
        let fmt_real;
        let name: &str = match self.fmt_target(name, args) {
            Some(r) => {
                fmt_real = r;
                &fmt_real
            }
            None => name,
        };
        // HOOK constant-time: select/barrier/secure_zero (ct.rs, SPEC §9.2/§9.3)
        // HOOK faden: the three thread primitives (thread.rs, round 49)
        if crate::thread::is_thread_call(name) && !self.info.fns.contains_key(name) {
            return crate::thread::lower_thread_call(self, name, args, span);
        }
        // HOOK atomar: the atomic primitive (atomic.rs, round 47)
        // HOOK simd (round 82): the vector and crypto instructions.
        if crate::simd::is_simd_call(name) && !self.info.fns.contains_key(name) {
            return crate::simd::lower_call(self, name, args, span);
        }
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
        // ROUND 68 (impls.rs::field_fn): `c.hook(a, b)` where `hook` is a
        // FIELD of the receiver holding a function value. Word 0 of the
        // record is the code address; the receiver itself is NOT an
        // argument, it is only the place the value is read from.
        let mut field_target: Option<((Val, Val), crate::sema::FnSig)> = None;
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
                    None => {
                        let (sidx, params, ret) =
                            match crate::impls::field_fn(&self.info.tcx, &et, m) {
                                Some(x) => x,
                                None => return self.ice(span, "unknown method in lowering"),
                            };
                        let recv = match args.first() {
                            Some(e) => e.clone(),
                            None => return self.ice(span, "method call without receiver"),
                        };
                        // The receiver is evaluated FIRST — same order as
                        // for an ordinary method call.
                        let base = match &et {
                            Type::Struct(_) => self.lower_addr(&recv)?,
                            _ => self.lower_expr(&recv)?,
                        };
                        let fa = self.field_addr(base, sidx, m, span)?;
                        let rec = self.load(FTy::Ptr, fa);
                        let code = self.load(FTy::Ptr, rec);
                        field_target =
                            Some(((code, rec), crate::sema::FnSig { params, ret }));
                        name
                    }
                }
            }
        };
        // ROUND 58 (fnval.rs): the callee sits in a VARIABLE of function
        // type. Word 0 of the function record is the code address; the
        // record itself goes in as the LAST argument, so that a closure body
        // finds its captured values there (a named function never reads it).
        let mut indirect: Option<(Val, Val)> = None;
        // ROUND 68: with a function value out of a field the receiver drops
        // out of the argument list.
        let mut skip_receiver = false;
        let sig = match dyn_sig {
            Some(s) => s,
            None => match field_target {
                Some((cr, s)) => {
                    indirect = Some(cr);
                    skip_receiver = true;
                    s
                }
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
            // ROUND 68: the receiver of a function value out of a field is
            // no argument — it was read above already.
            if i == 0 && skip_receiver {
                continue;
            }
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
                let mut v = self.lower_expr(a)?;
                // ROUND 70: a narrower integer is widened to the width of the
                // parameter. Until now argument and parameter always had the
                // same type, so nothing changes here; `f"{x}"` with an i32 is
                // the first place where an i32 meets an i64 parameter
                // (`io.fmt_number`), and the upper half of the register has
                // to be right for that.
                if let (Some(pt), Some(at)) = (sig.params.get(i), scalar_fty(&t)) {
                    if t.is_concrete_int() && pt.is_concrete_int() && pt.bits() > t.bits() {
                        if let Some(want) = scalar_fty(pt) {
                            if want != at {
                                v = self.push(want, Op::Cast { src: v, from: at });
                            }
                        }
                    }
                }
                vals.push(v);
                continue;
            }
            let (size, align) = self.size_align(&t);
            match abi::classify(&t, &self.info.tcx) {
                c @ ArgClass::Regs { .. } => {
                    let addr = self.lower_addr(a)?;
                    let ws0: Vec<abi::Word> = c.words().to_vec();
                    let mut ws = self.load_words(addr, size, &ws0)?;
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
                // ROUND 71: an aggregate of at most 8 bytes comes back in
                // `rax` -- or in `xmm0`, if its single eightbyte is SSE.
                let rw = abi::classify(&sig.ret, &self.info.tcx)
                    .words()
                    .first()
                    .copied()
                    .unwrap_or(abi::Word::Int);
                let w = self.push(word_fty(rw), op);
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
            // ROUND 94: the stamp stays until the next statement sets it --
            // every instruction of this statement carries it, not only the
            // first one (`fir::Loc`).
            self.set_loc(sp);
            self.decl_line = Some((sp.file, sp.line));
        }
        match s {
            Stmt::Error(_) => Some(()),
            Stmt::Let { name, init, span, .. } => {
                // HOOK fehlerunionen: implicit conversion at 'let' (lower_errors.rs)
                if let Some(r) = crate::lower_errors::hook_let(self, name, init) {
                    return r;
                }
                // ROUND 71: the variable gets the type that the initialiser
                // HANDS OUT -- `let w: f64 = x_f32` makes an f64 variable,
                // not a four byte one.
                let t = self.out_ty(init);
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
            // ROUND 70 - `x op= e`.
            //
            // THE POINT OF THE WHOLE THING stands in the first line: the
            // address of the target is computed EXACTLY ONCE. With
            // `a[f()] += 1` a rewrite into `a[f()] = a[f()] + 1` would run
            // `f()` twice - the classic mistake of this extension.
            // `tests/1338_assign_op_once.fi` counts the calls.
            //
            // Everything after that is what `x = x op e` produces anyway:
            // load, the same FIR operation, store. So the overflow rules
            // are the same too, because it is the same instruction.
            Stmt::AssignOp { target, op, value, span } => {
                let addr = self.lower_addr(target)?;
                let t = self.ty_of(target);
                // `s += x` on `str`: the collector builds the new octets.
                // The target is PINNED, so that passing it as an argument
                // does not compute its address a second time.
                if crate::strtype::is_str_like(&t) {
                    self.pinned.insert(target.id, addr);
                    let a = target.clone();
                    let b = value.clone();
                    let r = self.lower_call(crate::strtype::FN_CONCAT, &[a, b], Some(addr), *span);
                    self.pinned.remove(&target.id);
                    r?;
                    return crate::gc_lower::hook_assign(self, target);
                }
                let ft = self.fty_of(target)?;
                let cur = self.load(ft, addr);
                let mut rhs = self.lower_expr(value)?;
                if matches!(op, ast::BinOp::Shl | ast::BinOp::Shr) {
                    // The shift amount is brought to the width of the left
                    // operand - the same line as in `lower_binary`.
                    let bt = self.fty_of(value)?;
                    if bt != ft {
                        rhs = self.push(ft, Op::Cast { src: rhs, from: bt });
                    }
                }
                let bop = match op {
                    ast::BinOp::Add => FBin::Add,
                    ast::BinOp::Sub => FBin::Sub,
                    ast::BinOp::Mul => FBin::Mul,
                    ast::BinOp::Div => FBin::Div,
                    ast::BinOp::Rem => FBin::Rem,
                    ast::BinOp::And => FBin::And,
                    ast::BinOp::Or => FBin::Or,
                    ast::BinOp::Xor => FBin::Xor,
                    ast::BinOp::Shl => FBin::Shl,
                    ast::BinOp::Shr => FBin::Shr,
                    _ => return self.ice(*span, "unknown operator in a compound assignment"),
                };
                let res = self.push(ft, Op::Bin(bop, cur, rhs));
                self.store(ft, addr, res);
                crate::gc_lower::hook_assign(self, target)
            }
            // ROUND 70 - `x++` / `x--`: load, plus/minus one, store. The
            // address is computed once here as well.
            Stmt::Step { target, up, .. } => {
                let addr = self.lower_addr(target)?;
                let ft = self.fty_of(target)?;
                let cur = self.load(ft, addr);
                let one = self.constant(ft, 1);
                let bop = if *up { FBin::Add } else { FBin::Sub };
                let res = self.push(ft, Op::Bin(bop, cur, one));
                self.store(ft, addr, res);
                Some(())
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
                                    // ROUND 71: the single eightbyte keeps
                                    // its class -- `rax` or `xmm0`.
                                    let rw: Vec<abi::Word> = abi::classify(&t, &self.info.tcx)
                                        .words()
                                        .to_vec();
                                    let w = self.load_words(addr, size, &rw)?;
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
        // ROUND 64: with the TYPE, not without. Until now the loop variable
        // was the only declared name without one, and in the debugger it
        // showed up as `<error>`.
        self.declare_ty(name, islot, ty.clone());
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
                c @ ArgClass::Regs { .. } => {
                    let ws: Vec<abi::Word> = c.words().to_vec();
                    for w in &ws {
                        pf.push(word_fty(*w));
                    }
                    kinds.push(ParamKind::Words(ws.len()));
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
        // aggregate: either a pointer (sret) or ONE word — in `rax`, or in
        // `xmm0` when that eightbyte is of the SSE class (round 71).
        if sret {
            FTy::Ptr
        } else {
            word_fty(
                abi::classify(&sig.ret, &info.tcx)
                    .words()
                    .first()
                    .copied()
                    .unwrap_or(abi::Word::Int),
            )
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
        pinned: HashMap::new(),
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

        params_done: false,
        decl_line: None,
    };
    let mut next = 0usize;
    if sret {
        lo.sret = Some(lo.f.param_val(0));
        next = 1;
    }
    lo.enter();
    // ROUND 64: the parameters carry the line of the `fn` declaration.
    if !d.span.is_none() {
        lo.decl_line = Some((d.span.file, d.span.line));
    }
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
    // ROUND 64: from here on every declared name is a local variable.
    lo.params_done = true;
    // The result type, for `DW_AT_type` of the subprogram.
    if dwarf::with_variables() {
        dwarf::set_fn_type(&d.name, crate::dwarf_info::dtype_of(&info.tcx, &sig.ret));
    }
    // HOOK fnval: inside a closure body the captured values are ordinary
    // names — they lie in the record, which arrived as the last parameter
    // (fnval.rs, round 58).
    if let Some(caps) = crate::fnval::captures_of_fn(&d.name) {
        let env = match lo.lookup(crate::fnval::ENV_PARAM) {
            Some(slot) => lo.load(FTy::Ptr, slot),
            None => {
                dg.error(d.span, "internal error while lowering to FIR: closure without record");
                return None;
            }
        };
        for c in caps {
            let a = lo.field_addr_at(env, c.off);
            lo.declare_ty(&c.name, a, c.ty.clone());
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
        // **Round 75** (SPEC §14.5) — `extern fn` has no Firn body: there
        // is nothing to lower to FIR. Its signature already sits in
        // `info.fns` (from `sema::collect_fns`, unconditionally) so every
        // CALLER of it type-checks and lowers normally; `codegen_x86::label`
        // resolves the call name through `extfn.rs` instead of expecting a
        // `Func` with this name to exist in `m.funcs`.
        if d.extern_info.is_some() {
            continue;
        }
        match lower_fn(d, info, dg) {
            Some(f) => m.funcs.push(f),
            None => ok = false,
        }
    }
    // HOOK fnval: every closure literal becomes a function of its own
    // (fnval.rs, round 58). Its last parameter is the record.
    for d in crate::fnval::collect(prog) {
        let fd = ast::FnDecl {
            name: crate::fnval::closure_fn(d.id),
            params: crate::fnval::closure_params(&d),
            ret: d.ret.clone(),
            body: d.body.clone(),
            span: d.span,
            attrs: Vec::new(),
            extern_info: None,
        };
        match lower_fn(&fd, info, dg) {
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
        ast::Block { stmts, span: Span::new(1, 1, 1), end: Span::new(1, 1, 1) }
    }

    fn info_of(b: &B, tcx: TypeCtx, fns: Vec<(&str, FnSig)>) -> TypeInfo {
        let mut ti = TypeInfo {
            tcx,
            expr_types: b.types.clone(),
            consts: HashMap::new(),
            statics: HashMap::new(),
            fns: HashMap::new(),
            widen_f32: std::collections::HashSet::new(),
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
            extern_info: None,
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
            extern_info: None,
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
            extern_info: None,
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
            extern_info: None,
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
            extern_info: None,
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
