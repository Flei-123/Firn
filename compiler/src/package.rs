//! Project manifest `firn.pkg` — name, version, entry point,
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
//! tab, a `#` AT THE START OF A WORD opens a comment up to the end of the
//! line, empty lines do not count. There are no quotes and no escapes — a
//! value therefore contains no space, and it may not begin with `#`, but
//! it may carry one inside it (round FIRNHUB: `git+<url>#<ref>`).
//!
//! ```text
//! package      demo            # required, exactly once
//! version      0.1.0           # required, exactly once, num.num.num
//! main         src/main.fi     # at most once, relative to the manifest;
//!                              #      a library has none
//! source       src             # 0..n, relative; without it the
//!                              #      manifest directory itself counts
//! public       geo point       # 0..n, module interface of the package;
//!                              #      without it everything is public
//! needs        geo ../geo      # 0..n, label + source [+ version wish]
//! ```
//!
//! ROUND FIRNHUB — WHERE A DEPENDENCY MAY COME FROM
//! ---------------------------------------------------------------------
//! The source of a `needs` line is ONE word (the format has no quotes, so
//! a value never contains a space) and its kind is decided PURELY
//! LEXICALLY, so that both compilers agree without asking the file system:
//!
//! ```text
//! needs json ../json                                    local path
//! needs json ../json 0.2.0                              …with a version wish
//! needs json git+https://host/firn-json#v1.2.0          git, fixed reference
//! needs json https://host/json-1.2.0.tar#sha256=<64>    archive with checksum
//! needs json 1.2.0                                      registry short form
//! ```
//!
//! * `git+` MUST carry `#<reference>` — a commit or a tag. A branch name
//!   is accepted as text but is a bad idea and the report says so; what is
//!   refused is the FLOATING form without any `#` at all.
//! * `http(s)://` MUST carry `#sha256=<64 lower case hex>`. An archive
//!   without a checksum is not a dependency, it is a wish.
//! * The registry short form is the one that LOOKS LIKE A VERSION
//!   (`number.number.number`). A local directory literally named `1.2.0`
//!   is therefore out of reach — a price of exactly one pathological name,
//!   paid so that `needs json 1.2.0` can mean what everybody reads into it.
//!
//! The COMPILER NEVER SPEAKS TO THE NETWORK. A remote source is resolved
//! through `firn.have` (written by `firnpkg fetch`) into a directory of
//! the content addressed cache; from there on everything is a local path
//! again. See `read_have` below and `package_world::cache_root`.
//!
//! Unknown keys are ERRORS, no silent skipping: a mistyped `publi` would
//! otherwise open up an interface that nobody ever wanted to
//! open.

/// Where a dependency comes from. Decided purely lexically out of the
/// source word of a `needs` line, so both compilers classify alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// A directory on this machine, relative to the manifest (round 48).
    Path,
    /// `git+<url>#<reference>`.
    Git,
    /// `http(s)://<url>#sha256=<64 hex>`.
    Archive,
    /// `<number.number.number>` — resolved through the index.
    Registry,
}

/// The prefix that marks a git source.
pub const GIT: &str = "git+";
/// The fragment that carries the checksum of an archive.
pub const SHA_FRAGMENT: &str = "#sha256=";

/// Classify the source word of a `needs` line.
pub fn origin_of(s: &str) -> Origin {
    if s.starts_with(GIT) {
        return Origin::Git;
    }
    if s.starts_with("https://") || s.starts_with("http://") {
        return Origin::Archive;
    }
    if is_version(s) {
        return Origin::Registry;
    }
    Origin::Path
}

/// Is `s` exactly 64 lower case hex digits? The shape of every checksum in
/// this project — `firn.lock` writes them, `firn.have` carries them and a
/// `needs` line may name one.
pub fn is_hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// `git+<url>#<ref>` -> the url. Only ever called on `Origin::Git`.
pub fn git_url(s: &str) -> &str {
    let rest = &s[GIT.len()..];
    match rest.find('#') {
        Some(i) => &rest[..i],
        None => rest,
    }
}

/// `git+<url>#<ref>` -> the reference.
pub fn git_ref(s: &str) -> &str {
    let rest = &s[GIT.len()..];
    match rest.find('#') {
        Some(i) => &rest[i + 1..],
        None => "",
    }
}

/// `<url>#sha256=<hex>` -> the url.
pub fn archive_url(s: &str) -> &str {
    match s.find('#') {
        Some(i) => &s[..i],
        None => s,
    }
}

/// `<url>#sha256=<hex>` -> the checksum.
pub fn archive_hash(s: &str) -> &str {
    match s.find(SHA_FRAGMENT) {
        Some(i) => &s[i + SHA_FRAGMENT.len()..],
        None => "",
    }
}

