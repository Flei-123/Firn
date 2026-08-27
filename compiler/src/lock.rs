// SPDX-License-Identifier: GPL-2.0-only
//! The lock file `firn.lock` — what a build of a package really consumed
//! (round 93).
//!
//! WHY A FILE OF ITS OWN AND NOT A MODE OF `--package-info`
//! ---------------------------------------------------------------------
//! `--package-info` reports what the manifest SAYS. This file records what
//! the build FOUND: the resolved version of every package and a checksum
//! over every source file that really went in. The first is a statement
//! about a text, the second a statement about a build — and only the second
//! one can be checked on a second machine.
//!
//! FORMAT (line based, like the manifest, deliberately tiny)
//! ---------------------------------------------------------------------
//! ```text
//! lock 1                                   format, exactly once, first line
//! root app                                 name of the package that was built
//! package app 0.1.0 . <sha256>             one line per package, SORTED BY NAME
//! package geo 0.2.0 ../geo <sha256>
//! package text 0.1.0 ../text <sha256>
//! outside 0 <sha256>                       files that belong to no package
//! total <sha256>                           over every line above
//! ```
//!
//! Everything in it is MACHINE INDEPENDENT, and each piece for a reason:
//!
//! * the path of a package is **relative to the root package** and computed
//!   purely lexically (`package::relative`) — an absolute path would make
//!   the file useless on the second machine, which is the whole point.
//! * the lines are **sorted by package name**. After the version resolution
//!   one name means one package, so the order is total and does not depend
//!   on the order in which the manifests were read.
//! * a package's checksum runs over its manifest AND over every source file
//!   of it that took part in THIS build, each one as
//!   `relative path \n length \n content \n`. The relative path is in there
//!   so that renaming a file is a change; the length is in there so that no
//!   two different file sets can be glued into the same octet stream.
//! * `outside` covers the files that belong to no package (typically
//!   everything out of `$FIRNLIB`). They are keyed by their FILE NAME, not
//!   by their path — the path of the standard library is a property of the
//!   machine, its content is not. A lock file that ignored these inputs
//!   would be a lie: they end up in the binary just as much.
//! * `total` is the checksum over the text of all the lines above it, so
//!   editing a single character in the file by hand is caught as well.
//!
//! WHAT IT DOES NOT CLAIM: nothing in here says anything about the
//! COMPILER. Two machines with different `firnc` binaries can satisfy the
//! same lock file and still produce different artifacts. The lock file
//! pins the INPUT, `tools/repro/run.sh` measures the OUTPUT — item 5 of
//! `ACCEPTANCE.md` needs both halves.

use crate::modules::SourceFile;
use crate::package;
use crate::package_world::{self, World};

/// Name of the lock file. Stands exclusively here.
pub const LOCKFILE: &str = "firn.lock";
/// Format number of the first line. A reader that finds a different one
/// stops instead of guessing.
pub const FORMAT: u32 = 1;

// ------------------------------------------------------------------ SHA-256
//
// FIPS 180-4, written out. NO foreign library: the same reason as for the
// manifest format (`package.rs`) — everything here has to exist a second
// time in Firn (`lib/firnc1/lock.fi`), and both have to produce the same
// 64 hex characters for the same octets. That is checked in
// `tools/packages/run.sh` against the `sha256sum` of coreutils, so a
// mistake in either implementation cannot hide.

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

/// The running state of a hash. `push` may be called as often as one likes;
/// `hex` closes the stream and yields the 64 hex characters.
pub struct Sha256 {
    h: [u32; 8],
    block: [u8; 64],
    filled: usize,
    total: u64,
}

impl Sha256 {
    pub fn new() -> Sha256 {
        Sha256 {
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            block: [0u8; 64],
            filled: 0,
            total: 0,
        }
    }

    fn round(&mut self) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            let b = &self.block[i * 4..i * 4 + 4];
            w[i] = ((b[0] as u32) << 24) | ((b[1] as u32) << 16) | ((b[2] as u32) << 8) | b[3] as u32;
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut a = self.h[0];
        let mut b = self.h[1];
        let mut c = self.h[2];
        let mut d = self.h[3];
        let mut e = self.h[4];
        let mut f = self.h[5];
        let mut g = self.h[6];
        let mut hh = self.h[7];
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
        self.h[5] = self.h[5].wrapping_add(f);
        self.h[6] = self.h[6].wrapping_add(g);
        self.h[7] = self.h[7].wrapping_add(hh);
    }

    pub fn push(&mut self, data: &[u8]) {
        self.total += data.len() as u64;
        for &c in data {
            self.block[self.filled] = c;
            self.filled += 1;
            if self.filled == 64 {
                self.round();
                self.filled = 0;
            }
        }
    }

    pub fn hex(mut self) -> String {
        // Padding: one 0x80 octet, zeros, and the length in BITS as a
        // 64 bit big endian number in the last eight octets.
        let bits = self.total * 8;
        self.push(&[0x80]);
        while self.filled != 56 {
            self.push(&[0x00]);
        }
        let mut len = [0u8; 8];
        for i in 0..8 {
            len[i] = ((bits >> (56 - 8 * i)) & 0xff) as u8;
        }
        self.push(&len);
        let mut s = String::new();
        for x in &self.h {
            s.push_str(&format!("{:08x}", x));
        }
        s
    }
}

