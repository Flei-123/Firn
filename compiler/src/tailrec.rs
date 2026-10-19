//! Self tail-recursion elimination (pass `tailrec`, release levels only).
//!
//! Two shapes of a self call in tail position are turned into a jump back to
//! the start of the function:
//!
//!   (a) `return f(args)`                    -- plain tail call
//!   (b) `return e OP f(args)` / `f(args) OP e` -- accumulator form, for
//!       OP in {add, mul, or, xor} on integers and the explicit wrapping
//!       `+%` / `*%`. These are associative AND commutative, so
//!       `f(x) = e OP f(x')` becomes `f'(x, acc) = acc OP f(x)` with
//!       `f'(x, acc) = f'(x', acc OP e)` and `f'` returns `acc OP v` wherever
//!       `f` returned `v`. The accumulator starts at the identity.
//!
//! What is deliberately NOT done (correctness before gain):
//!   * checked `+` / `*` (`CheckedBin`, release-safe): re-association changes
//!     WHICH partial result overflows (and so the panic text and the time of
//!     the panic relative to later side effects). Such returns stay calls.
//!     Plain tail calls (a) are always safe and are done on every level.
//!   * `and`, floats, saturating ops, bool.
//!   * functions with `#[no_inline]` (stack depth is wanted), constant time /
//!     secret code, interrupt handlers, inline assembler, volatile ops,
//!     entry blocks that are jump targets, and any stack cell (`alloca`)
//!     whose address is used for anything but a direct load/store -- the
//!     frame slot is reused by the next iteration, a pointer into the old
//!     frame handed down the recursion would see it overwritten.
//!
//! Observable change: the stack no longer grows with the recursion depth, so
//! a recursion that used to overflow the stack now runs (in constant stack).
//!
//! Switch it off with `--no-pass=tailrec` or `FIRN_NO_TAILREC=1`.

use crate::fir::*;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AccOp {
    Add,
    Mul,
    Or,
    Xor,
    WrapAdd,
    WrapMul,
}

impl AccOp {
    fn identity(self) -> i128 {
        match self {
            AccOp::Add | AccOp::Or | AccOp::Xor | AccOp::WrapAdd => 0,
            AccOp::Mul | AccOp::WrapMul => 1,
        }
    }
    fn op(self, a: Val, b: Val) -> Op {
        match self {
            AccOp::Add => Op::Bin(BinOp::Add, a, b),
            AccOp::Mul => Op::Bin(BinOp::Mul, a, b),
            AccOp::Or => Op::Bin(BinOp::Or, a, b),
            AccOp::Xor => Op::Bin(BinOp::Xor, a, b),
            AccOp::WrapAdd => Op::BinWrapSat { kind: WrapSatKind::Wrap, op: BinOp::Add, a, b },
            AccOp::WrapMul => Op::BinWrapSat { kind: WrapSatKind::Wrap, op: BinOp::Mul, a, b },
        }
    }
}

fn is_int(t: FTy) -> bool {
    matches!(t, FTy::I8 | FTy::I16 | FTy::I32 | FTy::I64 | FTy::U8 | FTy::U16 | FTy::U32 | FTy::U64)
}

fn acc_op_of(op: &Op) -> Option<(AccOp, Val, Val)> {
    match op {
        Op::Bin(BinOp::Add, a, b) => Some((AccOp::Add, *a, *b)),
        Op::Bin(BinOp::Mul, a, b) => Some((AccOp::Mul, *a, *b)),
        Op::Bin(BinOp::Or, a, b) => Some((AccOp::Or, *a, *b)),
        Op::Bin(BinOp::Xor, a, b) => Some((AccOp::Xor, *a, *b)),
        Op::BinWrapSat { kind: WrapSatKind::Wrap, op: BinOp::Add, a, b } => Some((AccOp::WrapAdd, *a, *b)),
        Op::BinWrapSat { kind: WrapSatKind::Wrap, op: BinOp::Mul, a, b } => Some((AccOp::WrapMul, *a, *b)),
        _ => None,
    }
}

struct Site {
    block: usize,
    /// accumulator form: (operator, the other operand `e`)
    acc: Option<(AccOp, Val)>,
}

fn use_counts(f: &Func) -> HashMap<Val, usize> {
    let mut m: HashMap<Val, usize> = HashMap::new();
    let mut buf = Vec::new();
    for b in &f.blocks {
        for i in &b.insts {
            buf.clear();
            i.op.uses(&mut buf);
            for v in &buf {
                *m.entry(*v).or_insert(0) += 1;
            }
        }
        match &b.term {
            Term::BrCond { cond, .. } => *m.entry(*cond).or_insert(0) += 1,
            Term::Switch { val, .. } => *m.entry(*val).or_insert(0) += 1,
            Term::Ret(Some(v)) => *m.entry(*v).or_insert(0) += 1,
            _ => {}
        }
    }
    m
}

