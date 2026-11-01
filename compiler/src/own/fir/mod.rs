pub(crate) mod bits;
pub(crate) mod flow;
pub(crate) mod index;
pub(crate) mod insert;
pub(crate) mod live;
pub(crate) mod registry;
pub(crate) mod reuse;
pub(crate) mod reuse_apply;
pub(crate) mod sites;
pub(crate) mod slots;
pub(crate) mod summary;

use crate::fir::{Module, Op};
use crate::opt::Level;

pub(crate) fn active(level: Level) -> bool {
    super::enabled()
        && level != Level::Dev
        && matches!(crate::target::active(), crate::target::Target::X86_64 | crate::target::Target::Aarch64)
        && !crate::prof::is_kernel()
        && std::env::var_os("FIRN_OWN_NO_FIR").is_none()
}

pub(crate) fn root_names() -> Vec<String> {
    if super::enabled() {
        registry::root_names()
    } else {
        Vec::new()
    }
}

pub(crate) fn prepare(m: &mut Module) {
    let defined: crate::fx::HashSet<String> = m.funcs.iter().map(|f| f.name.clone()).collect();
    let renames: Vec<(&str, &str)> = registry::RENAMES.iter().copied().filter(|(_, to)| defined.contains(*to)).collect();
    for f in m.funcs.iter_mut() {
        if registry::producer(&f.name).is_some() {
            f.inline_hint = Some(false);
        }
        if renames.iter().any(|(_, to)| *to == f.name) {
            continue;
        }
        for b in f.blocks.iter_mut() {
            for inst in b.insts.iter_mut() {
                if let Op::Call { name, .. } = &mut inst.op {
                    if let Some((_, to)) = renames.iter().find(|(from, _)| *from == name.as_str()) {
                        *name = to.to_string();
                    }
                }
            }
        }
    }
}

pub(crate) fn run(m: &mut Module) -> Vec<usize> {
    let mode = super::mode();
    let free_fn = registry::free_fn(mode);
    if !m.funcs.iter().any(|f| f.name == free_fn) {
        return Vec::new();
    }
    let owner_fn = registry::drop_fn(registry::Drop::SlotsOwner, mode);
    let owner_fn = if m.funcs.iter().any(|f| f.name == owner_fn) { owner_fn } else { free_fn };
    let reuse_on = mode == super::Mode::On && std::env::var_os("FIRN_OWN_NO_REUSE").is_none();
    let reuse_calls = m.funcs.iter().any(|f| f.name == registry::REUSE_ALLOC);
    let t0 = std::time::Instant::now();
    let (mut sites, mut frees, mut reuses) = (0usize, 0usize, 0usize);
    let base: Vec<usize> = (0..m.funcs.len()).filter(|&i| has_producer_call(&m.funcs[i])).collect();
    if base.is_empty() {
        report(sites, frees, reuses, t0);
        return Vec::new();
    }
    let ts = std::time::Instant::now();
    let (sums, roots, st) = summary::compute(m, &base);
    let sum_ms = ts.elapsed().as_millis();
    let ta = std::time::Instant::now();
    let plans = crate::parfn::map_slice(&roots, |&fi| analyze(&m.funcs[fi], &sums, reuse_on, reuse_calls));
    let ana_ms = ta.elapsed().as_millis();
    let defined: crate::fx::HashSet<String> = m.funcs.iter().map(|f| f.name.clone()).collect();
    let mut reverted: Vec<usize> = Vec::new();
    let window = root_window();
    for (pos, (&fi, plan)) in roots.iter().zip(plans).enumerate() {
        let f = &mut m.funcs[fi];
        sites += plan.sites;
        let mut plan = plan;
        if plan.pending.is_some() && std::env::var_os("FIRN_OWN_TRACE").is_some() {
            let kinds = plan.pending.as_ref().map(|p| format!("{:?}", p.2)).unwrap_or_default();
            eprintln!("ownership fir: root {} {} {}", pos, f.name, kinds);
        }
        if pos < window.0 || pos >= window.1 {
            plan.pending = None;
        }
        if revert_calls(f, &plan.kept, &defined) {
            reverted.push(fi);
        }
        if let Some((sl, d, drops, plans, allocs, exits)) = plan.pending {
            let fns: Vec<&str> = drops.iter().map(|d| if *d == registry::Drop::SlotsOwner { owner_fn } else { free_fn }).collect();
            let marked: crate::fx::HashSet<usize> = plans.iter().map(|p| p.free).collect();
            let (n, handles) = insert::apply(f, &sl, d, &fns, &marked);
            frees += n;
            reuses += reuse_apply::apply(f, &plans, &allocs, &handles, &exits, free_fn);
        }
    }
    if std::env::var_os("FIRN_OWN_STATS").is_some() {
        eprintln!(
            "ownership fir: summaries {} ms ({} borrow evals in {} waves, {} fresh evals), {} roots analyzed in {} ms",
            sum_ms,
            st.borrow_evals,
            st.waves,
            st.fresh_evals,
            roots.len(),
            ana_ms
        );
    }
    report(sites, frees, reuses, t0);
    if reverted.is_empty() {
        return reverted;
    }
    let changed = inline_reverted(m);
    if std::env::var_os("FIRN_OWN_STATS").is_some() {
        eprintln!("ownership fir: reverted calls in {} functions, {} changed by inlining", reverted.len(), changed.len());
    }
    changed
}

