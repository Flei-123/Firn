# Selbst-Hosting: Plan und ehrlicher Stand

**Anforderung:** `L1` · `SPEC.md` §11 (Bootstrap-Plan) · `ABNAHME.md` Punkt 1
**Stand (Runde 31): der Fixpunkt steht.** `firnc1` — der Compiler, geschrieben
in Firn — übersetzt **sich selbst**, und das Ergebnis ist ein Fixpunkt:
Stufe 2 (von `firnc1` erzeugt) und Stufe 3 (von Stufe 2 erzeugt) sind
**zeichengleich**. Nachweis: `tools/fixpunkt.sh`, Abschnitt 17 von `test.sh`.
Der Verlauf dorthin steht unten, Runde für Runde, mit Messwerten statt
Behauptungen — §21 ist der Schlussstein.

---

## 1. Der Plan (unverändert aus SPEC §11)

| Stufe | geschrieben in | übersetzt von | Ergebnis |
|---|---|---|---|
| 0 | Rust | `cargo` | `firnc0` — übersetzt die Teilmenge aus §12.1 |
| 1 | Firn (Teilmenge §12.1) | `firnc0` | `firnc1` |
| 2 | Firn (voller Umfang) | `firnc1` | `firnc2`, danach übersetzt `firnc2` sich selbst |
| 3 | — | — | Fixpunkt: `firnc2` und `firnc2'` bit-identisch |

Regel bleibt: **Stufe 1 darf nur benutzen, was `firnc0` beherrscht.** Der
Fixpunkt-Vergleich ist die einzige belastbare Korrektheitsaussage.

## 2. Reihenfolge, in der Stufe 1 entstehen müsste

1. **Laufzeitkern in Firn** (`lib/rt/`): `mmap`-Allokator (Bump + freie Listen),
   `memcpy`, `memset`, Prozessende. Nur `syscall` nötig — geht heute schon.
2. **Sammlungen** (`lib/std/`): `Vec[T]`, `Map[K,V]`, `Str`/`Bytes`.
   Braucht Generics (Modul `types`) und den Allokator aus 1.
3. **Ein-/Ausgabe**: `read`, `write`, `open`, `close`, `execve` als
   Firn-Hüllen um `syscall`. Geht heute schon, fehlt aber als Bibliothek.
4. **Compiler-Datenstrukturen**: Token, AST, FIR — rekursive Bäume über
   `Vec`-Indizes statt Zeigern (siehe 4.).
5. **Portierung in dieser Reihenfolge**: `config` → `diag` → `lexer` → `ast`
   → `parser` → `types` → `sema` → `fir` → `lower` → `codegen_x86` → `main`.
   Der Optimierer kommt zuletzt; `firnc1` darf ohne ihn arbeiten.

## 3. Was heute schon in Firn geschrieben werden **könnte**

Alles, was mit **fester** Größe auskommt und keinen Heap braucht. Konkret:

* Zahlen- und Bitalgorithmen: Ganzzahlparser, Basisumrechnung, `wrap`/`lit_fits`
  aus `sema.rs`, Ausrichtungs-/Layoutrechnung aus `types.rs` (`round_up`,
  `size_of`, `align_of`) — sie arbeiten auf Skalaren und festen Feldern.
* Tabellengetriebene Erkennung: Schlüsselwort-Tabelle des Lexers, die
  Operator-Erkennung (`punct`), die Registernamen-Tabelle des Codegenerators.
* Die gesamte **Klassifikation der Aufrufkonvention** (`abi.rs`): reine
  Fallunterscheidung auf Typ und Größe.
* Zustandsmaschinen ohne dynamische Speicherverwaltung — genau deshalb ist der
  HTML5-Tokenizer der richtige erste Härtetest und nicht der Compiler.

## 4. Was konkret fehlt — die Liste

Sortiert nach „blockiert am meisten zuerst". `[ ]` = fehlt,
`[~]` = teilweise (Modul dieser Runde), `[x]` = vorhanden.

| # | Merkmal | Stand | Warum der Compiler es braucht |
|---|---|---|---|
| 1 | **Heap-Allokator** (`mmap`-basiert, `alloc`/`free`) | **`[x]`** `lib/rt/rt.fi` (Runde 15) | Ohne ihn gibt es kein `Vec`, keinen AST, keine Symboltabelle |
| 2 | **`Vec[T]`** (wachsendes Feld) | **`[x]`** als Bibliothek `lib/rt/vec.fi` (Runde 18, `tests/640_vec_module.fi`) | Tokenstrom, Anweisungslisten, Blocklisten — überall |
| 3 | **Hash-Abbildung `Map[K,V]`** | **`[x]`** `lib/rt/map.fi` + `lib/rt/intern.fi` (Runde 19) | Namenstabellen (`fns`, `consts`, Bereiche) |
| 4 | **Zeichenketten** `Str`/`Bytes` mit Verkettung | `[~]` `rt.Buf` + `intern.Interner` (Runde 19); es fehlt ein `Str`-Typ mit Verkettungsoperator | Bezeichner, Fehlermeldungen, Assemblertext |
| 5 | **Textformatierung** (`format`-Ersatz) | **`[~]`** `buf_push_dez_u64/i64`, `buf_push_hex_u64` in `lib/rt/` | Jede Diagnose und der gesamte Assembler-Ausdruck |
| 6 | **Summentypen + `match`** | **`[x]`** ohne Typparameter (`tests/201_enum_payload.fi`); `enum Name[T]` fehlt (Runde 20 geprueft) | `TokKind`, `ExprKind`, `Op`, `Term` sind alle Summentypen |
| 7 | **Rekursive Datentypen** (`Box`-Ersatz) | **`[x]`** über `*mut` auf den eigenen Typ, in Runde 20 nachgeprüft | `Expr` enthält `Expr`; heute nur über Zeiger + Allokator |
| 8 | **Methoden / `impl`** | `[ ]` | Kosmetik, ersetzbar durch freie Funktionen mit erstem Parameter |
| 9 | **Schnittstellen / dynamischer Versand** | `[ ]` | Für Stufe 1 **nicht** nötig |
| 10 | **Fehlerbehandlung** (`Result`, `?`) | `[ ]` | Ersetzbar durch Summentyp + `match`, sobald 6 steht |
| 11 | **Prozessstart** (`fork`/`execve`-Hülle) | **`[x]`** seit Runde 28 (`rt.lauf`, `tests/700_prozessstart.fi`) | `firnc` ruft `as` und `ld` auf |
| 12 | **Dateizugriff** (`open`/`read`/`write`) | **`[x]`** `lies_datei`, `lies_stdin`, `schreib_alles` in `lib/rt/` | Quelle lesen, `.s` schreiben |
| 13 | **Veränderliche globale Zustände** | `[ ]` (nur `const`) | Umgehbar: Kontext-Struct durchreichen — der Rust-Code tut das schon fast überall |
| 14 | **Aggregate an Funktionsgrenzen** | `[x]` seit Runde 2 | Strukturen als Parameter/Rückgabe |
| 15 | **mehr als 6 Parameter** | `[x]` seit Runde 2 | `emit_inst(e, f, fr, i, …)` |
| 16 | **Modulsystem** | `[x]` seit Runde 2 | Der Compiler hat 24 Dateien |
| 17 | **`for`/`break`/`continue`** | `[x]` seit Runde 2 | Jede Schleife im Compiler |
| 18 | **`comptime`-Codeerzeugung** | `[ ]` | Nur für Unicode-Tabellen (`ABNAHME` Punkt 6), nicht für Stufe 1 |

## 5. Wie viel des Compilers wäre heute portierbar? — gemessen, nicht geschätzt

Messmethode (reproduzierbar):

```console
$ cd compiler/src
$ for f in *.rs; do
    tot=$(grep -c . $f)
    dyn=$(grep -cE "Vec<|String|HashMap|HashSet|Box<|format!|\.push\(|\.clone\(\)|&str" $f)
    echo "$f $tot $dyn"
  done
```

Gezählt wird, wie viele nichtleere Zeilen eine dynamische Datenstruktur oder
Textformatierung berühren — also genau das, was Firn heute **nicht** hat.

| Datei | Zeilen | davon mit `Vec`/`String`/`Map`/`Box`/`format!` |
|---|---:|---:|
| `abi.rs` | 113 | 1 |
| `types.rs` | 192 | 19 |
| `lexer.rs` | 514 | 26 |
| `dwarf.rs` | 138 | 16 |
| `codegen_x86.rs` | 569 | 72 |
| `parser.rs` | 1500 | 78 |
| `sema.rs` | 2302 | 153 |
| `lower.rs` | 1419 | 123 |
| … (alle 24 Dateien) | **16480** | **1600** |

Daraus folgt **nicht** „90 % sind portierbar". Die Abhängigkeit ist nicht
zeilen-, sondern strukturweise: eine einzige `Vec` in einer Funktion macht die
ganze Funktion unportierbar, und der Tokenstrom (`Vec<Token>`) zieht sich durch
Lexer, Parser und alle Tests.

**Ehrliche Bewertung, Datei für Datei:**

| Datei | heute portierbar? |
|---|---|
| `config.rs`, `abi.rs` | **ja, vollständig** (226 Zeilen) |
| `types.rs` | ja bis auf die `HashMap` der Strukturnamen — mit fester Obergrenze und linearer Suche portierbar (192 Zeilen) |
| `lexer.rs` | Erkennungslogik ja, Ausgabe `Vec<Token>` nein → braucht Punkt 1+2 |
| `diag.rs`, `parser.rs`, `sema.rs`, `lower.rs`, `opt.rs`, `codegen_x86.rs` | **nein**, alle brauchen Heap, `Vec` und Textformatierung |

**Zahl, die zählt:** heute vollständig in Firn schreibbar sind
`config.rs` + `abi.rs` + `types.rs` ≈ **418 von 16.480 Zeilen ≈ 2,5 %** des
Compilers. Mit Punkt 1–5 aus der Liste (Allokator, `Vec`, `Map`, `Str`,
Formatierung) wären es nach unserer Durchsicht **> 80 %** — diese fünf Punkte
sind der ganze Unterschied zwischen „geht nicht" und „geht".

## 6. Nächster Schritt

`lib/rt/alloc.fi` (Bump-Allokator über `mmap`) und `lib/std/vec.fi` als erste
Firn-Bibliotheken, beide mit eigenen Testprogrammen unter `tests/`. Erst danach
lohnt sich der erste Compilerteil in Firn — und der ist der **Lexer**, weil er
die kleinste Schnittstelle hat (Text hinein, Tokenfeld hinaus).

---

## 6. Was Runde 15 geliefert hat — `lib/rt/`

Die drei Punkte, die oben am stärksten blockierten (1, 5, 12), stehen jetzt als
**eine** Bibliothek in Firn: `lib/rt/rt.fi`.

| Bereich | Funktionen |
|---|---|
| Speicher | `heap_alloc`, `heap_free`, `mem_copy`, `mem_set`, `mem_eq` |
| Puffer | `Buf` mit `buf_push`, `buf_push_bytes`, `buf_reserve`, `buf_at`, `buf_len` |
| Zahl → Text | `buf_push_dez_u64`, `buf_push_dez_i64`, `buf_push_hex_u64` |
| Ein-/Ausgabe | `lies_datei`, `lies_stdin`, `schreib_alles`, `beende` |
| Rohzugriff | `ld8`/`st8` … `ld64`/`st64` |

Nachweis: `tests/610_rt.fi` — Allokation, 5.000 Byte durch mehrere
Verdopplungen, Formatierung in Dezimal und Hex, eine Datei lesen und wieder
ausgeben; in allen drei Baustufen.

**Warum das zählt:** ein Compiler muss seine Quelle lesen, Text aufbauen und
`.s` schreiben. Genau diese drei Dinge kann Firn jetzt ohne Rust und ohne libc.

**Ehrlich dazu:**

* Der Allokator gibt Speicher **seitenweise** an das Betriebssystem zurück und
  hat keine Freiliste für kleine Blöcke. Für einen Compilerlauf ist das in
  Ordnung (Arena-artig), für einen Dauerläufer nicht.
* Diese Bibliothek **ersetzt die vorhandenen noch nicht**. `lib/html/mem.fi`,
  `lib/str/alloc.fi` und `lib/gc/gc.fi` haben weiterhin ihre eigenen,
  leicht verschiedenen Fassungen. Das Zusammenführen ist ein eigener Schritt
  mit eigenem Risiko — es steht aus, und diese Zeile bleibt hier stehen, bis
  es erledigt ist.
* `Vec[T]` (typisiert, generisch) fehlt weiterhin. `rt.Buf` ist die
  Byte-Fassung davon.

---

## 7. Runde 16/17: `size_of[T]()`, `Vec[T]` — drei Blocker, zwei behoben

**Gebaut:** `size_of[T]()` liefert die Größe eines Typs in Bytes zur
Übersetzungszeit (`compiler/src/sizeof.rs`, `tests/611_size_of.fi`). Damit
läuft ein **wachsendes** `Vec[T]` auf dem Heap: `tests/620_vec_heap.fi` legt
1.000 `i32`, 300 `u8` und 100 `u64` an — drei Ausprägungen, drei
Elementgrößen, alle drei Baustufen.

Zur Laufzeit bleibt von `size_of` nichts übrig: der Typprüfer rechnet die
Größe aus, das Lowering setzt eine Konstante ein.

**Drei Blocker, die beim Bau von `lib/rt/vec.fi` sichtbar wurden.** Sie sind
der eigentliche Ertrag dieser Runde — ohne sie lässt sich keine
Bibliothekssammlung schreiben, und der Compiler in Stufe 1 besteht aus nichts
anderem:

| # | Blocker | Stand |
|---|---|---|
| B1 | Generische Vorlage aus einem Modul nicht benutzbar | **behoben (Runde 17)** |
| B2 | Generische Vorlage sieht die Namen ihrer eigenen Moduldatei nicht | **behoben (Runde 18)** |
| B3 | Importe werden relativ zur Wurzeldatei aufgelöst | **behoben (Runde 17)** |

### B3 — behoben

`modules::resolve` sucht einen Importpfad jetzt **zuerst relativ zur Datei, die
den Import schreibt**, und erst danach relativ zur Wurzel. Der Rückfall bleibt,
damit bestehende Programme unverändert laufen.

### B1 — behoben

Die Vorabsuche nach generischen Vorlagen (`sema_generic::hook_prescan`) lief je
Datei **unmittelbar vor deren Parsen**. Die Wurzeldatei wird zuerst geparst —
sie kannte die Vorlagen der Module also noch nicht.
`modules::build_program` lext jetzt **erst alle Dateien und scannt sie vorab**,
dann wird geparst.

### B2 — behoben

Die Ursache war handfester als vermutet: **generische Vorlagen liegen nicht in
`Program::funcs`**, sondern in `sema_generic::REG`. Das Modul-Umschreiben in
`modules::build_program` läuft über `Program::funcs` — es erreichte die
Vorlagen also nie. Eine Vorlage sah dadurch nur die Namen der Wurzeldatei;
selbst eine Hilfsfunktion in derselben Moduldatei meldete *unbekannte
funktion*.

`build_program` schickt jetzt **auch die Vorlagen der jeweiligen Datei** durch
denselben `Renamer`. Der **Name** der Vorlage bleibt dabei unangetastet: die
Ausprägung sucht ihn später unter dem ursprünglichen Namen
(`mono::expand_fn` über `Instantiation::base`), und generische Namen gelten
programmweit.

### Was damit möglich wurde

`lib/rt/vec.fi` ist die erste echte **generische Sammlung als Bibliothek**:
sie bindet `rt` aus ihrem eigenen Verzeichnis ein (B3), ruft
`rt.heap_alloc`/`rt.mem_copy` aus dem Rumpf einer Vorlage (B2), und die
Wurzeldatei schreibt `var v: Vec[i32] = vec_neu[i32]()` (B1). Die Doppelung der
Speicherfunktionen ist wieder verschwunden.

Nachweis: `tests/640_vec_module.fi` (1.000 `i32`, 300 `u8`, 100 `u64`, `pop`,
`setzen`, Zugriff jenseits des Endes) in allen drei Baustufen.

