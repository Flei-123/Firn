//! **Round ANDROID — everything about the third target that is not code.**
//!
//! `codegen_a64.rs` does not appear in this file and this file does not
//! appear in `codegen_a64.rs`. That separation is the whole claim of the
//! round: an Android build differs from a `aarch64-linux-gnu` build in
//! *nothing that a code generator decides* and in *everything that a linker
//! decides*.
//!
//! What a linker decides here:
//!
//! | question | `aarch64-linux-gnu` | `aarch64-linux-android` |
//! |---|---|---|
//! | C library | glibc (or none at all — Firn calls the kernel itself) | **Bionic** (`libc.so` from the NDK) |
//! | start files | none | `crtbegin_dynamic.o` + `crtend_android.o` |
//! | loader | `/lib/ld-linux-aarch64.so.1` | **`/system/bin/linker64`** |
//! | position independent | optional | **required** — Android has refused a non-PIE executable since Android 5 (2014) |
//! | form an app loads | executable | **shared library** (`.so`) |
//! | API level | — | **has to be chosen**; Bionic's set of functions grew with the releases |
//! | `.note.android.ident` | absent | comes in with the start file |
//!
//! ### The API level
//!
//! `libc.so` in the NDK is not a library, it is a *stub*: a shared object
//! that contains no code, only the symbol table Android of that release
//! offers. There is one per API level, and linking against level 24 is the
//! promise "this artifact does not use anything Android 7.0 did not have".
//! The default here is **24** (Android 7.0, 2016) because it is the oldest
//! level in current NDKs that every Play Store device satisfies; `26` is
//! present as well and is what an app targeting Android 8 would take.
//!
//! ### Where the NDK is looked for
//!
//! In this order, first hit wins:
//!
//!   1. `FIRN_ANDROID_NDK`
//!   2. `ANDROID_NDK_HOME`
//!   3. `ANDROID_NDK_ROOT`
//!   4. `<sdk>/ndk/<highest version>` for every `<sdk>` in
//!      `ANDROID_SDK_ROOT`, `ANDROID_HOME`, `$HOME/android-sdk`
//!   5. `<sdk>/ndk-partial/<highest version>` — the same, for a sysroot
//!      that was unpacked out of the NDK zip without the 2.6 GB of
//!      toolchain that this compiler does not use (it brings its own code
//!      generator and links with binutils).
//!
//! A directory counts as an NDK when
//! `toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android`
//! is under it. Nothing is guessed: if it is not there, the error says
//! where it looked.

use std::cell::Cell;
use std::path::{Path, PathBuf};

/// Android 7.0. See the module documentation for why.
pub const DEFAULT_API: u32 = 24;

/// The oldest and the newest level a name may carry. Outside this the
/// number is a typo, not a wish.
const API_MIN: u32 = 16;
const API_MAX: u32 = 99;

/// Where the aarch64 part of an NDK sysroot sits, relative to its root.
const SYSROOT_LIB: &str = "toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android";

/// Android's dynamic loader. 64 bit; the 32 bit one is `/system/bin/linker`.
pub const LINKER64: &str = "/system/bin/linker64";

thread_local! {
    static API: Cell<u32> = const { Cell::new(DEFAULT_API) };
}

/// `--android-api=<n>`.
pub fn api_set(text: &str) -> Result<(), String> {
    let n: u32 = text
        .parse()
        .map_err(|_| format!("'--android-api={}' is no number", text))?;
    if !(API_MIN..=API_MAX).contains(&n) {
        return Err(format!(
            "android api level {} is out of range ({}..{})",
            n, API_MIN, API_MAX
        ));
    }
    API.with(|a| a.set(n));
    Ok(())
}

/// The API level of this compilation.
pub fn api() -> u32 {
    API.with(|a| a.get())
}

#[cfg(test)]
pub fn api_reset() {
    API.with(|a| a.set(DEFAULT_API));
}

/// A usable NDK: its root, the directory of the aarch64 stub libraries for
/// the chosen API level, and that level.
#[derive(Clone, Debug)]
pub struct Ndk {
    pub root: PathBuf,
    /// `<root>/…/aarch64-linux-android/<api>` — the crt objects and the
    /// stub libraries of exactly this level.
    pub lib_dir: PathBuf,
    pub api: u32,
}

