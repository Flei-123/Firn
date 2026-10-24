// SPDX-License-Identifier: MPL-2.0
use crate::lock::Sha256;
use crate::modules::SourceFile;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

const MAGIC: &str = "FIRNCACHE1";
const OBJECT_MAGIC: &str = "FIRNOBJECT1";
const DEFAULT_MAX_MB: u64 = 512;
const STALE_TMP: Duration = Duration::from_secs(3600);
const EXCLUDED_ENV: [&str; 4] = ["FIRN_CACHE_DIR", "FIRN_CACHE_MAX_MB", "FIRN_NO_CACHE", "FIRN_CACHE_LOG"];

static TAINT: AtomicBool = AtomicBool::new(false);

pub fn taint() {
    TAINT.store(true, Ordering::SeqCst);
}

pub fn log(event: &str) {
    if std::env::var_os("FIRN_CACHE_LOG").is_some() {
        eprintln!("cache: {}", event);
    }
}

pub fn tainted() -> bool {
    TAINT.load(Ordering::SeqCst)
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct LinkInfo {
    pub kernel: bool,
    pub statics: bool,
    pub libs: Vec<String>,
    pub exports: Vec<String>,
}

impl LinkInfo {
    pub fn capture() -> LinkInfo {
        LinkInfo {
            kernel: crate::prof::is_kernel(),
            statics: crate::statics::any(),
            libs: crate::extfn::link_libs(),
            exports: crate::extfn::export_names(),
        }
    }
}

pub struct Entry {
    pub link: LinkInfo,
    pub asm: String,
}

pub struct Head {
    pub link: LinkInfo,
    path: PathBuf,
    offset: usize,
    len: usize,
    sum: String,
}

#[derive(Clone)]
pub struct Slot {
    pub dir: PathBuf,
    pub key: String,
}

pub fn cache_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("FIRN_CACHE_DIR") {
        if !d.is_empty() {
            return Some(PathBuf::from(d));
        }
    }
    let home = std::env::var_os("HOME")?;
    if home.is_empty() {
        return None;
    }
    Some(PathBuf::from(home).join(".cache").join("firn"))
}

pub fn disabled_by_env() -> bool {
    match std::env::var_os("FIRN_NO_CACHE") {
        Some(v) => !v.is_empty() && v != "0",
        None => false,
    }
}

fn max_bytes() -> u64 {
    let mb = std::env::var("FIRN_CACHE_MAX_MB")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_MAX_MB);
    mb.saturating_mul(1024 * 1024)
}

fn push_field(h: &mut Sha256, tag: &str, data: &[u8]) {
    h.push(tag.as_bytes());
    h.push(b"\n");
    h.push(format!("{}", data.len()).as_bytes());
    h.push(b"\n");
    h.push(data);
    h.push(b"\n");
}

fn key_args(args: &[String]) -> Vec<&str> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "-o" {
            i += 2;
            continue;
        }
        if a == "--no-cache" || (a.starts_with("-o") && !a.starts_with("--")) {
            i += 1;
            continue;
        }
        out.push(a);
        i += 1;
    }
    out
}

pub fn key(args: &[String], files: &[SourceFile], manifests: &[String]) -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe_bytes = std::fs::read(exe).ok()?;
    let mut h = Sha256::new();
    push_field(&mut h, "compiler", crate::lock::sha256_hex(&exe_bytes).as_bytes());
    push_field(&mut h, "cwd", crate::package_world::cwd().as_bytes());
    for a in key_args(args) {
        push_field(&mut h, "arg", a.as_bytes());
    }
    let mut env: Vec<(String, Vec<u8>)> = std::env::vars_os()
        .filter_map(|(k, v)| {
            let k = k.into_string().ok()?;
            if k.starts_with("FIRN") && !EXCLUDED_ENV.contains(&k.as_str()) {
                Some((k, v.into_encoded_bytes()))
            } else {
                None
            }
        })
        .collect();
    env.sort();
    for (k, v) in &env {
        push_field(&mut h, "env", k.as_bytes());
        push_field(&mut h, "val", v);
    }
    for f in files {
        push_field(&mut h, "path", f.path.display().to_string().as_bytes());
        push_field(&mut h, "src", f.src.as_bytes());
    }
    for m in manifests {
        push_field(&mut h, "manifest", m.as_bytes());
        let body = std::fs::read(m).unwrap_or_default();
        push_field(&mut h, "manifest-body", &body);
    }
    Some(h.hex())
}

