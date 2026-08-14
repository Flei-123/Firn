# Firn — Fahrplan

**Stand:** 2026-08-14 (v0.2) · **Bezug:** `SPEC.md`, `DESIGNZIELE.md`, `ABNAHME.md`,
`../karstos-browser/FIRN-ANFORDERUNGEN.md`, `../karstos-browser/PLAN-FIRN.md`
Zeitangaben = Arbeitsaufwand einer Person mit KI-Unterstützung, nicht Kalenderzeit.

---

## Was sich gegenüber v0.1 geändert hat

Firn ist seit der Browser-Entscheidung (**B1**: jede Zeile ausführbarer Code der
Karstos-Browser-Engine ist Firn) **kritischer Pfad Nummer 1** des gesamten
Ökosystems. Das ändert den Fahrplan an drei Stellen:

* **Neue Pflichtteile:** Opt-in-GC, WTF-16-Zeichenketten, Constant-Time-Primitive,
  Abwicklung für JS, Kompilierzeit-Codegenerierung, Debugger, Paketverwaltung.
* **Neues Leistungsziel:** ≤ 2× Rust auf Mikrobenchmarks. Das verschiebt Arbeit
  vom Sprachumfang in den Optimierer.
* **Gestrichen:** aarch64- und WASM-Backend haben keinen Termin mehr
  (`FIRN-ANFORDERUNGEN.md` §11 braucht beides nicht). Das entlastet spürbar.

**Zwei Härtetests entscheiden alles** und sind deshalb vorgezogen:
HTML5-Tokenizer (100 % html5lib, ≤ 2× Referenz) und DOM-Prototyp mit Zyklen
(24 h ohne Speicherwachstum). Fallen sie durch, wird nicht der Browser
repariert, sondern Firn.

---

## Fundamentarbeit aus DESIGNZIELE.md (neu, 14.08.2026)

`DESIGNZIELE.md` prüft zehn bekannte Schwachstellen heutiger Sprachen und trennt,
was **jetzt** ins Fundament muss (später unmöglich) von dem, was **additiv**
nachrüstbar ist. Sechs Punkte betreffen den Compiler direkt und sind unten in die
Phasen eingearbeitet:

| Fundamentpunkt | Warum jetzt | Phase |
|---|---|---|
| **Kein `async`-Schlüsselwort**, Codegen ohne Annahme über Stapelstetigkeit | Farbe wäre später nicht mehr zu entfernen; `Io` als Parameter braucht Stapelwechsel | 2 (Regel), 3–4 (Umsetzung) |
| **`!T` + `#[must_consume]`**, Regel „jede Allokation ist fehlbar" | Rust-for-Linux belegt: nicht nachrüstbar | 2 → 3 |
| **Ergebnisort-Operand in FIR und Lowering** | Teuerste Fundamentarbeit — jetzt hat das Lowering ~2.000 Zeilen, später 20.000 | **2** |
| **Feldzugriff vom Speicherort trennen** (Vorbedingung für SoA) | Solange `a.b` fest „Basis + Versatz" heißt, ist SoA tot | 2/3 |
| **Optimierungsdurchgänge einzeln schaltbar, mit Etikett „debugerhaltend"** | sonst ist die `--dev-fast`-Stufe später ein Umbau jedes Durchgangs | 2 → 3 |
| **Prüfphasen wiedereintrittsfähig**, FIR interpretierbar | Vorbedingung für `comptime`/`emit` | 2 → 3 |

Nachrüstbar und deshalb **nicht** eingeplant: stabiles ABI (nur ein
Symbol-Namensschema als Vorleistung in Phase 3), Hot Reload (kein Termin, siehe
`DESIGNZIELE.md` §9 — die ehrliche Einschätzung lautet: lohnt sich nicht).

---

## Wie realistisch ist das?

| Sprache | Erster Compiler | Version 1.0 / stabil | Dauer |
|---|---|---|---|
| Rust | 2006 (Graydon, in OCaml) | Mai 2015 | **9 Jahre** |
| Zig | 2015 | noch nicht (0.16, 2026) | **11+ Jahre** |
| Go | 2007 | März 2012 | 5 Jahre, mit Google-Team |
| Odin | 2016 | noch nicht stabil | 10 Jahre |

Firn wird nicht schneller fertig, nur weil KI mitschreibt. KI beschleunigt das
Tippen, nicht die Entwurfsentscheidungen und nicht das Finden der Fehler, die
erst bei 50.000 Zeilen echtem Code auftauchen. Die frühen Phasen (Parser,
Typprüfer, Codegen für eine Teilmenge) schrumpfen von Monaten auf Tage. Die
späten Phasen (Selbst-Hosting, Optimierer auf ≤ 2× Rust, GC im Dauerlauf,
Stabilität) schrumpfen kaum.

