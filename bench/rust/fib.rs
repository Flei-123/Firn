// SPDX-License-Identifier: GPL-2.0-only
// Gegenstueck zu bench/firn/fib.fi — gleiche Arbeit, gleiches Ergebnis.
use std::hint::black_box;

fn fib(n: i32) -> i32 {
    if n < 2 {
        return n;
    }
    fib(n - 1) + fib(n - 2)
}

fn main() {
    let mut s: u64 = 0;
    let mut k: i32 = 0;
    while k < 3 {
        s += fib(black_box(32 - k)) as u64;
        k += 1;
    }
    println!("{}", black_box(s));
}
