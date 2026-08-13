# Firn — Sprachspezifikation

**Arbeitstitel:** Firn · **Dateiendung:** `.fi` · **Status:** Entwurf v0.1 (2026-08-13)
**Autor:** Justin (GitHub: Flei123) · **Zielsystem:** Karstos / karst-Kernel, x86_64

> **Umbenennbarkeit.** Sprachname und Dateiendung stehen an *genau einer* Stelle im
> Compiler: `compiler/src/config.rs` (`LANG_NAME`, `FILE_EXT`, `LANG_NAME_LOWER`).
> Jede Fehlermeldung, jeder Hilfetext und jede Dateisuche liest von dort. Ein
> Umbenennen der Sprache ist eine Änderung von drei Konstanten, kein Refactoring.

---

## 0. Warum überhaupt eine eigene Sprache

Karstos ist heute in Rust geschrieben. Das funktioniert, erzeugt aber eine
Abhängigkeit, die Justin langfristig nicht will: Rust bestimmt, was der Kernel
darf (Rust-Editionen, `no_std`-Grenzen, LLVM-Zielunterstützung, Compiler-Bugs,
Projektpolitik). Wer ein eigenes Betriebssystem baut, aber den Compiler von
anderen bezieht, besitzt sein System nicht wirklich — er mietet es.

Firn ist der Versuch, diese letzte Abhängigkeit aufzulösen: **eine Sprache, ein
Compiler, ein Codegenerator, alles im eigenen Baum.** Kernel, Systemprogramme
und Anwendungen (bis hin zum Web-Frontend über ein WASM-Backend) sollen in
derselben Sprache entstehen.

**Ehrlichkeit vorweg:** Das ist ein Jahrzehnt-Projekt. Rust brauchte 9 Jahre bis
1.0, Zig ist nach 10 Jahren bei 0.16 und noch nicht stabil. Dieses Dokument
beschreibt das *Ziel*; der begleitende Prototyp implementiert bewusst nur den in
§12 markierten Ausschnitt — aber diesen wirklich, bis zum laufenden Binary.

---

## 1. Leitsätze

1. **Nichts passiert versteckt.** Keine implizite Allokation, keine impliziten
   Typumwandlungen, kein Operator-Overloading, keine versteckten Funktionsaufrufe.
   Was im Quelltext steht, ist was passiert.
2. **Eine Sprache, zwei Profile.** Kernel-Code und Anwendungscode teilen Syntax,
   Typsystem und Compiler. Der Unterschied ist, welche *Fähigkeiten* verfügbar
   sind — nicht welcher Dialekt.
3. **Sicher per Default, unsicher auf Ansage.** Speicherfehler sind
   Übersetzungsfehler, außer man schreibt `unsafe` hin.
4. **Ein Weg, etwas zu tun.** Wo C++ fünf Wege hat und Rust drei, hat Firn einen.
5. **Der Compiler gehört uns.** Kein LLVM im Bootstrap-Pfad (§8).
6. **Lesbarkeit vor Kürze.** Der Compiler wird irgendwann in sich selbst
   geschrieben sein; unlesbarer Code ist dann ein Systemrisiko.

---

## 2. Profile — `kernel` und `app`

Ein Modul deklariert sein Profil in der ersten Zeile:

```firn
profile kernel   // freistehend, kein Allokator, kein Runtime
profile app      // Standardbibliothek, Allokator, optional RC
```

Ohne Angabe gilt `app`. Der Compiler-Schalter `--profile=kernel` erzwingt das
Profil für die gesamte Übersetzungseinheit (Prüfung: Modul-Deklaration und
Schalter müssen übereinstimmen, sonst Fehler).

| Eigenschaft                        | `kernel`                          | `app`                            |
|------------------------------------|-----------------------------------|----------------------------------|
| Heap-Allokation                    | nur über explizit übergebenen `Allocator` | globaler Allokator verfügbar |
| Versteckte Allokation              | verboten (Compilerfehler)         | verboten (gleiche Regel)         |
| `Rc[T]` / Referenzzählung          | nicht verfügbar                   | verfügbar, explizit              |
| Garbage Collection                 | nie                               | **nie** — siehe unten            |
| Panik bei Bereichsüberschreitung   | ruft `karst_panic`, konfigurierbar| ruft Laufzeit-Panik-Handler      |
| Gleitkomma                         | nur mit `#[allow_fp]` (FPU-Zustand!) | frei                          |
| Stack-Tiefe prüfbar (`#[max_stack]`)| ja                               | ja, meist ungenutzt              |
| Ziel-Binärformat                   | ELF-Objekt, freistehend           | ELF-Executable / WASM            |

**Entscheidung gegen GC — auch im `app`-Profil.** Der ursprüngliche Gedanke war
„optional GC im App-Profil". Das wird verworfen, und zwar aus einem konkreten
Grund: Ein GC ist keine Bibliothek, er ist eine Eigenschaft des *gesamten*
Objektgraphen. Sobald eine Sprache GC-Werte kennt, müssen alle Datenstrukturen,
alle FFI-Grenzen und der Codegenerator (Stack-Maps, Schreibbarrieren,
Safepoints) das wissen. Man bekommt faktisch zwei Sprachen mit einer Syntax —
genau das, was Leitsatz 2 verhindern soll. Der Komfort, den ein GC bringt, wird
stattdessen durch drei billigere Mittel erreicht:

