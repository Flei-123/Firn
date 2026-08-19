# `Rc` / `Weak` — Stufe 2 des Speichermodells

Bezug: `SPEC.md` §3.2 (drei Stufen), §3.4 (`Rc[T]`, `Weak[T]`, `Arc[T]`),
§3.6 (Rohzeiger), `DESIGNZIELE.md` §2 (fehlbare Allokation),
`../karstos-browser/FIRN-ANFORDERUNGEN.md` Anforderung **S7**.

Diese Datei beschreibt, was im Baum steht, wie es benutzt wird, und **was
bewusst fehlt**. Alles hier Behauptete ist mit einem Testprogramm belegt, das
`bash test.sh` in drei Baustufen faehrt.

---

## 1. Wozu

Stufe 2 ist fuer **geteilte, unveraenderliche** Werte: berechnete Stilwerte,
internierte Atome, Schriftdaten. Genau die Faelle, in denen Stylo `Arc` nimmt.
Ein Wert, viele Leser, keine Aenderung, Freigabe beim letzten Leser.

`Rc` ist **nicht** das Werkzeug fuer den DOM — siehe Abschnitt 6.

---

## 2. Wo es liegt

| Datei | Rolle |
|---|---|
| `tests/modules/rc.fi` | **die eine Implementierung** (Modul `rc`) |
| `lib/rc/rc.fi` | Symlink auf genau diese Datei — der Bibliothekspfad existiert, ohne Code zu doppeln |
| `lib/rc/parts/*.fi` | die Rumpfe der Testprogramme |
| `lib/rc/gen_tests.sh` | setzt Rumpf + Implementierung zu `tests/55*_rc_*.fi` und `tests/neg/rc_*.fi` zusammen |
| `tests/550_rc_basic.fi` … `554`, `tests/neg/rc_*.fi` | die erzeugten Testprogramme |

**Warum zusammenkopiert und nicht `import modules.rc`?** Stufe 0 loest
generische Vorlagen nicht ueber Modulgrenzen auf. Beide Formen scheitern
bereits im Parser:

```
var a: rc.Zaehlverweis[Stil] = …   // error: erwartet '=' nach dem namen in einer 'var'-anweisung
rc.rc_neu[Stil](&h, w, &a)         // error: nur direkte funktionsnamen koennen aufgerufen werden
```

(`compiler/src/sema_generic.rs::hook_generic_call` verlangt einen einfachen
`Ident`; das Modulsystem liefert an dieser Stelle einen `Field`-Ausdruck.)
Dieselbe Loesung benutzt `lib/str` bereits seit Runde 2
(`tools/strlib/expand.py`): die Bibliothek steht **einmal** im Baum und wird
woertlich in die Testprogramme eingesetzt. Erzeugen mit

```
bash lib/rc/gen_tests.sh
```

---

## 3. Schnittstelle

```firn
error AllocError { OutOfMemory }

struct Zaehlverweis[T]  { block: usize }   // = Rc[T]   aus SPEC §3.4
struct Schwachverweis[T]{ block: usize }   // = Weak[T] aus SPEC §3.4
struct RcHeap { … }                        // Halde fester Kapazitaet
```

| Funktion | Bedeutung |
|---|---|
| `rc_heap_init(h, bytes) -> bool` | Halde per `mmap` anlegen. `false` = fehlgeschlagen — **sichtbarer** Fehlschlag, kein stiller Ersatz. Kein `MAP_FIXED`, keine feste Adresse. |
| `rc_heap_frei(h)` | `munmap` |
| `rc_neu[T](h, wert, aus) -> AllocError!bool` | Wert auf die Halde legen, starker Verweis nach `*aus`. **Fehlbar** (DESIGNZIELE §2), Ergebnis ist `#[must_consume]`. |
| `rc_lesen[T](r) -> T` | **nur lesen** — liefert eine Kopie |
| `rc_klonen[T](r) -> Zaehlverweis[T]` | starker Zaehler + 1 |
| `rc_freigeben[T](h, r)` | starker Zaehler − 1, leert `*r`; bei 0 und ohne schwachen Verweis geht der Block in die Freiliste |
| `rc_stark_zahl[T](r)`, `rc_schwach_zahl[T](r)` | Zaehlerstaende, messbar |
| `rc_leer[T]()`, `rc_ist_leer[T](r)`, `rc_gleich[T](a,b)` | Nullwert, Test, Identitaet |
| `weak_von[T](r) -> Schwachverweis[T]` | schwacher Verweis, haelt **nicht** am Leben |
| `aufwerten[T](w) -> Zaehlverweis[T]` | Aufwertung; **sichtbar leer**, wenn der starke Zaehler 0 ist |
| `weak_freigeben[T](h, w)` | schwacher Zaehler − 1; gibt den Block frei, wenn auch stark 0 ist |
| `rc_heap_lebende/belegte_bytes/allokationen/freigaben(h)` | echte Zaehlwerte fuer Tests und Messungen |
| `rc_roh_adresse`, `rc_wert_adresse`, `rc_roh_verweis` | Rohzugriff nach SPEC §3.6, ausdruecklich fuer Werkzeuge und den Leck-Nachweis |

