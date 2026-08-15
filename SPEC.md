# Firn — Sprachspezifikation

**Arbeitstitel:** Firn · **Dateiendung:** `.fi` · **Stand:** v0.2 (2026-08-13)
**Autor:** Justin (GitHub: Flei123) · **Zielsysteme:** Karstos / karst-Kernel **und
die Karstos-Browser-Engine**, x86_64

> **Ergänzendes Dokument.** `DESIGNZIELE.md` behandelt zehn bekannte
> Schwachstellen heutiger Sprachen (Funktionsfarben, fehlbare Allokation,
> Capability-Module, stabiles ABI, Debug-Bau-Geschwindigkeit,
> In-Place-Initialisierung, comptime/Reflexion, Datenlayout/SoA, Hot Reload) und
> trennt, was **jetzt** ins Fundament muss von dem, was nachrüstbar ist. Wo es
> dieser Spezifikation widerspricht, gewinnt `DESIGNZIELE.md` und §7 bzw. §15
> werden korrigiert — das ist bereits geschehen.

> **Umbenennbarkeit.** Sprachname und Dateiendung stehen an *genau einer* Stelle im
> Compiler: `compiler/src/config.rs` (`LANG_NAME`, `FILE_EXT`, `LANG_NAME_LOWER`).
> Jede Fehlermeldung, jeder Hilfetext und jede Dateisuche liest von dort. Ein
> Umbenennen der Sprache ist eine Änderung von drei Konstanten, kein Refactoring.

---

## Änderungshistorie

### v0.2 (2026-08-13) — Browser-Anforderungen eingearbeitet

Auslöser: Die Entscheidung **B1** im Projekt `karstos-browser` — *jede Zeile
ausführbarer Code der Browser-Engine ist Firn.* Damit ist Firn nicht mehr eine
Sprache für einen Kernel, sondern der **kritische Pfad Nummer 1** eines
Ökosystems, das DOM, Layout, JavaScript-Engine, TLS/Krypto und einen Rasterizer
umfasst. Maßgebliches Anforderungsdokument:
`../karstos-browser/FIRN-ANFORDERUNGEN.md` (13 Abschnitte, Abnahme in §13).

Vier Entscheidungen aus v0.1 werden dadurch **revidiert**. Sie stehen hier offen,
weil eine Spezifikation, die ihre Kehrtwenden versteckt, wertlos ist:

| v0.1 sagte | v0.2 sagt | Warum |
|---|---|---|
| „**Kein GC** — auch nicht im App-Profil, nie" | **Opt-in-Tracing-GC** für markierte Typen (§3.5) | Der DOM ist ein zyklischer Objektgraph, und die JS-Engine braucht ohnehin einen Sammler. Ohne GC landet man bei Geckos Weg: Referenzzählung plus nachgerüstetem Zyklensammler — mehr Arbeit, nicht weniger |
| „**Vererbung** bewusst weggelassen" | **Einfachvererbung für `gc class`** (§4.4) | Der DOM *ist* eine Vererbungshierarchie (`Node`→`Element`→`HTMLElement`→…) und wird in Web IDL auch so normativ beschrieben. Nachbauen mit Komposition kostet mehr, als es spart |
| „Referenzen sind **immer** zweitklassig" | Zweitklassig bleibt der **Standard**; `Gc[T]` und `Rc[T]` sind **erstklassige** Zeiger (§3.2) | Zweitklassige Referenzen lösen das Lebensdauerproblem elegant, können aber prinzipiell keinen zyklischen Graphen ausdrücken. Genau das braucht der DOM |
| „**WASM-Backend** in v0.6, aarch64 in v0.5" | Beide **verschoben, ohne Termin** (§10.5) | `FIRN-ANFORDERUNGEN.md` §11 stellt ausdrücklich klar: für den Browser wird weder ein WASM- noch ein aarch64-Backend gebraucht. WASM-*Ausführung* im Browser ist ein Interpreter in Firn, kein Compiler-Backend. Das entlastet den Umfang erheblich |

Neu hinzugekommen: §8 (Zeichenketten, inkl. **WTF-16**), §9 (**Constant-Time**
für Krypto), §5.3 (**Abwicklung/Ausnahmen** für JS-Semantik), §6.4
(Kompilierzeit-Codegenerierung), §10.3 (Leistungsziel **≤ 2× Rust**), §16
(Rückverfolgung Anforderung → Abschnitt).

Unverändert geblieben: Profile (§2), Fehlerbehandlung über Ergebnistypen als
Normalweg (§5.1), `comptime` statt Makrosprache (§6), kein LLVM im
Bootstrap-Pfad (§10.2), Bootstrap-Stufen (§11), und der Stand der
Stufe-0-Umsetzung (§14).

### v0.1 (2026-08-13) — Erstfassung
Sprache für Kernel und Anwendungen, Prototyp `firnc0` gebaut und getestet.

---

## 0. Warum überhaupt eine eigene Sprache

Karstos ist heute in Rust geschrieben. Das funktioniert, erzeugt aber eine
Abhängigkeit, die Justin langfristig nicht will: Rust bestimmt, was der Kernel
darf (Rust-Editionen, `no_std`-Grenzen, LLVM-Zielunterstützung, Compiler-Bugs,
Projektpolitik). Wer ein eigenes Betriebssystem baut, aber den Compiler von
anderen bezieht, besitzt sein System nicht wirklich — er mietet es.

Seit der Browser-Entscheidung ist der Einsatz höher. Firn muss nicht nur einen
Kernel tragen, sondern **eine halbe Million Zeilen Browser-Engine**. Das ist die
härteste Prüfung, die es für eine Sprache gibt — und zugleich die Chance, dass
Firn dabei erwachsen wird. Ein Sprachdesign-Fehler kostet in dieser Lage nicht
Wochen, sondern Jahre. Deshalb ist dieses Dokument ausführlicher geworden und
entscheidet mehr, als für einen Kernel allein nötig wäre.

**Ehrlichkeit vorweg:** Das ist ein Jahrzehnt-Projekt. Rust brauchte 9 Jahre bis
1.0, Zig ist nach 11 Jahren bei 0.16 und noch nicht stabil. Dieses Dokument
beschreibt das *Ziel*; der begleitende Compiler implementiert den in §14
markierten Ausschnitt — aber diesen wirklich, bis zum laufenden Binary.

---

## 1. Leitsätze

1. **Nichts passiert versteckt.** Keine implizite Allokation, keine impliziten
   Typumwandlungen, kein Operator-Overloading, keine versteckten Funktionsaufrufe.
2. **Eine Sprache, zwei Profile.** Kernel-Code und Anwendungscode teilen Syntax,
   Typsystem und Compiler. Der Unterschied ist, welche *Fähigkeiten* verfügbar
   sind — nicht welcher Dialekt.
3. **Sicher per Default, unsicher auf Ansage.** Speicherfehler sind
   Übersetzungsfehler, außer man schreibt `unsafe` hin.
4. **Wer nicht bestellt, zahlt nicht.** GC, Vtables, Abwicklungstabellen,
   Bereichsprüfungen: jede dieser Fähigkeiten kostet nur den Code, der sie
   anfordert. Der Rasterizer, der Tokenizer und die Krypto-Primitive bezahlen
   nichts davon. Dieser Leitsatz ist neu in v0.2 und der wichtigste.
5. **Ein Weg, etwas zu tun.** Wo C++ fünf Wege hat und Rust drei, hat Firn einen.
6. **Der Compiler gehört uns.** Kein LLVM im Bootstrap-Pfad (§10.2).
7. **Lesbarkeit vor Kürze.** Der Compiler wird in sich selbst geschrieben sein;
   unlesbarer Code ist dann ein Systemrisiko.
8. **Ausnahmen von diesen Leitsätzen werden benannt, nicht verschwiegen.**
   Jede Stelle, an der Firn gegen Leitsatz 1 oder 5 verstößt (Vtables, GC,
   Abwicklung), steht in diesem Dokument mit Begründung und Preis.

---

## 2. Profile — `kernel` und `app`

Ein Modul deklariert sein Profil in der ersten Zeile:

```firn
profile kernel   // freistehend, kein Allokator, kein Runtime
profile app      // Standardbibliothek, Allokator, optional GC-Heap
```

Ohne Angabe gilt `app`. Der Compiler-Schalter `--profile=kernel` erzwingt das
Profil für die gesamte Übersetzungseinheit.

| Eigenschaft | `kernel` | `app` |
|---|---|---|
| Heap-Allokation | nur über explizit übergebenen `Allocator` | globaler Allokator verfügbar |
| Versteckte Allokation | verboten (Compilerfehler) | verboten (gleiche Regel) |
| `Rc[T]` (Zählung, azyklisch) | nicht verfügbar | verfügbar, explizit |
| **`Gc[T]` (Tracing-Sammler)** | **nicht verfügbar** | verfügbar, **opt-in pro Typ** (§3.5) |
| **Abwicklung / `throw`** (§5.3) | **verboten** | erlaubt in `#[unwinds]`-Funktionen |
| Panik bei Bereichsüberschreitung | ruft `karst_panic`, konfigurierbar | Laufzeit-Panik-Handler |
| Gleitkomma | nur mit `#[allow_fp]` (FPU-Zustand!) | frei |
| Stack-Tiefe prüfbar (`#[max_stack]`) | ja | ja |
| Ziel-Binärformat | ELF-Objekt, freistehend | ELF-Executable |

Der Browser läuft im `app`-Profil. Der Rasterizer, der Tokenizer und die
Krypto-Bibliothek laufen zwar im `app`-Profil, benutzen aber **keine**
GC-Typen und sind zusätzlich mit `#[no_gc]` markiert (§3.5.4) — der Compiler
stellt dann statisch sicher, dass in diesen Funktionen keine Sammlung ausgelöst
werden kann.

---

## 3. Speichermodell — die zentrale Entscheidung

`FIRN-ANFORDERUNGEN.md` §1 nennt das „die wichtigste Einzelentscheidung. Sie
kippt oder trägt das ganze Projekt." Diese Einschätzung wird geteilt.

### 3.1 Das Problem, genau benannt

Zwei Arten von Speicher stehen sich gegenüber und haben **gegensätzliche**
Anforderungen:

* **Heiße, azyklische Daten** — Tokenizer-Puffer, Rasterizer-Kantenlisten,
  Krypto-Zustand, Bilddecoder. Millionen Operationen pro Sekunde, kein Zyklus
  weit und breit, jede Indirektion und jede Barriere kostet messbar.
* **Der Objektgraph** — DOM-Knoten mit Eltern- *und* Kindverweisen, live
  `HTMLCollection`s, Event-Listener-Closures, die ihren Knoten festhalten,
  JS-Wrapper ↔ DOM-Knoten, Layout-Baum → DOM-Baum, Observer. **Durchgehend
  zyklisch, über eine Sprachgrenze hinweg.**

Eine einzige Strategie kann beides nicht gut. Der Fehler, den man hier machen
kann, ist, sich für *eine* zu entscheiden.

### 3.2 Die Entscheidung: drei Stufen, aufsteigend teuer

Firn hat **drei** Verweisarten. Der Standard ist die billigste; die teuren
müssen im Typ hingeschrieben werden.

| Stufe | Verweis | Kosten | Zyklen? | Erstklassig? | Wofür |
|---|---|---|---|---|---|
| **1** | Besitz + `&T` / `inout T` (zweitklassig) | **null** | nein | nein — nur Parameter | Standard. Tokenizer, Rasterizer, Krypto, Sammlungen, 90 % allen Codes |
| **2** | `Rc[T]` / `Weak[T]` — Zählung, unveränderlich | 1 Zähler, atomar nur bei `Arc[T]` | **leckt bei Zyklen** | ja | Geteilte *unveränderliche* Werte: berechnete Stilwerte, internierte Atome, Schriftdaten |
| **3** | `Gc[T]` / `GcWeak[T]` — Tracing-Sammler | Zuweisungsbarriere + Sammelpausen | **ja, löst sie auf** | ja | DOM-Knoten, JS-Objekte, Closures, Observer — und nur die |

**Stufe 1 bleibt der Normalfall und ist unverändert aus v0.1:** ein Eigentümer,
Zuweisung verschiebt (außer bei trivialen Typen), Referenzen leben nur im
Aufrufrahmen und dürfen weder gespeichert noch zurückgegeben werden. Deshalb
braucht Firn **keine Lebensdauer-Annotationen** und der Prüfer bleibt
intraprozedural. Details in §3.3.

**Stufe 3 ist die Neuerung von v0.2** und in §3.5 ausführlich beschrieben.

### 3.3 Stufe 1 — Besitz und zweitklassige Referenzen (unverändert)

* Jeder Wert hat **genau einen Eigentümer**. Zuweisung/Übergabe **verschiebt**,
  außer der Typ ist *trivial* (Ganzzahlen, `bool`, `f32`/`f64`, Rohzeiger,
  Arrays trivialer Typen, Structs mit ausschließlich trivialen Feldern). Es gibt
  **keinen** benutzerdefinierbaren Kopierkonstruktor.
* `&T` — geteilter, lesender Zugriff, beliebig viele.
  `inout T` — exklusiver, schreibender Zugriff, genau einer.
* **Zweitklassigkeit:** Referenzen dürfen nur Funktionsparameter sein und nach
  unten weitergereicht werden. Nicht in Structs, nicht in Arrays, nicht
  zurückgegeben, nicht in langlebige Variablen gebunden.