* **Wertsemantik mit Moves** (§3) — der Normalfall braucht gar keine
  Lebensdauer-Überlegung.
* **Arenen/Regionen** (§3.4) — für Compiler, Parser, Request-Handler: alles in
  eine Arena, am Ende ein `arena.reset()`. Das ist in der Praxis *schneller* als
  GC und trivial zu verstehen.
* **`Rc[T]` explizit** — wenn wirklich geteiltes Eigentum nötig ist, sichtbar im
  Typ, mit dem bekannten Preis (Zyklen lecken; `Weak[T]` existiert).

Preis dieser Entscheidung: zyklische Objektgraphen sind unbequem
(Indizes/Handles statt Zeiger). Das ist ein realer Nachteil gegenüber
GC-Sprachen und wird nicht schöngeredet.

---

## 3. Speichermodell — die zentrale Entscheidung

Zur Wahl standen drei Modelle:

| Modell | Vertreter | Sicherheit | Komplexität | Bootstrappbar? |
|---|---|---|---|---|
| A: Ownership + Borrowing mit Lebensdauern | Rust | hoch | **sehr hoch** (Lifetimes, NLL, Varianz) | schwer |
| B: Regionen/Arenen | Cyclone, Zig-Idiom | mittel–hoch | mittel | ja |
| C: Explizit unsicher + Werkzeuge | Zig, C | niedrig (Laufzeitprüfung) | niedrig | ja |

**Gewählt: A′ — Ownership mit Moves plus *zweitklassige* Referenzen.**
Also Rusts Eigentumsmodell, aber ohne das Teuerste daran: ohne
Lebensdauer-Parameter.

### 3.1 Die Regel

* Jeder Wert hat **genau einen Eigentümer**. Zuweisung/Übergabe **verschiebt**
  (move), außer der Typ ist *trivial* (Ganzzahlen, `bool`, Rohzeiger, Arrays
  trivialer Typen, Structs, deren Felder alle trivial sind). Triviale Typen
  werden kopiert. Es gibt **keinen** benutzerdefinierbaren Kopierkonstruktor.
* Referenzen gibt es in zwei Formen:
  * `&T` — geteilter, lesender Zugriff, beliebig viele gleichzeitig
  * `inout T` — exklusiver, schreibender Zugriff, genau einer, keine `&T` daneben
* **Zweitklassigkeit (der entscheidende Punkt):** Referenzen dürfen
  *ausschließlich* als Funktionsparameter existieren und an aufgerufene
  Funktionen weitergereicht werden. Sie dürfen **nicht** in Structs gespeichert,
  **nicht** in Arrays gelegt, **nicht** zurückgegeben und **nicht** in
  Variablen mit längerer Lebensdauer gebunden werden.

Daraus folgt: eine Referenz lebt niemals länger als der Aufrufrahmen, aus dem
sie stammt. **Deshalb braucht Firn keine Lebensdauer-Annotationen.** Die Prüfung
ist rein innerhalb einer Funktion durchführbar (intraprozedural) — jeder, der
einen Baumdurchlauf schreiben kann, kann den Prüfer schreiben. Genau das ist für
Selbst-Hosting entscheidend: Rusts Borrow-Checker ist ein Forschungsprojekt,
dieser hier ist ein Wochenendprojekt.

