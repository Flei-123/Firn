//! Optimizer on FIR: constant folding and removal of dead code.
//!
//! INTERFACE (fixed):
//!   `pub fn optimize(m: &mut fir::Module) -> OptStats`
//! Rule: the optimization may NEVER change program behaviour. The test suite
//! runs every program with and without `--no-opt` and compares.
//!
//! Transformations carried out (all behaviour preserving):
//!  1. **Constant folding** over `Op::Bin`, `Op::Cmp`, `Op::Un` and
//!     `Op::Cast` when all operands are `Op::Const`. The result gets
//!     normalized with `FTy::truncate` to width and sign of the result type
//!     and replaces the instruction by `Op::Const` — the `Val` id stays the
//!     same, all uses stay valid.
//!     NOT folded are: division/remainder by zero, the overflow case
//!     `MIN / -1` or `MIN % -1` (both raise a CPU exception) and shifts
//!     with a width >= the bit width (undefined on x86).
//!  2. **Simplification of `brcond`** with a constant condition (or equal
//!     targets) to `br`. Only that makes unreachable code come about.
//!  3. **Dead code**: unreachable blocks (reachability from `bb0` through
//!     `Term::successors`) get removed and the remaining blocks renumbered
//!     without gaps (the invariant `blocks[i].id == i` survives, every
//!     terminator gets rewritten). Unused PURE instructions (no
//!     `store`/`call`/`syscall`/`copymem`) get removed; `alloca` only when
//!     its pointer is used nowhere any more.
//!
//! Iterated gets up to the fixpoint, yet at most `MAX_ROUNDS` times, so that
//! the optimizer cannot hang under any circumstances.

use crate::fir::{BinOp, BlockId, CmpOp, FTy, Func, Module, Op, Term, UnOp, Val};
use std::collections::{HashMap, HashSet};

/// hard upper bound of the fixpoint iterations
const MAX_ROUNDS: u32 = 50;

#[derive(Clone, Copy, Debug, Default)]
pub struct OptStats {
    /// count of instructions folded into constants
    pub folded: usize,
    /// removed instructions (dead/unused and pure)
    pub removed_insts: usize,
    /// removed, unreachable basic blocks
    pub removed_blocks: usize,
    /// resolved `load`s (mem2reg + local store forwarding)
    pub promoted_loads: usize,
    /// propagated copies / algebraic identities
    pub copies: usize,
    /// removed common subexpressions
    pub cse: usize,
    /// merged or bridged blocks
    pub merged_blocks: usize,
    /// embedded calls (inline.rs)
    pub inlined: usize,
    /// removed range checks provably always satisfied
    pub removed_checks: usize,
    /// loop invariant instructions that moved into the preheader
    pub hoisted: usize,
    /// edges threaded past a bool confluence
    pub threaded: usize,
}

// ----------------------------------------------------- Pass register ---
//
// DESIGN_GOALS.md §5 and §10.4 point 4: every optimization pass has a LABEL,
// a SWITCH and a TAG `debug preserving yes/no`. Only that way can the build
// level `--dev-fast` (fast, yet debuggable) be built later without touching
// every pass. The register is the single truth about which passes exist —
// `--list-passes` prints it.

/// Build level. `DevFast` is the default (DESIGN_GOALS.md §5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    /// no optimization at all (`--no-opt`) — for compiler troubleshooting only
    Dev,
    /// debug preserving passes only — the everyday mode
    DevFast,
    /// all passes (checks stay on as soon as there are any)
    ReleaseSafe,
    /// all passes
    ReleaseFast,
}

impl Level {
    pub fn from_str(s: &str) -> Option<Level> {
        match s {
            "dev" => Some(Level::Dev),
            "dev-fast" => Some(Level::DevFast),
            "release-safe" => Some(Level::ReleaseSafe),
            "release-fast" => Some(Level::ReleaseFast),
            _ => None,
        }
    }
    /// Does non-debug-preserving work run at this level too?
    fn allows_all(self) -> bool {
        matches!(self, Level::ReleaseSafe | Level::ReleaseFast)
    }
}

/// Scope of a pass.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    /// works on a single function, runs within the fixpoint loop
    Func,
    /// works on the whole module, runs once
    Module,
}

/// Description of a pass.
pub struct PassInfo {
    /// switch label for `--no-pass=`
    pub name: &'static str,
    pub scope: Scope,
    /// **Tag.** `true` = every named variable still shows its correct value at
    /// every breakpoint; the call stack stays readable.
    /// `false` = the pass destroys the debug picture and runs at the release
    /// levels only.
    pub debug_preserving: bool,
    pub what: &'static str,
}