fn objects_dir(dir: &Path) -> PathBuf {
    dir.join("objects")
}

fn entry_path(dir: &Path, key: &str) -> PathBuf {
    objects_dir(dir).join(format!("{}.fc", key))
}

fn fast_sum(data: &[u8]) -> u64 {
    let mut h: u64 = 0x9E37_79B9_7F4A_7C15 ^ data.len() as u64;
    let mut chunks = data.chunks_exact(8);
    for c in chunks.by_ref() {
        let mut w = [0u8; 8];
        w.copy_from_slice(c);
        h = (h.rotate_left(5) ^ u64::from_le_bytes(w)).wrapping_mul(0x517C_C1B7_2722_0A95);
    }
    for b in chunks.remainder() {
        h = (h.rotate_left(5) ^ *b as u64).wrapping_mul(0x517C_C1B7_2722_0A95);
    }
    h ^ (h >> 32)
}

fn encode_head(link: &LinkInfo, asm: &str) -> String {
    let mut s = String::new();
    s.push_str(MAGIC);
    s.push('\n');
    s.push_str(&format!("kernel={}\n", link.kernel as u8));
    s.push_str(&format!("statics={}\n", link.statics as u8));
    for l in &link.libs {
        s.push_str(&format!("lib={}\n", l));
    }
    for x in &link.exports {
        s.push_str(&format!("export={}\n", x));
    }
    s.push_str(&format!("len={}\n", asm.len()));
    s.push_str(&format!("sum={:016x}\n", fast_sum(asm.as_bytes())));
    s.push_str("---\n");
    s
}

fn parse_head(bytes: &[u8]) -> Option<(LinkInfo, usize, usize, String)> {
    let marker = b"\n---\n";
    let at = bytes.windows(marker.len()).take(4096).position(|w| w == marker)?;
    let head = std::str::from_utf8(&bytes[..at]).ok()?;
    let mut lines = head.lines();
    if lines.next()? != MAGIC {
        return None;
    }
    let mut link = LinkInfo::default();
    let mut len: Option<usize> = None;
    let mut sum: Option<String> = None;
    for l in lines {
        let (k, v) = l.split_once('=')?;
        match k {
            "kernel" => link.kernel = v == "1",
            "statics" => link.statics = v == "1",
            "lib" => link.libs.push(v.to_string()),
            "export" => link.exports.push(v.to_string()),
            "len" => len = v.parse().ok(),
            "sum" => sum = Some(v.to_string()),
            _ => return None,
        }
    }
    Some((link, at + marker.len(), len?, sum?))
}

fn decode(mut bytes: Vec<u8>) -> Option<Entry> {
    let (link, offset, len, sum) = parse_head(&bytes)?;
    bytes.drain(..offset);
    if bytes.len() != len {
        return None;
    }
    if format!("{:016x}", fast_sum(&bytes)) != sum {
        return None;
    }
    let asm = String::from_utf8(bytes).ok()?;
    Some(Entry { link, asm })
}

fn touch(path: &Path) {
    if let Ok(f) = std::fs::File::options().append(true).open(path) {
        let _ = f.set_modified(SystemTime::now());
    }
}

pub fn lookup(dir: &Path, key: &str) -> Option<Entry> {
    let path = entry_path(dir, key);
    let bytes = std::fs::read(&path).ok()?;
    let e = decode(bytes)?;
    touch(&path);
    Some(e)
}

pub fn lookup_head(dir: &Path, key: &str) -> Option<Head> {
    use std::io::Read;
    let path = entry_path(dir, key);
    let mut f = std::fs::File::open(&path).ok()?;
    let total = f.metadata().ok()?.len() as usize;
    let mut buf = vec![0u8; 4096.min(total)];
    f.read_exact(&mut buf).ok()?;
    let (link, offset, len, sum) = parse_head(&buf)?;
    if offset + len != total {
        return None;
    }
    touch(&path);
    Some(Head { link, path, offset, len, sum })
}

