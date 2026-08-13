//! Lowering AST -> FIR.
//!
//! SCHNITTSTELLE (fest):
//!   `pub fn lower(prog: &ast::Program, info: &sema::TypeInfo, dg: &mut Diags)
//!        -> Option<fir::Module>`
//! Zusicherung an das Backend: jeder Block hat einen echten Terminator
//! (kein `Term::Unset`), alle `alloca` stehen im Eintrittsblock.
//!
//! Variablenmodell (siehe docs/FIR.md): KEINE Phi-Knoten. Jede lokale Variable
//! und jeder Parameter bekommt einen `alloca`-Slot; Zugriffe sind `load`/
//! `store`. Aggregate (Structs, Arrays) sind nie FIR-Werte, sondern immer nur
//! Adressen; Kopien laufen ueber `copymem`.

use std::collections::HashMap;

use crate::ast::{self, Expr, ExprKind, Program, Stmt};
use crate::diag::{Diags, Span};
use crate::fir::{
    BinOp as FBin, BlockId, CmpOp, FTy, Func, Inst, Module, Op, Term, UnOp as FUn, Val,
};
use crate::sema::TypeInfo;
use crate::types::Type;

/// Obergrenze fuer Verschachtelung (Schutz vor Rekursionsexplosion).
const MAX_DEPTH: u32 = 200;

/// Skalarer FIR-Typ zu einem Quelltyp; `None` fuer Aggregate und fuer
/// (nach der Typpruefung eigentlich unmoegliche) unaufgeloeste Typen.
fn scalar_fty(t: &Type) -> Option<FTy> {
    Some(match t {
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
        Type::Void => FTy::Void,
        Type::Array(..) | Type::Struct(_) | Type::UntypedInt | Type::Error => return None,
    })
}

fn is_agg(t: &Type) -> bool {
    matches!(t, Type::Array(..) | Type::Struct(_))
}

struct Local {
    slot: Val,
}

struct Lower<'a> {
    info: &'a TypeInfo,
    dg: &'a mut Diags,
    f: Func,
    cur: BlockId,
    scopes: Vec<HashMap<String, Local>>,
    depth: u32,
}

impl<'a> Lower<'a> {
    fn err<T>(&mut self, span: Span, msg: impl Into<String>) -> Option<T> {
        self.dg.error(span, msg);
        None
    }

    /// Interner Fehler: nur erreichbar, wenn die Typpruefung ihre Zusicherung
    /// verletzt. Wird als normale Diagnose gemeldet, nie als Panik.
    fn ice<T>(&mut self, span: Span, what: &str) -> Option<T> {
        self.dg.error(
            span,
            format!("interner fehler beim uebersetzen nach FIR: {}", what),
        );
        None
    }

    // ---- Hilfen -------------------------------------------------------

    fn ty_of(&self, e: &Expr) -> Type {
        self.info.expr_ty(e.id).clone()
    }

    fn fty_of(&mut self, e: &Expr) -> Option<FTy> {
        let t = self.ty_of(e);
        match scalar_fty(&t) {
            Some(f) => Some(f),
            None => self.ice(e.span, "ausdruck hat keinen skalaren typ"),
        }
    }

    fn size_align(&self, t: &Type) -> (u64, u64) {
        (
            self.info.tcx.size_of(t).max(1),
            self.info.tcx.align_of(t).max(1),
        )
    }

    fn push(&mut self, ty: FTy, op: Op) -> Val {
        self.f.push(self.cur, ty, op)
    }

    fn push_void(&mut self, ty: FTy, op: Op) {
        self.f.push_void(self.cur, ty, op)
    }

    fn konst(&mut self, ty: FTy, v: i128) -> Val {
        self.push(ty, Op::Const(v))
    }

    fn load(&mut self, ty: FTy, addr: Val) -> Val {
        self.push(ty, Op::Load { addr })
    }

    fn store(&mut self, ty: FTy, addr: Val, val: Val) {
        self.push_void(ty, Op::Store { addr, val })
    }

    /// `base + off` Bytes; konstante 0 wird weggelassen.
    fn ptradd_const(&mut self, base: Val, off: u64) -> Val {
        if off == 0 {
            return base;
        }
        let o = self.konst(FTy::I64, off as i128);
        self.push(FTy::Ptr, Op::PtrAdd { base, off: o })
    }

    fn new_block(&mut self) -> BlockId {
        self.f.add_block()
    }

