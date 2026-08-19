//! Die Paketwelt: das Wurzelmanifest, alle ueber `brauche` erreichbaren
//! Pakete und der Graph dazwischen.
//!
//! `package.rs` kennt nur das Format (reine Funktionen, ohne Dateisystem).
//! Hier kommt das Dateisystem dazu: Manifest suchen, lesen, Abhaengigkeiten
//! nachladen, Zyklen finden.
//!
//! SUCHE NACH DEM MANIFEST: von dem Verzeichnis der Wurzeldatei aus nach
//! OBEN, bis `firn.package` gefunden ist oder das Dateisystem endet
//! (`paket::SUCHTIEFE` als Notbremse). Findet sich keins, ist die Welt LEER —
//! und dann verhaelt sich der Uebersetzer exakt wie vor Runde 48. Das ist
//! Absicht: alles Neue haengt am Manifest, nichts aendert sich ohne eins.

use crate::package::{self, Manifest};

/// Ein geladenes Paket.
pub struct Package {
    pub manifest: Manifest,
    /// Verzeichnis des Manifests, normalisiert und absolut.
    pub root: String,
    /// Pfad der Manifestdatei, wie er gemeldet wird.
    pub manifestpfad: String,
    /// Index in `Welt::pakete` je Eintrag von `manifest.abhaengig`.
    pub edges: Vec<usize>,
}

/// Alle Pakete dieser Uebersetzung. `pakete[0]` ist das Wurzelpaket.
pub struct World {
    pub packages: Vec<Package>,
}

fn err(text: String) -> String {
    format!("error: {}\n", text)
}

fn error_with_note(text: String, note: String) -> String {
    format!("error: {}\nhinweis: {}\n", text, note)
}

/// Arbeitsverzeichnis, normalisiert. Alles Interne rechnet absolut, damit
/// „liegt diese Datei in jenem Paket" eine reine Zeichenkettenfrage bleibt.
pub fn cwd() -> String {
    match std::env::current_dir() {
        Ok(p) => package::normalize(&p.display().to_string()),
        Err(_) => "/".to_string(),
    }
}

/// `pfad` absolut machen (relativ zu `cwd`).
pub fn absolute(path: &str, cwd: &str) -> String {
    if path.starts_with('/') {
        package::normalize(path)
    } else {
        package::join(cwd, path)
    }
}

fn is_file(p: &str) -> bool {
    std::path::Path::new(p).is_file()
}

/// Sucht `firn.package` ab `verzeichnis` nach oben. Gibt das Verzeichnis
/// zurueck, in dem es liegt.
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
            return Err(err(format!("kann '{}' nicht lesen: {}", mpath, e)));
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
    /// Leere Welt: kein Manifest, alles wie vor Runde 48.
    pub fn empty() -> World {
        World { packages: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }

    /// Welt zu einem Wurzelverzeichnis (dort MUSS ein Manifest liegen).
    pub fn ab_root(root: &str) -> Result<World, String> {
        let c = cwd();
        let w = absolute(root, &c);
        if !is_file(&package::join(&w, package::MANIFEST)) {
            return Err(error_with_note(
                format!("kein manifest in '{}'", root),
                format!("erwartet wird die datei '{}'", package::join(root, package::MANIFEST)),
            ));
        }
        World::build(&w)
    }

    /// Welt zu einer Quelldatei: Manifest ab ihrem Verzeichnis nach oben
    /// suchen. Ohne Fund eine leere Welt.
    pub fn ab_file(file: &str) -> Result<World, String> {
        let c = cwd();
        let d = package::dirname(&absolute(file, &c));
        match search_manifest(&d) {
            Some(w) => World::build(&w),
            None => Ok(World::empty()),
        }
    }

    /// Baut die Welt ab einem absoluten, normalisierten Wurzelverzeichnis.
    fn build(root: &str) -> Result<World, String> {
        let mut packages: Vec<Package> = Vec::new();
        let (m, mp) = load(root)?;
        packages.push(Package {
            manifest: m,
            root: root.to_string(),
            manifestpfad: mp,
            edges: Vec::new(),
        });
        // BREITENSUCHE ueber `brauche`. Ein bereits geladenes Paket wird am
        // Wurzelverzeichnis wiedererkannt — derselbe Ort ist dasselbe Paket,
        // auch wenn zwei Manifeste ihn verschieden schreiben.
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
                                    "{}:{}: abhaengigkeit '{}' hat kein manifest",
                                    mpath, a.line, a.name
                                ),
                                format!("erwartet wird '{}'", package::join(&dw, package::MANIFEST)),
                            ));
                        }
                        let (dm, dmp) = load(&dw)?;
                        if dm.name != a.name {
                            return Err(error_with_note(
                                format!(
                                    "{}:{}: abhaengigkeit '{}' zeigt auf paket '{}'",
                                    mpath, a.line, a.name, dm.name
                                ),
                                format!("'{}' nennt sich selbst '{}'", dmp, dm.name),
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
        let world = World { packages };
        world.check_cycles()?;
        Ok(world)
    }

    /// Tiefensuche mit drei Farben: 0 = ungesehen, 1 = auf dem Weg,
    /// 2 = fertig. Trifft der Weg auf sich selbst, ist es ein Zyklus.
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
                    format!("paketzyklus: {}", z),
                    "abhaengigkeiten muessen einen kreisfreien graphen bilden".to_string(),
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

    /// Zu welchem Paket gehoert diese Datei? Laengste passende Wurzel
    /// gewinnt, damit ein Paket IM Verzeichnis eines anderen liegen darf.
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

    /// Index der Abhaengigkeit `name` von Paket `i`.
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

/// Fehlertext „modul ist nicht oeffentlich" — an EINER Stelle, damit
/// `firnc0` und `firnc1` denselben Satz schreiben.
pub fn text_not_public(module: &str, package_name: &str, manifestpfad: &str) -> String {
    error_with_note(
        format!("modul '{}' ist in paket '{}' nicht oeffentlich", module, package_name),
        format!("ergaenze 'public {}' in '{}'", module, manifestpfad),
    )
}

/// Fehlertext „paket ist keine abhaengigkeit".
pub fn text_no_dependency(target: &str, of: &str, manifestpfad: &str) -> String {
    error_with_note(
        format!("paket '{}' ist keine abhaengigkeit von paket '{}'", target, of),
        format!("ergaenze 'needs {} <pfad>' in '{}'", target, manifestpfad),
    )
}

/// Fehlertext „zwei dateien, ein modulname".
pub fn text_name_clash(module: &str, a: &str, b: &str) -> String {
    error_with_note(
        format!("namenskonflikt: modul '{}' kommt aus zwei dateien", module),
        format!("'{}' und '{}'", a, b),
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
    fn error_texts_are_fixed() {
        assert_eq!(
            text_not_public("inner", "geo", "/p/geo/firn.package"),
            "error: modul 'inner' ist in paket 'geo' nicht oeffentlich\n\
             hinweis: ergaenze 'public inner' in '/p/geo/firn.package'\n"
        );
        assert_eq!(
            text_no_dependency("geo", "app", "/p/app/firn.package"),
            "error: paket 'geo' ist keine abhaengigkeit von paket 'app'\n\
             hinweis: ergaenze 'needs geo <pfad>' in '/p/app/firn.package'\n"
        );
        assert_eq!(
            text_name_clash("util", "/a/util.fi", "/b/util.fi"),
            "error: namenskonflikt: modul 'util' kommt aus zwei dateien\n\
             hinweis: '/a/util.fi' und '/b/util.fi'\n"
        );
    }
}
