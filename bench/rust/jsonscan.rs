// SPDX-License-Identifier: GPL-2.0-only
// Counterpart to bench/firn/jsonscan.fi.
use std::hint::black_box;

#[inline(always)]
fn is_ws(c: u8) -> bool { c == 32 || c == 9 || c == 10 || c == 13 }
#[inline(always)]
fn is_digit(c: u8) -> bool { c >= 48 && c <= 57 }

unsafe fn scan(p: *mut u8, len: usize) -> u64 {
    let mut i = 0usize;
    let mut tokens: u64 = 0;
    let mut depth: u64 = 0;
    let mut maxdepth: u64 = 0;
    while i < len {
        let c = *p.add(i);
        if is_ws(c) {
            i += 1;
        } else if c == 123 || c == 91 {
            depth += 1;
            if depth > maxdepth { maxdepth = depth; }
            tokens += 1;
            i += 1;
        } else if c == 125 || c == 93 {
            depth -= 1;
            tokens += 1;
            i += 1;
        } else if c == 58 || c == 44 {
            tokens += 1;
            i += 1;
        } else if c == 34 {
            i += 1;
            while i < len {
                let d = *p.add(i);
                if d == 92 { i += 2; }
                else if d == 34 { i += 1; break; }
                else { i += 1; }
            }
            tokens += 1;
        } else if is_digit(c) || c == 45 {
            i += 1;
            while i < len {
                let d = *p.add(i);
                if is_digit(d) || d == 46 || d == 101 || d == 69 || d == 43 || d == 45 { i += 1; }
                else { break; }
            }
            tokens += 1;
        } else {
            i += 1;
            while i < len {
                let d = *p.add(i);
                if d >= 97 && d <= 122 { i += 1; } else { break; }
            }
            tokens += 1;
        }
    }
    tokens * 7 + maxdepth
}

fn main() {
    let rec: &[u8] = br#"{"id":1234,"name":"item \"x\"","ok":true,"v":[1,2,3]},"#;
    let reclen = rec.len();
    let count: usize = black_box(500000);
    let len = reclen * count + 2;
    let mut mem: Vec<u8> = vec![0u8; len];
    let buf = mem.as_mut_ptr();
    unsafe {
        *buf.add(0) = 91;
        let mut at = 1usize;
        let mut i = 0usize;
        while i < count {
            let mut j = 0usize;
            while j < reclen {
                *buf.add(at) = rec[j];
                at += 1;
                j += 1;
            }
            i += 1;
        }
        *buf.add(at) = 93;
        at += 1;
        println!("{}", black_box(scan(buf, at)));
    }
}
