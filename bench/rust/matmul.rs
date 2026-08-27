// SPDX-License-Identifier: GPL-2.0-only
// Gegenstueck zu bench/firn/matmul.fi.
use std::hint::black_box;

fn main() {
    let n: usize = black_box(240);
    let mut a: Vec<i32> = vec![0; n * n];
    let mut b: Vec<i32> = vec![0; n * n];
    let mut c: Vec<i32> = vec![0; n * n];
    let (pa, pb, pc) = (a.as_mut_ptr(), b.as_mut_ptr(), c.as_mut_ptr());
    unsafe {
        let mut i = 0usize;
        while i < n {
            let mut j = 0usize;
            while j < n {
                *pa.add(i * n + j) = ((i + 2 * j) % 17) as i32;
                *pb.add(i * n + j) = ((3 * i + j) % 13) as i32;
                j += 1;
            }
            i += 1;
        }
        let mut total: u64 = 0;
        let mut pass = 0;
        while pass < 3 {
            let mut r = 0usize;
            while r < n {
                let mut cc = 0usize;
                while cc < n {
                    let mut s: i32 = 0;
                    let mut k = 0usize;
                    while k < n {
                        s += *pa.add(r * n + k) * *pb.add(k * n + cc);
                        k += 1;
                    }
                    *pc.add(r * n + cc) = s;
                    cc += 1;
                }
                r += 1;
            }
            let mut d = 0usize;
            while d < n {
                total += *pc.add(d * n + d) as u64;
                d += 1;
            }
            pass += 1;
        }
        println!("{}", black_box(total));
    }
}