**Damit ist `lib/std/` schreibbar** — der nächste Schritt auf der Liste in §2.


---

## 9. Runde 19: `Map[K, V]` und der `Interner` — Punkt 3 ist erledigt

**Gebaut:** `lib/rt/map.fi` (Hash-Abbildung) und `lib/rt/intern.fi`
(Zeichenkette → Nummer). Beide als **Bibliothek**, nicht in der Wurzeldatei —
das geht erst, seit B1–B3 gefallen sind (§7, §8).

### Warum zwei Bausteine und nicht einer

`Map[K, V]` hat **skalare** Schlüssel. Eine Namenstabelle des Compilers hat
aber Bezeichner als Schlüssel. Der `Interner` schließt die Lücke: er vergibt je
Bezeichner eine `u32`, danach rechnet alles mit Nummern. Der Rust-Compiler
macht es nicht anders. Zwei Gewinne über „geht auch" hinaus: der Vergleich
zweier Bezeichner ist ein `u32`-Vergleich statt einer Byteschleife, und jeder
Bezeichner liegt genau einmal im Speicher.

### `Map[K, V]` — offene Adressierung

Drei getrennte Felder (Schlüssel, Werte, Zustand) statt eines Feldes aus
Paaren: das spart die Ausrichtungslöcher bei ungleichen Größen, und die
Sondierschleife liest nur ein Byte je Schritt. Kapazität ist immer eine
Zweierpotenz (`hash & (kap-1)` statt Division), Lastgrenze 3/4.

Zustand je Platz: **0 = leer, 1 = belegt, 2 = gelöscht.** Der Unterschied
zwischen 0 und 2 ist der Kern: eine Suche darf bei 2 **nicht** abbrechen, sonst
verliert sie alles, was hinter einer Löschung liegt. Grabsteine zählen in die
Lastgrenze mit — sonst entartet eine Tabelle, in die dauernd eingefügt und
gelöscht wird, zur linearen Suche.

### Die Streuung — gemessen statt behauptet

Die naheliegende Frage: braucht eine Tabelle mit fortlaufenden internen Nummern
überhaupt eine Hashrunde? Gemessen (20.000 Schlüssel, 32.768 Plätze, Last 0,61;
mittlere Sondierschritte je Einfügung / längste Kette):

| Schlüsselmuster | mit Streuung | ohne (Schlüssel = Platz) |
|---|---|---|
| fortlaufend `k` | 1,80 / 38 | **1,00 / 1** |
| `k * 1024` (Zeiger) | 1,76 / 37 | 313,00 / 625 |
| `k * 65536` | 1,62 / 39 | 10000,50 / 20000 |
| `k << 32` (nur hohe Bits) | 1,80 / 53 | 10000,50 / 20000 |

Die Antwort ist also **nicht** „die Streuung ist immer besser" — im Idealfall
kostet sie 0,8 Sondierschritte. Sie kauft dafür, dass Schlüssel, die sich nur
in hohen Bits unterscheiden (Zeiger, ausgerichtete Adressen, verschobene
Nummern), die Tabelle nicht auf **lineare Suche** zurückwerfen. 0,8 gegen
10.000 ist kein knapper Abwägungsfall.

### Was die Abbildung wirklich bringt

Dieselbe Aufgabe — 20.000 Einträge anlegen, 100.000 Suchen — einmal über
`Map[u32, i64]` und einmal über lineare Suche in zwei `Vec`:

| | `Map` | lineare Suche in `Vec` |
|---|---|---|
| Wanduhr | **0,005 s** | 2,238 s |

Faktor **447**. Für den Namensauflöser des Compilers ist das der Unterschied
zwischen „läuft" und „läuft nicht".

### `Interner` — Kollisionen korrekt, nicht bloß unwahrscheinlich

Alle Bytes hintereinander in einem `rt.Buf`, Versatz und Länge je Nummer in
zwei `Vec[u32]`, dazu eine eigene Sondiertabelle aus `u32`. Beim Sondieren wird
der **Text** verglichen, nicht der Hashwert — zwei verschiedene Bezeichner mit
gleichem FNV-Wert bekommen also verschiedene Nummern. Das ist der Unterschied
zwischen korrekt und „bisher nicht aufgefallen".

Eine Falle steckt im Umzug: `intern_nummer` muss den Hash aus dem **eigenen**
Puffer nehmen, nachdem kopiert wurde — der übergebene Zeiger kann in denselben
Puffer gezeigt haben und beim Wachsen ungültig geworden sein.

Nachweis: `tests/650_map_module.fi` (1.000 Einträge über mehrere Verdopplungen,
Überschreiben, Löschen **und Suchen hinter dem Grabstein**, Wiedereinfügen,
Durchlauf, drei Ausprägungen mit ungleichen Größen, negative Schlüssel,
`map_reserve` ohne Umstreuen) und `tests/651_intern_module.fi` (Präfixe „ab" vs.
„abc", leere Zeichenkette, 2.000 erzeugte Bezeichner mit umziehendem Puffer,
Nummer als `Map`-Schlüssel) — beide in allen drei Baustufen.

### Ehrliche Grenzen

* **Kein Entfernen im `Interner`.** Er wächst nur. Für einen Compilerlauf
  richtig, für einen Dauerläufer nicht.
* `intern_zeiger` gilt nur **bis zum nächsten `intern_nummer`** — der
  Textpuffer kann umziehen. Wer den Zeiger behalten will, kopiert.
* `map_hol` liefert bei fehlendem Schlüssel `0 as V`. Wer 0 von „fehlt"
  unterscheiden muss, braucht `map_hat` davor. Sauber wäre `Option[V]` — das
  setzt Punkt 6 (Summentypen) voraus.
* Ein `Str`-Typ mit Verkettungsoperator fehlt weiter (Punkt 4 bleibt `[~]`).

### Stand der Liste nach dieser Runde

| Punkt | Stand |
|---|---|
| 1 Heap-Allokator | ✅ |
| 2 `Vec[T]` | ✅ |
| 3 `Map[K,V]` | ✅ |
| 4 `Str`/`Bytes` | `[~]` |
| 5 Textformatierung | `[~]` |
| 12 Dateizugriff | ✅ |

**Nächste Blocker:** Summentypen mit Nutzlast + `match` (Punkt 6) und
rekursive Datentypen (Punkt 7) — zusammen sind sie der AST.


---

## 10. Runde 20: der Lexer von Firn, geschrieben in Firn

**Stufe 1 hat angefangen.** `lib/firnc1/lexer.fi` ist der erste Compilerteil in
Firn — 1.009 Zeilen auf `rt`, `Vec[T]` und `Interner`. §2 nennt den Lexer als
richtigen Anfang, weil er die kleinste Schnittstelle hat: Text hinein,
Tokenfeld hinaus.

### Der Maßstab kommt von außen

Ein Lexer lässt sich nicht gegen sich selbst prüfen. `bin/lexdump.fi` schreibt
den Tokenstrom in **genau** dem Format von `firnc0 --emit=tokens`;
`tools/lex_compare.sh` lässt beide über `tests/`, `lib/`, `bin/` und
`bench/` laufen und vergleicht Oktett für Oktett.

| | |
|---|---:|
| Dateien Oktett-gleich | **294** |
| Dateien abweichend | 1 (benannt, siehe unten) |
| übersprungen (`firnc0` meldet dort selbst einen Fehler) | 2 |
| verglichene Token | **211.405** |

Das läuft als Abschnitt 11 in `test.sh` bei jeder Änderung mit.

### Was das gefunden hat: ein echter Fehler in Stufe 0

Der in Firn geschriebene Lexer las `10.0` und bekam ein anderes Bitmuster als
`firnc0` — aber **nur mit Optimierer**. Ursache: `f64` ist 64 Bit breit und
gilt im FIR als vorzeichenlos. Zwei Stellen haben daraus geschlossen,
`u64 -> f64` sei eine reine Umdeutung desselben Bitmusters:

* `mem2reg.rs` strich die Umwandlung **ersatzlos**,
* `opt.rs::fold_cast` faltete sie zu einer reinen Bitoperation.

Aus `100 as f64` wurde damit das Bitmuster 100 — also `5e-322` statt `100.0`.
Es ist genau umgekehrt: von allen Umwandlungen ist die zwischen Ganzzahl und
Gleitkomma die **einzige**, die die Bits wirklich ändert (`cvtsi2sd`). Der Weg
ohne Optimierer war die ganze Zeit richtig.

Beide Stellen sind behoben; `fold_cast` rechnet die Umwandlung jetzt **echt**
(und faltet auch `f64 -> Ganzzahl`, außer bei NaN, Unendlich und außerhalb des
Zielbereichs). Rückfalltest: `tests/591_f64_conversion.fi`, der wie jeder
Positivtest mit **und** ohne Optimierer läuft.

Das ist der eigentliche Ertrag dieser Runde. Ein Bootstrap ist keine
Fleißaufgabe — er ist ein **Prüfstand**, weil zwei unabhängige Umsetzungen
gegeneinander stehen. Der Fehler lag seit Runde 14 im Baum und ist durch 590
Tests gerutscht.

### Wie der Lexer gebaut ist

* **Struktur der Felder statt Feld der Strukturen.** `Vec[T]` verlangt
  `T: Scalar`, ein Token ist ein Verbund — also liegen Art, Zeile, Spalte,
  Länge, Zahlwert und Nummer in sechs gleich langen `Vec`. Der Parser liest
  fast immer nur die Art, und die liegt so dicht beieinander.
* **Zeichen gegen Oktette.** `firnc0` arbeitet auf `Vec<char>`, Spalten zählen
  Zeichen. Der Firn-Lexer arbeitet auf Oktetten und zählt eine Spalte nur beim
  **führenden** Oktett eines UTF-8-Zeichens. Ohne das verschieben sich alle
  Spalten hinter einem Umlaut in derselben Zeile — und genau das prüft der
  Vergleich mit, weil `tests/570_zeichenkettenliterale.fi` Umlaute enthält.
* **Worttafeln statt globaler Zustand.** Firn hat keine veränderlichen
  globalen Zustände (Punkt 13). Die Schlüsselwort- und Namenstabellen entstehen
  deshalb aus **einem** Literal, das an `|` zerlegt wird.
* Bezeichner gehen durch den `Interner` aus Runde 19; Zeichenkettenliterale
  werden vollständig entschlüsselt, samt `\x`, `\u{...}`, Surrogatpaaren in
  `Str` und ungepaarten Surrogaten in `Str16`.

### Die eine Abweichung — benannt, nicht weggeräumt

`tests/590_f64.fi`, das Literal **`1e308`**. Der Firn-Lexer nutzt den schnellen
Pfad von Clinger: passt die Mantisse in 2^53 und liegt der Zehnerexponent
zwischen -22 und 22, dann ist eine einzige Multiplikation in binary64 korrekt
gerundet. Die Erweiterung nach oben (Überschuss in die Mantisse ziehen) deckt
Exponenten bis 37 ab. Darüber wird schrittweise multipliziert, und das liegt um
**ein ULP** daneben.

Gemessen: von 211.405 Token im Korpus braucht **genau eines** den langsamen
Pfad. Korrekt wäre Eisel-Lemire mit 128-Bit-Arithmetik — die fehlt noch, und
die Zeile bleibt in `tools/lex_compare.sh` stehen, bis sie da ist.

### Weitere ehrliche Grenzen

* **Diagnosen sind nicht portiert.** Der Firn-Lexer zählt Fehler, er meldet
  sie nicht mit Zeile und Text. Das ist das Modul `diag` und ein eigener
  Schritt. Deshalb überspringt der Vergleich die zwei Dateien, bei denen
  `firnc0` selbst einen Fehler meldet.
* **Unicode-Leerraum** (U+00A0, U+2028 …) gilt in `firnc0` als Leerraum, im
  Firn-Lexer nicht. Im ganzen Korpus kommt keiner vor; der Unterschied ist
  benannt, nicht behoben.
* Der Lexer erzeugt noch **keinen** AST — der Parser ist der nächste Schritt.

### Nebenbefunde zur Liste in §4

* Punkt 6 (**Summentypen**) ist ohne Typparameter **vollständig da** —
  `enum Wert { Nichts, Zahl(i32), Paar(i32, i32) }` mit Bindung im Muster
  läuft seit Runde 2. Was fehlt, ist `enum Name[T]`: der Parser kennt keine
  Typparameterliste hinter einem Aufzählungsnamen (`Option[T]`, `Result[T,E]`).
* Punkt 7 (**rekursive Datentypen**) läuft über `*mut` auf den eigenen Typ:
  `enum Ausdruck { Zahl(i64), Plus(*mut Ausdruck, *mut Ausdruck) }` übersetzt
  und rechnet. Ein `Box`-Typ mit eigener Freigabe fehlt — die Bäume des
  Compilers werden ohnehin über `Vec`-Indizes gebaut (§2.4).

### Stand der Liste

| Punkt | Stand |
|---|---|
| 1 Heap-Allokator · 2 `Vec[T]` · 3 `Map[K,V]` | ✅ |
| 6 Summentypen + `match` | ✅ (ohne Typparameter) |
| 7 rekursive Datentypen | ✅ (über Zeiger) |
| 12 Dateizugriff | ✅ |
| 4 `Str`/`Bytes` · 5 Textformatierung | `[~]` |
| 8 Methoden · 9 Schnittstellen · 10 `Result`/`?` · 11 `fork`/`execve` · 13 globale Zustände | `[ ]` |

**Nächster Schritt:** `config` und `diag` in Firn — und danach der Parser. Für
`firnc1` als eigenständiges Programm fehlt Punkt 11 (`fork`/`execve`), weil
`firnc` `as` und `ld` aufruft. Das ist der einzige Punkt der Liste, für den es
keinen Umweg gibt.


---

## 11. Runde 21: `diag` in Firn — und der Beweis über die Fehlerausgabe

`lib/firnc1/diag.fi` ist der zweite Compilerteil in Firn. Der Lexer **zählt**
Fehler jetzt nicht mehr, er **meldet** sie — mit Datei, Zeile, Spalte,
Quelltextzeile und Markierung, im verbindlichen Format aus `diag.rs`:

```text
error: in einem zeichenkettenliteral: \u{...} ist nicht abgeschlossen
  --> tests/lexneg/u_escape.fi:3:23
   |
 3 |     var b: [u8; 4] = "\u{}"
   |                       ^ hier
4 Fehler gefunden
```

### Der Vergleich prüft jetzt beide Ströme

`tools/lex_compare.sh` vergleicht nicht mehr nur den Tokenstrom, sondern auch
die **Fehlerausgabe** — Oktett für Oktett gegen `firnc0 --emit=tokens`.

| | |
|---|---:|
| Dateien gleich (beide Ströme) | **306** |
| davon mit Diagnosen | **10** |
| verglichene Token | **216.489** |
| abweichend | 1 (`1e308`, benannt) |
| übersprungen (Modulbruchstück) | 2 |

Die zehn Fälle in `tests/lexneg/` decken jede Meldung ab, die der Lexer
erzeugen kann: unbekanntes Zeichen, Zahl zu groß, ungültige Ziffer zur Basis,
offener Blockkommentar, offenes Literal, unbekannte Maskierung, `\x` ohne
zwei Ziffern, `\u{...}` in allen vier Fehlerformen, ungepaartes Surrogat,
`\u` in `b"..."`, Nicht-ASCII in `b"..."`, `\xFF` in `"..."` und leerer
Exponent.

### Was dafür an der Sprache fehlte: Aufrufargumente

Eine Diagnose enthält den **Dateinamen** — also muss das Programm ihn
entgegennehmen können. Firn konnte das nicht. Jetzt gibt es eine zweite
erlaubte Form des Einstiegspunkts:

```firn
fn main(start: u64) -> i32
```