/// All passes. Order = order of execution within one round.
pub const PASSES: &[PassInfo] = &[
    PassInfo {
        name: "fold",
        scope: Scope::Func,
        debug_preserving: true,
        what: "constant folding (Bin/Cmp/Un/Cast with constant operands)",
    },
    PassInfo {
        name: "mem2reg",
        scope: Scope::Func,
        debug_preserving: true,
        what: "stack slots to values, forwarding of local loads, dead stores",
    },
    PassInfo {
        name: "copyprop",
        scope: Scope::Func,
        debug_preserving: true,
        what: "propagate copies, algebraic identities",
    },
    PassInfo {
        name: "cse",
        scope: Scope::Func,
        debug_preserving: true,
        what: "combine common subexpressions",
    },
    PassInfo {
        name: "licm",
        scope: Scope::Func,
        debug_preserving: true,
        what: "hoist loop invariant computations into the preheader",
    },
    PassInfo {
        name: "bce",
        scope: Scope::Func,
        debug_preserving: true,
        what: "remove provably always satisfied bounds checks",
    },
    PassInfo {
        name: "thread-bool",
        scope: Scope::Func,
        debug_preserving: true,
        what: "jump threading through bool cells (short circuit && / ||)",
    },
    PassInfo {
        name: "simplify-term",
        scope: Scope::Func,
        debug_preserving: true,
        what: "simplify brcond with constant condition to br",
    },
    PassInfo {
        name: "merge-blocks",
        scope: Scope::Func,
        debug_preserving: true,
        what: "merge empty and singly linked basic blocks",
    },
    PassInfo {
        name: "dce",
        scope: Scope::Func,
        debug_preserving: true,
        what: "remove unreachable blocks and unused pure instructions",
    },
    PassInfo {
        name: "inline",
        scope: Scope::Module,
        debug_preserving: false,
        what: "inline calls (size heuristic) — makes the call stack unreadable",
    },
];

/// What shall be executed during a run.
#[derive(Clone, Debug)]
pub struct OptConfig {
    pub level: Level,
    /// passes switched off one by one (`--no-pass=`)
    pub disabled: Vec<String>,
}

impl Default for OptConfig {
    fn default() -> Self {
        OptConfig { level: Level::ReleaseFast, disabled: Vec::new() }
    }
}

impl OptConfig {
    /// Does this pass run?
    pub fn runs(&self, name: &str) -> bool {
        if self.level == Level::Dev {
            return false;
        }
        if self.disabled.iter().any(|d| d == name) {
            return false;
        }
        match PASSES.iter().find(|p| p.name == name) {
            Some(p) => p.debug_preserving || self.level.allows_all(),
            // Unknown labels cannot occur (internal callers only), yet they
            // get executed conservatively rather than silently skipped.
            None => true,
        }
    }
    /// Does this pass label exist at all?
    pub fn is_known(name: &str) -> bool {
        PASSES.iter().any(|p| p.name == name)
    }
}

/// The register as text (for `--list-passes`).
pub fn passes_text() -> String {
    let mut out = String::from(
        "optimization passes (order = execution order)\n\nNAME            SCOPE    DEBUG-PRESERVING  DESCRIPTION\n",
    );
    for p in PASSES {
        out.push_str(&format!(
            "{:<15} {:<8} {:<15} {}\n",
            p.name,
            match p.scope {
                Scope::Func => "function",
                Scope::Module => "module",
            },
            if p.debug_preserving { "ja" } else { "NO" },
            p.what
        ));
    }
    out.push_str(
        "\nBuild levels: --opt-level=dev | dev-fast | release-safe | release-fast\n'dev-fast' runs only the debug-preserving passes.\nDisable individually: --no-pass=<name> (may be repeated).\n",
    );
    out
}

// --------------------------------------------------------- Execution ---

/// Full optimization (`Level::ReleaseFast`) — short form for the module tests.
#[cfg(test)]
pub fn optimize(m: &mut Module) -> OptStats {
    optimize_with(m, &OptConfig::default())
}

pub fn optimize_with(m: &mut Module, cfg: &OptConfig) -> OptStats {
    let mut st = OptStats::default();
    if cfg.level == Level::Dev {
        return st;
    }
    // Clean up per function first, so that the size heuristic of the inliner
    // works on bodies that are simplified already.
    for f in m.funcs.iter_mut() {
        optimize_func(f, &mut st, cfg);
    }
    if cfg.runs("inline") {
        st.inlined += crate::inline::inline_module(m);
        for f in m.funcs.iter_mut() {
            optimize_func(f, &mut st, cfg);
        }
    }
    st
}

