// SPDX-License-Identifier: MPL-2.0
use crate::fir::{BinOp, BlockId, CmpOp, FTy, Func, Inst, Module, Op, Term, Val};
use crate::fx::{HashMap, HashSet};
use crate::wasm_cfg;

const SWITCH_AFTER: i128 = 128;
const CHUNK_LEAF: i128 = 4096;
const CHUNK_OUTER: i128 = 8;
const MAX_LOOP_INSTS: usize = 1500;
const MAX_LIVE_BITS: usize = 400_000_000;
const SLOT_BYTES: u64 = 16;
const MAX_STATE: usize = 40;

struct LoopShape {
    header: BlockId,
    body: Vec<BlockId>,
    in_body: Vec<bool>,
    leaf: bool,
}

struct Plan {
    shape: LoopShape,
    exits: Vec<BlockId>,
    params: Vec<Val>,
    remat: Vec<Val>,
    results: Vec<Val>,
    name: String,
}

fn tune() -> (i128, i128, i128) {
    if std::env::var_os("FIRN_WASM_OUTLINE_STRESS").is_some() {
        (1, 2, 1)
    } else {
        (SWITCH_AFTER, CHUNK_LEAF, CHUNK_OUTER)
    }
}

fn bits_set(s: &mut [u64], v: Val) {
    s[v as usize / 64] |= 1u64 << (v % 64);
}

fn bits_clear(s: &mut [u64], v: Val) {
    s[v as usize / 64] &= !(1u64 << (v % 64));
}

fn bits_has(s: &[u64], v: Val) -> bool {
    s[v as usize / 64] >> (v % 64) & 1 == 1
}

fn bits_iter(s: &[u64]) -> Vec<Val> {
    let mut out = Vec::new();
    for (w, word) in s.iter().enumerate() {
        let mut x = *word;
        while x != 0 {
            let bit = x.trailing_zeros();
            x &= x - 1;
            out.push((w as u32) * 64 + bit);
        }
    }
    out
}

fn term_uses(t: &Term) -> Option<Val> {
    match t {
        Term::BrCond { cond, .. } => Some(*cond),
        Term::Switch { val, .. } => Some(*val),
        Term::Ret(Some(v)) => Some(*v),
        _ => None,
    }
}

fn find_loops(f: &Func) -> Option<Vec<LoopShape>> {
    let cfg = wasm_cfg::analyse(f);
    if !cfg.reducible {
        return None;
    }
    let n = f.blocks.len();
    let mut preds: Vec<Vec<BlockId>> = vec![Vec::new(); n];
    for &b in &cfg.order {
        f.blocks[b as usize].term.for_each_succ(|s| preds[s as usize].push(b));
    }
    let mut out: Vec<LoopShape> = Vec::new();
    for &h in &cfg.order {
        if !cfg.loop_header[h as usize] {
            continue;
        }
        let mut in_body = vec![false; n];
        in_body[h as usize] = true;
        let mut stack: Vec<BlockId> = Vec::new();
        for &p in &preds[h as usize] {
            if cfg.rpo[p as usize] != wasm_cfg::NONE && cfg.rpo[p as usize] >= cfg.rpo[h as usize] && !in_body[p as usize] {
                in_body[p as usize] = true;
                stack.push(p);
            }
        }
        while let Some(b) = stack.pop() {
            for &p in &preds[b as usize] {
                if cfg.rpo[p as usize] != wasm_cfg::NONE && !in_body[p as usize] {
                    in_body[p as usize] = true;
                    stack.push(p);
                }
            }
        }
        let body: Vec<BlockId> = (0..n as u32).filter(|b| in_body[*b as usize]).collect();
        out.push(LoopShape { header: h, body, in_body, leaf: true });
    }
    let headers: Vec<BlockId> = out.iter().map(|l| l.header).collect();
    for l in out.iter_mut() {
        l.leaf = !headers.iter().any(|h| *h != l.header && l.in_body[*h as usize]);
    }
    Some(out)
}

