use crate::cli::Options;
use std::path::{Path, PathBuf};

pub enum Job {
    Run { rel: String, level: String },
    Negative { rel: String },
    Unit { rel: String },
    Skip { rel: String, level: String, reason: &'static str },
}

fn fi_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x == "fi").unwrap_or(false) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn relative(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).display().to_string()
}

fn selected(o: &Options, rel: &str) -> bool {
    match &o.filter {
        Some(s) => rel.contains(s.as_str()),
        None => true,
    }
}

fn head_has(src: &str, line: &str) -> bool {
    src.lines().take(5).any(|l| l == line)
}

fn is_unit_file(src: &str) -> bool {
    src.lines().any(|l| l.starts_with("#[test]")) && !src.lines().any(|l| l.starts_with("fn main"))
}

fn skip_reason(src: &str, level: &str) -> Option<&'static str> {
    if level != "opt" && head_has(src, "// only_mode: opt") {
        return Some("only_mode: opt");
    }
    if level == "opt" && head_has(src, "// not_mode: opt") {
        return Some("not_mode: opt");
    }
    None
}

pub fn collect(o: &Options) -> Vec<Job> {
    let mut jobs = Vec::new();
    for dir in ["tests", "tests/opt", "examples"] {
        for f in fi_files(&o.root.join(dir)) {
            let rel = relative(&o.root, &f);
            if !selected(o, &rel) {
                continue;
            }
            let src = std::fs::read_to_string(&f).unwrap_or_default();
            if is_unit_file(&src) {
                if o.levels.iter().any(|l| l == "devfast") {
                    jobs.push(Job::Unit { rel });
                }
                continue;
            }
            for level in &o.levels {
                match skip_reason(&src, level) {
                    Some(reason) => jobs.push(Job::Skip { rel: rel.clone(), level: level.clone(), reason }),
                    None => jobs.push(Job::Run { rel: rel.clone(), level: level.clone() }),
                }
            }
        }
    }
    for f in fi_files(&o.root.join("tests/neg")) {
        let rel = relative(&o.root, &f);
        if selected(o, &rel) {
            jobs.push(Job::Negative { rel });
        }
    }
    jobs
}