    fn set_term(&mut self, t: Term) {
        let b = self.cur;
        self.f.set_term(b, t);
    }

    fn terminated(&self) -> bool {
        self.f.is_terminated(self.cur)
    }

    fn enter(&mut self) {
        self.scopes.push(HashMap::new());
    }
    fn leave(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: &str, slot: Val) {
        if let Some(s) = self.scopes.last_mut() {
            s.insert(name.to_string(), Local { slot });
        }
    }

    fn lookup(&self, name: &str) -> Option<Val> {
        for s in self.scopes.iter().rev() {
            if let Some(l) = s.get(name) {
                return Some(l.slot);
            }
        }
        None
    }

    // ---- Ausdruecke: Adresse (lvalue / Aggregat) -----------------------

    fn lower_addr(&mut self, e: &Expr) -> Option<Val> {
        if self.depth > MAX_DEPTH {
            return self.err(e.span, "ausdruck zu tief verschachtelt");
        }
        self.depth += 1;
        let r = self.lower_addr_inner(e);
        self.depth -= 1;
        r
    }

    fn lower_addr_inner(&mut self, e: &Expr) -> Option<Val> {
        match &e.kind {
            ExprKind::Ident(name) => match self.lookup(name) {
                Some(slot) => Some(slot),
                None => {
                    if self.info.consts.contains_key(name) {
                        self.err(e.span, "eine konstante hat keine adresse")
                    } else {
                        self.ice(e.span, "unbekannter name im lowering")
                    }
                }
            },
            ExprKind::Unary(ast::UnOp::Deref, inner) => self.lower_expr(inner),
            ExprKind::Field(base, fname, fspan) => {
                let bt = self.ty_of(base);
                let (sidx, baddr) = match &bt {
                    Type::Struct(i) => (*i, self.lower_addr(base)?),
                    // `p.f` auf einem Zeiger auf Struct: automatisch dereferenzieren
                    Type::Ptr { inner, .. } => match **inner {
                        Type::Struct(i) => (i, self.lower_expr(base)?),
                        _ => return self.ice(*fspan, "feldzugriff auf nicht-struct"),
                    },
                    _ => return self.ice(*fspan, "feldzugriff auf nicht-struct"),
                };
                let off = match self.info.tcx.structs.get(sidx).and_then(|s| s.field(fname)) {
                    Some(f) => f.offset,
                    None => return self.ice(*fspan, "unbekanntes feld im lowering"),
                };
                Some(self.ptradd_const(baddr, off))
            }
            ExprKind::Index(base, idx) => {
                let bt = self.ty_of(base);
                let (elem, baddr) = match &bt {
                    Type::Array(el, _) => ((**el).clone(), self.lower_addr(base)?),
                    Type::Ptr { inner, .. } => ((**inner).clone(), self.lower_expr(base)?),
                    _ => return self.ice(e.span, "index auf nicht-indizierbarem typ"),
                };
                let esz = self.info.tcx.size_of(&elem).max(1);
                let iv = self.lower_expr(idx)?;
                let ift = self.fty_of(idx)?;
                let iv64 = if ift == FTy::U64 {
                    iv
                } else {
                    self.push(FTy::U64, Op::Cast { src: iv, from: ift })
                };
                let sz = self.konst(FTy::U64, esz as i128);
                let off = self.push(FTy::U64, Op::Bin(FBin::Mul, iv64, sz));
                Some(self.push(FTy::Ptr, Op::PtrAdd { base: baddr, off }))
            }
            ExprKind::StructLit(..) | ExprKind::ArrayLit(_) => {
                let t = self.ty_of(e);
                let (size, align) = self.size_align(&t);
                let slot = self.f.alloca(size, align);
                self.write_into(slot, e)?;
                Some(slot)
            }
            _ => self.err(e.span, "dieser ausdruck hat keine adresse"),
        }
    }

    /// Schreibt den Wert von `e` an die Adresse `addr` (skalar: `store`,
    /// Literal: feld-/elementweise, sonstiges Aggregat: `copymem`).
    fn write_into(&mut self, addr: Val, e: &Expr) -> Option<()> {
        if self.depth > MAX_DEPTH {
            return self.err(e.span, "ausdruck zu tief verschachtelt");
        }
        self.depth += 1;
        let r = self.write_into_inner(addr, e);
        self.depth -= 1;
        r
    }

