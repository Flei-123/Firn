use crate::cli::Options;
use crate::exec;
use crate::location;
use crate::model::Case;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

pub fn level_flags(level: &str) -> &'static [&'static str] {
    match level {
        "noopt" => &["--no-opt"],
        "devfast" => &["--opt-level=dev-fast"],
        "safe" => &["--opt-level=release-safe"],
        _ => &["--opt-level=release-fast"],
    }
}

pub fn out_path(work: &Path, group: &str, rel: &str) -> std::path::PathBuf {
    let dir = work.join(group);
    let _ = std::fs::create_dir_all(&dir);
    dir.join(rel.trim_end_matches(".fi").replace('/', "_"))
}

pub fn first_line(text: &str) -> String {
    text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").to_string()
}

fn header(o: &Options, rel: &str) -> String {
    let src = std::fs::read_to_string(o.root.join(rel)).unwrap_or_default();
    src.lines().next().unwrap_or("").to_string()
}

fn after(hdr: &str, key: &str) -> String {
    hdr.split_once(key).map(|x| x.1.to_string()).unwrap_or_else(|| hdr.to_string())
}

fn compile(o: &Options, work: &Path, rel: &str, level: &str) -> Result<std::path::PathBuf, (String, Option<(u32, u32)>)> {
    let bin = out_path(work, level, rel);
    let mut cmd = Command::new(&o.compiler);
    cmd.current_dir(&o.root).args(level_flags(level)).arg("-o").arg(&bin).arg(rel);
    let out = exec::run(&mut cmd, Duration::from_secs(o.timeout_secs)).map_err(|e| (format!("compiler cannot be started: {}", e), None))?;
    if out.timed_out {
        return Err((format!("compilation timed out after {} s", o.timeout_secs), None));
    }
    if out.code == Some(0) {
        return Ok(bin);
    }
    let text = out.combined_text();
    let pos = location::first(&text).map(|(_, l, c)| (l, c));
    Err((format!("compilation: {}", first_line(&text)), pos))
}

pub fn run_program(o: &Options, work: &Path, rel: &str, level: &str) -> Case {
    let t0 = Instant::now();
    let case = Case::new(rel, level, "run");
    let bin = match compile(o, work, rel, level) {
        Ok(b) => b,
        Err((msg, pos)) => {
            let c = case.failed(msg, t0.elapsed().as_micros());
            return match pos {
                Some((l, col)) => c.at(l, col),
                None => c,
            };
        }
    };
    let mut cmd = Command::new(&bin);
    cmd.current_dir(&o.root);
    let result = exec::run(&mut cmd, Duration::from_secs(o.timeout_secs));
    let _ = std::fs::remove_file(&bin);
    let run = match result {
        Ok(r) => r,
        Err(e) => return case.failed(format!("program cannot be started: {}", e), t0.elapsed().as_micros()),
    };
    let micros = t0.elapsed().as_micros();
    if run.timed_out {
        return case.failed(format!("program timed out after {} s", o.timeout_secs), micros);
    }
    let hdr = header(o, rel);
    let stderr = run.stderr_text();
    let at_panic = |c: Case| match location::last(&stderr) {
        Some((_, l, col)) => c.at(l, col),
        None => c,
    };
    let with_stderr = |msg: String| {
        let first = first_line(&stderr);
        if first.is_empty() { msg } else { format!("{} ({})", msg, first) }
    };
    if hdr.contains("expect_out:") {
        let want = after(&hdr, "expect_out: ");
        if run.code != Some(0) {
            let msg = with_stderr(format!("program ended with {} (expected exit code 0)", run.describe_exit()));
            return at_panic(case.failed(msg, micros));
        }
        let got = run.stdout_text().trim_end_matches('\n').to_string();
        if got == want {
            return case.passed(micros);
        }
        return case.failed(format!("output '{}', expected '{}'", got, want), micros);
    }
    if hdr.contains("expect_exit:") {
        let want = after(&hdr, "expect_exit: ");
        if run.code.map(|c| c.to_string()) == Some(want.clone()) {
            return case.passed(micros);
        }
        let msg = with_stderr(format!("{}, expected exit code {}", run.describe_exit(), want));
        return at_panic(case.failed(msg, micros));
    }
    case.failed("no expectation in line 1 (// expect_exit: / // expect_out:)".to_string(), micros)
}