Blocklayout: 32 Byte Kopf (`stark`, `schwach`, `klasse`, Freilisten-Verkettung),
danach der Wert. Acht Groessenklassen 64 … 8192 Byte mit je einer Freiliste;
groessere Nutzlasten sind `AllocError::OutOfMemory` (belegt in
`tests/553_rc_fallible.fi`).

### Beispiel

```firn
var h: RcHeap = heap_leer()
if rc_heap_init(&h, 65536 as usize) == false { return 90 }

var a: Zaehlverweis[Stil] = rc_leer[Stil]()
let ok: bool = rc_neu[Stil](&h, Stil{ farbe: 1 as u32, groesse: 16 as u32, zeilen: 3 }, &a)
               catch false
if ok == false { return 1 }

var b: Zaehlverweis[Stil] = rc_klonen[Stil](a)   // stark = 2
let s: Stil = rc_lesen[Stil](b)                  // nur lesen
rc_freigeben[Stil](&h, &b)                       // stark = 1
rc_freigeben[Stil](&h, &a)                       // stark = 0 -> Block frei
```

---

## 4. `Rc` ist IMMER unveraenderlich

Es gibt kein `RefCell`-Aequivalent, keine Innenveraenderlichkeit und in diesem
Modul **keine schreibende Funktion**. `rc_lesen` liefert eine Kopie; der
Versuch, ueber sie den geteilten Wert zu aendern, ist ein Compilerfehler:

```
tests/neg/rc_unveraenderlich.fi:396:5
error: linke seite ist kein zuweisbarer ausdruck (variable, feld, index oder '*zeiger')
```

Wer teilen **und** aendern will, nimmt `Gc[T]` (SPEC §3.5) oder einen Lock.

---

## 5. Fehlbare Allokation

`rc_neu` liefert `AllocError!bool`. Ein verworfenes Ergebnis ist ein
Compilerfehler:

```
tests/neg/rc_discarded.fi:393:5
error: das ergebnis darf nicht verworfen werden: der typ 'AllocError!bool'
       ist mit #[must_consume] gekennzeichnet
```

`tests/553_rc_fallible.fi` belegt: Halde mit einer Seite → 64 Bloecke, die 65.
Allokation meldet `AllocError::OutOfMemory`, der Ausgabeverweis bleibt leer;
nach einer Freigabe gelingt die naechste Allokation wieder; eine zu grosse
Nutzlast scheitert ebenfalls sauber; `try` reicht den Fehler durch die
Aufrufkette.

Anders als beim GC gibt es hier **keinen Sammellauf vor dem Fehler** — die
Zaehlung gibt sofort frei, es gibt nichts nachzuholen.

---

## 6. Zyklen lecken — und warum `Rc` fuer den DOM nicht vorgesehen ist

**Das ist kein Fehler dieser Umsetzung, sondern die Eigenschaft der Zaehlung.**
Halten sich zwei Werte gegenseitig stark, faellt kein Zaehler je auf 0. Der
Speicher bleibt bis zum Programmende gehalten.

`tests/552_rc_cycle_leak.fi` macht das sichtbar statt es zu verstecken.
Gemessene Ausgabe (in allen drei Baustufen gleich):

```
1 1 2 128 200 12800 198 1 0
```

* `1 1` — nach dem Verwerfen **beider** aeusseren Griffe stehen beide starken
  Zaehler weiter auf 1: der Zyklus haelt sich selbst.
* `2 128` — zwei lebende Bloecke, 128 belegte Bytes, obwohl niemand mehr
  herankommt.
* `200 12800` — nach 100 angelegten **und verworfenen** Zyklen sind es 200
  Bloecke und 12.800 Bytes. Der Verbrauch waechst monoton: ein Leck.
* `198` — von Hand aufgeloest wird nur der eine Zyklus, dessen Adressen der
  Test noch kennt. Fuer die uebrigen 99 ist die Adresse weg — genau das ist die
  Lage im echten Programm.
* `1 0` — dieselbe Struktur mit `Weak` statt einem zweiten starken Verweis:
  die Aufwertung liefert sichtbar leer (`1`) und am Ende sind **null** Bloecke
  belegt (`0`).