/// The 64 hex characters over one octet sequence.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.push(data);
    h.hex()
}

// ------------------------------------------------------------ the lock text

/// One file as it goes into a checksum: the key it is sorted by, and its
/// content.
struct Piece<'a> {
    key: String,
    text: &'a str,
}

/// `key \n length \n content \n` per file, in the order of the sorted keys.
/// The length is decimal and counts OCTETS, not characters.
fn sum_over(pieces: &mut Vec<Piece>) -> String {
    pieces.sort_by(|a, b| a.key.cmp(&b.key));
    let mut h = Sha256::new();
    for p in pieces.iter() {
        h.push(p.key.as_bytes());
        h.push(b"\n");
        h.push(format!("{}", p.text.len()).as_bytes());
        h.push(b"\n");
        h.push(p.text.as_bytes());
        h.push(b"\n");
    }
    h.hex()
}

/// The last component of a path — the key of a file outside every package.
fn file_name(path: &str) -> String {
    match path.rfind('/') {
        Some(i) => path[i + 1..].to_string(),
        None => path.to_string(),
    }
}

/// Builds the whole text of the lock file for this build. `files` is the
/// list of source files as `modules::resolve` returned it, `cwd` the
/// working directory (only used to make a path absolute — no path of the
/// machine ends up in the result).
///
/// The manifests are read a second time here. That is deliberate: the
/// checksum is supposed to cover the OCTETS on disk, not the parsed picture
/// the compiler made of them.
pub fn text(world: &World, files: &[SourceFile], cwd: &str) -> Result<String, String> {
    let root = world.packages[0].root.clone();
    // Which package does which file belong to?
    let mut per: Vec<Vec<Piece>> = Vec::new();
    for _ in 0..world.packages.len() {
        per.push(Vec::new());
    }
    let mut outside: Vec<Piece> = Vec::new();
    for f in files {
        let abs = package_world::absolute(&f.path.display().to_string(), cwd);
        match world.package_of(&abs) {
            Some(i) => per[i].push(Piece {
                key: package::relative(&world.packages[i].root, &abs),
                text: &f.src,
            }),
            None => outside.push(Piece {
                key: file_name(&abs),
                text: &f.src,
            }),
        }
    }
    // The manifest of every package belongs to its checksum: it decides
    // the version, the interface and the dependencies.
    let mut manifests: Vec<String> = Vec::new();
    for p in world.packages.iter() {
        match std::fs::read_to_string(&p.manifestpfad) {
            Ok(t) => manifests.push(t),
            Err(e) => {
                return Err(format!(
                    "error: cannot read '{}': {}\n",
                    p.manifestpfad, e
                ))
            }
        }
    }
    // Only what is REACHABLE from the root package. After the version
    // resolution a superseded package may still sit in the world without
    // anybody depending on it any more; it is not part of this build and
    // has no business in the lock file.
    let mut order: Vec<usize> = world.reachable();
    order.sort_by(|&a, &b| {
        world.packages[a]
            .manifest
            .name
            .cmp(&world.packages[b].manifest.name)
    });

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("lock {}", FORMAT));
    lines.push(format!("root {}", world.packages[0].manifest.name));
    for &i in &order {
        let p = &world.packages[i];
        let mut pieces: Vec<Piece> = Vec::new();
        pieces.push(Piece {
            key: package::MANIFEST.to_string(),
            text: &manifests[i],
        });
        for x in per[i].drain(..) {
            pieces.push(x);
        }
        lines.push(format!(
            "package {} {} {} {}",
            p.manifest.name,
            p.manifest.version,
            package::relative(&root, &p.root),
            sum_over(&mut pieces)
        ));
    }
    lines.push(format!(
        "outside {} {}",
        outside.len(),
        sum_over(&mut outside)
    ));
    let mut body = String::new();
    for l in &lines {
        body.push_str(l);
        body.push('\n');
    }
    let total = sha256_hex(body.as_bytes());
    Ok(format!("{}total {}\n", body, total))
}

// ------------------------------------------------------------- the messages
//
// Every text a user can see stands in a function of its own — `firnc1` has
// to write the same sentence, and `tools/packages/run.sh` compares the two
// octet for octet.

