//! Die Paketwelt: das Wurzelmanifest, alle ueber `brauche` erreichbaren
//! Pakete und der Graph dazwischen.
//!
//! `paket.rs` kennt nur das Format (reine Funktionen, ohne Dateisystem).
//! Hier kommt das Dateisystem dazu: Manifest suchen, lesen, Abhaengigkeiten
//! nachladen, Zyklen finden.
//!
//! SUCHE NACH DEM MANIFEST: von dem Verzeichnis der Wurzeldatei aus nach
//! OBEN, bis `firn.paket` gefunden ist oder das Dateisystem endet
//! (`paket::SUCHTIEFE` als Notbremse). Findet sich keins, ist die Welt LEER —
//! und dann verhaelt sich der Uebersetzer exakt wie vor Runde 48. Das ist
//! Absicht: alles Neue haengt am Manifest, nichts aendert sich ohne eins.

use crate::paket::{self, Manifest};

/// Ein geladenes Paket.
pub struct Paket {
    pub manifest: Manifest,
    /// Verzeichnis des Manifests, normalisiert und absolut.
    pub wurzel: String,
    /// Pfad der Manifestdatei, wie er gemeldet wird.
    pub manifestpfad: String,
    /// Index in `Welt::pakete` je Eintrag von `manifest.abhaengig`.
    pub kanten: Vec<usize>,
}

/// Alle Pakete dieser Uebersetzung. `pakete[0]` ist das Wurzelpaket.
pub struct Welt {
    pub pakete: Vec<Paket>,
}

fn fehler(text: String) -> String {
    format!("error: {}\n", text)
}

fn fehler_mit_hinweis(text: String, hinweis: String) -> String {
    format!("error: {}\nhinweis: {}\n", text, hinweis)
}

/// Arbeitsverzeichnis, normalisiert. Alles Interne rechnet absolut, damit
/// „liegt diese Datei in jenem Paket" eine reine Zeichenkettenfrage bleibt.
pub fn cwd() -> String {
    match std::env::current_dir() {
        Ok(p) => paket::normalisiere(&p.display().to_string()),
        Err(_) => "/".to_string(),
    }
}

/// `pfad` absolut machen (relativ zu `cwd`).
pub fn absolut(pfad: &str, cwd: &str) -> String {
    if pfad.starts_with('/') {
        paket::normalisiere(pfad)
    } else {
        paket::verbinde(cwd, pfad)
    }
}

fn ist_datei(p: &str) -> bool {
    std::path::Path::new(p).is_file()
}

/// Sucht `firn.paket` ab `verzeichnis` nach oben. Gibt das Verzeichnis
/// zurueck, in dem es liegt.
pub fn suche_manifest(verzeichnis: &str) -> Option<String> {
    let mut d = paket::normalisiere(verzeichnis);
    for _ in 0..paket::SUCHTIEFE {
        if ist_datei(&paket::verbinde(&d, paket::MANIFEST)) {
            return Some(d);
        }
        let hoch = paket::verbinde(&d, "..");
        if hoch == d {
            return None;
        }
        d = hoch;
    }
    None
}

fn lade(wurzel: &str) -> Result<(Manifest, String), String> {
    let mpfad = paket::verbinde(wurzel, paket::MANIFEST);
    let text = match std::fs::read_to_string(&mpfad) {
        Ok(t) => t,
        Err(e) => {
            return Err(fehler(format!("kann '{}' nicht lesen: {}", mpfad, e)));
        }
    };
    match paket::lies(&text) {
        Ok(m) => Ok((m, mpfad)),
        Err(f) => {
            if f.zeile == 0 {
                Err(fehler(format!("{}: {}", mpfad, f.msg)))
            } else {
                Err(fehler(format!("{}:{}: {}", mpfad, f.zeile, f.msg)))
            }
        }
    }
}

impl Welt {
    /// Leere Welt: kein Manifest, alles wie vor Runde 48.
    pub fn leer() -> Welt {
        Welt { pakete: Vec::new() }
    }

    pub fn ist_leer(&self) -> bool {
        self.pakete.is_empty()
    }