fn split_returns(f: &mut Func) -> bool {
    let loops = match find_loops(f) {
        Some(l) => l,
        None => return false,
    };
    let n = f.blocks.len();
    let mut in_any = vec![false; n];
    for l in &loops {
        for &b in &l.body {
            in_any[b as usize] = true;
        }
    }
    let mut changed = false;
    for b in 0..n {
        if in_any[b] && matches!(f.blocks[b].term, Term::Ret(_)) {
            let nb = f.add_block();
            let t = std::mem::replace(&mut f.blocks[b].term, Term::Br(nb));
            f.blocks[nb as usize].term = t;
            changed = true;
        }
    }
    changed
}

fn loop_allowed(f: &Func, l: &LoopShape) -> bool {
    if l.header == 0 {
        return false;
    }
    let mut count = 0usize;
    for &b in &l.body {
        for i in &f.blocks[b as usize].insts {
            count += 1;
            match &i.op {
                Op::Alloca { .. } | Op::Asm { .. } | Op::GcAddr { .. } | Op::ThreadSpawn { .. } | Op::ThreadSelf | Op::Phi { .. } => return false,
                _ => {}
            }
        }
    }
    count <= MAX_LOOP_INSTS
}

fn liveness(f: &Func) -> Option<Vec<Vec<u64>>> {
    let nv = f.val_types.len();
    let nb = f.blocks.len();
    if nv.saturating_mul(nb) > MAX_LIVE_BITS {
        return None;
    }
    let words = nv.div_ceil(64).max(1);
    let mut live_in: Vec<Vec<u64>> = vec![vec![0u64; words]; nb];
    let mut ops: Vec<Val> = Vec::new();
    let mut changed = true;
    while changed {
        changed = false;
        for b in (0..nb).rev() {
            let blk = &f.blocks[b];
            let mut live = vec![0u64; words];
            blk.term.for_each_succ(|s| {
                let (src, dst) = (&live_in[s as usize], &mut live);
                for k in 0..words {
                    dst[k] |= src[k];
                }
            });
            if let Some(v) = term_uses(&blk.term) {
                bits_set(&mut live, v);
            }
            for i in blk.insts.iter().rev() {
                if let Some(d) = i.dst {
                    bits_clear(&mut live, d);
                }
                ops.clear();
                i.op.uses(&mut ops);
                for v in &ops {
                    bits_set(&mut live, *v);
                }
            }
            if live != live_in[b] {
                live_in[b] = live;
                changed = true;
            }
        }
    }
    Some(live_in)
}

fn retarget(t: &mut Term, from: BlockId, to: BlockId) {
    match t {
        Term::Br(b) => {
            if *b == from {
                *b = to;
            }
        }
        Term::BrCond { then_bb, else_bb, .. } => {
            if *then_bb == from {
                *then_bb = to;
            }
            if *else_bb == from {
                *else_bb = to;
            }
        }
        Term::Switch { cases, default, .. } => {
            for (_, b) in cases.iter_mut() {
                if *b == from {
                    *b = to;
                }
            }
            if *default == from {
                *default = to;
            }
        }
        _ => {}
    }
}

