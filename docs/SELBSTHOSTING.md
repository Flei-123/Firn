# Selbst-Hosting: Plan und ehrlicher Stand

**Anforderung:** `L1` · `SPEC.md` §11 (Bootstrap-Plan) · `ABNAHME.md` Punkt 1
**Stand:** Stufe 0 (`firnc0`, in Rust) läuft. Stufe 1 ist **nicht begonnen**.
Diese Datei sagt, *was heute schon in Firn schreibbar wäre* und *was konkret
fehlt* — mit Liste, nicht mit Gefühl.

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
| 2 | **`Vec[T]`** (wachsendes Feld) | **`[x]`** als Bibliothek `lib/rt/vec.fi` (Runde 18, `tests/640_vec_modul.fi`) | Tokenstrom, Anweisungslisten, Blocklisten — überall |
| 3 | **Hash-Abbildung `Map[K,V]`** | **`[x]`** `lib/rt/map.fi` + `lib/rt/intern.fi` (Runde 19) | Namenstabellen (`fns`, `consts`, Bereiche) |
| 4 | **Zeichenketten** `Str`/`Bytes` mit Verkettung | `[~]` `rt.Buf` + `intern.Interner` (Runde 19); es fehlt ein `Str`-Typ mit Verkettungsoperator | Bezeichner, Fehlermeldungen, Assemblertext |
| 5 | **Textformatierung** (`format`-Ersatz) | **`[~]`** `buf_push_dez_u64/i64`, `buf_push_hex_u64` in `lib/rt/` | Jede Diagnose und der gesamte Assembler-Ausdruck |
| 6 | **Summentypen + `match`** | **`[x]`** ohne Typparameter (`tests/201_enum_payload.fi`); `enum Name[T]` fehlt (Runde 20 geprueft) | `TokKind`, `ExprKind`, `Op`, `Term` sind alle Summentypen |
| 7 | **Rekursive Datentypen** (`Box`-Ersatz) | **`[x]`** über `*mut` auf den eigenen Typ, in Runde 20 nachgeprüft | `Expr` enthält `Expr`; heute nur über Zeiger + Allokator |
| 8 | **Methoden / `impl`** | `[ ]` | Kosmetik, ersetzbar durch freie Funktionen mit erstem Parameter |
| 9 | **Schnittstellen / dynamischer Versand** | `[ ]` | Für Stufe 1 **nicht** nötig |
| 10 | **Fehlerbehandlung** (`Result`, `?`) | `[ ]` | Ersetzbar durch Summentyp + `match`, sobald 6 steht |
| 11 | **Prozessstart** (`fork`/`execve`-Hülle) | `[ ]` | `firnc` ruft `as` und `ld` auf |
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

Nachweis: `tests/640_vec_modul.fi` (1.000 `i32`, 300 `u8`, 100 `u64`, `pop`,
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

Nachweis: `tests/650_map_modul.fi` (1.000 Einträge über mehrere Verdopplungen,
Überschreiben, Löschen **und Suchen hinter dem Grabstein**, Wiedereinfügen,
Durchlauf, drei Ausprägungen mit ungleichen Größen, negative Schlüssel,
`map_reserve` ohne Umstreuen) und `tests/651_intern_modul.fi` (Präfixe „ab" vs.
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
`tools/lex_vergleich.sh` lässt beide über `tests/`, `lib/`, `bin/` und
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
Zielbereichs). Rückfalltest: `tests/591_f64_umwandlung.fi`, der wie jeder
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
die Zeile bleibt in `tools/lex_vergleich.sh` stehen, bis sie da ist.

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
