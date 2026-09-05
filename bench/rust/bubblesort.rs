// SPDX-License-Identifier: GPL-2.0-only
// Gegenstueck zu bench/firn/bubblesort.fi.
use std::hint::black_box;

fn main() {
    let n: usize = black_box(6000);
    let mut a: Vec<i32> = vec![0; n];
    let p = a.as_mut_ptr();
    unsafe {
        let mut seed: u64 = 88172645463325252;
        let mut i = 0usize;
        while i < n {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            *p.add(i) = (seed % 1000000) as i32;
            i += 1;
        }
        let mut pass = 0usize;
        while pass < n {
            let mut j = 0usize;
            let mut swapped = 0i32;
            while j + 1 < n - pass {
                let x = *p.add(j);
                let y = *p.add(j + 1);
                if x > y {
                    *p.add(j) = y;
                    *p.add(j + 1) = x;
                    swapped = 1;
                }
                j += 1;
            }
            if swapped == 0 {
                pass = n;
            }
            pass += 1;
        }
        let mut total: u64 = 0;
        let mut k = 0usize;
        while k < n {
            total += *p.add(k) as u64 * (k % 7 + 1) as u64;
            k += 1;
        }
        println!("{}", black_box(total));
    }
}
