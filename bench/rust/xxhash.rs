// SPDX-License-Identifier: GPL-2.0-only
// Counterpart to bench/firn/xxhash.fi.
use std::hint::black_box;

const P1: u64 = 11400714785074694791;
const P2: u64 = 14029467366897019727;
const P3: u64 = 1609587929392839161;
const P4: u64 = 9650029242287828579;
const P5: u64 = 2870177450012600261;

#[inline(always)]
fn rotl(x: u64, r: u32) -> u64 { (x << r) | (x >> (64 - r)) }
#[inline(always)]
fn round_(acc: u64, input: u64) -> u64 {
    rotl(acc.wrapping_add(input.wrapping_mul(P2)), 31).wrapping_mul(P1)
}
#[inline(always)]
fn merge(acc: u64, val: u64) -> u64 {
    (acc ^ round_(0, val)).wrapping_mul(P1).wrapping_add(P4)
}
#[inline(always)]
fn avalanche(h0: u64) -> u64 {
    let mut h = h0;
    h ^= h >> 33;
    h = h.wrapping_mul(P2);
    h ^= h >> 29;
    h = h.wrapping_mul(P3);
    h ^= h >> 32;
    h
}

unsafe fn xxh64(p: *mut u8, len: usize, seed: u64) -> u64 {
    let q = p as *mut u64;
    let mut h: u64;
    let mut i = 0usize;
    if len >= 32 {
        let mut v1 = seed.wrapping_add(P1).wrapping_add(P2);
        let mut v2 = seed.wrapping_add(P2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(P1);
        while i + 32 <= len {
            v1 = round_(v1, *q.add(i >> 3));
            v2 = round_(v2, *q.add((i >> 3) + 1));
            v3 = round_(v3, *q.add((i >> 3) + 2));
            v4 = round_(v4, *q.add((i >> 3) + 3));
            i += 32;
        }
        h = rotl(v1, 1)
            .wrapping_add(rotl(v2, 7))
            .wrapping_add(rotl(v3, 12))
            .wrapping_add(rotl(v4, 18));
        h = merge(h, v1);
        h = merge(h, v2);
        h = merge(h, v3);
        h = merge(h, v4);
    } else {
        h = seed.wrapping_add(P5);
    }
    h = h.wrapping_add(len as u64);
    while i + 8 <= len {
        h = rotl(h ^ round_(0, *q.add(i >> 3)), 27).wrapping_mul(P1).wrapping_add(P4);
        i += 8;
    }
    while i < len {
        h = rotl(h ^ (*p.add(i) as u64).wrapping_mul(P5), 11).wrapping_mul(P1);
        i += 1;
    }
    avalanche(h)
}

fn main() {
    let len: usize = black_box(64 * 1024 * 1024);
    let mut mem: Vec<u8> = vec![0u8; len];
    let buf = mem.as_mut_ptr();
    unsafe {
        let mut i = 0usize;
        let mut x: u64 = 12345;
        while i < len {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            *buf.add(i) = ((x >> 33) & 255) as u8;
            i += 1;
        }
        let mut h: u64 = 0;
        let mut k = 0usize;
        while k < 4 {
            h ^= xxh64(buf, len, k as u64);
            k += 1;
        }
        println!("{}", black_box(h));
    }
}
