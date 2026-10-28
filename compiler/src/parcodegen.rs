// SPDX-License-Identifier: MPL-2.0
//! Per-function parallel driver for the x86-64 code generator.
//!
//! Every function is emitted by a worker into a private `Emitter`. Four kinds
//! of compilation-wide numbering leak into the text, and each one is made
//! local to a function and renumbered when the results are put together in
//! function order:
//!
//!  * `.Lfconst<n>`    float pool (`fpool`)
//!  * `.Lpanicmsg<n>`  panic message table (`panic_rt`)
//!  * `.Lmcpy_*<n>`    copy idiom sites (`memcpy_idiom`)
//!  * `.Lcpuid_*_<n>`  cpuid sequence labels (`simd`)
//!
//! The first occurrence order of the serial run is the concatenation of the
//! per-function first occurrence orders, so the renumbered text is byte
//! identical to the serial one.
//!
//! `FIRN_CODEGEN_THREADS=n` picks the thread count (default: min(8, cores),
//! `1` = the serial loop in `codegen_x86::emit`).

use crate::codegen_x86::{self, Emitter};
use crate::dwarf_info::FnInfo;
use crate::fir::Func;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

const MIN_FUNCS: usize = 32;
const WORKER_STACK: usize = 128 << 20;

pub(crate) struct Emitted {
    pub(crate) text: String,
    pub(crate) debug_funcs: Vec<FnInfo>,
}

pub(crate) struct Effects {
    pool: Vec<(u64, bool)>,
    msgs: Vec<String>,
    index_used: bool,
    sites: usize,
    cpuid: u64,
}

impl Effects {
    fn begin() {
        crate::fpool::reset();
        crate::panic_rt::take_local();
        crate::memcpy_idiom::sites_set(0);
        crate::simd::labels_set(0);
    }

    fn collect() -> Effects {
        let (msgs, index_used) = crate::panic_rt::take_local();
        Effects {
            pool: crate::fpool::take(),
            msgs,
            index_used,
            sites: crate::memcpy_idiom::sites_get(),
            cpuid: crate::simd::labels_get(),
        }
    }
}

struct Local {
    emitted: Emitted,
    fx: Effects,
}

struct Ctx {
    target: crate::target::State,
    externs: crate::extfn::Snapshot,
    profile: crate::prof::Profile,
    checked: bool,
    runtime: bool,
    handler: Option<String>,
}

impl Ctx {
    fn capture() -> Ctx {
        Ctx {
            target: crate::target::state(),
            externs: crate::extfn::snapshot(),
            profile: crate::prof::active(),
            checked: crate::checkmode::is_checked(),
            runtime: crate::gc::runtime_active(),
            handler: crate::panic_rt::handler(),
        }
    }

    fn install(&self) {
        crate::target::set_state(self.target);
        crate::extfn::install(&self.externs);
        crate::prof::set_active(self.profile);
        crate::checkmode::set_checked(self.checked);
        crate::gc::runtime_set(self.runtime);
        crate::panic_rt::set_handler(self.handler.clone());
    }
}

fn min_funcs() -> usize {
    std::env::var("FIRN_CODEGEN_MIN_FUNCS").ok().and_then(|v| v.parse().ok()).unwrap_or(MIN_FUNCS)
}

pub(crate) fn enabled(nfuncs: usize) -> bool {
    nfuncs >= min_funcs()
        && !crate::target::windows()
        && matches!(crate::target::active(), crate::target::Target::X86_64)
        && crate::parfn::threads_from("FIRN_CODEGEN_THREADS") > 1
}