/// May this function be rewritten at all?
fn eligible(f: &Func) -> bool {
    if f.inline_hint == Some(false) || f.constant_time || f.interrupt || !f.secret.is_empty() {
        return false;
    }
    // block 0 must not be a jump target (it is split below)
    for b in &f.blocks {
        if b.term.successors().contains(&0) {
            return false;
        }
    }
    // stack cells: only direct load/store addressing
    let mut cells: HashMap<Val, ()> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            match (&i.op, i.dst) {
                (Op::Alloca { .. }, Some(d)) => {
                    cells.insert(d, ());
                }
                (Op::Select { .. } | Op::Barrier { .. } | Op::SecureZero { .. } | Op::Asm { .. }, _) => return false,
                (Op::MmioLoad { .. } | Op::MmioStore { .. } | Op::ThreadSpawn { .. }, _) => return false,
                _ => {}
            }
        }
    }
    if !cells.is_empty() {
        let mut buf = Vec::new();
        for b in &f.blocks {
            for i in &b.insts {
                buf.clear();
                i.op.uses(&mut buf);
                match &i.op {
                    Op::Load { .. } => {}
                    Op::Store { val, .. } => {
                        if cells.contains_key(val) {
                            return false;
                        }
                    }
                    _ => {
                        if buf.iter().any(|v| cells.contains_key(v)) {
                            return false;
                        }
                    }
                }
            }
            match &b.term {
                Term::BrCond { cond: v, .. } | Term::Switch { val: v, .. } | Term::Ret(Some(v)) => {
                    if cells.contains_key(v) {
                        return false;
                    }
                }
                _ => {}
            }
        }
    }
    true
}

fn is_self_call(f: &Func, i: &Inst) -> bool {
    match &i.op {
        Op::Call { name, args } => name == &f.name && args.len() == f.params.len(),
        _ => false,
    }
}

fn find_sites(f: &Func) -> Vec<Site> {
    let uc = use_counts(f);
    let mut sites: Vec<Site> = Vec::new();
    let mut acc_kind: Option<AccOp> = None;
    for (bi, b) in f.blocks.iter().enumerate() {
        let rv = match &b.term {
            Term::Ret(r) => *r,
            _ => continue,
        };
        let n = b.insts.len();
        if n == 0 {
            continue;
        }
        // (a) plain tail call
        let last = &b.insts[n - 1];
        if is_self_call(f, last) && last.ty == f.ret {
            let plain = match (f.ret, rv, last.dst) {
                (FTy::Void, None, None) => true,
                (FTy::Void, _, _) => false,
                (_, Some(r), Some(d)) => r == d && uc.get(&d).copied().unwrap_or(0) == 1,
                _ => false,
            };
            if plain {
                sites.push(Site { block: bi, acc: None });
                continue;
            }
        }
        // (b) accumulator form
        if n >= 2 && is_int(f.ret) {
            let bin = &b.insts[n - 1];
            let call = &b.insts[n - 2];
            if let (Some(r), Some(c), Some(rr)) = (bin.dst, call.dst, rv) {
                if rr == r && is_self_call(f, call) && call.ty == f.ret && bin.ty == f.ret {
                    if let Some((k, x, y)) = acc_op_of(&bin.op) {
                        let e = if x == c && y != c {
                            Some(y)
                        } else if y == c && x != c {
                            Some(x)
                        } else {
                            None
                        };
                        if let Some(e) = e {
                            let ok_uses = uc.get(&c).copied().unwrap_or(0) == 1 && uc.get(&r).copied().unwrap_or(0) == 1;
                            let same_kind = acc_kind.map(|a| a == k).unwrap_or(true);
                            if ok_uses && same_kind {
                                acc_kind = Some(k);
                                sites.push(Site { block: bi, acc: Some((k, e)) });
                            }
                        }
                    }
                }
            }
        }
    }
    sites
}

