// SPDX-License-Identifier: GPL-2.0-only
// Counterpart to bench/firn/bitmap.fi.
use std::hint::black_box;

#[inline(always)]
unsafe fn ld8(p: *mut u8, i: usize) -> u8 { *p.add(i) }
#[inline(always)]
unsafe fn st8(p: *mut u8, i: usize, v: u8) { *p.add(i) = v; }

#[inline(always)]
unsafe fn bit_get(p: *mut u8, i: usize) -> bool { ld8(p, i >> 3) & (1u8 << (i & 7)) != 0 }
#[inline(always)]
unsafe fn bit_set(p: *mut u8, i: usize) { st8(p, i >> 3, ld8(p, i >> 3) | (1u8 << (i & 7))); }
#[inline(always)]
unsafe fn bit_clear(p: *mut u8, i: usize) { st8(p, i >> 3, ld8(p, i >> 3) & (255u8 - (1u8 << (i & 7)))); }

unsafe fn take(p: *mut u8, n: usize, from: usize) -> usize {
    let mut f = from;
    while f < n {
        if ld8(p, f >> 3) == 255 {
            f = (f | 7) + 1;
        } else {
            if !bit_get(p, f) {
                bit_set(p, f);
                return f;
            }
            f += 1;
        }
    }
    n
}

fn main() {
    let frames: usize = black_box(1usize << 20);
    let bytes: usize = frames >> 3;
    let mut mem: Vec<u8> = vec![0u8; bytes];
    let bm = mem.as_mut_ptr();
    unsafe {
        let mut total: u64 = 0;
        let mut round = 0usize;
        while round < 8 {
            let mut got = 0usize;
            let mut cursor = 0usize;
            while got < frames {
                let f = take(bm, frames, cursor);
                if f == frames {
                    got = frames;
                } else {
                    total += f as u64;
                    cursor = f;
                    got += 1;
                }
            }
            let mut k = 0usize;
            while k < frames {
                bit_clear(bm, k);
                k += 7;
            }
            k = 0;
            while k < frames {
                let g = take(bm, frames, k);
                if g < frames {
                    total += 1;
                }
                k += 4096;
            }
            let mut i = 0usize;
            while i < bytes {
                st8(bm, i, 0);
                i += 1;
            }
            round += 1;
        }
        println!("{}", black_box(total));
    }
}
