//! Projektmanifest `firn.paket` — Name, Version, Einstiegspunkt,
//! Quellverzeichnisse, oeffentliche Module, Abhaengigkeiten.
//!
//! WARUM KEIN TOML (Runde 48, Entscheidung mit Begruendung)
//! ---------------------------------------------------------------------
//! TOML ist eine echte Spezifikation: maskierte und mehrzeilige
//! Zeichenketten, Reihungen, eingebettete Tabellen, Datumswerte,
//! Zahlensyntax. Dieser Uebersetzer hat KEINE Fremdbibliotheken, und alles
//! muss ZWEIMAL stehen — in Rust (`firnc0`) und in Firn (`lib/firnc1/package.fi`,
//! ohne libc, nur Puffer und `syscall`). Ein *halbes* TOML waere die
//! schlechteste Loesung: es sieht aus wie TOML, nimmt aber gueltige
//! TOML-Dateien nicht an oder liest sie anders. Deshalb ein eigenes,
//! absichtlich winziges Zeilenformat mit eigener Endung — niemand erwartet
//! davon TOML-Semantik.
//!
//! FORMAT
//! ---------------------------------------------------------------------
//! Eine Anweisung je Zeile: `schluessel wert [wert ...]`. Trennzeichen sind
//! Leerzeichen und Tabulator, `#` leitet einen Kommentar bis zum Zeilenende
//! ein, Leerzeilen zaehlen nicht. Es gibt keine Anfuehrungszeichen und keine
//! Maskierungen — ein Wert enthaelt deshalb weder Leerzeichen noch `#`.
//!
//! ```text
//! paket        demo            # Pflicht, genau einmal
//! version      0.1.0           # Pflicht, genau einmal, zahl.zahl.zahl
//! start        src/main.fi     # hoechstens einmal, relativ zum Manifest;
//!                              #      eine Bibliothek hat keinen
//! quelle       src             # 0..n, relativ; ohne Angabe gilt das
//!                              #      Manifestverzeichnis selbst
//! oeffentlich  geo punkt       # 0..n, Modulschnittstelle des Pakets;
//!                              #      ohne Angabe ist alles oeffentlich
//! brauche      geo ../geo      # 0..n, Name + lokaler Pfad
//! ```
//!
//! Unbekannte Schluessel sind ein FEHLER, kein stilles Ueberlesen: ein
//! vertipptes `publi` wuerde sonst eine Schnittstelle oeffnen, die
//! niemand oeffnen wollte.

/// Dateiname des Manifests. Steht ausschliesslich hier.
pub const MANIFEST: &str = "firn.package";

/// Wie viele Verzeichnisebenen die Suche nach oben hoechstens geht.
pub const SUCHTIEFE: usize = 64;

/// Eine Abhaengigkeit: Name (wird zum Importpraefix) und lokaler Pfad.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    pub path: String,
    pub line: u32,
}

/// Der Inhalt eines Manifests, geprueft, aber noch ohne Bezug zum Dateisystem.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    /// Einstiegspunkt. LEER heisst: das Paket ist eine Bibliothek und kann
    /// nicht mit `--paket` gebaut werden.
    pub start: String,
    /// Quellverzeichnisse, relativ zum Manifest. Nie leer (Standard: `.`).
    pub sources: Vec<String>,
    /// Oeffentliche Module. LEER heisst: alles ist oeffentlich — dieselbe
    /// Regel wie bei `export { … }` innerhalb einer Datei.
    pub public: Vec<String>,
    pub dependent: Vec<Dependency>,
}

/// Fehler beim Lesen eines Manifests. `zeile` = 0 heisst: betrifft die Datei
/// als Ganzes (fehlende Pflichtangabe).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub line: u32,
    pub msg: String,
}

impl Manifest {
    /// Ist `modul` von aussen sichtbar?
    pub fn is_public(&self, module: &str) -> bool {
        self.public.is_empty() || self.public.iter().any(|m| m == module)
    }
}

// --------------------------------------------------------------- Pfadrechnen
//
// REIN LEXIKALISCH, ohne Dateisystem: dieselbe Rechnung muss in Firn
// nachvollziehbar sein, und `--paket-info` soll auf beiden Uebersetzern
// zeichengleich ausfallen. Symbolische Verweise werden dabei NICHT aufgeloest
// (das kann `firnc1` ohne libc nicht, und es wuerde die Ausgabe
// rechnerabhaengig machen).

/// `a/./b/../c` -> `a/c`. Fuehrender Schraegstrich bleibt erhalten.
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

/// `basis` + `rel`, normalisiert. Ein absolutes `rel` gewinnt.
pub fn join(base: &str, rel: &str) -> String {
    if rel.starts_with('/') {
        return normalize(rel);
    }
    if base.is_empty() {
        return normalize(rel);
    }
    normalize(&format!("{}/{}", base, rel))
}

/// Verzeichnisteil eines Pfades (ohne den letzten Namen).
pub fn dirname(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => path[..i].to_string(),
        None => ".".to_string(),
    }
}

/// Letzter Namensteil ohne `.fi`-Endung — der Modulname einer Datei.
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

/// Liegt `pfad` in `wurzel` (oder IST er es)? Beide muessen normalisiert sein.
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

// ------------------------------------------------------------------- Pruefen

/// Bezeichner: Buchstabe zuerst, dann Buchstaben, Ziffern, Unterstrich.
/// Paket- und Modulnamen werden zu Importpraefixen, deshalb dieselbe Regel
/// wie fuer Bezeichner der Sprache.
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

/// `zahl.zahl.zahl`, jede Stelle mindestens eine Ziffer.
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

/// Pfad INNERHALB des Pakets: relativ, ohne `..`, nicht leer.
pub fn is_inner_path(s: &str) -> bool {
    if s.is_empty() || s.starts_with('/') {
        return false;
    }
    !s.split('/').any(|t| t == "..")
}

/// Pfad einer Abhaengigkeit: darf hinausfuehren, aber nicht leer sein.
pub fn is_outer_path(s: &str) -> bool {
    !s.is_empty()
}

// -------------------------------------------------------------------- Lesen

fn words(line: &str) -> Vec<&str> {
    line
        .split(|c| c == ' ' || c == '\t')
        .filter(|w| !w.is_empty())
        .collect()
}

/// Liest ein Manifest aus dem Text. Reine Funktion: kein Dateisystem, damit
/// die Regeln einzeln pruefbar bleiben.
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
                if w.len() != 3 {
                    return Err(err("'needs' expects a name and a path"));
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

// ------------------------------------------------------------------ Ausgabe

/// Der Bericht von `--paket-info`. Zeichengleich in beiden Uebersetzern;
/// alle Pfade sind rein lexikalisch aus `wurzel` gebaut (kein `getcwd`,
/// keine symbolischen Verweise), damit die Ausgabe nicht vom Rechner abhaengt.
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
        s.push_str(&format!("needs {} {}\n", a.name, join(&w, &a.path)));
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
        // Ohne 'source' gilt das Manifestverzeichnis selbst.
        assert_eq!(x.sources, vec![".".to_string()]);
        assert!(x.public.is_empty());
        assert!(x.dependent.is_empty());
        // Leere Schnittstelle heisst: alles oeffentlich (wie 'export').
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
        // 'start' ist KEINE Pflicht: eine Bibliothek hat keinen Einstiegspunkt.
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

    #[test]
    fn membership_zu_a_package() {
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