fn plan_loops(f: &Func, shapes: Vec<LoopShape>, budget: &mut usize) -> Vec<Plan> {
    let live_in = match liveness(f) {
        Some(l) => l,
        None => return Vec::new(),
    };
    let nv = f.val_types.len();
    let words = nv.div_ceil(64).max(1);
    let mut ndef = vec![0u32; nv];
    let mut def_block: Vec<BlockId> = vec![0; nv];
    let mut def_inst: HashMap<Val, Inst> = HashMap::default();
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                ndef[d as usize] += 1;
                def_block[d as usize] = b.id;
                if matches!(i.op, Op::Const(_) | Op::GlobalAddr { .. } | Op::FnRef { .. } | Op::VtabAddr { .. }) {
                    def_inst.insert(d, i.clone());
                }
            }
        }
    }
    let mut plans: Vec<Plan> = Vec::new();
    for shape in shapes {
        if !loop_allowed(f, &shape) {
            continue;
        }
        let size: usize = shape.body.iter().map(|b| f.blocks[*b as usize].insts.len() + 1).sum();
        if size > *budget {
            continue;
        }
        let mut exits: Vec<BlockId> = Vec::new();
        let mut defined = vec![0u64; words];
        for &b in &shape.body {
            let blk = &f.blocks[b as usize];
            for i in &blk.insts {
                if let Some(d) = i.dst {
                    bits_set(&mut defined, d);
                }
            }
            blk.term.for_each_succ(|s| {
                if !shape.in_body[s as usize] && !exits.contains(&s) {
                    exits.push(s);
                }
            });
        }
        let h = shape.header as usize;
        let mut params: Vec<Val> = Vec::new();
        let mut remat: Vec<Val> = Vec::new();
        for v in bits_iter(&live_in[h]) {
            let rematable = ndef[v as usize] == 1
                && def_inst.contains_key(&v)
                && !shape.in_body[def_block[v as usize] as usize]
                && v as usize >= f.params.len();
            if rematable {
                remat.push(v);
            } else {
                params.push(v);
            }
        }
        let mut want = live_in[h].clone();
        for t in &exits {
            for k in 0..words {
                want[k] |= live_in[*t as usize][k];
            }
        }
        let results: Vec<Val> = bits_iter(&want).into_iter().filter(|v| bits_has(&defined, *v)).collect();
        if params.len() > MAX_STATE || results.len() > MAX_STATE {
            continue;
        }
        *budget -= size;
        let name = format!("{}__loop{}", f.name, shape.header);
        plans.push(Plan { shape, exits, params, remat, results, name });
    }
    let def_snapshot = def_inst;
    for p in plans.iter_mut() {
        p.remat.retain(|v| def_snapshot.contains_key(v));
    }
    plans
}

struct Builder<'a> {
    f: &'a Func,
    g: Func,
    map: HashMap<Val, Val>,
}

impl<'a> Builder<'a> {
    fn val(&mut self, v: Val) -> Val {
        if let Some(n) = self.map.get(&v) {
            return *n;
        }
        let n = self.g.new_val_pub(self.f.val_ty(v));
        self.map.insert(v, n);
        n
    }

    fn store_results(&mut self, blk: BlockId, slot: Val, results: &[Val]) {
        for (k, v) in results.iter().enumerate() {
            let addr = if k == 0 {
                slot
            } else {
                let off = self.g.push(blk, FTy::I64, Op::Const((k as u64 * SLOT_BYTES) as i128));
                self.g.push(blk, FTy::Ptr, Op::PtrAdd { base: slot, off })
            };
            let nv = self.val(*v);
            let ty = self.f.val_ty(*v);
            self.g.push_void(blk, ty, Op::Store { addr, val: nv });
        }
    }

    fn finish(&mut self, blk: BlockId, id: i128) {
        let c = self.g.push(blk, FTy::I32, Op::Const(id));
        self.g.set_term(blk, Term::Ret(Some(c)));
    }
}

