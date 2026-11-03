use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;

const REQUIRED: [&str; 9] = [
    "lang_name",
    "cli",
    "compiler",
    "ext",
    "env_prefix",
    "symbol_prefix",
    "package_file",
    "lock_file",
    "toolchain_file",
];

const LEGACY: [&str; 4] = ["env_prefix", "package_file", "lock_file", "cli"];

fn parse(text: &str) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let mut main = BTreeMap::new();
    let mut legacy = BTreeMap::new();
    let mut in_legacy = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_legacy = line == "[legacy]";
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            panic!("brand.toml: bad line '{}'", line);
        };
        let v = v.trim().trim_matches('"').to_string();
        if v.contains('"') || v.contains('\\') {
            panic!("brand.toml: unsupported characters in '{}'", line);
        }
        let target = if in_legacy { &mut legacy } else { &mut main };
        target.insert(k.trim().to_string(), v);
    }
    (main, legacy)
}

fn emit(out: &mut String, name: &str, value: &str) {
    out.push_str(&format!("pub const {}: &str = \"{}\";\n", name, value));
}

fn main() {
    let dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let path = Path::new(&dir).join("..").join("brand.toml");
    println!("cargo:rerun-if-changed={}", path.display());
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e));
    let (main_values, legacy) = parse(&text);
    for key in REQUIRED {
        match main_values.get(key) {
            Some(v) if !v.is_empty() => {}
            _ => panic!("brand.toml: missing or empty key '{}'", key),
        }
    }
    let get = |k: &str| main_values[k].clone();
    let env_prefix = get("env_prefix");
    let mut out = String::new();
    emit(&mut out, "LANG_NAME", &get("lang_name"));
    emit(&mut out, "CLI", &get("cli"));
    emit(&mut out, "COMPILER", &get("compiler"));
    emit(&mut out, "EXT", &get("ext"));
    emit(&mut out, "ENV_PREFIX", &env_prefix);
    emit(&mut out, "SYMBOL_PREFIX", &get("symbol_prefix"));
    emit(&mut out, "PACKAGE_FILE", &get("package_file"));
    emit(&mut out, "LOCK_FILE", &get("lock_file"));
    emit(&mut out, "TOOLCHAIN_FILE", &get("toolchain_file"));
    emit(&mut out, "ENV_ALLOW", &format!("{}_", env_prefix));
    emit(&mut out, "SYSROOT_ENV", &format!("{}LIB", env_prefix));
    emit(&mut out, "USE_AS_ENV", &format!("{}_USE_AS", env_prefix));
    emit(&mut out, "DEFAULT_DLL", &format!("{}.dll", get("cli")));
    emit(&mut out, "DEFAULT_SO", &format!("lib{}.so", get("cli")));
    emit(&mut out, "C_V128_TYPE", &format!("{}_v128", get("cli")));
    for key in LEGACY {
        let v = legacy.get(key).cloned().unwrap_or_default();
        emit(&mut out, &format!("LEGACY_{}", key.to_uppercase()), &v);
    }
    let dest = Path::new(&env::var("OUT_DIR").unwrap()).join("brand.rs");
    fs::write(dest, out).unwrap();
}