Beim Prozessstart zeigt `rsp` auf `[argc][argv0]..[argvN][0][envp..]`;
`_start` legt diesen Zeiger nach `rdi`, also in den ersten Parameter. Ein
Programm mit `fn main() -> i32` merkt davon nichts — es liest `rdi` nie.
`lib/rt/rt.fi` bekommt dazu `arg_anzahl`, `arg_zeiger` und `c_laenge`; die
argv-Zeichenketten sind nullabgeschlossen und damit genau das, was
`lies_datei` erwartet. Nachweis: `tests/660_args.fi`.

Das ist der erste Teil von Punkt 11 der Liste. `fork`/`execve` fehlen weiter —
ohne sie kann `firnc1` `as` und `ld` nicht aufrufen.

### Wie `diag` gebaut ist

Dieselbe Form wie im Lexer: **Struktur der Felder**. Eine Diagnose besteht aus
vier Zahlen (Datei, Zeile, Spalte, Länge) in `Vec[u32]` und drei Textstücken
(Meldung, Markierungstext, Hinweis) als Versatz/Länge in **einem** Puffer.

Zwei Dinge, die beim Nachbauen wichtig waren und die man leicht übersieht:

* **Der Markierungsversatz zählt Zeichen, nicht Oktette** — und ein Tabulator
  zählt als vier. Ohne beides steht das Dach hinter einem Umlaut oder hinter
  einem Tabulator an der falschen Stelle.
* **Doppelte Meldungen an derselben Stelle werden unterdrückt.** Ohne das
  erzeugt die Fehlerwiederherstellung Reihen identischer Zeilen — und der
  Vergleich mit `firnc0` fällt sofort um.

Firn hat keine Textverkettung (Punkt 4 der Liste). Die Meldungen entstehen
deshalb in einem `rt.Buf`: **der Puffer ist die Verkettung.** Zahlen kommen
über `buf_push_dez_u64`, ein Zeichen der Quelle wird oktettweise kopiert, und
für `U+{:04X}` steht eine eigene kleine Hexausgabe daneben.

### Ehrliche Grenzen

* `diag` kann bisher nur, was der Lexer braucht: `error` und `error_note` mit
  der Markierung „hier". Frei wählbare Markierungstexte, mehrere Markierungen
  je Diagnose und Farbausgabe gibt es nicht — `diag.rs` hat sie auch nicht.
* Die Obergrenze von 40 Meldungen ist übernommen, aber nicht geprüft: kein
  Testfall im Korpus erzeugt so viele Lexfehler.
* `config` ist **nicht** portiert. Es sind drei Konstanten und eine Funktion;
  ohne Textverkettung wäre `compiler_name()` mehr Aufwand als Nutzen. Der
  Punkt bleibt offen, bis `Str` steht.

### Stand

| Teil | Stand |
|---|---|
| `lexer` | ✅ in Firn, gegen `firnc0` geprüft |
| `diag` | ✅ in Firn, Fehlerausgabe gegen `firnc0` geprüft |
| `config` | `[ ]` (braucht `Str`) |
| `ast`, `parser`, … | `[ ]` |

**Nächster Schritt:** der Parser. Er braucht einen AST — und damit zum ersten
Mal rekursive Datenstrukturen über `Vec`-Indizes, genau wie in §2.4 geplant.


---

## 12. Runde 22: der Parser der Kernsprache in Firn

`lib/firnc1/ast.fi` und `lib/firnc1/parser.fi` sind der dritte und vierte Teil
von Stufe 1. Damit ist die Kette **Text → Token → Baum** vollständig in Firn
geschrieben.

### Der Baum liegt in `Vec`, nicht in Zeigern

§2.4 sagt seit der ersten Fassung, wie der AST gebaut sein muss: rekursive
Bäume über `Vec`-Indizes. `ast.Baum` hält dreiunddreißig `Vec` mit festen
Plätzen je Knotenart:

| | |
|---|---|
| Ausdruck | `e_art` · `e_a` · `e_b` · `e_c` · `e_zahl` |
| Anweisung | `s_art` · `s_a` · `s_b` · `s_c` · `s_d` |
| Typ | `t_art` · `t_a` · `t_b` |
| Block | `b_off` · `b_len` → Bereich in `sliste` |

Kinderlisten veränderlicher Länge (Aufrufargumente, Arrayelemente,
Structfelder) liegen hintereinander in **einem** `Vec`; der Knoten merkt sich
Versatz und Anzahl. Kein Allokator je Knoten, keine Freigabereihenfolge, kein
Zerstörer — `baum_frei` gibt alles in einem Zug frei.

### Der Fehler, der genau daraus entstand

Der naheliegende Weg — jedes Argument sofort an `kinder` anhängen — ist
**falsch**, sobald ein Argument selbst einen Aufruf enthält: dessen Argumente
landen dann mitten in der äußeren Liste. Aus

```firn
rt.buf_push_bytes(b, intern.intern_zeiger(it, nr), intern.intern_laenge(it, nr))
```

wurde ein Aufruf mit **sieben** Argumenten statt drei. Der Vergleich mit
`firnc0` hat das in der ersten Runde gefunden, an genau der Datei, die den
Vergleich selbst druckt. Die Kinderliste wird jetzt erst gesammelt und dann am
Stück abgelegt.

### Der Maßstab: `--emit=ast-kanon`

`--emit=ast` ist Rusts `{:#?}` — an `Box`, `Some`/`None` und Feldnamen
gebunden. Ein Parser in einer anderen Sprache kann das nicht nachbauen, ohne
Rusts Debug-Ausgabe nachzuäffen; dann prüft der Vergleich die Formatierung
statt den Baum. `compiler/src/ast_canon.rs` erzeugt deshalb eine
**sprachneutrale** geklammerte Form:

```text
(fn u64_nach_f64 ((param m u64)) f64 (blk (ret (as (id m) f64))))
```

Gedruckt wird **nur die Wurzeldatei**, vor dem Zusammenführen der Module und
vor der Monomorphisierung — der Parser in Firn sieht ebenfalls genau eine
Datei.

### Ergebnis

`tools/parser_compare.sh`, Abschnitt 12 in `test.sh`:

| | |
|---|---:|
| Bäume gleich | **166** |
| abweichend | 1 (`1e308`, bekannt aus Runde 20 — ein **Lexer**fall) |
| nicht Kernsprache | 109 |
| übersprungen (`firnc0` kommt selbst nicht durch) | 36 |

### Was der Parser NICHT kann — gezählt, nicht übergangen

Alles, was seinen Baum außerhalb von `Program` hält: `enum`/`match`,
Fehlerunionen (`E!T`, `try`, `catch`), generische Vorlagen, `gc class`,
Attribute, `comptime`. Eine Vorabsuche im Tokenstrom erkennt diese Dateien am
Muster (`::`, `?`, `IDENT !`, `fn name[`, der Bezeichner `gc`) und meldet
Rückgabewert 3 — sie zählen als **109 „nicht Kern"** und verschwinden nicht
still in einer Erfolgszahl.

Ebenfalls nicht portiert: die **Fehlerwiederherstellung**. `firnc0` sammelt bis
zu vierzig Meldungen und liest weiter; dieser Parser meldet die erste und hört
auf. Für den Vergleich ohne Belang — er läuft nur über Quellen, die `firnc0`
fehlerfrei liest —, für einen echten Compiler nicht.

Und: **Quellpositionen stehen nicht in der kanonischen Form.** Sie gehören zum
Baum, aber ihre Zusammensetzung (`Parser::join` über Teilausdrücke) ist eine
eigene Verabredung. Der Baum stimmt; ob jede Spanne stimmt, ist noch nicht
geprüft.

### Eine Regel, die man leicht übersieht

Ein Operator am **Zeilenanfang** setzt den Ausdruck nicht fort, solange keine
Klammer offen ist. Deshalb ist

```firn
a
- b
```

zwei Anweisungen und keine Subtraktion. Dieselbe Regel hatte mich beim
Schreiben von `diag.fi` schon einmal erwischt (Runde 21, mehrzeilige
`&&`-Bedingung); im Parser steht sie jetzt als `weiter()`.

### Stand

| Teil | Stand |
|---|---|
| `lexer` | ✅ in Firn, gegen `firnc0` geprüft |
| `diag` | ✅ in Firn, Fehlerausgabe geprüft |
| `ast` + `parser` | ✅ Kernsprache, 166 Bäume gleich |
| `config` | `[ ]` (braucht `Str`) |
| `types`, `sema`, `fir`, `lower`, `codegen` | `[ ]` |

**Nächster Schritt:** `types` und `abi` — §3 nennt sie als die Dateien, die
schon heute vollständig in Firn schreibbar wären (reine Fallunterscheidung auf
Typ und Größe). Danach wird es ernst: `sema`.


---

## 13. Runde 23: `types` und `abi` in Firn — Layout und Aufrufkonvention

§3 nennt `types.rs` und `abi.rs` seit der ersten Fassung als die Dateien, die
**heute schon vollständig in Firn schreibbar** wären: sie rechnen nur auf Typ
und Größe, ohne Heap und ohne Text. `lib/firnc1/types.fi` löst das ein.

### Warum ausgerechnet die zwei als Nächstes

Layout und Aufrufkonvention sind die Stellen, an denen ein Compiler **still
falsch** wird. Ein Feldversatz daneben, ein Aggregat in Registern statt im
Speicher — das Programm läuft, nur eben falsch, und der Fehler zeigt sich
irgendwo ganz anders. Zwei unabhängige Umsetzungen gegeneinander zu stellen
findet hier mehr als jeder ausgedachte Testfall.

### Der Maßstab: `--emit=layout`

`compiler/src/layout_canon.rs` druckt je Struct Größe, Ausrichtung und **jeden
Feldversatz**, je Funktion die System-V-Klasse jedes Arguments und des
Rückgabewertes samt `sret`:

```text
(struct Loecher groesse 32 ausrichtung 8 (feld a versatz 0 …) (feld b versatz 8 …) …)
(fn summe_gross (arg Gross groesse 24 klasse mem) (ret i64 groesse 8 klasse int1 sret 0))
```

Aufgelöst wird nur die Wurzeldatei. Ein Name, den es dort nicht gibt (etwa
`rt.Buf` aus einem anderen Modul), wird auf **beiden** Seiten zu `?` mit Größe
0 und Ausrichtung 1 — sonst würde der Vergleich an einer künstlichen
Unsicherheit scheitern statt an einem echten Unterschied.

### Ergebnis

`tools/types_compare.sh`, Abschnitt 13 in `test.sh`:

| | |
|---|---:|
| Layouts gleich | **169** |
| abweichend | **0** |
| davon mit echten Structs | 35 |

Null Abweichungen im ersten Lauf. Das ist ungewöhnlich und hat einen Grund:
die Regeln sind kurz und stehen ausgeschrieben in SPEC §11 und §13 — anders
als beim Parser, wo hundert kleine Verabredungen zusammenkommen.

### `tests/670_layout_abi.fi` — die Formen, die das Korpus nicht hatte

Das vorhandene Korpus prüft Layout nur nebenbei. Der neue Test prüft es
**gegeneinander**:

* **Feldversätze zur Laufzeit nachgerechnet**, über Adressdifferenzen statt
  über eine Tabelle im Kopf. Damit hängt nicht nur die Rechnung, sondern der
  *erzeugte Code* an denselben Zahlen.
* **Alle drei ABI-Grenzen**: bis 8 Byte (ein Wort), 9..16 Byte (zwei Wörter —
  Rückgabe aber schon über den versteckten Zeiger, SPEC §14.1), über 16 Byte
  (Speicher).
* Struct im Struct, Array im Struct, Löcher durch Ausrichtung, neun Wörter
  Argumente (die letzten laufen über den Stapel), und Kopiersemantik bei
  Wertübergabe.

Läuft mit und ohne Optimierer, und beide Layoutausgaben stimmen überein.

### Ehrliche Grenzen

* `types.fi` löst nur auf, was die **Kernsprache** kennt. `enum`-Layout
  (Marke + überlagerte Varianten), Fehlerunionen und `gc class` fehlen — sie
  gehören zu den 109 Dateien, die der Parser ohnehin nicht liest.
* Es gibt **keine Typprüfung**. Was hier steht, ist Layout und ABI, nicht
  `sema`: keine Zuweisbarkeit, keine Literalableitung, keine Fehlermeldungen
  zu Typen. Das ist der nächste und mit Abstand größte Brocken.
* Die Klasse `Sse` fehlt genau wie in `abi.rs` — Stufe 0 übergibt auch `f64`
  in Ganzzahlregistern. Das ist eine bekannte Abweichung von System V und
  steht so in SPEC §14.1.

### Stand

| Teil | Stand |
|---|---|
| `lexer` · `diag` · `ast` + `parser` | ✅ in Firn, gegen `firnc0` geprüft |
| `types` + `abi` | ✅ in Firn, 169 Layouts gleich |
| `config` | `[ ]` (braucht `Str`) |
| `sema` | `[ ]` ← der nächste und größte Schritt |
| `fir`, `lower`, `codegen_x86` | `[ ]` |

**Nächster Schritt:** `sema` — Namensauflösung, Typprüfung, Literalableitung.
Dafür stehen jetzt alle Bausteine bereit: `Map[K,V]` für die Bereichsketten,
der `Interner` für die Namen, `diag` für die Meldungen und `typen` für die
Typen selbst.


---

## 14. Runde 24: der Typprüfer in Firn

`lib/firnc1/sema.fi` ist der sechste Teil von Stufe 1 und der bisher größte.
Damit ist die Kette **Text → Token → Baum → Typen** vollständig in Firn
geschrieben.

### Was genau geprüft wird

`firnc0` gibt eine Zusicherung: nach der Prüfung hat **jeder** Ausdruck einen
konkreten Typ — nie `UntypedInt`, nie `Error`. Genau diese Zusicherung ist der
Maßstab. `--emit=typen` druckt den kanonischen Baum aus Runde 22 mit dem Typ an
jedem Ausdruck:

```text
(let summe i32 (bin + (id a :i32) (int 1 :i32) :i32))
```

Die `1` ist hier `i32`, obwohl nirgends `i32` danebensteht. Das ist der Kern:

### Bidirektional, und die Reihenfolge ist Vertrag

Ein Ganzzahlliteral hat in Firn **keinen eigenen Typ**. Er kommt entweder aus
dem Kontext oder aus dem anderen Operanden. Dafür gibt es zwei Wege durch
denselben Baum:

* `probe` ermittelt den Typ **ohne** zu melden und ohne die Tabelle zu
  beschreiben — damit `a + 1` den Typ des Literals aus `a` gewinnt;
* `ausdruck` prüft wirklich und trägt den Typ ein.

Die Reihenfolge ist festgelegt: **erst der bereits typisierte Operand, dann der
Kontexthinweis.** Andernfalls meldet `let x: i64 = a + 1` (mit `a: i32`) einen
verwirrenden Operandenfehler statt des echten Fehlers an der Zuweisung.

### Ergebnis

`tools/sema_compare.sh`, Abschnitt 14 in `test.sh`:

| | |
|---|---:|
| Dateien mit identischer Typtabelle | **113** |
| dabei verglichene Ausdrücke | **24.529** |
| abweichend | 1 (`1e308`, bekannt aus Runde 20 — ein *Lexer*fall) |
| nicht Kernsprache | 36 |
| `comptime`-Auswertung nötig | 1 |
| übersprungen (`firnc0` prüft die Datei nicht einzeln) | 168 |

24.529 Ausdrücke, jeder mit demselben Typ wie in `firnc0`. Die 168
übersprungenen Dateien binden fast alle ein Modul ein — einzeln betrachtet sind
deren Namen unbekannt, und dann prüft auch `firnc0` nicht.

### `tests/680_typableitung.fi`

Der neue Test fährt die Ecken ab, die das Korpus nicht hatte: der Typ aus dem
anderen Operanden, der linke Operand einer Verschiebung, Argumenttyp als
Hinweis, Index immer `usize`, verschachtelte Struct-Literale, Zeiger auf
Zeiger, `for`-Bereich mit zwei Grenzen, Schatten in einem inneren Block, und
ein Wiederholungsliteral, dessen Länge aus einem konstanten Ausdruck kommt.