/// File name of the manifest. Stands exclusively here.
pub const MANIFEST: &str = "firn.pkg";

/// How many directory levels the upward search covers at most.
pub const SEARCH_DEPTH: usize = 64;

/// One dependency: name (becomes the import prefix), the SOURCE as it
/// stands in the manifest (a local path or one of the remote forms of
/// round FIRNHUB) and the version WISH. `want` empty means: any version
/// will do.
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
    pub main: String,
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

/// Where the comment of this line starts (its length when there is none).
///
/// ROUND FIRNHUB: a `#` opens a comment only where a WORD BEGINS — at the
/// start of the line or behind a separator. Round 48 cut at every `#`, and
/// that made `git+https://host/repo#v1.2.0` impossible: the reference is
/// part of the source, not a remark. The rule stays as short as it was —
/// a comment is a word that starts with `#`; a `#` inside a word belongs
/// to the word.
pub fn comment_at(line: &str) -> usize {
    let b = line.as_bytes();
    for i in 0..b.len() {
        if b[i] == b'#' && (i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t') {
            return i;
        }
    }
    b.len()
}

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
    let mut has_main = false;
    let mut nr = 0u32;
    for raw in text.split('\n') {
        nr += 1;
        let without_cr = raw.strip_suffix('\r').unwrap_or(raw);
        let line = &without_cr[..comment_at(without_cr)];
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
            "main" => {
                if w.len() != 2 {
                    return Err(err("'main' expects exactly one path"));
                }
                if has_main {
                    return Err(err("'main' appears more than once in the manifest"));
                }
                if !is_inner_path(w[1]) {
                    return Err(err(&format!(
                        "invalid path '{}' (relative, without '..')",
                        w[1]
                    )));
                }
                m.main = w[1].to_string();
                has_main = true;
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
                    return Err(err("'needs' expects a name and a source"));
                }
                if w.len() > 4 {
                    return Err(err("'needs' expects at most one version behind the source"));
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
                // ROUND FIRNHUB: where the dependency comes from. The
                // remote forms are checked HERE, on the text, because a
                // source that cannot be fetched is a mistake in the
                // manifest and not a failure of the network.
                match origin_of(w[2]) {
                    Origin::Path => {
                        if !is_outer_path(w[2]) {
                            return Err(err("'needs' expects a name and a source"));
                        }
                    }
                    Origin::Git => {
                        if git_url(w[2]).is_empty() {
                            return Err(err(&format!("git source '{}' has no address", w[2])));
                        }
                        if git_ref(w[2]).is_empty() {
                            return Err(err(&format!(
                                "git source '{}' has no fixed reference (expected 'git+<url>#<commit-or-tag>')",
                                w[2]
                            )));
                        }
                    }
                    Origin::Archive => {
                        if !is_hex64(archive_hash(w[2])) {
                            return Err(err(&format!(
                                "archive source '{}' has no checksum (expected '<url>#sha256=<64 hex digits>')",
                                w[2]
                            )));
                        }
                    }
                    Origin::Registry => {}
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
                    "unknown key '{}' (allowed: package, version, main, source, public, needs)",
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

// -------------------------------------------------------------- firn.have
//
// WHAT THIS FILE IS FOR, and why it is not `firn.lock` (round FIRNHUB)
// ---------------------------------------------------------------------
// `firn.lock` is written AFTER a build and says what went in. `firn.have`
// is written BEFORE one, by `firnpkg fetch`, and says which octets a
// remote `needs` line was resolved to. Two producers, two moments, two
// files — a fetcher that wrote into `firn.lock` would have to invent the
// checksums of a build that has not happened yet.
//
// It sits next to the ROOT manifest and covers the WHOLE graph, including
// the remote dependencies of dependencies. That is the same decision the
// lock file makes: one file per project, not one per package, so a
// fetched package stays a pure content tree and does not have to be
// republished when something below it moves.
//
//     have 1
//     need json git+https://host/firn-json#v1.2.0 4f3c…(40) 9c1e…(64)
//     need date https://host/date-1.0.tar#sha256=aa…  aa…(64)  bb…(64)
//
// Fields: name · the source EXACTLY as it stands in the manifest · what
// the source resolved to (a commit, or the checksum of the archive
// octets) · the CONTENT HASH of the unpacked tree. The last one is the
// address in the cache and the only one the compiler needs to find the
// files; the first three are there so that a mismatch between manifest
// and fetched state is an error and not a surprise.

/// Name of the file that records what was fetched. Stands exclusively here.
pub const HAVEFILE: &str = "firn.have";
/// Format number of its first line.
pub const HAVE_FORMAT: u32 = 1;

/// One resolved remote dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub name: String,
    pub source: String,
    pub resolved: String,
    pub content: String,
    pub line: u32,
}

/// Reads `firn.have`. A pure function on the text, like `read`.
pub fn read_have(text: &str) -> Result<Vec<Fetched>, Error> {
    let mut out: Vec<Fetched> = Vec::new();
    let mut head = false;
    let mut nr = 0u32;
    for raw in text.split('\n') {
        nr += 1;
        let without_cr = raw.strip_suffix('\r').unwrap_or(raw);
        let line = match without_cr.find('#') {
            // A `#` inside a source word is part of it; a comment only
            // starts a line here. The manifest allows a trailing comment,
            // this file does not — it is written by a program.
            Some(0) => "",
            _ => without_cr,
        };
        let w = words(line);
        if w.is_empty() {
            continue;
        }
        let err = |msg: &str| Error { line: nr, msg: msg.to_string() };
        if !head {
            if w[0] != "have" || w.len() != 2 || w[1] != HAVE_FORMAT.to_string() {
                return Err(err(&format!(
                    "expected 'have {}' as the first line",
                    HAVE_FORMAT
                )));
            }
            head = true;
            continue;
        }
        if w[0] != "need" {
            return Err(err(&format!("unknown key '{}' (allowed: need)", w[0])));
        }
        if w.len() != 5 {
            return Err(err(
                "'need' expects a name, a source, a resolved reference and a content hash",
            ));
        }
        if !is_hex64(w[4]) {
            return Err(err(&format!("invalid content hash '{}'", w[4])));
        }
        if out.iter().any(|f: &Fetched| f.name == w[1]) {
            return Err(err(&format!("package '{}' appears more than once", w[1])));
        }
        out.push(Fetched {
            name: w[1].to_string(),
            source: w[2].to_string(),
            resolved: w[3].to_string(),
            content: w[4].to_string(),
            line: nr,
        });
    }
    if !head {
        return Err(Error {
            line: 0,
            msg: format!("the file needs a first line 'have {}'", HAVE_FORMAT),
        });
    }
    Ok(out)
}

/// The entry for this name AND this source. Both have to fit: a
/// `firn.have` that answers for `json` but was written for another source
/// is a stale file, and a build must not quietly use it.
pub fn have_find<'a>(have: &'a [Fetched], name: &str, source: &str) -> Option<&'a Fetched> {
    have.iter().find(|f| f.name == name && f.source == source)
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
    if !m.main.is_empty() {
        s.push_str(&format!("main {}\n", join(&w, &m.main)));
    }
    for q in &m.sources {
        s.push_str(&format!("source {}\n", join(&w, q)));
    }
    for o in &m.public {
        s.push_str(&format!("public {}\n", o));
    }
    for a in &m.dependent {
        // A LOCAL path is reported the way this build will read it, i.e.
        // joined onto the package root. A REMOTE source is reported
        // VERBATIM: joining it would produce nonsense
        // (`/p/app/git+https://…`), and the spelling in the manifest is
        // exactly what identifies it later in `firn.have`.
        let place = if origin_of(&a.path) == Origin::Path {
            join(&w, &a.path)
        } else {
            a.path.clone()
        };
        if a.want.is_empty() {
            s.push_str(&format!("needs {} {}\n", a.name, place));
        } else {
            // A report that hid the version wish would be a lie about the
            // manifest — `firnc1` writes the same line (`world_info`).
            s.push_str(&format!("needs {} {} {}\n", a.name, place, a.want));
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
        let x = m("package demo\nversion 0.1.0\nmain src/main.fi\n");
        assert_eq!(x.name, "demo");
        assert_eq!(x.version, "0.1.0");
        assert_eq!(x.main, "src/main.fi");
        // Without 'source' the manifest directory itself counts.
        assert_eq!(x.sources, vec![".".to_string()]);
        assert!(x.public.is_empty());
        assert!(x.dependent.is_empty());
        // Empty interface means: everything public (like 'export').
        assert!(x.is_public("irgendwas"));
    }

    #[test]
    fn comments_blank_lines_tabs() {
        let x = m("# head\n\n\tpackage\tdemo\t# name\nversion 1.2.3\nmain a.fi\n   \n");
        assert_eq!(x.name, "demo");
        assert_eq!(x.version, "1.2.3");
    }

    #[test]
    fn sources_public_and_dependencies() {
        let x = m("package app\nversion 0.0.1\nmain src/main.fi\nsource src\nsource extra\n\
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
        assert_eq!(read("version 1.0.0\nmain a.fi\n").unwrap_err().msg,
                   "the manifest needs a line 'package <name>'");
        assert_eq!(read("package a\nmain a.fi\n").unwrap_err().msg,
                   "the manifest needs a line 'version <number.number.number>'");
        // 'main' is NOT required: a library has no entry point.
        assert_eq!(read("package a\nversion 1.0.0\n").unwrap().main, "");
    }

    #[test]
    fn unknown_key_is_in_error() {
        let e = read("package a\nversion 1.0.0\nmain a.fi\npubli b\n").unwrap_err();
        assert_eq!(e.line, 4);
        assert!(e.msg.starts_with("unknown key 'publi'"), "{}", e.msg);
    }

    #[test]
    fn checked_become_name_version_path() {
        assert!(read("package 1a\nversion 1.0.0\nmain a.fi\n").unwrap_err().msg.contains("invalid name '1a'"));
        assert!(read("package a\nversion 1.0\nmain a.fi\n").unwrap_err().msg.contains("invalid version '1.0'"));
        assert!(read("package a\nversion 1.0.0\nmain ../x.fi\n").unwrap_err().msg.contains("invalid path '../x.fi'"));
        assert!(read("package a\nversion 1.0.0\nmain /x.fi\n").unwrap_err().msg.contains("invalid path '/x.fi'"));
        assert!(is_name("a_1"));
        assert!(!is_name(""));
        assert!(!is_name("a-b"));
        assert!(is_version("10.20.30"));
        assert!(!is_version("1.2.3.4"));
        assert!(!is_version("1.x.3"));
    }

    #[test]
    fn duplicate_entries_become_reported() {
        assert!(read("package a\npackage b\nversion 1.0.0\nmain a.fi\n").unwrap_err().msg.contains("'package' appears more than once"));
        assert!(read("package a\nversion 1.0.0\nversion 1.0.1\nmain a.fi\n").unwrap_err().msg.contains("'version' appears more than once"));
        assert!(read("package a\nversion 1.0.0\nmain a.fi\nsource s\nsource s\n").unwrap_err().msg.contains("source 's' appears more than once"));
        assert!(read("package a\nversion 1.0.0\nmain a.fi\npublic m m\n").unwrap_err().msg.contains("module 'm' appears more than once"));
        assert!(read("package a\nversion 1.0.0\nmain a.fi\nneeds g ../g\nneeds g ../h\n").unwrap_err().msg.contains("appears more than once as a dependency"));
        assert!(read("package a\nversion 1.0.0\nmain a.fi\nneeds a ../a\n").unwrap_err().msg.contains("has the same name as the package itself"));
    }

    #[test]
    fn wrong_arity() {
        assert!(read("package a b\nversion 1.0.0\nmain a.fi\n").unwrap_err().msg.contains("'package' expects exactly one name"));
        assert!(read("package a\nversion 1.0.0\nmain a.fi\nneeds g\n").unwrap_err().msg.contains("'needs' expects a name and a source"));
        assert!(read("package a\nversion 1.0.0\nmain a.fi\npublic\n").unwrap_err().msg.contains("'public' expects at least one module name"));
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
        let x = m("package app\nversion 0.1.0\nmain s.fi\nneeds geo ../geo 0.2.0\nneeds t ../t\n");
        assert_eq!(x.dependent[0].want, "0.2.0");
        assert_eq!(x.dependent[1].want, "");
        assert!(read("package a\nversion 1.0.0\nneeds g ../g 0.2\n")
            .unwrap_err()
            .msg
            .contains("invalid version '0.2'"));
        assert!(read("package a\nversion 1.0.0\nneeds g ../g 0.2.0 x\n")
            .unwrap_err()
            .msg
            .contains("'needs' expects at most one version behind the source"));
        // And the old message for a line that is too short stays what it was.
        assert!(read("package a\nversion 1.0.0\nneeds g\n")
            .unwrap_err()
            .msg
            .contains("'needs' expects a name and a source"));
        assert_eq!(
            info_text(&x, "/p/app"),
            "package app\nversion 0.1.0\nroot /p/app\nmain /p/app/s.fi\nsource /p/app\n\
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
        let x = m("package app\nversion 0.2.0\nmain src/main.fi\nsource src\n\
                   public app\nneeds geo ../geo\n");
        assert_eq!(
            info_text(&x, "./example/app/"),
            "package app\nversion 0.2.0\nroot example/app\nmain example/app/src/main.fi\n\
             source example/app/src\npublic app\nneeds geo example/geo\n"
        );
    }
}