impl Head {
    pub fn read_asm(&self) -> Option<String> {
        let mut bytes = std::fs::read(&self.path).ok()?;
        if bytes.len() != self.offset + self.len {
            return None;
        }
        bytes.drain(..self.offset);
        if format!("{:016x}", fast_sum(&bytes)) != self.sum {
            return None;
        }
        String::from_utf8(bytes).ok()
    }
}

fn object_path(dir: &Path, key: &str, id: &str) -> PathBuf {
    objects_dir(dir).join(format!("{}.{}.o", key, id))
}

fn encode_object(obj: &[u8]) -> Vec<u8> {
    let mut v = format!(
        "{}\nlen={}\nsum={:016x}\n---\n",
        OBJECT_MAGIC,
        obj.len(),
        fast_sum(obj)
    )
    .into_bytes();
    v.extend_from_slice(obj);
    v
}

fn decode_object(mut bytes: Vec<u8>) -> Option<Vec<u8>> {
    let marker = b"\n---\n";
    let at = bytes.windows(marker.len()).take(256).position(|w| w == marker)?;
    let head = std::str::from_utf8(&bytes[..at]).ok()?;
    let mut lines = head.lines();
    if lines.next()? != OBJECT_MAGIC {
        return None;
    }
    let len: usize = lines.next()?.strip_prefix("len=")?.parse().ok()?;
    let sum = lines.next()?.strip_prefix("sum=")?.to_string();
    bytes.drain(..at + marker.len());
    if bytes.len() != len || format!("{:016x}", fast_sum(&bytes)) != sum {
        return None;
    }
    Some(bytes)
}

pub fn object_id(assembler: &str) -> String {
    let mut h = Sha256::new();
    push_field(&mut h, "assembler", assembler.as_bytes());
    h.hex()[..16].to_string()
}

pub fn lookup_object(slot: &Slot, id: &str) -> Option<Vec<u8>> {
    let path = object_path(&slot.dir, &slot.key, id);
    let obj = decode_object(std::fs::read(&path).ok()?)?;
    touch(&path);
    Some(obj)
}

pub fn store_object(slot: &Slot, id: &str, obj: &[u8]) -> bool {
    if !entry_path(&slot.dir, &slot.key).is_file() {
        return false;
    }
    write_atomic(&slot.dir, &object_path(&slot.dir, &slot.key, id), &encode_object(obj))
}

fn write_atomic(dir: &Path, target: &Path, data: &[u8]) -> bool {
    use std::io::Write;
    let objs = objects_dir(dir);
    if std::fs::create_dir_all(&objs).is_err() {
        return false;
    }
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = objs.join(format!(".tmp-{}-{}", std::process::id(), nonce));
    let written = std::fs::File::create(&tmp).and_then(|mut f| f.write_all(data).and_then(|_| f.flush()));
    if written.is_err() || std::fs::rename(&tmp, target).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    prune(dir, max_bytes());
    true
}

pub fn store(dir: &Path, key: &str, link: &LinkInfo, asm: &str) -> bool {
    let mut data = encode_head(link, asm).into_bytes();
    data.extend_from_slice(asm.as_bytes());
    write_atomic(dir, &entry_path(dir, key), &data)
}