**Ehrliche Erwartung:** *Nutzbar für kleine Karstos-Systemprogramme* in 6–12
Monaten. *Selbst-hostender Compiler* in 1–2 Jahren. *Abnahme nach
`FIRN-ANFORDERUNGEN.md` §13 bestanden* — also bereit für die erste
Browser-Bibliothek — realistisch **2–4 Jahre**. `PLAN-FIRN.md` veranschlagt für
Phase F0 allein 27 Personenmonate.

---

## Phase 0 — Spezifikation ✔

* `SPEC.md` v0.1: Profile, Besitzmodell, Fehlerbehandlung, `comptime`,
  Backend-Strategie, Bootstrap, Grammatik.
* `SPEC.md` v0.2: Speichermodell in drei Stufen mit **Opt-in-GC**, Vererbung für
  `gc class`, WTF-16, Constant-Time, Abwicklung, Leistungsziel, Rückverfolgung.
* `ABNAHME.md`: die sechs Prüfpunkte aus `FIRN-ANFORDERUNGEN.md` §13 als
  abhakbare Liste.

## Phase 1 — `firnc0`: Prototyp in Rust ✔

Teilmenge aus `SPEC.md` §14, wirklich bis zum laufenden Binary.
Lexer, Parser, Typprüfer, FIR, Konstantenfaltung + DCE, x86_64-Codegen ohne
LLVM, `syscall`, 75 Testprogramme × 2 Durchläufe + 15 Negativtests, alle grün.
**Ergebnis:** kompilierbare Sprache, noch kein Werkzeug.

## Phase 2 — v0.2: Sprachkern für Browser-Code `← hier stehen wir`

Was der Browser vom *Sprachkern* verlangt, ohne Laufzeit und ohne Bibliothek.

* **Summentypen + `match`** mit Vollständigkeitsprüfung, Sprungtabellen (`L4`, `P4`)
* **Generics** durch Monomorphisierung (`L5`)
* **Zeichenketten**: `Bytes`, `Str` (UTF-8), **`Str16` (WTF-16)**, `Atom`,
  korrekt gerundetes `strtod`, kürzeste Double-Ausgabe (`Z1`–`Z6`)
* **Optimierer**: Inlining, echte Registerzuteilung, DCE, Bereichsprüfungen
  entfernen — mit **gemessenem** Vergleich gegen Rust (`P1`–`P3`, `P5`, `P9`)
* **`secret[T]` + `#[constant_time]`**, `secure_zero`, `u128` (`C1`–`C3`, `C5`)
* **Speichermodell**: `Rc[T]`/`Weak[T]`, `Gc[T]`/`GcWeak[T]`, `gc class`,
  `#[no_gc]` (`S1`–`S3`, `S7`)
* `break`/`continue`, `for`, `defer`, `drop`, Move-Prüfer, Referenztypen
* **Härtetest 1**: HTML5-Tokenizer gegen html5lib
* **Härtetest 2**: DOM-Prototyp mit Zyklen im Dauerlauf
* Testrunner mit maschinenlesbarer Ausgabe (`W2`)
* **Fundamentarbeit aus `DESIGNZIELE.md`** — vor allem anderen:
  * **Ergebnisort** (`DESIGNZIELE.md` §6): Aggregatrückgabe schreibt direkt ans
    Ziel, Zielort-Operand in FIR. Zuerst prüfen, was `compiler/src/abi.rs` heute
    tut
  * **Feldzugriff ↔ Speicherort trennen** (§8): Zwischenschicht im Lowering
    statt fest verdrahtetem „Basis + Versatz"
  * **Durchgangsregister** (§5): jeder Optimierungsdurchgang bekommt Name,
    Schalter und Etikett *debugerhaltend ja/nein*; Zeileninfo überlebt jeden
    Durchgang
  * **Prüfphasen wiedereintrittsfähig** (§7): „prüfe diese neu entstandene
    Funktion" muss möglich sein
  * **Regel festschreiben**: kein `async`-Schlüsselwort, keine unfehlbare
    Allokationsfunktion, keine Ambient-Autorität in der Bibliothek
* **Aufwand:** Monate, nicht Wochen. Das ist der eigentliche Brocken.

**Zwischenstand 13.08.2026 (Runde 2 zusammengeführt), ehrlich:**

