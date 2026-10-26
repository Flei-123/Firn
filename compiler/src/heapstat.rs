// SPDX-License-Identifier: MPL-2.0
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

pub struct Counting;

static ON: AtomicBool = AtomicBool::new(false);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

pub fn enable() {
    ON.store(true, Relaxed);
}

pub fn live() -> usize {
    LIVE.load(Relaxed)
}

pub fn take_peak() -> usize {
    PEAK.swap(LIVE.load(Relaxed), Relaxed)
}

fn grow(n: usize) {
    let now = LIVE.fetch_add(n, Relaxed) + n;
    PEAK.fetch_max(now, Relaxed);
}

fn shrink(n: usize) {
    LIVE.fetch_sub(n, Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = System.alloc(l);
        if !p.is_null() && ON.load(Relaxed) {
            grow(l.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = System.alloc_zeroed(l);
        if !p.is_null() && ON.load(Relaxed) {
            grow(l.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        if ON.load(Relaxed) {
            shrink(l.size());
        }
        System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let q = System.realloc(p, l, n);
        if !q.is_null() && ON.load(Relaxed) {
            if n >= l.size() {
                grow(n - l.size());
            } else {
                shrink(l.size() - n);
            }
        }
        q
    }
}
