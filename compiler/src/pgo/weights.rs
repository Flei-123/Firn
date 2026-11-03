// SPDX-License-Identifier: MPL-2.0

use crate::fir::{Func, Loc};
use crate::fx::HashMap;
use std::sync::OnceLock;

pub const HOT_FRACTION: u64 = 64;
pub const HOT_BOOST: f64 = 8.0;

#[derive(Default)]
pub struct Weights {
    pub locs: HashMap<(u32, u32, u32), u64>,
    pub entries: HashMap<String, u64>,
    pub max: u64,
    pub stale_funcs: usize,
    pub used_funcs: usize,
}

static WEIGHTS: OnceLock<Option<Weights>> = OnceLock::new();

fn build() -> Option<Weights> {
    let path = super::use_path()?;
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("warning: profile {}: {}", path.display(), e);
            return None;
        }
    };
    let prof = match super::format::parse(&bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("warning: {} ({}), profile ignored", e, path.display());
            return None;
        }
    };
    let current = crate::dwarf::files();
    let file_map: Vec<Option<u32>> = prof
        .desc
        .files
        .iter()
        .map(|fe| {
            let idx = current.iter().position(|p| *p == fe.path)?;
            (super::hash::file_hash(&current[idx]) == fe.hash).then_some(idx as u32)
        })
        .collect();
    let mut w = Weights::default();
    let mut k = 0usize;
    for fe in &prof.desc.funcs {
        let n = fe.blocks.len();
        let counts = &prof.counts[k..k + n];
        k += n;
        if super::hash::of(&fe.name) != Some(fe.hash) {
            w.stale_funcs += 1;
            continue;
        }
        w.used_funcs += 1;
        if let Some(&c) = counts.first() {
            w.entries.insert(fe.name.clone(), c);
        }
        for (locs, &c) in fe.blocks.iter().zip(counts.iter()) {
            for &(fi, l, col) in locs {
                if let Some(Some(cur)) = file_map.get(fi as usize) {
                    let e = w.locs.entry((*cur, l, col)).or_insert(0);
                    *e = e.saturating_add(c);
                }
            }
        }
    }
    w.max = w.locs.values().copied().max().unwrap_or(0);
    if std::env::var_os("FIRN_PGO_LOG").is_some() {
        eprintln!(
            "pgo: {} functions used, {} stale, {} source positions, max count {}",
            w.used_funcs,
            w.stale_funcs,
            w.locs.len(),
            w.max
        );
    }
    Some(w)
}

pub fn get() -> Option<&'static Weights> {
    if super::use_path().is_none() {
        return None;
    }
    WEIGHTS.get_or_init(build).as_ref()
}

pub fn loc_count(l: Loc) -> Option<u64> {
    if l.is_none() {
        return None;
    }
    get()?.locs.get(&(l.file, l.line, l.col)).copied()
}

pub fn block_weight(f: &Func, bi: usize) -> Option<u64> {
    let w = get()?;
    let mut best: Option<u64> = None;
    for i in &f.blocks[bi].insts {
        if i.loc.is_none() {
            continue;
        }
        if let Some(&c) = w.locs.get(&(i.loc.file, i.loc.line, i.loc.col)) {
            best = Some(best.map_or(c, |b| b.max(c)));
        }
    }
    best
}

pub fn block_weights(f: &Func) -> Option<Vec<Option<u64>>> {
    get()?;
    let v: Vec<Option<u64>> = (0..f.blocks.len()).map(|bi| block_weight(f, bi)).collect();
    if v.iter().all(|x| x.is_none()) {
        return None;
    }
    Some(v)
}

pub fn is_hot(c: u64) -> bool {
    match get() {
        Some(w) => w.max > 0 && c.saturating_mul(HOT_FRACTION) >= w.max,
        None => false,
    }
}

pub fn site_scale(base: f64, l: Loc) -> f64 {
    match loc_count(l) {
        None => base,
        Some(0) => 0.0,
        Some(c) if is_hot(c) => base * HOT_BOOST,
        Some(_) => base,
    }
}

pub fn func_cold(f: &Func) -> bool {
    match block_weights(f) {
        Some(v) => v.iter().all(|x| matches!(x, None | Some(0))),
        None => false,
    }
}
