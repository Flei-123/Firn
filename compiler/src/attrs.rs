//! **Attributregister** — die einzige Wahrheit darueber, welche Attribute es
//! gibt, wo sie stehen duerfen und welche davon in Stufe 0 wirklich etwas tun.
//!
//! Firns Spezifikation stuetzt sich an vielen Stellen auf Attribute:
//! `#[must_consume]` (SPEC §3.3, §5.1), `#[no_gc]` (§3.5.4),
//! `#[constant_time]` (§9.2), `#[unwinds]` (§5.3), `#[packed]`/`#[align(n)]`
//! (§13), `#[layout(soa)]` (DESIGNZIELE §8), `#[abi_stable]`/`#[frozen]`
//! (DESIGNZIELE §4), `#[hot]` (DESIGNZIELE §9).
//!
//! Sie kommen zu sehr verschiedenen Zeitpunkten. Damit das nicht in einem
//! Wildwuchs aus verstreuten Zeichenkettenvergleichen endet, stehen sie **hier**
//! in einer Tabelle — mit Ziel, Umsetzungsstand und Zweck. `--list-attrs` gibt
//! sie aus.
//!
//! Regel des Projekts: Was nicht umgesetzt ist, meldet einen **sauberen
//! Compilerfehler** mit Zeile und Spalte — niemals einen Absturz und niemals
//! stillschweigendes Ignorieren. Ein ignoriertes `#[constant_time]` waere die
//! gefaehrlichste Sorte Fehler, die es in dieser Sprache geben kann.

/// Worauf ein Attribut geschrieben werden darf.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ziel {
    /// nur vor `fn`
    Funktion,
    /// nur vor `struct` (und spaeter `enum`)
    Typ,
    /// vor beidem
    Beides,
}

impl Ziel {
    fn text(self) -> &'static str {
        match self {
            Ziel::Funktion => "fn",
            Ziel::Typ => "struct",
            Ziel::Beides => "fn, struct",
        }
    }
    fn erlaubt_fn(self) -> bool {
        matches!(self, Ziel::Funktion | Ziel::Beides)
    }
    fn erlaubt_typ(self) -> bool {
        matches!(self, Ziel::Typ | Ziel::Beides)
    }
}

pub struct AttrInfo {
    pub name: &'static str,
    pub ziel: Ziel,
    /// Anzahl erwarteter Argumente in Klammern (0 = ohne Klammern).
    pub args: usize,
    /// Tut es in Stufe 0 wirklich etwas?
    pub umgesetzt: bool,
    pub was: &'static str,
}

/// Alle Attribute, die die Sprache kennt.
pub const ATTRS: &[AttrInfo] = &[
    AttrInfo {
        name: "must_consume",
        ziel: Ziel::Beides,
        args: 0,
        umgesetzt: true,
        was: "Ergebnis darf nicht verworfen werden (SPEC 3.3, 5.1)",
    },
    AttrInfo {
        name: "no_gc",
        ziel: Ziel::Funktion,
        args: 0,
        umgesetzt: false,
        was: "kein Sammellauf in diesem Aufrufbaum (SPEC 3.5.4)",
    },
    AttrInfo {
        name: "constant_time",
        ziel: Ziel::Funktion,
        args: 0,
        umgesetzt: false,
        was: "kein Sprung auf Geheimnisdaten, im Codegen geprueft (SPEC 9.2)",
    },
    AttrInfo {
        name: "unwinds",
        ziel: Ziel::Funktion,
        args: 0,
        umgesetzt: false,
        was: "darf 'throw' ausloesen oder durchlassen (SPEC 5.3)",
    },
    AttrInfo {
        name: "packed",
        ziel: Ziel::Typ,
        args: 0,
        umgesetzt: false,
        was: "Felder ohne Auffuellbytes anordnen (SPEC 13)",
    },
    AttrInfo {
        name: "align",
        ziel: Ziel::Typ,
        args: 1,
        umgesetzt: false,
        was: "Ausrichtung erzwingen, z. B. #[align(64)] (SPEC 13)",
    },
    AttrInfo {
        name: "layout",
        ziel: Ziel::Typ,
        args: 1,
        umgesetzt: false,
        was: "Anordnung waehlen, z. B. #[layout(soa)] (DESIGNZIELE 8)",
    },
    AttrInfo {
        name: "no_move",
        ziel: Ziel::Typ,
        args: 0,
        umgesetzt: false,
        was: "nach dem Aufbau nicht mehr verschiebbar (DESIGNZIELE 6)",
    },
    AttrInfo {
        name: "abi_stable",
        ziel: Ziel::Beides,
        args: 1,
        umgesetzt: false,
        was: "stabiles ABI ueber Komponentengrenzen (DESIGNZIELE 4)",
    },
    AttrInfo {
        name: "frozen",
        ziel: Ziel::Typ,
        args: 0,
        umgesetzt: false,
        was: "Layout eingefroren, dafuer wieder einbettbar (DESIGNZIELE 4)",
    },
    AttrInfo {
        name: "hot",
        ziel: Ziel::Funktion,
        args: 0,
        umgesetzt: false,
        was: "zur Laufzeit austauschbar (DESIGNZIELE 9, ohne Termin)",
    },
];

