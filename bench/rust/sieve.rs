// SPDX-License-Identifier: GPL-2.0-only
// Gegenstueck zu bench/firn/sieve.fi.
use std::hint::black_box;

fn main() {
    let n: usize = black_box(5000000);
    let mut buf: Vec<u8> = vec![0u8; n + 1];
    let p = buf.as_mut_ptr();
    let mut total: u64 = 0;
    let mut pass = 0;
    while pass < 2 {
        unsafe {
            let mut k = 0usize;
            while k <= n {
                *p.add(k) = 1;
                k += 1;
            }
            let mut i = 2usize;
            while i * i <= n {
                if *p.add(i) == 1 {
                    let mut j = i * i;
                    while j <= n {
                        *p.add(j) = 0;
                        j += i;
                    }
                }
                i += 1;
            }
            let mut count: u64 = 0;
            let mut m = 2usize;
            while m <= n {
                count += *p.add(m) as u64;
                m += 1;
            }
            total += count;
        }
        pass += 1;
    }
    println!("{}", black_box(total));
}
