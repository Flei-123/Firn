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
| 2 | **`Vec[T]`** (wachsendes Feld) | `[~]` Generics stehen; `rt.Buf` ist die Byte-Fassung (wächst durch Verdopplung), die typisierte fehlt | Tokenstrom, Anweisungslisten, Blocklisten — überall |
| 3 | **Hash-Abbildung `Map[K,V]`** | `[~]` | Namenstabellen (`fns`, `consts`, Bereiche) |
| 4 | **Zeichenketten** `Str`/`Bytes` mit Verkettung | `[~]` Modul `str` dieser Runde | Bezeichner, Fehlermeldungen, Assemblertext |
| 5 | **Textformatierung** (`format`-Ersatz) | **`[~]`** `buf_push_dez_u64/i64`, `buf_push_hex_u64` in `lib/rt/` | Jede Diagnose und der gesamte Assembler-Ausdruck |
| 6 | **Summentypen + `match`** | `[~]` Modul `types` dieser Runde | `TokKind`, `ExprKind`, `Op`, `Term` sind alle Summentypen |
| 7 | **Rekursive Datentypen** (`Box`-Ersatz) | `[ ]` | `Expr` enthält `Expr`; heute nur über Zeiger + Allokator |
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
