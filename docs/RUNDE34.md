# Runde 34 — `gc class`, `Gc[T]`, schwache Referenzen, `#[no_gc]` in firnc1

## Ziel
Der groesste verbleibende Block der Kernsprache: der GC-Tramp (Vorbild
`compiler/src/gc.rs`, `nogc.rs`, ca. 2 250 Zeilen Rust). Die Laufzeit
`lib/gc/gc.fi` existiert schon (in Firn geschrieben) und wird automatisch
eingezogen, sobald irgendwo im Importgraphen `gc class` steht.

## Gebaut
- `lib/firnc1/gc.fi` (854 Z.): die Klassen-Registrierung — Praefixlayout bei
  `extends`, starke/schwache Feldversatztabellen, `Gc[C]`/`GcWeak[C]` als
  Typen, `gc C{...}` -> `AllocError!Gc[C]`, `x.as?[C]`, Verwandtschafts-
  pruefung fuer Identitaetsvergleiche.
- `lib/firnc1/gctext.fi` (323 Z.): die Laufzeit `lib/gc/gc.fi` als
  eingebetteter Quelltext; der Treiber haengt sie als Modul ohne Alias in
  denselben Baum (Wurzelnamensraum: `gc_init()` heisst ueberall `gc_init()`).
- `lib/firnc1/nogc.fi` (341 Z.): der transitive `#[no_gc]`-Pruefer —
  Regel (ii) aus SPEC §3.5.4 ueber Modulgrenzen hinweg.
- Hooks in parser (603/920/1195/2405), sema (Feldzugriff durch `Gc[T]`,
  `weak`/`stark`, `gc C{}`, Vergleiche), lower und codegen (Schreibbarrieren
  beim Feldzugriff, Allokation ueber die Laufzeit).

## Der Fund dieser Runde (wieder erst am Korpus sichtbar)
`bin/firnc1.fi` suchte die Woerter `gc`/`class`/`AllocError` per
`intern_finde` — die Nummern existieren nur, wenn die WURZELDATEI die
Woerter enthaelt. Stand `gc class` nur in einem importierten Modul
(tests/560 -> modules/dom.fi), lief der Scan mit -1 und fand nichts:
keine Laufzeit, keine `AllocError`-Menge, stiller Sema-Fehler.
Firnc0 hat dieselbe Stelle in `main.rs`, dort steht `intern_nummer`.
Jetzt `intern_nummer` — internen ist billig und idempotent.

## Messwerte
- `tools/self_compare.sh`: 169 -> **179** verhaltensgleich, 0 abweichend,
  0 fehlerhaft (alle neun gc-Dateien inklusive 510 Zyklus und 560 DOM-Zyklen)
- Fixpunkt: Stufe 2 == Stufe 3, zeichengleich, **279 201 Zeilen** Assembler
- Negativtests brechen wie firnc0 ab: nogc_transitiv, nogc_aufruf_ohne_attribut,
  nogc_modulgrenze, gc_klasse_auf_dem_stapel, gc_mehrfachvererbung,
  gc_as_nicht_verwandt (jeweils rc=1 auf beiden Seiten)
- Neu: `tests/770_gc_core.fi` + `tests/modules/kern/gccore.fi` — gc-Klasse
  nur im Modul, Zyklus ueberlebt unter Wurzel, Statistik geprueft