Vorbild ist Hylo/Val („mutable value semantics") und Swifts Parameterkonventionen
— nicht neu erfunden, aber bewusst gewählt.

### 3.2 Der ehrliche Preis

Was damit **nicht** geht:

* Keine Struct-Felder vom Typ Referenz. Kein `struct Iter { list: &List }`.
  Iteratoren werden als *Index + Zugriffsfunktion* gebaut oder mit Rohzeigern in
  `unsafe`.
* Keine Funktion gibt eine Referenz auf ein Element zurück
  (`fn get(&self, i) -> &T` existiert nicht). Stattdessen: Wert zurückgeben
  (bei trivialen Typen kostenlos), Index zurückgeben, oder eine
  `with_element(i, fn(inout T))`-Form, die die Referenz nur nach unten reicht.
* Verkettete Listen, Graphen, Bäume mit Elternzeigern: brauchen Indizes in ein
  Arena-Array (`u32`-Handles) — oder `unsafe` mit Rohzeigern.
* Selbstbezügliche Structs bleiben verboten (wie in Rust ohne `Pin`).

Das ist spürbar unbequemer als Rust bei generischen Bibliotheken und deutlich
unbequemer als GC-Sprachen bei Graphen. Dafür entfällt die gesamte Kategorie
„Lifetime-Fehlermeldung, die niemand versteht". Für Kernel- und Systemcode, wo
Arena-Allokation und Index-Handles ohnehin die Norm sind, ist der Preis niedrig.
Für ein GUI-Framework mit Objektbaum ist er hoch — dort ist `Rc[T]` das Mittel.

### 3.3 Zerstörung

Deterministisch am Ende des Gültigkeitsbereichs, in umgekehrter
Deklarationsreihenfolge. Ein Typ kann `fn drop(inout self)` definieren; das ist
ein ganz normaler Aufruf an einer vorhersagbaren Stelle, kein verstecktes
Verhalten. Zusätzlich gibt es `defer { ... }` (Zig-Idee) für Aufräumarbeit, die
nicht an einen Typ gebunden ist. Wurde ein Wert verschoben, läuft sein `drop`
nicht mehr (Move-Tracking im Prüfer, statisch — kein Laufzeit-Flag; bei
bedingten Moves ist der Zweig-Merge konservativ und meldet einen Fehler statt
still ein Flag einzuführen).

`#[must_consume]` markiert Typen, die nicht einfach fallengelassen werden dürfen
(z. B. ein Lock-Guard im Kernel, ein `Result`, ein DMA-Puffer): Der Compiler
verlangt, dass sie an eine Funktion übergeben werden, die sie verbraucht.

### 3.4 Regionen / Arenen

Erstklassig, weil sie das Arbeitspferd ersetzen sollen, das anderswo der GC ist:

```firn
fn parse(src: &Str, alloc: inout Arena) -> Ast {
    let node = alloc.new(Node{ kind: NodeKind.Add, lhs: 0, rhs: 0 })
    ...
}

fn main() -> i32 {
    var arena = Arena.with_capacity(1 << 20)
    defer arena.deinit()
    let ast = parse(&source, inout arena)
    ...
}
```

Ein `Arena` ist ein normaler Wert. Es gibt keine Magie und keinen
Regionen-Typparameter (das wäre wieder eine Lebensdauer). Der Schutz gegen
„Zeiger überlebt Arena" kommt aus §3.1: Man kann aus der Arena keine Referenz
herausreichen, nur Handles (`u32`-Index) — und ein Handle in eine
zurückgesetzte Arena ist ein logischer, kein Speicherfehler.

### 3.5 Rohzeiger und `unsafe`

`*T` (unveränderlich) und `*mut T`. Dereferenzieren, Zeigerarithmetik,
Typumdeutung (`transmute`), MMIO-Zugriff und Inline-Assembler sind nur in
`unsafe { }` erlaubt. `unsafe` schaltet **nur** diese Operationen frei; alle
anderen Regeln (Moves, Typen) gelten weiter. Jeder `unsafe`-Block braucht einen
Kommentar mit der Begründung — der Compiler erzwingt das mit
`--deny=undocumented-unsafe` (Standard im `kernel`-Profil).

---

## 4. Herkunft der Ideen — und was bewusst fehlt

**Von Rust übernommen:** Eigentum und Moves, `unsafe` als Grenze,
`Result`-artige Fehlerbehandlung ohne Ausnahmen, Aufzählungen mit Nutzdaten und
erschöpfendes Mustervergleichen, Modulsystem mit expliziten Exporten,
Ausdrucksorientierung (`if`/`match` liefern Werte).

**Von Zig übernommen:** `comptime` statt eines zweiten Template-Systems, „keine
versteckte Steuerung" (kein Operator-Overloading, keine impliziten
Umwandlungen), explizit übergebene Allokatoren, `defer`/`errdefer`, Fehler als
Wertetyp, Übersetzungszeit-Reflexion, `test`-Blöcke direkt in der Quelldatei.

**Von C übernommen:** Ein flaches, vorhersagbares Speicherlayout, direkte
Abbildbarkeit auf Maschinenbefehle, kleine Kernsprache, ABI-Ehrlichkeit
(System-V ohne Überraschungen), die Möglichkeit, den erzeugten Assembler zu
lesen und wiederzuerkennen.

**Bewusst weggelassen:**

| Weggelassen | Grund |
|---|---|
| Lebensdauer-Parameter (`'a`) | größter Komplexitätstreiber in Rust; §3.1 macht sie überflüssig |
| Vererbung, Klassen | Komposition + Schnittstellen reichen |
| Operator-Overloading | `a + b` soll eine Addition sein, kein Funktionsaufruf mit Nebenwirkung |
| Implizite Umwandlungen (auch verlustfrei!) | `u8 → u32` schreibt man `as u32`; Zahlenfehler sollen sichtbar sein |
| Ausnahmen / `panic` als Steuerfluss | Panik = Programmierfehler, nicht behandelbar |
| Makros mit eigener Syntax | `comptime` kann dasselbe mit gewöhnlichem Firn-Code |
| Konstruktoren/Destruktoren mit Magie | nur `drop`, sonst normale Funktionen |
| `async`/`await` in der Sprache | Steuerfluss-Transformation gehört in eine Bibliothek, nicht in den Compiler; siehe §7 |
| Überladen von Funktionsnamen | erschwert Fehlermeldungen und Selbst-Hosting |
| Automatische Dereferenzierung | `p.*.feld`, nicht `p.feld` |
| Vorprozessor | keiner. `comptime if` ersetzt `#ifdef` |
| Garbage Collection | §2 |

---

## 5. Fehlerbehandlung

Zwei streng getrennte Kategorien:

**(a) Erwartete Fehler** — Datei fehlt, Speicher voll, ungültige Eingabe.
Modelliert als Fehlerunion, Zig-nah:

