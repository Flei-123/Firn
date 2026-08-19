//! Projektmanifest `firn.paket` — Name, Version, Einstiegspunkt,
//! Quellverzeichnisse, oeffentliche Module, Abhaengigkeiten.
//!
//! WARUM KEIN TOML (Runde 48, Entscheidung mit Begruendung)
//! ---------------------------------------------------------------------
//! TOML ist eine echte Spezifikation: maskierte und mehrzeilige
//! Zeichenketten, Reihungen, eingebettete Tabellen, Datumswerte,
//! Zahlensyntax. Dieser Uebersetzer hat KEINE Fremdbibliotheken, und alles
//! muss ZWEIMAL stehen — in Rust (`firnc0`) und in Firn (`lib/firnc1/paket.fi`,
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
//! vertipptes `oeffentlih` wuerde sonst eine Schnittstelle oeffnen, die
//! niemand oeffnen wollte.

/// Dateiname des Manifests. Steht ausschliesslich hier.
pub const MANIFEST: &str = "firn.paket";

/// Wie viele Verzeichnisebenen die Suche nach oben hoechstens geht.
pub const SUCHTIEFE: usize = 64;

/// Eine Abhaengigkeit: Name (wird zum Importpraefix) und lokaler Pfad.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Abhaengigkeit {
    pub name: String,
    pub pfad: String,
    pub zeile: u32,
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
    pub quellen: Vec<String>,
    /// Oeffentliche Module. LEER heisst: alles ist oeffentlich — dieselbe
    /// Regel wie bei `export { … }` innerhalb einer Datei.
    pub oeffentlich: Vec<String>,
    pub abhaengig: Vec<Abhaengigkeit>,
}

/// Fehler beim Lesen eines Manifests. `zeile` = 0 heisst: betrifft die Datei
/// als Ganzes (fehlende Pflichtangabe).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fehler {
    pub zeile: u32,
    pub msg: String,
}

