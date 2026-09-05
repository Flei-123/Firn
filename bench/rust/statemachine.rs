// SPDX-License-Identifier: GPL-2.0-only
// Gegenstueck zu bench/firn/statemachine.fi.
use std::hint::black_box;

fn main() {
    let n: usize = black_box(8388608);
    let mut buf: Vec<u8> = vec![0u8; n];
    let p = buf.as_mut_ptr();
    let pat: [u8; 10] = [60, 97, 32, 120, 61, 39, 49, 39, 62, 116];
    unsafe {
        let mut i = 0usize;
        while i < n {
            *p.add(i) = pat[i % 10];
            i += 1;
        }
        let mut total: u64 = 0;
        let mut pass = 0;
        while pass < 4 {
            let mut state: i32 = 0;
            let mut tags: u64 = 0;
            let mut text: u64 = 0;
            let mut k = 0usize;
            while k < n {
                let ch = *p.add(k);
                if state == 0 {
                    if ch == 60 {
                        state = 1;
                    } else {
                        text += 1;
                    }
                } else if state == 1 {
                    if ch == 62 {
                        state = 0;
                        tags += 1;
                    } else if ch == 39 {
                        state = 2;
                    } else if ch == 32 {
                        state = 3;
                    }
                } else if state == 2 {
                    if ch == 39 {
                        state = 1;
                    }
                } else if ch == 62 {
                    state = 0;
                    tags += 1;
                } else if ch == 39 {
                    state = 2;
                }
                k += 1;
            }
            total += tags + text;
            pass += 1;
        }
        println!("{}", black_box(total));
    }
}
