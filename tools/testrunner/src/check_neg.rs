use crate::cli::Options;
use crate::check_run::out_path;
use crate::exec;
use crate::model::Case;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

pub fn run_negative(o: &Options, work: &Path, rel: &str) -> Case {
    let t0 = Instant::now();
    let case = Case::new(rel, "default", "negative");
    let bin = out_path(work, "neg", rel);
    let mut cmd = Command::new(&o.compiler);
    cmd.current_dir(&o.root).arg("-o").arg(&bin).arg(rel);
    let out = match exec::run(&mut cmd, Duration::from_secs(o.timeout_secs)) {
        Ok(x) => x,
        Err(e) => return case.failed(format!("compiler cannot be started: {}", e), t0.elapsed().as_micros()),
    };
    let micros = t0.elapsed().as_micros();
    let _ = std::fs::remove_file(&bin);
    if out.timed_out {
        return case.failed(format!("compilation timed out after {} s", o.timeout_secs), micros);
    }
    let text = out.combined_text();
    if out.code == Some(0) {
        return case.failed("the compiler reported NO error".to_string(), micros);
    }
    if text.contains("panicked at") || text.contains("RUST_BACKTRACE") {
        return case.failed("a Rust panic instead of an error message".to_string(), micros);
    }
    let src = std::fs::read_to_string(o.root.join(rel)).unwrap_or_default();
    let mut lines = src.lines();
    let h1 = lines.next().unwrap_or("");
    let h2 = lines.next().unwrap_or("");
    let exp = match h1.split_once("expect_error:") {
        Some((_, e)) => e.trim().to_string(),
        None => return case.failed("no expect_error line".to_string(), micros),
    };
    let (pos, msg) = exp.split_once(' ').unwrap_or((exp.as_str(), ""));
    if !text.contains(&format!(":{}", pos)) {
        return case.failed(format!("position '{}' is missing from the message", pos), micros);
    }
    if !text.contains(msg) {
        return case.failed(format!("text '{}' is missing from the message", msg), micros);
    }
    if !text.contains('^') {
        return case.failed("no marker (^) in the message".to_string(), micros);
    }
    if let Some((_, c)) = h2.split_once("expect_error_count:") {
        let want: usize = c.trim().parse().unwrap_or(0);
        let got = text.lines().filter(|l| l.starts_with("error:")).count();
        if got != want {
            return case.failed(format!("{} errors reported, expected {}", got, want), micros);
        }
    }
    case.passed(micros)
}
