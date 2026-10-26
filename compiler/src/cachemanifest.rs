// SPDX-License-Identifier: MPL-2.0
use crate::buildcache;
use crate::keyhash::digest_hex;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAGIC: &str = "FIRNRESOLVE1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seen {
    File { path: String, sha: String },
    Probe { path: String, exists: bool },
}

static SEEN: Mutex<Vec<Seen>> = Mutex::new(Vec::new());

fn push(item: Seen) {
    if let Ok(mut v) = SEEN.lock() {
        v.push(item);
    }
}

pub fn exists(p: &Path) -> bool {
    let found = p.exists();
    push(Seen::Probe { path: p.display().to_string(), exists: found });
    found
}

pub fn is_file(p: &Path) -> bool {
    let found = p.is_file();
    push(Seen::Probe { path: format!("file:{}", p.display()), exists: found });
    found
}

pub fn read_source(p: &Path, text: &str) {
    push(Seen::File { path: p.display().to_string(), sha: digest_hex(text.as_bytes()) });
}

fn recorded() -> Vec<Seen> {
    SEEN.lock().map(|v| v.clone()).unwrap_or_default()
}

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{:02x}", b)).collect()
}

fn unhex(s: &str) -> Option<String> {
    if s.len() % 2 != 0 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()).collect();
    String::from_utf8(bytes?).ok()
}

fn encode(key: &str, seen: Vec<Seen>) -> Vec<u8> {
    let mut body = String::new();
    body.push_str(&format!("key={}\n", key));
    for s in seen {
        match s {
            Seen::File { path, sha } => body.push_str(&format!("file {} {}\n", hex(&path), sha)),
            Seen::Probe { path, exists } => body.push_str(&format!("probe {} {}\n", hex(&path), exists as u8)),
        }
    }
    format!("{}\nsum={}\n---\n{}", MAGIC, digest_hex(body.as_bytes()), body).into_bytes()
}

fn decode(bytes: &[u8]) -> Option<(String, Vec<Seen>)> {
    let text = std::str::from_utf8(bytes).ok()?;
    let (head, body) = text.split_once("\n---\n")?;
    let mut hl = head.lines();
    if hl.next()? != MAGIC {
        return None;
    }
    let sum = hl.next()?.strip_prefix("sum=")?;
    if digest_hex(body.as_bytes()) != sum {
        return None;
    }
    let mut lines = body.lines();
    let key = lines.next()?.strip_prefix("key=")?.to_string();
    let mut seen = Vec::new();
    for l in lines {
        let f: Vec<&str> = l.split(' ').collect();
        match f.as_slice() {
            ["file", p, sha] => seen.push(Seen::File { path: unhex(p)?, sha: sha.to_string() }),
            ["probe", p, e] => seen.push(Seen::Probe { path: unhex(p)?, exists: *e == "1" }),
            _ => return None,
        }
    }
    Some((key, seen))
}

fn holds(s: &Seen) -> bool {
    match s {
        Seen::File { path, sha } => match std::fs::read(path) {
            Ok(b) => digest_hex(&b) == *sha,
            Err(_) => false,
        },
        Seen::Probe { path, exists } => match path.strip_prefix("file:") {
            Some(p) => PathBuf::from(p).is_file() == *exists,
            None => PathBuf::from(path).exists() == *exists,
        },
    }
}

fn manifest_path(dir: &Path, mkey: &str) -> PathBuf {
    buildcache::objects_dir(dir).join(format!("{}.rm", mkey))
}

pub fn lookup(dir: &Path, mkey: &str) -> Option<String> {
    let p = manifest_path(dir, mkey);
    let (key, seen) = decode(&std::fs::read(&p).ok()?)?;
    if !seen.iter().all(holds) {
        return None;
    }
    buildcache::touch(&p);
    Some(key)
}

pub fn store(dir: &Path, mkey: &str, key: &str) -> bool {
    store_seen(dir, mkey, key, recorded())
}

fn store_seen(dir: &Path, mkey: &str, key: &str, seen: Vec<Seen>) -> bool {
    buildcache::write_atomic(dir, &manifest_path(dir, mkey), &encode(key, seen))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> PathBuf {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let d = std::env::temp_dir().join(format!("firn-rm-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn manifest_is_valid_until_an_input_or_probe_changes() {
        let d = tmpdir();
        let f = d.join("a.fi");
        let ghost = d.join("ghost.fi");
        std::fs::write(&f, "one").unwrap();
        let seen = vec![
            Seen::File { path: f.display().to_string(), sha: digest_hex(b"one") },
            Seen::Probe { path: ghost.display().to_string(), exists: false },
        ];
        assert!(store_seen(&d, "m1", "keyA", seen));
        assert_eq!(lookup(&d, "m1").as_deref(), Some("keyA"));
        std::fs::write(&ghost, "x").unwrap();
        assert!(lookup(&d, "m1").is_none());
        std::fs::remove_file(&ghost).unwrap();
        assert_eq!(lookup(&d, "m1").as_deref(), Some("keyA"));
        std::fs::write(&f, "two").unwrap();
        assert!(lookup(&d, "m1").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn damaged_manifest_is_a_miss() {
        let d = tmpdir();
        let f = d.join("b.fi");
        std::fs::write(&f, "q").unwrap();
        let seen = vec![Seen::File { path: f.display().to_string(), sha: digest_hex(b"q") }];
        assert!(store_seen(&d, "m2", "keyB", seen));
        let p = manifest_path(&d, "m2");
        let mut b = std::fs::read(&p).unwrap();
        let n = b.len();
        b[n - 3] ^= 1;
        std::fs::write(&p, &b).unwrap();
        assert!(lookup(&d, "m2").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