Daraus folgt, dass eine Referenz niemals ihren Aufrufrahmen überlebt — der Grund,
warum es keine `'a`-Parameter gibt. Vorbild: Hylo/Val („mutable value
semantics"), Swifts Parameterkonventionen.

**Preis, unverändert ehrlich:** keine Referenzfelder in Structs, keine
Funktionen, die Referenzen zurückgeben, keine selbstbezüglichen Strukturen.
Iteratoren werden als Index + Zugriffsfunktion oder als
`with_element(i, fn(inout T))` gebaut. Für Bäume und Graphen ohne GC:
`u32`-Handles in ein Arena-Array.

**Zerstörung:** deterministisch am Blockende, umgekehrte Deklarationsreihenfolge.
`fn drop(inout self)` ist ein normaler Aufruf an vorhersagbarer Stelle. `defer {}`
für typunabhängiges Aufräumen, `errdefer {}` nur auf dem Fehlerpfad. Verschobene
Werte werden nicht mehr freigegeben (statisches Move-Tracking; bedingte Moves
werden konservativ abgelehnt statt still mit einem Laufzeit-Flag versehen).
`#[must_consume]` erzwingt, dass ein Wert verbraucht wird (Lock-Guards,
`!T`-Ergebnisse, DMA-Puffer).

**Arenen** bleiben erstklassiges Werkzeug und sind für den Browser wichtiger als
je zuvor: Parser-Knoten, Layout-Zwischenergebnisse und Stilberechnung pro Frame
gehören in eine Arena mit `reset()`, nicht in den GC.

### 3.4 Stufe 2 — `Rc[T]`: geteilt, aber unveränderlich

Erfüllt `S7` aus den Anforderungen („geteilte, unveränderliche Werte mit
Zählung", für berechnete Stilwerte — Stylo macht das mit `Arc`).

* `Rc[T]` ist **immer unveränderlich**. Es gibt kein `RefCell`-Äquivalent, keine
  Innenveränderlichkeit über `Rc`. Wer teilen und ändern will, nimmt `Gc[T]`
  oder einen Lock.
* `Arc[T]` ist die fadensichere Variante (atomarer Zähler). Getrennter Typ, damit
  einfädiger Code den atomaren Zähler nicht bezahlt.
* `Weak[T]` bricht Zyklen manuell — für Fälle, in denen der Zyklus offensichtlich
  und lokal ist.
* **Zyklen lecken.** Das steht so in der Dokumentation und ist der Grund, warum
  `Rc` für den DOM ausdrücklich **nicht** vorgesehen ist.

### 3.5 Stufe 3 — der Opt-in-Tracing-GC

#### 3.5.1 Warum überhaupt, und warum nicht die Alternativen

Drei Wege standen zur Wahl (`FIRN-ANFORDERUNGEN.md` §1):

| Weg | Ergebnis |
|---|---|
| **Arena + typisierte Indizes** | Löst Zyklen elegant und ist schnell. Scheitert aber an der JS-Grenze: **die JS-Engine braucht zwingend einen Sammler**, weil JS-Objektlebensdauern nicht statisch bekannt sind. Man hätte also *zwei* Systeme, und die gefährlichste Zyklenklasse — DOM-Knoten ↔ JS-Wrapper — läuft genau über deren Grenze. Genau dort entstehen in echten Browsern die Lecks |
| **Referenzzählung + schwache Verweise** | Vorhersagbar, aber **Zyklen lecken**. Gecko hat deshalb `nsCycleCollector` nachgerüstet: ein Sammler, nur komplizierter und langsamer als ein richtiger. Der Weg endet dort, wo man nicht hin wollte, nur teurer |
| **Tracing-GC, opt-in pro Typ** | **Gewählt.** DOM und JS teilen sich einen Sammler, die Grenze existiert nicht. Ladybird belegt mit `LibGC`, dass eine überschaubare eigene Bibliothek dafür genügt |

Das Entscheidende ist das **opt-in**: Firn wird dadurch *keine* GC-Sprache.
Der GC ist ein Heap, den ein Typ ausdrücklich anfordert.

#### 3.5.2 Wie es aussieht

```firn
gc class Node {
    parent:   GcWeak[Node]        // schwach: bricht keinen Zyklus, ist aber
    children: GcVec[Gc[Node]]     //          semantisch richtig für Observer
    listeners: GcVec[Gc[Closure]] // Zyklus Knoten -> Closure -> Knoten: egal
}

gc class Element extends Node {
    tag:   Atom
    attrs: GcMap[Atom, Str]
}

fn append(parent: Gc[Node], child: Gc[Node]) {
    parent.children.push(child)
    child.parent = weak(parent)      // Zuweisungsbarriere: nur hier
}
```

* `gc class` ist die **einzige** Art, GC-verwaltete Werte zu deklarieren.
  Ein `gc class`-Wert existiert ausschließlich auf dem GC-Heap; es gibt ihn nicht
  auf dem Stack und nicht als Feld eines gewöhnlichen `struct`.
* `Gc[T]` ist ein **erstklassiger** Zeiger: speicherbar, zurückgebbar, in
  Sammlungen legbar. Genau das, was Stufe 1 nicht kann und der DOM braucht.
* `GcWeak[T]` erfüllt `S3` (Observer, Caches, Wrapper-Tabellen).
* `struct` bleibt, was es war: flach, wertbasiert, ohne GC, ohne Vtable.

#### 3.5.3 Wie der Sammler arbeitet — und was das kostet

| Eigenschaft | Entscheidung | Preis, offen benannt |
|---|---|---|
| **Heap-Verfolgung** | **präzise**. Jeder `gc class` bekommt vom Compiler eine erzeugte `trace`-Funktion aus dem bekannten Feldlayout | keiner nennenswert |
| **Stack-Verfolgung** | **konservativ** (Stack und Register werden nach Bitmustern durchsucht, die wie GC-Zeiger aussehen) | Zwei echte Nachteile: (a) **falsches Festhalten** — eine Ganzzahl, die zufällig wie ein Zeiger aussieht, hält ein Objekt am Leben; (b) **kein verschiebender Sammler möglich**, weil man einen konservativ gefundenen Wurzelzeiger nicht umschreiben darf. Also **kein Kompaktieren**, Fragmentierung muss der Allokator per Größenklassen erledigen |
| Warum trotzdem konservativ | Präzise Stack-Karten zwingen den Codegenerator zu Safepoint-Metadaten an jedem Aufruf und schränken die Registerzuteilung ein. Das ist viel Arbeit und bremst genau den Optimierer, der laut §10.3 auf ≤ 2× Rust kommen muss. **Ladybirds `LibGC` scannt ebenfalls konservativ** und trägt damit einen echten Browser | Der Preis ist gedeckelt und messbar; die Alternative kostet Monate am kritischen Pfad |
| **Sammelzeitpunkt** | **nur an Allokationsstellen** eines GC-Typs. Kein preemptives Sammeln, keine Signale, keine Safepoint-Polls in Schleifen | Eine Endlosschleife ohne GC-Allokation blockiert eine Sammlung — akzeptabel, weil sie dann auch keinen Müll erzeugt |
| **Algorithmus** | Mark-Sweep, **inkrementell mit Dreifarbenmarkierung** ab v0.5 (`S5`), Dijkstra-Einfügebarriere beim Schreiben eines `Gc[T]`-Feldes | Die Barriere kostet — aber **nur** beim Schreiben von `Gc[T]`-Feldern. Nicht-GC-Code führt sie nie aus (Leitsatz 4) |
| **Pausenzeiten** | messbar über `gc.stats()`, begrenzbar über `gc.set_budget(ms)` (`S6`) | |
| **Finalisierer** | `fn finalize(inout self)` (`S4`), läuft **nach** dem Einsammeln, darf **nicht** wiederbeleben und keine neuen GC-Objekte anlegen — der Compiler prüft das | eingeschränkt, dafür berechenbar |
| **Fäden** | Ein GC-Heap **pro Faden**, keine `Gc[T]`-Übergabe zwischen Fäden (`Gc[T]` ist nicht sendbar, §7) | Paralleles Layout arbeitet auf Arena-Daten, nicht auf GC-Daten. Das ist eine echte Einschränkung und steht hier, damit sie beim Layout-Entwurf bekannt ist |

#### 3.5.4 `#[no_gc]` — die Garantie für heiße Pfade

```firn
#[no_gc]
fn tokenize(input: &[u8], out: inout TokenBuf) -> usize { ... }
```

In einer `#[no_gc]`-Funktion sind verboten: GC-Allokation, Aufruf von Funktionen
ohne `#[no_gc]`, Schreiben in `Gc[T]`-Felder. Der Compiler prüft das
**transitiv** und meldet einen Fehler mit Zeile/Spalte, wenn die Kette bricht.
Daraus folgt statisch: **in diesem Aufrufbaum kann keine Sammlung stattfinden**,
es gibt keine Barriere und keine Pause. Das ist die Zusage an Rasterizer,
Tokenizer und Krypto.

#### 3.5.5 Abnahme

`FIRN-ANFORDERUNGEN.md` §13 Punkt 2 / `TODO-FIRN.md` 0.9: DOM-Prototyp mit
Eltern-/Kind-Zyklen und Listener-Zyklen, **24 h Dauerlauf ohne
Speicherwachstum**. Bis dieser Test läuft, gilt der GC als *entworfen*, nicht als
*belegt*. Siehe `ABNAHME.md`.

### 3.6 Rohzeiger und `unsafe`

`*T` und `*mut T`. Dereferenzieren, Zeigerarithmetik, `transmute`, MMIO und
Inline-Assembler nur in `unsafe { }`. `unsafe` schaltet **nur** diese Operationen
frei; Moves und Typregeln gelten weiter. Jeder Block braucht eine Begründung als
Kommentar (`--deny=undocumented-unsafe`, Standard im `kernel`-Profil). Erfüllt
`L3` — der Rasterizer ist ohne diese Ausnahme nicht schnell schreibbar.

---

## 4. Herkunft der Ideen — und was bewusst fehlt

**Von Rust:** Eigentum und Moves, `unsafe` als Grenze, Ergebnistypen statt
Ausnahmen im Normalfall, Aufzählungen mit Nutzdaten und erschöpfendes
Mustervergleichen, Modulsystem mit expliziten Exporten, Ausdrucksorientierung.

**Von Zig:** `comptime` statt Templates und Makros, „keine versteckte Steuerung",
explizit übergebene Allokatoren, `defer`/`errdefer`, Fehler als Wertetyp,
Übersetzungszeit-Reflexion, `test`-Blöcke in der Quelldatei.

**Von C:** flaches, vorhersagbares Speicherlayout, direkte Abbildbarkeit auf
Maschinenbefehle, kleine Kernsprache, ABI-Ehrlichkeit.

**Von Ladybird/`LibGC` (neu in v0.2):** der opt-in-Tracing-GC mit konservativem
Stack-Scan als bewiesen tragfähiger Weg für eine kleine Mannschaft.

### 4.4 Vererbung — die Kehrtwende, offen begründet

v0.1 hat Vererbung ausgeschlossen. v0.2 lässt sie **eingeschränkt** zu:

* **nur für `gc class`**, nicht für `struct`, nicht für Aufzählungen
* **nur einfach** (eine Basis), keine Mehrfachvererbung
* Felder sind **nie** virtuell; Methoden nur mit `virtual` / `override`
* Aufwärtsumwandlung ist kostenlos (`Gc[HTMLElement]` → `Gc[Node]`),
  Abwärtsumwandlung ist geprüft: `node.as?[HTMLInputElement]` liefert
  `?Gc[HTMLInputElement]` und prüft die Typkennung zur Laufzeit

Begründung: Der DOM ist in Web IDL **normativ** als Vererbungshierarchie
beschrieben — `Node` → `Element` → `HTMLElement` → `HTMLInputElement`, über
hundert Klassen tief. `L6` verlangt ausdrücklich statische **und** dynamische
Auflösung. Die Alternative (Komposition mit `base`-Feld plus manueller
Aufwärtsumwandlung und selbstgebauten Typkennungen) baut Vererbung nach, nur
schlechter und mit mehr Handarbeit. Wo eine Sprache eine Domäne modellieren
muss, die selbst eine Hierarchie ist, ist Vererbung das ehrliche Werkzeug.

**Preis:** Firn hat damit Vtables und verletzt Leitsatz 1 („nichts versteckt")
an einer Stelle. Der Preis wird begrenzt: nur `gc class`, nur bei `virtual`
markierten Methoden, und `struct` bleibt vollständig frei davon. Ein Rasterizer
sieht nie eine Vtable.

### 4.5 Bewusst weggelassen

| Weggelassen | Grund |
|---|---|
| Lebensdauer-Parameter (`'a`) | größter Komplexitätstreiber in Rust; §3.3 macht sie überflüssig |
| Mehrfachvererbung, Klassen für Wertetypen | §4.4 ist die eng gezogene Ausnahme |
| Operator-Overloading | `a + b` soll eine Addition sein |
| Implizite Umwandlungen (auch verlustfreie) | `u8 → u32` schreibt man `as u32` |
| Ausnahmen als *normaler* Fehlerweg | `L7`: Parser erzeugen dauernd erwartbare Fehler; Ausnahmen dafür sind Gift für die Geschwindigkeit. §5.3 ist die eng gezogene Ausnahme für JS |
| Makros mit eigener Syntax | `comptime` und Bauskripte reichen (`FIRN-ANFORDERUNGEN.md` §11: „kein turingvollständiges Makrosystem") |
| `async`/`await` **als Sprachfarbe** | §7 — ersetzt durch `Io` als Parameter (`DESIGNZIELE.md` §1) |
| Überladen von Funktionsnamen | erschwert Fehlermeldungen und Selbst-Hosting |
| Automatische Dereferenzierung | `p.*.feld`, nicht `p.feld`. Ausnahme: `Gc[T]` wird automatisch dereferenziert, weil `node.*.children.*` unlesbar wäre — das ist bewusst und steht hier |
| Vorprozessor | `comptime if` ersetzt `#ifdef` |
| **JIT / Codeerzeugung zur Laufzeit** | `FIRN-ANFORDERUNGEN.md` §11: die JS-Engine ist ein Bytecode-Interpreter. Ladybird hat seinen JIT 2024 wieder ausgebaut, V8 hat einen jitless-Modus |
| **Dynamische Bibliotheken** | statisch gelinkt; spart `dlopen` in Karstos komplett (`R5`) |
| **C++-Interop** | bei B1 gibt es keinen C++-Code. Genau das hat Ladybird bei Swift 1,5 Jahre gekostet |

---

## 5. Fehlerbehandlung

Drei streng getrennte Kategorien. Die dritte ist neu in v0.2.

### 5.1 Erwartete Fehler — Ergebnistypen (`L7`, der Normalweg)

```firn
error IoError { NotFound, Permission, Closed }

fn read_all(path: &Str, alloc: inout Arena) -> IoError!Buf {
    let fd = try open(path)
    errdefer close(fd)
    ...
    return buf
}

let buf = read_all(&p, inout a) catch |e| {
    match e { IoError.NotFound => return default_buf(), else => return e }
}
```

`!T` ist eine Union aus Fehlermenge und Erfolgstyp; die Fehlermenge darf
weggelassen und vom Compiler **abgeleitet** werden. Ein nicht behandeltes `!T`
kann nicht fallengelassen werden (`#[must_consume]`). Kein Abwickeln, keine
Landing-Pads — das ist der Pfad, den ein Parser millionenfach geht.

### 5.2 Programmierfehler — `panic`

Indexüberlauf, Division durch null, verletzte Zusicherung. Nicht behandelbar,
ruft einen Handler (`app`: Meldung + Abbruch; `kernel`: `karst_panic`).
`--release-fast` schaltet die Prüfungen ab; dann ist der Fall undefiniert und das
steht so in der Dokumentation.

**Stapelüberlauf (`L12`)** gehört hierher: HTML-Tree-Builder und JS-Interpreter
rekursieren tief, und eine tief verschachtelte Seite darf kein Absturz sein.
Firn löst das mit (a) Schutzseiten, die einen sauberen Panik-Pfad auslösen statt
`SIGSEGV`, und (b) der Eigenfunktion `stack_remaining() -> usize`, mit der ein
rekursiver Abstieg vor dem nächsten Schritt aufgeben kann — das ist der Weg, den
die HTML-Spezifikation für Verschachtelungsgrenzen ohnehin vorsieht.

### 5.3 Abwicklung für JS-Semantik (`L8`) — neu in v0.2

JavaScript kennt `throw`, Web IDL wirft `DOMException`, und beides muss durch
hunderte Aufrufebenen des Interpreters und der Bindungen zurücklaufen. Alles über
`!T` zu führen hieße, jede einzelne IDL-Funktion zu einer Ergebnisfunktion zu
machen — der Aufwand und der Codeumfang wären erheblich.

**Entscheidung: Firn bekommt eine Abwicklung, aber streng eingezäunt.**

* Nur Funktionen mit `#[unwinds]` dürfen `throw` auslösen oder durchlassen.
  Ohne diese Markierung ist eine Funktion abwicklungsfrei — der Compiler prüft
  das transitiv wie bei `#[no_gc]`.
* **Tabellengesteuerte Abwicklung in zwei Phasen**, kein `setjmp`. Kosten auf dem
  Erfolgspfad: **null Instruktionen**. Kosten im Binärabbild: Abwicklungstabellen
  — aber nur für `#[unwinds]`-Funktionen.
* Beim Abwickeln laufen `drop` und `defer` der verlassenen Rahmen. Das ist die
  Stelle, an der Abwicklung teuer zu implementieren ist, und sie wird hier
  ausdrücklich als Aufwandsposten benannt.
* Im `kernel`-Profil ist `throw` **verboten**.
* `catch` in §5.1 (für `!T`) und `catch` in §5.3 (für `throw`) sind
  **verschiedene** Konstrukte mit verschiedener Syntax (`catch |e|` gegenüber
  `try { } catch (e) { }`), damit sie nicht verwechselt werden.

**Ehrliche Einordnung:** Das ist die zweite Stelle (nach §4.4), an der der
Browser Firn etwas aufzwingt, das eine Systemsprache sonst nicht bräuchte. Der
Zaun (`#[unwinds]`) ist die Gegenmaßnahme.

---

## 6. Metaprogrammierung

### 6.1 `comptime` — Generics ohne zweites Typsystem

```firn
fn max[comptime T: type](a: T, b: T) -> T {
    comptime assert(is_ordered(T), "max verlangt einen ordnungsfähigen Typ")
    return if a > b { a } else { b }
}

struct Vec[comptime T: type] { data: *mut T, len: usize, cap: usize }

comptime if target.arch == Arch.X86_64 { ... } else { ... }
```

Monomorphisierung (`L5`), kein Typlöschen, keine versteckten Vtables.
**Nachteil offen benannt:** Fehler in generischem Code zeigen sich erst bei der
Instanziierung. Gemildert durch `comptime assert` und deklarierte Anforderungen
(`where has_method(T, "next")`), bleibt aber schlechter als Rusts Traits.

### 6.2 Schnittstellen (`L6`)

`interface` mit **statischer** Auflösung (monomorphisiert, Standardfall) und
**dynamischer** Auflösung über `dyn Interface` (Vtable, ausdrücklich
hingeschrieben). Zusammen mit §4.4 deckt das die DOM-Hierarchie ab: `gc class`
für die Hierarchie selbst, `interface` für querschnittliche Fähigkeiten
(`EventTarget`, `Serializable`).

### 6.3 Aufzählungen und Musterabgleich (`L4`)

Summentypen mit Nutzdaten, `match` mit **Vollständigkeitsprüfung zur
Übersetzungszeit** — ein nicht abgedeckter Fall ist ein Fehler, kein Warnhinweis.
Der HTML-Tokenizer hat rund 80 Zustände, CSS-Werte eine riesige Variantenmenge;
ohne beides wird das unwartbar. Der Codegenerator erzeugt **Sprungtabellen**, wo
die Varianten dicht liegen (`P4`) — 80 Zustände dürfen keine 80 Vergleiche sein.

### 6.4 Kompilierzeit-Codegenerierung (`G1`–`G4`) — neu in v0.2

Ein Browser besteht zu einem erheblichen Teil aus *erzeugtem* Code: Web-IDL-
Bindungen (Ladybird: 697 `.idl`-Dateien), CSS-Eigenschaftstabellen, 2.231
HTML-Entities, Unicode-Tabellen aus der UCD, CLDR-Daten.

* **Bauskripte** (`build.fi`) laufen vor der Übersetzung, sind gewöhnliche
  Firn-Programme und schreiben Firn-Quelltext. Kein turingvollständiges
  Makrosystem — bewusst, siehe §4.5.
* Erzeugter Quelltext ist lesbar und behält über `#line`-Angaben den Bezug zur
  Eingabedatei, damit der Debugger etwas Sinnvolles zeigt (`G2`).
* Für große statische Tabellen (`G4`) liefert die Standardbibliothek
  **perfektes Hashing** und **komprimierte Tries** als Bauzeit-Werkzeug.
* Abnahme (`FIRN-ANFORDERUNGEN.md` §13 Punkt 6): eine Unicode-Tabelle wird aus
  der UCD erzeugt.

---

## 7. Nebenläufigkeit

* **Bausteine in der Sprache:** `atomic[T]` mit expliziter Speicherordnung,
  `fence`, und die Regeln aus §3. Speichermodell nach Happens-before (`N2`).
* **Datenrennen-Prävention (`N4`):** Zwei Markierungen, entsprechend Rusts
  `Send`/`Sync`, aber vom Compiler abgeleitet statt von Hand implementiert:
  `#[sendable]` und `#[shareable]`. **`Gc[T]` ist weder das eine noch das
  andere** — GC-Heaps sind fadenlokal (§3.5.3). Paralleles Layout arbeitet
  deshalb auf Arena-Daten und `Arc[T]`-Stilwerten, nicht auf DOM-Knoten. Diese
  Einschränkung ist eine Folge der GC-Entscheidung und steht hier, damit sie
  beim Layout-Entwurf bekannt ist, nicht erst beim Debuggen.
* **Kein `async`/`await` als Sprachfarbe** (revidiert 14.08.2026, ausführlich
  begründet in `DESIGNZIELE.md` §1). Eine `async`-Markierung färbt jeden
  Aufrufer und zerreißt das Ökosystem — in Rust gibt es deshalb zwei
  inkompatible E/A-Welten. Firn übernimmt stattdessen **Zigs Modell aus 0.16**:
  **`Io` wird als Parameter übergeben**, genau wie der `Allocator`.
  `io.async(f, …)` drückt *Unabhängigkeit* aus und ist unfehlbar;
  `io.concurrent(…)` fordert echte Gleichzeitigkeit und darf scheitern.
  `Future[T]` ist `#[must_consume]`, Abbruch (`cancel`) gehört zum Vertrag.
  Damit ist `N6` **erfüllt, ohne den Compiler anzufassen** — es gibt keine
  Zustandsmaschinen-Transformation und kein `async`-Schlüsselwort. Umsetzung:
  `Io.Threaded`, `Io.SingleThread` (deterministisch, erfüllt `N7`), später
  `Io.Evented` mit stapelvollen Koroutinen. Preis: ein Stapel je Koroutine, und
  `Io` muss durchgereicht werden.
* **Strukturierte Nebenläufigkeit** als Bibliotheksmuster über `#[must_consume]`.
* **Deterministischer Einzelfadenmodus** für reproduzierbare Reftests (`N7`).

---

## 8. Zeichenketten — neu in v0.2

`FIRN-ANFORDERUNGEN.md` §3 nennt das „unterschätzt, aber kritisch". Zu Recht:
Der Zeichenkettentyp zieht sich durch jedes Modul und ist nachträglich nicht mehr
zu ändern.

### 8.1 Vier getrennte Typen

| Typ | Inhalt | Wohlgeformt? | Wofür |
|---|---|---|---|
| `Bytes` | rohe Oktette | — | Netzwerkdaten, Dateien, Bilddaten. **Kein Text** (`Z7`) |
| `Str` | UTF-8 | **garantiert** — wird an der Grenze geprüft | Alles Interne der Web-Plattform (`Z1`) |
| **`Str16`** | Folge von `u16`-Codeeinheiten | **ausdrücklich nicht** | **JavaScript-Zeichenketten** (`Z2`) |
| `Atom` | `u32`-Kennung in eine globale Tabelle | — | Tag-, Attribut-, Eigenschaftsnamen (`Z4`) |

Die Trennung `Bytes` ↔ `Str` erzwingt der Compiler: `Bytes` wird nie
stillschweigend zu Text.

### 8.2 `Str16` und WTF-16 — der Punkt, den fast alle übersehen

JavaScript-Zeichenketten sind Folgen von 16-Bit-Codeeinheiten und dürfen
**einzelne, ungepaarte Surrogate** enthalten. `"\uD800"` ist ein gültiger,
alltäglicher JS-String. Eine Sprache, deren Zeichenkettentyp nur wohlgeformtes
Unicode zulässt, **kann JavaScript nicht korrekt umsetzen** — sie würde entweder
ersetzen (falsches Ergebnis) oder abbrechen (falsches Verhalten).

* `Str16` prüft **nichts** und normalisiert **nichts**. Es ist ein `[]u16` mit
  Zeichenketten-Operationen. Das ist Absicht, kein Versäumnis.
* Umwandlungen sind **explizit** und ihre Fehlbarkeit steht im Typ:
  * `Str → Str16` gelingt immer
  * `Str16 → Str` ist fehlbar: `to_utf8() -> ?Str` (nichts bei ungepaartem
    Surrogat) bzw. `to_utf8_lossy() -> Str` (ersetzt durch U+FFFD)
  * **`Wtf8`** als verlustfreie Brücke: kann ungepaarte Surrogate in einer
    UTF-8-artigen Kodierung halten. Für Zwischenspeicherung und IPC
* Negativprüfung ist Abnahmebestandteil: ein Test konstruiert `Str16` mit
  ungepaartem Surrogat, prüft, dass es **erhalten** bleibt, dass `to_utf8()`
  nichts liefert und `to_utf8_lossy()` U+FFFD liefert.

### 8.3 Atome (`Z4`)

Tag- und Attributnamen werden beim Selektor-Abgleich millionenfach verglichen.
`Atom` ist eine internierte `u32`-Kennung; Vergleich ist ein
Ganzzahlvergleich. Häufige Atome (`div`, `class`, `id`, …) werden zur
**Bauzeit** vergeben (§6.4) und haben feste, kleine Nummern — damit sind
`match`-Sprungtabellen über Tag-Namen möglich.

### 8.4 Zahlen ↔ Text (`Z5`, `Z6`)

Zwei Anforderungen, die naiv umgesetzt garantiert falsch sind:

* **`strtod`, korrekt gerundet.** CSS-Zahlen und `parseFloat` müssen bitgenau
  stimmen. Umsetzung: schneller Weg über 128-Bit-Festkomma (Eisel-Lemire) mit
  Rückfall auf exakte Großzahlarithmetik in den Randfällen.
* **Kürzeste Ausgabe mit Rückwandlungsgarantie** (Ryū/Grisu-Klasse).
  `Number.prototype.toString` ist in ECMAScript **exakt** vorgeschrieben; eine
  naive Ausgabe fällt in test262 durch.

Beide gehören in die Standardbibliothek, nicht in den Compiler, und werden gegen
die öffentlichen Testvektoren geprüft.

### 8.5 Seile (`Z3`)

`document.write` und JS-Verkettung in Schleifen erzeugen quadratische Kosten,
wenn jede Verkettung kopiert. Vorgesehen ist eine Seilstruktur (`Rope`) als
Bibliothekstyp — **SOLL**, nicht **MUSS**, und deshalb ohne Termin.

---

## 9. Constant-Time und Krypto — neu in v0.2

`FIRN-ANFORDERUNGEN.md` §7: „der Punkt, den fast alle übersehen." Bei B1 wird
TLS 1.3 selbst geschrieben. Und hier steht die Anforderung **im direkten
Widerspruch** zu §10.3: Ein Optimierer, der `if (secret)` in einen Sprung
umschreibt oder ein `memset` von Schlüsselmaterial als toten Code entfernt,
zerstört die Sicherheit. Genau deshalb muss das **jetzt** ins Sprachdesign und
kann nicht nachgerüstet werden — ein Optimierer, der ohne Bremse gebaut wurde,
lässt sich später nicht überreden.

### 9.1 `secret[T]` — Geheimnis als Typ

```firn
fn ct_eq(a: &[secret[u8]], b: &[secret[u8]]) -> secret[bool] {
    var acc: secret[u8] = 0
    var i: usize = 0
    while i < a.len {
        acc = acc | (a[i] ^ b[i])      // erlaubt: datenunabhängig
        i = i + 1
    }
    return acc == 0                     // ergibt secret[bool]
}
```

`secret[T]` ist ein Typqualifizierer auf Ganzzahl- und `bool`-Typen. Der
Typprüfer verbietet darauf:

* **Verzweigen**: `if secret_bool { }` ist ein **Compilerfehler**, kein
  Warnhinweis. Stattdessen `select(cond, a, b)` → `cmov`.
* **Indizieren**: `tabelle[secret_index]` ist ein Fehler (Cache-Seitenkanal,
  `C4`). Stattdessen ein konstantzeitiges Auswahlmuster über die ganze Tabelle.
* **Dividieren** und `%`: variable Latenz auf realer Hardware — verboten.
* **Ausgeben** oder in nicht-`secret` umwandeln: nur über `declassify(x)`, das
  im Quelltext auffällt und in Prüfungen gesucht werden kann.

Die Markierung breitet sich durch Ausdrücke aus: `secret[u8] ^ u8` ergibt
`secret[u8]`.

### 9.2 Die Zusage des Optimierers (`C1`, `C2`)

Die `secret`-Markierung wird bis in die IR getragen. Für Werte mit dieser
Markierung gilt für **jeden** Optimierungsdurchgang:

* `select` darf **niemals** in eine Verzweigung umgeschrieben werden
  (die naheliegende „Optimierung" bei vorhersagbaren Bedingungen ist verboten)
* Speicherzugriffe dürfen nicht in datenabhängige Zugriffe umgeformt werden
* Speicherschreibvorgänge auf `secret`-Speicher gelten **nie** als tot
* `barrier(inout x)` ist eine undurchsichtige Sperre, die jeder Durchgang
  respektiert

**Prüfbar, nicht nur behauptet:** `#[constant_time]` auf einer Funktion schaltet
eine Prüfung **im Codegenerator** ein. Entsteht dort ein bedingter Sprung, dessen
Bedingung von einem `secret`-Wert abhängt, bricht die Übersetzung mit einem
Fehler ab. Das ist stärker als ein nachgelagerter Test — aber der Test
(Assembler-Inspektion) bleibt zusätzlich, weil eine Prüfung im Compiler auch
falsch sein kann.

### 9.3 Weitere Krypto-Anforderungen

| # | Anforderung | Umsetzung in Firn |
|---|---|---|
| `C3` | Speicher zuverlässig löschen | `secure_zero(inout buf)` — eingebaut, für DCE unantastbar |
| `C5` | 128-Bit-Arithmetik | `u128`/`i128` als echte Typen; zusätzlich `mul_wide(u64, u64) -> (u64, u64)` als Eigenfunktion (bildet auf `mul` ab) |
| `C6` | AES-NI, SHA-Extensions, CLMUL | über Inline-Assembler (`L15`) und später benannte Eigenfunktionen — **SOLL** |
| `C7` | CSPRNG des Systems | `sys.random(inout buf)`, im `app`-Profil Pflichtbestandteil der Laufzeit |

---

## 10. Backend-Strategie

### 10.1 Aufbau

```
Quelle .fi
  └─> Lexer ──> Parser ──> AST
        └─> Resolver (Namen, Module)
              └─> Typprüfer + Move-/Referenzprüfer + secret-Prüfung
                    └─> HIR (entzuckert: for→while, comptime aufgelöst,
                             Monomorphisierung)
                          └─> FIR  (eigene IR, SSA-artig, Basisblöcke,
                                    trägt secret- und gc-Markierungen)
                                ├─> Optimierer (§10.3)
                                └─> Backends:
                                     ├─ x86_64  (eigener Codegen)   ← einziges Ziel
                                     ├─ aarch64 (verschoben, §10.5)
                                     ├─ wasm32  (verschoben, §10.5)
                                     └─ llvm-ir (optional, nie im Bootstrap-Pfad)
```

**FIR** ist die Sollbruchstelle: typisiert, Basisblöcke mit genau einem
Terminator, keine x86-Eigenheiten, dokumentiert in `docs/FIR.md`, mit
Textformat für Tests. Neu in v0.2: FIR trägt die `secret`-Markierung (§9.2) und
weiß, welche Werte GC-Zeiger sind (für die erzeugten `trace`-Funktionen).

### 10.2 Warum nicht LLVM — unverändert, aber teurer geworden

Die Begründung aus v0.1 gilt weiter: Unabhängigkeit ist das Projektziel;
Selbst-Hosting verlangt, dass der Compiler in Firn keine C++-ABI bedienen muss;
im Kernel erzeugt LLVM eigenmächtig Aufrufe (`memcpy`, Gleitkomma-Hilfen) und hat
eigene Vorstellungen von Stack-Probing; und der gesamte Compiler soll in unter
einer Minute reproduzierbar baubar sein.

**Was sich geändert hat:** Mit dem Leistungsziel aus §10.3 ist diese Entscheidung
deutlich teurer als in v0.1. Damals war „2–5× langsamer als LLVM" hinnehmbar.
Jetzt lautet die Anforderung `P9`: **≤ 2× Rust**, gemessen. Das heißt, ein
erheblicher Teil der Compilerarbeit fließt in den Optimierer, nicht in
Sprachmerkmale.

Ein zusätzliches Argument ist in v0.2 dazugekommen und wiegt schwer: **§9
verlangt einen Optimierer mit Bremse.** LLVM gibt diese Zusage nicht — es gibt
in LLVM keine belastbare Garantie, dass ein `select` nicht doch zu einem Sprung
wird. Wer Constant-Time-Krypto ernst meint, ist mit einem eigenen Optimierer
sogar besser dran. Das war 2026 kein geplanter Vorteil, ist aber einer.

**Tür bleibt offen:** ein `llvm`-Backend hinter einem Schalter, als
Vergleichsmaßstab („was hätte LLVM daraus gemacht?"). Nie im Bootstrap-Pfad.

### 10.3 Leistungsziel: ≤ 2× Rust (`P1`–`P9`) — neu in v0.2

**Das ist die Anforderung, an der Firn-only realistisch scheitern kann.** Ein
HTML-Tokenizer läuft über jedes Zeichen jeder Seite. Ist er 10× zu langsam, ist
der Browser 10× zu langsam, und das lässt sich später nicht herausoptimieren.

Pflichtdurchgänge im Optimierer:

| # | Durchgang | Warum |
|---|---|---|
| `P1` | **Inlining, auch über Modulgrenzen** | ohne das ist jeder Zugriffsmethodenaufruf ein echter Call |
| `P2` | Konstantenfaltung, Vereinfachung, tote Code-Entfernung | Grundlage, in Stufe 0 vorhanden |
| `P3` | **Registerzuteilung** (linear scan; Graphfärbung später) | Stufe 0 hat *keine* — sie legt jeden Wert auf den Stack. Das ist der größte Einzelposten |
| `P4` | **Sprungtabellen für `match`** | 80 Tokenizer-Zustände dürfen keine 80 Vergleiche sein |
| `P5` | Bereichsprüfungen entfernen, wo beweisbar | sonst kostet Speichersicherheit im Rasterizer 20–40 % |
| `P6` | Schleifeninvarianten herausziehen, Abrollen | **SOLL** |
| `P7` | **Layout-Kontrolle über Strukturen** (`#[packed]`, `#[align]`, feste Feldreihenfolge) | ein DOM-Knoten wird millionenfach angelegt |

**Messung, nicht Behauptung:** Eine Mikrobenchmark-Suite vergleicht dieselben
Programme in Firn und in Rust auf demselben Rechner. Das Ergebnis wird als Zahl
dokumentiert — **auch wenn das Ziel verfehlt wird**. Ein ehrlich gemessenes
„3,4× langsamer" ist wertvoll; ein behauptetes „ungefähr 2×" ist wertlos.

### 10.4 Debug-Informationen und Werkzeuge (`W1`–`W9`)

500.000 Zeilen ohne Debugger sind unpflegbar (`W3`). Deshalb gehören zum
Backend: **DWARF-Grundlagen** (Zeilentabelle, Funktionsbereiche, danach
Variablenorte), ein **Testrunner mit maschinenlesbarer Ausgabe** (`W2` — ohne
den gibt es keine WPT-Quoten und keine Fortschrittskurve), **Paketverwaltung mit
reproduzierbarem Bau** (`W1`), sowie **Profiler** (`W4`) und
**Fuzzing-Anbindung** (`W5`), letztere weil Parser und Bilddecoder bei B1 die
Hauptangriffsfläche sind und es keine gehärtete Fremdbibliothek gibt.

### 10.5 Was verschoben wurde

`FIRN-ANFORDERUNGEN.md` §11 stellt klar, was **nicht** gebraucht wird:

* **kein aarch64-Backend**, bis Karstos auf ARM zielt
* **kein WASM-Backend** — die WASM-*Ausführung* im Browser ist ein Interpreter
  in Firn, kein Compiler-Backend
* **kein JIT**, keine Codeerzeugung zur Laufzeit
* **keine dynamischen Bibliotheken**, **keine C++-Interop**

v0.1 hatte aarch64 für v0.5 und WASM für v0.6 vorgesehen. Beides ist gestrichen
und ohne Termin. Der Traum „Firn im Browser statt JavaScript" bleibt — aber er
ist jetzt ein *späteres* Ziel und blockiert nichts.

---

## 11. Bootstrap-Plan

| Stufe | geschrieben in | übersetzt von | Ergebnis | Status |
|---|---|---|---|---|
| **0** | Rust | `cargo`/`rustc` | `firnc0` — übersetzt die Teilmenge aus §14 | **läuft** |
| **1** | Firn (Teilmenge §14) | `firnc0` | `firnc1` | v0.3 |
| **2** | Firn (voller Umfang) | `firnc1` | `firnc2`, danach übersetzt `firnc2` sich selbst | v0.4 |
| **3** | — | — | **Fixpunkt:** `firnc2` und `firnc2'` bit-identisch → Selbst-Hosting (`L1`, Abnahme §13.1) | v0.5 |
| **4** | — | — | `firnc0` eingefroren, vorkompiliertes `firnc1` archiviert (Gegenmaßnahme gegen „Trusting Trust") | v0.6 |

Regeln: Stufe 1 darf nur benutzen, was `firnc0` beherrscht. Jede Stufe wird durch
die **vollständige** Testsuite geprüft, nicht durch „baut durch". Der
Fixpunkt-Vergleich ist die einzige belastbare Korrektheitsaussage.

---

## 12. Syntax

```firn
profile app

import std.io

const MAX: u32 = 100

struct Point { x: i32, y: i32 }

enum Shape { Circle(f64), Rect(Point, Point) }

gc class Node { parent: GcWeak[Node], children: GcVec[Gc[Node]] }

fn dist2(p: &Point, q: &Point) -> i32 {
    let dx = p.x - q.x
    let dy = p.y - q.y
    return dx * dx + dy * dy
}

fn main() -> i32 {
    let p = Point{ x: 3, y: 4 }
    let q = Point{ x: 0, y: 0 }
    if dist2(&p, &q) == 25 { return 0 } else { return 1 }
}
```

`let` unveränderlich, `var` veränderlich. Eckige Klammern für generische
Parameter (`Vec[u8]`), damit `<` eindeutig Vergleich bleibt und der Parser ohne
Rückverfolgung auskommt. Semikolon optional. Sichtbarkeit über eine
`export`-Liste pro Modul, nicht am einzelnen Element.

### 12.1 Grammatik der v0-Teilmenge (EBNF)

Das ist die Grammatik, die `firnc0` **wirklich** umsetzt. Die Erweiterungen aus
§3–§9 (`gc class`, `enum`/`match`, `interface`, `secret`, `throw`) sind darin
absichtlich noch nicht enthalten.

```ebnf
program     = { item } ;
item        = fn_decl | struct_decl | const_decl | profile_decl
            | import_decl | export_decl ;          (* Runde 2 *)
profile_decl= "profile" ident ;
import_decl = "import" ident { "." ident } ;       (* Runde 2 *)
export_decl = "export" "{" [ ident { "," ident } ] "}" ;   (* Runde 2 *)
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
            | for_stmt | jump_stmt                 (* Runde 2 *)
            | return_stmt | expr_stmt | block ;
for_stmt    = "for" ident "in" expr ".." expr block ;      (* Runde 2 *)
jump_stmt   = "break" | "continue" ;                       (* Runde 2 *)
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
primary     = int_lit | bool_lit | qualified | "(" expr ")" | struct_lit
            | array_lit | "syscall" "(" args ")" ;
qualified   = ident [ "." ident ]                  (* modul.name, Runde 2 *) ;
struct_lit  = ident "{" { ident ":" expr "," } "}" ;
array_lit   = "[" [ expr { "," expr } ] "]"
            | "[" expr ";" expr "]" ;              (* Wiederholung, Runde 2 *)
```

---

## 13. Zahlen, Layout, ABI

* Ganzzahltypen mit ausgeschriebener Breite: `i8…i64`, `u8…u64`, `u128`/`i128`
  (§9.3), `usize`, `isize`. **Kein** `int`. Literale sind typlos bis zur
  Verwendung und müssen eindeutig ableitbar sein.
* **Überlaufsemantik ausdrücklich (`L9`):** `+` prüft in `--debug` und läuft in
  `--release-fast` um (definiert, **nicht** undefiniert — bewusst anders als C).
  Zusätzlich explizite Operatoren: `+%` umlaufend, `+|` sättigend. HTML- und
  CSS-Parsing haben Grenzwertfälle, die die Spezifikation exakt vorschreibt;
  dafür braucht man beides ohne Umweg.
* **Gleitkomma (`L10`):** `f32`/`f64` nach IEEE-754 mit exakter Semantik,
  einschließlich NaN-Behandlung. JS-Zahlen *sind* Doubles, `calc()` rechnet in
  Doubles; Abweichungen zeigen sich als falsches Layout. Keine
  „schnelle Mathematik", keine Umsortierung von Gleitkommaoperationen durch den
  Optimierer — die ist bei IEEE-754 nicht wertneutral.
* Umwandlungen ausschließlich mit `as`, auch verlustfreie.
* **Struct-Layout (`P7`):** Deklarationsreihenfolge mit natürlicher Ausrichtung,
  **kein** Umsortieren. `#[packed]`, `#[align(n)]`.
* **Aufrufkonvention:** System V AMD64 (`rdi, rsi, rdx, rcx, r8, r9`, Rückgabe in
  `rax`, 16-Byte-Ausrichtung, `rbx, rbp, r12–r15` erhalten). `extern "C"` ist
  dasselbe. Zusätzlich (`L13`): eine dokumentierte, stabile **Firn↔Firn-ABI**
  über Komponentengrenzen, weil der Browser aus getrennten
  Karstos-Komponenten besteht, die miteinander reden.
* `syscall(nr, a1, …, a6)` ist eingebaut und bildet direkt auf `syscall` ab
  (`rax, rdi, rsi, rdx, r10, r8, r9`).

### 13.1 Ergebnisort — eine Garantie, keine Optimierung

Bei `let x: T = ausdruck`, `return ausdruck` und Feldzuweisung kennt der
erzeugende Ausdruck die **Zieladresse** und schreibt direkt dorthin. Es entsteht
**kein** Zwischenwert auf dem Stapel, der anschließend kopiert wird.

Das gilt als **Sprachgarantie**, nicht als Optimiererleistung — sie hält also
auch in `--dev` und `--dev-fast`. Der Grund ist praktisch: `let b: [u8; 8<<20] =
…` darf nicht erst 8 MB Stapel belegen. Genau daran scheitert Rust
(`Box::new([0u8; 8*1024*1024])` läuft über den Stapel), weshalb Rust-for-Linux
die Bibliothek `pin-init` nachbauen musste.

**Stand der Umsetzung (14.08.2026):** Für **Aggregatrückgaben** ist das bereits
so — `abi::ret_needs_sret()` klassifiziert Rückgaben über 16 Byte als `MEMORY`
mit verstecktem Zeiger in `rdi`, und das Lowering reicht die Zieladresse durch
(`compiler/src/lower.rs:604`). **Noch offen:** dieselbe Garantie für Struct- und
Arrayliterale sowie für den geplanten `init`-Ausdruck. Ausführlich in
`DESIGNZIELE.md` §6.

---

## 14. Was `firnc0` (Stufe 0) implementiert — verbindlich

Vertrag zwischen Spezifikation und Code. Alles hier läuft wirklich; alles andere
in diesem Dokument ist Zukunft und wird im README als „noch nicht" geführt.

**Enthalten:**
* Lexer und handgeschriebener rekursiv absteigender Parser, Fehlermeldungen mit
  Datei, Zeile, Spalte, Quelltextzeile und Markierung; mehrere Fehler pro Lauf.
* Typprüfer: `i8…i64`, `u8…u64`, `usize`, `isize`, `bool`, `*T`/`*mut T`,
  Structs mit Feldzugriff, Arrays fester Größe mit Index, Funktionen. Keine
  implizite Umwandlung.
* Ausdrücke: `+ - * / %`, `& | ^ << >>`, Vergleiche, `&& ||` (kurzschließend),
  unäres `-`, `!`, `&`, `*`.
* Anweisungen: `let`, `var`, Zuweisung an Variable/Feld/Index/Dereferenzierung,
  `if`/`else`, `while`, `return`, Blöcke. Funktionen mit Parametern, Rekursion.
* `syscall(...)` mit bis zu 6 Argumenten.
* **FIR** in Basisblöcken, dokumentiert, mit Textausgabe (`--emit=fir`),
  Konstantenfaltung und Entfernen toten Codes, mit Vorher/Nachher-Test.
* **x86_64-Codegen ohne LLVM**: Assemblerausgabe für `as`/`ld`, System-V-ABI,
  eigene Registerbelegung — *ehrlich benannt:* das ist **keine**
  Registerzuteilung im Sinne von Lebendigkeitsanalyse/Graphfärbung. Stufe 0 gibt
  jedem FIR-Wert einen eigenen Stack-Slot und rechnet in `rax`/`rcx`/`rdx`
  (reines Spilling). Korrekt, aber langsam. Echte Registerzuteilung ist `P3`.
* Testsuite mit ≥ 40 `.fi`-Programmen plus Negativtests.

**Nicht enthalten (Stufe 0), Stand nach Runde 3:** `interface`,
`drop`, Move-Prüfer, Referenztypen `&T`/`inout T` als geprüfte Typen
(nur Rohzeiger), Arenen, Abwicklung/`throw`, `f32`, `u128`,
`Arc[T]`, Standardbibliothek, Nebenläufigkeit, Paketverwaltung, aarch64, WASM,
LLVM-Backend. Nicht umgesetzte Typkonstruktoren melden einen eigenen Fehler mit
Zeile/Spalte statt eines Syntaxfehlers (`Rc[T]`, `Weak[T]`, `Arc[T]`; Nachweis:
`tests/neg/int_gc_nicht_umgesetzt.fi`).

**Runde 3 hat aus dieser Liste gestrichen:** Fehlerunionen `E!T` mit `try`/
`catch` (§14.1.fehler), `secret`/Constant-Time-Primitive (§9, `compiler/src/ct.rs`)
sowie den **gesamten Opt-in-Tracing-GC**: `gc class` mit Einfachvererbung,
`Gc[T]`, `GcWeak[T]`, fehlbare Allokation `AllocError!Gc[T]`, `#[no_gc]` und
`Rc`/`Weak` als reines Firn-Modul (§14.1.gc).

**Runde 2 hat aus dieser Liste gestrichen** (jeweils einzeln in §14.1 belegt):
Module/Imports und `export` (Punkt 15), Generics per Monomorphisierung
(§14.1.types), `enum`/`match` mit Vollständigkeitsprüfung (§14.1.types),
die Zeichenkettentypen `Bytes`/`Str`/`Str16`/`Atom` als Bibliothek in Firn
(§14.1.str), `.debug_line` für `gdb` (Punkt 16) sowie Optimierung weit über
Konstantenfaltung/DCE hinaus: mem2reg, CSE, Inlining, Blockverschmelzung und
**echte Registerzuteilung** (§14.1.opt). Der oben unter „x86_64-Codegen"
genannte Satz „reines Spilling, keine Registerzuteilung" gilt seit Runde 2
nur noch für `--no-opt`.

### 14.2 Attribute

Die Spezifikation stützt sich an vielen Stellen auf Attribute (`#[must_consume]`,
`#[no_gc]`, `#[constant_time]`, `#[unwinds]`, `#[packed]`, `#[align(n)]`,
`#[layout(soa)]`, `#[no_move]`, `#[abi_stable]`, `#[frozen]`, `#[hot]`). Sie
entstehen zu sehr verschiedenen Zeitpunkten. Damit daraus kein Wildwuchs wird,
gilt:

* **Ein Register.** `compiler/src/attrs.rs` ist die einzige Wahrheit darüber,
  welche Attribute es gibt, wohin sie gehören, wie viele Argumente sie nehmen
  und ob Stufe 0 sie umsetzt. `firnc --list-attrs` gibt das Register aus.
* **Nie stillschweigend ignorieren.** Ein bekanntes, aber nicht umgesetztes
  Attribut ist ein **Übersetzungsfehler** mit Zeile, Spalte und Hinweis auf den
  geplanten Zweck. Ein übergangenes `#[constant_time]` wäre der gefährlichste
  Fehler, den diese Sprache haben kann (§9.2).
* **Unbekannte Attribute** liefern einen Vorschlag, wenn es ein Tippfehler ist.
* **Falsches Ziel** (z. B. `#[packed]` vor einer Funktion) ist ein Fehler.

In Stufe 0 umgesetzt ist genau eines: **`#[must_consume]`**, vor `fn` und vor
`struct`. Geprüft wird die Teilmenge, die ohne Move-Prüfer entscheidbar ist —
*das Ergebnis eines Aufrufs darf nicht als Anweisung verworfen werden*. Die
volle Form aus §3.3 (*der Wert muss an eine verbrauchende Funktion übergeben
werden*) kommt mit dem Move-Prüfer in ROADMAP Phase 2. Diese Einschränkung ist
im Compiler dokumentiert und hier ausdrücklich benannt, damit `#[must_consume]`
nicht mehr verspricht, als es hält.

### 14.1 Nachtrag: bewusste Abweichungen der Stufe-0-Umsetzung (`firnc0`)

Hält fest, wo die Umsetzung enger ist als der Text oben — damit Spezifikation und
Code nicht auseinanderlaufen.

1. ~~**Aggregate an Funktionsgrenzen.**~~ **Gestrichen in Runde 2** (Modul
   `kern`): Structs und Arrays sind als Parameter und als Rückgabewert erlaubt.
   Die System-V-Klassifikation steht in `compiler/src/abi.rs`
   (`ArgClass::{Integer, Memory}`, `classify`) und ist die einzige Wahrheit
   über die Aufrufkonvention. Die Klasse `Sse` fehlt dort bewusst, solange es
   keine Gleitkommatypen gibt (siehe Punkt 20). Nachweis: `tests/100_agg_param_8.fi` bis
   `tests/105_agg_wertsemantik.fi`.
   **Zwei bewusste Abweichungen von System V bleiben** und sind hier
   festgehalten (siehe auch Punkt 15):
   * Aggregate der MEMORY-Klasse (> 16 Byte) werden als *versteckter Zeiger auf
     eine Kopie des Aufrufers* übergeben statt als Stapelkopie.
   * Rückgaben von Aggregaten laufen bereits **ab 9 Byte** über den versteckten
     Zeiger in `rdi` (System V nutzt für 9–16 Byte `rax:rdx`); bis 8 Byte
     liefert `rax` das Wort.
   Beides ist für sich geschlossen (Aufrufer und Aufgerufener folgen derselben
   Regel), aber **nicht** binärkompatibel zu C für diese Fälle.
2. **Typlose Literale.** Es gibt **keinen** Vorgabetyp. `let x = 5` ist ein
   Fehler, `let x: i32 = 5` und `let x = 5 as i32` sind richtig. Kontext liefern:
   Typannotation, Zieltyp einer Zuweisung, Parametertyp, Rückgabetyp, `as`, der
   andere (typisierte) Operand eines Binäroperators, die Indexposition (`usize`).
3. **Keine Laufzeitprüfungen.** Überlauf, Division durch null und
   Bereichsüberschreitung sind in Stufe 0 nicht geprüft (§13 beschreibt den
   Zielzustand). Verhalten entspricht `--release-fast`.
4. **`const`** ist auf skalare, zur Übersetzungszeit auswertbare
   Ganzzahl-/`bool`-Ausdrücke beschränkt.
5. **Globale Variablen** gibt es nicht (nur `const`).
6. **`profile`-Deklaration** wird geparst und geprüft, hat aber keine Wirkung:
   erzeugt wird immer ein freistehendes Binary mit `_start` ohne libc.
7. **`extern fn`** wird syntaktisch erkannt, aber mit klarem Fehler abgelehnt.
8. **Rückgabewert des Programms.** `fn main() -> i32`; `_start` ruft `main` und
   übergibt das Ergebnis an `exit` (Exit-Code = Wert & 0xFF).
9. ~~**Höchstens 6 Funktionsparameter.**~~ **Gestrichen in Runde 2** (Modul
   `kern`): Argumente ab dem siebten INTEGER-Wort werden vor dem `call` bei
   `[rsp+8k]` abgelegt, die 16-Byte-Ausrichtung bleibt erhalten; der
   Aufgerufene liest sie bei `[rbp+16+8k]`. Nachweis:
   `tests/108_stapelargumente.fi` und der Codegen-Test
   `stapelargumente_ab_dem_siebten_wort`.
10. **Parameter sind unveränderlich** (wie `let`-Bindungen).
11. ~~**Kein Wiederholungsliteral `[wert; N]`.**~~ **Gestrichen in Runde 2**
    (Modul `kern`): `[wert; N]` gibt es, `N` ist ein konstanter Ausdruck. Der
    Wert wird genau einmal ausgewertet; bis 8 Elemente entrollt das Lowering,
    darüber entsteht eine Schleife. Nachweis:
    `tests/109_wiederholungsliteral.fi`.
12. **`as` bindet stärker als die unären Operatoren.** `&s.a as u64` bedeutet
    `&(s.a as u64)`; gemeint ist `(&s.a) as u64`.
13. ~~**Kein `break`/`continue`.**~~ **Gestrichen in Runde 2** (Modul `kern`):
    `break`, `continue` und `for i in a..b` gibt es; die Entzuckerung findet
    ausschließlich im Lowering statt (`continue` springt in einer `for`-Schleife
    auf den Fortschaltblock, nicht auf den Kopf). Außerhalb einer Schleife ist
    `break`/`continue` ein Fehler mit Zeile/Spalte. Nachweis:
    `tests/106_for_schleife.fi`, `tests/107_break_continue.fi`,
    `tests/neg/kern_break_ausserhalb.fi`.
14. **Assembler-Ausgabe** ist Intel-Syntax mit `.intel_syntax noprefix`. `as` und
    `ld` werden ausschließlich als Assembler bzw. Linker aufgerufen, nie ein
    C-Compiler.
15. **Modulsystem (Runde 2, Modul `kern`).** `import pfad.modul`,
    `export { a, b }` und `modul.name` gibt es; mehrere `.fi`-Dateien werden zu
    **einem** Binary übersetzt. Umgesetzt ist *Gesamtprogramm-Übersetzung mit
    getrennten Namensräumen* (Namen der Nicht-Wurzelmodule heißen intern
    `modul__name`), **nicht** getrennte Objektdateien mit Schnittstellendateien.
    Es gibt keine Paketverwaltung (`W1`, ABNAHME Punkt 5 bleibt offen).
16. **Zeilennummern für den Debugger (Runde 2, Modul `kern`).** Der Compiler
    schreibt `.file`/`.loc`-Direktiven; `as` erzeugt daraus `.debug_line`.
    Anweisungsgenaue Zeilen gibt es **nur ohne Optimierer** (`--no-opt`), weil
    die FIR keine Quellpositionen trägt und der Optimierer Instruktionen
    entfernt und Blöcke neu nummeriert. Mit Optimierer bleibt die Zeile der
    `fn`-Deklaration. Variablen zeigt `gdb` noch nicht (kein `.debug_info` für
    lokale Namen).

17. **Aufzählungsnamen sind programmweit, nicht je Modul (Runde 3).** `enum`
    wird in der Registrierung von `sema_match` unter seinem nackten Namen
    geführt. Ein `enum` in einem importierten Modul ist deshalb als
    `Ampel::Rot` anzusprechen, **nicht** als `zustand.Ampel::Rot`; zwei Module
    dürfen keine gleichnamige Aufzählung deklarieren. Nachweis:
    `tests/231_modul_match.fi`. (Runde 3 hat an derselben Stelle einen echten
    Fehler behoben: die Rumpfblöcke der `match`-Fälle liegen in der
    Registrierung und wurden vom Modulsystem nicht umgeschrieben — `match` in
    einem importierten Modul war unbenutzbar. `compiler/src/modules.rs`
    besucht sie jetzt über `sema_match::take_match`/`put_match`.)

18. **Fehlermeldungen in importierten Modulen (Runde 3).** Zeile und Spalte
    stimmen, der angezeigte **Dateiname** ist jedoch der der Wurzeldatei. Die
    Quelltextkarte führt zwar Dateinummern, die Diagnose wählt daraus aber noch
    nicht die richtige Datei aus. Offen.

19. **Constant-Time-Primitive: nur die drei Bausteine, kein `secret[T]`
    (Runde 4).** Aus §9 sind umgesetzt: `select(bedingung, a, b)` (wird `cmov`,
    nie ein Sprung), `barrier(x)` (undurchsichtige Sperre) und
    `secure_zero(zeiger, anzahl_bytes)` (überlebt jeden Durchgang,
    `rep stosb`). Sie stehen in `compiler/src/ct.rs` und gelten auf skalaren
    Typen (Ganzzahl, `bool`, Zeiger). **Nicht** umgesetzt sind der
    Typqualifizierer `secret[T]`, die Ausbreitung der Markierung durch
    Ausdrücke, `declassify` und damit auch die Wirkung von `#[constant_time]`:
    das Attribut bleibt in `attrs.rs` als *nicht umgesetzt* geführt und meldet
    einen sauberen Fehler (`tests/neg/attr_nicht_umgesetzt.fi`). Die Prüfung im
    Codegenerator (bedingter Sprung auf einem `secret`-Wert bricht ab) ist
    vorhanden, bekommt aber erst mit `secret[T]` Futter. Abweichung in der
    Schreibweise: §9 schreibt `barrier(inout x)` und `secure_zero(inout buf)`;
    Stufe 0 kennt kein `inout`, deshalb nimmt `barrier` den Wert und liefert
    ihn zurück, und `secure_zero` nimmt Zeiger und Byteanzahl. Eine eigene
    Funktion gleichen Namens verdeckt das Primitiv. Nachweis:
    `tests/430_ct_select.fi` bis `tests/433_ct_secure_zero.fi`, fünf
    Negativtests `tests/neg/ct_*.fi` und vier Codegen-Nachweise in
    `compiler/src/ct.rs`.

20. **Keine Gleitkommatypen (Runde 4 bestätigt).** `f32`/`f64` gibt es nicht;
    deshalb führt `abi.rs` auch keine SSE-Klasse. Sie kommt zusammen mit den
    Typen — dann erzwingt die Vollständigkeitsprüfung des Compilers, dass jede
    Fallunterscheidung sie behandelt. Eine Variante ohne Erzeuger wäre toter
    Code, der nur mit einem Unterdrückungsattribut warnungsfrei bliebe.

#### 14.1.types — Summentypen, Musterabgleich, Generics (Runde 2, Modul `types`)

Mit Runde 2 setzt `firnc0` §6.3 (`L4`) und Generics (`L5`) um: `enum` mit
Nutzdaten, `match` mit **Vollständigkeitsprüfung zur Übersetzungszeit**
(fehlender Fall = Fehler mit Zeile/Spalte und Nennung der Variante),
Sprungtabellen im Codegenerator und Monomorphisierung. Bewusst enger als der
Text oben ist dabei Folgendes:

T1. **`match` ist eine Anweisung, kein Ausdruck.** `let x = match e { .. }`
    wird nicht unterstützt; jeder Fall hat einen Block als Rumpf. Grund: der
    Ergebniswert eines Musterabgleichs verlangt eine Zusammenführung von
    Werten (φ) im Lowering, die Stufe 0 nicht kennt. Zuweisung im Rumpf ist
    der Ersatz.
T2. **Eine Aufzählung darf nicht dem Wert nach in einem `struct` liegen.**
    `struct S { a: E }` meldet einen Fehler mit Hinweis auf `*mut E`. Grund:
    das Struct-Layout steht fest, bevor die Aufzählungen ausgelegt werden.
    Aufzählung in Aufzählung ist dagegen erlaubt (verschachtelte Muster).
T3. **Keine generischen Aufzählungen** (`enum Option[T]`). Generisch sind nur
    Funktionen und Structs.
T4. **Kein `match` im Rumpf einer generischen Vorlage.** Die Fallrümpfe liegen
    außerhalb des AST und würden je Ausprägung nicht ersetzt; der Compiler
    meldet das als Fehler statt still falschen Code zu erzeugen.
T5. **Keine Alternativmuster** (`A | B`) und keine Wächter (`if`) im Muster.
T6. **Aufzählungen und generische Vorlagen sind dateilokal.** `modul.E::V`
    wird nicht aufgelöst; eine Aufzählung wird in der Datei benutzt, in der
    sie steht (mehrere Dateien in einer Übersetzung sind erlaubt, solange die
    Aufzählung nicht über die Dateigrenze angesprochen wird).
T7. **Anforderungen an Typparameter** sind auf `Any`, `Int` und `Scalar`
    beschränkt (kein Schnittstellensystem, §6.2 bleibt offen).
T8. **Bereichsmuster** gelten nur für Ganzzahlen und sind auf ganzzahlige
    Grenzen beschränkt (`1..4`, `4..=9`, `-5..=-1`).

Speicherlayout einer Aufzählung (verbindlich, Grundlage für `tok` und die
Fehlersuche): `__tag: u32` bei Offset 0, danach die Nutzdaten ab
`round_up(4, payload_align)`; die Nutzdatenbereiche verschiedener Varianten
**überlagern** sich, Größe und Ausrichtung ergeben sich aus der größten
Variante (mindestens 4). Namensschema der Monomorphisierung: `name__T1_T2`.

#### 14.1.gc — Opt-in-Tracing-GC, `gc class`, DOM-Prototyp (Runde 3)

Umgesetzt und mit laufendem Code belegt:

* **`gc class Name [extends Basis] { … }`** mit Präfixlayout — die geerbten
  Felder liegen vorn, deshalb ist die Aufwärtsumwandlung `Gc[Element]` →
  `Gc[Node]` kostenlos. Abwärts nur geprüft: `x.as?[Element]` liefert den
  Nullwert, wenn der Typ nicht passt.
* **`Gc[T]`** als erstklassiger starker Zeiger (Feldzugriff ohne
  `(*p).feld`), **`GcWeak[T]`** als schwacher Verweis mit `weak(g)`/`stark(w)`,
  Nullwerte `gc_null[T]()`/`weak_null[T]()`.
* **Allokation ist fehlbar**: `gc C{…}` hat den Typ `AllocError!Gc[C]`
  (DESIGNZIELE §2). Bei erschöpftem Heap wird **erst gesammelt, dann
  gescheitert** — nachgewiesen in `tests/535_gc_fehlbare_allokation.fi` mit
  einer Obergrenze von 256 KiB.
* **Mark-Sweep**, anhaltend, Sammlung nur an Allokationsstellen und bei
  `gc_collect()`. **Präzise** Heap-Verfolgung über eine compilergenerierte
  Typtabelle (Feldoffsets je Klasse, getrennt nach stark und schwach),
  **konservativer** Scan von Stapel **und** Registern. Kein Kompaktieren,
  Größenklassen-Allokator, `mmap` ohne feste Adresse.
* **Einfügebarriere** beim Schreiben eines `Gc`-Zeigers in ein Feld
  (`gc_barriers()` zählt sie mit).
* **`#[no_gc]`** transitiv geprüft: verboten sind GC-Allokation, Aufruf einer
  nicht markierten Funktion und das Schreiben in ein `Gc`/`GcWeak`-Feld. Der
  HTML5-Tokenizer in `lib/html/` ist so markiert.
* **Messwerte zur Laufzeit**: `gc_collections`, `gc_live_objects`,
  `gc_live_bytes`, `gc_heap_bytes`, `gc_pause_ns_last/max/total`,
  `gc_barriers`, `gc_set_max_bytes`.
* **`Rc[T]`/`Weak[T]`** als reines Firn-Modul (`tests/modules/rc.fi`), immer
  unveränderlich, fehlbare Allokation, `#[must_consume]`. Zyklen lecken dort
  **absichtlich** und werden sichtbar gemacht (`tests/552_rc_zyklus_leck.fi`),
  statt sie wegzuerklären.

**Belegt am DOM** (`lib/dom/dom.fi`, `tests/560_dom_zyklen.fi`,
`tools/dom_soak/run.sh`): sechs Zyklenarten — Eltern↔Kind beide stark,
`Element extends Node` mit Attributen, Knoten↔Listener, Sammlung→Wurzel,
Observer über `GcWeak`, Knoten↔JS-Wrapper. Dauerlauf **100.000.000
Zyklensätze = 700.000.000 Objekte bei konstant 1.364 KiB RSS**; die
Zählverweis-Gegenprobe mit identischem Graphen braucht nach 2.000.000 Zyklen
**750.080 KiB**. Bericht: `docs/berichte/dom.md`.

**Ehrliche Grenzen dieser Umsetzung:**

* **Keine Finalisierer**, **kein inkrementelles Sammeln**, keine `GcVec`/`GcMap`,
  kein `virtual`. Die längste gemessene Pause ist **3,54 ms** — für einen
  Browser mit 16-ms-Bildabstand bereits zu viel.
* **Ein Faden.** Der Zustandsblock ist fadenlokal gedacht; Stufe 0 hat nur einen.
* **Der konservative Scan hat einen Preis, der sich messen lässt:** eine alte
  Zeigerkopie in einem **lebenden** Stapelrahmen hält ihr Objekt am Leben. Wer
  einen Sammellauf im selben Rumpf prüft, in dem er das Objekt erzeugt hat,
  misst deshalb nicht, was er zu messen glaubt. Die Laufzeit überschreibt den
  toten Stapelbereich unter dem eigenen Rahmen (`__gc_scrub_tief`); für den
  lebenden Rahmen gibt es keine Abhilfe außer präzisen Stapelkarten.
* **`Gc[modul.Klasse]` ist nicht schreibbar** — ein `gc class` aus einem anderen
  Modul kann in einer Typangabe nicht benannt werden. Wurzelprogramme reichen
  deshalb nur Zahlen über die Modulgrenze (siehe `lib/dom/soak_gc.fi`).
* **Fragmentierung** bei wechselnden Objektgrößen ist ungeprüft; der Dauerlauf
  benutzt immer denselben Satz.

#### 14.1.module — zwei Grenzen des Modulsystems behoben (Runde 17)

**Importpfade werden zuerst relativ zur importierenden Datei gesucht**, erst
danach relativ zur Wurzeldatei. Vorher galt nur die zweite Regel, und damit
konnte eine Bibliothek keine andere einbinden. Der Rückfall auf die Wurzel
bleibt, damit bestehender Quelltext unverändert übersetzt.

**Generische Vorlagen aus Modulen sind benutzbar.** Die Vorabsuche nach
Vorlagen lief je Datei unmittelbar vor deren Parsen; die Wurzeldatei wird
zuerst geparst und kannte die Vorlagen der Module deshalb nicht.
`modules::build_program` lext jetzt erst alle Dateien und scannt sie vorab.

Nachweis für beides: `tests/630_modulkette.fi`.

**Offen (`docs/SELBSTHOSTING.md` §7, B2):** eine generische Vorlage sieht nur
die Namen der **Wurzeldatei**, nicht die ihrer eigenen Moduldatei — selbst eine
Hilfsfunktion zwölf Zeilen darüber meldet *unbekannte funktion*. Solange das
gilt, kann ein Modul keine generische Sammlung anbieten, die intern arbeitet.

#### 14.1.sizeof — `size_of[T]()` (Runde 16)

`size_of[T]()` liefert die Größe eines Typs in **Bytes**, ermittelt zur
Übersetzungszeit. Zur Laufzeit bleibt davon nichts übrig: der Typprüfer rechnet
die Größe aus dem Layout aus (`TypeCtx::size_of`), das Lowering setzt eine
Konstante ein.

```firn
let n: usize = size_of[i32]()       // 4
let m: usize = size_of[Punkt]()     // gerechnetes Struct-Layout, nicht die Feldsumme
var feld: [u8; 16] = [0 as u8; 16]  // taugt als Array-Länge
```

Gebaut wie `gc_null[C]()`: der Parser erkennt die Form und verpackt sie als
Aufruf mit einem reservierten Namen (`size_of$…`), in dem der Typname steckt.
`size_of` ist damit **kein Schlüsselwort** und kollidiert mit keinem
Bezeichner.

**Innerhalb generischer Vorlagen** wird der Typparameter im Namen mit ersetzt
(`mono::subst_call_name`) — ohne das meldet der Typprüfer *unbekannter typ 'T'*,
sobald die Vorlage ausgeprägt wird.

**Grenzen:** nur ein **Typname** als Argument, kein zusammengesetzter
Typausdruck (`size_of[*mut u8]` geht nicht — wer das braucht, gibt dem Typ
einen Namen). `size_of[void]` ist ein Fehler.

**Wozu:** ohne die Elementgröße lässt sich die Adresse des `i`-ten Elements
nicht ausrechnen, und damit gibt es kein wachsendes `Vec[T]`
(`docs/SELBSTHOSTING.md` §4, Punkt 2).

#### 14.1.comptime — Auswertung zur Übersetzungszeit (Runde 12)

`compiler/src/comptime.rs` führt **eigene Funktionen zur Übersetzungszeit
aus** — mit Schleifen, Verzweigungen, lokalen Variablen und Rekursion. Der
Einstieg ist jede Stelle, an der ein konstanter Ausdruck erwartet wird:

```firn
fn fakultaet(n: i64) -> i64 { … }

const FAK10: i64 = fakultaet(10)      // 3628800, zur Übersetzungszeit
var feld: [u8; 120] = …               // Ergebnisse taugen als Array-Länge
```

**Umfang:** Ganzzahlen und `bool`; `let`/`var`, Zuweisung an lokale Variablen,
`if`/`else`, `while`, `for`, `break`, `continue`, `return`, Blöcke; alle
Operatoren mit Kurzschluss bei `&&`/`||`, Umwandlungen mit korrektem
Zurechtschneiden, Aufrufe (auch rekursiv).

**Grenzen, die eingehalten werden:** höchstens 2.000.000 ausgeführte
Anweisungen und 64 verschachtelte Aufrufe. Beides endet mit einer Meldung samt
Quellposition — ein `comptime` darf den Compiler nicht aufhängen
(`tests/neg/comptime_endlos.fi`).

**Nicht möglich:** Zeiger, Arrays, Structs, `syscall`, Gleitkomma,
GC-Allokation. Alles davon braucht einen Speicher zur Übersetzungszeit; der
kommt mit `emit`. Ein Versuch wird gemeldet, nicht still falsch übersetzt
(`tests/neg/comptime_zeiger.fi`).

**`emit` gibt es seit Runde 13.** Ein `comptime { … }`-Block auf oberster Ebene
baut mit `emit_roh("…")` und `emit_zahl(x)` **Firn-Quelltext** auf, der im
selben Lauf gelext, geparst und ans Programm angehängt wird — danach sieht der
Typprüfer keinen Unterschied zu von Hand geschriebenem Code.

```firn
comptime {
    emit_roh("fn tab_gross(c: i64) -> i64 {\n")
    for c in 97..123 {
        emit_roh("    if c == ")
        emit_zahl(c)
        emit_roh(" { return ")
        emit_zahl(gross(c))
        emit_roh(" }\n")
    }
    emit_roh("    return c\n}\n")
}
```

`firnc --emit=comptime` gibt den erzeugten Quelltext aus, statt weiterzubauen —
so lässt sich prüfen, was der Compiler wirklich vor sich hat.

**Wie `emit_roh` ohne Zeichenketten im Interpreter auskommt:** der Parser hat
`"abc"` bereits in ein Array-Literal aus Oktetten verwandelt (§14.1.str); der
Interpreter liest es zurück. Damit braucht `comptime` keine
Zeichenkettenunterstützung, um Text zu erzeugen.

**Reihenfolge im Übersetzungslauf:** die Blöcke laufen **vor** der Typprüfung,
direkt nach dem Zusammenführen der Module. Sie dürfen deshalb **keine
programmweiten Konstanten** benutzen, wohl aber jede Funktion des Programms
aufrufen. Der erzeugte Text bekommt eine eigene Dateinummer (`<comptime>`) in
Diagnosen *und* in der Zeilentabelle — fehlt Letzteres, erzeugt der
Codegenerator `.loc`-Direktiven mit einer Nummer, die `as` nicht kennt.

**Datenzugriff zur Übersetzungszeit (Runde 14).** `datei_groesse("pfad")` und
`datei_byte("pfad", i)` lesen eine Datendatei, während der Compiler läuft.
Byteweise — damit braucht der Interpreter weder Zeichenketten noch Arrays.
`tests/602_comptime_ucd.fi` liest eine Datei im Format von `UnicodeData.txt`
(semikolongetrennte Felder, Codepunkt in Feld 0, Großschreibung in Feld 12) und
erzeugt daraus die Nachschlagefunktion `ucd_gross`.

**SICHERHEIT — und zwar von Anfang an.** Dateizugriff zur Übersetzungszeit ist
ein Einfallstor für Lieferketten-Angriffe: eine eingebundene Bibliothek könnte
sonst beim Bauen `/etc/passwd` lesen und den Inhalt in den erzeugten Code
schreiben. Deshalb gilt:

* nur **relativ zur Wurzelquelldatei**,
* **kein `..`** an irgendeiner Stelle des Pfades,
* **kein absoluter Pfad**.

Beides wird abgewiesen, mit Meldung und Quellposition
(`tests/neg/comptime_datei_absolut.fi`, `comptime_datei_eltern.fi`). Das ist
bewusst enger als nötig; wenn Firn das Fähigkeitenmodell aus `DESIGNZIELE.md`
§3 bekommt, wird daraus eine Erlaubnis, die ein Modul ausdrücklich anfordern
muss.

#### 14.1.f64 — Gleitkomma (Runde 11)

`f64` ist seit Runde 11 ein Sprachtyp: Literale (`1.5`, `1e3`, `1_000.25`,
`1.5e-1`), die Grundrechenarten `+ - * /`, alle sechs Vergleiche, das
Vorzeichen `-x` und die Umwandlungen `ganzzahl as f64` / `f64 as ganzzahl`
(abschneidend Richtung null, wie in C). Nachweis: `tests/590_f64.fi` mit 29
Prüfungen in allen drei Baustufen, darunter NaN, Unendlich und negative Null.

**IEEE-754 wird eingehalten, auch im unbequemen Fall.** `ucomisd` setzt bei NaN
`ZF=PF=CF=1` — der ungeordnete Fall sieht damit aus wie „kleiner oder gleich",
und `nan < 1.0` lieferte im ersten Versuch **wahr**. Richtig ist falsch. Gelöst
wird das nicht mit Nachrechnen am Paritätsflag, sondern durch **Vertauschen der
Operanden**: `a < b` wird als `b > a` mit `seta` erzeugt, und `seta`/`setae`
sind von sich aus ungeordnet-sicher. Nur `==` und `!=` brauchen zusätzlich
`setnp`/`setp`.

**Bewusst weggelassen, mit Begründung:**

* **Kein `f32`.** Deshalb sind Gleitkommaliterale NICHT typlos — `1.5` ist
  immer `f64`. Sobald `f32` dazukommt, wird daraus eine Ableitung aus dem
  Zusammenhang.
* **Kein `%`** (das wäre `fmod` und braucht eine Bibliotheksfunktion) und
  **keine Bitoperationen** auf `f64` — auf einem Bitmuster haben sie keine
  sinnvolle Bedeutung. Wer sie braucht, wandelt ausdrücklich in `u64` um.
  Negativtest: `tests/neg/f64_kein_modulo.fi`.
* **Keine implizite Umwandlung**, auch nicht zwischen `i64` und `f64`
  (`tests/neg/f64_keine_implizite_umwandlung.fi`).
* **Keine Konstantenfaltung.** Der Wert einer `Op::Const` mit `FTy::F64` ist
  ein **Bitmuster**; die Faltung in `opt.rs` rechnet ganzzahlig und würde aus
  `1.5 + 1.5` stillen Unsinn machen. Sie ist deshalb für jede Instruktion
  gesperrt, an der ein `f64` beteiligt ist. Rundungstreue Faltung kommt mit
  `comptime`.

**Zwei ehrliche Einschränkungen der Umsetzung:**

F1. **Keine Registerzuteilung für `f64`.** Der Linear Scan in `regalloc.rs`
    kennt nur die Ganzzahlregister; `f64` lebt in den SSE-Registern und
    bräuchte eine zweite Registerklasse mit eigenen Intervallen. Solange die
    fehlt, geht **jede Funktion, in der ein `f64` vorkommt, über den
    Grundpfad** in `codegen_x86.rs` — korrekt, aber ohne Registerzuteilung und
    damit deutlich langsamer. Gerechnet wird in `xmm0`/`xmm1`, gelesen und
    geschrieben über `rax`.

F2. **Eigenes ABI statt System-V.** Ein `f64` wird als Bitmuster in den
    GANZZAHL-Registern übergeben und zurückgegeben, nicht in `xmm0`–`xmm7`.
    Innerhalb von Firn ist das durchgängig und korrekt; für Aufrufe fremder
    Bibliotheken wäre es falsch. Firn ruft heute nichts Fremdes auf (kein
    libc), und die Angleichung gehört zu F1 — beides braucht dieselbe
    SSE-Registerklasse.

#### 14.1.str — Zeichenketten und Zahlen ↔ Text (Runde 2, Modul `str`)

Mit Runde 2 sind §8.1–§8.4 umgesetzt: `Bytes`/`Str`/`Str16`/`Atom` mit dem in
§8.1 festgelegten Layout, WTF-16 **ohne jede Prüfung**, WTF-8 als verlustfreie
Brücke, korrekt gerundetes `strtod` und kürzeste Ausgabe mit
Rückwandlungsgarantie. Bewusst enger als der Text oben ist Folgendes:

S1. **Zeichenkettenliterale sind seit Runde 8 im Quelltext benutzbar.**
    `compiler/src/strings.rs` entschlüsselt `"..."` (UTF-8, geprüft),
    `b"..."` (rohe Oktette) und `u"..."` (WTF-16) samt aller Maskierungen
    einschließlich `\uXXXX` und `\u{...}` **mit ungepaarten Surrogaten**;
    `firnc --strlit=<literal>` zeigt das Ergebnis. Der Lexer ruft das jetzt
    auf (`lexer::string_literal`, VOR der Bezeichnererkennung — sonst
    verschluckt `is_ident_start` das `b` bzw. `u` des Präfixes).

    **Ein Literal ist ein ARRAY-Literal**, kein eigener Typ: `"abc"` hat den
    Typ `[u8; 3]`, `u"abc"` den Typ `[u16; 3]`. Damit gelten alle Regeln für
    Arrays — insbesondere die Längenprüfung: `var a: [u8; 5] = "abc"` meldet
    *array-literal hat 3 elemente, erwartet werden 5*. Der Parser wandelt das
    Literal unmittelbar in `ExprKind::ArrayLit` um; Typprüfer, Lowering und
    Codegenerator sehen es nie.

S8. **Literale liegen im Rahmen, nicht in `.rodata`.** Aus S1 folgt: die Daten
    entstehen als Folge einzelner Speicherbefehle beim Betreten des Blocks.
    Für Meldungen und Pfade ist das gleichgültig, für eine 4-KiB-Tabelle wäre
    es das nicht. Eine echte `.rodata`-Sektion mit `Str`/`Str16` als
    Bibliothekstyp (Zeiger + Länge) kommt mit der Standardbibliothek;
    `LitValue::asm_data()` erzeugt die Assemblerdaten dafür bereits.
    Ebenfalls offen: `Str`/`Bytes`/`Str16` als **Typ** eines Literals — heute
    ist das Ergebnis ein Array, das man von Hand in einen dieser Typen füllt.
S2. **Kein Gleitkommatyp in der Sprache.** `strtod` liefert und `dtoa`
    verbraucht das **Bitmuster** eines `binary64` als `u64`. Die Rechnung ist
    ohnehin vollständig ganzzahlig (exakte Großzahlarithmetik); sobald `f64`
    existiert, kommt nur eine dünne Hülle darüber. Rundung und Sonderwerte
    (`±0`, `±Infinity`, `NaN`) entsprechen IEEE-754 bzw. ECMAScript.
S3. **`Bytes`/`Str`/`Str16`/`Atom` sind Bibliothekstypen** (`lib/str/*.fi`),
    keine eingebauten Typen. Das Layout aus §8.1 ist eingehalten; die Trennung
    erzwingt der Typprüfer, weil es verschiedene `struct`-Typen sind
    (Negativtests `tests/neg/str_bytes_ist_kein_text.fi`,
    `tests/neg/str16_ist_kein_bytes.fi`). `Str` ist `Bytes` mit geprüftem
    Inhalt (`bytes_is_str`), kein eigener Typ — die Umdeutung von `Bytes` zu
    `Str` ist damit noch nicht compilergeprüft.
S4. **Die API ist zeigerbasiert.** Weil §14.1 Punkt 1 (keine Aggregate an
    Funktionsgrenzen), Punkt 5 (keine globalen Variablen) und das fehlende
    `inout`/`&` gelten, heißt der Konstruktor `str16_init(s: *mut Str16)`
    statt `str16_new() -> Str16`, und `atom_intern` bekommt die Tabelle als
    ersten Parameter. Die Namen `str16_push`, `str16_len`, `str16_at`,
    `atom_intern` sind wie vereinbart unverändert.
S5. **Kein eigener `Wtf8`-Typ.** WTF-8 ist eine `Bytes`-Darstellung mit den
    Funktionen `str16_to_wtf8` / `wtf8_to_str16`.
S6. **Kein `Rope`** (§8.5 ist ein SOLL ohne Termin).
S7. **Atome bekommen ihre Nummern zur Laufzeit**, in der Reihenfolge des
    Internierens (§8.3 sieht feste Nummern zur Bauzeit vor). Wer feste kleine
    Nummern braucht, interniert seine Namen beim Start in fester Reihenfolge.
S8. **`strtod` erkennt keine Sonderformen**: kein `Infinity`, kein `NaN`, kein
    Hexadezimalgleitkomma, kein führender Leerraum. Solche Eingaben liefern
    „nichts verbraucht" (`consumed == 0`).
S9. **Über 780 signifikante Ziffern hinaus** wird ein Klebebit gesetzt statt
    weiterzurechnen. Das Ergebnis bleibt korrekt gerundet (abgeschnittene
    Ziffern können eine exakte Mitte nur verlassen, nie erzeugen).

#### 14.1.opt — Optimierer und Registerzuteilung (Runde 2, Modul `opt`)

O1. ~~**Keine Registerzuteilung, jeder Wert im Stack-Slot.**~~ **Gestrichen in
    Runde 2** (Modul `opt`, Anforderung `P3`): `compiler/src/regalloc.rs`
    enthaelt eine echte Zuteilung — Lebendigkeitsanalyse je Basisblock,
    daraus ein Intervall je Wert, danach **linear scan** mit aktiver Liste und
    gewichteter Auslagerung (Verwendungen x Schleifentiefe). Vergeben werden
    `rbx`, `r12`–`r15` (callee-saved, in Prolog/Epilog gesichert) sowie
    `r8`–`r11` fuer Intervalle, die keinen `call`/`syscall` einschliessen.
    Der Satz in §14 („jedem FIR-Wert einen eigenen Stack-Slot … reines
    Spilling") beschreibt ab Runde 2 nur noch den **Grundpfad**, der weiter
    existiert und benutzt wird, wenn `emit_func_ra` nicht zustaendig ist
    (siehe O2). Nachweis: `tests/opt/regalloc_loop.fi` — der Schleifenrumpf
    im erzeugten Assembler enthaelt **keinen einzigen** Stackzugriff
    (`bash test_opt.sh` prueft das).
O2. **Zwei Codegen-Pfade.** Der registerbewusste Pfad uebernimmt nur, was er
    vollstaendig beherrscht. Er gibt ab an den Grundpfad bei: mehr als sechs
    Parametern oder Argumenten (Stapelargumente, §14.1 Punkt 9), unbekannter
    Blocknummerierung und **eingeschalteten anweisungsgenauen Debugzeilen**
    (`--no-opt`, §14.1 Punkt 16). Damit gilt: `--no-opt` erzeugt Code des
    Grundpfades, ohne `--no-opt` Code des Registerpfades. Beide Pfade liefern
    fuer jedes Testprogramm dasselbe Ergebnis — genau das prueft `test.sh`.
O3. **Zellen im Register statt Phi-Knoten.** FIR hat keine Phi-Knoten (§8.1),
    deshalb kann `mem2reg` nur `alloca`s aufloesen, in die **genau einmal**
    geschrieben wird und deren `store` alle `load`s dominiert. Mehrfach
    geschriebene Zellen (Schleifenzaehler!) loest stattdessen der
    Registerzuteiler auf: eine nicht entkommende `alloca` bis 8 Byte mit
    einheitlicher Zugriffsbreite lebt ueber die ganze Funktion in einem
    Register. Das ist funktional gleichwertig, aber lokaler als echtes SSA.
O4. **Leistungsziel §10.3 (`P1`, ≤ 2x Rust) noch nicht erreicht.** Gemessen am
    13.08.2026 mit `bash bench/run.sh` (6 Mikrobenchmarks, je doppelt in Firn
    und in Rust `-O`, Median aus 7 Laeufen): **Median 2,75x**, Spanne 1,57x
    (Fibonacci) bis 4,95x (Matrixmultiplikation). Die Zahlen stehen in
    `bench/RESULTS.md`, `README.md` und `ABNAHME.md` — nicht geschoent. Der
    Abstand entsteht vor allem dort, wo LLVM vektorisiert (Sieb, Matmul);
    Firn erzeugt ausschliesslich skalaren Code (kein SIMD, `L16` offen).
O5. **Bereichspruefungen (`P5`).** Die Sprache erzeugt in Stufe 0 gar keine
    Bereichspruefungen (§14.1 Punkt 3). Der vorhandene Durchgang entfernt
    stattdessen **beweisbar wiederholte Bedingungen**: ein `brcond`, dessen
    Bedingung auf dem Weg dorthin schon entschieden wurde (Kette von
    Bloecken mit genau einem Vorgaenger), wird zum unbedingten Sprung.
    Nachweis: `tests/opt/redundant_check.fi`.
O6. **Inlining ueber Modulgrenzen** ergibt sich daraus, dass das Modulsystem
    (§14.1 Punkt 15) alle Dateien in EIN `fir::Module` uebersetzt; ein
    importierter Aufruf ist fuer den Durchgang nicht von einem lokalen zu
    unterscheiden. Rekursion (auch indirekt) wird nie eingebettet,
    `#[constant_time]`-Funktionen und Funktionen mit `secret`-Werten bleiben
    aussen vor.

#### 14.1.fehlerunionen — Fehlerunionen `E!T` (Runde 3, Modul `fehlerunionen`)

Mit Runde 3 ist §5.1 als Sprachmittel umgesetzt: `error`-Deklaration,
Typsyntax `E!T` (Rückgabetyp, Variablentyp, Feldtyp, Parametertyp), implizite
Umwandlung bei `return`, `try`, `catch` und `catch |e| ersatz`. Ein `!T`-Wert
ist implizit `#[must_consume]`.

**Darstellung (verbindlich).** Eine Fehlerunion ist ein Struct in
`types::TypeCtx` mit `__err: u32` bei Offset 0 (`0` = Erfolg, Fehlercodes ab
`1` in Deklarationsreihenfolge der Fehlermenge) und `__val: T` bei
`round_up(4, align(T))`; der reine Fehlerwert `E` ist der Struct `{ __err: u32 }`.
Damit gelten Aggregat-ABI (§14.1 Punkt 1), Registerzuteilung und Codegen
unverändert. Seitentabellen und Prüfung stehen in `compiler/src/errors.rs`,
das Lowering in `compiler/src/lower_errors.rs`.

Bewusst enger als der Text in §5.1 ist Folgendes:

F1. **Die Fehlermenge wird nicht abgeleitet.** `E!T` muss vollständig
    hingeschrieben werden; ein `!T` ohne Fehlermenge (§5.1 „die Fehlermenge
    darf weggelassen und vom Compiler abgeleitet werden") ist nicht umgesetzt
    und meldet einen Syntaxfehler mit Zeile und Spalte.
F2. **Fehlermengennamen sind programmweit**, nicht je Modul — wie
    Aufzählungsnamen (§14.1.types). `LeseFehler::Ende` gilt in jeder Datei,
    `modul.LeseFehler::Ende` gibt es nicht. Nachweis:
    `tests/414_modul_fehler.fi`.
F3. **`try` verlangt dieselbe Fehlermenge.** Es gibt keine Vereinigung oder
    Verbreiterung von Fehlermengen und kein Umschlüsseln beim Durchreichen;
    unterschiedliche Mengen sind ein Fehler mit Zeile und Spalte
    (`tests/neg/err_falsche_menge.fi`).
F4. **`catch |e| …` bindet an einen Ausdruck, nicht an einen Block.** Der
    Fehlerwert `e` hat den Typ der Fehlermenge und wird mit `==`/`!=`
    untersucht (`tests/419_catch_bindung.fi`); `match e { … }` auf einem
    Fehlerwert ist **nicht** umgesetzt und meldet einen sauberen Fehler.
    Auch der Schreibweise nach ist `catch` damit enger als das Beispiel in
    §5.1, das einen Block mit `return` darin zeigt.
F5. **`defer` gibt es seit Runde 9, `errdefer` noch nicht.**

    `defer <anweisung>` schiebt die Anweisung bis zum Verlassen des
    umschliessenden Blocks auf. Erlaubt ist ein Block (`defer { … }`) oder eine
    einzelne Anweisung (`defer close(fd)`). Ausgefuehrt wird in **umgekehrter
    Reihenfolge** der Vereinbarung, und zwar bei jedem Verlassen: am Blockende,
    bei `return`, bei `break` und bei `continue`.

    * **`return` raeumt alle Ebenen ab**, innerste zuerst — der Rueckgabewert
      ist zu diesem Zeitpunkt bereits berechnet (`lower::ret_term`).
    * **`break`/`continue` raeumen genau die Ebenen ab, die INNERHALB der
      Schleife vereinbart wurden** (`lower::loops` merkt sich dafuer die Tiefe
      des `defer`-Stapels beim Betreten der Schleife).
    * **Auswertungszeitpunkt: wie Zig, nicht wie Go.** Der Rumpf wird erst beim
      Verlassen ausgewertet, mit den Werten von *dann*. Go wertet die Argumente
      sofort aus und legt sie in versteckten Kopien ab; das widerspricht dem
      Grundsatz „nichts Verstecktes" (§2). Nachweis:
      `tests/580_defer.fi`, Abschnitt 3.
    * **Ein Sprung aus dem Rumpf heraus ist ein Fehler** (`return`, `break`,
      `continue`) — er wuerde die Reihenfolge der uebrigen aufgeschobenen
      Anweisungen zerreissen. Negativtests `tests/neg/defer_return.fi`,
      `tests/neg/defer_break.fi`. Innerhalb einer Schleife, die im `defer`
      selbst beginnt, sind `break`/`continue` erlaubt.

    **`errdefer` gibt es seit Runde 10.** Es laeuft nur, wenn die Funktion ueber
    einen Fehler verlassen wird. `defer` und `errdefer` teilen sich EINE Liste
    je Blockebene und laufen in gemeinsamer umgekehrter Reihenfolge — steht das
    `errdefer` hinter dem `defer`, laeuft es also zuerst.

    Als Fehlerpfad gilt:
    * die Weitergabe durch **`try`** (`lower_errors::return_error`),
    * ein **`return E::Variante`** — der Typpruefer meldet das als
      `CoerceKind::FromError`.

    Nicht als Fehlerpfad gilt ein gewoehnlicher Rueckgabewert, auch wenn die
    Funktion eine Fehlerunion liefert.

    **Ehrliche Grenze:** wird eine FERTIGE Fehlerunion weitergereicht
    (`let u: E!i32 = f()` … `return u`), steht erst zur Laufzeit fest, ob der
    Fehlerpfad genommen wird. Stufe 0 entscheidet das nicht und **lehnt den
    Fall ab**, statt `errdefer` still zu uebergehen — mit Hinweis auf
    `return try …`. Nachweis: `tests/neg/errdefer_union_weitergabe.fi`.
    Die Laufzeitunterscheidung (zwei Aufraeumpfade hinter einer Verzweigung auf
    den Fehlercode) ist moeglich und kommt, wenn sie gebraucht wird.
F6. **Kein Erfolgstyp `()`.** `E!()` ist nicht schreibbar (Stufe 0 kennt `()`
    nicht als Typsyntax); eine Funktion ohne Nutzergebnis liefert z. B.
    `E!i32`.
F7. **Wo implizit umgewandelt wird**, ist abschließend aufgezählt: `return`,
    `let x: E!T = …`, Zuweisung, Feld eines Struct-Literals und Argument eines
    Aufrufs. In Array-Literalen und in Vergleichen wird **nicht** umgewandelt.
F8. **Im Fehlerfall ist `__val` unbestimmt.** Definiert ist nur `__err`; wer
    den Erfolgswert im Fehlerfall liest (nur über eine eigene Struct-Sicht
    möglich), liest Füllwerte.
F9. **`catch` bindet schwächer als jeder Operator.** `a catch b * 2` ist
    `a catch (b * 2)`, `(a catch b) * 2` braucht Klammern. `try` bindet so
    stark wie ein unärer Operator: `try f() + 1` ist `(try f()) + 1`.
F10. **Fehlerunion über einem Struct-Erfolgstyp taugt nicht als Feldtyp eines
    Structs.** Wenn `sema::collect_structs` die Feldtypen auflöst, stehen die
    Struct-Layouts noch nicht fest — die Fehlerunion bekäme eine falsche Größe.
    Statt eines stillen Fehl-Layouts gibt es einen Fehler mit Zeile und Spalte
    (`tests/neg/err_union_in_struct.fi`). Mit skalarem Erfolgstyp
    (`E!i32`, `E!*mut u8`) ist der Feldtyp erlaubt (`tests/408_union_feld.fi`),
    als Rückgabe-, Variablen- und Parametertyp jeder Erfolgstyp.


---

## 15. Offene Fragen

1. **Präzises Stack-Scanning später nachrüsten?** Konservativ (§3.5.3) schließt
   einen kompaktierenden Sammler dauerhaft aus. Wenn Fragmentierung im
   Dauerlauf zum Problem wird, ist das die Stelle, an der nachgebessert werden
   muss — und es wird teuer. Entscheidung vertagt bis nach dem 24-h-Test.
2. ~~**`async`/Koroutinen**~~ — **entschieden am 14.08.2026**: `Io` als
   Parameter statt Sprachfarbe (§7, `DESIGNZIELE.md` §1). Offen bleibt nur die
   Größe der Koroutinen-Stapel und ob sie wachsen dürfen.
3. **Seile** (`Z3`, SOLL) — ab wann lohnt der Aufwand?
4. **SIMD** (`L16`, SOLL) — als eingebaute Vektortypen oder nur über
   Inline-Assembler?
5. **Bedingte Moves.** Konservativ ablehnen (jetzige Wahl) oder Laufzeit-Flags
   wie Rusts Drop-Flags?
6. **Paketverwaltung** (`W1`, MUSS) — Entwurf steht noch aus.
7. **Ausrichtung an Karstos.** Sobald `firnc` Kernelmodule übersetzt, muss die
   Aufrufkonvention gegen den bestehenden Rust-Code in karst geprüft werden.

---

## 16. Rückverfolgung: Anforderung → Abschnitt

Damit prüfbar ist, dass keine Anforderung aus `FIRN-ANFORDERUNGEN.md`
stillschweigend unter den Tisch gefallen ist. Der Umsetzungsstand steht in
`ABNAHME.md`, nicht hier.

| Anforderung | Abschnitt |
|---|---|
| `S1` deterministische Speicherverwaltung als Standard | §3.3 |
| `S2` Opt-in-GC-Heap mit Zyklenauflösung | §3.5 |
| `S3` schwache Verweise | §3.4 (`Weak`), §3.5.2 (`GcWeak`) |
| `S4` Finalisierer · `S5` inkrementell · `S6` Pausenzeiten | §3.5.3 |
| `S7` geteilte unveränderliche Werte mit Zählung | §3.4 (`Rc`/`Arc`) |
| `L1` Selbst-Hosting | §11 |
| `L2` Speichersicherheit als Standard · `L3` markierte Unsicherheit | §3.3, §3.6 |
| `L4` Summentypen + Vollständigkeitsprüfung | §6.3 |
| `L5` Generics mit Monomorphisierung | §6.1 |
| `L6` Schnittstellen statisch **und** dynamisch | §6.2, §4.4 |
| `L7` Ergebnistypen · `L8` Abwicklung für JS | §5.1, §5.3 |
| `L9` Ganzzahl-Semantik · `L10` IEEE-754 | §13 |
| `L11` `const`-Auswertung · `L12` tiefe Rekursion | §6.1, §5.2 |
| `L13` stabile ABI · `L14` C-FFI · `L15` Assembly · `L16` SIMD | §13, §3.6, §15.4 |
| `Z1`–`Z7` Zeichenketten inkl. WTF-16 | §8 |
| `N1`–`N7` Nebenläufigkeit | §7 |
| `P1`–`P9` Codegenerierung und Leistung | §10.3 |
| `G1`–`G4` Kompilierzeit-Codegenerierung | §6.4 |
| `C1`–`C7` Krypto und Constant-Time | §9 |
| `B1`–`B13` Standardbibliothek | §8, §3.4 — Rest in ROADMAP Phase 3 |
| `R1`–`R6` Laufzeit auf Karstos | ROADMAP Phase 6 |
| `W1`–`W10` Werkzeuge | §10.4 |

---

*Dieses Dokument ist die Wahrheit über das Ziel. Der Code ist die Wahrheit über
den Stand. Wo beide auseinandergehen, gewinnt der Code — und das Dokument wird
korrigiert, nicht der Code beschönigt.*
