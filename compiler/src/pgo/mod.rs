// SPDX-License-Identifier: MPL-2.0

pub mod format;
pub mod hash;
pub mod instrument;
pub mod layout;
pub mod weights;

use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

pub const VERSION: u32 = 1;

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub gen: Option<Option<PathBuf>>,
    pub use_path: Option<PathBuf>,
}

static CONFIG: RwLock<Option<Config>> = RwLock::new(None);
static GEN_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn parse_arg(a: &str) -> Option<Result<(), String>> {
    let mut cfg = CONFIG.write().unwrap_or_else(|e| e.into_inner());
    let c = cfg.get_or_insert_with(Config::default);
    if a == "--profile-gen" {
        c.gen = Some(None);
        return Some(Ok(()));
    }
    if let Some(p) = a.strip_prefix("--profile-gen=") {
        if p.is_empty() {
            return Some(Err("--profile-gen= expects a path".to_string()));
        }
        c.gen = Some(Some(PathBuf::from(p)));
        return Some(Ok(()));
    }
    if let Some(p) = a.strip_prefix("--profile-use=") {
        if p.is_empty() {
            return Some(Err("--profile-use= expects a profile file".to_string()));
        }
        c.use_path = Some(PathBuf::from(p));
        return Some(Ok(()));
    }
    None
}

fn config() -> Config {
    CONFIG.read().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

pub fn active() -> bool {
    let c = config();
    c.gen.is_some() || c.use_path.is_some()
}

pub fn gen_requested() -> bool {
    config().gen.is_some()
}

pub fn finish_config(output: Option<&Path>, input: &Path) -> Result<(), String> {
    let c = config();
    if c.gen.is_some() && c.use_path.is_some() {
        return Err("--profile-gen and --profile-use exclude each other".to_string());
    }
    if let Some(g) = c.gen {
        let base = match (g, output) {
            (Some(p), _) => p,
            (None, Some(o)) => PathBuf::from(format!("{}.fprof", o.display())),
            (None, None) => input.with_extension("fprof"),
        };
        let abs = if base.is_absolute() {
            base
        } else {
            std::env::current_dir().map_err(|e| e.to_string())?.join(base)
        };
        let _ = GEN_PATH.set(abs);
    }
    Ok(())
}

pub fn gen_path() -> Option<&'static PathBuf> {
    GEN_PATH.get()
}

pub fn use_path() -> Option<PathBuf> {
    config().use_path
}

pub fn check_target() -> Result<(), String> {
    if !gen_requested() {
        return Ok(());
    }
    let t = crate::target::active();
    if t != crate::target::Target::X86_64 || crate::target::windows() || crate::target::android() {
        return Err("--profile-gen supports only x86-64 Linux executables for now".to_string());
    }
    Ok(())
}