```firn
error IoError { NotFound, Permission, Closed }

fn read_all(path: &Str, alloc: inout Arena) -> IoError!Buf {
    let fd = try open(path)            // gibt den Fehler nach oben weiter
    errdefer close(fd)                 // läuft nur auf dem Fehlerpfad
    ...
    return buf
}

// Behandeln:
let buf = read_all(&p, inout a) catch |e| {
    match e { IoError.NotFound => return default_buf(), else => return e }
}
```

* `!T` ist eine Union aus Fehlermenge und Erfolgstyp. Die Fehlermenge darf
  weggelassen (`!T`) und dann vom Compiler *abgeleitet* werden — das ist
  Zigs beste Idee: keine Fehlertyp-Bürokratie, trotzdem exakt.
* `try` ist kein Zucker für „ignorieren": ohne `try`/`catch` ist der Wert nicht
  benutzbar (Typfehler). Ein nicht behandeltes `!T` kann nicht fallengelassen
  werden (`#[must_consume]`).
* Kein Stack-Unwinding, keine Landing-Pads, kein `Drop`-während-Unwind. Das hält
  den Codegenerator klein und den Kernel deterministisch.

**(b) Programmierfehler** — Indexüberlauf, Division durch null, verletzte
Zusicherung. Führen zu `panic`. Panik ist **nicht** behandelbar; sie ruft einen
Handler auf (`app`: Meldung + Abbruch; `kernel`: `karst_panic`, konfigurierbar).
Mit `--release-fast` lassen sich die Prüfungen ausschalten — dann ist der
entsprechende Fall undefiniertes Verhalten und das steht so in der Dokumentation.

---

## 6. Generics und Metaprogrammierung — `comptime`

Kein zweites Typsystem, keine Templates, keine Makros. Übersetzungszeit ist
dieselbe Sprache, nur früher ausgeführt:

```firn
fn max[comptime T: type](a: T, b: T) -> T {
    comptime assert(is_ordered(T), "max verlangt einen ordnungsfähigen Typ")
    return if a > b { a } else { b }
}

struct Vec[comptime T: type] {
    data: *mut T,
    len:  usize,
    cap:  usize,
}

// Übersetzungszeit-Verzweigung ersetzt #ifdef:
comptime if target.arch == Arch.X86_64 { ... } else { ... }
```

* `comptime`-Parameter sind gewöhnliche Parameter, die zur Übersetzungszeit
  bekannt sein müssen. `type` ist ein gewöhnlicher Typ von Werten.
* Monomorphisierung: pro benutzter Instanziierung eine Funktion. Kein
  Typlöschen, keine versteckten Vtables.
* **Ein Nachteil offen benannt:** Fehler in generischem Code zeigen sich erst
  bei der Instanziierung (wie in C++/Zig, anders als bei Rusts Traits). Firn
  mildert das mit `comptime assert` und *deklarierten Anforderungen*
  (`where has_method(T, "next")`), die vor dem Einsetzen geprüft werden — es
  bleibt aber schlechter als echte Trait-Signaturen. Bewusster Tausch:
  Einfachheit des Compilers gegen Fehlermeldungsqualität.
* Ab v0.4 zusätzlich `interface` (statisch aufgelöste Schnittstellen, ohne
  Vererbung) für Bibliotheks-APIs, die stabile Signaturen brauchen.

---

## 7. Nebenläufigkeit

* **In der Sprache:** nur die Bausteine — `atomic[T]` mit expliziter
  Speicherordnung, `fence`, und die Regel aus §3: `inout` ist exklusiv, `&` ist
  geteilt und lesend. Daraus folgt Datenrennenfreiheit für sicheren Code, ohne
  ein `Send`/`Sync`-Traitsystem: eine Referenz kann einen Aufrufrahmen ohnehin
  nicht verlassen (§3.1), also kann sie auch nicht in einen anderen Faden
  wandern. Was zwischen Fäden geteilt wird, muss `Shared[T]` sein — ein Typ,
  der explizit einen Zugriffsmechanismus (Lock, Atomic, Channel) mitbringt.