    /// Welt zu einem Wurzelverzeichnis (dort MUSS ein Manifest liegen).
    pub fn ab_wurzel(wurzel: &str) -> Result<Welt, String> {
        let c = cwd();
        let w = absolut(wurzel, &c);
        if !ist_datei(&paket::verbinde(&w, paket::MANIFEST)) {
            return Err(fehler_mit_hinweis(
                format!("kein manifest in '{}'", wurzel),
                format!("erwartet wird die datei '{}'", paket::verbinde(wurzel, paket::MANIFEST)),
            ));
        }
        Welt::bauen(&w)
    }

    /// Welt zu einer Quelldatei: Manifest ab ihrem Verzeichnis nach oben
    /// suchen. Ohne Fund eine leere Welt.
    pub fn ab_datei(datei: &str) -> Result<Welt, String> {
        let c = cwd();
        let d = paket::verzeichnis(&absolut(datei, &c));
        match suche_manifest(&d) {
            Some(w) => Welt::bauen(&w),
            None => Ok(Welt::leer()),
        }
    }

    /// Baut die Welt ab einem absoluten, normalisierten Wurzelverzeichnis.
    fn bauen(wurzel: &str) -> Result<Welt, String> {
        let mut pakete: Vec<Paket> = Vec::new();
        let (m, mp) = lade(wurzel)?;
        pakete.push(Paket {
            manifest: m,
            wurzel: wurzel.to_string(),
            manifestpfad: mp,
            kanten: Vec::new(),
        });
        // BREITENSUCHE ueber `brauche`. Ein bereits geladenes Paket wird am
        // Wurzelverzeichnis wiedererkannt — derselbe Ort ist dasselbe Paket,
        // auch wenn zwei Manifeste ihn verschieden schreiben.
        let mut i = 0usize;
        while i < pakete.len() {
            let eigene_wurzel = pakete[i].wurzel.clone();
            let deps = pakete[i].manifest.abhaengig.clone();
            let mpfad = pakete[i].manifestpfad.clone();
            let mut kanten = Vec::new();
            for a in &deps {
                let dw = absolut(&paket::verbinde(&eigene_wurzel, &a.pfad), &eigene_wurzel);
                let vorhanden = pakete.iter().position(|p| p.wurzel == dw);
                let idx = match vorhanden {
                    Some(k) => k,
                    None => {
                        if !ist_datei(&paket::verbinde(&dw, paket::MANIFEST)) {
                            return Err(fehler_mit_hinweis(
                                format!(
                                    "{}:{}: abhaengigkeit '{}' hat kein manifest",
                                    mpfad, a.zeile, a.name
                                ),
                                format!("erwartet wird '{}'", paket::verbinde(&dw, paket::MANIFEST)),
                            ));
                        }
                        let (dm, dmp) = lade(&dw)?;
                        if dm.name != a.name {
                            return Err(fehler_mit_hinweis(
                                format!(
                                    "{}:{}: abhaengigkeit '{}' zeigt auf paket '{}'",
                                    mpfad, a.zeile, a.name, dm.name
                                ),
                                format!("'{}' nennt sich selbst '{}'", dmp, dm.name),
                            ));
                        }
                        pakete.push(Paket {
                            manifest: dm,
                            wurzel: dw,
                            manifestpfad: dmp,
                            kanten: Vec::new(),
                        });
                        pakete.len() - 1
                    }
                };
                kanten.push(idx);
            }
            pakete[i].kanten = kanten;
            i += 1;
        }
        let welt = Welt { pakete };
        welt.pruefe_zyklen()?;
        Ok(welt)
    }

