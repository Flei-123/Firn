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
| 11 | **Prozessstart** (`fork`/`execve`-Hülle) | `[~]` Aufrufargumente seit Runde 21 (`fn main(start: u64)`), `fork`/`execve` fehlen | `firnc` ruft `as` und `ld` auf |
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

`tools/lex_vergleich.sh` vergleicht nicht mehr nur den Tokenstrom, sondern auch
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
`lies_datei` erwartet. Nachweis: `tests/660_argumente.fi`.

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
statt den Baum. `compiler/src/ast_kanon.rs` erzeugt deshalb eine
**sprachneutrale** geklammerte Form:

```text
(fn u64_nach_f64 ((param m u64)) f64 (blk (ret (as (id m) f64))))
```

Gedruckt wird **nur die Wurzeldatei**, vor dem Zusammenführen der Module und
vor der Monomorphisierung — der Parser in Firn sieht ebenfalls genau eine
Datei.

### Ergebnis

`tools/parser_vergleich.sh`, Abschnitt 12 in `test.sh`:

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
und Größe, ohne Heap und ohne Text. `lib/firnc1/typen.fi` löst das ein.

### Warum ausgerechnet die zwei als Nächstes

Layout und Aufrufkonvention sind die Stellen, an denen ein Compiler **still
falsch** wird. Ein Feldversatz daneben, ein Aggregat in Registern statt im
Speicher — das Programm läuft, nur eben falsch, und der Fehler zeigt sich
irgendwo ganz anders. Zwei unabhängige Umsetzungen gegeneinander zu stellen
findet hier mehr als jeder ausgedachte Testfall.

### Der Maßstab: `--emit=layout`

`compiler/src/layout_kanon.rs` druckt je Struct Größe, Ausrichtung und **jeden
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

`tools/typen_vergleich.sh`, Abschnitt 13 in `test.sh`:

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

* `typen.fi` löst nur auf, was die **Kernsprache** kennt. `enum`-Layout
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

`tools/sema_vergleich.sh`, Abschnitt 14 in `test.sh`:

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
`ast.fi` und `typen.fi` kamen aus dem Git zurück, `sema.fi` und `druck.fi`
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

### `tests/690_lowering_kern.fi`

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
