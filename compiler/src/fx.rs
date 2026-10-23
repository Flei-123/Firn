//! Fast non-cryptographic hasher (FxHash style) and `HashMap`/`HashSet` aliases built on it.
//!
//! The compiler never hashes untrusted keys with a security requirement, and SipHash
//! (std default) showed up at ~15 % of the optimizer time. Iteration order of these
//! maps must never influence emitted output (it was already random with `RandomState`).

use std::hash::{BuildHasherDefault, Hasher};

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

#[derive(Default, Clone, Copy)]
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    #[inline(always)]
    fn add(&mut self, w: u64) {
        self.hash = (self.hash.rotate_left(5) ^ w).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut b = bytes;
        while b.len() >= 8 {
            self.add(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]));
            b = &b[8..];
        }
        if b.len() >= 4 {
            self.add(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64);
            b = &b[4..];
        }
        for &x in b {
            self.add(x as u64);
        }
    }
    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(i as u64);
    }
    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.add(i as u64);
    }
    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(i as u64);
    }
    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }
    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        // hashbrown takes the top 7 bits as control tag and the low bits as bucket index:
        // fold so both ends carry entropy.
        self.hash ^ (self.hash >> 29)
    }
}

pub type FxBuild = BuildHasherDefault<FxHasher>;
pub type HashMap<K, V> = std::collections::HashMap<K, V, FxBuild>;
pub type HashSet<K> = std::collections::HashSet<K, FxBuild>;