impl Manifest {
    /// Ist `modul` von aussen sichtbar?
    pub fn ist_oeffentlich(&self, modul: &str) -> bool {
        self.oeffentlich.is_empty() || self.oeffentlich.iter().any(|m| m == modul)
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
pub fn normalisiere(pfad: &str) -> String {
    let absolut = pfad.starts_with('/');
    let mut teile: Vec<&str> = Vec::new();
    for t in pfad.split('/') {
        if t.is_empty() || t == "." {
            continue;
        }
        if t == ".." {
            let hoch = match teile.last() {
                Some(l) => *l != "..",
                None => false,
            };
            if hoch {
                teile.pop();
            } else if !absolut {
                teile.push("..");
            }
            continue;
        }
        teile.push(t);
    }
    let mut s = String::new();
    if absolut {
        s.push('/');
    }
    s.push_str(&teile.join("/"));
    if s.is_empty() {
        s.push('.');
    }
    s
}

/// `basis` + `rel`, normalisiert. Ein absolutes `rel` gewinnt.
pub fn verbinde(basis: &str, rel: &str) -> String {
    if rel.starts_with('/') {
        return normalisiere(rel);
    }
    if basis.is_empty() {
        return normalisiere(rel);
    }
    normalisiere(&format!("{}/{}", basis, rel))
}

/// Verzeichnisteil eines Pfades (ohne den letzten Namen).
pub fn verzeichnis(pfad: &str) -> String {
    match pfad.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => pfad[..i].to_string(),
        None => ".".to_string(),
    }
}

/// Letzter Namensteil ohne `.fi`-Endung — der Modulname einer Datei.
pub fn modulname(pfad: &str) -> String {
    let letzter = match pfad.rfind('/') {
        Some(i) => &pfad[i + 1..],
        None => pfad,
    };
    match letzter.rfind('.') {
        Some(i) if i > 0 => letzter[..i].to_string(),
        _ => letzter.to_string(),
    }
}

/// Liegt `pfad` in `wurzel` (oder IST er es)? Beide muessen normalisiert sein.
pub fn liegt_in(pfad: &str, wurzel: &str) -> bool {
    if pfad == wurzel {
        return true;
    }
    if wurzel == "/" {
        return pfad.starts_with('/');
    }
    pfad.len() > wurzel.len()
        && pfad.starts_with(wurzel)
        && pfad.as_bytes()[wurzel.len()] == b'/'
}

// ------------------------------------------------------------------- Pruefen

/// Bezeichner: Buchstabe zuerst, dann Buchstaben, Ziffern, Unterstrich.
/// Paket- und Modulnamen werden zu Importpraefixen, deshalb dieselbe Regel
/// wie fuer Bezeichner der Sprache.
pub fn ist_name(s: &str) -> bool {
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
pub fn ist_version(s: &str) -> bool {
    let mut teile = 0;
    for t in s.split('.') {
        teile += 1;
        if t.is_empty() || !t.bytes().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    teile == 3
}

/// Pfad INNERHALB des Pakets: relativ, ohne `..`, nicht leer.
pub fn ist_innenpfad(s: &str) -> bool {
    if s.is_empty() || s.starts_with('/') {
        return false;
    }
    !s.split('/').any(|t| t == "..")
}

/// Pfad einer Abhaengigkeit: darf hinausfuehren, aber nicht leer sein.
pub fn ist_aussenpfad(s: &str) -> bool {
    !s.is_empty()
}

// -------------------------------------------------------------------- Lesen

fn worte(zeile: &str) -> Vec<&str> {
    zeile
        .split(|c| c == ' ' || c == '\t')
        .filter(|w| !w.is_empty())
        .collect()
}

/// Liest ein Manifest aus dem Text. Reine Funktion: kein Dateisystem, damit
/// die Regeln einzeln pruefbar bleiben.
pub fn lies(text: &str) -> Result<Manifest, Fehler> {
    let mut m = Manifest::default();
    let mut hat_name = false;
    let mut hat_version = false;
    let mut hat_start = false;
    let mut nr = 0u32;
    for roh in text.split('\n') {
        nr += 1;
        let ohne_cr = roh.strip_suffix('\r').unwrap_or(roh);
        let zeile = match ohne_cr.find('#') {
            Some(i) => &ohne_cr[..i],
            None => ohne_cr,
        };
        let w = worte(zeile);
        if w.is_empty() {
            continue;
        }
        let fehler = |msg: &str| Fehler { zeile: nr, msg: msg.to_string() };
        match w[0] {
            "paket" => {
                if w.len() != 2 {
                    return Err(fehler("'paket' erwartet genau einen namen"));
                }
                if hat_name {
                    return Err(fehler("'paket' steht mehrfach im manifest"));
                }
                if !ist_name(w[1]) {
                    return Err(fehler(&format!(
                        "ungueltiger name '{}' (buchstabe zuerst, dann buchstaben, ziffern, unterstrich)",
                        w[1]
                    )));
                }
                m.name = w[1].to_string();
                hat_name = true;
            }
            "version" => {
                if w.len() != 2 {
                    return Err(fehler("'version' erwartet genau eine versionsnummer"));
                }
                if hat_version {
                    return Err(fehler("'version' steht mehrfach im manifest"));
                }
                if !ist_version(w[1]) {
                    return Err(fehler(&format!(
                        "ungueltige version '{}' (erwartet zahl.zahl.zahl)",
                        w[1]
                    )));
                }
                m.version = w[1].to_string();
                hat_version = true;
            }
            "start" => {
                if w.len() != 2 {
                    return Err(fehler("'start' erwartet genau einen pfad"));
                }
                if hat_start {
                    return Err(fehler("'start' steht mehrfach im manifest"));
                }
                if !ist_innenpfad(w[1]) {
                    return Err(fehler(&format!(
                        "ungueltiger pfad '{}' (relativ, ohne '..')",
                        w[1]
                    )));
                }
                m.start = w[1].to_string();
                hat_start = true;
            }
            "quelle" => {
                if w.len() != 2 {
                    return Err(fehler("'quelle' erwartet genau einen pfad"));
                }
                if !ist_innenpfad(w[1]) {
                    return Err(fehler(&format!(
                        "ungueltiger pfad '{}' (relativ, ohne '..')",
                        w[1]
                    )));
                }
                let q = normalisiere(w[1]);
                if m.quellen.iter().any(|x| *x == q) {
                    return Err(fehler(&format!("quelle '{}' steht mehrfach im manifest", w[1])));
                }
                m.quellen.push(q);
            }
            "oeffentlich" => {
                if w.len() < 2 {
                    return Err(fehler("'oeffentlich' erwartet mindestens einen modulnamen"));
                }
                for x in &w[1..] {
                    if !ist_name(x) {
                        return Err(fehler(&format!(
                            "ungueltiger name '{}' (buchstabe zuerst, dann buchstaben, ziffern, unterstrich)",
                            x
                        )));
                    }
                    if m.oeffentlich.iter().any(|y| y == x) {
                        return Err(fehler(&format!(
                            "modul '{}' steht mehrfach in 'oeffentlich'",
                            x
                        )));
                    }
                    m.oeffentlich.push(x.to_string());
                }
            }
            "brauche" => {
                if w.len() != 3 {
                    return Err(fehler("'brauche' erwartet einen namen und einen pfad"));
                }
                if !ist_name(w[1]) {
                    return Err(fehler(&format!(
                        "ungueltiger name '{}' (buchstabe zuerst, dann buchstaben, ziffern, unterstrich)",
                        w[1]
                    )));
                }
                if !ist_aussenpfad(w[2]) {
                    return Err(fehler("'brauche' erwartet einen namen und einen pfad"));
                }
                if m.abhaengig.iter().any(|a| a.name == w[1]) {
                    return Err(fehler(&format!(
                        "paket '{}' steht mehrfach als abhaengigkeit im manifest",
                        w[1]
                    )));
                }
                m.abhaengig.push(Abhaengigkeit {
                    name: w[1].to_string(),
                    pfad: w[2].to_string(),
                    zeile: nr,
                });
            }
            other => {
                return Err(fehler(&format!(
                    "unbekannter schluessel '{}' (erlaubt: paket, version, start, quelle, oeffentlich, brauche)",
                    other
                )));
            }
        }
    }
    if !hat_name {
        return Err(Fehler { zeile: 0, msg: "das manifest braucht eine zeile 'paket <name>'".to_string() });
    }
    if !hat_version {
        return Err(Fehler { zeile: 0, msg: "das manifest braucht eine zeile 'version <zahl.zahl.zahl>'".to_string() });
    }
    if m.abhaengig.iter().any(|a| a.name == m.name) {
        let z = m.abhaengig.iter().find(|a| a.name == m.name).map(|a| a.zeile).unwrap_or(0);
        return Err(Fehler {
            zeile: z,
            msg: format!("abhaengigkeit '{}' heisst wie das paket selbst", m.name),
        });
    }
    if m.quellen.is_empty() {
        m.quellen.push(".".to_string());
    }
    Ok(m)
}

// ------------------------------------------------------------------ Ausgabe

/// Der Bericht von `--paket-info`. Zeichengleich in beiden Uebersetzern;
/// alle Pfade sind rein lexikalisch aus `wurzel` gebaut (kein `getcwd`,
/// keine symbolischen Verweise), damit die Ausgabe nicht vom Rechner abhaengt.
pub fn info_text(m: &Manifest, wurzel: &str) -> String {
    let w = normalisiere(wurzel);
    let mut s = String::new();
    s.push_str(&format!("paket {}\n", m.name));
    s.push_str(&format!("version {}\n", m.version));
    s.push_str(&format!("wurzel {}\n", w));
    if !m.start.is_empty() {
        s.push_str(&format!("start {}\n", verbinde(&w, &m.start)));
    }
    for q in &m.quellen {
        s.push_str(&format!("quelle {}\n", verbinde(&w, q)));
    }
    for o in &m.oeffentlich {
        s.push_str(&format!("oeffentlich {}\n", o));
    }
    for a in &m.abhaengig {
        s.push_str(&format!("brauche {} {}\n", a.name, verbinde(&w, &a.pfad)));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str) -> Manifest {
        lies(text).expect("manifest sollte gueltig sein")
    }

    #[test]
    fn kleinstes_gueltiges_manifest() {
        let x = m("paket demo\nversion 0.1.0\nstart src/main.fi\n");
        assert_eq!(x.name, "demo");
        assert_eq!(x.version, "0.1.0");
        assert_eq!(x.start, "src/main.fi");
        // Ohne 'quelle' gilt das Manifestverzeichnis selbst.
        assert_eq!(x.quellen, vec![".".to_string()]);
        assert!(x.oeffentlich.is_empty());
        assert!(x.abhaengig.is_empty());
        // Leere Schnittstelle heisst: alles oeffentlich (wie 'export').
        assert!(x.ist_oeffentlich("irgendwas"));
    }

    #[test]
    fn kommentare_leerzeilen_tabulatoren() {
        let x = m("# kopf\n\n\tpaket\tdemo\t# name\nversion 1.2.3\nstart a.fi\n   \n");
        assert_eq!(x.name, "demo");
        assert_eq!(x.version, "1.2.3");
    }

    #[test]
    fn quellen_oeffentlich_und_abhaengigkeiten() {
        let x = m("paket app\nversion 0.0.1\nstart src/main.fi\nquelle src\nquelle extra\n\
                   oeffentlich a b\noeffentlich c\nbrauche geo ../geo\nbrauche txt /opt/txt\n");
        assert_eq!(x.quellen, vec!["src".to_string(), "extra".to_string()]);
        assert_eq!(x.oeffentlich, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        assert!(x.ist_oeffentlich("b"));
        assert!(!x.ist_oeffentlich("d"));
        assert_eq!(x.abhaengig.len(), 2);
        assert_eq!(x.abhaengig[0].name, "geo");
        assert_eq!(x.abhaengig[0].pfad, "../geo");
        assert_eq!(x.abhaengig[1].name, "txt");
        assert_eq!(x.abhaengig[1].pfad, "/opt/txt");
    }

    #[test]
    fn fehlende_pflichtangaben() {
        assert_eq!(lies("version 1.0.0\nstart a.fi\n").unwrap_err().msg,
                   "das manifest braucht eine zeile 'paket <name>'");
        assert_eq!(lies("paket a\nstart a.fi\n").unwrap_err().msg,
                   "das manifest braucht eine zeile 'version <zahl.zahl.zahl>'");
        // 'start' ist KEINE Pflicht: eine Bibliothek hat keinen Einstiegspunkt.
        assert_eq!(lies("paket a\nversion 1.0.0\n").unwrap().start, "");
    }

    #[test]
    fn unbekannter_schluessel_ist_ein_fehler() {
        let e = lies("paket a\nversion 1.0.0\nstart a.fi\noeffentlih b\n").unwrap_err();
        assert_eq!(e.zeile, 4);
        assert!(e.msg.starts_with("unbekannter schluessel 'oeffentlih'"), "{}", e.msg);
    }

    #[test]
    fn geprueft_werden_name_version_pfad() {
        assert!(lies("paket 1a\nversion 1.0.0\nstart a.fi\n").unwrap_err().msg.contains("ungueltiger name '1a'"));
        assert!(lies("paket a\nversion 1.0\nstart a.fi\n").unwrap_err().msg.contains("ungueltige version '1.0'"));
        assert!(lies("paket a\nversion 1.0.0\nstart ../x.fi\n").unwrap_err().msg.contains("ungueltiger pfad '../x.fi'"));
        assert!(lies("paket a\nversion 1.0.0\nstart /x.fi\n").unwrap_err().msg.contains("ungueltiger pfad '/x.fi'"));
        assert!(ist_name("a_1"));
        assert!(!ist_name(""));
        assert!(!ist_name("a-b"));
        assert!(ist_version("10.20.30"));
        assert!(!ist_version("1.2.3.4"));
        assert!(!ist_version("1.x.3"));
    }

    #[test]
    fn doppelte_eintraege_werden_gemeldet() {
        assert!(lies("paket a\npaket b\nversion 1.0.0\nstart a.fi\n").unwrap_err().msg.contains("'paket' steht mehrfach"));
        assert!(lies("paket a\nversion 1.0.0\nversion 1.0.1\nstart a.fi\n").unwrap_err().msg.contains("'version' steht mehrfach"));
        assert!(lies("paket a\nversion 1.0.0\nstart a.fi\nquelle s\nquelle s\n").unwrap_err().msg.contains("quelle 's' steht mehrfach"));
        assert!(lies("paket a\nversion 1.0.0\nstart a.fi\noeffentlich m m\n").unwrap_err().msg.contains("modul 'm' steht mehrfach"));
        assert!(lies("paket a\nversion 1.0.0\nstart a.fi\nbrauche g ../g\nbrauche g ../h\n").unwrap_err().msg.contains("steht mehrfach als abhaengigkeit"));
        assert!(lies("paket a\nversion 1.0.0\nstart a.fi\nbrauche a ../a\n").unwrap_err().msg.contains("heisst wie das paket selbst"));
    }

    #[test]
    fn falsche_stelligkeit() {
        assert!(lies("paket a b\nversion 1.0.0\nstart a.fi\n").unwrap_err().msg.contains("'paket' erwartet genau einen namen"));
        assert!(lies("paket a\nversion 1.0.0\nstart a.fi\nbrauche g\n").unwrap_err().msg.contains("'brauche' erwartet einen namen und einen pfad"));
        assert!(lies("paket a\nversion 1.0.0\nstart a.fi\noeffentlich\n").unwrap_err().msg.contains("'oeffentlich' erwartet mindestens einen modulnamen"));
    }

    #[test]
    fn pfade_normalisieren() {
        assert_eq!(normalisiere("a/./b/../c"), "a/c");
        assert_eq!(normalisiere("/a/b/../../c"), "/c");
        assert_eq!(normalisiere("/.."), "/");
        assert_eq!(normalisiere("../../a"), "../../a");
        assert_eq!(normalisiere(""), ".");
        assert_eq!(normalisiere("."), ".");
        assert_eq!(normalisiere("/"), "/");
        assert_eq!(normalisiere("a//b/"), "a/b");
        assert_eq!(verbinde("/x/y", "../z"), "/x/z");
        assert_eq!(verbinde("/x/y", "/abs"), "/abs");
        assert_eq!(verbinde("", "a/b"), "a/b");
        assert_eq!(verzeichnis("/a/b/c.fi"), "/a/b");
        assert_eq!(verzeichnis("/c.fi"), "/");
        assert_eq!(verzeichnis("c.fi"), ".");
        assert_eq!(modulname("/a/b/geo.fi"), "geo");
        assert_eq!(modulname("geo"), "geo");
    }

    #[test]
    fn zugehoerigkeit_zu_einem_paket() {
        assert!(liegt_in("/a/b/c.fi", "/a/b"));
        assert!(liegt_in("/a/b", "/a/b"));
        assert!(!liegt_in("/a/bc/d.fi", "/a/b"));
        assert!(!liegt_in("/a", "/a/b"));
        assert!(liegt_in("/a", "/"));
    }

    #[test]
    fn infotext_ist_rein_lexikalisch() {
        let x = m("paket app\nversion 0.2.0\nstart src/main.fi\nquelle src\n\
                   oeffentlich app\nbrauche geo ../geo\n");
        assert_eq!(
            info_text(&x, "./beispiel/app/"),
            "paket app\nversion 0.2.0\nwurzel beispiel/app\nstart beispiel/app/src/main.fi\n\
             quelle beispiel/app/src\noeffentlich app\nbrauche geo beispiel/geo\n"
        );
    }
}
