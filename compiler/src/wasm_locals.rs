// SPDX-License-Identifier: MPL-2.0
//! **Round OPT-GENERAL -- the locals of a WebAssembly function, packed by
//! liveness.**
//!
//! INTERFACE:
//!   `pub fn pack(params: &[VT], locals: Vec<VT>, body: &mut Vec<Ins>) -> Vec<VT>`
//!
//! `codegen_wasm.rs` gives every FIR value a local of its own. An optimizing
//! engine (TurboFan) does not care: it builds SSA from the locals anyway. A
//! one-pass engine (V8's Liftoff, Certus' AOT) does care -- every local is a
//! stack slot or a register it has to track, and a function with 300 locals
//! of which 20 are alive at any time is 280 slots of waste.
//!
//! This pass works on the finished instruction list, after everything else,
//! so it sees exactly what the engine will see:
//!
//!  1. basic blocks of the structured code (`block`/`loop`/`if`/`else`/`end`,
//!     the branches, `return`, `unreachable`),
//!  2. liveness of every local over them (`local.get` uses, `local.set` and
//!     `local.tee` define),
//!  3. an interference graph: a local defined while another one is alive
//!     cannot share its slot,
//!  4. a greedy colouring per value type, in order of first appearance.
//!     Parameters keep their index and may be reused once they are dead.
//!
//! A local that may be READ BEFORE IT IS WRITTEN on some path from the entry
//! keeps a slot of its own: it relies on WebAssembly's zero initialisation,
//! and a shared slot could hand it the value of another local.
//!
//! `FIRN_WASM_NO_PACK=1` switches the pass off (for comparisons).

use crate::wasm_enc::{Ins, VT};

fn words(n: usize) -> usize {
    n.div_ceil(64).max(1)
}

#[derive(Clone)]
struct Bits(Vec<u64>);

impl Bits {
    fn new(n: usize) -> Bits {
        Bits(vec![0; words(n)])
    }
    fn set(&mut self, i: usize) {
        self.0[i / 64] |= 1u64 << (i % 64);
    }
    fn clr(&mut self, i: usize) {
        self.0[i / 64] &= !(1u64 << (i % 64));
    }
    fn get(&self, i: usize) -> bool {
        self.0[i / 64] & (1u64 << (i % 64)) != 0
    }
    fn or(&mut self, o: &Bits) -> bool {
        let mut ch = false;
        for (a, b) in self.0.iter_mut().zip(o.0.iter()) {
            let n = *a | *b;
            if n != *a {
                *a = n;
                ch = true;
            }
        }
        ch
    }
    fn ones(&self) -> Vec<usize> {
        let mut v = Vec::new();
        for (w, x) in self.0.iter().enumerate() {
            let mut x = *x;
            while x != 0 {
                let b = x.trailing_zeros() as usize;
                x &= x - 1;
                v.push(w * 64 + b);
            }
        }
        v
    }
}