Dabei fiel eine Grenze von Stufe 0 auf, die vorher nirgends stand: **die Länge
im Arraytyp muss ein Literal sein** (`[u8; 12]`), im Wiederholungsliteral darf
dagegen ein konstanter Ausdruck stehen (`[0 as u8; FLAECHE]`).

### Was NICHT portiert ist — benannt, nicht verschwiegen

* **Sämtliche Fehlermeldungen.** Die Firn-Fassung *zählt* Fehler, sie
  beschreibt sie nicht. Der Vergleich läuft nur über Quellen, die `firnc0`
  fehlerfrei prüft; dort ist die Zahl null, und was zählt, sind die Typen.
  Für einen echten Compiler ist das zu wenig — `diag` steht bereit, die
  Meldungstexte fehlen.
* **Erreichbarkeitsanalyse** (`return` am Ende jedes Pfades),
  **Veränderlichkeitsprüfung** (`let` gegen `var`) und die
  **Rekursionsprüfung** für Structs.
* **`comptime`-Auswertung** konstanter Ausdrücke: ein Aufruf in einem `const`
  wird von `firnc0` zur Übersetzungszeit ausgeführt. Solche Dateien liefern
  Rückgabewert 4 und werden gesondert gezählt.
* Die **Intrinsics für konstante Laufzeit** (`select`, `barrier`,
  `secure_zero`) sind gewöhnliche Bezeichner, kein Schlüsselwort — sie lassen
  sich nur am Namen erkennen. Wer eine eigene Funktion `select` schreibt, fällt
  dadurch aus dem Vergleich heraus; das ist die vorsichtige Richtung.

### Ein Zwischenfall, der hierher gehört

Beim Anlegen der Symlinks in `lib/firnc1/` hat ein falsch geschriebener
`ln -sf`-Aufruf vier echte Dateien durch Verweise auf sich selbst ersetzt.
`ast.fi` und `types.fi` kamen aus dem Git zurück, `sema.fi` und `print.fi`
waren noch nicht eingecheckt und mussten neu geschrieben werden. Lehre, ohne
Beschönigung: **vor jedem Sammelbefehl auf Verzeichnisse committen.** Der
Verlust hat eine halbe Runde gekostet.

### Stand

| Teil | Stand |
|---|---|
| `lexer` · `diag` · `ast` + `parser` · `types` + `abi` | ✅ |
| `sema` | ✅ Kern: Namen, Bereiche, Typen — ohne Meldungstexte |
| `config` | `[ ]` (braucht `Str`) |
| `fir`, `lower`, `codegen_x86` | `[ ]` |

**Nächster Schritt:** `fir` und `lower` — vom geprüften Baum zur
Zwischendarstellung. Der Maßstab liegt schon bereit: `firnc0 --emit=fir-raw`
druckt die FIR vor jeder Optimierung.


---

## 15. Runde 25: `fir` und `lower` — die Zwischendarstellung in Firn

`lib/firnc1/fir.fi` und `lib/firnc1/lower.fi` sind der siebte und achte Teil
von Stufe 1. Damit reicht die Kette in Firn von **Text bis zur
Zwischendarstellung**: Token → Baum → Typen → **FIR**.

### Der schärfste Vergleich der ganzen Reihe

`firnc0 --emit=fir-raw` druckt die FIR **direkt nach dem Lowering**, vor jeder
Optimierung. Verglichen wird der Text Oktett für Oktett — und der enthält
alles, worauf es ankommt: Wertnummern, Blocknummern, Reihenfolge der
Instruktionen, Terminatoren. Zwei Wertnummern in anderer Reihenfolge, ein Block
zu viel oder zu wenig, und der Text stimmt nicht mehr.

| | |
|---|---:|
| Dateien mit identischer FIR | **66** |
| verglichene Instruktionen | **2.129** |
| abweichend | 1 (`1e308`, bekannt aus Runde 20) |
| Aggregate oder `defer` (nicht portiert) | 48 |
| nicht Kernsprache | 35 |
| übersprungen (`firnc0` übersetzt nicht einzeln) | 172 |

### Zwei Dinge, die man nicht raten kann

**Die Wertnummern folgen dem Anlegen, die gedruckte Reihenfolge nicht.** Jede
`alloca` wandert an den Anfang des Eintrittsblocks, ihre Nummer bleibt aber
dort, wo sie entstanden ist. Deshalb steht in `main`

```text
%0 = alloca.ptr size=4 align=4
%2 = alloca.ptr size=4 align=4
%1 = const.i32 0
```

— `%1` ist zwischen den beiden Allocas entstanden und wird trotzdem danach
gedruckt. Wer die Allocas einfach vorn erzeugt, bekommt andere Nummern.

**Nach `return`, `break` und `continue` beginnt ein neuer, unerreichbarer
Block.** Er bleibt im Text stehen (`bb7`, `bb8` …); die Codebereinigung
entfernt ihn erst später. Wer ihn weglässt, bekommt einen anderen Text.

### Drei Fehler, die der Vergleich sofort gefunden hat

* **Argumentlisten schoben sich ineinander.** `zwei(zwei(1,2), zwei(3,4))` —
  die Argumente des inneren Aufrufs landeten mitten in der Liste des äußeren.
  Derselbe Fehler wie beim Parser in Runde 22, an derselben Ursache: eine
  gemeinsame Kinderliste, in die während des Sammelns geschrieben wird.
  Wieder gilt: erst sammeln, dann am Stück ablegen.
* **Ein Aufruf ohne Rückgabewert darf keinen Wert definieren.** `call.void`
  steht ohne `%n =` da; wer trotzdem eine Nummer vergibt, verschiebt alle
  folgenden.
* **`const.u64 18446744073709551615` wurde zu `const.u64 -1`.** Im Rust-Compiler
  steht dort ein `i128`, der den Wert druckt, wie er ist. Vorzeichen gehört nur
  zu vorzeichenbehafteten Typen.

### Was NICHT portiert ist — gezählt, nicht übergangen

* **Aggregate**: Structs und Arrays als Wert, als Argument, als Rückgabe, ihre
  Literale. Das hängt an der Aufrufkonvention (Wörter gegen Speicher, `sret`)
  und an `write_into` — ein eigener Schritt. **48 Dateien** fallen dadurch
  heraus, und das ist mit Abstand der größte offene Posten.
* **`defer` und `errdefer`**: sie brauchen einen Stapel je Blockebene und
  müssen bei `return`, `break` und `continue` in der richtigen Tiefe ablaufen.
* Die Zeilentabelle für `.debug_line` (`dwarf.rs`) — sie ändert den FIR-Text
  nicht, gehört aber zum Lowering.

### `tests/690_lowering_core.fi`

Fährt die Formen ab, an denen Nummerierung und Blockbildung hängen:
verschachtelte Aufrufe, Aufruf ohne Rückgabewert, Kurzschluss mit `&&`/`||`,
`while` mit `break` und `continue`, `for` mit `continue` (der Fortschaltblock
muss trotzdem laufen), Verschiebung mit ungleich breitem rechten Operanden, der
größte `u64`-Wert und Zeiger auf Zeiger.

### Stand

| Teil | Stand |
|---|---|
| `lexer` · `diag` · `ast` + `parser` · `types` + `abi` · `sema` | ✅ |
| `fir` | ✅ Struktur und Textform |
| `lower` | ✅ skalarer Kern · `[ ]` Aggregate, `defer` |
| `config` | `[ ]` (braucht `Str`) |
| `codegen_x86` | `[ ]` |

**Nächster Schritt:** Aggregate im Lowering — das schließt die 48 Dateien auf
und ist die Voraussetzung dafür, dass `firnc1` sich selbst übersetzen könnte.
Danach bleibt der Codegenerator.


---

## 16. Runde 26: Aggregate im Lowering

Der größte offene Posten aus Runde 25 ist zu. `lib/firnc1/lower.fi` übersetzt
jetzt auch Structs und Arrays — als Variable, als Argument, als Rückgabe und
als Literal.

| | Runde 25 | Runde 26 |
|---|---:|---:|
| Dateien mit identischer FIR | 66 | **113** |
| verglichene Instruktionen | 2.129 | **36.217** |
| wegen Aggregaten ausgeschlossen | 48 | **0** |
| wegen `defer` ausgeschlossen | — | 1 |

Von 2.129 auf 36.217 verglichene Instruktionen: nicht weil mehr Dateien
dazukamen, sondern weil die *großen* dazukamen — der HTML5-Tokenizer, der
DOM-Dauerlauf, die Vergleichswerkzeuge selbst.

### Aggregate bewegen sich nie als Wert

Es gibt genau zwei Wege, und beide arbeiten mit **Adressen**:

* `schreib_nach(adr, e)` legt einen Ausdruck an einer Adresse ab. Ein
  Struct-Literal wird **feldweise** geschrieben, ein Array-Literal
  **elementweise**, ein fremdes Aggregat mit `copymem` kopiert.
* `adresse(e)` besorgt die Adresse eines vorhandenen Aggregats. Für ein
  Literal oder einen Aufruf entsteht dabei ein Zwischenplatz.

### An der Funktionsgrenze entscheidet `abi`

| Größe | Weg |
|---|---|
| bis 16 Byte | ein oder zwei **Ganzzahlwörter** |
| darüber | versteckter **Zeiger auf eine Kopie** des Aufrufers |
| Rückgabe über 8 Byte | versteckter Zeiger in `rdi` (`sret`) |

Beim Laden in Wörter steckt eine Falle, die `types.rs` schon benennt und die
hier nachgebaut werden musste: **ist die Größe kein Vielfaches von acht, läuft
es über einen aufgefüllten Zwischenpuffer** — sonst läge der letzte `load`
teilweise *hinter* dem Objekt.

### Drei Abweichungen, die der Vergleich gefunden hat

* **Syscall-Argumente**: *jedes* geht als `i64` hinein, auch ein Zeiger. Mir
  fehlte genau ein `cast.ptr.i64` — und damit verschoben sich alle folgenden
  Wertnummern.
* **Das Wiederholungsliteral als Schleife** lädt den Index **einmal** und
  benutzt ihn für die Elementadresse *und* für das Hochzählen. Ein zweiter
  `load` ist eine Instruktion zu viel.
* Die Elementadresse bringt den Index **zuerst** auf `u64` und rechnet erst
  danach — die Reihenfolge steht in `layout.rs` und ist Vertrag.

### Was noch fehlt

**`defer` und `errdefer`.** Sie brauchen einen Stapel je Blockebene und müssen
bei `return`, `break` und `continue` in der richtigen Tiefe ablaufen (SPEC
§5.1). Genau **eine** Datei im vergleichbaren Korpus fällt dadurch heraus.

### Stand

| Teil | Stand |
|---|---|
| `lexer` · `diag` · `ast` + `parser` · `types` + `abi` · `sema` · `fir` | ✅ |
| `lower` | ✅ Skalare **und** Aggregate · `[ ]` `defer` |
| `config` | `[ ]` (braucht `Str`) |
| `codegen_x86` | `[ ]` ← der letzte große Schritt |

**Nächster Schritt:** der Codegenerator. Von der FIR zu x86-64-Assembler — und
damit zum ersten Mal ein Programm, das `firnc1` von vorne bis hinten selbst
übersetzt hat.


---

## 17. Runde 27: der Codegenerator — der erste Lauf von vorne bis hinten

`lib/firnc1/codegen.fi` erzeugt x86-64-Assembler aus der FIR, und
`bin/firnc1.fi` hängt die ganze Kette aneinander:

```text
Text → Token → Baum → Typen → FIR → Assembler
```

**109 Testprogramme wurden vollständig vom Firn-Compiler übersetzt,
assembliert, gelinkt, ausgeführt — und verhalten sich genau wie die von
`firnc0` erzeugten.** Gleicher Rückgabewert, gleiche Ausgabe, null
Abweichungen.

### Bewusst ohne Registerzuteilung

`firnc0` hat dafür `regalloc.rs` mit Lebendigkeitsanalyse und Verschmelzung.
`codegen.fi` hat das nicht: **jeder FIR-Wert bekommt einen Platz im Rahmen**,
und jede Instruktion lädt ihre Operanden, rechnet und schreibt zurück. Der
erzeugte Code ist deutlich langsamer — aber er ist richtig, und er passt in
einen Nachmittag.

Das ändert den Maßstab, und zwar zum Besseren: **verglichen wird nicht der
Assemblertext, sondern das Verhalten.** Zwei Codegeneratoren mit
unterschiedlicher Registerzuteilung *können* keinen gleichen Text erzeugen —
und sie müssen es auch nicht. Für einen Codegenerator ist „das Programm tut
dasselbe" ohnehin die ehrlichere Frage als „die Zeichen stimmen überein".

### Rahmen und Aufrufkonvention

`[rbp - 8*(v+1)]` ist der Platz des Wertes `%v`; dahinter liegen die Bereiche
der `alloca`s. Ein Wert liegt **immer als volle 64 Bit** da, passend
vorzeichen- oder nullerweitert — deshalb läuft nach jeder Rechnung eine
Normalisierung. Der Sonderfall dabei: `movzx r64, r/m32` gibt es nicht, ein
`mov exx, exx` nullt die oberen 32 Bit von selbst.

Argumente gehen nach System V in `rdi, rsi, rdx, rcx, r8, r9`; ein Syscall
nimmt die Nummer in `rax` und danach `rdi, rsi, rdx, r10, r8, r9`. `copymem`
wird zu `rep movsb`.

### Ergebnis

`tools/self_compare.sh`, Abschnitt 16 in `test.sh`:

| | |
|---|---:|
| **gleiches Verhalten** | **109** |
| abweichend | **0** |
| fehlerhaft | **0** |
| nicht Kernsprache | 35 |
| `defer` | 1 |
| Codegenerator fehlt (Gleitkomma, > 6 Argumente) | 5 |
| übersprungen (`firnc0` übersetzt nicht einzeln) | 38 |

### Was noch fehlt

* **Gleitkomma.** Es braucht die SSE-Register und eine eigene Klassifikation;
  `firnc0` behandelt `f64` in Stufe 0 ohnehin abweichend von System V
  (SPEC §14.1). Fünf Dateien fallen dadurch heraus.
* **Mehr als sechs Argumente** je Aufruf — die weiteren gehen über den Stapel.
* **`fork`/`execve`** (Punkt 11): `as` und `ld` ruft noch das Skript auf, nicht
  `firnc1`. Ohne das gibt es kein eigenständiges `firnc1`-Programm.
* **Das Modulsystem**: `firnc1` liest genau eine Datei. Für den Fixpunkt müsste
  es `import` auflösen — und der Compiler selbst besteht aus vielen Dateien.

### Stand der Kette

| Teil | Stand |
|---|---|
| `lexer` · `diag` · `ast` + `parser` · `types` + `abi` · `sema` · `fir` · `lower` | ✅ |
| `codegen_x86` | ✅ ohne Registerzuteilung und ohne Gleitkomma |
| `config` · Modulsystem · `fork`/`execve` | `[ ]` |

**Der Weg zum Fixpunkt (SPEC §11, Stufe 2/3) ist damit erstmals sichtbar:**
Modulsystem, Prozessstart, dann `firnc1` auf sich selbst. Was heute schon gilt:
**109 Programme, die kein Rust angefasst hat.**


---

## 18. Runde 28: Prozessstart — Punkt 11 ist zu

`firnc1` ruft `as` und `ld` jetzt **selbst** auf. Damit ist der letzte Punkt
der Liste in §4 erledigt, für den es keinen Umweg gab.

```console
$ ./.firnc1 quelle.fi -o programm
$ ./programm
```

Dazwischen liegt kein Skript mehr: `bin/firnc1.fi` schreibt `programm.s`,
startet `/usr/bin/as` und `/usr/bin/ld` über `fork`/`execve` und wartet mit
`wait4` auf den Endestatus.

### Die eine Zeile, die man nicht vergessen darf

```firn
if kind == 0 {
    syscall(SYS_EXECVE, pfad as i64, argv as i64, 0, 0, 0, 0)
    beende(127)          // <- ohne das laeuft der Compiler zweimal
}
```

