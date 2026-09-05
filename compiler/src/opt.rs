// SPDX-License-Identifier: GPL-2.0-only
//! Optimizer on FIR: constant folding and removal of dead code.
//!
//! INTERFACE (fixed):
//!   `pub fn optimize(m: &mut fir::Module) -> OptStats`
//! Rule: the optimization may NEVER change program behaviour. The test suite
//! runs every program with and without `--no-opt` and compares.
//!
//! Transformations carried out (all behaviour preserving):
//!  1. **Constant folding** over `Op::Bin`, `Op::Cmp`, `Op::Un` and
//!     `Op::Cast` when all operands are `Op::Const`. The result is
//!     normalized with `FTy::truncate` to the width and sign of the result
//!     type and replaces the instruction by `Op::Const` — the `Val` id stays
//!     the same, all uses stay valid.
//!     NOT folded are: division/remainder by zero, the overflow case
//!     `MIN / -1` or `MIN % -1` (both raise a CPU exception) and shifts
//!     with a width >= the bit width (undefined on x86).
//!  2. **Simplification of `brcond`** with a constant condition (or equal
//!     targets) to `br`. Only that makes unreachable code arise.
//!  3. **Dead code**: unreachable blocks (reachability from `bb0` through
//!     `Term::successors`) are removed and the remaining blocks renumbered
//!     without gaps (the invariant `blocks[i].id == i` survives, every
//!     terminator is rewritten). Unused PURE instructions (no
//!     `store`/`call`/`syscall`/`copymem`) are removed; `alloca` only when
//!     its pointer is used nowhere any more.
//!
//! It iterates up to the fixpoint, but at most `MAX_ROUNDS` times, so that
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
    /// ROUND 82: places at which `peephole.rs` replaced an instruction
    pub strength: usize,
    /// ROUND 92: phi entries dropped or phis folded back into one value
    pub phis_folded: usize,
}

// ----------------------------------------------------- Pass register ---
//
// DESIGN_GOALS.md §5 and §10.4 point 4: every optimization pass has a NAME,
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
    /// works on a single function, runs inside the fixpoint loop
    Func,
    /// works on the whole module, runs once
    Module,
}

/// Description of a pass.
pub struct PassInfo {
    /// switch name for `--no-pass=<name>`
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
        name: "strength",
        scope: Scope::Func,
        debug_preserving: true,
        what: "negated comparison, brcond over a negation, unsigned / and % by a power of two",
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
        what: "remove provably always satisfied range, index and arithmetic checks",
    },
    PassInfo {
        name: "thread-bool",
        scope: Scope::Func,
        debug_preserving: true,
        what: "jump threading through bool cells and bool phis (short circuit && / ||)",
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
        // DESIGN_GOALS.md line 554: "--dev-fast (default)" -- the CLI default
        // WITHOUT --opt-level=/--no-opt has to be `DevFast`, not `ReleaseFast`.
        // Round 72 found this line lying: `firnc -o x file.fi` silently built
        // release-fast (unchecked arithmetic) while every doc and the `Level`
        // enum comment above said dev-fast is the default.
        OptConfig { level: Level::DevFast, disabled: Vec::new() }
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
            // An unknown name cannot occur (internal callers only), but it is
            // executed conservatively rather than silently skipped.
            None => true,
        }
    }
    /// Does this pass name exist at all?
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

