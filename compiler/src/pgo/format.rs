// SPDX-License-Identifier: MPL-2.0

use super::VERSION;

pub const MAGIC: &str = "FIRNPGO";
pub const END: &str = "END\n";

#[derive(Clone, Debug, PartialEq)]
pub struct FileEntry {
    pub path: String,
    pub hash: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FuncEntry {
    pub name: String,
    pub hash: u64,
    pub blocks: Vec<Vec<(u32, u32, u32)>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Descriptor {
    pub files: Vec<FileEntry>,
    pub funcs: Vec<FuncEntry>,
}

impl Descriptor {
    pub fn counters(&self) -> usize {
        self.funcs.iter().map(|f| f.blocks.len()).sum()
    }

    pub fn to_text(&self) -> String {
        let mut s = format!("{} {}\n", MAGIC, VERSION);
        for f in &self.files {
            s.push_str(&format!("S\t{:016x}\t{}\n", f.hash, f.path));
        }
        for f in &self.funcs {
            s.push_str(&format!("U\t{:016x}\t{}\t{}\n", f.hash, f.blocks.len(), f.name));
            for b in &f.blocks {
                let locs: Vec<String> = b.iter().map(|(fi, l, c)| format!("{}:{}:{}", fi, l, c)).collect();
                s.push_str(&format!("B\t{}\n", locs.join(" ")));
            }
        }
        s.push_str(END);
        s
    }
}

pub struct Profile {
    pub desc: Descriptor,
    pub counts: Vec<u64>,
}

fn parse_loc(t: &str) -> Option<(u32, u32, u32)> {
    let mut it = t.split(':');
    let a = it.next()?.parse().ok()?;
    let b = it.next()?.parse().ok()?;
    let c = it.next()?.parse().ok()?;
    Some((a, b, c))
}

pub fn parse(bytes: &[u8]) -> Result<Profile, String> {
    let end = bytes
        .windows(END.len())
        .position(|w| w == END.as_bytes())
        .ok_or("profile: missing END marker")?;
    let head = std::str::from_utf8(&bytes[..end]).map_err(|_| "profile: header is not UTF-8")?;
    let mut lines = head.lines();
    let first = lines.next().ok_or("profile: empty")?;
    let mut fw = first.split(' ');
    if fw.next() != Some(MAGIC) {
        return Err("profile: bad magic".to_string());
    }
    let ver: u32 = fw.next().and_then(|v| v.parse().ok()).ok_or("profile: bad version")?;
    if ver != VERSION {
        return Err(format!("profile: version {} not supported (expected {})", ver, VERSION));
    }
    let mut desc = Descriptor::default();
    for line in lines {
        let mut parts = line.splitn(4, '\t');
        match parts.next() {
            Some("S") => {
                let h = u64::from_str_radix(parts.next().unwrap_or(""), 16).map_err(|_| "profile: bad file hash")?;
                let p = parts.next().unwrap_or("").to_string();
                desc.files.push(FileEntry { path: p, hash: h });
            }
            Some("U") => {
                let h = u64::from_str_radix(parts.next().unwrap_or(""), 16).map_err(|_| "profile: bad function hash")?;
                let _n = parts.next();
                let name = parts.next().unwrap_or("").to_string();
                desc.funcs.push(FuncEntry { name, hash: h, blocks: Vec::new() });
            }
            Some("B") => {
                let f = desc.funcs.last_mut().ok_or("profile: block before function")?;
                let locs = parts.next().unwrap_or("").split(' ').filter(|t| !t.is_empty()).filter_map(parse_loc).collect();
                f.blocks.push(locs);
            }
            _ => return Err(format!("profile: bad line '{}'", line)),
        }
    }
    let body = &bytes[end + END.len()..];
    let n = desc.counters();
    if body.len() != n * 8 {
        return Err(format!("profile: {} counter bytes, expected {}", body.len(), n * 8));
    }
    let counts = body.chunks_exact(8).map(|c| u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]])).collect();
    Ok(Profile { desc, counts })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let d = Descriptor {
            files: vec![FileEntry { path: "a b.fi".to_string(), hash: 7 }],
            funcs: vec![FuncEntry { name: "main".to_string(), hash: 9, blocks: vec![vec![(0, 3, 4)], vec![]] }],
        };
        let mut bytes = d.to_text().into_bytes();
        bytes.extend_from_slice(&5u64.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        let p = parse(&bytes).unwrap();
        assert_eq!(p.desc, d);
        assert_eq!(p.counts, vec![5, 0]);
    }

    #[test]
    fn rejects_other_version() {
        assert!(parse(b"FIRNPGO 99\nEND\n").is_err());
    }
}