/// Every place that is asked, in order, as text — the error message prints
/// it, so that "not found" says what to do about it.
fn candidates() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for v in ["FIRN_ANDROID_NDK", "ANDROID_NDK_HOME", "ANDROID_NDK_ROOT"] {
        if let Ok(p) = std::env::var(v) {
            if !p.is_empty() {
                out.push(PathBuf::from(p));
            }
        }
    }
    let mut sdks: Vec<PathBuf> = Vec::new();
    for v in ["ANDROID_SDK_ROOT", "ANDROID_HOME"] {
        if let Ok(p) = std::env::var(v) {
            if !p.is_empty() {
                sdks.push(PathBuf::from(p));
            }
        }
    }
    if let Ok(h) = std::env::var("HOME") {
        sdks.push(PathBuf::from(h).join("android-sdk"));
    }
    for sdk in sdks {
        for sub in ["ndk", "ndk-partial", "ndk-bundle"] {
            let d = sdk.join(sub);
            if is_ndk(&d) {
                out.push(d.clone());
                continue;
            }
            // `ndk/<version>` — newest first, so a machine with several
            // installed NDKs takes the newest without being told.
            if let Ok(rd) = std::fs::read_dir(&d) {
                let mut vs: Vec<PathBuf> = rd
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect();
                vs.sort();
                vs.reverse();
                out.extend(vs);
            }
        }
    }
    out
}

fn is_ndk(root: &Path) -> bool {
    root.join(SYSROOT_LIB).is_dir()
}

/// Which API levels a given NDK offers — used by the error message, so a
/// wrong `--android-api=` is answered with the list instead of a shrug.
fn levels(root: &Path) -> Vec<u32> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root.join(SYSROOT_LIB)) {
        for e in rd.flatten() {
            if let Some(n) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) {
                if e.path().join("crtbegin_so.o").is_file() {
                    out.push(n);
                }
            }
        }
    }
    out.sort_unstable();
    out
}

