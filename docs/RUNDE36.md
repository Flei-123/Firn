# Runde 36 — ct-Intrinsics, errdefer, must_consume (die letzten drei Kernbloecke)

Stand nach Runde 36: selbst_vergleich **185 gleich / 0 abweichend / 0 fehlerhaft**,
test.sh **640/640**, Fixpunkt **zeichengleich (284207 Zeilen Assembler)**.

## 1. Konstante Laufzeit / ct-Intrinsics (Vorbild compiler/src/ct.rs)

Drei Intrinsics in firnc1 portiert — Parser-Kernregistrierung (parser.fi,
`erweiterungen_suchen`: nur Kern, sobald die gc-Registrierung angemeldet ist,
wie bei `barrier` in Runde 34), Sema (sema.fi, eigene Erkennung vor dem
Funktions-Lookup — eine eigene Funktion gleichen Namens gewinnt), Lowering
(lower.fi, eigene Fir-Terme) und Codegen (codegen.fi).

- `select(bedingung, a, b)` — datenunabhaengige Auswahl. Sema: genau 3
  Argumente, Bedingung `bool`, nur skalare Typen (int/bool/Zeiger), beide
  Zweige exakt denselben Typs, keine implizite Umwandlung. Codegen: cmov-
  Muster, keine Spruenge.
- `secure_zero(zeiger, anzahl)` — nullt den Puffer, darf nie wegoptimiert
  werden (SPEC §9.3, C3). Sema: Zeiger + Ganzzahl. Codegen: Schleife mit
  volatile-Store-Semantik.
- `select` mit Zeigertypen (431) und `secure_zero` (433) decken die
  zusaetzlichen Faelle ab.

Tests: tests/430_ct_select.fi, 431_ct_select_zeiger.fi, 432_ct_barrier.fi
(Vervollstaendigung), 433_ct_secure_zero.fi, Kern-Test tests/780_ct_kern.fi.
Negativtests (rc=1 beidseitig): tests/neg/ct_select_bedingung.fi,
ct_select_stellenzahl.fi, ct_select_typen_verschieden.fi,
ct_secure_zero_kein_zeiger.fi, ct_barrier_aggregat.fi.

## 2. errdefer (Vorbild Stufe 0, defer.fi als firnc1-Vorbild)

`errdefer` laeuft nur, wenn der Block mit einem Fehler verlassen wird.
Umsetzung: `defer_bis_fehler` im Parser/Sema, `ret_term_fehler` im Lowering —
die Defer-Kette wird am fehlerhaften Return-Punkt in umgekehrter
Reihenfolge abgearbeitet, am Erfolgspfad nicht. Fertige-Union-Weitergabe
wird korrekt abgelehnt (tests/neg/errdefer_union_weitergabe.fi, rc=1).
Commit 3144601.

## 3. #[must_consume] (Vorbild compiler/src/attrs.rs)

Attribut auf Funktionen und Structs: Werden Aufruf-Ergebnisse (fn) bzw.
Werte des Typs (struct) verworfen, bricht der Compiler ab
(`check_discard` in sema.fi). Negativtests attr_must_consume_* rc=1.
Commit 6ef2616.

## Messwerte

| Werkzeug | vorher (Runde 34) | nachher |
|---|---|---|
| tools/selbst_vergleich.sh | 180 gleich | **185 gleich, 0 abw., 0 fehlerhaft, NICHT KERN 0** |
| test.sh | 637/637 | **640/640** |
| tools/fixpunkt.sh | 279201 Zeilen | **284207 Zeilen, zeichengleich** |

## Grenzen (ehrlich benannt)

- selbst_vergleich COMPTIME: 1 — 600_comptime.fi (rc=4) bleibt der einzige
  benannte Restfall; die comptime-Maschine aus Runde 35 deckt den vollen
  Stufe-0-Sprachschatz dort noch nicht ab.
- fir_vergleich: 1 ungleich (bekannt und benannt aus Vor-Runden).
- UEBERSPRUNGEN: 15 Dateien, die firnc0 nicht einzeln uebersetzt
  (unveraendert, kein Rueckschritt).