/// Successors of every instruction position (`n` = the end of the body).
fn successors(body: &[Ins]) -> Option<Vec<Vec<usize>>> {
    let n = body.len();
    // match the structure: for every opener its `end` (and `else`)
    let mut end_of = vec![usize::MAX; n];
    let mut else_of = vec![usize::MAX; n];
    let mut stack: Vec<usize> = Vec::new();
    for (p, i) in body.iter().enumerate() {
        match i {
            Ins::Block | Ins::Loop | Ins::If => stack.push(p),
            Ins::Else => {
                let o = *stack.last()?;
                else_of[o] = p;
                end_of[p] = usize::MAX;
            }
            Ins::End => {
                if let Some(o) = stack.pop() {
                    end_of[o] = p;
                    if else_of[o] != usize::MAX {
                        let e = else_of[o];
                        end_of[e] = p;
                    }
                }
                // an `end` without an opener closes the function body
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        return None;
    }
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    // labels: for each position the stack of enclosing branch targets
    let mut ctl: Vec<(usize, bool)> = Vec::new(); // (opener, is_loop)
    let target = |ctl: &Vec<(usize, bool)>, d: u32, end_of: &Vec<usize>| -> Option<usize> {
        let k = ctl.len().checked_sub(1 + d as usize)?;
        let (o, lp) = ctl[k];
        Some(if lp { o } else { end_of[o] })
    };
    for (p, i) in body.iter().enumerate() {
        let next = p + 1;
        match i {
            Ins::Block | Ins::Loop => {
                ctl.push((p, matches!(i, Ins::Loop)));
                succ[p].push(next);
            }
            Ins::If => {
                ctl.push((p, false));
                succ[p].push(next);
                let alt = if else_of[p] != usize::MAX { else_of[p] + 1 } else { end_of[p] };
                succ[p].push(alt);
            }
            Ins::Else => {
                // end of the then part: jump to the `end`
                succ[p].push(end_of[p]);
            }
            Ins::End => {
                ctl.pop();
                succ[p].push(next);
            }
            Ins::Br(d) => succ[p].push(target(&ctl, *d, &end_of)?),
            Ins::BrIf(d) => {
                succ[p].push(next);
                succ[p].push(target(&ctl, *d, &end_of)?);
            }
            Ins::BrTable(ls, d) => {
                for l in ls {
                    succ[p].push(target(&ctl, *l, &end_of)?);
                }
                succ[p].push(target(&ctl, *d, &end_of)?);
            }
            Ins::Return | Ins::Unreachable => {}
            _ => succ[p].push(next),
        }
    }
    for s in succ.iter_mut() {
        s.sort_unstable();
        s.dedup();
    }
    Some(succ)
}

/// Packs the locals. Yields the new local declarations (after the
/// parameters) and rewrites the indices in `body`.
pub fn pack(params: &[VT], locals: Vec<VT>, body: &mut Vec<Ins>) -> Vec<VT> {
    if std::env::var_os("FIRN_WASM_NO_PACK").is_some() || locals.len() < 2 {
        return locals;
    }
    let np = params.len();
    let nl = np + locals.len();
    let ty_of = |l: usize| -> VT { if l < np { params[l] } else { locals[l - np] } };
    let succ = match successors(body) {
        Some(s) => s,
        None => return locals,
    };
    let n = body.len();
    // basic blocks: leaders
    let mut leader = vec![false; n + 1];
    leader[0] = true;
    for p in 0..n {
        let straight = succ[p].len() == 1 && succ[p][0] == p + 1;
        if !straight {
            for &s in &succ[p] {
                if s <= n {
                    leader[s] = true;
                }
            }
            if p + 1 <= n {
                leader[p + 1] = true;
            }
        }
        if matches!(body[p], Ins::Loop | Ins::End | Ins::Block | Ins::If | Ins::Else) {
            leader[p] = true;
            if p + 1 <= n {
                leader[p + 1] = true;
            }
        }
    }
    let starts: Vec<usize> = (0..n).filter(|p| leader[*p]).collect();
    let nb = starts.len();
    let mut block_of = vec![0usize; n + 1];
    for (k, &s) in starts.iter().enumerate() {
        let e = if k + 1 < nb { starts[k + 1] } else { n };
        for p in s..e {
            block_of[p] = k;
        }
    }
    let bend = |k: usize| if k + 1 < nb { starts[k + 1] } else { n };
    // block successors (n = the exit)
    let mut bsucc: Vec<Vec<usize>> = vec![Vec::new(); nb];
    for k in 0..nb {
        let last = bend(k) - 1;
        for &s in &succ[last] {
            if s < n {
                bsucc[k].push(block_of[s]);
            }
        }
        bsucc[k].sort_unstable();
        bsucc[k].dedup();
    }
    // use/def per block
    let mut uses: Vec<Bits> = vec![Bits::new(nl); nb];
    let mut defs: Vec<Bits> = vec![Bits::new(nl); nb];
    for k in 0..nb {
        for p in starts[k]..bend(k) {
            match body[p] {
                Ins::LocalGet(l) => {
                    let l = l as usize;
                    if l < nl && !defs[k].get(l) {
                        uses[k].set(l);
                    }
                }
                Ins::LocalSet(l) | Ins::LocalTee(l) => {
                    let l = l as usize;
                    if l < nl {
                        defs[k].set(l);
                    }
                }
                _ => {}
            }
        }
    }
    // liveness
    let mut live_in: Vec<Bits> = uses.clone();
    let mut live_out: Vec<Bits> = vec![Bits::new(nl); nb];
    let mut changed = true;
    let mut rounds = 0;
    while changed && rounds < 10_000 {
        changed = false;
        rounds += 1;
        for k in (0..nb).rev() {
            let mut out = Bits::new(nl);
            for &s in &bsucc[k] {
                out.or(&live_in[s]);
            }
            if out.0 != live_out[k].0 {
                live_out[k] = out.clone();
            }
            // in = use | (out - def)
            let mut inb = uses[k].clone();
            for (w, x) in inb.0.iter_mut().enumerate() {
                *x |= out.0[w] & !defs[k].0[w];
            }
            if inb.0 != live_in[k].0 {
                live_in[k] = inb;
                changed = true;
            }
        }
    }
    if changed {
        return locals; // did not settle; leave the function as it is
    }
    // interference
    let mut adj: Vec<Bits> = vec![Bits::new(nl); nl];
    for k in 0..nb {
        let mut live = live_out[k].clone();
        for p in (starts[k]..bend(k)).rev() {
            match body[p] {
                Ins::LocalGet(l) => {
                    let l = l as usize;
                    if l < nl {
                        live.set(l);
                    }
                }
                Ins::LocalSet(l) | Ins::LocalTee(l) => {
                    let l = l as usize;
                    if l < nl {
                        for m in live.ones() {
                            if m != l {
                                adj[l].set(m);
                                adj[m].set(l);
                            }
                        }
                        live.clr(l);
                    }
                }
                _ => {}
            }
        }
    }
    // parameters are defined at the entry: they interfere with every local
    // alive there, and with each other
    let entry = &live_in[0];
    for pnum in 0..np {
        for m in entry.ones() {
            if m != pnum {
                adj[pnum].set(m);
                adj[m].set(pnum);
            }
        }
        for q in 0..np {
            if q != pnum {
                adj[pnum].set(q);
            }
        }
    }
    // locals read before written keep a slot of their own
    let pinned: Vec<bool> = (0..nl).map(|l| l >= np && entry.get(l)).collect();
    // colouring: slot -> type; params keep their index
    let mut color = vec![usize::MAX; nl];
    let mut slot_ty: Vec<VT> = Vec::new();
    for pnum in 0..np {
        color[pnum] = pnum;
        slot_ty.push(params[pnum]);
    }
    // order of first appearance
    let mut order: Vec<usize> = Vec::new();
    let mut seen = vec![false; nl];
    for i in body.iter() {
        if let Ins::LocalGet(l) | Ins::LocalSet(l) | Ins::LocalTee(l) = i {
            let l = *l as usize;
            if l < nl && l >= np && !seen[l] {
                seen[l] = true;
                order.push(l);
            }
        }
    }
    // members per slot, to test interference against the whole slot
    let mut members: Vec<Vec<usize>> = (0..np).map(|p| vec![p]).collect();
    let mut slot_pinned: Vec<bool> = vec![false; np];
    for &l in &order {
        let t = ty_of(l);
        let mut chosen = usize::MAX;
        if !pinned[l] {
            for s in 0..slot_ty.len() {
                if slot_ty[s] != t || slot_pinned[s] {
                    continue;
                }
                if members[s].iter().all(|m| !adj[l].get(*m)) {
                    chosen = s;
                    break;
                }
            }
        }
        if chosen == usize::MAX {
            chosen = slot_ty.len();
            slot_ty.push(t);
            members.push(Vec::new());
            slot_pinned.push(pinned[l]);
        }
        members[chosen].push(l);
        color[l] = chosen;
    }
    // locals never mentioned keep nothing
    for i in body.iter_mut() {
        match i {
            Ins::LocalGet(l) | Ins::LocalSet(l) | Ins::LocalTee(l) => {
                let lu = *l as usize;
                if lu < nl && color[lu] != usize::MAX {
                    *l = color[lu] as u32;
                }
            }
            _ => {}
        }
    }
    slot_ty[np..].to_vec()
}
