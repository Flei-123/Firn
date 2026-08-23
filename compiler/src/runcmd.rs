//! **ROUND 84** — the machinery behind `firnc run`: the cache key and the
//! cache directory.
//!
//! The command itself lives in `main.rs` (it needs `Options` and `run`).
//! What is here is the part that has to be RIGHT rather than convenient:
//! the fingerprint of a compilation.
//!
//! THE RULE FOR THE CACHE: it may never hand back a wrong answer. So the
//! key covers EVERYTHING that can change the produced binary:
//!
//!   * the source text of the root file AND of every module `import`
//!     reaches (through the same resolution the compiler uses — including
//!     `$FIRNLIB` and the package search path), plus the PATH of each of
//!     those files;
//!   * every package manifest of the world the file lives in (a manifest
//!     can switch the profile or add a search path);
//!   * the compiler: its version string, and additionally the size and the
//!     modification time of the binary that is running. The version string
//!     alone would be a trap during development — it is a constant and does
//!     not move when the compiler is rebuilt;
//!   * the build level, the target machine and the profile.
//!
//! Everything is fed length prefixed, so that two different lists of files
//! cannot produce the same octet stream by concatenation.
//!
//! When anything about the resolution fails (a missing module, a broken
//! manifest) there is no key: the caller then compiles without a cache and
//! the compiler prints the real error message. Erring towards "compile
//! again" is the only direction in which a cache is allowed to err.

use std::path::{Path, PathBuf};

// ------------------------------------------------------------------ SHA-256
//
// FIPS 180-4, in fifty lines. Not because it is fast (it is not the
// bottleneck — reading the sources costs more), but because a hash that
// decides whether a compilation is skipped should not be a home-made
// 64 bit mixer. The compiler has no dependencies and keeps it that way.

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

pub struct Sha256 {
    h: [u32; 8],
    buf: [u8; 64],
    fill: usize,
    total: u64,
}

impl Sha256 {
    pub fn new() -> Sha256 {
        Sha256 {
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buf: [0u8; 64],
            fill: 0,
            total: 0,
        }
    }

    fn block(&mut self, b: &[u8]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b2, mut c, mut d, mut e, mut f, mut g, mut h) = (
            self.h[0], self.h[1], self.h[2], self.h[3], self.h[4], self.h[5], self.h[6], self.h[7],
        );
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b2) ^ (a & c) ^ (b2 & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b2;
            b2 = a;
            a = t1.wrapping_add(t2);
        }
        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b2);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
        self.h[5] = self.h[5].wrapping_add(f);
        self.h[6] = self.h[6].wrapping_add(g);
        self.h[7] = self.h[7].wrapping_add(h);
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.total = self.total.wrapping_add(data.len() as u64);
        if self.fill > 0 {
            let need = 64 - self.fill;
            let take = need.min(data.len());
            self.buf[self.fill..self.fill + take].copy_from_slice(&data[..take]);
            self.fill += take;
            data = &data[take..];
            if self.fill == 64 {
                let b = self.buf;
                self.block(&b);
                self.fill = 0;
            }
        }
        while data.len() >= 64 {
            let (head, rest) = data.split_at(64);
            let mut b = [0u8; 64];
            b.copy_from_slice(head);
            self.block(&b);
            data = rest;
        }
        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.fill = data.len();
        }
    }

    pub fn finish(mut self) -> String {
        let bits = self.total.wrapping_mul(8);
        self.update(&[0x80]);
        while self.fill != 56 {
            self.update(&[0]);
        }
        // `update` counted the padding into `total`; the length written here
        // is the one remembered before.
        let b = self.buf;
        let mut last = [0u8; 64];
        last[..56].copy_from_slice(&b[..56]);
        last[56..].copy_from_slice(&bits.to_be_bytes());
        self.block(&last);
        let mut s = String::with_capacity(64);
        for v in self.h.iter() {
            s.push_str(&format!("{:08x}", v));
        }
        s
    }
}

/// Length prefixed: `<8 octets length><octets>`. Without the prefix the two
/// lists `["ab","c"]` and `["a","bc"]` would hash the same.
fn feed(h: &mut Sha256, part: &[u8]) {
    h.update(&(part.len() as u64).to_be_bytes());
    h.update(part);
}

// ------------------------------------------------------------ the directory

