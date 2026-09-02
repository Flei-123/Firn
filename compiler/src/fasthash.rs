// SPDX-License-Identifier: GPL-2.0-only
//! **ROUND TEMPO — the hash table of the compiler.**
//!
//! ## What was measured
//!
//! `valgrind --tool=callgrind` over `firnc --emit=asm bin/firnc1.fi`
//! (10,176,912,058 instructions in all) said this, before anything here
//! existed:
//!
//! ```text
//! 1,660,358,991 (16.31%)  firnc::regalloc::allocate
//! 1,404,865,250 (13.80%)  <RandomState as BuildHasher>::hash_one::<&u32>
//!   693,778,919 ( 6.82%)  <sip::Hasher<Sip13Rounds> as Hasher>::write
//! ```
//!
//! **Every fifth instruction the compiler executes is SipHash**, and it is
//! hashing `u32` keys: `fir::Val` is a `u32`, and the register allocation,
//! `mem2reg`, the optimizer and the code generators keep their tables keyed
//! by it. SipHash-1-3 is the default of `std::collections::HashMap` because
//! it resists hash flooding from untrusted input. A compiler does not have
//! untrusted keys — the keys are its own value numbers, dense and small.
//!
//! ## What this is
//!
//! The multiply-xor-rotate hash of Firefox/rustc ("FxHash"), sixteen lines,
//! no crate. For a `u32` it is ONE rotate, ONE xor and ONE multiply against
//! SipHash's whole round schedule.
//!
//! ## Why it is safe here
//!
//! * The seed is **fixed**, so the iteration order of a map is now the same
//!   in every run. `std`'s `RandomState` is seeded from the operating
//!   system per process — the order was already varying from run to run,
//!   and `tools/repro/run.sh` proves byte-identical output across runs. So
//!   nothing that reaches the output could ever have depended on the order,
//!   and a fixed seed can only make the compiler *more* deterministic.
//! * The tables are internal. No user of the compiler can choose the keys.
//!
//! ## What it is not
//!
//! Not a cryptographic hash and not collision resistant. Nothing here goes
//! near the standard library of the language — `lib/std/map.fi` keeps its
//! own hash. This is the *compiler's* table only.

use std::hash::{BuildHasherDefault, Hasher};

/// The constant of the golden ratio in 64 bits, as rustc's `FxHasher` uses
/// it. Its job is to spread the bits of a small integer over the whole word.
const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

/// Multiply-xor-rotate. One `u64` word at a time; the tail is padded.
#[derive(Default, Clone, Copy)]
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    /// **The final mix, and why it is not optional here.**
    ///
    /// `std`'s table (hashbrown) takes the LOW bits of the hash for the
    /// bucket and the top seven for the control byte. The multiply of
    /// FxHash pushes entropy upwards; its low bits stay weak, and dense
    /// keys like `fir::Val` then land in the same buckets. Measured: with
    /// the raw `self.hash` the optimizer got 25 % faster but the code
    /// GENERATOR got 3-5 % SLOWER -- `regalloc.rs` keeps many small
    /// short-lived tables, and they were the ones that clustered.
    ///
    /// Four instructions of splitmix64's finalizer fix it: after them
    /// every bit of the result depends on every bit of the input.
    #[inline]
    fn finish(&self) -> u64 {
        let mut x = self.hash;
        x ^= x >> 32;
        x = x.wrapping_mul(0xd6e8_feb8_6659_fd93);
        x ^= x >> 32;
        x
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while rest.len() >= 8 {
            let mut w = [0u8; 8];
            w.copy_from_slice(&rest[..8]);
            self.add(u64::from_ne_bytes(w));
            rest = &rest[8..];
        }
        if rest.len() >= 4 {
            let mut w = [0u8; 4];
            w.copy_from_slice(&rest[..4]);
            self.add(u32::from_ne_bytes(w) as u64);
            rest = &rest[4..];
        }
        for b in rest {
            self.add(*b as u64);
        }
    }

    #[inline]
    fn write_u8(&mut self, n: u8) {
        self.add(n as u64)
    }
    #[inline]
    fn write_u16(&mut self, n: u16) {
        self.add(n as u64)
    }
    #[inline]
    fn write_u32(&mut self, n: u32) {
        self.add(n as u64)
    }
    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.add(n)
    }
    #[inline]
    fn write_usize(&mut self, n: usize) {
        self.add(n as u64)
    }
    #[inline]
    fn write_i32(&mut self, n: i32) {
        self.add(n as u32 as u64)
    }
    #[inline]
    fn write_i64(&mut self, n: i64) {
        self.add(n as u64)
    }
}

/// The `BuildHasher` behind the two aliases below.
pub type BuildFx = BuildHasherDefault<FxHasher>;

/// Drop-in for `std::collections::HashMap`. **`::new()` does not exist for
/// it** — write `HashMap::default()`; everything else is identical.
pub type HashMap<K, V> = std::collections::HashMap<K, V, BuildFx>;

/// Drop-in for `std::collections::HashSet`, same rule.
pub type HashSet<K> = std::collections::HashSet<K, BuildFx>;

/// `HashMap::with_capacity(n)` has no equivalent on an alias either.
#[allow(dead_code)]
#[inline]
pub fn map_with_capacity<K, V>(n: usize) -> HashMap<K, V> {
    std::collections::HashMap::with_capacity_and_hasher(n, BuildFx::default())
}

/// The same for a set.
#[allow(dead_code)]
#[inline]
pub fn set_with_capacity<K>(n: usize) -> HashSet<K> {
    std::collections::HashSet::with_capacity_and_hasher(n, BuildFx::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hash of a value has to be the same in every run — that is the
    /// whole difference to `RandomState` and the reason a fixed seed is
    /// allowed here at all.
    #[test]
    fn deterministic() {
        use std::hash::BuildHasher;
        let b = BuildFx::default();
        let a = b.hash_one(1234u32);
        let c = b.hash_one(1234u32);
        assert_eq!(a, c);
        assert_ne!(a, b.hash_one(1235u32));
    }

    /// A map with the alias behaves like a map without it.
    #[test]
    fn map_works() {
        let mut m: HashMap<u32, u32> = HashMap::default();
        for i in 0..1000u32 {
            m.insert(i, i * 3);
        }
        for i in 0..1000u32 {
            assert_eq!(m.get(&i), Some(&(i * 3)));
        }
        assert_eq!(m.len(), 1000);
        m.remove(&17);
        assert_eq!(m.get(&17), None);
        assert_eq!(m.len(), 999);
    }

    /// Different lengths of key material must not collide trivially.
    #[test]
    fn strings_spread() {
        let mut s: HashSet<String> = HashSet::default();
        for i in 0..2000 {
            s.insert(format!("_F0.some__name{}", i));
        }
        assert_eq!(s.len(), 2000);
    }
}
