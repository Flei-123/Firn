# Zeichenreferenzen im HTML5-Tokenizer (`lib/html/entities.fi`)

Modul `tokenizer-text` aus `PLAN.md` §1. Setzt die Zeichenreferenz-Zustaende
des WHATWG-HTML-Standards (§13.2.5.72 – §13.2.5.80) **in Firn** um und liefert
die offizielle Namenstabelle mit **2.231** Eintraegen.

## Dateien

| Datei | Rolle |
|---|---|
| `lib/html/entities.fi` | Zustaende, Namenssuche, numerische Referenzen (Firn, ~330 Zeilen) |
| `lib/html/entities_data.fi` | **erzeugte** Namenstabelle als u64-Woerter (Firn, ~4.660 Zeilen) |
| `tools/tokenizer/gen_entities.py` | Erzeuger der Tabelle aus `html.entities.html5` |
| `lib/html/entities_probe.fi` | Pruefstand: fahre nur den Zeichenreferenz-Teil (Firn) |
| `tools/tokenizer/pruefe_entities.py` | Werkbank: Pruefstand gegen die html5lib-Daten |

## Schnittstelle (Vertrag mit `lib/html/tokenizer.fi`, PLAN.md §2.3)

```
fn char_ref(input: *mut mem.CpBuf, pos: usize, in_attr: bool,
            s: *mut tokens.Sink) -> usize
fn char_ref_out(input: *mut mem.CpBuf, pos: usize, in_attr: bool,
                s: *mut tokens.Sink, out: u32) -> usize
```

`pos` zeigt hinter das `&`. Rueckgabe ist die neue Position; ausgegeben wird
ueber `tokens.sink_emit_char` bzw. — bei `in_attr` — ueber
`tokens.tok_attr_value_push`. Der Aufruf kann nicht fehlschlagen: im
schlechtesten Fall wird das `&` selbst ausgegeben. Es gibt hier **keinen**
Zustand „nicht unterstuetzt".

`char_ref_out` ist dieselbe Funktion mit getrenntem Ausgabeschalter
(`out == 0` -> Zeichenstrom, sonst Attributwert), falls die Sonderregel des
Standards und das Ausgabeziel einmal auseinanderfallen sollen.

## Umgesetzte Regeln

* **Character reference state**: `#` -> numerisch, alphanumerisch -> Name,
  sonst nur `&` ausgeben.
* **Named character reference state**: laengster Treffer zuerst, Namen mit und
  ohne Semikolon (`&amp` genauso wie `&amp;`), Ersatz mit einem **oder zwei**
  Codepunkten (93 der 2.231 Eintraege haben zwei).
* **Sonderregel im Attributwert**: Name ohne Semikolon, gefolgt von `=` oder
  einem alphanumerischen Zeichen -> nicht ersetzen, Text unveraendert.
* **Ambiguous ampersand state**: kein Treffer -> `&` und die alphanumerische
  Folge unveraendert; `;` wird dem aufrufenden Zustand zurueckgegeben.
* **Numeric character reference**: dezimal und hexadezimal (`&#x…`/`&#X…`),
  fehlendes Semikolon erlaubt, fehlende Ziffern geben `&#`/`&#x` woertlich aus,
  Ueberlauf wird abgefangen (Zahlen > 0x10FFFF).
* **Numeric character reference end state**: `0`, Werte > 0x10FFFF und
  Surrogate werden zu U+FFFD; die C1-Ersetzungstabelle (0x80–0x9F, 27 Werte)
  ist vollstaendig umgesetzt.

## Namenstabelle: warum erzeugter Quelltext

Stufe 0 kennt weder Zeichenkettenliterale noch globale Felder (`const` nur
skalar, siehe SPEC §14.1). Die Tabelle wird deshalb als Folge von
u64-Woertern in einen Speicherbereich geschrieben:

```
0            u64   Kennung
OFF_NAMEN    u8[]  alle 2.231 Namen hintereinander (16.641 Byte), sortiert
OFF_LEN      u8[]  Laenge je Eintrag
OFF_WERT     u64[] Ersatz: cp1 | cp2 << 32   (cp2 == 0: nur ein Zeichen)
OFF_POS      u32[] Anfang je Name — beim ersten Zugriff berechnet
BYTES        u32[] Verzeichnis nach erstem Zeichen (256 × Anfang/Ende)
```

Der Bereich liegt an fester Adresse `0x6000_0000_0000` und wird beim ersten
Zugriff mit `mmap(MAP_FIXED_NOREPLACE)` angelegt; meldet der Kern `EEXIST`,
ist er schon da (die Kennung wird geprueft). Gesucht wird mit einer
Binaersuche innerhalb des Bereichs, den das Verzeichnis fuer das erste
Zeichen vorgibt; der Bewerber steht in einem Feld auf dem Stapel
(`[u32; 33]`) — eine Zeichenreferenz fordert **keinen** Speicher an.

Die Tabelle stammt aus `html.entities.html5` der Python-Standardbibliothek
(offizielle WHATWG-Liste), **nicht** aus den Testdaten:

```
python3 tools/tokenizer/gen_entities.py
```

## Nachweis

```
compiler/target/release/firnc -o .tokenizer-work/entities_probe lib/html/entities_probe.fi
python3 tools/tokenizer/pruefe_entities.py
```

Der Pruefstand nimmt aus den offiziellen html5lib-Daten alle Faelle, die sich
allein mit dem Zeichenreferenz-Teil entscheiden lassen (Data state, kein `<`,
Erwartung nur Character-Token) und vergleicht Zeichen fuer Zeichen:

```
Zeichenreferenzen (lib/html/entities.fi), reine Data-state-Faelle
  bestanden: 4657 / 4657
```

Das ist ein **Modulnachweis, keine Bilanz** — die verbindliche Zahl ueber alle
6.810 Faelle liefert allein `tools/tokenizer/run.sh`. Dort schlagen die
Zeichenreferenzen mit `namedEntities.test` 4210/4210, `numericEntities.test`
336/336 und `entities.test` 80/80 zu Buche (letzteres enthaelt auch die
Attributwert-Sonderfaelle, die der enge Pruefstand nicht abdeckt).

## Kosten, ehrlich

* Ein `mmap`-Aufruf je Zeichenreferenz (die Pruefung „liegt die Tabelle schon
  da?" ist ohne globale Variablen nicht ohne Systemaufruf zu haben). Auf dem
  Messrechner rund 1 µs; auf dem entity-lastigen Messkorpus (302.912 `&` in
  4,08 MB) sind das etwa 0,12 s von rund 0,9 s Gesamtzeit. Sobald Stufe 0
  globale Daten kennt, faellt das weg — bis dahin steht es hier.
* Die Suche selbst kostet rund 0,9 µs je Referenz (Binaersuche im
  Erstzeichen-Bereich).
* Ohne Zeichenreferenzen braucht derselbe Korpus rund 0,48 s, mit ihnen rund
  0,89 s CPU-Zeit. `html5ever` braucht fuer denselben Korpus 0,34 s — Faktor
  2,6x fuer den gesamten Firn-Tokenizer (Messung und Messart:
  `bench/tokenizer/README.md`).

## Offen

* Keine Zwischenspeicherung des Tabellenzeigers (siehe oben).
* `char_ref` meldet keine Parse-Fehler nach aussen; die html5lib-Bilanz
  vergleicht nur den Tokenstrom, `ParseError`-Eintraege werden im Harness
  ohnehin entfernt.
