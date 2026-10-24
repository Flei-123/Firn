use crate::check_run::{first_line, out_path};
use crate::cli::Options;
use crate::exec;
use crate::json;
use crate::location;
use crate::model::Case;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

const LEVEL: &str = "devfast";

fn unit_case(rel: &str, row: &std::collections::HashMap<String, json::Value>) -> Option<Case> {
    let name = row.get("name")?.as_str()?;
    let file = row.get("file").and_then(|v| v.as_str()).unwrap_or(rel);
    let micros = row.get("us").and_then(|v| v.as_i64()).unwrap_or(0).max(0) as u128;
    let line = row.get("line").and_then(|v| v.as_i64()).unwrap_or(1).max(1) as u32;
    let col = row.get("col").and_then(|v| v.as_i64()).unwrap_or(1).max(1) as u32;
    let case = Case::new(file, LEVEL, "unit").named(format!("{}::{}", rel, name)).at(line, col);
    if row.get("status").and_then(|v| v.as_str()) == Some("pass") {
        return Some(case.passed(micros));
    }
    let reason = row.get("reason").and_then(|v| v.as_str()).unwrap_or("");
    let signal = row.get("signal").and_then(|v| v.as_i64()).unwrap_or(0);
    let exit = row.get("exit").and_then(|v| v.as_i64()).unwrap_or(0);
    let msg = if !reason.is_empty() {
        reason.to_string()
    } else if signal != 0 {
        format!("killed by signal {}", signal)
    } else {
        format!("exit code {}", exit)
    };
    Some(case.failed(msg, micros))
}

pub fn run_unit(o: &Options, work: &Path, rel: &str) -> Vec<Case> {
    let t0 = Instant::now();
    let bin = out_path(work, "unit", rel);
    let mut cmd = Command::new(&o.compiler);
    cmd.current_dir(&o.root)
        .args(["--test", "--format=json", "--opt-level=dev-fast"])
        .arg("-o")
        .arg(&bin)
        .arg(rel);
    let limit = Duration::from_secs(o.timeout_secs);
    let out = exec::run(&mut cmd, limit);
    let _ = std::fs::remove_file(&bin);
    let whole = Case::new(rel, LEVEL, "unit");
    let out = match out {
        Ok(x) => x,
        Err(e) => return vec![whole.failed(format!("compiler cannot be started: {}", e), t0.elapsed().as_micros())],
    };
    let micros = t0.elapsed().as_micros();
    if out.timed_out {
        return vec![whole.failed(format!("test binary timed out after {} s", o.timeout_secs), micros)];
    }
    let text = out.stdout_text();
    let cases: Vec<Case> = text
        .lines()
        .filter(|l| l.trim_start().starts_with("{\"name\""))
        .filter_map(json::parse_flat)
        .filter_map(|row| unit_case(rel, &row))
        .collect();
    if !cases.is_empty() {
        return cases;
    }
    let all = out.combined_text();
    let c = whole.failed(format!("no test report: {}", first_line(&all)), micros);
    match location::first(&all) {
        Some((_, l, col)) => vec![c.at(l, col)],
        None => vec![c],
    }
}