/// `$FIRN_CACHE`, else `$XDG_CACHE_HOME/firn`, else `$HOME/.cache/firn`,
/// else `/tmp/firn-cache-<user>`. The last one keeps `firnc run` working in
/// a container without a home directory instead of failing there.
pub fn cache_dir() -> PathBuf {
    if let Ok(v) = std::env::var("FIRN_CACHE") {
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    if let Ok(v) = std::env::var("XDG_CACHE_HOME") {
        if !v.is_empty() {
            return PathBuf::from(v).join("firn");
        }
    }
    if let Ok(v) = std::env::var("HOME") {
        if !v.is_empty() {
            return PathBuf::from(v).join(".cache").join("firn");
        }
    }
    let user = std::env::var("USER").unwrap_or_else(|_| "nobody".to_string());
    PathBuf::from(format!("/tmp/firn-cache-{}", user))
}

/// Empties the cache directory. Returns how many entries were removed.
/// Only the directory's own entries go — never a path outside it.
pub fn clear_cache() -> Result<usize, String> {
    let dir = cache_dir();
    let rd = match std::fs::read_dir(&dir) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(format!("{}: {}", dir.display(), e)),
    };
    let mut n = 0usize;
    for entry in rd {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => return Err(format!("{}: {}", dir.display(), e)),
        };
        let p = entry.path();
        let r = if p.is_dir() {
            std::fs::remove_dir_all(&p)
        } else {
            std::fs::remove_file(&p)
        };
        match r {
            Ok(()) => n += 1,
            Err(e) => return Err(format!("{}: {}", p.display(), e)),
        }
    }
    Ok(n)
}

// ------------------------------------------------------------------ the key

/// The fingerprint of the whole compilation, as 64 hexadecimal characters.
/// `None` means: this cannot be fingerprinted (a module is missing, a
/// manifest is broken) — then do not cache and let the compiler speak.
pub fn key_for(root: &Path, level: &str, target: &str, profile: &str) -> Option<String> {
    let world = crate::package_world::World::ab_file(&root.display().to_string()).ok()?;
    let files = crate::modules::resolve(root, &world).ok()?;
    let mut h = Sha256::new();
    feed(&mut h, b"firn run cache, format 1");
    feed(&mut h, crate::config::VERSION.as_bytes());
    feed(&mut h, crate::config::compiler_name().as_bytes());
    // The compiler binary itself: a rebuilt compiler produces a different
    // program from the same source, and the version string does not move.
    if let Ok(exe) = std::env::current_exe() {
        feed(&mut h, exe.display().to_string().as_bytes());
        if let Ok(m) = std::fs::metadata(&exe) {
            feed(&mut h, &m.len().to_be_bytes());
            if let Ok(t) = m.modified() {
                if let Ok(d) = t.duration_since(std::time::UNIX_EPOCH) {
                    feed(&mut h, &d.as_nanos().to_be_bytes());
                }
            }
        }
    }
    feed(&mut h, level.as_bytes());
    feed(&mut h, target.as_bytes());
    feed(&mut h, profile.as_bytes());
    // Every manifest of the world: it can name search paths and a profile.
    for p in &world.packages {
        feed(&mut h, p.manifestpfad.as_bytes());
        let text = std::fs::read(&p.manifestpfad).unwrap_or_default();
        feed(&mut h, &text);
    }
    // The sources, in the order the module resolution produced them.
    feed(&mut h, &(files.len() as u64).to_be_bytes());
    for f in &files {
        feed(&mut h, f.path.display().to_string().as_bytes());
        feed(&mut h, f.src.as_bytes());
    }
    Some(h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(data: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(data);
        h.finish()
    }

    #[test]
    fn sha256_known_answers() {
        // FIPS 180-4 / the usual vectors — checked against sha256sum.
        assert_eq!(
            hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn sha256_long_and_chunked() {
        // A million 'a' — the classic vector, and proof that the block
        // machinery survives a buffer boundary.
        let mut h = Sha256::new();
        let chunk = vec![b'a'; 1000];
        for _ in 0..1000 {
            h.update(&chunk);
        }
        assert_eq!(
            h.finish(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        // Same octets, silly chunk sizes: the same answer.
        let data: Vec<u8> = (0..300u32).map(|i| (i % 251) as u8).collect();
        let one = hex(&data);
        let mut g = Sha256::new();
        for part in data.chunks(7) {
            g.update(part);
        }
        assert_eq!(g.finish(), one);
    }

    #[test]
    fn feed_is_unambiguous() {
        let mut a = Sha256::new();
        feed(&mut a, b"ab");
        feed(&mut a, b"c");
        let mut b = Sha256::new();
        feed(&mut b, b"a");
        feed(&mut b, b"bc");
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn cache_dir_follows_the_environment() {
        // A pure look at the rule: FIRN_CACHE wins, then XDG, then HOME.
        // (Set through the process environment, one test only, so the
        // variables do not race with another test.)
        std::env::set_var("FIRN_CACHE", "/tmp/somewhere");
        assert_eq!(cache_dir(), PathBuf::from("/tmp/somewhere"));
        std::env::remove_var("FIRN_CACHE");
        std::env::set_var("XDG_CACHE_HOME", "/tmp/xdg");
        assert_eq!(cache_dir(), PathBuf::from("/tmp/xdg/firn"));
        std::env::remove_var("XDG_CACHE_HOME");
    }
}