Zum Vergleich `tests/554_rc_dauerlauf.fi`: 20.000 Runden mit Anlegen, Klonen,
schwachen Verweisen und Freigeben **ohne** Zyklus enden mit
`20000 0 0 192 1` — null lebende Objekte, null belegte Bytes, und die Halde
waechst nie ueber 192 Byte hinaus (die Freilisten werden wiederverwendet).
Die Zaehlung selbst leckt also nicht; nur der Zyklus tut es.

Weil `Rc` immer unveraenderlich ist, laesst sich ein Zyklus mit gewoehnlichem
Anwendungscode nicht einmal bauen — der Test traegt den Rueckverweis ueber
einen Rohzeiger nach (SPEC §3.6). Mit Innenveraenderlichkeit (die es hier
bewusst nicht gibt) waere derselbe Zyklus normaler Anwendungscode.

### Der DOM

Ein DOM besteht fast nur aus Zyklen: Elternverweis **und** Kinderliste,
Listener, die ihren Knoten halten, waehrend der Knoten den Listener haelt,
live `HTMLCollection`s, JS-Wrapper, die den Knoten halten, waehrend der Knoten
seinen Wrapper haelt. Jeder dieser Zyklen leckt unter Zaehlung. Von Hand
`Weak` zu setzen funktioniert nur, wenn der Zyklus **offensichtlich und lokal**
ist — beim DOM ist er weder das eine noch das andere.

Deshalb steht in `SPEC.md` §3.4 ausdruecklich: **`Rc` ist fuer den DOM nicht
vorgesehen.** Dafuer gibt es Stufe 3, den Opt-in-Tracing-GC aus §3.5
(`gc class`, `Gc[T]`, `GcWeak[T]`). `Rc` bleibt fuer geteilte, unveraenderliche
Blaetter des Graphen: Stilwerte, Atome, Schriftdaten.

---

## 7. Abweichungen von SPEC §3.4 und offene Punkte

Diese Punkte gehoeren nach `SPEC.md` §14.1; sie werden hier vollstaendig
aufgezaehlt und sind **nicht** in der SPEC wegretuschiert.

| Nr. | Abweichung | Grund |
|---|---|---|
| A1 | Die Typen heissen `Zaehlverweis[T]` / `Schwachverweis[T]`, nicht `Rc[T]` / `Weak[T]` | `Rc`, `Arc`, `Weak` sind im Parser als noch nicht umgesetzte Typkonstruktoren reserviert (`compiler/src/parser.rs::nicht_umgesetzter_typ`) und melden „ist in Stufe 0 nicht umgesetzt". Ein reines Firn-Modul kann diese Namen nicht belegen; die Compilerquellen gehoeren in dieser Runde anderen Modulen. Die **Funktionsnamen** folgen dem Vertrag. |
| A2 | `rc_neu(…)` statt `Rc[T].neu(…)` | Stufe 0 kennt keine Methoden |
| A3 | `h: *mut RcHeap` statt `inout alloc` | Stufe 0 kennt kein `inout` |
| A4 | Rueckgabe `AllocError!bool` + Ausgabezeiger statt `AllocError!Rc[T]` | Die Monomorphisierung setzt Typargumente in der Nutzlast einer Fehlerunion nicht ein: `fn f[T](..) -> AllocError!Zaehlverweis[T]` meldet „unbekannter typ 'Zaehlverweis__T'". Die Allokation bleibt vollstaendig fehlbar und `#[must_consume]`. |
| A5 | `Arc[T]` ist seit **Runde 47** gebaut: `lib/rc/arc.fi`, Typen `Atomverweis[T]`/`AtomSchwachverweis[T]`, Zaehler wirklich atomar (`__atomar_addieren` -> `lock xadd`, `compiler/src/atomic.rs`) | Nachweis `tools/atomic/run.sh` und `tests/830`-`833`. Ehrlich benannt bleibt: `aufwerten_atomar` braucht fuer echte Nebenlaeufigkeit einen Vergleichs-Tausch, den Runde 47 nicht baut, und Faeden hat Stufe 0 weiterhin keine (SPEC §7). Siehe `docs/RUNDE47.md`. |
| A6 | Keine Destruktoren: in einem Wert gespeicherte Verweise muessen von Hand geloest werden | `drop` (SPEC §3.3) ist in Stufe 0 nicht gebaut. Betrifft nur Werte, die selbst Verweise enthalten. |
| A7 | Halde mit fester Kapazitaet, kein Nachwachsen | macht die Allokation ehrlich fehlbar und ist die Grundlage fuer Speichergrenzen pro Auftrag |