pub fn prune(dir: &Path, cap: u64) {
    let rd = match std::fs::read_dir(objects_dir(dir)) {
        Ok(r) => r,
        Err(_) => return,
    };
    let now = SystemTime::now();
    let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let mut total = 0u64;
    for ent in rd.flatten() {
        let md = match ent.metadata() {
            Ok(m) if m.is_file() => m,
            _ => continue,
        };
        let mtime = md.modified().unwrap_or(now);
        let name = ent.file_name().to_string_lossy().to_string();
        if name.starts_with(".tmp-") {
            if now.duration_since(mtime).unwrap_or_default() > STALE_TMP {
                let _ = std::fs::remove_file(ent.path());
            }
            continue;
        }
        total += md.len();
        files.push((mtime, md.len(), ent.path()));
    }
    if total <= cap {
        return;
    }
    files.sort();
    let goal = cap / 10 * 8;
    for (_, len, path) in files {
        if total <= goal {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let d = std::env::temp_dir().join(format!("firn-cache-{}-{}-{}", tag, std::process::id(), nonce));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn entry(text: &str) -> Entry {
        Entry {
            link: LinkInfo {
                kernel: true,
                statics: false,
                libs: vec!["c".to_string(), "m".to_string()],
                exports: vec!["f".to_string()],
            },
            asm: text.to_string(),
        }
    }

    #[test]
    fn roundtrip_keeps_link_info_and_text() {
        let d = tmpdir("rt");
        let e = entry("line1\n---\nline2\n");
        assert!(store(&d, "k1", &e.link, &e.asm));
        let got = lookup(&d, "k1").unwrap();
        assert_eq!(got.asm, e.asm);
        assert_eq!(got.link, e.link);
        assert!(lookup(&d, "k2").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn corrupted_entry_is_a_miss() {
        let d = tmpdir("bad");
        let e = entry("hello world\n");
        assert!(store(&d, "k1", &e.link, &e.asm));
        let p = entry_path(&d, "k1");
        let mut b = std::fs::read(&p).unwrap();
        let n = b.len();
        b[n - 3] ^= 1;
        std::fs::write(&p, &b).unwrap();
        assert!(lookup(&d, "k1").is_none());
        std::fs::write(&p, b"garbage").unwrap();
        assert!(lookup(&d, "k1").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn prune_removes_oldest_first() {
        let d = tmpdir("prune");
        for i in 0..6 {
            let e = entry(&"x".repeat(1000));
            assert!(store(&d, &format!("k{}", i), &e.link, &e.asm));
            std::thread::sleep(Duration::from_millis(20));
        }
        let one = std::fs::metadata(entry_path(&d, "k0")).unwrap().len();
        prune(&d, one * 3);
        assert!(lookup(&d, "k5").is_some());
        assert!(lookup(&d, "k0").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn concurrent_stores_never_expose_partial_entries() {
        let d = tmpdir("conc");
        let text = "y".repeat(200_000);
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let d = d.clone();
                let text = text.clone();
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        let e = entry(&text);
                        assert!(store(&d, "same", &e.link, &e.asm));
                        let got = lookup(&d, "same").unwrap();
                        assert_eq!(got.asm.len(), text.len());
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn head_lookup_reads_link_info_and_asm_on_demand() {
        let d = tmpdir("head");
        let e = entry("body text\n");
        assert!(store(&d, "k1", &e.link, &e.asm));
        let h = lookup_head(&d, "k1").unwrap();
        assert_eq!(h.link, e.link);
        assert_eq!(h.read_asm().unwrap(), e.asm);
        let p = entry_path(&d, "k1");
        let mut b = std::fs::read(&p).unwrap();
        let n = b.len();
        b[n - 2] ^= 1;
        std::fs::write(&p, &b).unwrap();
        let h = lookup_head(&d, "k1").unwrap();
        assert!(h.read_asm().is_none());
        b.pop();
        std::fs::write(&p, &b).unwrap();
        assert!(lookup_head(&d, "k1").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn object_roundtrip_needs_entry_and_matching_id() {
        let d = tmpdir("obj");
        let slot = Slot { dir: d.clone(), key: "k1".to_string() };
        let obj = vec![0x7f, b'E', b'L', b'F', 0, 1, 2, 3];
        assert!(!store_object(&slot, "aa", &obj));
        let e = entry("x\n");
        assert!(store(&d, "k1", &e.link, &e.asm));
        assert!(store_object(&slot, "aa", &obj));
        assert_eq!(lookup_object(&slot, "aa").unwrap(), obj);
        assert!(lookup_object(&slot, "bb").is_none());
        let p = object_path(&d, "k1", "aa");
        let mut b = std::fs::read(&p).unwrap();
        let n = b.len();
        b[n - 1] ^= 1;
        std::fs::write(&p, &b).unwrap();
        assert!(lookup_object(&slot, "aa").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn object_id_depends_on_assembler_text() {
        assert_ne!(object_id("as 2.40"), object_id("as 2.41"));
        assert_eq!(object_id("as 2.40"), object_id("as 2.40"));
    }

    #[test]
    fn key_arguments_ignore_output_and_cache_switch() {
        let a: Vec<String> = ["x.fi", "-o", "out", "--no-cache", "-oother", "--emit=asm"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(key_args(&a), vec!["x.fi", "--emit=asm"]);
    }
}
