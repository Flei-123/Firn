// SPDX-License-Identifier: GPL-2.0-only
//! Test runner with machine-readable output (requirement `W2`).
//!
//! It compiles and starts the same test programs as `test.sh`, but reports the
//! result either as text or as JSON and gives the rate
//! as a number -- that makes it fit for CI.
//!
//! Usage:
//!   testrunner [--root <directory>] [--compiler <path>] [--format=json|text]
//!              [--filter <substring>] [--quiet]
//!
//! Exit code: 0 = every case passed, 1 = at least one failure,
//! 2 = a usage or environment error (compiler missing, directory missing).
//!
//! The expectations stand in line 1 of the test program:
//!   `// expect_exit: N`   the exit code of the program
//!   `// expect_out: TEXT` standard output (without a line end at the end)
//!   `// expect_error: L:C TEXT` (only under tests/neg/) the compilation MUST
//!                              fail and report the position + the text
//! Optionally line 2: `// expect_error_count: N`

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Text,
    Json,
}

struct Options {
    root: PathBuf,
    compiler: PathBuf,
    format: Format,
    filter: Option<String>,
    quiet: bool,
}

struct Case {
    name: String,
    mode: &'static str,
    status: &'static str,
    message: String,
    ms: u128,
}

fn usage() -> &'static str {
    "Aufruf: testrunner [--root <verzeichnis>] [--compiler <pfad>]\n\
     \x20                 [--format=json|text] [--filter <teilstring>] [--quiet]\n"
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut root = PathBuf::from(".");
    let mut compiler: Option<PathBuf> = None;
    let mut format = Format::Text;
    let mut filter = None;
    let mut quiet = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("{}", usage());
                std::process::exit(0);
            }
            "--quiet" => quiet = true,
            "--root" => {
                i += 1;
                root = PathBuf::from(args.get(i).ok_or("--root erwartet ein Verzeichnis")?);
            }
            "--compiler" => {
                i += 1;
                compiler = Some(PathBuf::from(
                    args.get(i).ok_or("--compiler erwartet einen Pfad")?,
                ));
            }
            "--filter" => {
                i += 1;
                filter = Some(args.get(i).ok_or("--filter erwartet einen Text")?.clone());
            }
            a => {
                if let Some(f) = a.strip_prefix("--format=") {
                    format = match f {
                        "json" => Format::Json,
                        "text" => Format::Text,
                        other => return Err(format!("unbekanntes Format '{}'", other)),
                    };
                } else {
                    return Err(format!("unbekannte Option '{}'", a));
                }
            }
        }
        i += 1;
    }
    let compiler = compiler.unwrap_or_else(|| root.join("compiler/target/release/firnc"));
    Ok(Options { root, compiler, format, filter, quiet })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = match parse_args(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("error: {}", e);
            eprint!("{}", usage());
            std::process::exit(2);
        }
    };
    if !opts.compiler.exists() {
        eprintln!(
            "error: Compiler '{}' nicht gefunden (vorher 'cargo build --release' laufen lassen)",
            opts.compiler.display()
        );
        std::process::exit(2);
    }
    let work = opts.root.join(".testrunner-work");
    let _ = std::fs::remove_dir_all(&work);
    if let Err(e) = std::fs::create_dir_all(&work) {
        eprintln!("error: kann '{}' nicht anlegen: {}", work.display(), e);
        std::process::exit(2);
    }

    let mut cases: Vec<Case> = Vec::new();
    for dir in ["tests", "tests/opt", "examples"] {
        for f in fi_files(&opts.root.join(dir)) {
            if skip(&opts, &f) {
                continue;
            }
            for mode in ["opt", "noopt"] {
                cases.push(run_positive(&opts, &work, &f, mode));
            }
        }
    }
    for f in fi_files(&opts.root.join("tests/neg")) {
        if skip(&opts, &f) {
            continue;
        }
        cases.push(run_negative(&opts, &work, &f));
    }

    let passed = cases.iter().filter(|c| c.status == "pass").count();
    let total = cases.len();
    let failed = total - passed;
    match opts.format {
        Format::Json => println!("{}", json(&cases, passed, total)),
        Format::Text => {
            for c in &cases {
                if c.status == "pass" {
                    if !opts.quiet {
                        println!("PASS  {} [{}]", c.name, c.mode);
                    }
                } else {
                    println!("FAIL  {} [{}]: {}", c.name, c.mode, c.message);
                }
            }
            let rate = if total == 0 { 0.0 } else { passed as f64 * 100.0 / total as f64 };
            println!("{} von {} bestanden ({:.1} %)", passed, total, rate);
        }
    }
    std::process::exit(if failed == 0 { 0 } else { 1 });
}

