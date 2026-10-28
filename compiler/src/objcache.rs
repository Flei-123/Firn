// SPDX-License-Identifier: MPL-2.0
use crate::buildcache::{self, Slot};
use crate::{asm_intern, target};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

static TOOL_ID: OnceLock<Option<String>> = OnceLock::new();

fn tool_id() -> Option<String> {
    TOOL_ID
        .get_or_init(|| {
            let t = target::active();
            if asm_intern::get() {
                return Some(format!("intern2:{}", t.as_flags().join(" ")));
            }
            let out = Command::new(t.assembler()).arg("--version").output().ok()?;
            if !out.status.success() {
                return None;
            }
            let version = String::from_utf8_lossy(&out.stdout);
            let first = version.lines().next().unwrap_or("");
            Some(format!("{}|{}|{}", t.assembler(), first, t.as_flags().join(" ")))
        })
        .clone()
}

fn id() -> Option<String> {
    Some(buildcache::object_id(&tool_id()?))
}

pub fn load(slot: &Slot) -> Option<Vec<u8>> {
    buildcache::lookup_object(slot, &id()?)
}

pub fn save(slot: &Slot, obj_path: &Path) -> bool {
    let id = match id() {
        Some(i) => i,
        None => return false,
    };
    match std::fs::read(obj_path) {
        Ok(bytes) => buildcache::store_object(slot, &id, &bytes),
        Err(_) => false,
    }
}