fn optimize_func(f: &mut Func, st: &mut OptStats, cfg: &OptConfig) {
    let mut round = 0;
    loop {
        round += 1;
        let mut changed = false;
        if cfg.runs("fold") {
            changed |= fold_constants(f, st);
        }
        if cfg.runs("mem2reg") {
            let p =
                crate::mem2reg::promote_single_store(f) + crate::mem2reg::forward_local_loads(f);
            let ds = crate::mem2reg::remove_dead_stores(f);
            st.removed_insts += ds;
            changed |= ds > 0;
            st.promoted_loads += p;
            changed |= p > 0;
        }
        if cfg.runs("copyprop") {
            let c = crate::mem2reg::copy_propagate(f);
            st.copies += c;
            changed |= c > 0;
        }
        if cfg.runs("cse") {
            let e = cse(f);
            st.cse += e;
            changed |= e > 0;
        }
        if cfg.runs("licm") {
            let h = crate::licm::hoist_loop_invariants(f);
            st.hoisted += h;
            changed |= h > 0;
        }
        if cfg.runs("bce") {
            let r = remove_redundant_checks(f);
            st.removed_checks += r;
            changed |= r > 0;
        }
        if cfg.runs("thread-bool") {
            let t = crate::threading::thread_bool_cells(f);
            st.threaded += t;
            changed |= t > 0;
        }
        if cfg.runs("simplify-term") {
            changed |= simplify_terminators(f);
        }
        if cfg.runs("merge-blocks") {
            let mb = crate::mem2reg::merge_blocks(f);
            st.merged_blocks += mb;
            changed |= mb > 0;
        }
        if cfg.runs("dce") {
            changed |= remove_unreachable_blocks(f, st);
            changed |= remove_dead_insts(f, st);
        }
        if !changed || round >= MAX_ROUNDS {
            break;
        }
    }
}

// ----------------------------------------------- common subexpressions (CSE) ---

/// Key of a pure, reusable expression.
#[derive(PartialEq, Eq, Hash, Clone)]
enum Key {
    Const(u8, i128),
    Bin(u8, u8, Val, Val),
    Cmp(u8, u8, Val, Val),
    Un(u8, u8, Val),
    Cast(u8, u8, Val),
    PtrAdd(Val, Val),
}

/// Number of a FIR type (fir::FTy does not derive `Hash`).
fn tyk(t: FTy) -> u8 {
    match t {
        FTy::F64 => 12,
        FTy::I8 => 1,
        FTy::I16 => 2,
        FTy::I32 => 3,
        FTy::I64 => 4,
        FTy::U8 => 5,
        FTy::U16 => 6,
        FTy::U32 => 7,
        FTy::U64 => 8,
        FTy::Bool => 9,
        FTy::Ptr => 10,
        FTy::Void => 11,
    }
}

fn bink(o: BinOp) -> u8 {
    match o {
        BinOp::Add => 1,
        BinOp::Sub => 2,
        BinOp::Mul => 3,
        BinOp::Div => 4,
        BinOp::Rem => 5,
        BinOp::And => 6,
        BinOp::Or => 7,
        BinOp::Xor => 8,
        BinOp::Shl => 9,
        BinOp::Shr => 10,
    }
}

fn cmpk(o: CmpOp) -> u8 {
    match o {
        CmpOp::Eq => 1,
        CmpOp::Ne => 2,
        CmpOp::Lt => 3,
        CmpOp::Le => 4,
        CmpOp::Gt => 5,
        CmpOp::Ge => 6,
    }
}

fn unk(o: UnOp) -> u8 {
    match o {
        UnOp::Neg => 1,
        UnOp::Not => 2,
    }
}

fn key_of(i: &crate::fir::Inst) -> Option<Key> {
    match &i.op {
        Op::Const(c) => Some(Key::Const(tyk(i.ty), *c)),
        Op::Bin(o, a, b) => Some(Key::Bin(tyk(i.ty), bink(*o), *a, *b)),
        Op::Cmp { op, ty, a, b } => Some(Key::Cmp(tyk(*ty), cmpk(*op), *a, *b)),
        Op::Un(o, a) => Some(Key::Un(tyk(i.ty), unk(*o), *a)),
        Op::Cast { src, from } => Some(Key::Cast(tyk(i.ty), tyk(*from), *src)),
        Op::PtrAdd { base, off } => Some(Key::PtrAdd(*base, *off)),
        // `load` depends on memory, `alloca` yields a separate address per
        // instruction, `select`/`barrier`/`secure_zero` are untouchable.
        _ => None,
    }
}

/// Removes pure expressions computed several times along the dominator tree:
/// one expression may only get replaced by a value whose definition
/// dominates the use.
fn cse(f: &mut Func) -> usize {
    if f.blocks.len() > 512 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    let dom = crate::mem2reg::dominators(f);
    let n = f.blocks.len();
    let mut avail: HashMap<Key, Vec<(usize, Val)>> = HashMap::new();
    let mut map: HashMap<Val, Val> = HashMap::new();
    for bi in 0..n {
        for ii in 0..f.blocks[bi].insts.len() {
            let inst = &f.blocks[bi].insts[ii];
            let d = match inst.dst {
                Some(d) => d,
                None => continue,
            };
            if f.is_secret(d) {
                continue;
            }
            let k = match key_of(inst) {
                Some(k) => k,
                None => continue,
            };
            let e = avail.entry(k).or_default();
            let mut hit = None;
            for &(ob, ov) in e.iter() {
                // Dominance: same block (earlier) or dominating block
                if (ob == bi || dom[bi][ob]) && !f.is_secret(ov) {
                    hit = Some(ov);
                    break;
                }
            }
            match hit {
                Some(ov) => {
                    map.insert(d, ov);
                }
                None => e.push((bi, d)),
            }
        }
    }
    if map.is_empty() {
        return 0;
    }
    let cnt = map.len();
    crate::mem2reg::replace_uses(f, &map);
    cnt
}