fn build_outlined(f: &Func, p: &Plan) -> Func {
    let mut ptys: Vec<FTy> = p.params.iter().map(|v| f.val_ty(*v)).collect();
    let has_slot = !p.results.is_empty();
    if has_slot {
        ptys.push(FTy::Ptr);
    }
    let mut g = Func::new(&p.name, ptys, FTy::I32);
    g.loc_stamp = f.loc_stamp;
    let mut map: HashMap<Val, Val> = HashMap::default();
    for (i, v) in p.params.iter().enumerate() {
        map.insert(*v, i as Val);
    }
    let slot = p.params.len() as Val;
    let mut bd = Builder { f, g, map };
    let mut remat_defs: HashMap<Val, Inst> = HashMap::default();
    for b in &f.blocks {
        for i in &b.insts {
            if let Some(d) = i.dst {
                if p.remat.contains(&d) {
                    remat_defs.insert(d, i.clone());
                }
            }
        }
    }
    for v in &p.remat {
        let nv = bd.val(*v);
        let mut inst = remat_defs[v].clone();
        inst.dst = Some(nv);
        bd.g.blocks[0].insts.push(inst);
    }
    let cnt = bd.g.push(0, FTy::I32, Op::Const(0));
    let mut bm: HashMap<BlockId, BlockId> = HashMap::default();
    for &b in &p.shape.body {
        let nb = bd.g.add_block();
        bm.insert(b, nb);
    }
    let chk = bd.g.add_block();
    let yield_blk = bd.g.add_block();
    let exit_blks: Vec<BlockId> = p.exits.iter().map(|_| bd.g.add_block()).collect();
    let h = p.shape.header;
    bd.g.set_term(0, Term::Br(bm[&h]));
    for &b in &p.shape.body {
        let nb = bm[&b];
        let src = &f.blocks[b as usize];
        for i in &src.insts {
            let mut op = i.op.clone();
            crate::promote::for_each_use_mut(&mut op, |v| *v = bd.val(*v));
            let dst = i.dst.map(|d| bd.val(d));
            bd.g.blocks[nb as usize].insts.push(Inst { dst, ty: i.ty, op, loc: i.loc });
        }
        let mut term = src.term.clone();
        match &mut term {
            Term::BrCond { cond, .. } => *cond = bd.val(*cond),
            Term::Switch { val, .. } => *val = bd.val(*val),
            _ => {}
        }
        let tgt = |t: BlockId| -> BlockId {
            if t == h {
                chk
            } else if let Some(x) = bm.get(&t) {
                *x
            } else {
                exit_blks[p.exits.iter().position(|e| *e == t).unwrap()]
            }
        };
        let term = match term {
            Term::Br(t) => Term::Br(tgt(t)),
            Term::BrCond { cond, then_bb, else_bb } => Term::BrCond { cond, then_bb: tgt(then_bb), else_bb: tgt(else_bb) },
            Term::Switch { val, ty, cases, default } => {
                Term::Switch { val, ty, cases: cases.into_iter().map(|(c, t)| (c, tgt(t))).collect(), default: tgt(default) }
            }
            other => other,
        };
        bd.g.blocks[nb as usize].term = term;
    }
    let chunk = if p.shape.leaf { tune().1 } else { tune().2 };
    let one = bd.g.push(chk, FTy::I32, Op::Const(1));
    bd.g.blocks[chk as usize].insts.push(Inst::new(Some(cnt), FTy::I32, Op::Bin(BinOp::Add, cnt, one)));
    let lim = bd.g.push(chk, FTy::I32, Op::Const(chunk));
    let lt = bd.g.push(chk, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I32, a: cnt, b: lim });
    bd.g.set_term(chk, Term::BrCond { cond: lt, then_bb: bm[&h], else_bb: yield_blk });
    if has_slot {
        bd.store_results(yield_blk, slot, &p.results);
    }
    bd.finish(yield_blk, 0);
    for (j, eb) in exit_blks.iter().enumerate() {
        if has_slot {
            bd.store_results(*eb, slot, &p.results);
        }
        bd.finish(*eb, (j + 1) as i128);
    }
    bd.g
}