| Punkt der Liste oben | Stand |
|---|---|
| Summentypen + `match` + Sprungtabellen | **fertig und geprüft** |
| Generics (Monomorphisierung) | **fertig und geprüft** |
| Zeichenketten `Bytes`/`Str`/`Str16`/`Atom`, `strtod`, kürzeste Ausgabe | **fertig** (ohne Stringliterale im Lexer) |
| Optimierer + gemessener Rust-Vergleich | **fertig, Ziel verfehlt**: Median 2,8×–3,4× statt ≤ 2× |
| `secret[T]`, `#[constant_time]`, `u128` | **nicht begonnen** |
| `Rc`/`Gc`/`gc class`/`#[no_gc]` | **nicht begonnen** |
| `break`/`continue`, `for` | fertig; `defer`, `drop`, Move-Prüfer, Referenztypen: nicht begonnen |
| Härtetest 1 (HTML5-Tokenizer) | **nicht begonnen — 0 von 6.810 Fällen** |
| Härtetest 2 (DOM-Dauerlauf) | **nicht begonnen** |
| Testrunner mit maschinenlesbarer Ausgabe (`W2`) | **fertig** (`tools/testrunner`, JSON) |

Vorgezogen aus Phase 3, weil ohne sie kein Tokenizer schreibbar ist:
**Modulsystem** (`import`/`export`) und `.debug_line` für `gdb`.
Zahlen und Befehle stehen in `ABNAHME.md`, die Reproduktion in `RUN.md`.

## Phase 3 — v0.3: Module, `comptime`, Standardbibliothek

* Modulsystem, `import`, `export`-Listen, getrennte Übersetzung
* `comptime`-Auswertung (Interpreter über FIR), `interface` statisch + dynamisch
* **Kompilierzeit-Codegenerierung** (`G1`–`G4`): Bauskripte, perfektes Hashing,
  komprimierte Tries — Abnahme: Unicode-Tabelle aus der UCD
* Standardbibliothek `B1`–`B11`: Sammlungen, E/A, Zeit, Formatierung, Sortieren,
  Zufall (CSPRNG getrennt vom schnellen Generator)
* Nebenläufigkeit `N1`–`N4`: Fäden, Atomics, Mutex/Condvar/RwLock/Kanäle,
  `#[sendable]`/`#[shareable]`
* **Paketverwaltung + reproduzierbarer Bau** (`W1`)
* **DWARF-Grundlagen + Debugger** (`W3`) — ohne ihn wird jede folgende Aufgabe
  dreimal so lang
* **Stufe 1 beginnt:** Lexer und Parser werden in Firn neu geschrieben
* Aus `DESIGNZIELE.md`:
  * **`Io` als Parameter** statt `async` (§1): `Io`-Schnittstelle, `Future[T]`
    als `#[must_consume]`, `io.async`/`io.concurrent`, `Io.Threaded` und
    `Io.SingleThread` (letzteres erfüllt `N7`)
  * **Fehlbare Allokation durchgängig** (§2): `Allocator` als Parameter,
    `try v.push(inout a, x)`, `reserve` + `push_within_capacity` für heiße Pfade
  * **Capability-Deklaration in `firn.toml`** (§3) + Bauskript-Sandbox ohne Netz
  * **Symbol-Namensschema mit Versionsplatz** (§4) — billige Vorleistung für ein
    späteres stabiles ABI
  * **Vier Baustufen** `--dev` / `--dev-fast` / `--release-safe` /
    `--release-fast` (§5); Ziel für `--dev-fast`: höchstens 2–3× langsamer als
    Release, nicht 30×
  * **`init`-Ausdruck** mit Teilaufräumung, `#[no_move]` (§6)
  * **`comptime`-Interpreter über FIR + `reflect.*` + `emit`** (§7) — Vorbedingung
    für Abnahmepunkt 6 (UCD-Tabelle) und jede Web-IDL-Bindung
  * **`SoaVec[T]` / `#[layout(soa)]`**, `#[bitfeld]`, `#[klein(N)]` (§8)
* **Aufwand:** 3–6 Monate

## Phase 4 — v0.4/0.5: Selbst-Hosting

* `firnc1` übersetzt `firnc2`, `firnc2` übersetzt sich selbst, Ergebnis
  bit-identisch (Fixpunkt) → `L1` und `ABNAHME.md` Punkt 1 erfüllt
* Rust wird Bootstrap-Archiv, `firnc0` eingefroren
* **Abwicklung/`throw`** (`L8`) mit Tabellen in zwei Phasen
* Inkrementeller GC mit Dreifarbenmarkierung (`S5`), Pausenzeiten messbar (`S6`)
* Profiler mit Flamegraphs (`W4`), Fuzzing-Anbindung (`W5`)
* `Io.Evented` mit stapelvollen Koroutinen (`DESIGNZIELE.md` §1)
* Hot Reload **Stufe B** — Daten neu laden statt Code (§9); kostenlos,
  löst geschätzt 80 % des Iterationsbedarfs ohne jede Sprachänderung
* **Aufwand:** 6–12 Monate · **Ab hier ist Firn eine echte Sprache**

## Phase 5 — Abnahme nach `FIRN-ANFORDERUNGEN.md` §13

Alle sechs Punkte aus `ABNAHME.md` grün. Erst danach darf im Browser-Projekt
Block 1 starten. **Das ist das eigentliche Ziel dieses Fahrplans.**

