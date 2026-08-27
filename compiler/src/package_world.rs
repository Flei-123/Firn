// SPDX-License-Identifier: GPL-2.0-only
//! The package world: the root manifest, every package reachable through
//! `needs` and the graph between them.
//!
//! `package.rs` knows the format only (pure functions, no file system).
//! Here the file system joins: find the manifest, read it, load dependencies
//! after it, spot cycles.
//!
//! SEARCH FOR THE MANIFEST: from the directory of the root file UPWARDS,
//! until `firn.package` shows up or the file system ends
//! (`package::SUCHTIEFE` as the emergency brake). Without a find the world
//! is EMPTY — the compiler then behaves exactly as before round 48. That is
//! deliberate: everything new hangs off the manifest, nothing changes without.

use crate::package::{self, Manifest};

/// One loaded package.
pub struct Package {
    pub manifest: Manifest,
    /// Directory of the manifest, normalized and absolute.
    pub root: String,
    /// Path of the manifest file, the way it is reported.
    pub manifestpfad: String,
    /// Index into `World::packages` per entry of `manifest.dependent`.
    pub edges: Vec<usize>,
}

/// All packages of this compilation. `packages[0]` is the root package.
pub struct World {
    pub packages: Vec<Package>,
}

fn err(text: String) -> String {
    format!("error: {}\n", text)
}

fn error_with_note(text: String, note: String) -> String {
    format!("error: {}\nnote: {}\n", text, note)
}

/// Working directory, normalized. Everything internal computes absolute, so
/// that "does this file sit inside that package" stays pure string work.
pub fn cwd() -> String {
    match std::env::current_dir() {
        Ok(p) => package::normalize(&p.display().to_string()),
        Err(_) => "/".to_string(),
    }
}

/// Make `path` absolute (relative to `cwd`).
pub fn absolute(path: &str, cwd: &str) -> String {
    if path.starts_with('/') {
        package::normalize(path)
    } else {
        package::join(cwd, path)
    }
}

/// The spelling under which a source file appears IN THIS BUILD
/// (round 93). A path inside the working directory becomes relative to it,
/// anything else stays what it was.
///
/// WHY, measured on `demos/packages/app`: the module search of a package
/// build hands out ABSOLUTE paths for every dependency — they are computed
/// from `cwd` in this very file. Those paths do not stay inside the
/// compiler. They end up
///
///   * in `.file` directives, hence in `.debug_line` of the artifact, and
///   * in the message table of the checked arithmetic (round 72), hence in
///     `.rodata` — that is TEXT THE PROGRAM PRINTS: "panic: integer
///     overflow in 'i32 * i32' at /root/…/firn/demos/packages/geo/src/
///     geo.fi:16:12".
///
/// So the artifact carried the name of the checkout directory twice over,
/// and two machines could not produce the same octets however equal their
/// sources were. Relative to the working directory the name is the same on
/// both machines; a path OUTSIDE the working directory stays visible
/// instead of turning into a chain of `..`.
pub fn build_path(path: &str, cwd: &str) -> String {
    let abs = absolute(path, cwd);
    if package::read_within(&abs, cwd) {
        package::relative(cwd, &abs)
    } else {
        path.to_string()
    }
}

fn is_file(p: &str) -> bool {
    std::path::Path::new(p).is_file()
}

/// Searches `firn.package` from `dirname` upwards. Yields the directory
/// that holds it.
pub fn search_manifest(dirname: &str) -> Option<String> {
    let mut d = package::normalize(dirname);
    for _ in 0..package::SUCHTIEFE {
        if is_file(&package::join(&d, package::MANIFEST)) {
            return Some(d);
        }
        let high = package::join(&d, "..");
        if high == d {
            return None;
        }
        d = high;
    }
    None
}

fn load(root: &str) -> Result<(Manifest, String), String> {
    let mpath = package::join(root, package::MANIFEST);
    let text = match std::fs::read_to_string(&mpath) {
        Ok(t) => t,
        Err(e) => {
            return Err(err(format!("cannot read '{}': {}", mpath, e)));
        }
    };
    match package::read(&text) {
        Ok(m) => Ok((m, mpath)),
        Err(f) => {
            if f.line == 0 {
                Err(err(format!("{}: {}", mpath, f.msg)))
            } else {
                Err(err(format!("{}:{}: {}", mpath, f.line, f.msg)))
            }
        }
    }
}