Kommt `execve` zurück, ist es **fehlgeschlagen** — und dann läuft das Kind im
Programm des Elternteils weiter. Ohne das `beende` würde bei einem fehlenden
`as` der ganze Compiler ein zweites Mal ablaufen. `tests/700_prozessstart.fi`
prüft genau diesen Fall mit einem Pfad, den es nicht gibt.

Dazu kam `rt.schreib_datei` (`open` mit `O_WRONLY|O_CREAT|O_TRUNC`, Rechte
0755) — der Compiler muss seine Ausgabe ja irgendwo hinlegen.

### Was der Nachweis jetzt wirklich zeigt

`tools/self_compare.sh` ruft **kein Werkzeug mehr selbst auf**. Es startet
`firnc1`, und alles Weitere passiert in Firn:

| | |
|---|---:|
| **gleiches Verhalten wie `firnc0`** | **109** |
| abweichend · fehlerhaft | **0** · **0** |

### Stand der Liste in §4

Von den achtzehn Punkten sind offen: **4** (`Str` mit Verkettung), **5**
(Textformatierung, teilweise), **8** (Methoden), **9** (Schnittstellen),
**10** (`Result`/`?`), **13** (veränderliche globale Zustände) und **18**
(`comptime`-Codeerzeugung). Keiner davon steht dem Fixpunkt im Weg — sie sind
Bequemlichkeit oder gehören zu Erweiterungen, die `firnc1` nicht liest.

**Was dem Fixpunkt im Weg steht, ist etwas anderes:** `firnc1` liest genau
**eine** Datei. Der Compiler selbst besteht aus vierzehn. Das Modulsystem ist
der nächste und vorletzte Schritt.


---

## 19. Runde 29: das Modulsystem — `firnc1` liest mehr als eine Datei

`firnc1` löst `import` jetzt selbst auf. Damit übersetzt der in Firn
geschriebene Compiler auch Programme aus mehreren Dateien — einschließlich
`tests/610_rt.fi`, das den **Laufzeitkern `lib/rt/rt.fi` selbst einbindet**.

| | Runde 28 | Runde 29 |
|---|---:|---:|
| gleiches Verhalten wie `firnc0` | 109 | **113** |
| abweichend · fehlerhaft | 0 · 0 | **0** · **0** |

### Ein Baum, eine Namenstafel, Umbenennung beim Parsen

`firnc0` führt die Dateien nach dem Parsen zusammen und schreibt die Namen mit
einem `Renamer` um (`modules.rs`). `firnc1` macht dasselbe **während** des
Parsens, und das passt besser zu seinem Aufbau:

* **Eine** Namenstafel für alle Dateien. Der Lexer bekommt dafür einen
  Zeiger auf eine gemeinsame `Interner` — ohne das meinte dieselbe Nummer in
  zwei Dateien zwei verschiedene Wörter.
* **Ein** Baum. Der Parser schreibt in einen fremden `ast.Baum`, statt einen
  eigenen anzulegen; damit entfällt jedes Zusammenführen von Indizes.
* **Umbenennung mit Alias.** Eine Vorabsuche sammelt die Namen, die eine Datei
  auf oberster Ebene deklariert; genau die bekommen `alias__` davor. Ein
  qualifizierter Zugriff `modul.name` wird zu demselben `modul__name`.

Die Wurzeldatei wird **zuletzt** geparst und **nicht** umbenannt. Die Module
davor: eine Konstante darf eine aus einem anderen Modul benutzen, also müssen
die Abhängigkeiten vorher im Baum stehen.

### `tests/710_module_core.fi`

Drei Ebenen, und der Name `wert` steht in **allen dreien** — in der
Wurzeldatei, in `kern/mid.fi` und in `kern/deep.fi`. Ohne Umbenennung würde
eine Fassung die andere verdecken. Dazu eine Konstante und ein `struct` aus
einem fremden Modul (als Wert übergeben) und eine Kette über zwei Ebenen
(`mittel` bindet `tief` ein).

### Ehrliche Grenzen

* **Kein Zyklenschutz und keine Wiederverwendung.** Bindet dieselbe Datei
  zweimal ein, wird sie zweimal geparst; ein Zyklus läuft ins Endlose.
  `firnc0` hat dafür eine `seen`-Menge — hier fehlt sie noch.
* **`export` wird gelesen, aber nicht durchgesetzt.** Ein Modul kann alles
  sehen, was ein anderes deklariert.
* Die Suche geht nur eine Ebene: **relativ zur Wurzeldatei**. `firnc0` sucht
  zuerst neben der importierenden Datei (Blocker B3 aus Runde 17) — für die
  Testprogramme ist beides dasselbe, für `lib/rt/vec.fi` nicht.

### Was jetzt noch zwischen hier und dem Fixpunkt liegt

`firnc1` kann mehrere Dateien lesen — aber **nicht seine eigenen**: `lexer.fi`,
`parser.fi` und `sema.fi` benutzen generische Sammlungen (`Vec[T]`, `Map[K,V]`)
und `defer`. Beides liest der Kernparser nicht. Der Weg dorthin ist damit klar
benannt und nicht mehr vage:

1. **Generics** im Parser und in der Monomorphisierung von `firnc1`
2. **`defer`** im Lowering
3. Gleitkomma im Codegenerator

Erst danach kann `firnc1` sich selbst übersetzen.


---

## 20. Runde 30: `defer`, Gleitkomma und Stapelargumente

Drei Lücken zu, und zwar die letzten, die nicht an Generics hängen.

| | Runde 29 | Runde 30 |
|---|---:|---:|
| gleiches Verhalten wie `firnc0` | 113 | **121** |
| wegen `defer` ausgeschlossen | 1 | **0** |
| wegen Codegenerator ausgeschlossen | 5 | **0** |
| abweichend · fehlerhaft | 0 · 0 | **0** · **0** |

### `defer` im Lowering

Ein Stapel je **Blockebene**. Beim Verlassen eines Blocks laufen dessen eigene
rückwärts; bei `return` laufen **alle** Ebenen der Funktion; bei `break` und
`continue` nur die, die **innerhalb** der Schleife vereinbart wurden — dafür
merkt sich jede Schleife die Stapeltiefe beim Betreten.

Der Stapel bleibt beim vorzeitigen Ablaufen **unverändert**: der Block räumt
seine eigene Ebene selbst ab. Was danach noch erzeugt wird, landet im
unerreichbaren Block hinter dem Sprung — doppelt ausgeführt wird nichts.

`tests/720_defer_core.fi` schreibt die Reihenfolge in einen Puffer, statt nur
Aufrufe zu zählen. Dabei fiel eine Eigenschaft auf, die ich falsch erwartet
hatte: **das Argument einer aufgeschobenen Anweisung wird erst beim Ablaufen
ausgewertet**, nicht bei der Vereinbarung. `defer merke(s, 49 + i)` schreibt
also den *späteren* Wert von `i`. Beide Compiler sind sich einig — der Test
steht jetzt mit dieser Einsicht da statt mit meiner Vermutung.

### Gleitkomma im Codegenerator

`xmm0`/`xmm1`, `addsd`/`subsd`/`mulsd`/`divsd`, `cvtsi2sd` und `cvttsd2si`.
Eine eigene Klassifikation an der Funktionsgrenze gibt es **nicht**: Stufe 0
übergibt `f64` in Ganzzahlregistern (SPEC §14.1, bewusste Abweichung von
System V), und genau das tut dieser Codegenerator auch.

Der Vergleich mit `comisd` setzt die Flaggen wie ein *unsigned*-Vergleich, und
**NaN setzt zusätzlich PF**. Nach IEEE-754 ist jeder Vergleich mit NaN falsch —
außer `!=`, das wahr sein muss. Also: bei `!=` wird PF **dazugeodert**, bei
allen anderen wird mit `setnp` **weggeundet**. `tests/590_f64.fi` prüft genau
diese Fälle (NaN gegen sich selbst, `<`, `>`, `>=`, Unendlich, minus null) und
läuft jetzt vollständig durch den Firn-Compiler.

### Stapelargumente

Mehr als sechs Argumente gehen nach System V über den Stapel, in **umgekehrter**
Reihenfolge. `rsp` muss beim `call` 16-ausgerichtet sein — bei ungerader Zahl
von Stapelargumenten liegt deshalb ein Füllwort davor. Die aufgerufene Funktion
findet sie bei `[rbp+16]`, `[rbp+24]`, …

### Was jetzt noch fehlt

**Nur noch Generics.** Von 121 vergleichbaren Programmen fällt keines mehr an
`defer`, Gleitkomma oder der Aufrufkonvention. Die 60 Dateien unter „nicht
Kernsprache" hängen an `enum`/`match`, Fehlerunionen, `gc class`, Attributen,
`comptime` — und an **generischen Vorlagen**, ohne die `firnc1` seine eigenen
Quellen nicht lesen kann (`Vec[T]`, `Map[K,V]`).

Damit ist der Weg zum Fixpunkt auf **einen** Punkt zusammengeschrumpft:
Generics im Parser und in der Monomorphisierung von `firnc1`.


---

## 21. Runde 31: Generics — und der Fixpunkt

**`firnc1` übersetzt sich selbst.** Das Ergebnis ist ein Fixpunkt im engen
Sinn: Stufe 2 und Stufe 3 sind Oktett für Oktett dieselbe Datei.

```
Stufe 1   firnc0 (Rust)  übersetzt  bin/firnc1.fi  ->  .firnc1     888 ms
Stufe 2   .firnc1        übersetzt  bin/firnc1.fi  ->  .firnc2    2070 ms
Stufe 3   .firnc2        übersetzt  bin/firnc1.fi  ->  .firnc3

.firnc2.s == .firnc3.s     147 220 Zeilen Assembler, zeichengleich
.firnc2   == .firnc3       792 240 Oktette, binärgleich
```

Stufe 1 wird **nicht** mitverglichen und muss es auch nicht: `firnc0` hat eine
Registerzuteilung, `lib/firnc1/codegen.fi` nicht. Verglichen wird, was ab
Stufe 2 stabil bleibt — und genau dort hängt das Ergebnis nicht mehr am
Rust-Compiler.

| | Runde 30 | Runde 31 |
|---|---:|---:|
| gleiches Verhalten wie `firnc0` | 121 | **131** |
| abweichend · fehlerhaft | 0 · 0 | **0** · **0** |
| wegen Generics ausgeschlossen | 9 | **0** |
| `firnc1` übersetzt sich selbst | nein | **ja, als Fixpunkt** |

`test.sh`: **628/628**, davon Abschnitt 17 der Fixpunkt. Laufzeit ~3 min.

### Monomorphisierung, nicht Typlöschung

`lib/firnc1/mono.fi` (853 Zeilen) ist die Portierung von `sema_generic.rs`
(Erfassung, Namensschema) und `mono.rs` (Ausprägung). Für **jede benutzte
Typkombination** entsteht eine eigene, vollständig konkrete Funktion bzw. ein
eigener Struct; der Typprüfer sieht danach nur noch gewöhnlichen Code.
Namensschema wie in Stufe 0: `vec_push__i32`, `Vec__ptrmut_u8`.

Vier Dinge, die man dabei nicht raten kann:

* **Die Vorlagen dürfen nicht in die Deklarationslisten des Baums.** Sonst
  stolpert der Typprüfer über `T`, und `--emit=ast-kanon` druckte sie —
  was `firnc0` nicht tut. Ihre *Knoten* stehen sehr wohl im Baum; `mono.fi`
  merkt sich nur die Einstiegspunkte.
* **Im Rumpf einer Vorlage sind Ausprägungen abstrakt.** `Vec[T]` innerhalb
  von `vec_neu[T]` ist noch keine Arbeit, sondern eine Vorschrift. Erst beim
  Nachbauen wird aus `Vec__T` ein `Vec__i32` — und *dann* kommt es auf den
  Arbeitsstapel.
* **Beim Nachbauen müssen Kinder-, Parameter-, Feld- und Anweisungslisten
  lückenlos liegen.** Also erst fertig bauen, sammeln, dann am Stück ablegen.
  Dieselbe Lehre wie im Parser (aus `f(a, g(b))` wurde dort einmal
  `f(a, b, g(b))`).
* **`size_of[T]()` ist kein Aufruf.** Der Typtext wandert in den Aufrufnamen
  (`size_of$i32`), der Typknoten zusätzlich nach `e_zahl`; aufgelöst wird
  beides erst im Typprüfer, der die Structtafel kennt. Ohne die Ersetzung
  *beider* Stellen meldet er „unbekannter typ 'T'".

Beim Selbstübersetzen entstehen daraus: **13 Funktionsvorlagen, 1
Structvorlage, 31 gemeldete Ausprägungen, 28 wirklich erzeugte** (die
Differenz sind die abstrakten, die nur in Vorlagenrümpfen stehen).

### Drei Fehler, die erst das Selbstübersetzen gefunden hat

Alle drei lagen **vor** dieser Runde im Code und wurden von 624 Tests nicht
berührt. Ein Compiler, der sich selbst übersetzt, ist der schärfere Test.

1. **`parser__lx` — ein Parameter, der umbenannt wurde.** Die Umbenennung eines
   Moduls (`alias__name`) traf jeden Bezeichner, der oben deklariert ist —
   auch dann, wenn ihn ein **Parameter verdeckt**. `parser.fi` hat eine
   Funktion `lx(p)` *und* einen Parameter `lx` in `par_neu(lx: u64)`; der
   Parameter hieß danach `parser__lx` und war weg. `modules.rs::Renamer` führt
   dafür eine Liste lokaler Namen — `parser.fi` tut das jetzt auch, mit
   Bereichen für Funktion, Block und `for`. Verdeckt werden **nur Werte**: ein
   Aufruf `lx(p)` meint weiter die Funktion (`is_value = false`), ein Typname
   ohnehin.
2. **Vorwärtsverweise zwischen Structs ergaben still die Größe 0.**
   `struct A { b: B }` **vor** `struct B` rechnete mit `groesse_von(B) = 0`,
   ohne Meldung — das Programm lief und rechnete falsch (`tests/fwd`-Fall:
   `firnc0` gab 0, `firnc1` gab 2). Aufgefallen ist es an der
   Monomorphisierung: `Vec__u32` entsteht **nach** allen handgeschriebenen
   Structs, wird aber von `lexer.Lexer` als Feld benutzt. `types.fi` rechnet
   das Layout jetzt **abhängigkeitsgetrieben** (`struct_layout` mit
   `zustand`-Markierung); über Zeiger wird nicht abgestiegen, damit
   `struct Knoten { naechster: *mut Knoten }` möglich bleibt.
3. **`import` galt nur für die Wurzeldatei.** Ein Modul durfte kein Modul
   einbinden. `tests/640_vec_module.fi` scheiterte still daran, dass
   `modules/vec.fi` sein eigenes `rt` nicht bekam. `bin/firnc1.fi` hält jetzt
   eine **Warteschlange** über den ganzen Einbindungsgraphen (Breitensuche wie
   `modules.rs::resolve`), mit Pfad-Dedup — damit sind auch die in Runde 29
   benannten Grenzen weg: kein Doppeltparsen, kein endloser Zyklus, und
   gesucht wird **zuerst neben der importierenden Datei**, dann neben der
   Wurzeldatei.

### Warum der Parser ein Register braucht

`vec_push[i32](&v, 3)` und `feld[i]` sind bis auf den Namen dieselbe Form. Der
Parser entscheidet **am Namen**, ob ein `[` eine Typargumentliste ist oder eine
Indizierung — also muss vor dem Parsen der ersten Datei feststehen, welche
Namen generisch sind. Deshalb läuft über **alle** Dateien zuerst eine
Vorabsuche nach `fn IDENT [` und `struct IDENT [` (`gen_vorab`), und erst
danach wird geparst. `firnc0` macht es seit Blocker B1 genauso
(`build_program`: erst alle lexen, dann alle parsen).

Der Preis ist ein zweites Lexen jeder Modulquelle. Das ist billiger als alle
Lexer gleichzeitig offen zu halten — und es macht die Reihenfolge der
`import`-Zeilen bedeutungslos.

### `tests/730_generics_core.fi`