// ------------------------------------------------------------- Range checks ---

/// Removes range checks provably always satisfied: a `brcond` on `i < n`
/// that a dominating `brcond` with the same condition decided as true (or
/// false) already turns into one unconditional jump. That way the duplicate
/// check vanishes which comes about at the access to a field within a loop
/// that got checked already.
/// Yields the count of removed checks.
fn remove_redundant_checks(f: &mut Func) -> usize {
    if f.blocks.len() > 512 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    let preds = crate::mem2reg::preds(f);
    let n = f.blocks.len();
    // Knowledge gets carried forward exclusively along chains with EXACTLY ONE
    // predecessor. Such a chain can hold no cycle (a block entered again would
    // have a second predecessor), so the value of the condition is unchanged
    // on the path actually taken.
    let mut known: Vec<HashMap<Val, bool>> = vec![HashMap::new(); n];
    for bi in 0..n {
        let mut cur = bi;
        let mut facts: HashMap<Val, bool> = HashMap::new();
        for _ in 0..64 {
            if preds[cur].len() != 1 {
                break;
            }
            let p = preds[cur][0];
            if p == cur {
                break;
            }
            if let Term::BrCond { cond, then_bb, else_bb } = f.blocks[p].term {
                if (then_bb as usize == cur) != (else_bb as usize == cur) {
                    facts.entry(cond).or_insert(then_bb as usize == cur);
                }
            }
            cur = p;
        }
        known[bi] = facts;
    }
    let mut removed = 0usize;
    for bi in 0..n {
        if let Term::BrCond { cond, then_bb, else_bb } = f.blocks[bi].term {
            if f.is_secret(cond) {
                continue; // SPEC §9.2: secret conditions stay untouched
            }
            if let Some(&v) = known[bi].get(&cond) {
                f.blocks[bi].term = Term::Br(if v { then_bb } else { else_bb });
                removed += 1;
            }
        }
    }
    removed
}

// ---------------------------------------------------------------- Folding ---

/// Collects all known constant values of the function.
fn const_map(f: &Func) -> HashMap<Val, i128> {
    let mut m = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Const(c)) = (i.dst, &i.op) {
                m.insert(d, *c);
            }
        }
    }
    m
}

fn fold_constants(f: &mut Func, st: &mut OptStats) -> bool {
    let mut consts = const_map(f);
    let mut changed = false;
    for bi in 0..f.blocks.len() {
        for ii in 0..f.blocks[bi].insts.len() {
            let (ty, op, dst) = {
                let i = &f.blocks[bi].insts[ii];
                (i.ty, i.op.clone(), i.dst)
            };
            let dst = match dst {
                Some(d) => d,
                None => continue,
            };
            // FLOATING POINT NEVER GETS FOLDED. The value of one `Op::Const` with
            // `FTy::F64` is a BIT PATTERN; the folding here computes
            // integer wise and would turn `1.5 + 1.5` into silent nonsense.
            // Folding floating point needs its own evaluation that is
            // faithful to rounding — that comes with `comptime` (SPEC §8.6).
            if ty == FTy::F64 || op_has_f64(&op, f) {
                continue;
            }
            let folded = match op {
                Op::Bin(bop, a, b) => match (consts.get(&a), consts.get(&b)) {
                    (Some(&x), Some(&y)) => fold_bin(ty, bop, x, y),
                    _ => None,
                },
                Op::Cmp { op, ty: oty, a, b } => match (consts.get(&a), consts.get(&b)) {
                    (Some(&x), Some(&y)) => Some(fold_cmp(oty, op, x, y)),
                    _ => None,
                },
                Op::Un(uop, a) => consts.get(&a).map(|&x| fold_un(ty, uop, x)),
                Op::Cast { src, from } => match consts.get(&src) {
                    Some(&x) => fold_cast(ty, from, x),
                    None => None,
                },
                _ => None,
            };
            if let Some(v) = folded {
                f.blocks[bi].insts[ii].op = Op::Const(v);
                consts.insert(dst, v);
                st.folded += 1;
                changed = true;
            }
        }
    }
    changed
}

fn fold_bin(ty: FTy, op: BinOp, a: i128, b: i128) -> Option<i128> {
    if ty == FTy::Void || ty.bits() == 0 {
        return None;
    }
    let a = ty.truncate(a);
    let b = ty.truncate(b);
    let bits = ty.bits() as i128;
    let min_signed: i128 = if ty.signed() { -(1i128 << (ty.bits() - 1)) } else { 0 };
    let r = match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => {
            if b == 0 || (ty.signed() && a == min_signed && b == -1) {
                return None;
            }
            a / b
        }
        BinOp::Rem => {
            if b == 0 || (ty.signed() && a == min_signed && b == -1) {
                return None;
            }
            a % b
        }
        BinOp::And => a & b,
        BinOp::Or => a | b,
        BinOp::Xor => a ^ b,
        BinOp::Shl => {
            if b < 0 || b >= bits {
                return None;
            }
            a << b
        }
        BinOp::Shr => {
            if b < 0 || b >= bits {
                return None;
            }
            // `a` is normalized sign correctly already: for unsigned
            // types not negative (-> logical shift), for signed ones
            // arithmetic.
            a >> b
        }
    };
    Some(ty.truncate(r))
}

