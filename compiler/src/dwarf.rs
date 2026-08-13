//! DWARF-Grundlagen: Zeilennummern (`.debug_line`) fuer den Debugger.
//!
//! Die FIR traegt keine Quellpositionen (`fir.rs` ist eingefroren). Deshalb
//! sammelt das Lowering die Zuordnung *Instruktion -> Quellzeile* hier in einer
//! Tabelle, die der Codegenerator beim Schreiben des Assemblers abfragt. Die
//! Zeilennummern werden als `.file`/`.loc`-Direktiven ausgegeben; `as` erzeugt
//! daraus die Abschnitte `.debug_line`, `.debug_info` und `.debug_abbrev`.
//!
//! Genauigkeit:
//!   * **immer**: Zeile der `fn`-Deklaration (Haltepunkt auf eine Funktion
//!     zeigt die richtige `.fi`-Datei und -Zeile)
//!   * **ohne Optimierer**: zusaetzlich anweisungsgenaue Zeilen. Mit
//!     Optimierer werden sie unterdrueckt, weil der Optimierer Instruktionen
//!     entfernt, verschiebt und Bloecke neu nummeriert — falsche Zeilen waeren
//!     schlimmer als keine.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
struct FuncLines {
    /// Position der `fn`-Zeile: (Dateinummer, Zeile)
    decl: Option<(u32, u32)>,
    /// (Block, Index der Instruktion im Block) -> (Dateinummer, Zeile)
    notes: HashMap<(u32, u32), (u32, u32)>,
}

#[derive(Default)]
struct Table {
    /// Quelldateien in der Reihenfolge ihrer Nummern (0-basiert).
    files: Vec<String>,
    funcs: HashMap<String, FuncLines>,
    /// Anweisungsgenaue Zeilen ausgeben?
    statements: bool,
}

static TABLE: Mutex<Option<Table>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Table) -> R) -> R {
    // Ein vergifteter Mutex ist nur nach einer Panik moeglich; dann wird der
    // innere Wert weiterbenutzt, statt eine zweite Panik auszuloesen.
    let mut guard = match TABLE.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    let t = guard.get_or_insert_with(Table::default);
    f(t)
}

/// Setzt die Tabelle zurueck und traegt die Quelldateien ein.
pub fn reset(files: Vec<String>, statements: bool) {
    with(|t| {
        t.files = files.clone();
        t.funcs.clear();
        t.statements = statements;
    });
}

/// Quelldateien in der Reihenfolge ihrer Nummern; leer = keine Debuginfo.
pub fn files() -> Vec<String> {
    with(|t| t.files.clone())
}

/// Position der `fn`-Deklaration merken.
pub fn set_fn(name: &str, file: u32, line: u32) {
    if line == 0 {
        return;
    }
    with(|t| {
        t.funcs.entry(name.to_string()).or_default().decl = Some((file, line));
    });
}

/// Quellzeile der Instruktion `idx` in Block `block` merken.
pub fn note(name: &str, block: u32, idx: u32, file: u32, line: u32) {
    if line == 0 {
        return;
    }
    with(|t| {
        if !t.statements {
            return;
        }
        t.funcs
            .entry(name.to_string())
            .or_default()
            .notes
            .entry((block, idx))
            .or_insert((file, line));
    });
}

/// Eine `alloca` wurde in Block `block` an Position `at` EINGEFUEGT: alle
/// Vermerke ab dieser Position rutschen um eins nach hinten.
pub fn shift_after_insert(name: &str, block: u32, at: u32) {
    with(|t| {
        if !t.statements {
            return;
        }
        if let Some(f) = t.funcs.get_mut(name) {
            let old = std::mem::take(&mut f.notes);
            for ((b, i), v) in old {
                let i2 = if b == block && i >= at { i + 1 } else { i };
                f.notes.insert((b, i2), v);
            }
        }
    });
}

/// Zeile der `fn`-Deklaration.
pub fn fn_line(name: &str) -> Option<(u32, u32)> {
    with(|t| t.funcs.get(name).and_then(|f| f.decl))
}

/// Zeile der Instruktion `idx` in Block `block`, sofern vermerkt.
pub fn line_at(name: &str, block: u32, idx: u32) -> Option<(u32, u32)> {
    with(|t| {
        t.funcs
            .get(name)
            .and_then(|f| f.notes.get(&(block, idx)).copied())
    })
}

/// `.file`-Direktiven fuer alle Quelldateien (Nummern sind 1-basiert).
pub fn file_directives() -> String {
    let mut out = String::new();
    for (i, f) in files().iter().enumerate() {
        out.push_str(&format!(".file {} \"{}\"\n", i + 1, f.replace('"', "\\\"")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Die Tabelle ist globaler Zustand — deshalb EIN Test, der beides prueft
    /// (parallele Tests wuerden sich sonst gegenseitig zuruecksetzen).
    #[test]
    fn vermerke_verschieben_sich_und_lassen_sich_abschalten() {
        reset(vec!["a.fi".to_string()], true);
        set_fn("f", 0, 3);
        note("f", 0, 2, 0, 10);
        shift_after_insert("f", 0, 1);
        assert_eq!(line_at("f", 0, 3), Some((0, 10)));
        assert_eq!(line_at("f", 0, 2), None);
        assert_eq!(fn_line("f"), Some((0, 3)));
        assert!(file_directives().contains(".file 1 \"a.fi\""));

        reset(vec!["a.fi".to_string()], false);
        note("g", 0, 0, 0, 7);
        assert_eq!(line_at("g", 0, 0), None);
        reset(Vec::new(), false);
    }
}
