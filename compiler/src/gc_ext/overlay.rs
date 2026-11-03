use std::collections::{HashMap, HashSet};

pub(crate) struct Item<'a> {
    pub name: &'a str,
    pub start: usize,
    pub end: usize,
}

const HEADS: &[&str] = &["pub fn ", "fn ", "const ", "static mut ", "static ", "struct ", "gc class ", "error ", "enum "];

fn head_name(line: &str) -> Option<(&str, bool)> {
    for h in HEADS {
        if let Some(rest) = line.strip_prefix(h) {
            let n = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(rest.len());
            if n == 0 {
                return None;
            }
            let single = h.starts_with("const") || h.starts_with("static");
            return Some((&rest[..n], single));
        }
    }
    None
}

pub(crate) fn items(text: &str) -> Vec<Item<'_>> {
    let mut out = Vec::new();
    let mut lines = Vec::new();
    let mut pos = 0;
    for l in text.split_inclusive('\n') {
        lines.push((pos, l));
        pos += l.len();
    }
    let mut i = 0;
    let mut attr_start: Option<usize> = None;
    while i < lines.len() {
        let (off, raw) = lines[i];
        let line = raw.trim_end_matches(['\n', '\r']);
        if line.starts_with("#[") {
            attr_start.get_or_insert(off);
            i += 1;
            continue;
        }
        match head_name(line) {
            Some((name, single)) => {
                let mut j = i;
                if !single && !line.trim_end().ends_with('}') {
                    while j + 1 < lines.len() && lines[j].1.trim_end_matches(['\n', '\r']) != "}" {
                        j += 1;
                    }
                }
                let end = lines[j].0 + lines[j].1.len();
                out.push(Item { name, start: attr_start.unwrap_or(off), end });
                i = j + 1;
            }
            None => i += 1,
        }
        attr_start = None;
    }
    out
}

pub(crate) fn overlay(core: &str, ext: &str) -> String {
    let ext_items = items(ext);
    let by_name: HashMap<&str, &str> = ext_items.iter().map(|it| (it.name, &ext[it.start..it.end])).collect();
    let mut used: HashSet<&str> = HashSet::new();
    let mut out = String::with_capacity(core.len() + ext.len());
    let mut pos = 0;
    for it in items(core) {
        if let Some(t) = by_name.get(it.name) {
            out.push_str(&core[pos..it.start]);
            out.push_str(t);
            pos = it.end;
            used.insert(it.name);
        }
    }
    out.push_str(&core[pos..]);
    for it in &ext_items {
        if !used.contains(it.name) {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&ext[it.start..it.end]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_in_place_and_appends_new() {
        let core = "const A: u64 = 1\nfn f() {\n    g()\n}\n#[inline]\nfn g() {\n}\n";
        let ext = "const A: u64 = 2\nfn g() {\n    h()\n}\nfn h() {\n}\n";
        let o = overlay(core, ext);
        assert_eq!(o, "const A: u64 = 2\nfn f() {\n    g()\n}\nfn g() {\n    h()\n}\nfn h() {\n}\n");
    }
}