    /// Tiefensuche mit drei Farben: 0 = ungesehen, 1 = auf dem Weg,
    /// 2 = fertig. Trifft der Weg auf sich selbst, ist es ein Zyklus.
    fn pruefe_zyklen(&self) -> Result<(), String> {
        let n = self.pakete.len();
        let mut farbe = vec![0u8; n];
        let mut weg: Vec<usize> = Vec::new();
        for s in 0..n {
            if farbe[s] != 0 {
                continue;
            }
            if let Some(z) = self.dfs(s, &mut farbe, &mut weg) {
                return Err(fehler_mit_hinweis(
                    format!("paketzyklus: {}", z),
                    "abhaengigkeiten muessen einen kreisfreien graphen bilden".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn dfs(&self, i: usize, farbe: &mut Vec<u8>, weg: &mut Vec<usize>) -> Option<String> {
        farbe[i] = 1;
        weg.push(i);
        for &k in &self.pakete[i].kanten {
            if farbe[k] == 1 {
                let ab = weg.iter().position(|&x| x == k).unwrap_or(0);
                let mut namen: Vec<String> = weg[ab..]
                    .iter()
                    .map(|&x| self.pakete[x].manifest.name.clone())
                    .collect();
                namen.push(self.pakete[k].manifest.name.clone());
                return Some(namen.join(" -> "));
            }
            if farbe[k] == 0 {
                if let Some(z) = self.dfs(k, farbe, weg) {
                    return Some(z);
                }
            }
        }
        weg.pop();
        farbe[i] = 2;
        None
    }

    /// Zu welchem Paket gehoert diese Datei? Laengste passende Wurzel
    /// gewinnt, damit ein Paket IM Verzeichnis eines anderen liegen darf.
    pub fn paket_von(&self, absoluter_pfad: &str) -> Option<usize> {
        let mut treffer: Option<usize> = None;
        for (i, p) in self.pakete.iter().enumerate() {
            if paket::liegt_in(absoluter_pfad, &p.wurzel) {
                let besser = match treffer {
                    None => true,
                    Some(t) => p.wurzel.len() > self.pakete[t].wurzel.len(),
                };
                if besser {
                    treffer = Some(i);
                }
            }
        }
        treffer
    }

    /// Index der Abhaengigkeit `name` von Paket `i`.
    pub fn kante(&self, i: usize, name: &str) -> Option<usize> {
        let p = &self.pakete[i];
        p.manifest
            .abhaengig
            .iter()
            .position(|a| a.name == name)
            .and_then(|k| p.kanten.get(k).copied())
    }

    pub fn name(&self, i: usize) -> &str {
        &self.pakete[i].manifest.name
    }
}

/// Fehlertext „modul ist nicht oeffentlich" — an EINER Stelle, damit
/// `firnc0` und `firnc1` denselben Satz schreiben.
pub fn text_nicht_oeffentlich(modul: &str, paketname: &str, manifestpfad: &str) -> String {
    fehler_mit_hinweis(
        format!("modul '{}' ist in paket '{}' nicht oeffentlich", modul, paketname),
        format!("ergaenze 'oeffentlich {}' in '{}'", modul, manifestpfad),
    )
}

/// Fehlertext „paket ist keine abhaengigkeit".
pub fn text_keine_abhaengigkeit(ziel: &str, von: &str, manifestpfad: &str) -> String {
    fehler_mit_hinweis(
        format!("paket '{}' ist keine abhaengigkeit von paket '{}'", ziel, von),
        format!("ergaenze 'brauche {} <pfad>' in '{}'", ziel, manifestpfad),
    )
}

/// Fehlertext „zwei dateien, ein modulname".
pub fn text_namenskonflikt(modul: &str, a: &str, b: &str) -> String {
    fehler_mit_hinweis(
        format!("namenskonflikt: modul '{}' kommt aus zwei dateien", modul),
        format!("'{}' und '{}'", a, b),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolut_und_cwd() {
        assert_eq!(absolut("/a/b", "/x"), "/a/b");
        assert_eq!(absolut("a/b", "/x"), "/x/a/b");
        assert_eq!(absolut("../a", "/x/y"), "/x/a");
        assert!(cwd().starts_with('/'));
    }

    #[test]
    fn fehlertexte_stehen_fest() {
        assert_eq!(
            text_nicht_oeffentlich("innen", "geo", "/p/geo/firn.paket"),
            "error: modul 'innen' ist in paket 'geo' nicht oeffentlich\n\
             hinweis: ergaenze 'oeffentlich innen' in '/p/geo/firn.paket'\n"
        );
        assert_eq!(
            text_keine_abhaengigkeit("geo", "app", "/p/app/firn.paket"),
            "error: paket 'geo' ist keine abhaengigkeit von paket 'app'\n\
             hinweis: ergaenze 'brauche geo <pfad>' in '/p/app/firn.paket'\n"
        );
        assert_eq!(
            text_namenskonflikt("util", "/a/util.fi", "/b/util.fi"),
            "error: namenskonflikt: modul 'util' kommt aus zwei dateien\n\
             hinweis: '/a/util.fi' und '/b/util.fi'\n"
        );
    }
}
