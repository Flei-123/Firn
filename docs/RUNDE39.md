# Runde 39: `lib/std/` — die Fassade, und `f"..."` — die String-Interpolation

Zwei Teile, zwei getrennte Commits (Limit-Lektion). Teil A baut keine
Sprache aus: ein Suchpfad und eine Bibliothek. Teil B baut das erste
Zucker-Feature: die Interpolation, aufgeloest zur Uebersetzungszeit.

## Teil A — der Import-Suchpfad und die std-Fassade

### Der Suchpfad (firnc0 und firnc1, dieselbe Reihenfolge)

`import a.b` sucht `a/b.fi` jetzt an bis zu vier Orten:

1. neben der **importierenden** Datei (bisherige Regel 1)
2. neben der **Wurzeldatei** (bisherige Regel 2)
3. in **`$FIRNLIB`** (Umgebungsvariable; leer/ungesetzt = Schritt entfaellt)
4. in **`<Verzeichnis des Compiler-Binarys>/../lib`** — das
   Installationslayout `bin/firnc` + `lib/`. `firnc0` liest dafuer
   `current_exe`; `firnc1` liest `/proc/self/exe` per `readlink`-Syscall.

Die neuen Orte kommen NACH den alten: bestehende Aufloesungen aendern sich
nicht (test.sh 640/640, selbst 186/0/0, Fixpunkt zeichengleich — vor dem
Bibliotheks-Commit nachgemessen). `firnc1` findet `envp` im Startblock
hinter `argv` (`umgebung()` in `bin/firnc1.fi`); `test.sh` und
`tools/selbst_vergleich.sh` exportieren `FIRNLIB=<repo>/lib`.

Nachweis aus einem fremden Verzeichnis (`/tmp/firnproj`): `import std.math`
uebersetzt und laeuft — per `FIRNLIB` und per Installationslayout
(`/tmp/firninst/{bin,lib}`, ohne Variable), auf beiden Compilern.

### Was in `lib/std/` steht

| Modul | Inhalt | Bauweise |
|---|---|---|
| `std.io` | `print`, `eprint` (stderr), `read_file`, `write_file`, der `Fmt`-Builder | Hand geschrieben ueber `import rt` |
| `std.math` | `PI()`, `E()`, `abs`, `min`, `max`, `clamp`, `isqrt`, `pow` (Ganzzahl) + `sqrt`, `powi` (f64) | Hand geschrieben, selbstgenuegsam |
| `std.str` | `Bytes`, `Str16`, UTF-8 (`utf8_push_cp`, `utf8_to_str16`, ...), `AtomTable` | **erzeugt** aus lib/str per `tools/strlib/expand.py` |
| `std.num` | `dtoa`, `strtod`, `bignum` (`Bn`) | **erzeugt** aus lib/num per expand.py |
| `std.vec` / `std.map` / `std.rt` / `std.intern` / `std.rc` | Re-export von lib/rt bzw. lib/rc | **Symlinks** |
| `std.mem` | `alloc`/`free` (roh, mmap) + `heap_*` (rc-Halde) | Hand geschrieben ueber `import rt` + `import rc` |

Drei ehrliche Bauentscheidungen:

* **lib/str und lib/num sind Einbindungs-Bibliotheken** (Stufe-0-Erbe):
  ihre Dateien verweisen sich textuell (`//#include`) und tragen keine
  Modulstruktur. Importieren kann man sie nicht — darum werden `std.str`
  und `std.num` mit dem bestehenden Werkzeug (`expand.py`, neue TARGETS)
  als je EIN Modul textuell zusammengesetzt und ins Repo geschrieben,
  genau wie die erzeugten Tests 300–308. lib/str und lib/num selbst bleiben
  unveraendert.
* **`std.vec`/`std.map` sind Symlinks, keine Huellen.** Eine Huell-Datei
  `lib/std/vec.fi` mit `import rt.vec` hiesse selbst `vec` und kollidierte
  mit dem Zielmodul gleichen Namens (Modulname = Dateistamm). Der Symlink
  ist der ehrliche Re-export: derselbe Inhalt, kanonisch sogar dieselbe
  Datei (`firnc0` dedupliziert kanonisiert; `firnc1` sieht durchweg
  dieselbe Pfadzeichenkette `lib/std/rt.fi`, weil alle std-internen
  `import rt` neben der Fassade aufloesen).
* **Generische Namen brauchen keine Fassade.** `Vec[T]`, `vec_neu`,
  `Map[K, V]`, `Zaehlverweis[T]`, `rc_neu` gelten in Firn programmweit
  (Runde 21: Vorlagen werden unter ihrem urspruenglichen Namen gesucht).
  Wer `std.mem` importiert, hat `rc` damit geladen und ruft `rc_neu[i64](..)`
  ohne jeden Qualifizierer. `std.mem` huellt darum nur die nicht-generische
  Halden-Verwaltung (`heap_init`, `heap_lebende`, ...) und das rohe
  `alloc`/`free`. Die GC-Laufzeit bleibt bewusst aussen vor: sie wird
  automatisch eingezogen, sobald `gc class` steht — ein Import waere
  weder noetig noch sichtbar.

Grenzen, benannt: `const` kann kein `f64` (Sema wertet Konstanten
ganzzahlig aus) — `PI`/`E` sind Funktionen. Eine leere Moduldatei ist ein
gueltiges Modul (Treffer steht im Rueckgabewert von `lies_datei`, nicht in
der Pufferlaenge). `firnc1` dedupliziert Pfade als Zeichenkette: wer
dieselbe Datei ueber zwei verschiedene Pfade einbindet (etwa `std.vec` UND
`rt.vec` mischen), laedt sie zweimal — `firnc0` kanonisiert, `firnc1`
(noch) nicht; nicht mischen.

`tests/790_std_kern.fi` fasst jede Fassade einmal an und laeuft auf beiden
Compilern zur selben Ausgabe (selbst-Vergleich: `GLEICH`).

## Teil B — `f"..."`: die String-Interpolation

*(wird mit dem Teil-B-Commit ergaenzt)*