impl World {
    /// Empty world: no manifest, everything as before round 48.
    pub fn empty() -> World {
        World { packages: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }

    /// World for a root directory (a manifest MUST sit there).
    pub fn ab_root(root: &str) -> Result<World, String> {
        let c = cwd();
        let w = absolute(root, &c);
        if !is_file(&package::join(&w, package::MANIFEST)) {
            return Err(error_with_note(
                format!("no manifest in '{}'", root),
                format!("the file '{}' is expected", package::join(root, package::MANIFEST)),
            ));
        }
        World::build(&w)
    }

    /// World for a source file: search the manifest from its directory
    /// upwards. Without a find, the world stays empty.
    pub fn ab_file(file: &str) -> Result<World, String> {
        let c = cwd();
        let d = package::dirname(&absolute(file, &c));
        match search_manifest(&d) {
            Some(w) => World::build(&w),
            None => Ok(World::empty()),
        }
    }

    /// Builds the world from one absolute, normalized root directory.
    fn build(root: &str) -> Result<World, String> {
        let mut packages: Vec<Package> = Vec::new();
        let (m, mp) = load(root)?;
        packages.push(Package {
            manifest: m,
            root: root.to_string(),
            manifestpfad: mp,
            edges: Vec::new(),
        });
        // BREADTH-FIRST SEARCH over `needs`. A package already loaded is
        // recognized by its root directory — the same place is the same package,
        // even when two manifests spell it differently.
        let mut i = 0usize;
        while i < packages.len() {
            let own_root = packages[i].root.clone();
            let deps = packages[i].manifest.dependent.clone();
            let mpath = packages[i].manifestpfad.clone();
            let mut edges = Vec::new();
            for a in &deps {
                let dw = absolute(&package::join(&own_root, &a.path), &own_root);
                let present = packages.iter().position(|p| p.root == dw);
                let idx = match present {
                    Some(k) => k,
                    None => {
                        if !is_file(&package::join(&dw, package::MANIFEST)) {
                            return Err(error_with_note(
                                format!(
                                    "{}:{}: dependency '{}' has no manifest",
                                    mpath, a.line, a.name
                                ),
                                format!("'{}' is expected", package::join(&dw, package::MANIFEST)),
                            ));
                        }
                        let (dm, dmp) = load(&dw)?;
                        if dm.name != a.name {
                            return Err(error_with_note(
                                format!(
                                    "{}:{}: dependency '{}' points to package '{}'",
                                    mpath, a.line, a.name, dm.name
                                ),
                                format!("'{}' names itself '{}'", dmp, dm.name),
                            ));
                        }
                        packages.push(Package {
                            manifest: dm,
                            root: dw,
                            manifestpfad: dmp,
                            edges: Vec::new(),
                        });
                        packages.len() - 1
                    }
                };
                edges.push(idx);
            }
            packages[i].edges = edges;
            i += 1;
        }
        let mut world = World { packages };
        // ROUND 93, IN THIS ORDER: first pick one version per package name
        // and bend the edges onto it, only then look for cycles. The other
        // way round the check would run on edges that the resolution is
        // about to change.
        world.resolve_versions()?;
        world.check_cycles()?;
        Ok(world)
    }

    /// ONE VERSION PER PACKAGE NAME (round 93).
    ///
    /// The breadth first search above keys a package by its ROOT DIRECTORY:
    /// two directories that call themselves `geo` are two packages to it.
    /// They must not be — the module system renames a module of a
    /// non-root file to `module__name`, so two `geo` in one build collide
    /// (round 48 turned that into an error). And `import geo` in two
    /// different packages has to mean the same thing anyway.
    ///
    /// So: per name the HIGHEST version wins, every edge is bent onto the
    /// winner, and afterwards every version wish is measured against it.
    /// Deterministic, without a network and without a solver — with local
    /// path dependencies there is nothing to search, only something to
    /// decide.
    fn resolve_versions(&mut self) -> Result<(), String> {
        // The winner per name. Names are short and few; a scan beats a map
        // here and keeps the order of the answer independent of any hash.
        let n = self.packages.len();
        let mut winner: Vec<usize> = Vec::new();
        for i in 0..n {
            let name = self.packages[i].manifest.name.clone();
            let mut place: Option<usize> = None;
            for &k in winner.iter() {
                if self.packages[k].manifest.name == name {
                    place = Some(k);
                    break;
                }
            }
            match place {
                None => winner.push(i),
                Some(k) => {
                    let old = self.packages[k].manifest.version.clone();
                    let new = self.packages[i].manifest.version.clone();
                    if package::version_higher(&new, &old) {
                        // Replace the loser in the list of winners.
                        for w in winner.iter_mut() {
                            if *w == k {
                                *w = i;
                            }
                        }
                    } else if !package::version_higher(&old, &new) {
                        // Same name, same version, two directories. There is
                        // no reason to prefer one, so nothing gets guessed.
                        let (a, b) = if self.packages[k].root <= self.packages[i].root {
                            (k, i)
                        } else {
                            (i, k)
                        };
                        return Err(text_two_directories(
                            &name,
                            &new,
                            &self.packages[a].root,
                            &self.packages[b].root,
                        ));
                    }
                }
            }
        }
        // Bend every edge onto the winner of its name.
        for i in 0..n {
            let mut edges = self.packages[i].edges.clone();
            for e in edges.iter_mut() {
                let name = self.packages[*e].manifest.name.clone();
                for &k in winner.iter() {
                    if self.packages[k].manifest.name == name {
                        *e = k;
                        break;
                    }
                }
            }
            self.packages[i].edges = edges;
        }
        // And now the wishes, against what really got picked.
        for i in 0..n {
            let deps = self.packages[i].manifest.dependent.clone();
            let mpath = self.packages[i].manifestpfad.clone();
            for (k, a) in deps.iter().enumerate() {
                if a.want.is_empty() {
                    continue;
                }
                let t = match self.packages[i].edges.get(k) {
                    Some(t) => *t,
                    None => continue,
                };
                let have = self.packages[t].manifest.version.clone();
                if !package::version_at_least(&have, &a.want) {
                    return Err(text_version_wish(
                        &mpath, a.line, &a.name, &have, &a.want,
                    ));
                }
            }
        }
        Ok(())
    }

    /// Every package that can be REACHED from the root package. After the
    /// resolution a superseded directory can still sit in the world with
    /// nobody pointing at it; it is not part of this build (see `lock.rs`).
    pub fn reachable(&self) -> Vec<usize> {
        let mut seen = vec![false; self.packages.len()];
        let mut stack: Vec<usize> = Vec::new();
        let mut out: Vec<usize> = Vec::new();
        if self.packages.is_empty() {
            return out;
        }
        stack.push(0);
        seen[0] = true;
        while let Some(i) = stack.pop() {
            out.push(i);
            for &k in &self.packages[i].edges {
                if !seen[k] {
                    seen[k] = true;
                    stack.push(k);
                }
            }
        }
        out.sort();
        out
    }

    /// Depth-first search with three colors: 0 = unseen, 1 = on the path,
    /// 2 = done. Once the path meets itself, that is a cycle.
    fn check_cycles(&self) -> Result<(), String> {
        let n = self.packages.len();
        let mut color = vec![0u8; n];
        let mut away: Vec<usize> = Vec::new();
        for s in 0..n {
            if color[s] != 0 {
                continue;
            }
            if let Some(z) = self.dfs(s, &mut color, &mut away) {
                return Err(error_with_note(
                    format!("package cycle: {}", z),
                    "dependencies must form an acyclic graph".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn dfs(&self, i: usize, color: &mut Vec<u8>, away: &mut Vec<usize>) -> Option<String> {
        color[i] = 1;
        away.push(i);
        for &k in &self.packages[i].edges {
            if color[k] == 1 {
                let ab = away.iter().position(|&x| x == k).unwrap_or(0);
                let mut names: Vec<String> = away[ab..]
                    .iter()
                    .map(|&x| self.packages[x].manifest.name.clone())
                    .collect();
                names.push(self.packages[k].manifest.name.clone());
                return Some(names.join(" -> "));
            }
            if color[k] == 0 {
                if let Some(z) = self.dfs(k, color, away) {
                    return Some(z);
                }
            }
        }
        away.pop();
        color[i] = 2;
        None
    }

    /// Which package does this file belong to? The longest matching root wins,
    /// so that a package may sit INSIDE the directory of another.
    pub fn package_of(&self, absolute_path: &str) -> Option<usize> {
        let mut hit: Option<usize> = None;
        for (i, p) in self.packages.iter().enumerate() {
            if package::read_within(absolute_path, &p.root) {
                let better = match hit {
                    None => true,
                    Some(t) => p.root.len() > self.packages[t].root.len(),
                };
                if better {
                    hit = Some(i);
                }
            }
        }
        hit
    }

    /// Index of the given dependency of package `i`.
    pub fn edge(&self, i: usize, name: &str) -> Option<usize> {
        let p = &self.packages[i];
        p.manifest
            .dependent
            .iter()
            .position(|a| a.name == name)
            .and_then(|k| p.edges.get(k).copied())
    }

    pub fn name(&self, i: usize) -> &str {
        &self.packages[i].manifest.name
    }
}

/// Error text "module is not public" — at ONE place, so that `firnc0` and
/// `firnc1` write the very same sentence.
pub fn text_not_public(module: &str, package_name: &str, manifestpfad: &str) -> String {
    error_with_note(
        format!("module '{}' is not public in package '{}'", module, package_name),
        format!("add 'public {}' in '{}'", module, manifestpfad),
    )
}

/// Error text "package is not a dependency".
pub fn text_no_dependency(target: &str, of: &str, manifestpfad: &str) -> String {
    error_with_note(
        format!("package '{}' is not a dependency of package '{}'", target, of),
        format!("add 'needs {} <path>' in '{}'", target, manifestpfad),
    )
}

/// Error text "the version wish is not met" (round 93). The line of the
/// `needs` entry is in it, because that is the place to change.
pub fn text_version_wish(
    manifestpfad: &str,
    line: u32,
    name: &str,
    have: &str,
    want: &str,
) -> String {
    err(format!(
        "{}:{}: dependency '{}' is version {}, needed is {} or higher with the same first number",
        manifestpfad, line, name, have, want
    ))
}

/// Error text "one name, two directories, one version" (round 93).
pub fn text_two_directories(name: &str, version: &str, a: &str, b: &str) -> String {
    error_with_note(
        format!("package '{}' comes from two directories with version {}", name, version),
        format!("'{}' and '{}'", a, b),
    )
}

/// Error text "two files, one module name".
pub fn text_name_clash(module: &str, a: &str, b: &str) -> String {
    error_with_note(
        format!("name conflict: module '{}' comes from two files", module),
        format!("'{}' and '{}'", a, b),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_and_cwd() {
        assert_eq!(absolute("/a/b", "/x"), "/a/b");
        assert_eq!(absolute("a/b", "/x"), "/x/a/b");
        assert_eq!(absolute("../a", "/x/y"), "/x/a");
        assert!(cwd().starts_with('/'));
    }

    #[test]
    fn build_paths_do_not_name_the_machine() {
        assert_eq!(build_path("/w/x/demos/a.fi", "/w/x"), "demos/a.fi");
        assert_eq!(build_path("demos/a.fi", "/w/x"), "demos/a.fi");
        assert_eq!(build_path("/w/x", "/w/x"), ".");
        // Outside the working directory: unchanged, and NOT a chain of '..'.
        assert_eq!(build_path("/usr/lib/firn/std.fi", "/w/x"), "/usr/lib/firn/std.fi");
        // The same source under two checkouts gives the same name.
        assert_eq!(
            build_path("/home/a/firn/demos/packages/geo/src/geo.fi", "/home/a/firn"),
            build_path("/tmp/b/firn/demos/packages/geo/src/geo.fi", "/tmp/b/firn")
        );
    }

    #[test]
    fn the_new_texts_of_round_93_are_fixed() {
        assert_eq!(
            text_version_wish("/p/app/firn.package", 7, "geo", "0.1.0", "0.2.0"),
            "error: /p/app/firn.package:7: dependency 'geo' is version 0.1.0, \
             needed is 0.2.0 or higher with the same first number\n"
        );
        assert_eq!(
            text_two_directories("geo", "0.2.0", "/p/a/geo", "/p/b/geo"),
            "error: package 'geo' comes from two directories with version 0.2.0\n\
             note: '/p/a/geo' and '/p/b/geo'\n"
        );
    }

    #[test]
    fn error_texts_are_fixed() {
        assert_eq!(
            text_not_public("inner", "geo", "/p/geo/firn.package"),
            "error: module 'inner' is not public in package 'geo'\n\
             note: add 'public inner' in '/p/geo/firn.package'\n"
        );
        assert_eq!(
            text_no_dependency("geo", "app", "/p/app/firn.package"),
            "error: package 'geo' is not a dependency of package 'app'\n\
             note: add 'needs geo <path>' in '/p/app/firn.package'\n"
        );
        assert_eq!(
            text_name_clash("util", "/a/util.fi", "/b/util.fi"),
            "error: name conflict: module 'util' comes from two files\n\
             note: '/a/util.fi' and '/b/util.fi'\n"
        );
    }
}