fn fold_cmp(ty: FTy, op: CmpOp, a: i128, b: i128) -> i128 {
    let a = ty.truncate(a);
    let b = ty.truncate(b);
    let r = match op {
        CmpOp::Eq => a == b,
        CmpOp::Ne => a != b,
        CmpOp::Lt => a < b,
        CmpOp::Le => a <= b,
        CmpOp::Gt => a > b,
        CmpOp::Ge => a >= b,
    };
    if r {
        1
    } else {
        0
    }
}

fn fold_un(ty: FTy, op: UnOp, a: i128) -> i128 {
    let a = ty.truncate(a);
    match op {
        UnOp::Neg => ty.truncate(-a),
        UnOp::Not => {
            if ty == FTy::Bool {
                if a & 1 != 0 {
                    0
                } else {
                    1
                }
            } else {
                ty.truncate(!a)
            }
        }
    }
}

fn fold_cast(to: FTy, from: FTy, a: i128) -> Option<i128> {
    if to == FTy::Void || from == FTy::Void {
        return None;
    }
    // Integer -> bool is no pure bit operation (comparison with 0 versus
    // "lowest bit"); the optimizer leaves that to the backend.
    if to == FTy::Bool && from != FTy::Bool {
        return None;
    }
    // FLOATING POINT IS NO BIT OPERATION.
    //
    // Up to round 20 `f64` fell under the same line as every integer. For
    // `u64 -> f64` that is wrong: the constant 100 became a `const.f64`
    // with the BIT PATTERN 100 (that is 5e-322), not with the value 100.0.
    // It came out only when the lexer written with Firn compiled `10.0`
    // and `firnc0` stood next to it — both token streams had to be equal,
    // and they were not. The path without the optimizer was right the
    // whole time (`cvtsi2sd`); the folding alone lied.
    if to == FTy::F64 || from == FTy::F64 {
        if to == FTy::F64 && from == FTy::F64 {
            return Some(a);
        }
        if to == FTy::F64 {
            let x = from.truncate(a);
            let f = if from.signed() { x as f64 } else { (x as u128) as f64 };
            return Some(f.to_bits() as i128);
        }
        // f64 -> integer: cutting towards zero, like `cvttsd2si`.
        // Outside the target range, for NaN and for infinity the instruction
        // yields a special value — then NOTHING gets folded, it is left to
        // the backend.
        let f = f64::from_bits((a as u128) as u64);
        if !f.is_finite() {
            return None;
        }
        let t = f.trunc();
        let bits = to.bits();
        if bits == 0 || bits > 64 {
            return None;
        }
        let (lo, hi): (f64, f64) = if to.signed() {
            (-(2f64.powi(bits as i32 - 1)), 2f64.powi(bits as i32 - 1))
        } else {
            (0.0, 2f64.powi(bits as i32))
        };
        if t < lo || t >= hi {
            return None;
        }
        return Some(to.truncate(t as i128));
    }
    Some(to.truncate(from.truncate(a)))
}

// ----------------------------------------------------------- Terminators ---

fn simplify_terminators(f: &mut Func) -> bool {
    let consts = const_map(f);
    let mut changed = false;
    for b in f.blocks.iter_mut() {
        if let Term::BrCond { cond, then_bb, else_bb } = b.term {
            if then_bb == else_bb {
                b.term = Term::Br(then_bb);
                changed = true;
            } else if let Some(&c) = consts.get(&cond) {
                b.term = Term::Br(if c != 0 { then_bb } else { else_bb });
                changed = true;
            }
        } else if let Term::Switch { val, cases, default, .. } = &b.term {
            // Constant label: jump straight to the matching branch.
            if let Some(&c) = consts.get(val) {
                let t = cases.iter().find(|(k, _)| *k == c).map(|(_, t)| *t).unwrap_or(*default);
                b.term = Term::Br(t);
                changed = true;
            } else if cases.iter().all(|(_, t)| *t == *default) {
                let d = *default;
                b.term = Term::Br(d);
                changed = true;
            }
        }
    }
    changed
}

// -------------------------------------------------------------- dead code ---

fn collect_uses(f: &Func, blocks: &[usize]) -> HashSet<Val> {
    let mut used = HashSet::new();
    let mut buf = Vec::new();
    for &bi in blocks {
        let b = &f.blocks[bi];
        for i in &b.insts {
            buf.clear();
            i.op.uses(&mut buf);
            for v in buf.iter() {
                used.insert(*v);
            }
        }
        match &b.term {
            Term::BrCond { cond, .. } => {
                used.insert(*cond);
            }
            Term::Ret(Some(v)) => {
                used.insert(*v);
            }
            Term::Switch { val, .. } => {
                used.insert(*val);
            }
            Term::Br(_) | Term::Ret(None) | Term::Unset => {}
        }
    }
    used
}