Der Test zur Runde, und er prüft genau das, was schiefging: eine Vorlage aus
einem **Modul**, mit `i32`/`u8`/`i64` ausgeprägt; eine Vorlage, die eine
Vorlage ruft — einmal mit demselben Typ, einmal mit einem festen anderen
(`groesse[u8]()` innerhalb von `laenge_von[T]`); `size_of[T]()` im
Vorlagenrumpf; ein Parameter `lx`, der die Funktion `lx` desselben Moduls
verdeckt; ein Struct mit **Vorwärtsverweis** (geprüft wird das *Layout*, nicht
nur dass es übersetzt); Ausprägungen als Feld eines eigenen Structs; und ein
Modul, das ein Modul **neben sich** einbindet und dessen Konstante aus einem
Vorlagenrumpf heraus benutzt.

### Ehrliche Grenzen

* **Die Einzeldatei-Werkzeuge kennen keine Generics.** `.astdump`,
  `.semadump`, `.firdump`, `.layoutdump` melden bei einer generischen Datei
  weiter „keine Kernsprache" (Rückgabewert 3). Das ist kein Verlust am
  Vergleich: `firnc0 --emit=ast-kanon` scheitert an denselben Dateien selbst,
  weil auch sein Parser das Register erst über den Modulweg bekommt — solche
  Dateien zählen dort als *übersprungen*.
* **`export` wird weiter nicht durchgesetzt.**
* **Pfad-Dedup vergleicht Zeichenketten, nicht kanonisierte Pfade.** Wer
  dieselbe Datei über zwei verschiedene Pfade einbindet, bekommt sie zweimal.
  `firnc0` kanonisiert dafür.
* **51 Dateien im Korpus bleiben außerhalb der Kernsprache** — und keine davon
  wegen Generics: `enum`/`match` (10), Fehlerunionen (20), `gc`/`rc` (14),
  Intrinsics für konstante Laufzeit (4), `errdefer` (1), `comptime` (2). Das
  ist Stufe 1, nicht Stufe 0.

### Was das jetzt heißt

`SPEC.md` §11 verlangt für Stufe 3 den Fixpunkt. Der steht — für die
Teilmenge, in der `firnc1` geschrieben ist. Der nächste ehrliche Schritt ist
nicht „mehr Bootstrap", sondern der **Sprachumfang**: solange `firnc1` kein
`enum`/`match` und keine Fehlerunionen liest, kann es `firnc0` nicht ersetzen,
sondern nur sich selbst tragen.


---

## 22. Runde 32: `enum` und `match` — der erste Sprachumfang nach dem Fixpunkt

`firnc1` liest jetzt Aufzählungen und den Musterabgleich — mit derselben
Architektur wie Stufe 0 (`sema_match.rs`): die Fälle eines `match` liegen
**nicht** im Syntaxbaum, sondern in einer Registrierung; im Baum steht nur
ein Aufruf `__match#<nummer>` ohne Argumente. Das Layout einer Aufzählung
wird als Struct mit den Feldern `__tag` und `__v<tag>_<i>` in den Typkontext
eingetragen, die Offsets in `pattern.fi` gerechnet, nicht von `types.fi`.

| | Runde 31 | Runde 32 |
|---|---:|---:|
| gleiches Verhalten wie `firnc0` (selbst) | 131 | **140** |
| abweichend · fehlerhaft | 0 · 0 | **0** · **0** |
| nicht Kernsprache (selbst) | 51 | **42** |
| Parser oktettgleich (`--emit=ast-kanon`) | 169 | **183** |
| Typen gleich (`--emit=typen`) | 115 | **123** |
| FIR oktettgleich (`--emit=fir-raw`) | 115 | **123** (37 845 Instruktionen) |
| Fixpunkt (Stufe 2 == Stufe 3) | 147 220 Zeilen | **173 103 Zeilen, zeichengleich** |

`test.sh`: **628/628**. Die Messlatte der Runde: `tests/200`–`206`, `230`,
`231` laufen durch `firnc1` mit identischem Verhalten — erreicht, genau die
neun mehr. Die vier Negativtests (`match_int_ohne_auffang`,
`match_missing_variant`, `match_unbekannte_variante`, `match_unerreichbar`)
brechen mit `rc=1` ab — einzeln gemessen; die Suite prüft Negativtests nur
gegen `firnc0`.

### Drei Stellen, an denen man das nicht raten kann

1. **Die `__match#`-Nummern der Wurzeldatei müssen die kleinen bleiben.**
   `firnc0` parst die Wurzel zuerst, `firnc1` die Module zuerst (Konstanten
   dürfen modulübergreifend sein). `tests/231_module_match.fi` hat zwei
   `match` im Modul und eines in der Wurzel — `--emit=ast-kanon` zeigt in
   der Wurzel `__match#0`. Der Vergleich läuft nur über die Einzeldatei-
   Werkzeuge, und die parsen genau eine Datei — dort stimmt die Nummer.
   Im vollen Lauf sind die Nummern intern und müssen nur eindeutig sein.
   Beides steht so, und beides ist gemessen.
2. **Die Bindung im Muster ist eine ADRESSE, kein Wert.** `Wert::Zahl(x)`
   bindet `x` an die Adresse des Nutzdatenfelds im Speicher der Aufzählung.
   Das Lowering erzeugt dafür die `ptradd`-Kette VOR dem Rumpf des Falls —
   bei zwei Bindungen zwei `ptradd`, dann erst der Rumpf. Wer die
   Reihenfolge vertauscht, bekommt andere Wertnummern und der FIR-Vergleich
   bricht. (Genau daneben lag ein erster Entwurf: im Nicht-Aufzählungs-Fall
   wurde der Schlüsselwert nie gesetzt — der Switch verzweigte auf `%0`.)
3. **Das Layout verlangt zwei getrennte Eingriffe in `types.fi`.** Die
   Aufzählungsnamen müssen VOR dem Structlayout angemeldet sein (sonst kennt
   `fn dauer(a: Ampel)` den Typ nicht), die Layouts erst NACHHER eingetragen
   werden (eine Aufzählung darf ein Struct dem Wert nach enthalten, nicht
   umgekehrt). Dazwischen steht `typen_aufloesen` — also bekam `Typen` eine
   Vormerkung (`typen_struct_vormerken`, zählt in `eoff`) und ein nachträgliches
   Festlegen (`typen_struct_festlegen`), das die fertigen Felder lückenlos
   anhängt. Der Index im Typkontext und der Index im Baum sind seither
   NICHT mehr dieselbe Zahl; `struct_layout` bekommt beide.

### Portiert, mit Absicht einfacher

* **Kein Sprungtabelle.** `codegen_switch.rs` baut ab acht dichten Marken
  eine Tabelle in `.rodata`; `lib/firnc1/codegen.fi` erzeugt immer die
  Vergleichskette. Verhaltensgleich — der Maßstab dieser Stufe ist das
  Verhalten, nicht der Assemblertext.
* **`u64`-Marken jenseits von `i64::MAX` lassen sich nicht niederschreiben.**
  Stufe 0 rechnet Musterwerte in `i128`; diese Stufe in `i64`. Kein
  Programm im Korpus hat eine solche Marke; es steht in `pattern.fi` und hier.
* **Fehler werden gezählt, nicht beschrieben** — wie überall in Stufe 1.
  Die Vollständigkeitsprüfung (`check_exhaustive`) IST portiert: ein
  fehlender Fall ist ein Fehler, kein Warnhinweis.

### Ehrliche Grenzen

* **`--emit=layout` vergleicht die enum/match-Dateien weiter nicht.** Der
  Maßstab (`layout_canon.rs`) kennt Aufzählungen nicht und druckt ihre Namen
  als `?`; `bin/layoutdump.fi` meldet das Muster-Register deshalb bewusst
  NICHT an, und die Dateien zählen dort als „nicht Kern" (gezählt, nicht
  übergeben: es sind 9 Programmdateien plus `tests/modules/state.fi`).
* **`match` in generischen Vorlagen** ist wie in Stufe 0 ein Fehler; hier
  zählt die Datei zusätzlich als „nicht Kern" (Vorab-Suche).
* **Enum dem Wert nach als Structfeld** bleibt ein Fehler (Zeiger geht);
  enum-in-enum dem Wert nach geht, mit Toposort und Zyklus als Fehler.
* **Der Fixpunkt bleibt vorerst „trivial":** die Quellen von `firnc1`
  benutzen `enum`/`match` selbst noch nicht — die neuen Pfade werden beim
  Selbstübersetzen zwar mitübersetzt (Stufe 2 == Stufe 3 beweist ihre
  Übersetzbarkeit), aber nicht durchlaufen. Ehrlich so, und jetzt möglich:
  nach dieser Runde KÖNNEN die Quellen es.

### Was das heißt

Von den 51 Dateien, die Runde 31 außerhalb der Kernsprache zählte, sind 9
übersiedelt — `enum`/`match` ist der erste Sprachumfang, der NACH dem
Fixpunkt dazukam, und der Fixpunkt steht danach weiter. Der Rest ist
benannt: Fehlerunionen (20), `gc`/`rc` (14), konstante Laufzeit (4),
`errdefer` (1), `comptime` (2).

## 23. Runde 33: Fehlerunionen — `error`, `try`, `catch`, `E!T`

`firnc1` liest jetzt Fehlerunionen — mit derselben Architektur wie Stufe 0
(`errors.rs`/`lower_errors.rs`): eine Registrierung ausserhalb des Baums
(`lib/firnc1/err.fi`), im Baum stehen Aufrufe `__try#` und `__catch#`,
und die Typangabe `E!T` wandert als Platzhalter `__eu#<nummer>` durch den
Baum, bis die Typaufloesung sie gegen die Registrierung aufloest. Die Union
selbst ist ein gewoehnlicher Struct `{ __err: u32, __val: T }` im
Typkontext — Aggregatrueckgabe, ABI und Codegen tragen sie unveraendert;
neu war nur die Senkung von `try`/`catch`/Umwandlung.

| | Runde 32 | Runde 33 |
|---|---:|---:|
| gleiches Verhalten wie `firnc0` (selbst) | 140 | **166** |
| abweichend · fehlerhaft | 0 · 0 | **0** · **0** |
| nicht Kernsprache (selbst) | 42 | **17** |
| Parser oktettgleich (`--emit=ast-kanon`) | 183 | **217** |
| Typen gleich (`--emit=typen`) | 123 | **142** (26 025 Ausdruecke) |
| FIR oktettgleich (`--emit=fir-raw`) | 123 | **142** (40 591 Instruktionen) |
| Fixpunkt (Stufe 2 == Stufe 3) | 173 103 Zeilen | **188 839 Zeilen, zeichengleich** |

`test.sh`: **631/631**. Die Messlatte der Runde waren die zwanzig Dateien
`tests/400`–`419` — erreicht, und fuenf mehr: `tests/550`–`554` (die
`rc`-Dateien) waren nur deshalb „nicht Kern", weil sie Fehlerunionen
benutzen (`AllocError!…`); sie sind mitgewandert. Die elf Negativtests
`tests/neg/err_*` brechen bei `firnc1` einzeln gemessen alle mit `rc=1` ab —
die Suite prueft Negativtests nur gegen `firnc0`.

### Drei Stellen, an denen man das nicht raten kann

1. **Die Typangabe `E!T` geht durch einen Importzyklus.** `types.fi` loest
   Typausdruecke auf, aber die Bedeutung des Platzhalters `__eu#<n>` kennt
   nur `err.fi` — und `err.fi` braucht `types.fi`, um die Union als
   Struct anzulegen. Firn kennt keine Funktionszeiger, also tragen die
   Typen einen Zeiger auf die Registrierung (`typen_fehler_setzen`) und
   `aufloesen` ruft `fehler.fehler_typ` direkt — `types.fi` und
   `err.fi` einander importierend. Dass `modules.rs` Zyklen aufloest,
   steht nirgends; es ist an einem Gegenstueck in `/tmp` gemessen, nicht
   geraten.
2. **Der Zeitpunkt der Union entscheidet ueber den Structindex.** In Stufe
   0 entsteht eine Union erst bei der Aufloesung (`get_or_create_union`),
   NACH allen Structs des Baums. Diese Stufe haelt die Reihenfolge mit zwei
   getrennten Wegen: FehlerMENGEN werden vorgemerkert (sie zaehlen zu
   `eoff`, wie die Aufzaehlungen), Unionen werden angehaengt
   (`typen_struct_anhaengen` zaehlt NICHT an `eoff`) und tragen ihr Layout
   sofort mit. Wer das verwechselt, bekommt andere Structindizes — und der
   Typen-Vergleich bricht. Dieselbe Stelle erklaert die Schutzweiche in
   `groesse_anfordern`: eine Union liegt HINTER dem Zustandsvektor der
   Structphase, ihr Layout steht aber schon — also gilt „jenseits des
   Vektors = fertig".
3. **Die implizite Umwandlung steckt an fuenf Stellen, nicht an einer.**
   `return`, `let`, Zuweisung, Argument und Structfeld bekommen je denselben
   Hook (`coerce_pruefen`, Vorbild `hook_coerce`): Erfolgswert wird
   `__err = 0` plus Wert, Fehlervariante wird `__err = code`. Vergisst man
   das ARGUMENT, kompiliert `nimm(7)` fuer `fn nimm(r: E!i32)` still
   falsch — der Aufrufer uebergibt ein Skalar, die Callee-Seite erwartet
   das Aggregat. Und weil die umgewandelte Form ihren eigenen Ausdruck
   noch einmal schreibt, braucht es die BUSY-Markierung aus Stufe 0,
   sonst laeuft die Senkung endlos im Kreis.

### Ehrliche Grenzen

* **`errdefer` bleibt draussen — jetzt ausdruecklich.** `tests/581`
  verbindet `errdefer` mit Fehlerunionen; bisher hielt es das `error`-
  Schluesselwort fern. `ret_term_fehler` in Stufe 0 laesst auf dem
  Fehlerpfad zusaetzlich die `errdefer`-Anweisungen laufen — das ist eine
  eigene Runde. Deshalb meldet die Vorabsuche `errdefer` jetzt von sich
  aus als „nicht Kern"; ohne die Marke waere `581` still mit falschem
  Verhalten kompiliert worden.
* **`--emit=layout` vergleicht die Fehlerunion-Dateien nicht** — derselbe
  Stand wie bei den Aufzaehlungen: `layout_canon.rs` loest `__eu#<n>`
  nicht auf und druckt `?`; `bin/layoutdump.fi` meldet das Register
  deshalb bewusst NICHT an, und die Dateien zaehlen dort als „nicht Kern".
* **`tests/130_must_consume.fi` bleibt „nicht Kern"** — es braucht die
  Attributsyntax `#[must_consume]` (`attrs.rs`), nicht die Fehlerunionen.
  Was die Runde dafuer mitbringt: ein `E!T`-Wert ist implizit
  must_consume, und das Verwerfen ist ein Fehler (`sema.fi`, Vorbild
  `check_discard`) — einzeln gemessen an `tests/neg/err_discarded.fi`
  (`rc=1`).
* **`E!T` in generischen Vorlagen** ist nicht durchdacht, nur benannt: der
  Platzhalter verweist auf den Typknoten, wie er GESCHRIEBEN steht — eine
  Ersetzung von `T` je Auspraegung sieht ihn nicht. Kein Programm im
  Korpus tut das.
* **Der Fixpunkt bleibt vorerst „trivial":** die Quellen von `firnc1`
  benutzen Fehlerunionen selbst noch nicht — die neuen Pfade werden beim
  Selbstuebersetzen mituebersetzt (Stufe 2 == Stufe 3 beweist das), aber
  nicht durchlaufen.

Neuer Dauer-Test: `tests/740_error_union_core.fi` — Fehlermenge und Union
aus einem Modul (`tests/modules/kern/mid.fi`), `try` ueber zwei Ebenen
durch die Modulgrenze, `catch |e|` mit Fehlervergleich, Umwandlung bei
`return`/`let`/Zuweisung/Argument, Union als Structfeld.

### Was das heisst