fn run_ordered<T, R, F>(items: Vec<T>, order: &[usize], ctx: Option<&Ctx>, work: F) -> Vec<R>
where
    T: Send,
    R: Send,
    F: Fn(T) -> R + Sync,
{
    let n = items.len();
    let nthreads = crate::parfn::threads_from("FIRN_CODEGEN_THREADS").min(n).max(1);
    let inputs: Vec<Mutex<Option<T>>> = items.into_iter().map(|t| Mutex::new(Some(t))).collect();
    let outputs: Vec<Mutex<Option<R>>> = (0..n).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    std::thread::scope(|sc| {
        let mut hs = Vec::new();
        for _ in 0..nthreads {
            let (inputs, outputs, next, work) = (&inputs, &outputs, &next, &work);
            let h = std::thread::Builder::new()
                .stack_size(WORKER_STACK)
                .spawn_scoped(sc, move || {
                    if let Some(c) = ctx {
                        c.install();
                    }
                    loop {
                        let k = next.fetch_add(1, Ordering::Relaxed);
                        if k >= order.len() {
                            break;
                        }
                        let i = order[k];
                        let item = inputs[i].lock().unwrap().take().unwrap();
                        let r = work(item);
                        *outputs[i].lock().unwrap() = Some(r);
                    }
                })
                .expect("spawn codegen worker");
            hs.push(h);
        }
        for h in hs {
            if let Err(e) = h.join() {
                std::panic::resume_unwind(e);
            }
        }
    });
    outputs.into_iter().map(|m| m.into_inner().unwrap().unwrap()).collect()
}

pub(crate) fn map_owned<T, R, F>(items: Vec<T>, weights: Vec<usize>, work: F) -> Vec<(R, Effects)>
where
    T: Send,
    R: Send,
    F: Fn(T) -> R + Sync,
{
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| weights[b].cmp(&weights[a]).then(a.cmp(&b)));
    let ctx = Ctx::capture();
    run_ordered(items, &order, Some(&ctx), |t| {
        Effects::begin();
        let r = work(t);
        (r, Effects::collect())
    })
}

fn emit_one(f: &Func, fsec: bool) -> Result<Local, String> {
    Effects::begin();
    let mut e = Emitter::fresh();
    if fsec {
        e.raw(&format!(".section .text.{},\"ax\",@progbits", codegen_x86::label(&f.name)));
    }
    codegen_x86::emit_func(&mut e, f)?;
    Ok(Local { emitted: Emitted { text: e.out, debug_funcs: e.debug_funcs }, fx: Effects::collect() })
}

struct Shift {
    pool: Vec<usize>,
    msgs: Vec<usize>,
    sites: usize,
    cpuid: u64,
}

impl Shift {
    fn is_identity(&self) -> bool {
        self.sites == 0
            && self.cpuid == 0
            && self.pool.iter().enumerate().all(|(i, g)| i == *g)
            && self.msgs.iter().enumerate().all(|(i, g)| i == *g)
    }
}

const PREFIXES: [(&str, u8); 7] = [
    ("fconst", 0),
    ("panicmsg", 1),
    ("mcpy_small", 2),
    ("mcpy_big", 2),
    ("mcpy_done", 2),
    ("cpuid_done_", 3),
    ("cpuid_no7_", 3),
];

fn relocate(text: &str, shift: &Shift) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 64);
    let mut last = 0usize;
    let mut from = 0usize;
    while let Some(p) = text[from..].find(".L") {
        let at = from + p + 2;
        from = at;
        let rest = &text[at..];
        let Some((prefix, kind)) = PREFIXES.iter().find(|(pre, _)| rest.starts_with(pre)) else {
            continue;
        };
        let digits_at = at + prefix.len();
        let len = text[digits_at..].bytes().take_while(|b| b.is_ascii_digit()).count();
        if len == 0 {
            continue;
        }
        let Ok(n) = text[digits_at..digits_at + len].parse::<usize>() else { continue };
        let new = match kind {
            0 => shift.pool.get(n).copied(),
            1 => shift.msgs.get(n).copied(),
            2 => Some(n + shift.sites),
            _ => Some(n + shift.cpuid as usize),
        };
        let Some(new) = new else { continue };
        out.push_str(&text[last..digits_at]);
        out.push_str(&new.to_string());
        last = digits_at + len;
        from = last;
    }
    out.push_str(&text[last..]);
    out
}