    fn write_into_inner(&mut self, addr: Val, e: &Expr) -> Option<()> {
        let t = self.ty_of(e);
        match &e.kind {
            ExprKind::StructLit(_, fields, span) => {
                let sidx = match t {
                    Type::Struct(i) => i,
                    _ => return self.ice(*span, "struct-literal ohne struct-typ"),
                };
                for (fname, fexpr, fspan) in fields {
                    let off = match self.info.tcx.structs.get(sidx).and_then(|s| s.field(fname)) {
                        Some(f) => f.offset,
                        None => return self.ice(*fspan, "unbekanntes feld im lowering"),
                    };
                    let fa = self.ptradd_const(addr, off);
                    self.write_into(fa, fexpr)?;
                }
                Some(())
            }
            ExprKind::ArrayLit(elems) => {
                let et = match &t {
                    Type::Array(el, _) => (**el).clone(),
                    _ => return self.ice(e.span, "array-literal ohne array-typ"),
                };
                let esz = self.info.tcx.size_of(&et).max(1);
                for (i, el) in elems.iter().enumerate() {
                    let ea = self.ptradd_const(addr, esz * i as u64);
                    self.write_into(ea, el)?;
                }
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

    // ---- Ausdruecke: Wert ---------------------------------------------

    fn lower_expr(&mut self, e: &Expr) -> Option<Val> {
        if self.depth > MAX_DEPTH {
            return self.err(e.span, "ausdruck zu tief verschachtelt");
        }
        self.depth += 1;
        let r = self.lower_expr_inner(e);
        self.depth -= 1;
        r
    }

    fn lower_expr_inner(&mut self, e: &Expr) -> Option<Val> {
        let t = self.ty_of(e);
        if is_agg(&t) {
            return self.ice(e.span, "aggregat als wert (nur adressen erlaubt)");
        }
        match &e.kind {
            ExprKind::Int(v) => {
                let ft = self.fty_of(e)?;
                Some(self.konst(ft, *v))
            }
            ExprKind::Bool(b) => Some(self.konst(FTy::Bool, if *b { 1 } else { 0 })),
            ExprKind::Ident(name) => {
                if let Some(slot) = self.lookup(name) {
                    let ft = self.fty_of(e)?;
                    Some(self.load(ft, slot))
                } else if let Some((ct, cv)) = self.info.consts.get(name).cloned() {
                    let ft = match scalar_fty(&ct) {
                        Some(f) => f,
                        None => return self.ice(e.span, "konstante mit nicht-skalarem typ"),
                    };
                    Some(self.konst(ft, cv))
                } else {
                    self.ice(e.span, "unbekannter name im lowering")
                }
            }
            ExprKind::Unary(op, inner) => self.lower_unary(e, *op, inner),
            ExprKind::Binary(op, a, b) => self.lower_binary(e, *op, a, b),
            ExprKind::Field(..) | ExprKind::Index(..) => {
                let addr = self.lower_addr(e)?;
                let ft = self.fty_of(e)?;
                Some(self.load(ft, addr))
            }
            ExprKind::Call(name, args, span) => {
                let ft = self.fty_of(e)?;
                if ft == FTy::Void {
                    return self.err(
                        *span,
                        "aufruf ohne rueckgabewert kann nicht als wert benutzt werden",
                    );
                }
                let a = self.lower_args(args)?;
                Some(self.push(ft, Op::Call { name: name.clone(), args: a }))
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
                    // `x as bool` ist definiert als `x != 0` (0/1, nie 2).
                    let z = self.konst(from, 0);
                    return Some(self.push(
                        FTy::Bool,
                        Op::Cmp { op: CmpOp::Ne, ty: from, a: src, b: z },
                    ));
                }
                Some(self.push(to, Op::Cast { src, from }))
            }
            ExprKind::StructLit(..) | ExprKind::ArrayLit(_) => {
                self.ice(e.span, "literal eines aggregats als wert")
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
        if op.is_logic() {
            return self.lower_shortcircuit(op, a, b);
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
            _ => return self.ice(e.span, "unbekannter binaeroperator"),
        };
        let av = self.lower_expr(a)?;
        let mut bv = self.lower_expr(b)?;
        if matches!(op, B::Shl | B::Shr) {
            // Der Verschiebungsbetrag wird auf die Breite des linken Operanden
            // gebracht; die Art der Verschiebung richtet sich nach `ft`.
            let bt = self.fty_of(b)?;
            if bt != ft {
                bv = self.push(ft, Op::Cast { src: bv, from: bt });
            }
        }
        Some(self.push(ft, Op::Bin(bop, av, bv)))
    }

    /// `&&` / `||` kurzschliessend: Ergebnis-Slot + Verzweigung, keine
    /// arithmetische Ersatzoperation.
    fn lower_shortcircuit(&mut self, op: ast::BinOp, a: &Expr, b: &Expr) -> Option<Val> {
        let slot = self.f.alloca(1, 1);
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

    fn lower_args(&mut self, args: &[Expr]) -> Option<Vec<Val>> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            let t = self.ty_of(a);
            if is_agg(&t) {
                return self.err(
                    a.span,
                    "aggregate als argument werden in stufe 0 nicht unterstuetzt",
                );
            }
            out.push(self.lower_expr(a)?);
        }
        Some(out)
    }

    /// Alle Syscall-Argumente auf `i64` erweitern (signed: vorzeichen-,
    /// sonst nullerweitert).
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

    // ---- Anweisungen ---------------------------------------------------

    fn lower_block(&mut self, b: &ast::Block) -> Option<()> {
        if self.depth > MAX_DEPTH {
            return self.err(b.span, "block zu tief verschachtelt");
        }
        self.depth += 1;
        self.enter();
        let mut r = Some(());
        for s in &b.stmts {
            if r.is_none() {
                break;
            }
            r = self.lower_stmt(s);
        }
        self.leave();
        self.depth -= 1;
        r
    }

    fn lower_stmt(&mut self, s: &Stmt) -> Option<()> {
        match s {
            Stmt::Error(_) => Some(()),
            Stmt::Let { name, init, span, .. } => {
                let t = self.ty_of(init);
                if matches!(t, Type::Void) {
                    return self.err(*span, "eine variable kann keinen wert ohne typ haben");
                }
                let (size, align) = self.size_align(&t);
                let slot = self.f.alloca(size, align);
                self.write_into(slot, init)?;
                self.declare(name, slot);
                Some(())
            }
            Stmt::Assign { target, value, .. } => {
                let addr = self.lower_addr(target)?;
                self.write_into(addr, value)
            }
            Stmt::Expr(e) => self.lower_expr_stmt(e),
            Stmt::Block(b) => self.lower_block(b),
            Stmt::Return { value, span } => {
                match value {
                    Some(v) => {
                        let t = self.ty_of(v);
                        if is_agg(&t) {
                            return self.err(
                                *span,
                                "rueckgabe eines aggregats wird in stufe 0 nicht unterstuetzt",
                            );
                        }
                        let rv = self.lower_expr(v)?;
                        self.set_term(Term::Ret(Some(rv)));
                    }
                    None => self.set_term(Term::Ret(None)),
                }
                // Alles danach ist unerreichbar: neuen Block oeffnen, damit die
                // Invariante "ein Terminator je Block" gilt.
                let dead = self.new_block();
                self.cur = dead;
                Some(())
            }
            Stmt::If { cond, then, els, .. } => self.lower_if(cond, then, els.as_deref()),
            Stmt::While { cond, body, .. } => self.lower_while(cond, body),
        }
    }

    fn lower_expr_stmt(&mut self, e: &Expr) -> Option<()> {
        match &e.kind {
            ExprKind::Call(name, args, _) => {
                let t = self.ty_of(e);
                let a = self.lower_args(args)?;
                let op = Op::Call { name: name.clone(), args: a };
                match scalar_fty(&t) {
                    Some(FTy::Void) | None => self.push_void(FTy::Void, op),
                    Some(ft) => {
                        let _ = self.push(ft, op);
                    }
                }
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
        self.lower_block(body)?;
        if !self.terminated() {
            self.set_term(Term::Br(head_bb));
        }

        self.cur = end_bb;
        Some(())
    }

    /// Schliesst alle noch offenen (unerreichbaren) Bloecke ab, damit kein
    /// `Term::Unset` uebrig bleibt.
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

fn lower_fn(d: &ast::FnDecl, info: &TypeInfo, dg: &mut Diags) -> Option<Func> {
    let sig = match info.fns.get(&d.name) {
        Some(s) => s.clone(),
        None => {
            dg.error(
                d.span,
                format!("interner fehler beim uebersetzen nach FIR: signatur von '{}' fehlt", d.name),
            );
            return None;
        }
    };
    let mut pf = Vec::with_capacity(sig.params.len());
    for (i, p) in sig.params.iter().enumerate() {
        let span = d.params.get(i).map(|p| p.span).unwrap_or(d.span);
        match scalar_fty(p) {
            Some(FTy::Void) | None => {
                dg.error(
                    span,
                    "parameter dieses typs werden in stufe 0 nicht unterstuetzt (nur skalare typen)",
                );
                return None;
            }
            Some(f) => pf.push(f),
        }
    }
    let rf = match scalar_fty(&sig.ret) {
        Some(f) => f,
        None => {
            dg.error(
                d.span,
                "rueckgabetypen dieser art werden in stufe 0 nicht unterstuetzt (nur skalare typen)",
            );
            return None;
        }
    };

    let f = Func::new(&d.name, pf.clone(), rf);
    let mut lo = Lower {
        info,
        dg,
        f,
        cur: 0,
        scopes: Vec::new(),
        depth: 0,
    };
    lo.enter();
    // Parameter bekommen einen Slot und werden im Eintrittsblock gesichert.
    for (i, p) in d.params.iter().enumerate() {
        let ft = match pf.get(i) {
            Some(f) => *f,
            None => break,
        };
        let slot = lo.f.alloca(ft.bytes().max(1), ft.bytes().max(1));
        let pv = lo.f.param_val(i);
        lo.store(ft, slot, pv);
        lo.declare(&p.name, slot);
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
    // Invariante pruefen, statt sie nur zu behaupten.
    for f in &m.funcs {
        for b in &f.blocks {
            if matches!(b.term, Term::Unset) {
                dg.error(
                    Span::none(),
                    format!(
                        "interner fehler beim uebersetzen nach FIR: block bb{} in '{}' ohne terminator",
                        b.id, f.name
                    ),
                );
                return None;
            }
            if b.id != 0 && b.insts.iter().any(|i: &Inst| matches!(i.op, Op::Alloca { .. })) {
                dg.error(
                    Span::none(),
                    format!(
                        "interner fehler beim uebersetzen nach FIR: alloca ausserhalb des eintrittsblocks in '{}'",
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

    /// Kleiner AST-Baukasten, damit das Lowering unabhaengig von Parser und
    /// Typpruefer geprueft werden kann.
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
        assert!(m.is_some(), "lowering lieferte kein modul");
        m.unwrap_or_default()
    }

    #[test]
    fn einfaches_return() {
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
    fn kurzschluss_erzeugt_verzweigung() {
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
        // Der Kurzschluss laeuft ueber einen bool-Slot, nicht ueber `and`.
        assert!(!t.contains("and.bool"), "{}", t);
    }

    /// Baut den AST des Beispielprogramms aus `docs/FIR.md` (siehe dort den
    /// Quelltext) und liefert Programm + Typinformation.
    fn doku_programm() -> (Program, TypeInfo) {
        let mut tcx = TypeCtx::new();
        let pi = tcx.declare("Point");
        tcx.set_fields(pi, vec![("x".into(), Type::I32), ("y".into(), Type::I32)]);
        let pt = Type::Struct(pi);
        let i32t = Type::I32;
        let mut b = B::new();

        // --- fn summe(n: i32) -> i32 ---
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
        let summe = ast::FnDecl {
            name: "summe".into(),
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
        let call = b.e(ExprKind::Call("summe".into(), vec![lim], Span::none()), i32t.clone());
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
        };

        let prog = Program { funcs: vec![summe, mainf], expr_count: b.next, ..Default::default() };
        let mut info = info_of(
            &b,
            tcx,
            vec![
                ("summe", FnSig { params: vec![Type::I32], ret: Type::I32 }),
                ("main", FnSig { params: vec![], ret: Type::I32 }),
            ],
        );
        info.consts.insert("LIMIT".into(), (Type::I32, 10));
        (prog, info)
    }

    /// Der in `docs/FIR.md` abgedruckte Dump muss dem entsprechen, was das
    /// Lowering wirklich erzeugt.
    #[test]
    fn doku_beispiel_stimmt() {
        let (prog, info) = doku_programm();
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
        assert!(!want.is_empty(), "kein ```firdump-Block in docs/FIR.md gefunden");
        assert_eq!(want, got);
    }

    #[test]
    fn while_und_index() {
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
        // alle Allocas im Eintrittsblock
        let f0 = &m.funcs[0];
        for b2 in &f0.blocks[1..] {
            assert!(!b2.insts.iter().any(|i| matches!(i.op, Op::Alloca { .. })));
        }
    }
}