Von den 42 Dateien ausserhalb der Kernsprache sind 25 uebergesiedelt —
uebrig bleiben `gc` (9), konstante Laufzeit (4), `comptime` (2),
`errdefer` (1) und die Attribute (1). Die fuenf `rc`-Dateien sind voll-
staendig mitgewandert: sie waren nur ueber ihre Fehlerunionen an die
Erweiterungen gebunden. Die Kernsprache kann jetzt Fehler — der naechste
ehrliche Schritt ist `gc`, der groesste verbleibende Block.


## 24. Runde 35: `comptime` — der Compiler fuehrt Code zur Uebersetzungszeit aus

Parallel zu Runde 34 in einem eigenen git-Worktree gebaut (Branch
`r35-comptime`), weil comptime der isolierteste Restblock war; der Merge
lief fast-forward ohne einen Konflikt.

`lib/firnc1/time.fi` (689 Zeilen) ist ein echter Interpreter nach dem
Vorbild von `compiler/src/comptime.rs`: die comptime-Bloecke der
Wurzeldatei werden zur Uebersetzungszeit ausgefuehrt, ihr erzeugter
Quelltext wird ueber dieselbe Lex/Parse-Maschinerie als Modul ohne Alias
in denselben Baum gehaengt — noch vor der Monomorphisierung, mit
demselben Interner. Der Parser liest `comptime { }` und weicht die
Vorabsuche dafuer auf; der Treiber `bin/firnc1.fi` verdrahtet den Lauf
zwischen Wurzelparser und `mono.gen_lauf`.

Ehrliche Grenzen, benannt statt verschwiegen: comptime in importierten
Modulen meldet `sema_braucht_comptime` weiter (nur die Wurzeldatei laeuft),
und Konstanten, die zur Uebersetzungszeit ausgewertet werden muessten,
aber nicht koennen, bleiben ein separater bekannter Fall.

Messwerte nach dem Merge: `test.sh` 634/634, `selbst_vergleich` 169
verhaltensgleiche Programme (vorher 166) bei 0 abweichend und 0
fehlerhaft, Fixpunkt steht — Stufe 2 == Stufe 3, zeichengleich, 210 324
Zeilen Assembler. Beide Zieldateien (601, 602 — darunter die
UCD-Tabellen-Erzeugung, der haerteste comptime-Fall im Korpus) laufen
identisch zu `firnc0`. Neu: `tests/760_comptime_core.fi` und
`docs/RUNDE35.md`.

Uebrig bleiben: `gc` (9), konstante Laufzeit (4), `errdefer` (1) und die
Attribute (1).

## 25. Runde 34: `gc class` — der groesste Block der Kernsprache

`gc class`, `Gc[T]`, `GcWeak[T]`, `weak`/`stark`, `x.as?[C]` und der
transitive `#[no_gc]`-Pruefer sind portiert (Vorbild `gc.rs`/`nogc.rs`).
Die Registrierung liegt in `lib/firnc1/gc.fi`, der nogc-Pruefer in
`lib/firnc1/nogc.fi`; die Laufzeit `lib/gc/gc.fi` — selbst in Firn
geschrieben — wird als eingebetteter Quelltext (`gctext.fi`) automatisch
eingezogen, sobald irgendwo im Importgraphen `gc class` steht, und liegt
im Wurzelnamensraum: `gc_init()` heisst in jedem Modul `gc_init()`.

Der Fund der Runde (wieder einer, den 600+ Tests nicht fanden): der
GC-Scan im Treiber suchte `gc`/`class`/`AllocError` per `intern_finde` —
Nummern, die nur existieren, wenn die Wurzeldatei die Woerter enthaelt.
Stand `gc class` nur in einem Modul (560 -> modules/dom.fi), lief der
Scan mit -1 und fand nichts: keine Laufzeit, keine AllocError-Menge,
stiller Sema-Fehler. `main.rs` nutzt an derselben Stelle `intern_nummer`
— jetzt hier auch.

Messwerte: `selbst_vergleich` 169 -> 179 verhaltensgleiche Programme
(alle neun gc-Dateien, darunter 510 Zyklus und 560 DOM-Zyklen mit echter
Aufloesung), 0 abweichend, 0 fehlerhaft. Fixpunkt steht: Stufe 2 ==
Stufe 3, zeichengleich, 279 201 Zeilen Assembler. Alle sechs
gc/nogc-Negativtests brechen wie firnc0 ab. Neu: `tests/770_gc_core.fi`
(gc-Klasse nur im Modul, Zyklus unter Wurzel) und `docs/RUNDE34.md`.

Uebrig bleiben: konstante Laufzeit (4), `errdefer` (1), `must_consume` (1).

## 26. Runde 36: ct-Intrinsics, `errdefer`, `must_consume` — die Kernsprache ist vollstaendig

Die letzten drei Bloecke sind portiert. `select(bedingung, a, b)`
(datenunabhaengige Auswahl, cmov statt Spruenge, beide Zweige exakt
derselbe skalare Typ), `secure_zero(zeiger, anzahl)` (Nullung mit
volatile-Store-Semantik, darf nie wegoptimiert werden, SPEC §9.3) und
die ct-Barriere — jeweils mit eigener Erkennung in der Sema VOR dem
Funktions-Lookup, damit eine eigene Funktion gleichen Namens gewinnt.
`errdefer` laeuft nur auf dem Fehlerweg (Fertige-Union-Ablehnung wie
Stufe 0), `#[must_consume]` an Funktionen und Structs meldet verworfene
Ergebnisse. Details in `docs/RUNDE36.md`, Kern-Test
`tests/780_ct_core.fi`.

Messwerte: `test.sh` 640/640, `selbst_vergleich` 186 verhaltensgleiche
Programme bei 0 abweichend und 0 fehlerhaft, Fixpunkt zeichengleich
(284 207 Zeilen Assembler). Alle acht Negativtests der Runde brechen wie
firnc0 ab. Damit steht die GANZE Kernsprache in firnc1: der Compiler in
Firn uebersetzt jedes Kernsprachen-Programm so wie der Rust-Compiler —
und sich selbst.

## 27. Runde 37: der Optimierer-Angriff — html5lib unter der 2x-Marke

Drei Optimierungen im Rust-Compiler (firnc1 enthaelt bewusst keinen
Optimierer; der fir-Vergleich laeuft auf `--emit=fir-raw` VOR jeder
Optimierung, deshalb gab es nichts zu spiegeln): Sprung-Fallthrough und
cmp direkt ins Zielregister (977a2ad), Registerpools rsi/rdi/rdx im
linear scan (ef4e530), Inliner-Korrektheit mit Rekursionsverbot und
Shift-Sofortform (bf13ed4). Tokenizer-Messreihe: html5lib 1,94x ->
1,69x (Ziel <=2x erreicht), realweb 4,82x -> 4,34x (Zwischenziel <=3x
verfehlt). Naechster Hebel: Intervall-Splitting + Coalescing im
Registerallokator (7 391 statische reg->reg-movs, 445 Store/Reload-Paare).

## 28. Runde 38: der Sammler lernt zwei Dinge — Rueckgabe und Scheiben

Stufe 2: komplett leere Chunks JEDER Groessenklasse gehen jetzt ans OS
zurueck (mit Zwei-Sweep-Hysterese gegen munmap/mmap-Pendeln), und eine
Grenzen-Kappe (4 MiB) sorgt dafuer, dass nach einer Grossobjekt-Phase nie
wieder "stille" Heaps ohne Sammlung entstehen. Phasen-Test: RSS-Ende
24 124 KiB (nie fallend) -> 2 112 KiB, Verhalten der Sammlung unveraendert.
Stufe 3: hybrid inkrementelles Sammeln ab 8 MiB Heap — Markieren in
Scheiben von 512 Objekten, Fegen in 2-Chunks-Scheiben, Dijkstra-
Einfuegebarriere, Weiss-Paritaet statt Marken-Reset. Pausen damit
heap-groessen-unabhaengig um 0,5 ms; unter 8 MiB bleibt der atomare
Pfad (Durchsatz kostet der Phasen-Check 8,7 %, Vorgabe war ±10 %).
Wichtiger Nebenbefund (RUNDE38.md): der Optimierer kann ein letztes
Null-Setzen als tot entfernen — Unerreichbarkeit gehoert in eine
Hilfsfunktion, der Scrubber genullt zurueckgekehrte Rahmen.
Finalisierer und Arc[T] sind benannte Restarbeit (Semantik- bzw.
Faden-Entscheidung noetig); der 30-Minuten-Dauerlauf laeuft nach.

## 29. Runde 39: `import std.*` und `f"..."` — Komfort kommt in die Sprache

Modul-Suchpfad in BEIDEN Compilern identisch: neben der importierenden
Datei, neben der Wurzeldatei, `$FIRNLIB`, `<exe>/../lib`
(Installationslayout). Darauf steht `lib/std/` — die Fassade im
C#-Stil ueber den bewaehrten Bausteinen (io, math, str, vec, map, num,
mem), Kern-Test tests/790_std_core.fi. Die String-Interpolation
`f"x = {x}"` zerlegt der Parser ZUR UEBERSETZUNGSZEIT in eine Kette auf
den Fmt-Builder (keine Varargs, kein Laufzeit-Parsen, keine
Verlangsamung), gebaut in firnc0 UND firnc1; Anzeige ist die von i64 —
die ehrlich benannte Grenze der Kernfassung. Kern-Test
tests/791_interpolation_core.fi, drei Negativtests brechen auf beiden
Seiten ab. Verifiziert aus einem /tmp-Projekt per FIRNLIB.

## 30. Runde 40: Regalloc gegen realweb — und ein Alias, der zu weit ging

Vier Hebel, alle mit callgrind belegt (die Wanduhr schwankt hier um
±30 % und hat den ersten Gewinn NICHT gezeigt): Register-Deskriptor
gegen Store->Reload, Zellen-Alias fuer Loads, zwei Schnellwege im
Tokenizer (`eingabe_pruefen` als EIN Bereichstest, `dekodiere` mit
Vorabreservierung), Immediates bis 32 Bit im vollen vorzeichenlosen
Bereich. realweb 4,34x -> 2,68x, html5lib 1,69x -> 1,33x,
Instruktionen realweb -44,4 %. Widerlegt und verworfen: "Textlauf am
Stueck" im Data-State (0,008 %) — der Zustands-`match` ist laengst
eine Sprungtabelle.

## 31. Runde 41: der Preis einer Optimierung, die der Verteiler nicht kannte

Der Zellen-Alias aus Runde 40 liess einen Load das Zellenregister
direkt lesen. Die Registerverteilung war da aber schon gelaufen — sie
kannte die vom Alias VERLAENGERTE Lebensspanne nicht und durfte
dasselbe Register an einen anderen Wert vergeben. In
bin/print.fi/drucke_binop wurde daraus `43 - &tab[start]` statt
`43 - start`: die Laenge unterlief, `rt.buf_wachse` verdoppelte bis
zum Ueberlauf und drehte sich ewig. Wirkung: `.astdump` hing bei JEDER
Datei mit `||` — also bei fast jeder — und test.sh blieb in Abschnitt
12 stehen. Zweites Loch derselben Optimierung: ein `call` zwischen
Load und Verwendung zerstoert caller-saved Register (layoutdump
stuerzte in `intern_finde` mit t=0 ab). Beide Faelle brechen den Alias
jetzt ab; die Korrektur kostet +4 Instruktionen auf 1,297 Mrd.

ZWEI LEHREN, teuer bezahlt:
  * Der Fehler war seit Runde 40 im Baum und 649/649 blieben gruen,
    weil die Dump-Binaries VERALTET wiederverwendet wurden. Ein
    Vergleichswerkzeug, das seinen Massstab nicht neu baut, prueft den
    Stand von gestern.
  * Zwei gleichzeitige Laeufe (Hauptrepo + Worktree) benutzten
    DIESELBEN /tmp-Dateien und ueberschrieben sich die
    Vergleichsausgaben — das sah wie 148 echte Abweichungen aus. Alle
    sechs Vergleichsskripte legen jetzt ein eigenes mktemp -d an.

Dazu der geplante Teil: Histogramm der EINZELNEN Scheiben (7 Typen x
16 Faecher) statt nur Maxima — der Markstapel laeuft nie ueber, das
teure Nachtragen kommt im Dauerbetrieb gar nicht vor. Und die
Markierscheibe endet nach einem ZEITBUDGET von 100 us statt nach 512
Objekten (ein Knoten mit vielen Zeigerfeldern kostet ein Vielfaches
eines Textknotens): Scheiben ueber 128 us von 60 865 auf 137 je 20 s,
im 60-Sekunden-Lauf 99,88 % zwischen 64 und 128 us, Durchsatz
unveraendert.

## 32. Runde 42: die std bekommt Tiefe

Die Fassade aus Runde 39 war breit und duenn — jedes Thema hatte ein
Modul, jedes Modul das Noetigste. Runde 42 fuellt sie: str (suchen,
teilen, verbinden, trimmen, ersetzen, Gross/Klein, Zeichen-Iteration),
num (Ganzzahl <-> Text in beide Richtungen, Basen, Ueberlauferkennung,
f64-Huelle ueber dtoa/strtod), vec (suchen, einfuegen, entfernen,
umkehren, sortieren, binaer suchen), map (Iteration, Schluessel/Werte,
herausnehmen), math (floor/ceil/round, exp/ln/log, Trigonometrie,
gcd/lcm, INF/NAN/EPSILON), io (Zeilen lesen, stdin, Anhaengen,
Zeichen/Hex/bool im Fmt-Builder). Neue Kern-Tests 800-806.
Abnahme im Hauptrepo nachgemessen: test.sh 673/673, selbst 196/0/0,
Fixpunkt Stufe 2 == Stufe 3 zeichengleich (309 468 Zeilen).

## 33. Runde 43: das Tempo-Ziel faellt — und zwar nicht im Tokenizer

Auftrag war `realweb` von 2,68x auf hoechstens 2x gegen `html5ever`. Der
dokumentierte Hebel (Intervall-Splitting im Regalloc) wurde **nicht**
gebraucht; das Profil zeigte den Aufwand woanders.

* **`mem_copy` kopiert wortweise** statt byteweise — realweb -22,2 %,
  html5lib -11,9 %. Die Selbstkosten von `main` fielen von 332,9 auf 45,8
  Mio. Instruktionen; ein einziger Block trug 109 davon.
* **Der Registerpfad kann Stapelargumente** (mehr als sechs Argumente
  landeten bisher immer im Speicher) — realweb -4,3 %.
* **Adressversatz wandert in den Speicherzugriff**, `mov r8, [r8+160]` statt
  Addition davor — realweb -0,8 %.

Instruktionen realweb **1.297.226.150 -> 957.989.680 (-26,15 %)**, html5lib
-13,39 %. Selbst nachgemessen auf dem Merge-Stand mit
`tools/tokenizer/durchsatz.sh`: **realweb 1,54x** (Ziel <= 2,00x erreicht),
**html5lib 0,95x** — auf den Grenzfaellen ist der Firn-Tokenizer damit
schneller als html5ever. Durchsatz realweb 29,25 MB/s.

**Methodische Lehre der Runde:** die Wanduhr streut ueber drei Messpaare
zwischen 2,58x und 2,85x fuer *dieselbe* Binary — der Wert 2,68x aus Runde 41
lag mitten in diesem Band. Belastbar sind nur die auf die Instruktion genau
reproduzierbaren callgrind-Zahlen; die Uhr taugt zur Kontrolle, nicht zum
Beweis. Zwei weitere Hypothesen (Label ohne Sprungziel, Sprungtabelle) wurden
begruendet zurueckgestellt, nachdem das Ziel erreicht war.

## 34. Runde 44: die Aufbauphase ohne Stop-the-World

Bis Runde 43 lief der Sammler unterhalb von `INKR_AB = 8 MiB` Heap atomar —
beim Aufbau einer grossen lebenden Menge waren das drei volle Laeufe mit bis
zu **11,82 ms**. Die Begruendung fuer diese Schwelle war richtig gemessen,
aber **falsch zugeordnet**: die Ursache sass nicht in der Heapgroesse,
sondern im gestueckelten Fegen.

