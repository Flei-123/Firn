use std::sync::atomic::{AtomicU64, Ordering};

static TREE_DROPS: AtomicU64 = AtomicU64::new(0);
static FUSED_CONCATS: AtomicU64 = AtomicU64::new(0);

pub(crate) fn tree_drops(n: usize) {
    TREE_DROPS.fetch_add(n as u64, Ordering::Relaxed);
}

pub(crate) fn fused_concat() {
    FUSED_CONCATS.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn report() {
    if std::env::var_os("FIRN_OWN_STATS").is_none() || !super::enabled() {
        return;
    }
    eprintln!(
        "firn-own-stats: mode={:?} tree_drops={} fused_concats={}",
        super::mode(),
        TREE_DROPS.load(Ordering::Relaxed),
        FUSED_CONCATS.load(Ordering::Relaxed)
    );
}
