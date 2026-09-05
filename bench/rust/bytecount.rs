// SPDX-License-Identifier: GPL-2.0-only
// Gegenstueck zu bench/firn/bytecount.fi.
use std::hint::black_box;

fn main() {
    let n: usize = black_box(16777216);
    let mut buf: Vec<u8> = vec![0u8; n];
    let p = buf.as_mut_ptr();
    unsafe {
        let mut i = 0usize;
        while i < n {
            *p.add(i) = (i % 251) as u8;
            i += 1;
        }
        let mut total: u64 = 0;
        let mut pass = 0;
        while pass < 8 {
            let mut k = 0usize;
            let mut c: u64 = 0;
            while k < n {
                if *p.add(k) == 65 {
                    c += 1;
                }
                if *p.add(k) == 66 {
                    c += 2;
                }
                k += 1;
            }
            total += c;
            pass += 1;
        }
        println!("{}", black_box(total));
    }
}