pub fn suche(name: &str) -> Option<&'static AttrInfo> {
    ATTRS.iter().find(|a| a.name == name)
}

/// Passt das Attribut auf dieses Ziel?
pub fn passt(a: &AttrInfo, auf_funktion: bool) -> bool {
    if auf_funktion {
        a.ziel.erlaubt_fn()
    } else {
        a.ziel.erlaubt_typ()
    }
}

/// Naechstliegender bekannter Name (Levenshtein-Abstand <= 3), fuer Vorschlaege.
pub fn vorschlag(name: &str) -> Option<&'static str> {
    let mut beste: Option<(usize, &'static str)> = None;
    for a in ATTRS {
        let d = abstand(name, a.name);
        if d <= 3 && beste.map(|(bd, _)| d < bd).unwrap_or(true) {
            beste = Some((d, a.name));
        }
    }
    beste.map(|(_, n)| n)
}

fn abstand(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut zeile: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut vorher = zeile[0];
        zeile[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let ersetzen = vorher + usize::from(ca != cb);
            vorher = zeile[j + 1];
            zeile[j + 1] = ersetzen.min(zeile[j] + 1).min(zeile[j + 1] + 1);
        }
    }
    zeile[b.len()]
}

/// Register als Text (fuer `--list-attrs`).
pub fn attrs_text() -> String {
    let mut out = String::new();
    out.push_str("Attribute\n\n");
    out.push_str("NAME            ZIEL         ARGS  STUFE 0     ZWECK\n");
    for a in ATTRS {
        out.push_str(&format!(
            "{:<15} {:<12} {:<5} {:<11} {}\n",
            a.name,
            a.ziel.text(),
            a.args,
            if a.umgesetzt { "umgesetzt" } else { "Fehler" },
            a.was
        ));
    }
    out.push_str(
        "\n'Fehler' heisst: das Attribut ist bekannt und geplant, wird aber in\n",
    );
    out.push_str(
        "Stufe 0 mit einer klaren Meldung abgelehnt statt still ignoriert.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_ist_eindeutig() {
        for (i, a) in ATTRS.iter().enumerate() {
            assert!(
                ATTRS.iter().skip(i + 1).all(|b| b.name != a.name),
                "doppelter Attributname: {}",
                a.name
            );
        }
    }

    #[test]
    fn nur_must_consume_ist_umgesetzt() {
        // Wird ein weiteres Attribut umgesetzt, MUSS dieser Test angepasst
        // werden — das erzwingt, dass README und SPEC mitgezogen werden.
        let u: Vec<&str> = ATTRS.iter().filter(|a| a.umgesetzt).map(|a| a.name).collect();
        assert_eq!(u, vec!["must_consume"]);
    }

    #[test]
    fn vorschlag_findet_tippfehler() {
        assert_eq!(vorschlag("must_consum"), Some("must_consume"));
        assert_eq!(vorschlag("no_gk"), Some("no_gc"));
        assert_eq!(vorschlag("voellig_anders_xyz"), None);
    }

    #[test]
    fn ziele_stimmen() {
        let mc = suche("must_consume").expect("must_consume");
        assert!(passt(mc, true) && passt(mc, false));
        let p = suche("packed").expect("packed");
        assert!(!passt(p, true) && passt(p, false));
        let ng = suche("no_gc").expect("no_gc");
        assert!(passt(ng, true) && !passt(ng, false));
    }
}
