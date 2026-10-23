// SPDX-License-Identifier: MPL-2.0
//! Per-function parallel driver for the optimizer (r948).
//!
//! The passes of `opt::optimize_func` are independent per function. They read
//! four pieces of per-compilation state that live in `thread_local`s of the
//! compiling thread, and one of them (`ivsr::TOUCHED`) is also written:
//!
//!  * `target::State`          read-only, copied
//!  * `statics` rodata sizes   read-only, shared through an `Arc` map
//!  * `rangecheck::LEN_GETTERS` read-only, copied
//!  * `ivsr::TOUCHED`          keyed by function NAME; a worker starts with a
//!    copy, adds the names of the functions it changed, and the sets are
//!    merged back into the compiling thread afterwards
//!
//! Output is byte identical to the serial run because (a) every task is one
//! function and no pass looks at another function, (b) the statistics are
//! plain sums, (c) TOUCHED is only ever asked about the function being
//! processed, and the batch is serial when two tasks share a name.
//!
//! `FIRN_OPT_THREADS=n` picks the thread count (default: min(4, cores),
//! `1` = serial).

use crate::fir::Func;
use crate::fx::{HashMap, HashSet};
use crate::opt::OptStats;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Below this many tasks a batch is not worth the thread start.
const MIN_TASKS: usize = 8;
/// Virtual stack of a worker (pages are only touched when used).
const WORKER_STACK: usize = 128 << 20;

/// How many threads a batch may use.
pub(crate) fn threads() -> usize {
    if let Some(n) = std::env::var("FIRN_OPT_THREADS").ok().and_then(|v| v.parse::<usize>().ok()) {
        return n.clamp(1, 64);
    }
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(4)
}

/// The state a worker needs from the compiling thread.
struct Ctx {
    target: crate::target::State,
    rodata: std::sync::Arc<HashMap<String, u64>>,
    len_getters: HashMap<String, i128>,
    touched: HashSet<String>,
}

impl Ctx {
    fn capture() -> Ctx {
        Ctx {
            target: crate::target::state(),
            rodata: crate::statics::rodata_sizes(),
            len_getters: crate::rangecheck::len_getters_get(),
            touched: crate::ivsr::touched_get(),
        }
    }

    fn install(&self) {
        crate::target::set_state(self.target);
        crate::statics::rodata_sizes_install(self.rodata.clone());
        crate::rangecheck::len_getters_set(self.len_getters.clone());
        crate::ivsr::touched_set(self.touched.clone());
    }
}

fn weight(f: &Func) -> usize {
    f.blocks.iter().map(|b| b.insts.len() + 1).sum()
}

/// Run `work` on every function of `tasks`; `reps[i]` times on task `i`
/// (the serial driver runs a function twice when it is listed twice).
/// Returns the summed statistics of all `work` calls.
///
/// Serial (same thread, same order) when only one thread is allowed, the
/// batch is small, or two tasks carry the same function name.
pub(crate) fn run<F>(tasks: Vec<(&mut Func, usize)>, work: F) -> OptStats
where
    F: Fn(&mut Func, &mut OptStats) + Sync,
{
    let mut total = OptStats::default();
    let nthreads = threads();
    let serial = nthreads <= 1 || tasks.len() < MIN_TASKS || {
        let mut seen: HashSet<&str> = HashSet::default();
        tasks.iter().any(|(f, _)| !seen.insert(f.name.as_str()))
    };
    if serial {
        for (f, reps) in tasks {
            for _ in 0..reps {
                work(f, &mut total);
            }
        }
        return total;
    }
    // Biggest first: the long tail is made of small functions.
    let mut order: Vec<usize> = (0..tasks.len()).collect();
    let w: Vec<usize> = tasks.iter().map(|(f, _)| weight(f)).collect();
    order.sort_by(|&a, &b| w[b].cmp(&w[a]).then(a.cmp(&b)));
    let slots: Vec<Mutex<(&mut Func, usize)>> = tasks.into_iter().map(Mutex::new).collect();
    let next = AtomicUsize::new(0);
    let ctx = Ctx::capture();
    let nthreads = nthreads.min(slots.len());
    let mut outs: Vec<(OptStats, HashSet<String>)> = Vec::new();
    std::thread::scope(|sc| {
        let mut hs = Vec::new();
        for _ in 0..nthreads {
            let (slots, order, next, ctx, work) = (&slots, &order, &next, &ctx, &work);
            let h = std::thread::Builder::new()
                .stack_size(WORKER_STACK)
                .spawn_scoped(sc, move || {
                    ctx.install();
                    let mut st = OptStats::default();
                    loop {
                        let k = next.fetch_add(1, Ordering::Relaxed);
                        if k >= order.len() {
                            break;
                        }
                        let mut g = slots[order[k]].lock().unwrap();
                        let reps = g.1;
                        for _ in 0..reps {
                            work(&mut *g.0, &mut st);
                        }
                    }
                    (st, crate::ivsr::touched_get())
                })
                .expect("spawn optimizer worker");
            hs.push(h);
        }
        for h in hs {
            match h.join() {
                Ok(o) => outs.push(o),
                Err(e) => std::panic::resume_unwind(e),
            }
        }
    });
    for (st, touched) in outs {
        total.add(&st);
        crate::ivsr::touched_extend(touched);
    }
    total
}