/// The NDK for this compilation, or a message that says what is missing.
pub fn find() -> Result<Ndk, String> {
    let want = api();
    // `FIRN_ANDROID_NDK` is an ORDER, not a hint. Whoever writes it down
    // meant that NDK, and quietly taking a different one because that one
    // turned out not to be there is how a build ends up linking against
    // something nobody chose. A wrong path stops the compilation here.
    if let Ok(p) = std::env::var("FIRN_ANDROID_NDK") {
        if !p.is_empty() && !is_ndk(Path::new(&p)) {
            return Err(format!(
                "FIRN_ANDROID_NDK points at '{}', and that is no NDK\n\
                 note: an NDK is a directory that contains\n       '{}'\n\
                 note: unset FIRN_ANDROID_NDK to search the usual places",
                p, SYSROOT_LIB
            ));
        }
    }
    let tried = candidates();
    let mut found_ndk: Option<PathBuf> = None;
    for c in &tried {
        if !is_ndk(c) {
            continue;
        }
        if found_ndk.is_none() {
            found_ndk = Some(c.clone());
        }
        let lib = c.join(SYSROOT_LIB).join(want.to_string());
        if lib.join("crtbegin_so.o").is_file() {
            return Ok(Ndk {
                root: c.clone(),
                lib_dir: lib,
                api: want,
            });
        }
    }
    // An NDK was there, only not that level: the more precise error.
    if let Some(root) = found_ndk {
        let have = levels(&root);
        let list = if have.is_empty() {
            "none".to_string()
        } else {
            have.iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        return Err(format!(
            "android api level {} is not in the NDK at '{}'\n\
             note: available levels: {}\n\
             note: choose one with --android-api=<n>",
            want,
            root.display(),
            list
        ));
    }
    let list = if tried.is_empty() {
        "  (no place to look — neither FIRN_ANDROID_NDK nor ANDROID_NDK_HOME,\n   \
         ANDROID_NDK_ROOT, ANDROID_SDK_ROOT, ANDROID_HOME or $HOME is set)"
            .to_string()
    } else {
        tried
            .iter()
            .map(|p| format!("  {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Err(format!(
        "no Android NDK found for --target=aarch64-linux-android\n\
         note: an NDK is a directory that contains\n       '{}'\n\
         note: set FIRN_ANDROID_NDK=<path to the NDK>\n\
         note: looked at:\n{}",
        SYSROOT_LIB, list
    ))
}

/// The name a shared library announces itself under (`DT_SONAME`). Taken
/// from the output file, because that is the name the app packages.
fn soname(out: &Path) -> String {
    out.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("libfirn.so")
        .to_string()
}

/// The complete `ld` command line for the Android target — without the
/// linker itself, which `target::linker()` names.
///
/// The order of the members is not decoration: a start file has to stand
/// BEFORE the object, an end file AFTER it and the libraries between them,
/// because GNU ld resolves in one pass from left to right.
pub fn link_args(ndk: &Ndk, obj: &Path, out: &Path, shared: bool) -> Vec<String> {
    let lib = &ndk.lib_dir;
    let mut a: Vec<String> = Vec::new();
    a.push("-o".into());
    a.push(out.display().to_string());
    if shared {
        // A shared library has no entry point and no loader of its own —
        // the app's process already has both.
        a.push("-shared".into());
        a.push("-soname".into());
        a.push(soname(out));
    } else {
        // PIE. Not a hardening option here: Android's loader refuses
        // ET_EXEC outright since Android 5.
        a.push("-pie".into());
        a.push("--dynamic-linker".into());
        a.push(LINKER64.into());
    }
    // Bionic is built with full RELRO and immediate binding, and its
    // loader expects the same of what it loads.
    a.push("-z".into());
    a.push("relro".into());
    a.push("-z".into());
    a.push("now".into());
    // Android pages are 4 KiB (and 16 KiB on newer devices); GNU ld would
    // otherwise align aarch64 segments to 64 KiB and pad the file.
    a.push("-z".into());
    a.push("max-page-size=4096".into());
    a.push("--eh-frame-hdr".into());
    // Nothing may stay open. On the gnu target an undefined symbol in a
    // `.so` is a run time surprise; here it is a linker error.
    a.push("--no-undefined".into());
    a.push("-L".into());
    a.push(lib.display().to_string());
    a.push(
        lib.join(if shared {
            "crtbegin_so.o"
        } else {
            "crtbegin_dynamic.o"
        })
        .display()
        .to_string(),
    );
    a.push(obj.display().to_string());
    // Bionic keeps the C library, the maths library and the loader
    // interface in three files; `liblog` is what every Android artifact is
    // allowed to assume.
    for l in ["-lc", "-lm", "-ldl", "-llog"] {
        a.push(l.into());
    }
    a.push(
        lib.join(if shared {
            "crtend_so.o"
        } else {
            "crtend_android.o"
        })
        .display()
        .to_string(),
    );
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_level_is_twentyfour() {
        api_reset();
        assert_eq!(api(), DEFAULT_API);
        assert_eq!(api(), 24);
    }

    #[test]
    fn a_level_that_is_no_number_is_refused() {
        api_reset();
        let e = api_set("später").unwrap_err();
        assert!(e.contains("is no number"), "{}", e);
        assert_eq!(api(), DEFAULT_API, "the wrong value must not stick");
    }

    #[test]
    fn a_level_out_of_range_is_refused() {
        api_reset();
        assert!(api_set("3").is_err());
        assert!(api_set("400").is_err());
        api_set("26").unwrap();
        assert_eq!(api(), 26);
        api_reset();
    }

    /// The two forms differ in the start file, the end file and the entry
    /// point — and in nothing else.
    #[test]
    fn the_two_forms_take_the_start_files_that_belong_to_them() {
        let ndk = Ndk {
            root: PathBuf::from("/ndk"),
            lib_dir: PathBuf::from("/ndk/lib/24"),
            api: 24,
        };
        let exe = link_args(&ndk, Path::new("a.o"), Path::new("a"), false).join(" ");
        assert!(exe.contains("crtbegin_dynamic.o"), "{}", exe);
        assert!(exe.contains("crtend_android.o"), "{}", exe);
        assert!(exe.contains("-pie"), "{}", exe);
        assert!(exe.contains(LINKER64), "{}", exe);
        assert!(!exe.contains("-shared"), "{}", exe);

        let so = link_args(&ndk, Path::new("a.o"), Path::new("libx.so"), true).join(" ");
        assert!(so.contains("crtbegin_so.o"), "{}", so);
        assert!(so.contains("crtend_so.o"), "{}", so);
        assert!(so.contains("-shared"), "{}", so);
        assert!(so.contains("-soname libx.so"), "{}", so);
        assert!(!so.contains("-pie"), "{}", so);
        assert!(!so.contains(LINKER64), "{}", so);
    }

    /// The start file stands before the object, the end file after it.
    #[test]
    fn the_order_of_the_start_files_is_the_one_ld_needs() {
        let ndk = Ndk {
            root: PathBuf::from("/ndk"),
            lib_dir: PathBuf::from("/ndk/lib/24"),
            api: 24,
        };
        let a = link_args(&ndk, Path::new("a.o"), Path::new("libx.so"), true);
        let begin = a.iter().position(|s| s.contains("crtbegin_so.o")).unwrap();
        let obj = a.iter().position(|s| s == "a.o").unwrap();
        let libc = a.iter().position(|s| s == "-lc").unwrap();
        let end = a.iter().position(|s| s.contains("crtend_so.o")).unwrap();
        assert!(begin < obj && obj < libc && libc < end);
    }

    #[test]
    fn bionic_and_not_glibc() {
        let ndk = Ndk {
            root: PathBuf::from("/ndk"),
            lib_dir: PathBuf::from("/x/aarch64-linux-android/24"),
            api: 24,
        };
        let a = link_args(&ndk, Path::new("a.o"), Path::new("a"), false).join(" ");
        assert!(a.contains("aarch64-linux-android/24"), "{}", a);
        assert!(!a.contains("ld-linux"), "{}", a);
    }

    /// A missing NDK produces a message that says what to do, not a broken
    /// artifact.
    #[test]
    fn a_missing_ndk_says_where_it_looked() {
        let e = {
            // No environment variable may point anywhere real for this.
            let keep: Vec<(String, Option<String>)> = [
                "FIRN_ANDROID_NDK",
                "ANDROID_NDK_HOME",
                "ANDROID_NDK_ROOT",
                "ANDROID_SDK_ROOT",
                "ANDROID_HOME",
                "HOME",
            ]
            .iter()
            .map(|k| (k.to_string(), std::env::var(k).ok()))
            .collect();
            for (k, _) in &keep {
                std::env::remove_var(k);
            }
            std::env::set_var("FIRN_ANDROID_NDK", "/definitely/not/an/ndk");
            let r = find();
            for (k, v) in &keep {
                match v {
                    Some(v) => std::env::set_var(k, v),
                    None => std::env::remove_var(k),
                }
            }
            r.unwrap_err()
        };
        assert!(e.contains("is no NDK"), "{}", e);
        assert!(e.contains("FIRN_ANDROID_NDK"), "{}", e);
        assert!(e.contains("/definitely/not/an/ndk"), "{}", e);
    }

    /// ...and with nothing pointed at anywhere, the message lists the
    /// places it looked instead of shrugging.
    #[test]
    fn nothing_anywhere_says_where_it_looked() {
        let keep: Vec<(String, Option<String>)> = [
            "FIRN_ANDROID_NDK",
            "ANDROID_NDK_HOME",
            "ANDROID_NDK_ROOT",
            "ANDROID_SDK_ROOT",
            "ANDROID_HOME",
            "HOME",
        ]
        .iter()
        .map(|k| (k.to_string(), std::env::var(k).ok()))
        .collect();
        for (k, _) in &keep {
            std::env::remove_var(k);
        }
        std::env::set_var("ANDROID_NDK_HOME", "/also/not/an/ndk");
        let r = find();
        for (k, v) in &keep {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        let e = r.unwrap_err();
        assert!(e.contains("no Android NDK found"), "{}", e);
        assert!(e.contains("/also/not/an/ndk"), "{}", e);
    }
}