/// Rewrites `f`. Yields true when something changed.
pub fn run(f: &mut Func) -> bool {
    if !f.blocks.iter().any(|b| b.insts.iter().any(|i| is_self_call(f, i))) {
        return false;
    }
    if !eligible(f) {
        return false;
    }
    let sites = find_sites(f);
    if sites.is_empty() {
        return false;
    }
    let acc_kind: Option<AccOp> = sites.iter().find_map(|s| s.acc.map(|(k, _)| k));
    let ret_ty = f.ret;

    // 1. split block 0: allocas stay, everything else moves to the new header
    let h = f.add_block();
    let hi = h as usize;
    {
        let old = std::mem::take(&mut f.blocks[0].insts);
        let (keep, moved): (Vec<Inst>, Vec<Inst>) = old.into_iter().partition(|i| matches!(i.op, Op::Alloca { .. }));
        f.blocks[0].insts = keep;
        f.blocks[hi].insts = moved;
        let t = std::mem::replace(&mut f.blocks[0].term, Term::Br(h));
        f.blocks[hi].term = t;
        for s in f.blocks[hi].term.successors() {
            crate::promote::rename_phi_pred_pub(f, s as usize, 0, h);
        }
    }
    let site_block = |s: &Site| if s.block == 0 { hi } else { s.block };

    // 2. which parameters change on the back edge?
    let nparams = f.params.len();
    let mut changing = vec![false; nparams];
    for s in &sites {
        let b = site_block(s);
        let n = f.blocks[b].insts.len();
        let call = if s.acc.is_some() { &f.blocks[b].insts[n - 2] } else { &f.blocks[b].insts[n - 1] };
        if let Op::Call { args, .. } = &call.op {
            for (i, a) in args.iter().enumerate() {
                if *a != i as Val {
                    changing[i] = true;
                }
            }
        }
    }
    let mut pmap: HashMap<Val, Val> = HashMap::new();
    for i in 0..nparams {
        if changing[i] {
            let ty = f.params[i];
            pmap.insert(i as Val, f.new_val_pub(ty));
        }
    }
    let acc_val: Option<Val> = acc_kind.map(|_| f.new_val_pub(ret_ty));
    crate::mem2reg::replace_uses(f, &pmap);

    // 3. cut the sites, collect the incoming values of the new phis
    let mut p_inc: Vec<Vec<(BlockId, Val)>> = vec![Vec::new(); nparams];
    for i in 0..nparams {
        if changing[i] {
            p_inc[i].push((0, i as Val));
        }
    }
    let mut a_inc: Vec<(BlockId, Val)> = Vec::new();
    let identity = acc_kind.map(|k| k.identity());
    if let (Some(_), Some(id)) = (acc_val, identity) {
        let c = f.new_val_pub(ret_ty);
        f.blocks[0].insts.push(Inst::new(Some(c), ret_ty, Op::Const(id)));
        a_inc.push((0, c));
    }
    let mut site_set: Vec<usize> = Vec::new();
    for s in &sites {
        let b = site_block(s);
        site_set.push(b);
        let n = f.blocks[b].insts.len();
        let (call, loc) = {
            let ci = if s.acc.is_some() { n - 2 } else { n - 1 };
            (f.blocks[b].insts[ci].op.clone(), f.blocks[b].insts[ci].loc)
        };
        let args = match call {
            Op::Call { args, .. } => args,
            _ => unreachable!(),
        };
        f.blocks[b].insts.truncate(if s.acc.is_some() { n - 2 } else { n - 1 });
        for i in 0..nparams {
            if changing[i] {
                p_inc[i].push((b as BlockId, args[i]));
            }
        }
        if let (Some(av), Some(k)) = (acc_val, acc_kind) {
            match s.acc {
                Some((_, e0)) => {
                    let e = pmap.get(&e0).copied().unwrap_or(e0);
                    let nv = f.new_val_pub(ret_ty);
                    f.blocks[b].insts.push(Inst::like(Some(nv), ret_ty, k.op(av, e), loc));
                    a_inc.push((b as BlockId, nv));
                }
                None => a_inc.push((b as BlockId, av)),
            }
        }
        f.blocks[b].term = Term::Br(h);
    }

    // 4. every other return gets the accumulator folded in
    if let (Some(av), Some(k)) = (acc_val, acc_kind) {
        for bi in 0..f.blocks.len() {
            if site_set.contains(&bi) {
                continue;
            }
            if let Term::Ret(Some(v)) = f.blocks[bi].term {
                let r = f.new_val_pub(ret_ty);
                let loc = f.blocks[bi].insts.last().map(|i| i.loc).unwrap_or(Loc::NONE);
                f.blocks[bi].insts.push(Inst::like(Some(r), ret_ty, k.op(av, v), loc));
                f.blocks[bi].term = Term::Ret(Some(r));
            }
        }
    }

    // 5. the phis at the top of the header
    let mut phis: Vec<Inst> = Vec::new();
    for i in 0..nparams {
        if changing[i] {
            let mut inc = std::mem::take(&mut p_inc[i]);
            inc.sort_by_key(|e| e.0);
            phis.push(Inst::new(Some(pmap[&(i as Val)]), f.params[i], Op::Phi { incoming: inc }));
        }
    }
    if let Some(av) = acc_val {
        a_inc.sort_by_key(|e| e.0);
        phis.push(Inst::new(Some(av), ret_ty, Op::Phi { incoming: a_inc }));
    }
    let rest = std::mem::take(&mut f.blocks[hi].insts);
    phis.extend(rest);
    f.blocks[hi].insts = phis;
    true
}