fn instrument(f: &mut Func, plans: &[Plan]) {
    let slots: Vec<Option<Val>> = plans
        .iter()
        .map(|p| if p.results.is_empty() { None } else { Some(f.alloca(p.results.len() as u64 * SLOT_BYTES, 16)) })
        .collect();
    let lead = f.blocks[0].insts.iter().take_while(|i| matches!(i.op, Op::Alloca { .. })).count();
    let mut inits: Vec<Inst> = Vec::new();
    let mut counters: Vec<Val> = Vec::new();
    for _ in plans {
        let c = f.new_val_pub(FTy::I32);
        counters.push(c);
        inits.push(Inst::new(Some(c), FTy::I32, Op::Const(0)));
    }
    for (k, i) in inits.into_iter().enumerate() {
        f.blocks[0].insts.insert(lead + k, i);
    }
    for (k, p) in plans.iter().enumerate() {
        let h = p.shape.header;
        let cnt = counters[k];
        let chk = f.add_block();
        let drv = f.add_block();
        for &u in &p.shape.body {
            retarget(&mut f.blocks[u as usize].term, h, chk);
        }
        let one = f.push(chk, FTy::I32, Op::Const(1));
        f.blocks[chk as usize].insts.push(Inst::new(Some(cnt), FTy::I32, Op::Bin(BinOp::Add, cnt, one)));
        let lim = f.push(chk, FTy::I32, Op::Const(tune().0));
        let lt = f.push(chk, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I32, a: cnt, b: lim });
        f.set_term(chk, Term::BrCond { cond: lt, then_bb: h, else_bb: drv });
        let mut args: Vec<Val> = p.params.clone();
        if let Some(s) = slots[k] {
            args.push(s);
        }
        let r = f.push(drv, FTy::I32, Op::Call { name: p.name.clone(), args });
        if let Some(s) = slots[k] {
            for (j, v) in p.results.iter().enumerate() {
                let addr = if j == 0 {
                    s
                } else {
                    let off = f.push(drv, FTy::I64, Op::Const((j as u64 * SLOT_BYTES) as i128));
                    f.push(drv, FTy::Ptr, Op::PtrAdd { base: s, off })
                };
                let ty = f.val_ty(*v);
                f.blocks[drv as usize].insts.push(Inst::new(Some(*v), ty, Op::Load { addr }));
            }
        }
        if p.exits.is_empty() {
            f.set_term(drv, Term::Br(drv));
        } else {
            let m = p.exits.len();
            let mut cases: Vec<(i128, BlockId)> = vec![(0, drv)];
            for j in 1..m {
                cases.push((j as i128, p.exits[j - 1]));
            }
            f.set_term(drv, Term::Switch { val: r, ty: FTy::I32, cases, default: p.exits[m - 1] });
        }
    }
}

fn recursive_set(m: &Module) -> HashSet<String> {
    let index: HashMap<&str, usize> = m.funcs.iter().enumerate().map(|(i, f)| (f.name.as_str(), i)).collect();
    let n = m.funcs.len();
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, f) in m.funcs.iter().enumerate() {
        for b in &f.blocks {
            for inst in &b.insts {
                if let Op::Call { name, .. } = &inst.op {
                    if let Some(j) = index.get(name.as_str()) {
                        succ[i].push(*j);
                    }
                }
            }
        }
    }
    let mut idx = vec![usize::MAX; n];
    let mut low = vec![0usize; n];
    let mut on = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut next = 0usize;
    let mut out: HashSet<String> = HashSet::default();
    for root in 0..n {
        if idx[root] != usize::MAX {
            continue;
        }
        let mut work: Vec<(usize, usize)> = vec![(root, 0)];
        idx[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on[root] = true;
        while let Some(&(v, k)) = work.last() {
            if k < succ[v].len() {
                work.last_mut().unwrap().1 += 1;
                let w = succ[v][k];
                if idx[w] == usize::MAX {
                    idx[w] = next;
                    low[w] = next;
                    next += 1;
                    stack.push(w);
                    on[w] = true;
                    work.push((w, 0));
                } else if on[w] {
                    low[v] = low[v].min(idx[w]);
                }
            } else {
                work.pop();
                if let Some(&(u, _)) = work.last() {
                    low[u] = low[u].min(low[v]);
                }
                if low[v] == idx[v] {
                    let mut comp: Vec<usize> = Vec::new();
                    while let Some(w) = stack.pop() {
                        on[w] = false;
                        comp.push(w);
                        if w == v {
                            break;
                        }
                    }
                    if comp.len() > 1 || succ[v].contains(&v) {
                        for w in comp {
                            out.insert(m.funcs[w].name.clone());
                        }
                    }
                }
            }
        }
    }
    out
}