* **Fegen mit Zeitbudget** statt in einem Zug, **Freilisten klassenweise** —
  ohne das benutzte das gestueckelte Fegen wieder frische Chunks statt der
  Freilisten und verdoppelte den Heap.
* **Zweite Uhr (Rechenzeit des Fadens)** neben der Wanduhr in der
  Pausenmessung; nur so laesst sich Fremdlast von echter Pause trennen —
  genau der Fehler, der in Runde 40 die 19-ms-Ausreisser erzeugt hatte.
* Neue Messwerkzeuge: `build.fi` (nullt das Histogramm NICHT, misst also
  auch die Aufbauphase), `durchsatz.fi` (feste Arbeit, gemessene Zeit),
  `ab.fi` (A/B im selben Prozess).

Ergebnis: laengste Unterbrechung **11,82 ms -> 0,45 ms** (5-s-Lauf), 0,62 ms
reine Rechenzeit im 10-Minuten-Dauerlauf. Durchsatzverlust 2 % bei kleiner,
0 % bei grosser lebender Menge. `tests/771_gc_build_without_stw.fi` prueft das
deterministisch ueber `gc_volle_laeufe() == 0` statt ueber einen
Zeitvergleich — der waere auf belasteter Maschine wertlos.

## 35. Runde 45: Methoden — `impl`

Bis hierher hatte Firn ausschliesslich freie Funktionen; das Praefix war der
Typ, von Hand hingeschrieben und ungeprueft (`bytes_push(&b, x)`). Jetzt:
`b.dazu(x)`, `quelle.trimme().laenge()`.

`impl` ist bewusst eine **Schreibhilfe**: `a.f(b)` wird nach `Typ_f(&a, b)`
aufgeloest, es gibt keinen dynamischen Versand und keine Vtable. Umgesetzt in
beiden Compilern im Gleichschritt — firnc0 mit neuer `compiler/src/impls.rs`
(490 Zeilen) plus Haken in Parser, Typpruefer, Lowering und nogc-Pruefung;
firnc1 in `parser.fi`, `sema.fi`, `lower`, `nogc.fi`. Dazu eine `impl`-Huelle
fuer `std.str`, Tests 810-812 und **acht Negativtests** (Empfaenger ohne
Adresse, Empfaenger ist Zeiger, kein struct, freie Funktion ist keine
Methode, falsche Argumentzahl/-typen).

## 36. Der Merge der Runden 43-45 — und dieselbe Falle zum vierten Mal

Die drei Runden liefen parallel in getrennten Worktrees mit sauber
getrennten Revieren (Regalloc/Codegen · GC · Parser/Sema) und liessen sich
bis auf einen `.gitignore`-Konflikt konfliktfrei zusammenfuehren.

Die Abnahme meldete danach **eine** Abweichung: `771_gc_build_without_stw.fi`,
firnc0 gab 0, firnc1 gab 3 (`gc_volle_laeufe() != 0`). Mit **frisch
gebautem** `.firnc1` war der Test dreimal hintereinander gruen. Ursache war
wieder ein wiederverwendetes Binary: `tools/self_compare.sh` baute
`.firnc1` nur, **wenn es fehlte** — nach dem Merge verglich es also einen
Compiler, den es nicht mehr gab.

Das ist derselbe Fehler wie bei den Dump-Binaries (Runde 41, dort behoben).
`self_compare.sh` baut `.firnc1` jetzt auch dann neu, wenn firnc0 oder
irgendeine Quelle unter `bin/` oder `lib/` juenger ist.
**Regel: nie ein Binary wiederverwenden, nur weil es existiert.**

**Abnahme des Merge-Stands im Hauptrepo, selbst gemessen:** `test.sh`
**696/696** · `self_compare.sh` **201 gleich / 0 abweichend / 0
fehlerhaft**, CODEGEN FEHLT 0 · `fixpunkt.sh` Stufe 2 == Stufe 3,
zeichengleich, **328.343 Zeilen** · Durchsatz realweb **1,54x**, html5lib
**0,95x**.

## 37. Runde 46: Schnittstellen — `interface` und dynamischer Versand

Runde 45 hatte Methoden nur als Schreibhilfe gebracht: `x.m(a)` wurde nach dem
**statischen** Typ zu `Typ__m(&x, a)`. Runde 46 ergänzt den Fall, den `SPEC.md`
§6.2 seit v0.1 fordert — **eine Aufrufstelle, viele Typen**:

```firn
interface Flaeche { fn flaeche(*self) -> i64 }
impl Flaeche for Rechteck { … }
let f: dyn Flaeche = (&r) as dyn Flaeche
f.flaeche()      // welcher Code laeuft, steht erst zur Laufzeit fest
```

`dyn I` ist ein Doppelzeiger (Datenzeiger + Methodentafel). Die Tafeln stehen
als `.L__iface.<I>.<T>` in `.rodata`. Zwei neue FIR-Instruktionen tragen das:
`O_CALLI` (Aufruf ueber einen Zeiger) und `O_VTAB` (Adresse einer Tafel) — in
beiden Uebersetzern identisch, 14 Negativtests decken fehlende Methoden,
falsche Signaturen, doppelte `impl` und unbekannte Schnittstellen ab. Der
Sammler erreicht die Objekte hinter dem Doppelzeiger weiterhin; ein eigener
GC-Test sichert das.

## 38. Runde 47: Finalisierer, `Arc[T]` und schwache Felder

Die seit Runde 38 benannte Restarbeit an der Speicherverwaltung. Finalisierer
laufen beim Einsammeln, Wiederbelebung ist definiert, der Sammler bleibt dabei
nicht-reentrant. Schwache Felder werden beim Einsammeln **wirklich genullt**
(eigener Test). `Arc[T]` bekam ein neues, unteilbares Primitiv: `O_ATOMADD` →
`lock xadd qword ptr [rcx], rax`, eine Instruktion; ohne das waere `Arc` nur
`Rc` mit anderem Namen.

**Messung:** im 150-s-Lauf mit Finalisierern **0 von 253 698** Unterbrechungen
ueber 1 ms (in Rechenzeit gemessen) — die Pausen sind nicht schlechter
geworden, sondern rund 2 % besser. Callgrind war hier untauglich: es
verschiebt den Stapel, weshalb die Runde den Stapelboden jetzt aus
`/proc/self/maps` liest statt ihn zu raten.

## 39. Runde 48: Pakete — Manifest, Sichtbarkeit, `--paket`

Bis hierhin gab es nur `import a.b` und die Umgebungsvariable `FIRNLIB`.
Runde 48 bringt ein Manifest `firn.paket` — **bewusst kein TOML**: das Format
hat sechs Schluesselwoerter, ist zeilenweise und laesst sich ohne Fremdparser
in beiden Uebersetzern lesen (`compiler/src/package_world.rs` und
`lib/firnc1/package.fi`). Dazu eine deterministische Suchreihenfolge
(Projektquellen → Abhaengigkeiten → `FIRNLIB` → Compilerverzeichnis) mit
klaren Fehlern bei Zyklen, fehlenden Paketen und Namenskonflikten, sowie
oeffentlich/privat auf Modulebene. `--paket` uebersetzt ein Projekt anhand
des Manifests; zusammen mit einer Quelldatei wird es in **beiden** Uebersetzern
gleich abgelehnt. Neuer Abschnitt 18 in `test.sh`: `tools/packages/run.sh`,
**21 Faelle durch beide Uebersetzer**.

## 40. Der Merge der Runden 46-48

Ein einziger echter Konflikt, und zwar ein interessanter: R46 und R47 hatten
**beide** eine neue FIR-Instruktion mit der Nummer 15 vergeben (`O_CALLI` bzw.
`O_ATOMADD`). Da Zweige die Nummernvergabe nicht sehen koennen, ist das kein
Fehler der Runden, sondern der Preis paralleler Arbeit am selben
Instruktionssatz — beim Zusammenfuehren umnummeriert auf `O_ATOMADD = 15`,
`O_CALLI = 16`, `O_VTAB = 17`. **Lehre:** neue FIR-Opcodes gehoeren in einen
reservierten Bereich pro Runde, sonst kostet jede Parallelrunde diesen Konflikt.

**Abnahme des Merge-Standes, im Hauptrepo selbst gemessen:** `test.sh`
**751/751** · `selbst_vergleich` **213 gleich / 0 abweichend / 0 fehlerhaft** ·
Fixpunkt zeichengleich (**427 401 Zeilen** Assembler, 2 459 904 Oktette) ·
Pakete 21/21.

## 41. Runden 50, 51, 52, 53 und ihr Merge

Vier parallele Runden auf Basis `cc1710f`, einzeln jeweils konfliktfrei
gegen `main`, gemeinsam in dieser Reihenfolge gemergt: r51-tempo,
r53-gcvec, r50-generik, r52-freistehend.

**Stand nach dem Merge (selbst gemessen, nicht aus Worker-Meldungen
uebernommen):**

| Pruefung | vorher | nachher |
|---|---|---|
| `test.sh` | 751/751 | **819/819** |
| `self_compare.sh` | 213/0/0 | **225 gleich / 0 abweichend / 0 fehlerhaft** |
| Fixpunkt | zeichengleich | **zeichengleich, 495.250 Zeilen** |
| Tokenizer realweb (callgrind) | 957.989.680 | **699.459.494** |

`CODEGEN FEHLT` steht erstmals auf **0** — die alte Luecke „Gleitkomma,
mehr als sechs Argumente" im Compiler in Firn ist geschlossen.

**Der einzige echte Merge-Konflikt** lag in `codegen_x86.rs`, `Term::Ret`:
Runde 51 hat das `xor eax, eax` vor dem Ruecksprung einer void-Funktion
ersatzlos gestrichen (4.229.623 nutzlose Instruktionen im Messlauf), Runde
52 hatte an derselben Stelle die Bedingung `!f.interrupt` eingezogen, damit
Unterbrechungsbehandler `rax` nicht anfassen. Aufgeloest zugunsten von
Runde 51: wo gar nichts mehr geschrieben wird, ist die Ausnahme fuer
Interrupts gegenstandslos — das ist strikt staerker, nicht schwaecher.

**Der Fund beim Nachpruefen: Runde 52 war unvollstaendig.**
`tools/freestanding/run.sh` bindet den Kernel gegen `demos/kernel/start.s`
— diese Datei existierte nie. Ursache ist Zeile 2 der `.gitignore`: das
Muster `*.s` fiel fuer erzeugten Assembler gedacht, verschluckte aber auch
den handgeschriebenen Boot-Vorspann. Der Worker sah in seinem Worktree eine
funktionierende Datei und meldete gruen; im Hauptrepo fehlte sie, und die
Abschnitte 3 und 3b (Binden, QEMU-Boot) schlugen fehl — also genau der
Nachweis, auf den es in dieser Runde ankommt.

Nachgetragen wurde ein vollstaendiger Vorspann (Multiboot-Kopf,
Seitentabellen zur Laufzeit gebaut und genullt, 1 GiB identisch mit
2-MiB-Seiten abgebildet, PAE, EFER.LME, CR0.PG, 64-Bit-GDT, Fernsprung in
den langen Modus, dann `KERN_START`) plus die Ausnahme
`!demos/kernel/start.s` in der `.gitignore`. Ergebnis:
**FREISTEHEND 41/41**, der Kernel **bootet in QEMU aus beiden Compilern**
und gibt seriell aus.

**Lehre (dritte Auspraegung derselben Regel):** eine gruene Worker-Meldung
beweist nur, dass es im Worktree des Workers lief. Erst Abschnitt 19 im
Hauptrepo beweist, dass es im Repo liegt. Nach `.gitignore` ist beim Merge
kuenftig zu sehen, wenn ein Zweig Dateien anlegt, die kein Erzeugnis sind.

## 42. Runde 54 (DOM), Runde 49 (Faeden) und der schwerste Merge bisher

`r54-dom` ging bis auf die `.gitignore` konfliktfrei ein. `r49-threads`
nicht: **13 Konflikte**, davon drei echte Kollisionen — zweimal hatten beide
Zweige DIESELBE Nummer vergeben, ohne voneinander zu wissen.

**Stand nach dem Merge (im Hauptrepo gemessen):** `test.sh` **846/846** ·
selbst_vergleich **232 gleich / 0 abweichend / 0 fehlerhaft** · Fixpunkt
zeichengleich, **561.666 Zeilen** · Faden-Dauerlauf 60 s: 1.171.122 Runden,
11.903 Sammellaeufe, 75.844 Anhalter, **RSS-Drift −128 KiB**.

### Die drei echten Kollisionen

1. **Slots im Zustandsblock.** Runde 53 legte `S_SLOTS_TID`, `S_SCHEIBE`,
   `S_ZBUDGET` auf 1960/1968/1976 — genau dorthin, wo Runde 49 ihre
   Fadentafel (`S_FADEN_TAB` …) hingelegt hat. Runde 53 zieht auf
   2120/2128/2136 um (frei, unterhalb `REG_SAVE_OFF` = 3968).
2. **Bit 8 im Quellscan.** `gc_quelle_scan` meldete in Runde 53 mit Bit 8
   „das Programm braucht GcVec/GcMap", in Runde 49 „das Programm bringt
   einen eigenen `__faden_arbeit` mit". Bit 8 bleibt bei den Sammlungen (es
   zaehlt auch aus MODULEN), der Fadenverteiler zieht auf **Bit 16** (zaehlt
   wie Bit 4 nur aus der Wurzeldatei).
3. **`laufzeit_quelle`** hat jetzt **vier** Parameter statt drei; beide
   Runden hatten den dritten fuer sich beansprucht.

Die Lehre aus §40 (reservierte Opcode-Bereiche) greift also zu kurz:
**reserviert gehoert jede fortlaufend vergebene Nummer** — Opcodes,
Slot-Offsets, Bitmasken, Testnummern. Die Faden-Tests hiessen 840–842 und
trafen damit auf 840–842 der Sammlungen; sie heissen jetzt 860–862.

### Der interessanteste Fund: ein Test, der vom kaputten Registerscan lebte

`842_gcmap_grund` schlug nach dem Merge fehl: nach dem Loeschen von 1000
Eintraegen blieben **768** Objekte am Leben. Kein Leck — die Ursache ist
Runde 49, die den **konservativen Registerscan repariert** hat: bis dahin
scannte `__gc_collect_now` die ersten 48 Oktette des Zustandsblocks statt
des Rettungsbereichs, der Scan lief also ins Leere. Seither halten alte
Bitmuster zwei fruehere Slot-Puffer der Karte fest, und die halten ihre
Werte.

Gepruefte Gegenprobe, in dieser Reihenfolge:

* Registerscan versuchsweise abgeschaltet → **unveraendert 768** (also nicht
  der Scan selbst),
* sechs statt zwei `gc_collect()` → **unveraendert** (also kein
  unvollendetes Fegen),
* Abschnitt in eine eigene Funktion ausgelagert, danach Registerwaesche
  durch tiefe Rekursion → **unveraendert**,
* `gc_stapel_saeubern()` vor der Messung → **0**.

Damit ist es kein Fehler, sondern der bekannte Preis eines konservativen
Sammlers: er DARF tote Objekte behalten. Wer das Gegenteil behauptet, muss
zuerst den toten Stapel saeubern — `tests/833` und `lib/dom` tun das seit
Runde 49, `842_gcmap_grund` tut es jetzt auch.

### Zwei Nebenbefunde

* Ein Rust-Modultest (`schmales_add_wird_nicht_zur_adresse`) suchte nach
  `add e…` und uebersah `add r10d` — der Merge verschob nur die
  Registerwahl. Der Test war zu eng, nicht der Code falsch.
* `tests/neg/arc_discarded.fi` erwartete `416:5`; die Faden-Erweiterung in
  `tests/modules/rc.fi` hat die erzeugte Datei verlaengert (jetzt `441:5`).
  Solche Positionen gehoeren in den Rumpf unter `lib/rc/parts/`, nicht in
  die erzeugte Datei.
