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
`tools/self_compare.sh` exportieren `FIRNLIB=<repo>/lib`.

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

`tests/790_std_core.fi` fasst jede Fassade einmal an und laeuft auf beiden
Compilern zur selben Ausgabe (selbst-Vergleich: `GLEICH`).

## Teil B — `f"..."`: die String-Interpolation

`f"x = {x}"` ist jetzt Firn. Der Parser zerlegt den Rumpf **zur
Uebersetzungszeit** in eine Aufrufkette auf den `Fmt`-Builder aus
`std.io` — keine Varargs, kein Laufzeit-Parsen:

```text
f"x = {x}!"  ==>
io.fmt_text(                       // "x = " als verstecktes let _fsegN
    io.fmt_zahl(                   // (x) as i64
        io.fmt_text(io.fmt_neu(), &_fseg0[0] as u64, 4),
        (x) as i64),
    &_fseg1[0] as u64, 1)          // "!"
```

Der Wert eines `f"..."` ist ein `std.io.Fmt` — gedruckt wird er mit
`io.fmt_druck(...)` (oder weiterverwendet: `fhelfer.meldung(7)` gibt
einen `Fmt` zurueck). Wer `f"..."` schreibt, braucht `import std.io`;
ohne den Import meldet die Aufloesung `io` wie bei jedem anderen
Modulzugriff.

### Wie es gebaut ist (beide Compiler gleich)

* **Lexer**: `f"..."` ist EIN Token mit dem ROHEN Rumpf (Klammern und
  Maskierungen unangetastet). firnc0: `strings.rs::lex_fstring_literal`
  neben `lex_string_literal`; firnc1: `L_FSTR` in `lex_text` — die
  Literaltafel traegt dort die rohen Oktette.
* **Parser**: der Klammer-Scan teilt den Rumpf in Text- und
  Ausdruckssegmente. Textsegmente werden per `decode_literal`
  (firnc0) bzw. per Neu-Lexen als `"..."`-Literal (firnc1 — dieselbe
  Entschluesselung per Bauart) dekodiert. Ausdruckssegmente werden mit
  aufgefuellten Positionen neu gelext und von EINEM Unter-Parser als
  genau ein Ausdruck gelesen — Fehler zeigen auf die echte Stelle in
  der Datei (`tests/neg/interp_unknown_name.fi`: 5:27, mitten im
  `f"..."`).
* **Das Hoist-Problem**: `(&_fseg[0])` braucht eine benannte Variable —
  ein nacktes `(&[104, 105][0])` hat keinen ableitbaren Typ und ist
  nicht adressierbar (gemessen, nicht geraten). Darum hebt der Parser
  jedes Textsegment als verstecktes `let _fsegN: [u8; N]` vor die
  umgebende Anweisung (`hoist`-Liste, `block` leert sie nach jeder
  Anweisung). In firnc0 heisst das Segment `_fseg<ExprId>`, in firnc1
  `_fseg<Literal>_<Segment>` — Namen sind Implementierungsdetail,
  das Verhalten ist identisch.

### Ehrliche Grenzen (Kernfassung)

* **Die Anzeige ist die von `i64`.** Der Parser kennt den Typ des
  Ausdrucks nicht — er setzt `(ausdruck) as i64` ein. Ganzzahlen
  stimmen immer; `u64` oberhalb von i64::MAX bricht um, `f64`
  schneidet ab (`{2.9}` → `2`), `bool` wird `0`/`1`.
* **Keine Schachtelung**: ein `f"..."` im Ausdruck eines `f"..."` ist
  ein Fehler (Schachteltiefe 1). In der Praxis endet ein `"` im
  Ausdruck ohnehin die aeussere Zeichenkette.
* **Keine Maskierung der Klammern**: `{{`/`}}` gibt es (noch) nicht;
  eine einzelne `}` ohne `{` ist ein Fehler
  (`tests/neg/interp_paren_alone.fi`), ebenso `{` ohne `}`
  (`tests/neg/interp_paren_open.fi`) und `{` im Ausdruck.