fn run_once_set(m: &Module, rec: &HashSet<String>) -> HashSet<String> {
    let index: HashMap<&str, usize> = m.funcs.iter().enumerate().map(|(i, f)| (f.name.as_str(), i)).collect();
    let n = m.funcs.len();
    let mut sites = vec![0u32; n];
    let mut bad = vec![false; n];
    let mut callers: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, f) in m.funcs.iter().enumerate() {
        let in_loop: Vec<bool> = match find_loops(f) {
            Some(ls) => {
                let mut mask = vec![false; f.blocks.len()];
                for l in &ls {
                    for &b in &l.body {
                        mask[b as usize] = true;
                    }
                }
                mask
            }
            None => vec![true; f.blocks.len()],
        };
        for b in &f.blocks {
            for inst in &b.insts {
                match &inst.op {
                    Op::Call { name, .. } => {
                        if let Some(j) = index.get(name.as_str()) {
                            sites[*j] += 1;
                            callers[*j].push(i);
                            if in_loop[b.id as usize] {
                                bad[*j] = true;
                            }
                        }
                    }
                    Op::FnRef { name } => {
                        if let Some(j) = index.get(name.as_str()) {
                            bad[*j] = true;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let mut once: Vec<bool> = (0..n)
        .map(|i| {
            let f = &m.funcs[i];
            f.name == "main" || (sites[i] >= 1 && sites[i] <= 2 && !bad[i] && !rec.contains(&f.name))
        })
        .collect();
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..n {
            if once[i] && m.funcs[i].name != "main" && callers[i].iter().any(|c| !once[*c]) {
                once[i] = false;
                changed = true;
            }
        }
    }
    (0..n).filter(|i| once[*i]).map(|i| m.funcs[i].name.clone()).collect()
}

pub fn run(m: &Module) -> Option<Module> {
    if std::env::var_os("FIRN_WASM_NO_OUTLINE").is_some() {
        return None;
    }
    let mut budget = m.inst_count() / 8 + 1500;
    let mut funcs: Vec<Func> = Vec::with_capacity(m.funcs.len());
    let mut extra: Vec<Func> = Vec::new();
    let mut any = false;
    let rec = recursive_set(m);
    let once = run_once_set(m, &rec);
    for f in &m.funcs {
        if !once.contains(&f.name) || rec.contains(&f.name) || f.constant_time || f.interrupt || !f.secret.is_empty() || f.name.contains("__loop") {
            funcs.push(f.clone());
            continue;
        }
        let mut work = f.clone();
        if split_returns(&mut work) {
            any = true;
        }
        let shapes = match find_loops(&work) {
            Some(s) if !s.is_empty() => s,
            _ => {
                funcs.push(work);
                continue;
            }
        };
        let plans = plan_loops(&work, shapes, &mut budget);
        if plans.is_empty() {
            funcs.push(work);
            continue;
        }
        for p in &plans {
            extra.push(build_outlined(&work, p));
        }
        instrument(&mut work, &plans);
        any = true;
        funcs.push(work);
    }
    if !any {
        return None;
    }
    if std::env::var_os("FIRN_WASM_OUTLINE_STATS").is_some() {
        let copied: usize = extra.iter().map(|g| g.inst_count()).sum();
        eprintln!("outline: {} loops outlined, {} instructions copied, module had {}", extra.len(), copied, m.inst_count());
    }
    funcs.extend(extra);
    Some(Module { funcs })
}
