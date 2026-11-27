// SPDX-License-Identifier: MPL-2.0
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct Finding {
    pub line: u32,
    pub col: u32,
    pub len: u32,
    pub message: String,
    pub severity: i64,
}

const SEVERITY_ERROR: i64 = 1;
const SEVERITY_WARNING: i64 = 2;

pub fn check_buffer(real_path: &str, src: &str) -> Option<Vec<Finding>> {
    let exe = std::env::current_exe().ok()?;
    let temp = temp_beside(real_path)?;
    std::fs::write(&temp, src).ok()?;
    let output = Command::new(exe)
        .arg("--emit=fir-raw")
        .arg(&temp)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();
    let _ = std::fs::remove_file(&temp);
    let output = output.ok()?;
    let text = String::from_utf8_lossy(&output.stderr).to_string();
    let temp_name = temp.file_name()?.to_string_lossy().to_string();
    Some(parse_report(&text, &temp_name))
}

fn temp_beside(real_path: &str) -> Option<PathBuf> {
    let dir = Path::new(real_path).parent()?;
    if !dir.is_dir() {
        return None;
    }
    Some(dir.join(format!(".firn-lsp-{}.fi", std::process::id())))
}

fn parse_report(text: &str, temp_name: &str) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    let mut foreign_file = String::new();
    for raw in text.lines() {
        let line = raw.trim_end();
        if let Some(msg) = line.strip_prefix("error: ") {
            out.push(finding(msg, SEVERITY_ERROR));
            foreign_file.clear();
        } else if let Some(msg) = line.strip_prefix("warning: ") {
            out.push(finding(msg, SEVERITY_WARNING));
            foreign_file.clear();
        } else if let Some(rest) = line.trim_start().strip_prefix("--> ") {
            if let Some(last) = out.last_mut() {
                foreign_file = place(last, rest, temp_name);
            }
        } else if let Some(note) = line.trim_start().strip_prefix("= ") {
            if let Some(last) = out.last_mut() {
                last.message.push('\n');
                last.message.push_str(note);
            }
        } else if let Some(carets) = caret_run(line) {
            if let Some(last) = out.last_mut() {
                if foreign_file.is_empty() {
                    last.len = carets;
                }
            }
        }
    }
    out.retain(|f| !f.message.ends_with("errors found") && !f.message.ends_with("error found"));
    out
}

fn finding(message: &str, severity: i64) -> Finding {
    Finding { line: 1, col: 1, len: 1, message: message.to_string(), severity }
}

fn place(target: &mut Finding, rest: &str, temp_name: &str) -> String {
    let mut parts = rest.rsplitn(3, ':');
    let col = parts.next().and_then(|c| c.trim().parse::<u32>().ok());
    let line = parts.next().and_then(|l| l.trim().parse::<u32>().ok());
    let file = parts.next().unwrap_or("");
    let (Some(line), Some(col)) = (line, col) else {
        return String::new();
    };
    if file.ends_with(temp_name) {
        target.line = line;
        target.col = col;
        return String::new();
    }
    target.message = format!("in {}:{}:{}: {}", file, line, col, target.message);
    file.to_string()
}

fn caret_run(line: &str) -> Option<u32> {
    let after_bar = line.split_once('|')?.1;
    let trimmed = after_bar.trim_start();
    if !trimmed.starts_with('^') {
        return None;
    }
    Some(trimmed.chars().take_while(|c| *c == '^').count() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = "error: unknown function 'foo'\n  --> /w/.firn-lsp-1.fi:3:3\n   |\n 3 |   foo()\n   |   ^^^ here\n   = help: did you mean 'bar'?\n1 error found\n";

    #[test]
    fn report_becomes_findings() {
        let found = parse_report(REPORT, ".firn-lsp-1.fi");
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].line, found[0].col, found[0].len), (3, 3, 3));
        assert!(found[0].message.contains("help: did you mean 'bar'?"));
    }

    #[test]
    fn foreign_file_errors_land_on_the_first_line() {
        let report = "error: boom\n  --> /lib/x.fi:9:4\n   |\n 9 | x\n   | ^ here\n";
        let found = parse_report(report, ".firn-lsp-1.fi");
        assert_eq!((found[0].line, found[0].col), (1, 1));
        assert!(found[0].message.starts_with("in /lib/x.fi:9:4: boom"));
    }
}