pub(crate) fn emit_all(funcs: &[&Func], fsec: bool, pre: Vec<Option<Effects>>) -> Result<Vec<Emitted>, String> {
    let clock = std::time::Instant::now();
    let timings = std::env::var_os("FIRN_STAGE_TIMINGS").is_some();
    let mut order: Vec<usize> = (0..funcs.len()).collect();
    let w: Vec<usize> = funcs.iter().map(|f| crate::parfn::weight(f)).collect();
    order.sort_by(|&a, &b| w[b].cmp(&w[a]).then(a.cmp(&b)));
    let ctx = Ctx::capture();
    let results = run_ordered(funcs.to_vec(), &order, Some(&ctx), |f| emit_one(f, fsec));
    let t_emit = clock.elapsed();

    let mut locals: Vec<Local> = Vec::with_capacity(results.len());
    for r in results {
        locals.push(r?);
    }
    let empty = Effects { pool: Vec::new(), msgs: Vec::new(), index_used: false, sites: 0, cpuid: 0 };
    let mut pool_parts: Vec<&[(u64, bool)]> = Vec::with_capacity(locals.len() * 2);
    let mut msg_parts: Vec<&[String]> = Vec::with_capacity(locals.len() * 2);
    let mut fxs: Vec<&Effects> = Vec::with_capacity(locals.len() * 2);
    for (l, p) in locals.iter().zip(&pre) {
        let p = p.as_ref().unwrap_or(&empty);
        for fx in [p, &l.fx] {
            pool_parts.push(fx.pool.as_slice());
            msg_parts.push(fx.msgs.as_slice());
            fxs.push(fx);
        }
    }
    let pool_maps = crate::fpool::merge_all(&pool_parts);
    let msg_maps = crate::panic_rt::merge_all(&msg_parts);
    let mut sites = crate::memcpy_idiom::sites_get();
    let mut cpuid = crate::simd::labels_get();
    let mut shifts: Vec<Shift> = Vec::with_capacity(locals.len());
    for (k, fx) in fxs.iter().enumerate() {
        if k % 2 == 1 {
            shifts.push(Shift { pool: pool_maps[k].clone(), msgs: msg_maps[k].clone(), sites, cpuid });
        }
        sites += fx.sites;
        cpuid += fx.cpuid;
        if fx.index_used {
            crate::panic_rt::note_index_site();
        }
    }
    crate::memcpy_idiom::sites_set(sites);
    crate::simd::labels_set(cpuid);
    let t_maps = clock.elapsed();

    let mut pending: Vec<(usize, String, Shift)> = Vec::new();
    let mut texts: Vec<Option<String>> = Vec::with_capacity(locals.len());
    let mut debug: Vec<Vec<FnInfo>> = Vec::with_capacity(locals.len());
    for (i, (l, s)) in locals.into_iter().zip(shifts).enumerate() {
        debug.push(l.emitted.debug_funcs);
        if s.is_identity() {
            texts.push(Some(l.emitted.text));
        } else {
            texts.push(None);
            pending.push((i, l.emitted.text, s));
        }
    }
    if !pending.is_empty() {
        let mut order: Vec<usize> = (0..pending.len()).collect();
        order.sort_by(|&a, &b| pending[b].1.len().cmp(&pending[a].1.len()).then(a.cmp(&b)));
        let idx: Vec<usize> = pending.iter().map(|p| p.0).collect();
        let done = run_ordered(pending, &order, None, |(_, text, shift)| relocate(&text, &shift));
        for (i, t) in idx.into_iter().zip(done) {
            texts[i] = Some(t);
        }
    }
    if timings {
        eprintln!(
            "  codegen x86 parallel: emit {:.0} ms, renumber {:.0} ms, relocate {:.0} ms",
            t_emit.as_secs_f64() * 1e3,
            (t_maps - t_emit).as_secs_f64() * 1e3,
            (clock.elapsed() - t_maps).as_secs_f64() * 1e3
        );
    }
    Ok(texts
        .into_iter()
        .zip(debug)
        .map(|(t, d)| Emitted { text: t.unwrap(), debug_funcs: d })
        .collect())
}
