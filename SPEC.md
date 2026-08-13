# Firn — Sprachspezifikation

**Arbeitstitel:** Firn · **Dateiendung:** `.fi` · **Stand:** v0.2 (2026-08-13)
**Autor:** Justin (GitHub: Flei123) · **Zielsysteme:** Karstos / karst-Kernel **und
die Karstos-Browser-Engine**, x86_64

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
| `async`/`await` in der Sprache | §7 |
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
* **Kein `async`/`await` im Compiler.** Zustandsmaschinen-Transformation zieht
  eine Laufzeit in die Sprache. Stattdessen Fäden/Tasks als Bibliothek und eine
  Ereignisschleife; `N6` bleibt damit auf **SOLL**-Niveau bewusst unerfüllt.
  Wird das für `Promise` und Generatoren zu unbequem, ist das der erste
  Kandidat für eine Revision — dann aber begründet und hier dokumentiert.
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

**Nicht enthalten (Stufe 0):** Module/Imports, `comptime`, Generics,
`interface`, `enum`/`match`, Fehlerunionen `!T`, `defer`/`drop`, Move-Prüfer,
Referenztypen `&T`/`inout T` als geprüfte Typen (nur Rohzeiger), Arenen,
**`Rc[T]`, `Gc[T]`, der gesamte GC**, `gc class` und Vererbung, Abwicklung/
`throw`, Gleitkomma, `u128`, **alle Zeichenkettentypen** (nur Byte-Arrays),
`secret`/Constant-Time, Standardbibliothek, Nebenläufigkeit, DWARF,
Paketverwaltung, aarch64, WASM, LLVM-Backend, Optimierung über
Konstantenfaltung/DCE hinaus.

### 14.1 Nachtrag: bewusste Abweichungen der Stufe-0-Umsetzung (`firnc0`)

Hält fest, wo die Umsetzung enger ist als der Text oben — damit Spezifikation und
Code nicht auseinanderlaufen.

1. **Aggregate an Funktionsgrenzen.** Parameter und Rückgabewerte dürfen nur
   *skalar* sein (Ganzzahl, `bool`, Zeiger) oder — beim Rückgabetyp — fehlen.
   Structs und Arrays werden per Zeiger übergeben. Grund: die
   System-V-Klassifikation zusammengesetzter Typen (INTEGER/SSE/MEMORY) ist
   umfangreich und fehleranfällig. Der Compiler meldet dafür einen sauberen
   Fehler, keinen Absturz.
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
9. **Höchstens 6 Funktionsparameter** (nur Registerargumente). Mehr meldet einen
   sauberen Fehler mit Zeile/Spalte.
10. **Parameter sind unveränderlich** (wie `let`-Bindungen).
11. **Kein Wiederholungsliteral `[wert; N]`** — Arrays werden elementweise
    initialisiert.
12. **`as` bindet stärker als die unären Operatoren.** `&s.a as u64` bedeutet
    `&(s.a as u64)`; gemeint ist `(&s.a) as u64`.
13. **Kein `break`/`continue`** — Schleifen werden über eine Bedingungsvariable
    verlassen.
14. **Assembler-Ausgabe** ist Intel-Syntax mit `.intel_syntax noprefix`. `as` und
    `ld` werden ausschließlich als Assembler bzw. Linker aufgerufen, nie ein
    C-Compiler.

---

## 15. Offene Fragen

1. **Präzises Stack-Scanning später nachrüsten?** Konservativ (§3.5.3) schließt
   einen kompaktierenden Sammler dauerhaft aus. Wenn Fragmentierung im
   Dauerlauf zum Problem wird, ist das die Stelle, an der nachgebessert werden
   muss — und es wird teuer. Entscheidung vertagt bis nach dem 24-h-Test.
2. **`async`/Koroutinen** (`N6`, SOLL). Zurzeit bewusst nicht in der Sprache.
   Wenn `Promise` und Generatoren in der JS-Engine ohne sie zu unbequem werden,
   ist das der erste Revisionskandidat.
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