fn remove_unreachable_blocks(f: &mut Func, st: &mut OptStats) -> bool {
    if f.blocks.is_empty() {
        return false;
    }
    let mut index_of: HashMap<BlockId, usize> = HashMap::new();
    for (i, b) in f.blocks.iter().enumerate() {
        index_of.insert(b.id, i);
    }
    // Reachability from the entry block
    let mut reachable = vec![false; f.blocks.len()];
    let mut stack = vec![0usize];
    reachable[0] = true;
    while let Some(bi) = stack.pop() {
        for s in f.blocks[bi].term.successors() {
            if let Some(&si) = index_of.get(&s) {
                if !reachable[si] {
                    reachable[si] = true;
                    stack.push(si);
                }
            }
        }
    }
    if reachable.iter().all(|&r| r) {
        return false;
    }

    // Safety net: if a value defined inside some unreachable block still
    // gets read out of reachable code (that would violate the SSA
    // dominance), NOTHING gets removed — better dead code than a
    // dangling Val id.
    let live_idx: Vec<usize> = (0..f.blocks.len()).filter(|&i| reachable[i]).collect();
    let used = collect_uses(f, &live_idx);
    for (i, b) in f.blocks.iter().enumerate() {
        if reachable[i] {
            continue;
        }
        for inst in &b.insts {
            if let Some(d) = inst.dst {
                if used.contains(&d) {
                    return false;
                }
            }
        }
    }

    let removed_insts: usize =
        f.blocks.iter().enumerate().filter(|(i, _)| !reachable[*i]).map(|(_, b)| b.insts.len()).sum();
    let removed_blocks = reachable.iter().filter(|&&r| !r).count();

    // renumber without gaps, the order survives
    let mut new_id: HashMap<BlockId, BlockId> = HashMap::new();
    let mut kept = Vec::with_capacity(live_idx.len());
    for (n, &i) in live_idx.iter().enumerate() {
        new_id.insert(f.blocks[i].id, n as BlockId);
        kept.push(f.blocks[i].clone());
    }
    for (n, b) in kept.iter_mut().enumerate() {
        b.id = n as BlockId;
        b.term = match &b.term {
            Term::Br(t) => Term::Br(new_id[t]),
            Term::BrCond { cond, then_bb, else_bb } => {
                Term::BrCond { cond: *cond, then_bb: new_id[then_bb], else_bb: new_id[else_bb] }
            }
            Term::Switch { val, ty, cases, default } => Term::Switch {
                val: *val,
                ty: *ty,
                cases: cases.iter().map(|(k, t)| (*k, new_id[t])).collect(),
                default: new_id[default],
            },
            other => other.clone(),
        };
    }
    f.blocks = kept;
    st.removed_blocks += removed_blocks;
    st.removed_insts += removed_insts;
    true
}

fn remove_dead_insts(f: &mut Func, st: &mut OptStats) -> bool {
    let mut changed = false;
    let mut round = 0;
    loop {
        round += 1;
        let all: Vec<usize> = (0..f.blocks.len()).collect();
        let used = collect_uses(f, &all);
        let mut removed = 0usize;
        for b in f.blocks.iter_mut() {
            let before = b.insts.len();
            b.insts.retain(|i| {
                if !i.op.is_pure() {
                    return true;
                }
                match i.dst {
                    Some(d) => used.contains(&d),
                    // pure instruction without result: without effect
                    None => false,
                }
            });
            removed += before - b.insts.len();
        }
        if removed == 0 {
            break;
        }
        st.removed_insts += removed;
        changed = true;
        if round >= MAX_ROUNDS {
            break;
        }
    }
    changed
}

