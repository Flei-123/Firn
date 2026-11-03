use crate::brand;
use std::path::Path;

pub fn apply_env_aliases() {
    if brand::LEGACY_ENV_PREFIX.is_empty() {
        return;
    }
    let old = format!("{}_", brand::LEGACY_ENV_PREFIX);
    let renamed: Vec<(String, String)> = std::env::vars()
        .filter_map(|(k, v)| {
            let rest = k.strip_prefix(old.as_str())?;
            Some((format!("{}_{}", brand::ENV_PREFIX, rest), v))
        })
        .collect();
    for (k, v) in renamed {
        if std::env::var_os(&k).is_none() {
            std::env::set_var(&k, v);
        }
    }
    let lib_old = format!("{}LIB", brand::LEGACY_ENV_PREFIX);
    if let Some(v) = std::env::var_os(&lib_old) {
        if std::env::var_os(brand::SYSROOT_ENV).is_none() {
            std::env::set_var(brand::SYSROOT_ENV, v);
        }
    }
}

pub fn existing_name(dir: &str, current: &str, legacy: &str) -> String {
    if legacy.is_empty() {
        return current.to_string();
    }
    let has = |n: &str| Path::new(dir).join(n).is_file();
    if !has(current) && has(legacy) {
        legacy.to_string()
    } else {
        current.to_string()
    }
}