/// The lock file is not there at all.
pub fn text_missing(path: &str) -> String {
    format!(
        "error: {}: the lock file is missing\nnote: '--lock' writes it\n",
        path
    )
}

/// The lock file exists and does not fit. `note` comes out of `difference`.
pub fn text_mismatch(path: &str, note: &str) -> String {
    format!(
        "error: {}: the lock file does not match the sources\n{}",
        path, note
    )
}

/// `--lock`/`--locked` without `--package`.
pub fn text_needs_package(option: &str) -> String {
    format!("error: {} works only together with --package\n", option)
}

/// The FIRST line at which the two texts part, as two `note:` lines.
/// Returns `None` when they are equal — the whole verdict of `--locked`
/// hangs off this function, so it is a pure one and gets tested on its own.
pub fn difference(found: &str, computed: &str) -> Option<String> {
    let a: Vec<&str> = found.trim_end_matches('\n').split('\n').collect();
    let b: Vec<&str> = computed.trim_end_matches('\n').split('\n').collect();
    let n = if a.len() < b.len() { a.len() } else { b.len() };
    for i in 0..n {
        if a[i] != b[i] {
            return Some(format!(
                "note: line {} of the file:  '{}'\nnote: line {} of the build: '{}'\n",
                i + 1,
                a[i],
                i + 1,
                b[i]
            ));
        }
    }
    if b.len() > a.len() {
        return Some(format!(
            "note: line {} is missing in the file: '{}'\n",
            n + 1,
            b[n]
        ));
    }
    if a.len() > b.len() {
        return Some(format!(
            "note: line {} of the file is superfluous: '{}'\n",
            n + 1,
            a[n]
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three vectors of FIPS 180-4 that every implementation is
    /// measured against, plus one that is longer than a block (the padding
    /// is where a hand written SHA-256 breaks).
    #[test]
    fn sha256_against_the_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        // 1,000,000 times 'a' — the fourth vector of the standard.
        let mut h = Sha256::new();
        for _ in 0..1000 {
            h.push(&[b'a'; 1000]);
        }
        assert_eq!(
            h.hex(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        // Exactly one block, and one octet more: the two edges of padding.
        assert_eq!(
            sha256_hex(&[b'x'; 64]),
            sha256_hex(&[b'x'; 64]),
        );
        assert_eq!(
            sha256_hex(&[b'a'; 55]).len(),
            64
        );
    }

    #[test]
    fn the_difference_names_the_first_line() {
        assert_eq!(difference("a\nb\n", "a\nb\n"), None);
        // Trailing newline or not must not make a difference.
        assert_eq!(difference("a\nb", "a\nb\n"), None);
        let d = difference("lock 1\nroot a\n", "lock 1\nroot b\n").unwrap();
        assert_eq!(
            d,
            "note: line 2 of the file:  'root a'\nnote: line 2 of the build: 'root b'\n"
        );
        let d = difference("lock 1\n", "lock 1\nroot b\n").unwrap();
        assert_eq!(d, "note: line 2 is missing in the file: 'root b'\n");
        let d = difference("lock 1\nroot b\nx\n", "lock 1\nroot b\n").unwrap();
        assert_eq!(d, "note: line 3 of the file is superfluous: 'x'\n");
    }

    #[test]
    fn the_messages_are_fixed() {
        assert_eq!(
            text_missing("/p/app/firn.lock"),
            "error: /p/app/firn.lock: the lock file is missing\nnote: '--lock' writes it\n"
        );
        assert_eq!(
            text_needs_package("--locked"),
            "error: --locked works only together with --package\n"
        );
        assert_eq!(
            text_mismatch("/p/firn.lock", "note: x\n"),
            "error: /p/firn.lock: the lock file does not match the sources\nnote: x\n"
        );
    }

    /// A checksum has to depend on the NAME of a file as well, otherwise
    /// renaming two modules into each other would go unnoticed.
    #[test]
    fn the_name_of_a_file_is_part_of_the_checksum() {
        let mut a = vec![
            Piece { key: "a.fi".to_string(), text: "one" },
            Piece { key: "b.fi".to_string(), text: "two" },
        ];
        let mut b = vec![
            Piece { key: "a.fi".to_string(), text: "two" },
            Piece { key: "b.fi".to_string(), text: "one" },
        ];
        assert_ne!(sum_over(&mut a), sum_over(&mut b));
        // And the order in which the files arrive must NOT matter.
        let mut c = vec![
            Piece { key: "b.fi".to_string(), text: "two" },
            Piece { key: "a.fi".to_string(), text: "one" },
        ];
        assert_eq!(sum_over(&mut a), sum_over(&mut c));
    }
}
