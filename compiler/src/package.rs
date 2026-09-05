// SPDX-License-Identifier: GPL-2.0-only
//! Project manifest `firn.package` — name, version, entry point,
//! source directories, public modules, dependencies.
//!
//! WHY NO TOML (round 48, decision with its reasoning)
//! ---------------------------------------------------------------------
//! TOML is a real specification: escaped and multi-line strings, arrays,
//! embedded tables, date values, number syntax. This compiler has NO
//! foreign libraries, and everything must stand TWICE — as Rust (`firnc0`)
//! and as Firn (`lib/firnc1/package.fi`, without libc, buffers and
//! `syscall` only). A *half* TOML would be the worst solution: it looks
//! like TOML, yet fails to accept valid TOML files or reads them
//! differently. Hence a format of its own, deliberately tiny and line
//! based, with a suffix of its own — nobody expects TOML semantics from
//! that.
//!
//! FORMAT
//! ---------------------------------------------------------------------
//! One statement per line: `key value [value ...]`. Separators are space and
//! tab, `#` opens a comment up to the end of the line, empty lines do not
//! count. There are no quotes and no escapes — a value therefore contains
//! neither spaces nor `#`.
//!
//! ```text
//! package      demo            # required, exactly once
//! version      0.1.0           # required, exactly once, num.num.num
//! start        src/main.fi     # at most once, relative to the manifest;
//!                              #      a library has none
//! source       src             # 0..n, relative; without it the
//!                              #      manifest directory itself counts
//! public       geo point       # 0..n, module interface of the package;
//!                              #      without it everything is public
//! needs        geo ../geo      # 0..n, label + local path
//! ```
//!
//! Unknown keys are ERRORS, no silent skipping: a mistyped `publi` would
//! otherwise open up an interface that nobody ever wanted to
//! open.

/// File name of the manifest. Stands exclusively here.
pub const MANIFEST: &str = "firn.package";

/// How many directory levels the upward search covers at most.
pub const SUCHTIEFE: usize = 64;

/// One dependency: name (becomes the import prefix), local path and the
/// version WISH. `want` empty means: any version will do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    pub path: String,
    /// Round 93: `needs geo ../geo 0.2.0` — empty when the line has no
    /// fourth word. A wish is met by the SAME first number and at least
    /// this version (see `version_at_least`).
    pub want: String,
    pub line: u32,
}

/// The content of a manifest, checked, but still without file system ties.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    /// Entry point. EMPTY means: the package is a library and cannot be built
    /// with `--package`.
    pub start: String,
    /// Source directories, relative to the manifest. Never empty (default: `.`).
    pub sources: Vec<String>,
    /// Public modules. EMPTY means: everything is public — the same rule as
    /// with `export { … }` inside a file.
    pub public: Vec<String>,
    pub dependent: Vec<Dependency>,
}

/// Error while reading a manifest. `line` = 0 means: concerns the file as a
/// whole (a required entry is missing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub line: u32,
    pub msg: String,
}

impl Manifest {
    /// Is `module` visible from outside?
    pub fn is_public(&self, module: &str) -> bool {
        self.public.is_empty() || self.public.iter().any(|m| m == module)
    }
}

// ----------------------------------------------------------- Path arithmetic
//
// PURELY LEXICAL, without a file system: the same arithmetic has to be
// reproducible in Firn, and `--package-info` shall come out character for
// character alike on both compilers. Symbolic links are NOT resolved
// along the way (`firnc1` cannot do that without libc, and it would make
// the output machine dependent).

/// `a/./b/../c` -> `a/c`. A leading slash stays.
pub fn normalize(path: &str) -> String {
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for t in path.split('/') {
        if t.is_empty() || t == "." {
            continue;
        }
        if t == ".." {
            let high = match parts.last() {
                Some(l) => *l != "..",
                None => false,
            };
            if high {
                parts.pop();
            } else if !absolute {
                parts.push("..");
            }
            continue;
        }
        parts.push(t);
    }
    let mut s = String::new();
    if absolute {
        s.push('/');
    }
    s.push_str(&parts.join("/"));
    if s.is_empty() {
        s.push('.');
    }
    s
}

