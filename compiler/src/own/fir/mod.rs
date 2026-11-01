pub(crate) mod bits;
pub(crate) mod flow;
pub(crate) mod index;
pub(crate) mod insert;
pub(crate) mod live;
pub(crate) mod registry;
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

pub(crate) fn run(m: &mut Module) {
    let free_fn = registry::free_fn(super::mode());
    if !m.funcs.iter().any(|f| f.name == free_fn) {
        return;
    }
    let t0 = std::time::Instant::now();
    let (mut sites, mut frees) = (0usize, 0usize);
    let roots: Vec<usize> = (0..m.funcs.len()).filter(|&i| has_producer_call(&m.funcs[i])).collect();
    if roots.is_empty() {
        report(sites, frees, t0);
        return;
    }
    let ts = std::time::Instant::now();
    let sums = summary::compute(m, &roots);
    let sum_ms = ts.elapsed().as_millis();
    for &fi in roots.iter() {
        let f = &mut m.funcs[fi];
        if f.constant_time || f.interrupt || !summary::analyzable(f) || f.name.starts_with("__gc_") || f.name.starts_with("__own_") {
            continue;
        }
        let ix = index::Index::build(f);
        let sl = slots::Slots::build(f, &ix);
        let mut fl = flow::Flow::build(f, &ix, &sl, &sums, true);
        if fl.sites.is_empty() || fl.sites.len().saturating_mul(f.inst_count()) > summary::MAX_SITE_WORK {
            continue;
        }
        sites += fl.sites.len();
        let d = live::deaths(f, &ix, &sl, &mut fl, &sums);
        frees += insert::apply(f, &sl, d, free_fn);
    }
    if std::env::var_os("FIRN_OWN_STATS").is_some() {
        eprintln!("ownership fir: summaries {} ms", sum_ms);
    }
    report(sites, frees, t0);
}

fn has_producer_call(f: &crate::fir::Func) -> bool {
    f.blocks.iter().any(|b| b.insts.iter().any(|i| matches!(&i.op, Op::Call { name, .. } if registry::producer(name).is_some())))
}

fn report(sites: usize, frees: usize, t0: std::time::Instant) {
    if std::env::var_os("FIRN_OWN_STATS").is_some() {
        eprintln!("ownership fir: {} sites, {} frees, {} ms", sites, frees, t0.elapsed().as_millis());
    }
}
