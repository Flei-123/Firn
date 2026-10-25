use crate::config::{self, AppConfig, Profile};
use crate::error::{LauncherError, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::Window;

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

pub fn emit(_w: &Window, _stage: &str, _a: u64, _b: u64, _msg: &str) {}

pub async fn get_vanilla_version_json(mc: &str) -> Result<Value> {
    let cache = config::versions_dir().join(mc).join(format!("{mc}.json"));
    if cache.exists() {
        return Ok(serde_json::from_str(&std::fs::read_to_string(&cache)?)?);
    }
    let client = crate::modrinth::client();
    let manifest: Value = client.get(MANIFEST).send().await?.json().await?;
    let url = manifest["versions"]
        .as_array()
        .and_then(|vs| vs.iter().find(|v| v["id"] == mc))
        .and_then(|v| v["url"].as_str())
        .ok_or_else(|| LauncherError::NotFound(mc.into()))?
        .to_string();
    let version: Value = client.get(url).send().await?.json().await?;
    std::fs::create_dir_all(cache.parent().unwrap())?;
    std::fs::write(&cache, serde_json::to_string(&version)?)?;
    Ok(version)
}

pub async fn prepare_assets(_w: &Window, _v: &Value) -> Result<String> {
    Ok("stub-index".into())
}

pub async fn prepare_client_jar(mc: &str, _v: &Value) -> Result<PathBuf> {
    let dest = config::versions_dir().join(mc).join(format!("{mc}.jar"));
    std::fs::create_dir_all(dest.parent().unwrap())?;
    std::fs::write(&dest, b"stub")?;
    Ok(dest)
}

pub async fn ensure_java(_w: &Window, _cfg: &AppConfig, _major: u32) -> Result<String> {
    Ok(std::env::var("FLSP_JAVA").unwrap_or_else(|_| "java".into()))
}

pub fn supervise(
    _w: &Window,
    profile: &Profile,
    _cfg: &AppConfig,
    mut child: std::process::Child,
    running: Arc<Mutex<HashMap<String, u32>>>,
) -> u32 {
    let pid = child.id();
    running.lock().unwrap().insert(profile.id.clone(), pid);
    let id = profile.id.clone();
    std::thread::spawn(move || {
        let _ = child.wait();
        running.lock().unwrap().remove(&id);
    });
    pid
}