/// `base` + `rel`, normalized. An absolute `rel` wins.
pub fn join(base: &str, rel: &str) -> String {
    if rel.starts_with('/') {
        return normalize(rel);
    }
    if base.is_empty() {
        return normalize(rel);
    }
    normalize(&format!("{}/{}", base, rel))
}

/// Directory part of a path (without the last component).
pub fn dirname(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => path[..i].to_string(),
        None => ".".to_string(),
    }
}

/// Last component without the `.fi` suffix — the module name of a file.
pub fn module_name(path: &str) -> String {
    let last = match path.rfind('/') {
        Some(i) => &path[i + 1..],
        None => path,
    };
    match last.rfind('.') {
        Some(i) if i > 0 => last[..i].to_string(),
        _ => last.to_string(),
    }
}

/// Path from `from` to `to`, purely lexical. Both must be normalized and
/// both absolute (or both relative to the same place). The answer is what
/// goes into the lock file, so it must NOT contain a piece of this machine:
/// `relative("/p/app", "/p/geo")` is `../geo` here and on the second
/// machine, whatever the checkout is called there.
pub fn relative(from: &str, to: &str) -> String {
    let f: Vec<&str> = from.split('/').filter(|x| !x.is_empty() && *x != ".").collect();
    let t: Vec<&str> = to.split('/').filter(|x| !x.is_empty() && *x != ".").collect();
    let mut i = 0;
    while i < f.len() && i < t.len() && f[i] == t[i] {
        i += 1;
    }
    let mut parts: Vec<String> = Vec::new();
    for _ in i..f.len() {
        parts.push("..".to_string());
    }
    for k in i..t.len() {
        parts.push(t[k].to_string());
    }
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

/// Does `path` sit inside `root` (or IS it that)? Both must be normalized.
pub fn read_within(path: &str, root: &str) -> bool {
    if path == root {
        return true;
    }
    if root == "/" {
        return path.starts_with('/');
    }
    path.len() > root.len()
        && path.starts_with(root)
        && path.as_bytes()[root.len()] == b'/'
}

// ------------------------------------------------------------------ Checking

/// Identifier: letter first, then letters, digits, underscore.
/// Package and module names become import prefixes, hence the same rule as
/// for identifiers of the language.
pub fn is_name(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    let c = b[0];
    if !(c.is_ascii_alphabetic() || c == b'_') {
        return false;
    }
    b.iter()
        .all(|&c| c.is_ascii_alphanumeric() || c == b'_')
}

/// `num.num.num`, every place at least one digit.
pub fn is_version(s: &str) -> bool {
    let mut parts = 0;
    for t in s.split('.') {
        parts += 1;
        if t.is_empty() || !t.bytes().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    parts == 3
}

/// The three numbers of a version. Only ever called on a text that
/// `is_version` accepted.
pub fn version_parts(s: &str) -> (u32, u32, u32) {
    let mut n = [0u32; 3];
    for (i, t) in s.split('.').enumerate() {
        if i > 2 {
            break;
        }
        let mut v: u32 = 0;
        for c in t.bytes() {
            // A version out of a manifest is short; a text that would
            // overflow here is nonsense and gets pinned instead of wrapping.
            v = v.saturating_mul(10).saturating_add((c - b'0') as u32);
        }
        n[i] = v;
    }
    (n[0], n[1], n[2])
}

/// Is `have` at least `want`, WITH THE SAME first number?
///
/// ONE rule, and cargo's special case for `0.x` is deliberately NOT copied:
/// there, `0.2.0` means "< 0.3.0" and `1.2.0` means "< 2.0.0", which is two
/// rules where one will do. Here the first number is the compatibility
/// promise, always — `needs geo ../geo 0.2.0` is met by 0.2.0 and by
/// 0.9.1, and never by 0.1.9 or 1.0.0. Local path dependencies have no
/// registry to negotiate with; a rule that fits on one line is worth more
/// than one that matches somebody else's tool.
pub fn version_at_least(have: &str, want: &str) -> bool {
    let (h0, h1, h2) = version_parts(have);
    let (w0, w1, w2) = version_parts(want);
    if h0 != w0 {
        return false;
    }
    if h1 != w1 {
        return h1 > w1;
    }
    h2 >= w2
}

/// Is `a` a higher version than `b`? The order in which the resolution
/// picks a winner among several directories with the same package name.
pub fn version_higher(a: &str, b: &str) -> bool {
    let (a0, a1, a2) = version_parts(a);
    let (b0, b1, b2) = version_parts(b);
    if a0 != b0 {
        return a0 > b0;
    }
    if a1 != b1 {
        return a1 > b1;
    }
    a2 > b2
}

/// Path INSIDE the package: relative, without `..`, not empty.
pub fn is_inner_path(s: &str) -> bool {
    if s.is_empty() || s.starts_with('/') {
        return false;
    }
    !s.split('/').any(|t| t == "..")
}

/// Path of a dependency: may lead outside, but must not be empty.
pub fn is_outer_path(s: &str) -> bool {
    !s.is_empty()
}

// ------------------------------------------------------------------ Reading

fn words(line: &str) -> Vec<&str> {
    line
        .split(|c| c == ' ' || c == '\t')
        .filter(|w| !w.is_empty())
        .collect()
}

/// Reads a manifest from the text. A pure function: no file system, so that
/// the rules stay checkable one by one.
pub fn read(text: &str) -> Result<Manifest, Error> {
    let mut m = Manifest::default();
    let mut has_name = false;
    let mut has_version = false;
    let mut has_start = false;
    let mut nr = 0u32;
    for raw in text.split('\n') {
        nr += 1;
        let without_cr = raw.strip_suffix('\r').unwrap_or(raw);
        let line = match without_cr.find('#') {
            Some(i) => &without_cr[..i],
            None => without_cr,
        };
        let w = words(line);
        if w.is_empty() {
            continue;
        }
        let err = |msg: &str| Error { line: nr, msg: msg.to_string() };
        match w[0] {
            "package" => {
                if w.len() != 2 {
                    return Err(err("'package' expects exactly one name"));
                }
                if has_name {
                    return Err(err("'package' appears more than once in the manifest"));
                }
                if !is_name(w[1]) {
                    return Err(err(&format!(
                        "invalid name '{}' (letter first, then letters, digits, underscore)",
                        w[1]
                    )));
                }
                m.name = w[1].to_string();
                has_name = true;
            }
            "version" => {
                if w.len() != 2 {
                    return Err(err("'version' expects exactly one version number"));
                }
                if has_version {
                    return Err(err("'version' appears more than once in the manifest"));
                }
                if !is_version(w[1]) {
                    return Err(err(&format!(
                        "invalid version '{}' (expected number.number.number)",
                        w[1]
                    )));
                }
                m.version = w[1].to_string();
                has_version = true;
            }
            "start" => {
                if w.len() != 2 {
                    return Err(err("'start' expects exactly one path"));
                }
                if has_start {
                    return Err(err("'start' appears more than once in the manifest"));
                }
                if !is_inner_path(w[1]) {
                    return Err(err(&format!(
                        "invalid path '{}' (relative, without '..')",
                        w[1]
                    )));
                }
                m.start = w[1].to_string();
                has_start = true;
            }
            "source" => {
                if w.len() != 2 {
                    return Err(err("'source' expects exactly one path"));
                }
                if !is_inner_path(w[1]) {
                    return Err(err(&format!(
                        "invalid path '{}' (relative, without '..')",
                        w[1]
                    )));
                }
                let q = normalize(w[1]);
                if m.sources.iter().any(|x| *x == q) {
                    return Err(err(&format!("source '{}' appears more than once in the manifest", w[1])));
                }
                m.sources.push(q);
            }
            "public" => {
                if w.len() < 2 {
                    return Err(err("'public' expects at least one module name"));
                }
                for x in &w[1..] {
                    if !is_name(x) {
                        return Err(err(&format!(
                            "invalid name '{}' (letter first, then letters, digits, underscore)",
                            x
                        )));
                    }
                    if m.public.iter().any(|y| y == x) {
                        return Err(err(&format!(
                            "module '{}' appears more than once in 'public'",
                            x
                        )));
                    }
                    m.public.push(x.to_string());
                }
            }
            "needs" => {
                if w.len() < 3 {
                    return Err(err("'needs' expects a name and a path"));
                }
                if w.len() > 4 {
                    return Err(err("'needs' expects at most one version behind the path"));
                }
                // ROUND 93: the fourth word is the version wish. It is
                // checked HERE for its shape, and in `package_world` against
                // what the package really offers.
                if w.len() == 4 && !is_version(w[3]) {
                    return Err(err(&format!(
                        "invalid version '{}' (expected number.number.number)",
                        w[3]
                    )));
                }
                if !is_name(w[1]) {
                    return Err(err(&format!(
                        "invalid name '{}' (letter first, then letters, digits, underscore)",
                        w[1]
                    )));
                }
                if !is_outer_path(w[2]) {
                    return Err(err("'needs' expects a name and a path"));
                }
                if m.dependent.iter().any(|a| a.name == w[1]) {
                    return Err(err(&format!(
                        "package '{}' appears more than once as a dependency in the manifest",
                        w[1]
                    )));
                }
                m.dependent.push(Dependency {
                    name: w[1].to_string(),
                    path: w[2].to_string(),
                    want: if w.len() == 4 { w[3].to_string() } else { String::new() },
                    line: nr,
                });
            }
            other => {
                return Err(err(&format!(
                    "unknown key '{}' (allowed: package, version, start, source, public, needs)",
                    other
                )));
            }
        }
    }
    if !has_name {
        return Err(Error { line: 0, msg: "the manifest needs a line 'package <name>'".to_string() });
    }
    if !has_version {
        return Err(Error { line: 0, msg: "the manifest needs a line 'version <number.number.number>'".to_string() });
    }
    if m.dependent.iter().any(|a| a.name == m.name) {
        let z = m.dependent.iter().find(|a| a.name == m.name).map(|a| a.line).unwrap_or(0);
        return Err(Error {
            line: z,
            msg: format!("dependency '{}' has the same name as the package itself", m.name),
        });
    }
    if m.sources.is_empty() {
        m.sources.push(".".to_string());
    }
    Ok(m)
}

// ------------------------------------------------------------------- Output

/// The report of `--package-info`. Character for character alike on both
/// compilers; all paths are built purely lexically from `root` (no
/// `getcwd`, no symbolic links), so the output does not depend on the machine.
pub fn info_text(m: &Manifest, root: &str) -> String {
    let w = normalize(root);
    let mut s = String::new();
    s.push_str(&format!("package {}\n", m.name));
    s.push_str(&format!("version {}\n", m.version));
    s.push_str(&format!("root {}\n", w));
    if !m.start.is_empty() {
        s.push_str(&format!("start {}\n", join(&w, &m.start)));
    }
    for q in &m.sources {
        s.push_str(&format!("source {}\n", join(&w, q)));
    }
    for o in &m.public {
        s.push_str(&format!("public {}\n", o));
    }
    for a in &m.dependent {
        if a.want.is_empty() {
            s.push_str(&format!("needs {} {}\n", a.name, join(&w, &a.path)));
        } else {
            // A report that hid the version wish would be a lie about the
            // manifest — `firnc1` writes the same line (`world_info`).
            s.push_str(&format!("needs {} {} {}\n", a.name, join(&w, &a.path), a.want));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str) -> Manifest {
        read(text).expect("manifest should be valid")
    }

    #[test]
    fn smallest_valid_manifest() {
        let x = m("package demo\nversion 0.1.0\nstart src/main.fi\n");
        assert_eq!(x.name, "demo");
        assert_eq!(x.version, "0.1.0");
        assert_eq!(x.start, "src/main.fi");
        // Without 'source' the manifest directory itself counts.
        assert_eq!(x.sources, vec![".".to_string()]);
        assert!(x.public.is_empty());
        assert!(x.dependent.is_empty());
        // Empty interface means: everything public (like 'export').
        assert!(x.is_public("irgendwas"));
    }

    #[test]
    fn comments_blank_lines_tabs() {
        let x = m("# head\n\n\tpackage\tdemo\t# name\nversion 1.2.3\nstart a.fi\n   \n");
        assert_eq!(x.name, "demo");
        assert_eq!(x.version, "1.2.3");
    }

    #[test]
    fn sources_public_and_dependencies() {
        let x = m("package app\nversion 0.0.1\nstart src/main.fi\nsource src\nsource extra\n\
                   public a b\npublic c\nneeds geo ../geo\nneeds txt /opt/txt\n");
        assert_eq!(x.sources, vec!["src".to_string(), "extra".to_string()]);
        assert_eq!(x.public, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        assert!(x.is_public("b"));
        assert!(!x.is_public("d"));
        assert_eq!(x.dependent.len(), 2);
        assert_eq!(x.dependent[0].name, "geo");
        assert_eq!(x.dependent[0].path, "../geo");
        assert_eq!(x.dependent[1].name, "txt");
        assert_eq!(x.dependent[1].path, "/opt/txt");
    }

    #[test]
    fn missing_required() {
        assert_eq!(read("version 1.0.0\nstart a.fi\n").unwrap_err().msg,
                   "the manifest needs a line 'package <name>'");
        assert_eq!(read("package a\nstart a.fi\n").unwrap_err().msg,
                   "the manifest needs a line 'version <number.number.number>'");
        // 'start' is NOT required: a library has no entry point.
        assert_eq!(read("package a\nversion 1.0.0\n").unwrap().start, "");
    }

    #[test]
    fn unknown_key_is_in_error() {
        let e = read("package a\nversion 1.0.0\nstart a.fi\npubli b\n").unwrap_err();
        assert_eq!(e.line, 4);
        assert!(e.msg.starts_with("unknown key 'publi'"), "{}", e.msg);
    }

    #[test]
    fn checked_become_name_version_path() {
        assert!(read("package 1a\nversion 1.0.0\nstart a.fi\n").unwrap_err().msg.contains("invalid name '1a'"));
        assert!(read("package a\nversion 1.0\nstart a.fi\n").unwrap_err().msg.contains("invalid version '1.0'"));
        assert!(read("package a\nversion 1.0.0\nstart ../x.fi\n").unwrap_err().msg.contains("invalid path '../x.fi'"));
        assert!(read("package a\nversion 1.0.0\nstart /x.fi\n").unwrap_err().msg.contains("invalid path '/x.fi'"));
        assert!(is_name("a_1"));
        assert!(!is_name(""));
        assert!(!is_name("a-b"));
        assert!(is_version("10.20.30"));
        assert!(!is_version("1.2.3.4"));
        assert!(!is_version("1.x.3"));
    }

    #[test]
    fn duplicate_entries_become_reported() {
        assert!(read("package a\npackage b\nversion 1.0.0\nstart a.fi\n").unwrap_err().msg.contains("'package' appears more than once"));
        assert!(read("package a\nversion 1.0.0\nversion 1.0.1\nstart a.fi\n").unwrap_err().msg.contains("'version' appears more than once"));
        assert!(read("package a\nversion 1.0.0\nstart a.fi\nsource s\nsource s\n").unwrap_err().msg.contains("source 's' appears more than once"));
        assert!(read("package a\nversion 1.0.0\nstart a.fi\npublic m m\n").unwrap_err().msg.contains("module 'm' appears more than once"));
        assert!(read("package a\nversion 1.0.0\nstart a.fi\nneeds g ../g\nneeds g ../h\n").unwrap_err().msg.contains("appears more than once as a dependency"));
        assert!(read("package a\nversion 1.0.0\nstart a.fi\nneeds a ../a\n").unwrap_err().msg.contains("has the same name as the package itself"));
    }

    #[test]
    fn wrong_arity() {
        assert!(read("package a b\nversion 1.0.0\nstart a.fi\n").unwrap_err().msg.contains("'package' expects exactly one name"));
        assert!(read("package a\nversion 1.0.0\nstart a.fi\nneeds g\n").unwrap_err().msg.contains("'needs' expects a name and a path"));
        assert!(read("package a\nversion 1.0.0\nstart a.fi\npublic\n").unwrap_err().msg.contains("'public' expects at least one module name"));
    }

    #[test]
    fn paths_normalize() {
        assert_eq!(normalize("a/./b/../c"), "a/c");
        assert_eq!(normalize("/a/b/../../c"), "/c");
        assert_eq!(normalize("/.."), "/");
        assert_eq!(normalize("../../a"), "../../a");
        assert_eq!(normalize(""), ".");
        assert_eq!(normalize("."), ".");
        assert_eq!(normalize("/"), "/");
        assert_eq!(normalize("a//b/"), "a/b");
        assert_eq!(join("/x/y", "../z"), "/x/z");
        assert_eq!(join("/x/y", "/abs"), "/abs");
        assert_eq!(join("", "a/b"), "a/b");
        assert_eq!(dirname("/a/b/c.fi"), "/a/b");
        assert_eq!(dirname("/c.fi"), "/");
        assert_eq!(dirname("c.fi"), ".");
        assert_eq!(module_name("/a/b/geo.fi"), "geo");
        assert_eq!(module_name("geo"), "geo");
    }

    /// ROUND 93: the path arithmetic of the lock file. `relative` is the
    /// one function whose result ends up IN a file that a second machine
    /// reads — a mistake here would only show up there.
    #[test]
    fn relative_paths_are_purely_lexical() {
        assert_eq!(relative("/p/app", "/p/geo"), "../geo");
        assert_eq!(relative("/p/app", "/p/app"), ".");
        assert_eq!(relative("/p/app", "/p/app/src"), "src");
        assert_eq!(relative("/p/app/src", "/p"), "../..");
        assert_eq!(relative("/p/app", "/q/geo"), "../../q/geo");
        assert_eq!(relative("/p/./app", "/p/app/x"), "x");
        // Different checkouts, same answer — that IS the requirement.
        assert_eq!(
            relative("/home/a/firn/demos/packages/app", "/home/a/firn/demos/packages/geo"),
            relative("/tmp/x/firn/demos/packages/app", "/tmp/x/firn/demos/packages/geo")
        );
    }

    /// ROUND 93: one rule for the version wish, and it is the same one for
    /// `0.x` as for `1.x`.
    #[test]
    fn version_wishes_have_exactly_one_rule() {
        assert_eq!(version_parts("1.20.3"), (1, 20, 3));
        assert!(version_at_least("0.2.0", "0.2.0"));
        assert!(version_at_least("0.2.7", "0.2.0"));
        assert!(version_at_least("0.9.1", "0.2.0"));
        assert!(!version_at_least("0.1.9", "0.2.0"));
        assert!(!version_at_least("1.0.0", "0.2.0"));
        assert!(!version_at_least("0.2.0", "1.0.0"));
        assert!(version_at_least("1.2.3", "1.2.3"));
        assert!(!version_at_least("1.2.2", "1.2.3"));
        assert!(version_higher("0.3.0", "0.2.9"));
        assert!(version_higher("1.0.0", "0.99.99"));
        assert!(!version_higher("0.2.0", "0.2.0"));
        // Two places with more digits than a version ever has: the
        // comparison must not wrap around.
        assert!(version_higher("1.10.0", "1.9.0"));
    }

    /// The fourth word of `needs`, and the two ways to get it wrong.
    #[test]
    fn the_version_wish_of_a_dependency() {
        let x = m("package app\nversion 0.1.0\nstart s.fi\nneeds geo ../geo 0.2.0\nneeds t ../t\n");
        assert_eq!(x.dependent[0].want, "0.2.0");
        assert_eq!(x.dependent[1].want, "");
        assert!(read("package a\nversion 1.0.0\nneeds g ../g 0.2\n")
            .unwrap_err()
            .msg
            .contains("invalid version '0.2'"));
        assert!(read("package a\nversion 1.0.0\nneeds g ../g 0.2.0 x\n")
            .unwrap_err()
            .msg
            .contains("'needs' expects at most one version behind the path"));
        // And the old message for a line that is too short stays what it was.
        assert!(read("package a\nversion 1.0.0\nneeds g\n")
            .unwrap_err()
            .msg
            .contains("'needs' expects a name and a path"));
        assert_eq!(
            info_text(&x, "/p/app"),
            "package app\nversion 0.1.0\nroot /p/app\nstart /p/app/s.fi\nsource /p/app\n\
             needs geo /p/geo 0.2.0\nneeds t /p/t\n"
        );
    }

    #[test]
    fn membership_to_a_package() {
        assert!(read_within("/a/b/c.fi", "/a/b"));
        assert!(read_within("/a/b", "/a/b"));
        assert!(!read_within("/a/bc/d.fi", "/a/b"));
        assert!(!read_within("/a", "/a/b"));
        assert!(read_within("/a", "/"));
    }

    #[test]
    fn infotext_is_pure_lexical() {
        let x = m("package app\nversion 0.2.0\nstart src/main.fi\nsource src\n\
                   public app\nneeds geo ../geo\n");
        assert_eq!(
            info_text(&x, "./example/app/"),
            "package app\nversion 0.2.0\nroot example/app\nstart example/app/src/main.fi\n\
             source example/app/src\npublic app\nneeds geo example/geo\n"
        );
    }
}
