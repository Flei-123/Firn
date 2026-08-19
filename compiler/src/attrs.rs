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
pub enum Target {
    /// nur vor `fn`
    Func,
    /// nur vor `struct` (und spaeter `enum`)
    Type,
    /// vor beidem
    Both,
}

impl Target {
    fn text(self) -> &'static str {
        match self {
            Target::Func => "fn",
            Target::Type => "struct",
            Target::Both => "fn, struct",
        }
    }
    fn allowed_fn(self) -> bool {
        matches!(self, Target::Func | Target::Both)
    }
    fn allowed_ty(self) -> bool {
        matches!(self, Target::Type | Target::Both)
    }
}

pub struct AttrInfo {
    pub name: &'static str,
    pub target: Target,
    /// Anzahl erwarteter Argumente in Klammern (0 = ohne Klammern).
    pub args: usize,
    /// Tut es in Stufe 0 wirklich etwas?
    pub implemented: bool,
    pub what: &'static str,
}

/// Alle Attribute, die die Sprache kennt.
pub const ATTRS: &[AttrInfo] = &[
    AttrInfo {
        name: "must_consume",
        target: Target::Both,
        args: 0,
        implemented: true,
        what: "Ergebnis darf nicht verworfen werden (SPEC 3.3, 5.1)",
    },
    AttrInfo {
        name: "no_gc",
        target: Target::Func,
        args: 0,
        implemented: true,
        what: "kein Sammellauf in diesem Aufrufbaum (SPEC 3.5.4)",
    },
    AttrInfo {
        name: "interrupt",
        target: Target::Func,
        args: 0,
        implemented: true,
        what: "unterbrechungs-einsprungpunkt: alle register retten, iretq (SPEC 2)",
    },
    AttrInfo {
        name: "allow_fp",
        target: Target::Both,
        args: 0,
        implemented: true,
        what: "gleitkomma im profil 'kernel' erlauben, FPU-Zustand (SPEC 2)",
    },
    AttrInfo {
        name: "constant_time",
        target: Target::Func,
        args: 0,
        implemented: false,
        what: "kein Sprung auf Geheimnisdaten, im Codegen geprueft (SPEC 9.2)",
    },
    AttrInfo {
        name: "unwinds",
        target: Target::Func,
        args: 0,
        implemented: false,
        what: "darf 'throw' ausloesen oder durchlassen (SPEC 5.3)",
    },
    AttrInfo {
        name: "packed",
        target: Target::Type,
        args: 0,
        implemented: false,
        what: "Felder ohne Auffuellbytes anordnen (SPEC 13)",
    },
    AttrInfo {
        name: "align",
        target: Target::Type,
        args: 1,
        implemented: false,
        what: "Ausrichtung erzwingen, z. B. #[align(64)] (SPEC 13)",
    },
    AttrInfo {
        name: "layout",
        target: Target::Type,
        args: 1,
        implemented: false,
        what: "Anordnung waehlen, z. B. #[layout(soa)] (DESIGNZIELE 8)",
    },
    AttrInfo {
        name: "no_move",
        target: Target::Type,
        args: 0,
        implemented: false,
        what: "nach dem Aufbau nicht mehr verschiebbar (DESIGNZIELE 6)",
    },
    AttrInfo {
        name: "abi_stable",
        target: Target::Both,
        args: 1,
        implemented: false,
        what: "stabiles ABI ueber Komponentengrenzen (DESIGNZIELE 4)",
    },
    AttrInfo {
        name: "frozen",
        target: Target::Type,
        args: 0,
        implemented: false,
        what: "Layout eingefroren, dafuer wieder einbettbar (DESIGNZIELE 4)",
    },
    AttrInfo {
        name: "hot",
        target: Target::Func,
        args: 0,
        implemented: false,
        what: "zur Laufzeit austauschbar (DESIGNZIELE 9, ohne Termin)",
    },
];

pub fn search(name: &str) -> Option<&'static AttrInfo> {
    ATTRS.iter().find(|a| a.name == name)
}

/// Passt das Attribut auf dieses Ziel?
pub fn fits(a: &AttrInfo, on_func: bool) -> bool {
    if on_func {
        a.target.allowed_fn()
    } else {
        a.target.allowed_ty()
    }
}

/// Naechstliegender bekannter Name (Levenshtein-Abstand <= 3), fuer Vorschlaege.
pub fn proposal(name: &str) -> Option<&'static str> {
    let mut best: Option<(usize, &'static str)> = None;
    for a in ATTRS {
        let d = distance(name, a.name);
        if d <= 3 && best.map(|(bd, _)| d < bd).unwrap_or(true) {
            best = Some((d, a.name));
        }
    }
    best.map(|(_, n)| n)
}

fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut line: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut before = line[0];
        line[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let replace = before + usize::from(ca != cb);
            before = line[j + 1];
            line[j + 1] = replace.min(line[j] + 1).min(line[j + 1] + 1);
        }
    }
    line[b.len()]
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
            a.target.text(),
            a.args,
            if a.implemented { "umgesetzt" } else { "Fehler" },
            a.what
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
    fn register_is_unique() {
        for (i, a) in ATTRS.iter().enumerate() {
            assert!(
                ATTRS.iter().skip(i + 1).all(|b| b.name != a.name),
                "doppelter Attributname: {}",
                a.name
            );
        }
    }

    #[test]
    fn only_must_consume_is_implemented() {
        // Wird ein weiteres Attribut umgesetzt, MUSS dieser Test angepasst
        // werden — das erzwingt, dass README und SPEC mitgezogen werden.
        // Stand Runde „Haertetest 2": zusaetzlich #[no_gc] (SPEC 3.5.4,
        // geprueft in nogc.rs, Testprogramme tests/54x_no_gc_*.fi und
        // tests/neg/nogc_*.fi).
        // Runde 52: dazu #[interrupt] und #[allow_fp] (SPEC 2, core.rs/prof.rs).
        let u: Vec<&str> = ATTRS.iter().filter(|a| a.implemented).map(|a| a.name).collect();
        assert_eq!(u, vec!["must_consume", "no_gc", "interrupt", "allow_fp"]);
    }

    #[test]
    fn not_implemented_attribute_report_next_a_error() {
        // Gegenprobe zu tests/neg/attr_not_implemented.fi: die uebrigen
        // Attribute bleiben abgelehnt, nichts wird still ignoriert.
        for name in ["constant_time", "unwinds", "packed", "align", "layout", "no_move", "hot"] {
            let a = search(name).expect(name);
            assert!(!a.implemented, "{} gilt unerwartet als umgesetzt", name);
        }
    }

    #[test]
    fn proposal_finds_typo() {
        assert_eq!(proposal("must_consum"), Some("must_consume"));
        assert_eq!(proposal("no_gk"), Some("no_gc"));
        assert_eq!(proposal("voellig_anders_xyz"), None);
    }

    #[test]
    fn targets_match() {
        let mc = search("must_consume").expect("must_consume");
        assert!(fits(mc, true) && fits(mc, false));
        let p = search("packed").expect("packed");
        assert!(!fits(p, true) && fits(p, false));
        let ng = search("no_gc").expect("no_gc");
        assert!(fits(ng, true) && !fits(ng, false));
    }
}