fn root_window() -> (usize, usize) {
    let parse = |v: &str| -> Option<(usize, usize)> {
        let (lo, hi) = v.split_once(':')?;
        Some((lo.parse().ok()?, hi.parse().ok()?))
    };
    std::env::var("FIRN_OWN_ROOT_WINDOW").ok().and_then(|v| parse(&v)).unwrap_or((0, usize::MAX))
}

struct Plan {
    kept: crate::fx::HashSet<(usize, usize)>,
    pending: Option<(slots::Slots, Vec<live::Free>, Vec<registry::Drop>, Vec<reuse::Plan>, Vec<reuse::Alloc>, Vec<Vec<(usize, usize)>>)>,
    sites: usize,
}

fn analyze(f: &crate::fir::Func, sums: &summary::Summaries, reuse_on: bool, reuse_calls: bool) -> Plan {
    let mut plan = Plan { kept: Default::default(), pending: None, sites: 0 };
    if summary::skipped(f) {
        return plan;
    }
    let ix = index::Index::build(f);
    let sl = slots::Slots::build(f, &ix);
    let mut fl = flow::Flow::build(f, &ix, &sl, sums, true);
    if fl.sites.is_empty() || fl.sites.len().saturating_mul(f.inst_count()) > summary::MAX_SITE_WORK {
        return plan;
    }
    plan.sites = fl.sites.len();
    let (d, freed) = live::deaths(f, &ix, &sl, &mut fl, sums);
    for (&at, &k) in fl.site_at.iter() {
        if freed[k] {
            plan.kept.insert(at);
        }
    }
    let (plans, allocs) = if reuse_on {
        let classes = reuse::site_classes(f, &ix, &fl.sites, &fl.drops, &fl.site_at);
        let allocs = reuse::allocs(f, &ix, reuse_calls);
        (reuse::plan(f, &ix, &d, &classes, &allocs), allocs)
    } else {
        (Vec::new(), Vec::new())
    };
    let exits: Vec<Vec<(usize, usize)>> = plans.iter().map(|p| reuse::side_exits(&ix, &p.path)).collect();
    plan.pending = Some((sl, d, fl.drops.clone(), plans, allocs, exits));
    plan
}

fn revert_calls(f: &mut crate::fir::Func, kept: &crate::fx::HashSet<(usize, usize)>, defined: &crate::fx::HashSet<String>) -> bool {
    let mut any = false;
    for (bi, b) in f.blocks.iter_mut().enumerate() {
        for (ii, inst) in b.insts.iter_mut().enumerate() {
            if kept.contains(&(bi, ii)) {
                continue;
            }
            if let Op::Call { name, .. } = &mut inst.op {
                if let Some((from, _)) = registry::RENAMES.iter().find(|(_, to)| *to == name.as_str()) {
                    if defined.contains(*from) {
                        *name = from.to_string();
                        any = true;
                    }
                }
            }
        }
    }
    any
}

fn inline_reverted(m: &mut Module) -> Vec<usize> {
    let mut saved: Vec<(usize, Option<bool>)> = Vec::new();
    for (i, f) in m.funcs.iter_mut().enumerate() {
        let mine = registry::RENAMES.iter().any(|(from, _)| *from == f.name);
        if mine || f.inline_hint == Some(true) {
            saved.push((i, f.inline_hint));
            f.inline_hint = if mine { Some(true) } else { None };
        }
    }
    let mut changed = Vec::new();
    crate::inline::inline_requested_tracked(m, &mut changed);
    for (i, h) in saved {
        m.funcs[i].inline_hint = h;
    }
    changed.sort_unstable();
    changed.dedup();
    changed
}

fn has_producer_call(f: &crate::fir::Func) -> bool {
    f.blocks.iter().any(|b| b.insts.iter().any(|i| matches!(&i.op, Op::Call { name, .. } if registry::producer(name).is_some() || registry::is_gc_alloc(name))))
}

fn report(sites: usize, frees: usize, reuses: usize, t0: std::time::Instant) {
    if std::env::var_os("FIRN_OWN_STATS").is_some() {
        eprintln!("ownership fir: {} sites, {} frees, {} reuses, {} ms", sites, frees, reuses, t0.elapsed().as_millis());
    }
}