## Phase 6 — Laufzeit auf Karstos (`R1`–`R6`)

* Firn-Laufzeit portiert: Speicher, Fäden, Datei, Zeit, E/A
* Trennung Laufzeit ↔ Plattformschicht, Kreuzcompiler nach Karstos im CI
* Firns eigene Testsuite läuft **auf Karstos** durch (`R6`)
* Läuft parallel zur Karstos-Kernel-Arbeit (K1–K10)

## Phase 7 — Karstos-Kernelmodule in Firn

* Kernel-Profil gegen echten karst-Code prüfen (ABI, Inline-Assembler, MMIO)
* Erstes Karstos-Modul in Firn (Kandidat: ein kleiner, isolierter Treiber)
* Danach schrittweise Ersetzung — **kein großer Neuschrieb**

## Phase 8 — v1.0: Stabilität

* Sprachstabilitätsversprechen, Rückwärtskompatibilität
* SIMD (`L16`), Schleifenoptimierung, optionales LLVM-Backend als Vergleichsmaß
* Formatierer, Linter, Abdeckungsmessung, Übersetzungs-Zwischenspeicher
* **Frühestens in mehreren Jahren**

## Ohne Termin (bewusst gestrichen)

* **aarch64-Backend** — erst wenn Karstos auf ARM zielt
* **WASM-Backend** — für den Browser nicht nötig; „Firn statt JavaScript im
  Browser" bleibt ein Fernziel, blockiert aber nichts
* **JIT**, dynamische Bibliotheken, C++-Interop — dauerhaft ausgeschlossen
* **Hot Reload Stufe C** (echter Codeaustausch) — `DESIGNZIELE.md` §9:
  kollidiert mit statischem Linken (`R5`) und Inlining über Modulgrenzen
  (`P1`). Ehrliche Einschätzung: lohnt sich nicht. Die Tür bleibt über
  `#[hot]` offen, mehr nicht
* **Stabiles ABI** (`#[abi_stable]`, `#[frozen]`) — erst wenn Karstos
  austauschbare Systemkomponenten braucht, Phase 7/8. IPC ist bis dahin
  der bessere Weg

---

## Woran das Projekt scheitern kann

Offen benannt, damit es nicht überrascht:

1. **Der Optimierer erreicht ≤ 2× Rust nicht.** Das ist das größte Einzelrisiko.
   Ein Tokenizer läuft über jedes Zeichen jeder Seite; 10× zu langsam heißt
   Browser 10× zu langsam, und das lässt sich später nicht herausoptimieren.
   Gegenmittel: früh und ehrlich messen (`Phase 2`), nicht am Ende.
2. **Der GC trägt den DOM nicht.** Konservatives Stack-Scanning schließt einen
   kompaktierenden Sammler aus; Fragmentierung im 24-h-Dauerlauf ist ein reales
   Risiko. Gegenmittel: Härtetest 2 früh, Größenklassen-Allokator.
3. **Durchhalten.** Der gefährlichste Punkt ist Phase 3/4 — der Reiz ist weg,
   die Arbeit wird zäh (Fehlermeldungen, Randfälle, Regressionen).
4. **Selbstbezug.** Ein Compiler, der sich selbst übersetzt, verbirgt Fehler
   hervorragend. Gegenmittel: Fixpunkt-Prüfung und eine ernst genommene
   Testsuite.
5. **Drei Baustellen gleichzeitig.** Karstos, Firn *und* der Browser ist viel.
   Firn darf Karstos nicht ausbremsen — deshalb bleibt Rust im Kernel, bis Firn
   nachweislich besser passt.
6. **Verbaute Fundamente.** Wird die Fundamentarbeit aus `DESIGNZIELE.md`
   §10 übersprungen, sind SoA-Layout, `comptime`-`emit` und die
   `--dev-fast`-Stufe später nur noch mit einem Umbau des gesamten
   Lowerings erreichbar. Gegenmittel: Phase 2 damit beginnen, nicht damit
   enden.
7. **Zielkonflikt Optimierer ↔ Krypto.** §9 der Spezifikation löst ihn auf dem
   Papier. Ob er in der Umsetzung hält, zeigt erst die Assembler-Inspektion.

---

## Nächster konkreter Schritt

Phase 2 abarbeiten, in dieser Reihenfolge (nach `FIRN-ANFORDERUNGEN.md` §12):
**Fundamentarbeit (`DESIGNZIELE.md` §10.4) → Speichermodell → Optimierer/
Messung → Zeichenketten → Testrunner → restlicher Sprachkern.**
Constant-Time wird dabei mitgebaut, nicht nachgerüstet. Die Fundamentarbeit
steht bewusst **vor** allem anderen: sie ist heute billig und später nicht
mehr bezahlbar.