/// **ROUND 82** — milliseconds per pass, summed over all functions.
///
/// Switched on with `FIRN_PASS_TIMINGS=1`. Why an environment variable and
/// not a flag: this is a measurement of the COMPILER, not of the program, and
/// it belongs next to `--timings` without giving the command line a second
/// switch that nobody uses twice a year.
#[derive(Default)]
pub struct PassClock {
    /// name, ms in all, ms of them for nothing, productive runs, idle runs
    pub rows: Vec<(&'static str, f64, f64, usize, usize)>,
    pub on: bool,
    pub rounds: usize,
}

impl PassClock {
    fn add(&mut self, name: &'static str, t: std::time::Instant) {
        self.add2(name, t, true)
    }
    /// ROUND 87: and whether the pass found anything. A pass that runs and
    /// changes nothing has still cost its full run time, and THAT is the
    /// number this round is about.
    fn add2(&mut self, name: &'static str, t: std::time::Instant, useful: bool) {
        if !self.on {
            return;
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let row = match self.rows.iter_mut().position(|r| r.0 == name) {
            Some(i) => &mut self.rows[i],
            None => {
                self.rows.push((name, 0.0, 0.0, 0, 0));
                self.rows.last_mut().unwrap()
            }
        };
        row.1 += ms;
        if useful {
            row.3 += 1;
        } else {
            row.2 += ms;
            row.4 += 1;
        }
    }
    fn print(&self) {
        if !self.on {
            return;
        }
        let total: f64 = self.rows.iter().map(|r| r.1).sum();
        let waste: f64 = self.rows.iter().map(|r| r.2).sum();
        let mut rows = self.rows.clone();
        rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        eprintln!("optimizer passes (milliseconds, {} fixpoint rounds in all)", self.rounds);
        eprintln!("  {:<16} {:>9} {:>7}   {:>9} {:>8} {:>8}",
                  "pass", "total ms", "share", "for nix ms", "runs", "of them 0");
        for (n, ms, idle, runs, nix) in &rows {
            eprintln!("  {:<16} {:8.1} {:6.1} %   {:9.1} {:8} {:8}",
                      n, ms, ms / total * 100.0, idle, runs + nix, nix);
        }
        eprintln!("  {:<16} {:8.1} ms of {:.1} ms ({:.1} %) went into passes that changed nothing",
                  "IN VAIN:", waste, total, waste / total * 100.0);
    }
}

pub fn optimize_with(m: &mut Module, cfg: &OptConfig) -> OptStats {
    let mut st = OptStats::default();
    if cfg.level == Level::Dev {
        return st;
    }
    let mut clk = PassClock {
        on: std::env::var_os("FIRN_PASS_TIMINGS").is_some(),
        ..Default::default()
    };
    // Clean up per function first, so that the size heuristic of the inliner
    // works on bodies that are already simplified.
    for f in m.funcs.iter_mut() {
        optimize_func(f, &mut st, cfg, &mut clk);
    }
    if cfg.runs("inline") {
        let t = std::time::Instant::now();
        st.inlined += crate::inline::inline_module(m);
        clk.add("inline", t);
        for f in m.funcs.iter() {
            phi_check(f, "inline");
        }
        for f in m.funcs.iter_mut() {
            optimize_func(f, &mut st, cfg, &mut clk);
        }
    }
    clk.print();
    st
}

// ROUND 87 -- WHY THE SAME PASS DOES NOT RUN TWICE OVER THE SAME CODE.
//
// The loop below runs all twelve passes until NOTHING changes any more. The
// last round of every function is therefore pure confirmation: twelve passes
// look at the code and all twelve say "nothing to do". With 1,308 functions
// in bin/firnc1.fi that is 1,308 rounds of the 5,840 measured -- and it is
// not only the last round. `bce` and `thread-bool` are usually finished
// after the first one and still look again four times.
//
// The rule that gets rid of it is simple and it is EXACT, not a heuristic:
//
//   a pass is deterministic. If it looked at exactly this code once and
//   changed nothing, then it will change nothing this time either.
//
// So the code carries a VERSION, counted up by every pass that changes
// something. A pass that reports "no change" writes down the version it saw.
// As long as the version has not moved on, the pass is skipped. The moment
// any other pass changes anything the version moves and everybody looks
// again. Nothing is left out that would have found something; the fixpoint
// reached is the same one, and the assembler it produces is octet-identical
// (measured over bin/firnc1.fi, 649,720 lines).
//
// The counting is not free -- one comparison per pass and round -- but the
// comparison costs nothing against `licm`, which walks the dominator tree.
struct Fix {
    /// version of the code, counted up by every change
    version: u64,
    /// per pass: the version at which it last found NOTHING.
    /// `u64::MAX` = it has not said that yet.
    quiet: [u64; PASS_SLOTS],
    changed: bool,
    /// rounds in which at least one pass really ran (for the statistics)
    ran: usize,
}

/// as many slots as there are passes in `PASSES`
const PASS_SLOTS: usize = 12;

impl Fix {
    fn new() -> Fix {
        Fix { version: 0, quiet: [u64::MAX; PASS_SLOTS], changed: false, ran: 0 }
    }
    /// Does slot `k` have to look at all?
    fn due(&self, k: usize) -> bool {
        self.quiet[k] != self.version
    }
    /// Report the outcome of a pass.
    fn note(&mut self, k: usize, changed: bool) {
        if changed {
            self.version += 1;
            self.changed = true;
        } else {
            self.quiet[k] = self.version;
        }
    }
}

/// ROUND 92 -- `FIRN_VERIFY_PHI=2` checks the phi invariants after EVERY
/// pass and names the one that broke them. That is how the duplicate entry
/// of `tests/800_std_str_core.fi` was found; it costs a predecessor table
/// per pass and is therefore off unless asked for.
///
/// HOW TO READ ITS OUTPUT. A line saying "TWO entries for bbN" is ALWAYS a
/// bug: one predecessor cannot bring two different values. A line saying
/// "phi has N entries, the block has M predecessors" may be a transient --
/// `simplify-term` removes an edge and the next `mem2reg` round trims the
/// entry that stood for it -- so what matters there is whether it survives
/// the next `mem2reg`.
fn phi_check(f: &Func, pass: &str) {
    if std::env::var_os("FIRN_VERIFY_PHI").map(|v| v == "2").unwrap_or(false) {
        if let Err(e) = f.verify_phis() {
            eprintln!("PHI BROKEN after '{}': {}", pass, e);
        }
    }
}

fn optimize_func(f: &mut Func, st: &mut OptStats, cfg: &OptConfig, clk: &mut PassClock) {
    let mut round = 0;
    let mut fx = Fix::new();
    loop {
        round += 1;
        fx.changed = false;
        if cfg.runs("fold") && fx.due(0) {
            let t = std::time::Instant::now();
            let c = fold_constants(f, st);
            clk.add2("fold", t, c);
            fx.note(0, c);
            phi_check(f, "fold");
        }
        if cfg.runs("mem2reg") && fx.due(1) {
            let t = std::time::Instant::now();
            // ROUND 92: `promote_allocas` is the real thing now (dominance
            // frontiers, phi nodes, renaming) and subsumes what
            // `promote_single_store` did until round 91 -- a cell written
            // once is just the case in which no phi is needed.
            // `simplify_phis` is the hygiene that keeps the entry lists in
            // step with a control flow graph the other passes keep changing.
            let p = crate::mem2reg::promote_allocas(f)
                + crate::mem2reg::forward_local_loads(f);
            let sp = crate::mem2reg::simplify_phis(f);
            let ds = crate::mem2reg::remove_dead_stores(f);
            st.removed_insts += ds;
            st.promoted_loads += p;
            st.phis_folded += sp;
            clk.add2("mem2reg", t, p > 0 || ds > 0 || sp > 0);
            fx.note(1, p > 0 || ds > 0 || sp > 0);
            phi_check(f, "mem2reg");
        }
        if cfg.runs("copyprop") && fx.due(2) {
            let t = std::time::Instant::now();
            let c = crate::mem2reg::copy_propagate(f);
            st.copies += c;
            clk.add2("copyprop", t, c > 0);
            fx.note(2, c > 0);
            phi_check(f, "copyprop");
        }
        if cfg.runs("strength") && fx.due(3) {
            let t = std::time::Instant::now();
            let n = crate::peephole::run(f);
            st.strength += n;
            clk.add2("strength", t, n > 0);
            fx.note(3, n > 0);
            phi_check(f, "strength");
        }
        if cfg.runs("cse") && fx.due(4) {
            let t = std::time::Instant::now();
            let e = cse(f);
            st.cse += e;
            clk.add2("cse", t, e > 0);
            fx.note(4, e > 0);
            phi_check(f, "cse");
        }
        if cfg.runs("licm") && fx.due(5) {
            let t = std::time::Instant::now();
            let h = crate::licm::hoist_loop_invariants(f);
            st.hoisted += h;
            clk.add2("licm", t, h > 0);
            fx.note(5, h > 0);
            phi_check(f, "licm");
        }
        if cfg.runs("bce") && fx.due(6) {
            let t = std::time::Instant::now();
            // ROUND SPEED -- the third question in the same slot: an
            // ARITHMETIC check whose operands cannot leave the type.
            // `rangecheck.rs`, and it belongs here because the register
            // this pass already describes itself as "range and index
            // checks".
            let r = remove_redundant_checks(f)
                + remove_provable_index_checks(f)
                + crate::rangecheck::remove_provable_arith_checks(f);
            st.removed_checks += r;
            clk.add2("bce", t, r > 0);
            fx.note(6, r > 0);
            phi_check(f, "bce");
        }
        if cfg.runs("thread-bool") && fx.due(7) {
            let clock = std::time::Instant::now();
            let t = crate::threading::thread_bool_cells(f)
                + crate::threading::thread_bool_phis(f);
            st.threaded += t;
            clk.add2("thread-bool", clock, t > 0);
            fx.note(7, t > 0);
            phi_check(f, "thread-bool");
        }
        if cfg.runs("simplify-term") && fx.due(8) {
            let t = std::time::Instant::now();
            let c = simplify_terminators(f);
            clk.add2("simplify-term", t, c);
            fx.note(8, c);
            phi_check(f, "simplify-term");
        }
        if cfg.runs("merge-blocks") && fx.due(9) {
            let t = std::time::Instant::now();
            let mb = crate::mem2reg::merge_blocks(f);
            st.merged_blocks += mb;
            clk.add2("merge-blocks", t, mb > 0);
            fx.note(9, mb > 0);
            phi_check(f, "merge-blocks");
        }
        if cfg.runs("dce") && fx.due(10) {
            let t = std::time::Instant::now();
            let a = remove_unreachable_blocks(f, st);
            let b = remove_dead_insts(f, st);
            clk.add2("dce", t, a || b);
            fx.note(10, a || b);
            phi_check(f, "dce");
        }
        clk.rounds += 1;
        if !fx.changed || round >= MAX_ROUNDS {
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
        FTy::F32 => 13,
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
        FTy::V128 => 14,
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
/// an expression may only be replaced by a value whose definition
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

/// Removes range checks that are provably always satisfied: a `brcond` on
/// `i < n` that a dominating `brcond` with the same condition has already
/// decided as true (or false) turns into an unconditional jump. That way
/// the duplicate check vanishes that arises when a field is accessed inside
/// a loop that has already been checked.
/// Yields the number of checks removed.
/// Which conditions are already decided when a block is entered.
///
/// Knowledge is carried forward exclusively along chains with EXACTLY ONE
/// predecessor. Such a chain can contain no cycle (a block entered again
/// would have a second predecessor), so the value of the condition is
/// unchanged on the path actually taken.
///
/// ROUND 89: split out of `remove_redundant_checks`, because the index
/// checks need the same facts and computing them twice, differently, is how
/// two passes end up disagreeing about what is known.
fn known_facts(f: &Func) -> Vec<HashMap<Val, bool>> {
    let preds = crate::mem2reg::preds(f);
    let n = f.blocks.len();
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
    known
}

fn remove_redundant_checks(f: &mut Func) -> usize {
    if f.blocks.len() > 512 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    let n = f.blocks.len();
    let known = known_facts(f);
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

/// **ROUND 89** — throws away the index checks the optimiser can PROVE are
/// satisfied (SPEC §13, item `L9`).
///
/// Two proofs, and no third one that "looks safe":
///
/// 1. **The index is a constant** inside the array. Then the check is a
///    comparison of two numbers the compiler already knows.
/// 2. **A dominating branch has already decided it.** The single
///    predecessor chain (`known_facts`) says which conditions hold on the
///    way into this block. If one of them is `i < k` (or `i <= k`) with a
///    constant `k` that does not reach past the array, then `i < len`
///    holds too. That is exactly the shape `for i in 0..n` lowers to
///    (`lower.rs::lower_for` builds a `while i < n` around the body), so
///    the loop the SPEC promises would not pay for the check does not pay
///    for it.
///
/// An index is a `usize`, so there is no lower bound to prove.
///
/// The instruction is not deleted here: its uses are pointed at the index
/// value and the now dead (pure) `const` that is left behind is removed by
/// `dce` in the same run. That way this pass does not have to renumber
/// anything.
/// Could this instruction have changed what is behind a pointer?
/// Deliberately generous: everything that is not obviously pure counts as
/// a write, because the question being answered ("is the value I loaded
/// still the value I am about to index with") must not be answered
/// optimistically.
fn writes_memory(op: &Op) -> bool {
    !matches!(
        op,
        Op::Const(_)
            | Op::Bin(..)
            | Op::BinWrapSat { .. }
            | Op::Cmp { .. }
            | Op::Un(..)
            | Op::Cast { .. }
            | Op::PtrAdd { .. }
            | Op::Load { .. }
            | Op::Alloca { .. }
            | Op::Select { .. }
            | Op::VtabAddr { .. }
            | Op::FnRef { .. }
            | Op::GlobalAddr { .. }
            | Op::CheckedBin { .. }
            | Op::CheckedDiv { .. }
            | Op::CheckedCast { .. }
            | Op::CheckedIdx { .. }
    )
}

fn remove_provable_index_checks(f: &mut Func) -> usize {
    if f.blocks.len() > 512 || f.blocks.iter().enumerate().any(|(i, b)| b.id as usize != i) {
        return 0;
    }
    let has_any = f
        .blocks
        .iter()
        .any(|b| b.insts.iter().any(|i| matches!(i.op, Op::CheckedIdx { .. })));
    if !has_any {
        return 0;
    }
    let consts = const_map(f);
    // Every comparison in the function, by the value it produces.
    let mut cmps: HashMap<Val, (CmpOp, Val, Val)> = HashMap::new();
    for b in &f.blocks {
        for i in &b.insts {
            if let (Some(d), Op::Cmp { op, a, b: rhs, .. }) = (i.dst, &i.op) {
                cmps.insert(d, (*op, *a, *rhs));
            }
        }
    }
    let known = known_facts(f);
    let preds = crate::mem2reg::preds(f);
    // Where every value is defined: (block, position in that block).
    let mut site: HashMap<Val, (usize, usize)> = HashMap::new();
    // The address a value was LOADED from, if it is a load.
    let mut loaded_from: HashMap<Val, Val> = HashMap::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ii, i) in b.insts.iter().enumerate() {
            if let Some(d) = i.dst {
                site.insert(d, (bi, ii));
                if let Op::Load { addr } = i.op {
                    loaded_from.insert(d, addr);
                }
            }
        }
    }
    // THE SAME VALUE, written twice.
    //
    // `while i < n { a[i] }` compares a load of `i` in the loop header and
    // indexes with a SECOND load of the same `i` in the body — two FIR
    // values, one variable. Firn's FIR has no phi nodes (mem2reg promotes
    // only what does not cross a block), so without this the proof would
    // fail on the very loop shape the SPEC promises it would succeed on.
    //
    // Sound because it is narrow: the two loads must read the SAME
    // address, the compare must sit in the IMMEDIATE single predecessor,
    // and between the first load and the index there must be no
    // instruction that writes memory at all — no store, no call, nothing
    // that could have changed what is in there.
    let same_place = |a: Val, idx: Val, bi: usize, ii: usize| -> bool {
        if a == idx {
            return true;
        }
        let (aa, ia) = match (loaded_from.get(&a), loaded_from.get(&idx)) {
            (Some(x), Some(y)) if x == y => (*x, *y),
            _ => return false,
        };
        let _ = (aa, ia);
        let (ab, ai) = match site.get(&a) {
            Some(x) => *x,
            None => return false,
        };
        if preds[bi].len() != 1 || preds[bi][0] != ab {
            return false;
        }
        for k in (ai + 1)..f.blocks[ab].insts.len() {
            if writes_memory(&f.blocks[ab].insts[k].op) {
                return false;
            }
        }
        for k in 0..ii {
            if writes_memory(&f.blocks[bi].insts[k].op) {
                return false;
            }
        }
        true
    };
    // An upper bound known for `idx` from the branches on the way in.
    let bound_ok = |facts: &HashMap<Val, bool>, idx: Val, len: u64, bi: usize, ii: usize| -> bool {
        for (cond, truth) in facts.iter() {
            if !*truth {
                continue;
            }
            let (op, a, b) = match cmps.get(cond) {
                Some(x) => *x,
                None => continue,
            };
            // `idx < k` and `idx <= k`, plus the mirrored spellings
            // `k > idx` / `k >= idx` — the same fact written the other way
            // round.
            let (limit, strict) = match (op, same_place(a, idx, bi, ii), same_place(b, idx, bi, ii)) {
                (CmpOp::Lt, true, _) => (b, true),
                (CmpOp::Le, true, _) => (b, false),
                (CmpOp::Gt, _, true) => (a, true),
                (CmpOp::Ge, _, true) => (a, false),
                _ => continue,
            };
            let k = match consts.get(&limit) {
                Some(&k) => k,
                None => continue,
            };
            if k < 0 {
                continue;
            }
            let k = k as u128;
            if (strict && k <= len as u128) || (!strict && k < len as u128) {
                return true;
            }
        }
        false
    };
    // The decision is taken with `f` read only (the proof above walks it),
    // and only then written back — one pass cannot both look and change.
    let mut hits: Vec<(usize, usize, Val, Val)> = Vec::new();
    for bi in 0..f.blocks.len() {
        for ii in 0..f.blocks[bi].insts.len() {
            let (dst, idx, len) = match (&f.blocks[bi].insts[ii].dst, &f.blocks[bi].insts[ii].op) {
                (Some(d), Op::CheckedIdx { idx, len, .. }) => (*d, *idx, *len),
                _ => continue,
            };
            let inside = match consts.get(&idx) {
                Some(&v) => v >= 0 && (v as u128) < len as u128,
                None => bound_ok(&known[bi], idx, len, bi, ii),
            };
            if inside {
                hits.push((bi, ii, dst, idx));
            }
        }
    }
    if hits.is_empty() {
        return 0;
    }
    let mut map: HashMap<Val, Val> = HashMap::new();
    for (bi, ii, dst, idx) in hits {
        f.blocks[bi].insts[ii].op = Op::Const(0);
        map.insert(dst, idx);
    }
    let cnt = map.len();
    crate::mem2reg::replace_uses(f, &map);
    cnt
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
            // FLOATING POINT IS NEVER FOLDED. The value of an `Op::Const` with
            // `FTy::F64` is a BIT PATTERN; the folding here computes
            // integer wise and would turn `1.5 + 1.5` into silent nonsense.
            // Folding floating point needs its own evaluation that is
            // faithful to rounding — that comes with `comptime` (SPEC §8.6).
            if ty.is_float() || op_has_float(&op, f) {
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
                // ROUND 72, second pass -- the checked and the explicitly
                // unchecked forms fold too. Without this, `release-safe`
                // ran every pass and folded NOTHING: the folder matched
                // `Op::Bin` and the very arithmetic it was there for had
                // become `Op::CheckedBin` one round earlier
                // (`tests/opt/fold_arith.fi` measured it: folded at
                // `release-fast`, not folded at `release-safe`).
                //
                // A checked operation folds ONLY when the result really
                // fits. One that does not is left standing, because it is
                // supposed to abort at run time -- replacing it with a
                // constant would delete a panic the program promised.
                Op::CheckedBin { op: bop, a, b, .. } => {
                    match (consts.get(&a), consts.get(&b)) {
                        (Some(&x), Some(&y)) => fold_checked_bin(ty, bop, x, y),
                        _ => None,
                    }
                }
                // `fold_bin` already refuses `b == 0` and `MIN / -1`, which
                // are exactly the two cases this operation exists to catch.
                Op::CheckedDiv { op: bop, a, b, .. } => {
                    match (consts.get(&a), consts.get(&b)) {
                        (Some(&x), Some(&y)) => fold_bin(ty, bop, x, y),
                        _ => None,
                    }
                }
                Op::CheckedCast { src, from, .. } => match consts.get(&src) {
                    Some(&x) => fold_checked_cast(ty, from, x),
                    None => None,
                },
                Op::BinWrapSat { kind, op: bop, a, b } => {
                    match (consts.get(&a), consts.get(&b)) {
                        (Some(&x), Some(&y)) => fold_wrap_sat(ty, kind, bop, x, y),
                        _ => None,
                    }
                }
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
            // `a` is already normalized with the right sign: for unsigned
            // types not negative (-> logical shift), for signed ones
            // arithmetic.
            a >> b
        }
    };
    Some(ty.truncate(r))
}

/// **ROUND 72** - the range of `ty` as `(MIN, MAX)` in `i128`, which holds
/// every one of them exactly, `u64::MAX` included.
fn ty_range(ty: FTy) -> (i128, i128) {
    let bits = ty.bits();
    if ty.signed() {
        (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
    } else {
        (0, (1i128 << bits) - 1)
    }
}

/// **ROUND 72** - a CHECKED `+ - *` of two constants. `None` means "leave
/// the instruction where it is": either the operator is not one of the
/// three, or the result does not fit and the program is supposed to abort.
fn fold_checked_bin(ty: FTy, op: BinOp, a: i128, b: i128) -> Option<i128> {
    if ty == FTy::Void || ty.bits() == 0 || ty.is_float() {
        return None;
    }
    let a = ty.truncate(a);
    let b = ty.truncate(b);
    // `i128` holds every product of two 64 bit values, so the arithmetic
    // here is the MATHEMATICAL one and the range test below is the real
    // question, not an approximation of it.
    let r = match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        _ => return None,
    };
    let (lo, hi) = ty_range(ty);
    if r < lo || r > hi {
        return None;
    }
    Some(r)
}

/// **ROUND 72** - a CHECKED narrowing `as` of a constant. Folds only when
/// the value survives the conversion; otherwise the check has to run.
fn fold_checked_cast(to: FTy, from: FTy, a: i128) -> Option<i128> {
    if to.is_float() || from.is_float() || to == FTy::Void || from == FTy::Void {
        return None;
    }
    let v = from.truncate(a);
    let (lo, hi) = ty_range(to);
    if v < lo || v > hi {
        return None;
    }
    Some(v)
}

/// **ROUND 72** - `+% -% *%` and `+| -| *|` of two constants. Neither can
/// fail, so both always fold: wrapping keeps the low order bits, saturating
/// clamps to the type's own MIN/MAX.
fn fold_wrap_sat(
    ty: FTy,
    kind: crate::fir::WrapSatKind,
    op: BinOp,
    a: i128,
    b: i128,
) -> Option<i128> {
    if ty == FTy::Void || ty.bits() == 0 || ty.is_float() {
        return None;
    }
    let a = ty.truncate(a);
    let b = ty.truncate(b);
    let r = match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        _ => return None,
    };
    match kind {
        crate::fir::WrapSatKind::Wrap => Some(ty.truncate(r)),
        crate::fir::WrapSatKind::Sat => {
            let (lo, hi) = ty_range(ty);
            Some(r.clamp(lo, hi))
        }
    }
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
    // It only came out when the lexer written in Firn compiled `10.0`
    // and `firnc0` stood next to it — both token streams had to be equal,
    // and they were not. The path without the optimizer was right the
    // whole time (`cvtsi2sd`); only the folding lied.
    if to.is_float() || from.is_float() {
        if to == from {
            return Some(a);
        }
        // ROUND 71 — the two widths among each other. BOTH directions really
        // change the bits (`cvtss2sd`/`cvtsd2ss`); the narrowing rounds to
        // nearest-even, exactly as the instruction does.
        if to.is_float() && from.is_float() {
            if to == FTy::F64 {
                return Some((f32::from_bits(bits32(a)) as f64).to_bits() as i128);
            }
            let n = (f64::from_bits((a as u128) as u64) as f32).to_bits();
            return Some((n as u64) as i128);
        }
        if to == FTy::F32 {
            let x = from.truncate(a);
            let f = if from.signed() { x as f32 } else { (x as u128) as f32 };
            return Some((f.to_bits() as u64) as i128);
        }
        if to == FTy::F64 {
            let x = from.truncate(a);
            let f = if from.signed() { x as f64 } else { (x as u128) as f64 };
            return Some(f.to_bits() as i128);
        }
        if from == FTy::F32 {
            // f32 -> integer: cutting towards zero, like `cvttss2si`.
            let f = f32::from_bits(bits32(a));
            if !f.is_finite() {
                return None;
            }
            let t = f.trunc();
            let bits = to.bits();
            if bits == 0 || bits > 64 {
                return None;
            }
            if to.signed() {
                if t < -9.223372036854776e18f32 || t >= 9.223372036854776e18f32 {
                    return None;
                }
                return Some(to.truncate(t as i64 as i128));
            }
            if t < 0.0 || t >= 1.8446744073709552e19f32 {
                return None;
            }
            return Some(to.truncate((t as u64) as i128));
        }
        // f64 -> integer: cutting towards zero, like `cvttsd2si`.
        // Outside the target range, for NaN and for infinity the instruction
        // yields a special value — then NOTHING is folded, it is left to
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

/// Every value that is really read.
///
/// ROUND 92 -- A PHI DOES NOT KEEP ITS OPERANDS ALIVE ON ITS OWN.
///
/// Counting phi operands like any other use looks harmless and is not: two
/// phis in a loop read each other, so a loop counter nobody uses any more
/// holds ITSELF alive for ever and the dead code pass never gets rid of it.
/// The answer is the standard one: seed the set from what NON-phi code
/// reads, and let a phi hand its "used" on to its operands only once the phi
/// itself has been reached. A ring of phis nothing else touches is then
/// never reached and falls away completely.
///
/// `phi_edge` decides which entries count at all. `remove_unreachable_blocks`
/// passes a filter that ignores entries coming out of blocks it is about to
/// delete -- without it a phi would hold a value of a dead block alive and
/// block its own removal (measured: `dce` stopped dead on the first `while`
/// loop with a `break` in it).
fn collect_uses_filtered(
    f: &Func,
    blocks: &[usize],
    phi_edge: &dyn Fn(crate::fir::BlockId) -> bool,
) -> HashSet<Val> {
    let mut used: HashSet<Val> = HashSet::new();
    let mut phi_args: HashMap<Val, Vec<Val>> = HashMap::new();
    let mut work: Vec<Val> = Vec::new();
    let mut buf = Vec::new();
    for &bi in blocks {
        let b = &f.blocks[bi];
        for i in &b.insts {
            if let (Some(d), Op::Phi { incoming }) = (i.dst, &i.op) {
                phi_args.insert(
                    d,
                    incoming.iter().filter(|(p, _)| phi_edge(*p)).map(|(_, v)| *v).collect(),
                );
                continue;
            }
            buf.clear();
            i.op.uses(&mut buf);
            for v in buf.iter() {
                if used.insert(*v) {
                    work.push(*v);
                }
            }
        }
        let t = match &b.term {
            Term::BrCond { cond, .. } => Some(*cond),
            Term::Ret(Some(v)) => Some(*v),
            Term::Switch { val, .. } => Some(*val),
            Term::Br(_) | Term::Ret(None) | Term::Unset => None,
        };
        if let Some(v) = t {
            if used.insert(v) {
                work.push(v);
            }
        }
    }
    while let Some(v) = work.pop() {
        if let Some(args) = phi_args.get(&v) {
            for a in args.clone() {
                if used.insert(a) {
                    work.push(a);
                }
            }
        }
    }
    used
}

fn collect_uses(f: &Func, blocks: &[usize]) -> HashSet<Val> {
    collect_uses_filtered(f, blocks, &|_| true)
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

    // Safety net: if a value defined inside an unreachable block is still
    // read from reachable code (that would violate the SSA dominance),
    // NOTHING is removed — better dead code than a dangling
    // Val id.
    let live_idx: Vec<usize> = (0..f.blocks.len()).filter(|&i| reachable[i]).collect();
    let reach_of = reachable.clone();
    let used = collect_uses_filtered(f, &live_idx, &|p| {
        index_of.get(&p).map(|&i| reach_of[i]).unwrap_or(false)
    });
    let mut blocked = false;
    'net: for (i, b) in f.blocks.iter().enumerate() {
        if reachable[i] {
            continue;
        }
        for inst in &b.insts {
            if let Some(d) = inst.dst {
                if used.contains(&d) {
                    blocked = true;
                    break 'net;
                }
            }
        }
    }
    if blocked {
        // **ROUND SPEED** — the safety net holds, and it must not leave the
        // function in a state the back end cannot lay out.
        //
        // `merge_blocks` leaves its corpses as an EMPTY block with
        // `Term::Unset`, and counts on this pass to remove them. When the
        // net above says "remove nothing", they stay -- and `regalloc`
        // then reports `block bbN has no terminator` and the compilation
        // fails on a block that is never entered. Round 10 of this round
        // made that combination reachable (it turns joins unreachable, and
        // a value of a now dead block can still be named by another dead
        // block, which is exactly what trips the net).
        //
        // So an EMPTY unreachable block without a terminator is closed with
        // a jump to itself. It is unreachable, so the jump is never taken;
        // it names no value, so it changes no live range; and it is a block
        // every back end can emit. Blocks with instructions are left
        // untouched -- deleting a definition another instruction still
        // names is what round 10 already paid for once (`inline.rs` copies
        // dead code too).
        for i in 0..f.blocks.len() {
            if !reachable[i] && f.blocks[i].insts.is_empty()
                && matches!(f.blocks[i].term, Term::Unset)
            {
                let id = f.blocks[i].id;
                f.blocks[i].term = Term::Br(id);
            }
        }
        return false;
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
        // ROUND 92: the phi entries name BLOCKS, and the blocks have just
        // been renumbered. An entry whose block is gone goes with it -- the
        // edge it stood for does not exist any more.
        let np = b.phi_count();
        for i in b.insts[..np].iter_mut() {
            if let Op::Phi { incoming } = &mut i.op {
                incoming.retain(|(p, _)| new_id.contains_key(p));
                for e in incoming.iter_mut() {
                    e.0 = new_id[&e.0];
                }
                incoming.sort_by_key(|(p, _)| *p);
            }
        }
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
                    // pure instruction without a result: no effect
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
        // 3 < 4 is true: the else branch falls away, the then branch is merged
        // into bb0 — what is left is ONE block with `ret 1`.
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
        // What is removed: the unused constant 99, plus (new in round 2) the
        // dead local cell — the `load` is forwarded to the stored value, after
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

    /// ROUND 92 -- THIS TEST USED TO SAY THE OPPOSITE, AND IT WAS RIGHT TO.
    ///
    /// Until round 91 it was called `loop_stays_untouched_and_terminated`
    /// and it asserted that the optimizer removes NOTHING here: `i` is
    /// written on every pass, FIR had no phi nodes, so `mem2reg` could not
    /// touch the cell and the `load`/`store` pair stayed in the loop for
    /// ever. That was not a wish, it was the honest description of what the
    /// compiler did -- and it is exactly the wall round 92 was opened to
    /// take down.
    ///
    /// The same eleven instructions now come out as six, with the counter in
    /// a phi and not one memory access left in the body. What the test still
    /// insists on is the other half of the old name: the optimizer HALTS,
    /// and the loop is still a loop afterwards.
    #[test]
    fn loop_counter_becomes_a_phi() {
        // while (i < 10) { i = i + 1 }
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
        optimize(&mut m);
        let g = &m.funcs[0];
        assert_eq!(g.blocks.len(), blocks, "the loop is still a loop");
        assert!(g.inst_count() < before, "{} instructions, was {}", g.inst_count(), before);
        // Nothing touches memory any more.
        for b in &g.blocks {
            for i in &b.insts {
                assert!(
                    !matches!(i.op, Op::Alloca { .. } | Op::Load { .. } | Op::Store { .. }),
                    "the counter is still in the frame:\n{}",
                    m.to_text()
                );
            }
        }
        // ... and the counter really is a phi in the loop head.
        assert!(
            m.funcs[0].blocks.iter().any(|b| b.insts.iter().any(|i| matches!(i.op, Op::Phi { .. }))),
            "{}",
            m.to_text()
        );
        assert!(m.funcs[0].verify_phis().is_ok(), "{:?}", m.funcs[0].verify_phis());
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

/// Does a floating point value show up anywhere in this instruction? For
/// constant folding that rules it out (see `fold_constants`).
fn op_has_float(op: &Op, f: &Func) -> bool {
    match op {
        Op::Cmp { ty, .. } => ty.is_float(),
        Op::Cast { from, .. } => from.is_float(),
        Op::Bin(_, a, b) => f.val_ty(*a).is_float() || f.val_ty(*b).is_float(),
        Op::Un(_, a) => f.val_ty(*a).is_float(),
        _ => false,
    }
}

/// **ROUND 71** — a 32-bit pattern out of the i128 an `Op::Const` carries.
fn bits32(a: i128) -> u32 {
    (a as u128) as u32
}
