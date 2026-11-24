// SPDX-License-Identifier: MPL-2.0
use super::{entry, libsystem};
use crate::target;
use std::path::{Path, PathBuf};
use std::process::Command;

const ASSEMBLER_ENV: &str = "FIRN_DARWIN_CC";
const LINKER_ENV: &str = "FIRN_DARWIN_LD";
const DEFAULT_ASSEMBLER: &str = "clang";
const DEFAULT_LINKER: &str = "ld64.lld";
const MIN_OS_ARM64: &str = "11.0";
const MIN_OS_X86_64: &str = "10.15";

fn tool(env: &str, default: &str) -> String {
    std::env::var(env).unwrap_or_else(|_| default.to_string())
}

fn is_arm64() -> bool {
    target::active() == target::Target::Aarch64
}

fn triple() -> String {
    if is_arm64() {
        format!("arm64-apple-macos{}", MIN_OS_ARM64)
    } else {
        format!("x86_64-apple-macos{}", MIN_OS_X86_64)
    }
}

fn run(mut cmd: Command, name: &str) -> Result<(), i32> {
    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => {
            eprintln!("error: '{}' failed ({})", name, s);
            Err(3)
        }
        Err(e) => {
            eprintln!("error: cannot run '{}': {} (clang and lld installed?)", name, e);
            Err(3)
        }
    }
}

pub fn assemble(asm: &Path, obj: &Path) -> Result<(), i32> {
    let name = tool(ASSEMBLER_ENV, DEFAULT_ASSEMBLER);
    let mut cmd = Command::new(&name);
    cmd.args(["-target", &triple()]);
    if is_arm64() {
        cmd.arg("-mcpu=apple-m1");
    }
    cmd.args(["-c", "-x", "assembler", "-o"]).arg(obj).arg(asm);
    run(cmd, &name)
}

fn stub_dir() -> Result<PathBuf, i32> {
    let dir = std::env::temp_dir().join(format!("firn-macos-{}", std::process::id()));
    let made = std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(dir.join("libSystem.tbd"), libsystem::tbd_text()));
    match made {
        Ok(()) => Ok(dir),
        Err(e) => {
            eprintln!("error: cannot write the libSystem stub: {}", e);
            Err(3)
        }
    }
}

pub fn link(obj: &Path, out: &Path) -> Result<(), i32> {
    let dir = stub_dir()?;
    let name = tool(LINKER_ENV, DEFAULT_LINKER);
    let (arch, min) = if is_arm64() { ("arm64", MIN_OS_ARM64) } else { ("x86_64", MIN_OS_X86_64) };
    let mut cmd = Command::new(&name);
    cmd.args(["-arch", arch, "-platform_version", "macos", min, min]);
    cmd.arg(format!("-L{}", dir.display()));
    cmd.args(["-lSystem", "-e", entry::ENTRY_SYMBOL, "-o"]).arg(out).arg(obj);
    let res = run(cmd, &name);
    let _ = std::fs::remove_dir_all(&dir);
    res
}
