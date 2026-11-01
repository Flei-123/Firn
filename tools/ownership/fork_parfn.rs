use crate::fir::Func;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) fn threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(8)
}

pub(crate) fn map_slice<T, R, F>(items: &[T], work: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    map_slice_with(threads(), items, work)
}

pub(crate) fn map_slice_with<T, R, F>(max_threads: usize, items: &[T], work: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    let nthreads = max_threads.min(items.len() / 64);
    if nthreads <= 1 {
        return items.iter().map(&work).collect();
    }
    let chunk = (items.len() + nthreads * 4 - 1) / (nthreads * 4);
    let nchunks = (items.len() + chunk - 1) / chunk;
    let next = AtomicUsize::new(0);
    let mut parts: Vec<(usize, Vec<R>)> = Vec::with_capacity(nchunks);
    std::thread::scope(|sc| {
        let mut hs = Vec::new();
        for _ in 0..nthreads {
            let (next, work) = (&next, &work);
            hs.push(sc.spawn(move || {
                let mut got: Vec<(usize, Vec<R>)> = Vec::new();
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    if k >= nchunks {
                        break;
                    }
                    let lo = k * chunk;
                    let hi = (lo + chunk).min(items.len());
                    got.push((k, items[lo..hi].iter().map(work).collect()));
                }
                got
            }));
        }
        for h in hs {
            match h.join() {
                Ok(g) => parts.extend(g),
                Err(e) => std::panic::resume_unwind(e),
            }
        }
    });
    parts.sort_by_key(|(k, _)| *k);
    parts.into_iter().flat_map(|(_, v)| v).collect()
}

pub(crate) fn map_weighted<T, R, F, W>(items: &[T], weight: W, work: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
    W: Fn(&T) -> usize,
{
    let nthreads = threads().min(items.len());
    if nthreads <= 1 {
        return items.iter().map(&work).collect();
    }
    let w: Vec<usize> = items.iter().map(&weight).collect();
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| w[b].cmp(&w[a]).then(a.cmp(&b)));
    let next = AtomicUsize::new(0);
    let mut parts: Vec<(usize, R)> = Vec::with_capacity(items.len());
    std::thread::scope(|sc| {
        let mut hs = Vec::new();
        for _ in 0..nthreads {
            let (next, work, order) = (&next, &work, &order);
            hs.push(sc.spawn(move || {
                let mut got: Vec<(usize, R)> = Vec::new();
                loop {
                    let k = next.fetch_add(1, Ordering::Relaxed);
                    if k >= order.len() {
                        break;
                    }
                    got.push((order[k], work(&items[order[k]])));
                }
                got
            }));
        }
        for h in hs {
            match h.join() {
                Ok(g) => parts.extend(g),
                Err(e) => std::panic::resume_unwind(e),
            }
        }
    });
    parts.sort_by_key(|(k, _)| *k);
    parts.into_iter().map(|(_, r)| r).collect()
}

pub(crate) fn weight(f: &Func) -> usize {
    f.blocks.iter().map(|b| b.insts.len() + 1).sum()
}
