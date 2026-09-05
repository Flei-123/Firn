// SPDX-License-Identifier: GPL-2.0-only
// Counterpart to bench/firn/memstride.fi.
use std::hint::black_box;

fn main() {
    let len: usize = black_box(256 * 1024 * 1024);
    let words = len / 8;
    let mut mem: Vec<u64> = vec![0u64; words];
    let buf = mem.as_mut_ptr();
    unsafe {
        let mut i = 0usize;
        while i < words {
            *buf.add(i) = (i & 1023) as u64;
            i += 1;
        }
        let mut total: u64 = 0;
        let stride = 520usize;
        let mut pass = 0usize;
        while pass < 8 {
            let mut k = pass;
            while k < words {
                total += *buf.add(k);
                k += stride;
            }
            pass += 1;
        }
        println!("{}", black_box(total));
    }
}