// ------------------------------------------------------------------ Tests ---

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fir::{Inst, Term};

    fn consts_in(f: &Func) -> Vec<i128> {
        let mut v = Vec::new();
        for b in &f.blocks {
            for i in &b.insts {
                if let Op::Const(c) = i.op {
                    v.push(c);
                }
            }
        }
        v
    }

    #[test]
    fn folds_arithmetic_and_removed_intermediates() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(20));
        let b = f.push(0, FTy::I32, Op::Const(2));
        let m = f.push(0, FTy::I32, Op::Bin(BinOp::Mul, a, b));
        let c = f.push(0, FTy::I32, Op::Const(2));
        let s = f.push(0, FTy::I32, Op::Bin(BinOp::Add, m, c));
        f.set_term(0, Term::Ret(Some(s)));
        let before = f.inst_count();
        let mut m0 = Module::new();
        m0.funcs.push(f);
        let st = optimize(&mut m0);
        let f = &m0.funcs[0];
        assert!(st.folded >= 2, "it must be folded: {:?}", st);
        assert!(f.inst_count() < before, "{} -> {}", before, f.inst_count());
        assert_eq!(f.inst_count(), 1);
        assert_eq!(consts_in(f), vec![42]);
        assert!(matches!(f.blocks[0].term, Term::Ret(Some(_))));
    }

    #[test]
    fn division_by_null_stays() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(7));
        let b = f.push(0, FTy::I32, Op::Const(0));
        let d = f.push(0, FTy::I32, Op::Bin(BinOp::Div, a, b));
        f.set_term(0, Term::Ret(Some(d)));
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        assert_eq!(st.folded, 0);
        assert_eq!(m.funcs[0].inst_count(), 3);
        assert!(matches!(m.funcs[0].blocks[0].insts[2].op, Op::Bin(BinOp::Div, _, _)));
    }

    #[test]
    fn wide_width_shift_stays() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let a = f.push(0, FTy::I32, Op::Const(1));
        let b = f.push(0, FTy::I32, Op::Const(32));
        let s = f.push(0, FTy::I32, Op::Bin(BinOp::Shl, a, b));
        f.set_term(0, Term::Ret(Some(s)));
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        assert_eq!(st.folded, 0);
        assert!(matches!(m.funcs[0].blocks[0].insts[2].op, Op::Bin(BinOp::Shl, _, _)));
    }

    #[test]
    fn overflow_becomes_correct_trimmed() {
        let mut f = Func::new("t", vec![], FTy::I8);
        let a = f.push(0, FTy::I8, Op::Const(100));
        let b = f.push(0, FTy::I8, Op::Const(100));
        let s = f.push(0, FTy::I8, Op::Bin(BinOp::Add, a, b));
        f.set_term(0, Term::Ret(Some(s)));
        let mut m = Module::new();
        m.funcs.push(f);
        optimize(&mut m);
        assert_eq!(consts_in(&m.funcs[0]), vec![-56]); // 200 mod 256 as i8
    }

    #[test]
    fn unsigned_shift_and_cast() {
        let mut f = Func::new("t", vec![], FTy::U64);
        let a = f.push(0, FTy::U8, Op::Const(200));
        let c = f.push(0, FTy::U64, Op::Cast { src: a, from: FTy::U8 });
        let sh = f.push(0, FTy::U64, Op::Const(1));
        let r = f.push(0, FTy::U64, Op::Bin(BinOp::Shr, c, sh));
        f.set_term(0, Term::Ret(Some(r)));
        let mut m = Module::new();
        m.funcs.push(f);
        optimize(&mut m);
        assert_eq!(consts_in(&m.funcs[0]), vec![100]);

        // signed shortening/widening
        let mut f = Func::new("t2", vec![], FTy::I64);
        let a = f.push(0, FTy::I8, Op::Const(-1));
        let c = f.push(0, FTy::I64, Op::Cast { src: a, from: FTy::I8 });
        f.set_term(0, Term::Ret(Some(c)));
        let mut m = Module::new();
        m.funcs.push(f);
        optimize(&mut m);
        assert_eq!(consts_in(&m.funcs[0]), vec![-1]);

        // i8 -1 -> u32 = 4294967295
        let mut f = Func::new("t3", vec![], FTy::U32);
        let a = f.push(0, FTy::I8, Op::Const(-1));
        let c = f.push(0, FTy::U32, Op::Cast { src: a, from: FTy::I8 });
        f.set_term(0, Term::Ret(Some(c)));
        let mut m = Module::new();
        m.funcs.push(f);
        optimize(&mut m);
        assert_eq!(consts_in(&m.funcs[0]), vec![4294967295]);
    }

    #[test]
    fn compare_and_branch_fold_unreachable_block_away() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let then_bb = f.add_block();
        let else_bb = f.add_block();
        let a = f.push(0, FTy::I32, Op::Const(3));
        let b = f.push(0, FTy::I32, Op::Const(4));
        let c = f.push(0, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I32, a, b });
        f.set_term(0, Term::BrCond { cond: c, then_bb, else_bb });
        let x = f.push(then_bb, FTy::I32, Op::Const(1));
        f.set_term(then_bb, Term::Ret(Some(x)));
        let y = f.push(else_bb, FTy::I32, Op::Const(2));
        f.set_term(else_bb, Term::Ret(Some(y)));
        let blocks_before = f.blocks.len();
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        let f = &m.funcs[0];
        // 3 < 4 is true: the else branch falls away, the then branch gets merged
        // into bb0 — left over is ONE block with `ret 1`.
        assert_eq!(st.removed_blocks, 2);
        assert!(f.blocks.len() < blocks_before);
        assert_eq!(f.blocks.len(), 1);
        // Block ids stay gapless and match their position
        for (i, b) in f.blocks.iter().enumerate() {
            assert_eq!(b.id, i as u32);
        }
        assert!(matches!(f.blocks[0].term, Term::Ret(Some(_))));
        assert_eq!(consts_in(f), vec![1]);
    }

    #[test]
    fn side_effects_stay_keep() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(4, 4);
        let v = f.push(0, FTy::I32, Op::Const(5));
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: v });
        let n = f.push(0, FTy::I64, Op::Const(60));
        let arg = f.push(0, FTy::I64, Op::Const(0));
        let sc = f.push(0, FTy::I64, Op::Syscall { args: vec![n, arg] });
        let unused = f.push(0, FTy::I32, Op::Const(99));
        let _ = unused;
        let call = f.push(0, FTy::I32, Op::Call { name: "f".into(), args: vec![] });
        let _ = call;
        let ld = f.push(0, FTy::I32, Op::Load { addr: slot });
        f.set_term(0, Term::Ret(Some(ld)));
        let _ = sc;
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        let f = &m.funcs[0];
        // Removed get: the unused constant 99, plus (new at round 2) the dead
        // local cell — the `load` gets forwarded to the stored value, after
        // which nobody reads from the `alloca` any more.
        // Syscall and call MUST stay.
        assert!(st.removed_insts >= 1);
        let kinds: Vec<&str> = f.blocks[0]
            .insts
            .iter()
            .map(|i: &Inst| match &i.op {
                Op::Alloca { .. } => "alloca",
                Op::Const(_) => "const",
                Op::Store { .. } => "store",
                Op::Syscall { .. } => "syscall",
                Op::Call { .. } => "call",
                Op::Load { .. } => "load",
                _ => "?",
            })
            .collect();
        assert_eq!(kinds, vec!["const", "const", "const", "syscall", "call"]);
        assert!(matches!(f.blocks[0].term, Term::Ret(Some(v)) if v == 1 + 0));
    }

    #[test]
    fn unused_alloca_vanishes_chained() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let slot = f.alloca(8, 8);
        let off = f.push(0, FTy::I64, Op::Const(4));
        let p = f.push(0, FTy::Ptr, Op::PtrAdd { base: slot, off });
        let _ld = f.push(0, FTy::I32, Op::Load { addr: p });
        let r = f.push(0, FTy::I32, Op::Const(0));
        f.set_term(0, Term::Ret(Some(r)));
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        assert_eq!(m.funcs[0].inst_count(), 1);
        assert_eq!(st.removed_insts, 4);
    }

    #[test]
    fn loop_stays_untouched_and_terminated() {
        // while (i < 10) { i = i + 1 }  — nothing of that is constant foldable,
        // the optimizer may remove nothing here and must halt.
        let mut f = Func::new("t", vec![], FTy::I32);
        let head = f.add_block();
        let body = f.add_block();
        let exit = f.add_block();
        let slot = f.alloca(4, 4);
        let zero = f.push(0, FTy::I32, Op::Const(0));
        f.push_void(0, FTy::I32, Op::Store { addr: slot, val: zero });
        f.set_term(0, Term::Br(head));
        let i = f.push(head, FTy::I32, Op::Load { addr: slot });
        let ten = f.push(head, FTy::I32, Op::Const(10));
        let c = f.push(head, FTy::Bool, Op::Cmp { op: CmpOp::Lt, ty: FTy::I32, a: i, b: ten });
        f.set_term(head, Term::BrCond { cond: c, then_bb: body, else_bb: exit });
        let i2 = f.push(body, FTy::I32, Op::Load { addr: slot });
        let one = f.push(body, FTy::I32, Op::Const(1));
        let s = f.push(body, FTy::I32, Op::Bin(BinOp::Add, i2, one));
        f.push_void(body, FTy::I32, Op::Store { addr: slot, val: s });
        f.set_term(body, Term::Br(head));
        let r = f.push(exit, FTy::I32, Op::Load { addr: slot });
        f.set_term(exit, Term::Ret(Some(r)));
        let before = f.inst_count();
        let blocks = f.blocks.len();
        let mut m = Module::new();
        m.funcs.push(f);
        let st = optimize(&mut m);
        assert_eq!(st.folded, 0);
        assert_eq!(st.removed_insts, 0);
        assert_eq!(st.removed_blocks, 0);
        assert_eq!(m.funcs[0].inst_count(), before);
        assert_eq!(m.funcs[0].blocks.len(), blocks);
    }

    #[test]
    fn chain_becomes_to_to_fixpunkt_folded() {
        let mut f = Func::new("t", vec![], FTy::I32);
        let mut v = f.push(0, FTy::I32, Op::Const(1));
        for _ in 0..10 {
            let one = f.push(0, FTy::I32, Op::Const(1));
            v = f.push(0, FTy::I32, Op::Bin(BinOp::Add, v, one));
        }
        let neg = f.push(0, FTy::I32, Op::Un(UnOp::Neg, v));
        f.set_term(0, Term::Ret(Some(neg)));
        let mut m = Module::new();
        m.funcs.push(f);
        optimize(&mut m);
        assert_eq!(m.funcs[0].inst_count(), 1);
        assert_eq!(consts_in(&m.funcs[0]), vec![-11]);
    }
}

/// Does some `f64` show up anywhere at this instruction? For constant
/// folding that rules it out (see `fold_constants`).
fn op_has_f64(op: &Op, f: &Func) -> bool {
    match op {
        Op::Cmp { ty, .. } => *ty == FTy::F64,
        Op::Cast { from, .. } => *from == FTy::F64,
        Op::Bin(_, a, b) => f.val_ty(*a) == FTy::F64 || f.val_ty(*b) == FTy::F64,
        Op::Un(_, a) => f.val_ty(*a) == FTy::F64,
        _ => false,
    }
}