* **Kein `async`/`await` im Compiler.** Zustandsmaschinen-Transformation ist eine
  große, schwer zu debuggende Codegenerator-Funktion, die zudem eine Laufzeit
  („wer führt aus?") in die Sprache zieht. Stattdessen: Fäden/Tasks als
  Bibliothek (`app`: Betriebssystem-Threads + Poll-Schleife; `kernel`: die
  karst-Scheduler-Primitiven direkt).
* **Strukturierte Nebenläufigkeit** als Bibliotheksmuster: ein `Scope`, der beim
  Verlassen alle in ihm gestarteten Aufgaben verbindet. Erzwungen über
  `#[must_consume]`.

---

## 8. Backend-Strategie

### 8.1 Aufbau

```
Quelle .fi
  └─> Lexer ──> Parser ──> AST
        └─> Resolver (Namen, Module)
              └─> Typprüfer + Move-/Referenzprüfer
                    └─> HIR (entzuckert: for→while, comptime aufgelöst)
                          └─> FIR  (eigene IR, SSA-artig, Basisblöcke)
                                ├─> Optimierer (Konstantenfaltung, DCE, später GVN)
                                └─> Backends:
                                     ├─ x86_64  (eigener Codegen)   ← Stufe 0/1
                                     ├─ aarch64 (geplant v0.5)
                                     ├─ wasm32  (geplant v0.6)
                                     └─ llvm-ir (optional, nie im Bootstrap-Pfad)
```

**FIR** ist die Sollbruchstelle. Sie ist explizit dokumentiert
(`docs/FIR.md`), typisiert, in Basisblöcke mit einem Terminator gegliedert, und
sie kennt keine x86-Eigenheiten. Jedes Backend ist dadurch austauschbar und
einzeln testbar (IR-Textformat rein, Maschinencode raus).

### 8.2 Warum nicht einfach LLVM?

LLVM wäre der schnellere Weg zum Ziel — bessere Optimierung, viele Zielsysteme,
Debug-Informationen geschenkt. Trotzdem ist es **nicht** die Basis, aus vier
Gründen:

1. **Unabhängigkeit ist das eigentliche Projektziel.** Ein Karstos, dessen
   Compiler ein 30-Millionen-Zeilen-C++-Projekt braucht, hat Rust nur gegen LLVM
   getauscht. Das Bootstrapping-Problem wäre nicht gelöst, nur verschoben.
2. **Selbst-Hosting.** Stufe 2 (§9) verlangt, dass der Firn-Compiler in Firn
   geschrieben ist. Eine LLVM-Anbindung müsste dann aus Firn heraus C++-ABI
   bedienen — genau die Abhängigkeit, die verschwinden soll.
3. **Kontrolle im Kernel.** LLVM erzeugt eigenmächtig Aufrufe an `memcpy`,
   `memset`, Gleitkomma-Hilfsroutinen; es hat eigene Vorstellungen von
   Stack-Probing, roten Zonen und Kontrollfluss-Absicherung. Im Kernel ist genau
   das eine Fehlerquelle. Eigener Codegen heißt: der Assembler enthält, was wir
   hineingeschrieben haben.
4. **Bauzeit und Reproduzierbarkeit.** Der gesamte Firn-Compiler soll in unter
   einer Minute aus dem Quelltext baubar sein und ein bit-identisches Ergebnis
   liefern.

**Was das kostet — ohne Beschönigung:** Der erzeugte Code wird auf Jahre
langsamer sein als LLVM-Code, realistisch Faktor 2–5 bei rechenintensiven
Schleifen (keine Autovektorisierung, keine gute Registerzuteilung, kein
Inlining-Heuristik-Tuning, kein Loop-Unrolling). Jedes neue Zielsystem ist
Monate Arbeit statt einer Kommandozeilenoption. Debug-Informationen (DWARF),
LTO, Sanitizer, Profil-geführte Optimierung: alles selbst zu bauen oder
zunächst nicht vorhanden.

**Tür bleibt offen:** Ein `llvm`-Backend hinter einem Übersetzungsschalter ist
ausdrücklich erlaubt und sinnvoll — für Anwendungscode, wo Geschwindigkeit
zählt, und als Vergleichsmaßstab („was hätte LLVM daraus gemacht?"). Es darf nur
niemals im Bootstrap-Pfad liegen: `firn` muss sich selbst ohne LLVM übersetzen
können. Diese Regel ist verbindlich.

### 8.3 WASM-Backend (v0.6)

Ziel ist, dass Justins Anwendungen ohne JavaScript im Browser laufen.
WASM ist für einen eigenen Codegenerator vergleichsweise dankbar: Stapelmaschine,
strukturierter Kontrollfluss, keine Registerzuteilung. Die einzige echte Arbeit
ist die Rückverwandlung des FIR-Basisblockgraphen in strukturierte Blöcke
(Relooper/Stackifier). Das ist der Grund, warum FIR den Kontrollfluss explizit
als Graph mit Dominanzinformation hält.

---

## 9. Bootstrap-Plan

| Stufe | Compiler geschrieben in | Übersetzt von | Ergebnis | Status |
|---|---|---|---|---|
| **0** | Rust | `cargo`/`rustc` | `firnc0` — übersetzt die Teilmenge aus §12 nach x86_64-Assembler | **dieser Prototyp** |
| **1** | Firn (Teilmenge §12) | `firnc0` | `firnc1` — kann dasselbe wie `firnc0` | v0.3 |
| **2** | Firn (voller v0.4-Umfang) | `firnc1` | `firnc2`; danach übersetzt `firnc2` sich selbst → `firnc2'` | v0.4 |
| **3** | — | — | **Fixpunkt:** `firnc2` und `firnc2'` sind bit-identisch → Selbst-Hosting erreicht, Rust ist nur noch historisch | v0.5 |
| **4** | — | — | `firnc0` (Rust) wird eingefroren und nur noch als Bootstrap-Archiv gepflegt; ein vorkompiliertes `firnc1`-Binary wird zusammen mit dem Quelltext archiviert (Trusting-Trust-Gegenmaßnahme: reproduzierbar aus zwei unabhängigen Wegen baubar) | v0.6 |

Regeln für den Bootstrap:
* Stufe 1 darf **nur** Sprachmerkmale benutzen, die `firnc0` beherrscht. Das ist
  der Grund, warum die Teilmenge in §12 so klein und so streng definiert ist.
* Jede Stufe wird durch die vollständige Testsuite geprüft, nicht nur durch
  „baut durch".
* Der Fixpunkt-Vergleich (Stufe 3) ist die einzige belastbare Aussage darüber,
  dass der Compiler korrekt ist — nicht das Selbstlob im README.

---

## 10. Syntax

Bewusst nah an Rust/Zig, damit Justin nicht umlernen muss. Geschweifte Klammern,
Semikolon optional (Zeilenende beendet eine Anweisung, wenn der Ausdruck
vollständig ist), Typ nach dem Namen.

```firn
profile app

import std.io

const MAX: u32 = 100

struct Point {
    x: i32,
    y: i32,
}

enum Shape {
    Circle(f64),
    Rect(Point, Point),
}

fn dist2(p: &Point, q: &Point) -> i32 {
    let dx = p.x - q.x
    let dy = p.y - q.y
    return dx * dx + dy * dy
}

fn sum_to(n: u32) -> u32 {
    var acc: u32 = 0
    var i: u32 = 1
    while i <= n {
        acc = acc + i
        i = i + 1
    }
    return acc
}

fn main() -> i32 {
    let p = Point{ x: 3, y: 4 }
    let q = Point{ x: 0, y: 0 }
    if dist2(&p, &q) == 25 { return 0 } else { return 1 }
}
```

Merkmale: `let` unveränderlich, `var` veränderlich (Umkehr von Rusts `mut`, weil
unveränderlich der Normalfall sein soll und *kein* Zusatzwort kosten darf).
Eckige Klammern für generische Parameter (`Vec[u8]`), damit `<` eindeutig
Vergleich bleibt und der Parser ohne Rückverfolgung auskommt. Keine
Sichtbarkeits-Schlüsselwörter am Element, sondern eine `export`-Liste pro Modul.

### 10.1 Grammatik der v0-Teilmenge (EBNF)

```ebnf
program     = { item } ;
item        = fn_decl | struct_decl | const_decl | profile_decl ;
profile_decl= "profile" ident ;
fn_decl     = [ "extern" ] "fn" ident "(" [ params ] ")" [ "->" type ] block ;
params      = param { "," param } [ "," ] ;
param       = ident ":" type ;
struct_decl = "struct" ident "{" { ident ":" type "," } "}" ;
const_decl  = "const" ident ":" type "=" expr ;

type        = "i8"|"i16"|"i32"|"i64"|"u8"|"u16"|"u32"|"u64"|"usize"|"isize"|"bool"
            | "*" [ "mut" ] type
            | "[" type ";" int_lit "]"
            | ident ;

block       = "{" { stmt } "}" ;
stmt        = let_stmt | var_stmt | assign | if_stmt | while_stmt
            | return_stmt | expr_stmt | block ;
let_stmt    = "let" ident [ ":" type ] "=" expr ;
var_stmt    = "var" ident [ ":" type ] "=" expr ;
assign      = lvalue "=" expr ;
lvalue      = ident | lvalue "." ident | lvalue "[" expr "]" | "*" lvalue ;
if_stmt     = "if" expr block [ "else" ( block | if_stmt ) ] ;
while_stmt  = "while" expr block ;
return_stmt = "return" [ expr ] ;

expr        = or_expr ;
or_expr     = and_expr { "||" and_expr } ;
and_expr    = cmp_expr { "&&" cmp_expr } ;
cmp_expr    = add_expr [ ( "=="|"!="|"<"|"<="|">"|">=" ) add_expr ] ;
add_expr    = mul_expr { ( "+"|"-"|"|"|"^" ) mul_expr } ;
mul_expr    = unary   { ( "*"|"/"|"%"|"&"|"<<"|">>" ) unary } ;
unary       = ( "-" | "!" | "&" | "*" ) unary | postfix ;
postfix     = primary { "." ident | "[" expr "]" | "(" [ args ] ")" | "as" type } ;
primary     = int_lit | bool_lit | ident | "(" expr ")" | struct_lit | array_lit
            | "syscall" "(" args ")" ;
struct_lit  = ident "{" { ident ":" expr "," } "}" ;
array_lit   = "[" [ expr { "," expr } ] "]" ;
```

---

## 11. Zahlen, Layout, ABI

* Ganzzahltypen mit ausgeschriebener Breite: `i8…i64`, `u8…u64`, `usize`,
  `isize`. **Kein** `int`. Literale sind typlos bis zur Verwendung und müssen
  eindeutig ableitbar sein, sonst Fehler („Typ des Literals unklar, schreibe
  `123 as u32`").
* Überlauf: in `--debug` geprüft (Panik), in `--release-fast` umlaufend
  (definiert, nicht undefiniert — bewusst anders als C).
* Umwandlungen ausschließlich mit `as`, auch verlustfreie.
* Struct-Layout: standardmäßig Deklarationsreihenfolge mit natürlicher
  Ausrichtung (**kein** Umsortieren — im Kernel muss man Layout vorhersagen
  können). `#[packed]` und `#[align(n)]` existieren.
* Aufrufkonvention: System V AMD64 (Ganzzahlargumente in
  `rdi, rsi, rdx, rcx, r8, r9`, Rückgabe in `rax`, 16-Byte-Stapelausrichtung an
  der Aufrufstelle, `rbx, rbp, r12–r15` erhalten). `extern "C"` ist dasselbe —
  Firn erfindet keine eigene ABI, weil sonst der Übergang zum bestehenden
  Karstos-Code unmöglich wäre.
* `syscall(nr, a1, …, a6)` ist ein eingebautes Sprachmittel und bildet direkt auf
  den `syscall`-Befehl ab (Argumente in `rax, rdi, rsi, rdx, r10, r8, r9`).
  Damit ist Ausgabe ohne libc möglich — Voraussetzung für ein freistehendes
  Ziel und für den Test „Hello World ohne C-Laufzeit".

---

## 12. Was der Prototyp (Stufe 0) implementiert — verbindlich

Diese Liste ist der Vertrag zwischen Spezifikation und Code. Alles hier muss
wirklich laufen; alles andere aus diesem Dokument ist Zukunft und wird im README
ausdrücklich als „noch nicht" geführt.

**Enthalten:**
* Lexer und handgeschriebener rekursiv absteigender Parser (kein Generator),
  Fehlermeldungen mit Datei, Zeile, Spalte, Quelltextzeile und Markierung.
* Typprüfer: `i8/i16/i32/i64`, `u8/u16/u32/u64`, `usize`, `isize`, `bool`,
  Zeiger `*T`/`*mut T`, Structs mit Feldzugriff, Arrays fester Größe mit Index,
  Funktionen. Keine implizite Umwandlung — nur `as`.
* Ausdrücke: `+ - * / %`, `& | ^ << >>`, Vergleiche, `&& ||` (kurzschließend),
  unäres `-`, `!`, Adresse `&`, Dereferenzierung `*`.
* Anweisungen: `let`, `var`, Zuweisung an Variable/Feld/Index/Dereferenzierung,
  `if`/`else`, `while`, `return`, Blöcke.
* Funktionen mit Parametern und Rückgabewert, Rekursion.
* `syscall(...)` mit bis zu 6 Argumenten.
* **FIR**: eigene IR in Basisblöcken, dokumentiert, mit Textausgabe
  (`--emit=fir`), Konstantenfaltung und Entfernen toten Codes, beides mit
  Vorher/Nachher-Test.
* **x86_64-Codegen ohne LLVM**: Assemblerausgabe für `as`/`ld`, System-V-ABI,
  eigene Registerbelegung — *ehrlich benannt:* das ist **keine** Registerzuteilung
  im Sinne von Lebendigkeitsanalyse/Graphfärbung. Stufe 0 gibt jedem FIR-Wert
  einen eigenen Stack-Slot und rechnet in `rax`/`rcx`/`rdx` (reines Spilling).
  Korrekt, aber langsam. Echte Registerzuteilung ist Phase 2 (ROADMAP).
* Testsuite mit ≥ 40 `.fi`-Programmen: übersetzen, ausführen, Rückgabewert und
  Ausgabe gegen Erwartung prüfen; dazu Negativtests für Fehlermeldungen.

**Nicht enthalten (Stufe 0):** Module/Imports, `comptime`, Generics,
Aufzählungen, `match`, Fehlerunionen `!T`, `defer`, `drop`, Referenztypen
`&T`/`inout T` als *geprüfte* Typen (Stufe 0 hat nur Rohzeiger), Move-Prüfer,
Arenen, Gleitkomma, Zeichenketten als Typ (nur Byte-Arrays), Standardbibliothek,
aarch64, WASM, LLVM-Backend, Optimierung über Konstantenfaltung/DCE hinaus.

Diese Trennung ist beabsichtigt: Ein kleiner Compiler, der wirklich Binärdateien
erzeugt, die laufen, ist mehr wert als ein großer, der nur behauptet zu
funktionieren.

---

## 13. Offene Fragen

1. **Zeichenketten.** `[]u8` mit Länge, UTF-8 garantiert? Oder ein eigener
   `Str`-Typ mit Prüfung an der Grenze? Entscheidung vertagt auf v0.3.
2. **Schnittstellen.** Reicht `comptime`-Duck-Typing dauerhaft, oder braucht es
   `interface` (§6)? Wird an der ersten echten Bibliothek entschieden.
3. **Bedingte Moves.** Konservativ ablehnen (jetzige Wahl) oder doch ein
   Laufzeit-Flag wie Rusts Drop-Flags? Ablehnen ist sauberer, aber unbequem.
4. **Paketverwaltung.** Bewusst noch nicht gedacht. Erst Sprache, dann Ökosystem.
5. **Ausrichtung an Karstos.** Sobald `firnc` Kernelmodule übersetzen kann, muss
   die Aufrufkonvention gegen den bestehenden Rust-Code in karst geprüft werden.

---

*Dieses Dokument ist die Wahrheit über das Ziel. Der Code ist die Wahrheit über
den Stand. Wo beide auseinandergehen, gewinnt der Code — und das Dokument wird
korrigiert, nicht der Code beschönigt.*

---

## 12.1 Nachtrag: bewusste Abweichungen der Stufe-0-Umsetzung (`firnc0`)

Dieser Abschnitt wurde beim Bau von `firnc0` ergaenzt. Er nimmt nichts aus §12
zurueck, sondern haelt fest, wo die Umsetzung enger ist als der Text oben —
damit Spezifikation und Code nicht auseinanderlaufen (siehe Schlusssatz des
Dokuments: wo beide abweichen, gewinnt der Code und das Dokument wird
korrigiert).

1. **Aggregate an Funktionsgrenzen.** Parameter und Rueckgabewerte duerfen in
   Stufe 0 nur *skalar* sein (Ganzzahl, `bool`, Zeiger) oder — beim Rueckgabetyp —
   fehlen. Structs und Arrays werden per Zeiger uebergeben (`*T` / `*mut T`).
   Grund: die System-V-Klassifikation zusammengesetzter Typen (INTEGER/SSE/MEMORY,
   Aufteilung auf zwei Register) ist umfangreich und fehleranfaellig; sie kommt in
   v0.2. Der Compiler meldet dafuer einen sauberen Fehler, keinen Absturz.
2. **Typlose Literale.** §11 verlangt, dass der Typ eines Literals eindeutig
   ableitbar ist. `firnc0` setzt das woertlich um: es gibt **keinen**
   Vorgabetyp. `let x = 5` ist ein Fehler, `let x: i32 = 5` und `let x = 5 as i32`
   sind richtig. Kontext liefern: Typannotation, Zieltyp einer Zuweisung,
   Parametertyp, Rueckgabetyp, `as`, der andere (typisierte) Operand eines
   Bin&auml;roperators und die Indexposition (`usize`).
3. **Keine Laufzeitpruefungen.** Ueberlauf, Division durch null und
   Bereichsueberschreitung sind in Stufe 0 **nicht** geprueft (§5(b) und §11
   beschreiben den Zielzustand). Verhalten entspricht `--release-fast`:
   umlaufende Arithmetik, Division durch null loest die CPU-Ausnahme aus.
   `--debug`-Pruefungen kommen mit dem Panik-Handler in v0.2.
4. **`const`** ist auf skalare, zur Uebersetzungszeit auswertbare
   Ganzzahl-/`bool`-Ausdruecke beschraenkt (keine Struct-/Array-Konstanten).
5. **Globale Variablen** gibt es nicht (nur `const`) — sie sind in §10.1 auch
   nicht vorgesehen.
6. **`profile`-Deklaration** wird geparst und geprueft (`kernel`/`app`), hat in
   Stufe 0 aber keine Wirkung: es gibt weder Allokator noch Standardbibliothek,
   erzeugt wird immer ein freistehendes Binary mit `_start` ohne libc.
7. **`extern fn`** (§10.1) wird syntaktisch erkannt, aber mit einem klaren
   Fehler abgelehnt ("in Stufe 0 nicht unterstuetzt"), weil es ohne Linker-
   Anbindung an eine Fremdbibliothek keinen Nutzen haette.
8. **Rueckgabewert des Programms.** `fn main() -> i32` ist der Einstiegspunkt;
   `_start` ruft `main` auf und uebergibt das Ergebnis an den `exit`-Syscall
   (Exit-Code = Wert & 0xFF, wie unter Linux ueblich).
9. **Hoechstens 6 Funktionsparameter.** Argumente werden ausschliesslich in den
   System-V-Registern `rdi, rsi, rdx, rcx, r8, r9` uebergeben; Stapelargumente
   sind nicht umgesetzt. Mehr als 6 Parameter melden einen sauberen Fehler
   (mit Zeile/Spalte), keinen Absturz. Stapelargumente kommen in v0.2.
10. **Parameter sind unveraenderlich.** Ein Parameter verhaelt sich wie eine
    `let`-Bindung; `p = ...` im Rumpf ist ein Fehler. Wer eine veraenderliche
    Kopie braucht, legt sie mit `var` an. (§10.1 sagt dazu nichts; so ist es
    umgesetzt.)
11. **Kein Wiederholungsliteral `[wert; N]`.** §10.1 kennt nur
    `array_lit = "[" [ expr { "," expr } ] "]"`; ein Array wird also elementweise
    initialisiert (siehe `tests/059_sieve.fi`). Die Kurzform kommt spaeter.
12. **`as` bindet staerker als die unaeren Operatoren** (`as` ist laut §10.1 ein
    Postfix-Operator). `&s.a as u64` bedeutet daher `&(s.a as u64)` und ist ein
    Fehler; gemeint ist `(&s.a) as u64`.
13. **Kein `break`/`continue`** (in §10.1 nicht vorgesehen): Schleifen werden
    ueber eine Bedingungsvariable verlassen (siehe `tests/057_*.fi`).
14. **Assembler-Ausgabe** ist Intel-Syntax mit `.intel_syntax noprefix`; §12
    laesst beide Syntaxformen zu. `as` und `ld` werden ausschliesslich als
    Assembler bzw. Linker aufgerufen, nie ein C-Compiler.