fn skip(o: &Options, f: &Path) -> bool {
    match &o.filter {
        Some(s) => !f.display().to_string().contains(s.as_str()),
        None => false,
    }
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

fn head_lines(path: &Path) -> (String, String) {
    let src = std::fs::read_to_string(path).unwrap_or_default();
    let mut it = src.lines();
    (
        it.next().unwrap_or("").to_string(),
        it.next().unwrap_or("").to_string(),
    )
}

fn case(name: &Path, mode: &'static str, status: &'static str, message: String, ms: u128) -> Case {
    Case {
        name: name.display().to_string(),
        mode,
        status,
        message,
        ms,
    }
}

fn run_positive(o: &Options, work: &Path, f: &Path, mode: &'static str) -> Case {
    let t0 = Instant::now();
    let stem = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let bin = work.join(format!("{}.{}", stem, mode));
    let mut cmd = Command::new(&o.compiler);
    if mode == "noopt" {
        cmd.arg("--no-opt");
    }
    let out = cmd.arg("-o").arg(&bin).arg(f).output();
    let out = match out {
        Ok(x) => x,
        Err(e) => {
            return case(f, mode, "fail", format!("Compiler nicht startbar: {}", e), t0.elapsed().as_millis())
        }
    };
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr)
            .lines()
            .next()
            .unwrap_or("Uebersetzung fehlgeschlagen")
            .to_string();
        return case(f, mode, "fail", format!("Uebersetzung: {}", msg), t0.elapsed().as_millis());
    }
    let (h1, _) = head_lines(f);
    let run = match Command::new(&bin).output() {
        Ok(r) => r,
        Err(e) => return case(f, mode, "fail", format!("Programm nicht startbar: {}", e), t0.elapsed().as_millis()),
    };
    let ms = t0.elapsed().as_millis();
    if let Some(exp) = h1.split("expect_exit:").nth(1) {
        let want: i32 = exp.trim().parse().unwrap_or(-1);
        let got = run.status.code().unwrap_or(-1);
        if got == want {
            return case(f, mode, "pass", String::new(), ms);
        }
        return case(f, mode, "fail", format!("Exit-Code {}, erwartet {}", got, want), ms);
    }
    if let Some(exp) = h1.split("expect_out:").nth(1) {
        let want = exp.trim_start().to_string();
        let got = String::from_utf8_lossy(&run.stdout).trim_end_matches('\n').to_string();
        if got == want {
            return case(f, mode, "pass", String::new(), ms);
        }
        return case(f, mode, "fail", format!("Ausgabe '{}', erwartet '{}'", got, want), ms);
    }
    case(f, mode, "fail", "keine Erwartung in Zeile 1".to_string(), ms)
}

fn run_negative(o: &Options, work: &Path, f: &Path) -> Case {
    let t0 = Instant::now();
    let stem = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let bin = work.join(format!("{}.neg", stem));
    let out = match Command::new(&o.compiler).arg("-o").arg(&bin).arg(f).output() {
        Ok(x) => x,
        Err(e) => return case(f, "neg", "fail", format!("Compiler nicht startbar: {}", e), t0.elapsed().as_millis()),
    };
    let ms = t0.elapsed().as_millis();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if out.status.success() {
        return case(f, "neg", "fail", "Compiler meldete KEINEN Fehler".to_string(), ms);
    }
    if text.contains("panicked at") || text.contains("RUST_BACKTRACE") {
        return case(f, "neg", "fail", "Rust-Panik statt Fehlermeldung".to_string(), ms);
    }
    let (h1, h2) = head_lines(f);
    let exp = match h1.split("expect_error:").nth(1) {
        Some(e) => e.trim().to_string(),
        None => return case(f, "neg", "fail", "keine expect_error-Zeile".to_string(), ms),
    };
    let mut it = exp.splitn(2, ' ');
    let pos = it.next().unwrap_or("").to_string();
    let msg = it.next().unwrap_or("").to_string();
    if !text.contains(&format!(":{}", pos)) {
        return case(f, "neg", "fail", format!("Position '{}' fehlt", pos), ms);
    }
    if !text.contains(&msg) {
        return case(f, "neg", "fail", format!("Text '{}' fehlt", msg), ms);
    }
    if !text.contains('^') {
        return case(f, "neg", "fail", "keine Markierung (^)".to_string(), ms);
    }
    if let Some(c) = h2.split("expect_error_count:").nth(1) {
        let want: usize = c.trim().parse().unwrap_or(0);
        let got = text.lines().filter(|l| l.starts_with("error:")).count();
        if got != want {
            return case(f, "neg", "fail", format!("{} Fehler gemeldet, erwartet {}", got, want), ms);
        }
    }
    case(f, "neg", "pass", String::new(), ms)
}

fn esc(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn json(cases: &[Case], passed: usize, total: usize) -> String {
    let failed = total - passed;
    let rate = if total == 0 { 0.0 } else { passed as f64 / total as f64 };
    let mut s = String::new();
    let _ = write!(
        s,
        "{{\"suite\":\"firn\",\"total\":{},\"passed\":{},\"failed\":{},\"rate\":{:.4},\"cases\":[",
        total, passed, failed, rate
    );
    for (i, c) in cases.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(
            s,
            "{{\"name\":\"{}\",\"mode\":\"{}\",\"status\":\"{}\",\"message\":\"{}\",\"duration_ms\":{}}}",
            esc(&c.name),
            c.mode,
            c.status,
            esc(&c.message),
            c.ms
        );
    }
    s.push_str("]}");
    s
}