* **Keine Zeichenketten im Ausdruck** (das `"` beendet das aeussere
  Literal) und keine Interpolation auf Item-Ebene (`const`) — es gibt
  keine Anweisung, vor die die Textsegmente gehoben werden koennten.
* **Bootstrap-Reihenfolge**: `firnc1` selbst benutzt `f"..."` noch
  nicht — der Lexer/Parser dafuer ist geschrieben, bevor das Feature
  existiert. Eine Umstellung einer kleinen Stelle ist nach bestandenem
  Fixpunkt moeglich (siehe unten).

### Nachweis

* `tests/791_interpolation_core.fi`: mehrere Segmente, Operatoren und
  Aufrufe in `{...}`, i32/u64/u8/bool, i64::MIN+1, und eine
  Interpolation **in einem importierten Modul**
  (`tests/modules/fhelper.fi`). Laeuft in beiden Compilern zur selben
  Ausgabe (selbst-Vergleich: `GLEICH`).
* Negativtests (`tests/neg/interp_*.fi`): unbekannter Name in `{...}`,
  `{` ohne `}`, `}` ohne `{` — jeweils rc=1 auf beiden Seiten.

### Zwei Befunde aus dem Bau, der Vollstaendigkeit halber

* **Die Parser-Kopie fuer Ausdruckssegmente muss den Baum ueber `fremd`
  teilen.** Im Wurzelbetrieb der Dump-Werkzeuge (`fremd == 0`) zeigt
  `par_baum` auf das Struct-Feld der Kopie — Knoten landeten im
  Stapelrahmen und waren nach der Rueckkehr tot (Segfault in `astdump`
  auf `f"{x}"`; im Modulbetrieb von `firnc1` unsichtbar, weil dort
  `fremd` ohnehin gesetzt ist). Die Kopie setzt darum
  `sub.fremd = par_baum(p)`.
* **Die versteckten Namen muessen zwischen den Compilern UEBEREINSTIMMEN.**
  `tools/parser_compare.sh` vergleicht die kanonischen Baeume
  Oktett fuer Oktett: `_fseg<ExprId>` (firnc0) und `_fseg<Knotenstand>`
  (firnc1) sind dieselbe Zahl, weil beide Parser Knoten in derselben
  Reihenfolge erzeugen — sonst waere `tests/791` dort eine Abweichung.

### Warum `firnc1` selbst noch kein `f"..."` benutzt

Der Fixpunkt steht; die optionale Umstellung einer kleinen Stelle wurde
bewusst NICHT gemacht, aus drei messbaren Gruenden: (1) die Desugar
ruft `io.fmt_*` — `firnc1` muesste `std.io` einbinden, und
`tools/fixpoint.sh` muesste `FIRNLIB` exportieren (die Dumps loesen
`std.io` sonst nicht auf); (2) die Kern-Fassade kennt nur
`fmt_zahl`/`fmt_text` — die Stellen, die sich anbieten (Lexer- und
Parser-Meldungen), brauchen Zeichen (`{c}` als Buchstabe, nicht als
Dezimalzahl) oder einen Puffer statt stdout; sonst aendert sich der
Meldungstext und `tools/lex_compare.sh` (vergleicht auch die
Fehlerstroeme) kippt; (3) der Gewinn waere kosmetisch. Was sie
freischaltet: `fmt_zeichen` + `fmt_inhalt(f, &buf)` in `std.io` und
`FIRNLIB` in `fixpunkt.sh`.

### Messwerte (Endstand Runde 39)

* `test.sh`: **649/649** (640 + 790×3 + 791×3 + 3 Interpolations-Negativtests)
* `tools/self_compare.sh`: **188/0/0** (790 und 791 sind `GLEICH`)
* Fixpunkt: Stufe 2 == Stufe 3, **289 096 Zeilen**, zeichengleich
* `tools/lex_compare.sh`, `parser_compare.sh`, `types_compare.sh`:
  gruen; die Interpolation ist in allen drei Stroemen identisch
  (Token, kanonischer Baum, Layout)
* `import std.math` aus `/tmp/firnproj`: FIRNLIB und Installationslayout
  (`bin/firnc` + `lib/`), auf beiden Compilern
