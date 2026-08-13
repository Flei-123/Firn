//! Zentrale Konfiguration: Sprachname und Dateiendung stehen AUSSCHLIESSLICH hier.
//! Umbenennen der Sprache = diese drei Konstanten aendern, sonst nichts.

pub const LANG_NAME: &str = "Firn";
pub const LANG_NAME_LOWER: &str = "firn";
pub const FILE_EXT: &str = "fi";

/// Name des Compiler-Binaries, abgeleitet aus dem Sprachnamen.
pub fn compiler_name() -> String {
    format!("{}c", LANG_NAME_LOWER)
}

/// Version des Prototypen (Stufe 0).
pub const VERSION: &str = "0.1.0 (Stufe 0)";
