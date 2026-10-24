// SPDX-License-Identifier: MPL-2.0
//! **GAPS — byte-assembled 32-bit loads (x86-64 only).**
//!
//! `b0 << 24 | b1 << 16 | b2 << 8 | b3` over four adjacent byte loads becomes
//! one 32-bit load plus `bswap`; the little-endian spelling becomes one plain
//! 32-bit load. Only when the four loads sit in one block with no memory write
//! between them and every intermediate value has exactly one reader.
//! `FIRN_NO_BSWAP=1` switches it off.

use crate::fir::{BinOp, FTy, Func, Inst, Op, UnOp, Val};
use crate::fx::HashMap;

type Defs = HashMap<Val, (usize, usize)>;
type Form = (Vec<Val>, i128);

struct Ctx<'a> {
    f: &'a Func,
    defs: Defs,
    uses: Vec<u32>,
    consts: HashMap<Val, i128>,
}

impl Ctx<'_> {
    fn op(&self, v: Val) -> Option<(&Inst, usize, usize)> {
        let (b, i) = *self.defs.get(&v)?;
        Some((&self.f.blocks[b].insts[i], b, i))
    }

    fn single_use(&self, v: Val) -> bool {
        self.uses.get(v as usize).copied() == Some(1)
    }

    fn form(&self, v: Val, depth: u32, out: &mut Form) -> bool {
        if let Some(c) = self.consts.get(&v) {
            out.1 += *c;
            return true;
        }
        if depth > 0 {
            if let Some((inst, _, _)) = self.op(v) {
                if let (Op::Bin(BinOp::Add, a, b), true) = (&inst.op, matches!(inst.ty, FTy::U64 | FTy::I64)) {
                    return self.form(*a, depth - 1, out) && self.form(*b, depth - 1, out);
                }
                if let Op::PtrAdd { base, off } = &inst.op {
                    return self.form(*base, depth - 1, out) && self.form(*off, depth - 1, out);
                }
            }
        }
        out.0.push(v);
        true
    }

    fn address_form(&self, v: Val) -> Option<Form> {
        let mut out: Form = (Vec::new(), 0);
        self.form(v, 6, &mut out);
        out.0.sort_unstable();
        Some(out)
    }

    fn flatten_or(&self, v: Val, leaves: &mut Vec<Val>, ors: &mut Vec<Val>) -> bool {
        if let Some((inst, _, _)) = self.op(v) {
            if let Op::Bin(BinOp::Or, a, b) = &inst.op {
                if inst.ty == FTy::U32 && self.single_use(v) {
                    ors.push(v);
                    return self.flatten_or(*a, leaves, ors) && self.flatten_or(*b, leaves, ors);
                }
            }
        }
        leaves.push(v);
        leaves.len() <= 4
    }

    fn byte_term(&self, v: Val) -> Option<(i128, Val, usize, usize)> {
        let (inst, _, _) = self.op(v)?;
        let (shift, cast_v) = match &inst.op {
            Op::Bin(BinOp::Shl, x, s) if inst.ty == FTy::U32 && self.single_use(v) => (*self.consts.get(s)?, *x),
            _ => (0, v),
        };
        let (cast, _, _) = self.op(cast_v)?;
        let load_v = match &cast.op {
            Op::Cast { src, from: FTy::U8 } if cast.ty == FTy::U32 && self.single_use(cast_v) => *src,
            _ => return None,
        };
        let (load, lb, li) = self.op(load_v)?;
        match &load.op {
            Op::Load { addr } if load.ty == FTy::U8 && self.single_use(load_v) => Some((shift, *addr, lb, li)),
            _ => None,
        }
    }
}

struct Edit {
    block: usize,
    root: usize,
    addr: Val,
    big_endian: bool,
}

fn match_root(cx: &Ctx, root_v: Val, rb: usize, ri: usize) -> Option<Edit> {
    let mut leaves = Vec::new();
    let mut ors = Vec::new();
    if !cx.flatten_or(root_v, &mut leaves, &mut ors) || leaves.len() != 4 {
        return None;
    }
    let mut terms: Vec<(i128, Val, Form, usize)> = Vec::new();
    let mut first = ri;
    for l in &leaves {
        let (shift, addr, lb, li) = cx.byte_term(*l)?;
        if lb != rb || li >= ri {
            return None;
        }
        first = first.min(li);
        terms.push((shift, addr, cx.address_form(addr)?, li));
    }
    for s in [0i128, 8, 16, 24] {
        if terms.iter().filter(|t| t.0 == s).count() != 1 {
            return None;
        }
    }
    let leaf_set = &terms[0].2 .0;
    if terms.iter().any(|t| &t.2 .0 != leaf_set) {
        return None;
    }
    let by_shift = |s: i128| terms.iter().find(|t| t.0 == s).unwrap();
    let c = |s: i128| by_shift(s).2 .1;
    let big_endian = c(24) + 1 == c(16) && c(16) + 1 == c(8) && c(8) + 1 == c(0);
    let little_endian = c(0) + 1 == c(8) && c(8) + 1 == c(16) && c(16) + 1 == c(24);
    if !big_endian && !little_endian {
        return None;
    }
    let low = if big_endian { by_shift(24) } else { by_shift(0) };
    let insts = &cx.f.blocks[rb].insts;
    if insts[first..ri].iter().any(|i| crate::opt::writes_memory(&i.op)) {
        return None;
    }
    Some(Edit { block: rb, root: ri, addr: low.1, big_endian })
}

pub(crate) fn run(f: &mut Func) -> usize {
    if std::env::var_os("FIRN_NO_BSWAP").is_some() {
        return 0;
    }
    let uses = crate::usecount::counts(f);
    let mut defs: Defs = HashMap::default();
    let mut consts: HashMap<Val, i128> = HashMap::default();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ii, i) in b.insts.iter().enumerate() {
            if let Some(d) = i.dst {
                defs.insert(d, (bi, ii));
                if let Op::Const(c) = &i.op {
                    consts.insert(d, *c);
                }
            }
        }
    }
    let mut edits: Vec<Edit> = Vec::new();
    {
        let cx = Ctx { f, defs, uses, consts };
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, i) in b.insts.iter().enumerate() {
                if let (Op::Bin(BinOp::Or, _, _), Some(d), FTy::U32) = (&i.op, i.dst, i.ty) {
                    if let Some(e) = match_root(&cx, d, bi, ii) {
                        edits.push(e);
                    }
                }
            }
        }
    }
    let n = edits.len();
    edits.sort_by(|a, b| (b.block, b.root).cmp(&(a.block, a.root)));
    for e in edits {
        let loc = f.blocks[e.block].insts[e.root].loc;
        if e.big_endian {
            let t = f.new_val_pub(FTy::U32);
            f.blocks[e.block].insts[e.root].op = Op::Un(UnOp::Bswap, t);
            let load = Inst::like(Some(t), FTy::U32, Op::Load { addr: e.addr }, loc);
            f.blocks[e.block].insts.insert(e.root, load);
        } else {
            f.blocks[e.block].insts[e.root].op = Op::Load { addr: e.addr };
        }
    }
    n
}
