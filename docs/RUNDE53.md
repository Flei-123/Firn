# Runde 53 — `GcVec` und `GcMap`: Sammlungen mit veränderlicher Länge im GC-Heap

Branch `r53-gcvec`, Basis `cc1710f`. Diese Runde schließt die letzte Lücke, die
eine Voruntersuchung für die Browser-Engine als *echte* Blockade benannt hat:
`SPEC.md` §3.5.2 schreibt `GcVec[Gc[Node]]` und `GcMap[Atom, Str]` als
DOM-Grundlage fest, §1197 gab bis eben zu: „Keine `GcVec`/`GcMap`, kein
`virtual`." Solange das fehlte, blieb der DOM bei fester Attributzahl
(`attr_name: [u32; 4]`) und einer Geschwisterkette ohne Index.

Ergebnis vorweg, alles selbst gemessen:

| | Basis `cc1710f` | Runde 53 |
|---|---|---|
| `test.sh` | 751/751 | **763/763** |
| `tools/selbst_vergleich.sh` | 213 / 0 / 0 | **217 / 0 / 0** |
| `tools/fixpunkt.sh` | zeichengleich, 427 401 Zeilen | **zeichengleich, 470 042 Zeilen** |
| längste Unterbrechung, **Rechenzeit**, Median aus 7 Läufen | 476 711 ns | **496 501 ns** |
| Unterbrechungen über 1,02 ms (Rechenzeit) | 1 von 307 089 | **0 von 538 936** |
| DOM-Dauerlauf, RSS | flach | **flach (1644 → 1644 KiB)** |
| Zählverweis-Gegenprobe | leckt | **leckt (Faktor 520)** |
| Kinder je Knoten im DOM | Kette ohne Index | **5000, indiziert** |
| Attribute je Element | **4** | **unbegrenzt** |

---

## 1. Der Entwurf: ein zweites Objekt, der Slot-Puffer

Der Sammler verfolgt den Heap **präzise** über die compilergenerierte
Typtabelle: je Klasse eine feste Liste von Feldoffsets (`gc.rs::typtabelle_asm`,
`gc.fi::__gc_trace`). Eine Sammlung, die wächst, passt da nicht hinein — ihre
Elementzahl steht erst zur Laufzeit fest. Genau das war der Grund für §1197,
und es ist kein Schönheitsfehler, sondern eine Eigenschaft des Verfahrens.

Der Ausweg ist **kein** Sonderweg im Sammler, sondern ein zweites Objekt:

```text
Gc[GcVec] --puffer--> Gc[GcSlots] --Elemente--> Gc[T] …
   ^ gewöhnliches            ^ Blockkopf-Bit F_SLOTS
     starkes Feld:             -> __gc_trace_slots liest n, Schrittweite
     Typtabelle + Barriere        und Zeigermaske AUS DEM PUFFER
     legt der Compiler
```

Der Slot-Puffer ist ein ganz gewöhnlicher GC-Block; er trägt nur ein Bit mehr
im Zustandswort seines Kopfes. Nutzdaten:

```text
[0..8)    u64  n            Zahl der Elemente, die verfolgt werden
[8..16)   u64  desc         schritt | (maske << 8)
[16..24)  u64  fortschritt  Wiederaufnahmepunkt der Markierung
[24..32)  u64  kap          Kapazität in Elementen
[32..)         die Elemente
```

`desc` deckt mit **einem** Puffertyp alles ab, was gebraucht wird:
`GcVec[Gc[T]]` (Schritt 1, Maske 1), `GcVec[u64]` (1/0),
`GcMap[Gc[K],Gc[V]]` (2/3), `GcMap[Atom,Gc[V]]` (2/2).

**Was das Bit kostet: nichts.** `F_SLOTS` ist Bit 5 des Zustandsworts
`[4..8)` — Bits 5..7 waren frei (0..1 Marke, 2 `F_FIN`, 3 `F_WART`,
4 `F_GETAN`, 8..31 Aufräumart). `__gc_trace` liest und schreibt dieses Wort
ohnehin; der Test ist ein `and` auf einem Register.

### 1.1 Warum die Stückelung sein muss — mit Zahl

Ein Puffer kann Millionen Elemente tragen. In *einem* Stück verfolgt, risse er
das 100-µs-Zeitbudget einer Markierscheibe (Runde 41/44) und damit die
Pausenzusage. Also verfolgt `__gc_trace_slots` höchstens `SLOT_SCHEIBE = 64`
Elemente je Aufruf, merkt sich den Wiederaufnahmepunkt **im Puffer** und legt
sich selbst wieder auf den Markstapel — der Puffer bleibt so lange **grau**.
Grau heißt genau „noch nicht fertig verfolgt"; das ist keine Umdeutung.

Läuft der Markstapel dabei über, findet `__gc_nachtragen` den Puffer als grauen
Block wieder und macht bei `fortschritt` weiter. Die Vollständigkeit hängt also
nicht am Stapel.

Ob die Stückelung wirklich nötig ist, wurde **gemessen** statt behauptet
(`aufbau.fi`, ein Knoten mit 120 000 Kindern in einer `GcVec`, Rechenzeit,
Median aus 3 Läufen):

| `SLOT_SCHEIBE` | längste Unterbrechung (Rechenzeit) | Objekte in 3 s |
|---|---|---|
| 16 | 468 900 ns | 9 542 018 |
| 32 | 499 770 ns | 9 892 018 |
| **64 (gewählt)** | **503 890 ns** | 10 172 018 |
| 128 | 478 460 ns | 10 928 018 |
| **10⁹ (= keine Stückelung)** | **2 671 821 ns** | 10 284 018 |

**Ohne Stückelung ist die längste Unterbrechung 5,3-mal so lang.** Zwischen 16
und 128 gibt es dagegen keinen belastbaren Unterschied — die Werte liegen in
derselben Streuung, die drei Läufe desselben Standes auch untereinander haben.
Die Konstante ist also eine **Sicherung gegen den pathologischen Fall**, kein
Feinstellrad. 64 ist die Mitte des gemessenen Plateaus.

---

## 2. Die Barrierenfrage — die eigentliche Arbeit dieser Runde

Seit Runde 44 markiert der Sammler in Scheiben. Er darf also mitten im Wachsen
laufen. Drei Stellen sind kritisch; alle drei hängen an derselben
Dijkstra-Einfügebarriere aus `gc.fi`.

**(B1) Der neue Puffer.** `gcvec_platz` legt einen größeren Puffer an — und
`__gc_alloc_raw` ist genau die Stelle, an der eine Scheibe läuft. Danach kann
der Vektor längst **schwarz** sein. Der neue Puffer wird trotzdem gefunden,
weil er über das gewöhnliche starke Feld `v.puffer` eingehängt wird und der
Compiler dort `__gc_barrier` legt.

**(B2) Der alte Puffer während des Umkopierens.** Zwischen Allokation und
Einhängen alloziert *nichts* — es kann also keine Scheibe laufen. Der alte
Puffer hängt die ganze Zeit am Vektor, der neue am Stapel (konservativ
erfasst). Fällt der alte hinterher heraus, ist er entweder schon verfolgt oder
liegt noch grau auf dem Markstapel; in beiden Fällen sind seine Elemente
gesehen.

**(B3) Jedes eingetragene Element.** Ein Element wird mit `__gc_st64`
geschrieben, nicht über ein typisiertes Feld — der Compiler kann hier keine
Barriere legen, weil er den Puffer gar nicht kennt. Also ruft die Bibliothek
sie selbst (`gcvec_anhaengen_roh`, `gcvec_setzen_roh`, `gcmap_setzen` für
Schlüssel *und* Wert).

Was **nicht** nötig ist: den Puffer beim Wachsen von Hand wieder grau zu
färben. Jedes Element, das je in ihm steht, ist entweder über (B3) grau
geworden oder stand vorher im alten Puffer, der nach (B2) verfolgt wird. Diese
Begründung steht so auch im Kopfkommentar von `lib/gc/gcvec.fi` — sie lässt
sich aus dem Code nicht ablesen.

### 2.1 Die erste Hypothese war falsch, und das ist der interessante Teil

Die naheliegende Vermutung lautete: *„Ein frisch angehängtes Objekt verschwindet
hinter dem schwarzen Puffer."* Der Test dafür war gebaut, die Gegenprobe lief —
und blieb **grün, auch ohne Barriere**.

Der Grund steht in `__gc_alloc_raw` und ist eine Zusage aus Runde 38:

> Während eines Zyklus frisch allozierte Objekte sind **grau** und kommen auf
> den Markstapel.

Für frisch allozierte Elemente existiert der Fall also gar nicht. Der Fall, den
es **wirklich** gibt, ist das **Umhängen eines schon vorhandenen Objekts** —
also genau `appendChild`:

```text
Ziel   schon verfolgt (schwarz)
Quelle noch nicht verfolgt (weiß)
dann `z` aus der Quelle heraus und in das Ziel hinein
```

Der Sammler sieht `z` danach nirgends mehr: im Ziel nicht, weil das schon durch
ist, in der Quelle nicht, weil es dort nicht mehr steht. `z` bleibt weiß und
wird gefegt.

### 2.2 Wie oft der Fall von selbst eintritt: fast nie

Gemessen an 200 000 Anhängungen mit dem üblichen Müllstrom:

| | Zahl | Anteil |
|---|---|---|
| Anhängungen in Phase 1 (Markieren) | 526 | 0,26 % |
| Anhängungen in Phase 2 (Fegen) | 32 | 0,02 % |
| **Pufferwachstum während eines Zyklus** | **0** | **0 %** |

Das ist kein Zufall, sondern Arithmetik: eine Markierphase dauert
`lebende_objekte / SCHEIBE_TRACE` Scheiben, und zwischen zwei Sammelläufen
liegen `GRENZE / Blockgröße` Allokationen. Das Verhältnis ist rund 1 : 512.
**Ein Test, der auf den Zufall wartet, prüft nichts.**

Deshalb gibt es zwei neue Laufzeitregler in `gc.fi` — derselbe Kunstgriff wie
`S_CPUUHR` (Runde 44) und `S_INKRAB`: Voreinstellung 0, dann ist der Code
bitgleich zu vorher, und nur Testprogramme stellen sie um.

```firn
gc_set_scheibe(2)        // 2 Objekte je Markierscheibe statt 512
gc_set_zeitbudget(ns)    // Zeitbudget von Markieren und Fegen
gc_phase()               // 0 Ruhe, 1 Markieren, 2 Fegen, 3 Finalisierer
```

Mit `gc_set_scheibe(2)` erstreckt sich die Markierphase über so viele
Allokationen, wie es lebende Objekte gibt. In `tests/841` fallen damit **78 762
von 78 762** Umhängungen in eine laufende Markierung.

### 2.3 Die Gegenprobe — wirklich ausgeführt

| Eingriff | Ergebnis |
|---|---|
| `tests/841` **mit** Barriere in `gcvec_anhaengen_roh` | 0 |
| `tests/841` **ohne** diese Barriere | **96** — Markensumme 81 672 010 statt 81 926 400, es fehlten **254 390** |
| `tests/843` **mit** Wertbarriere in `gcmap_setzen` | 0 |
| `tests/843` **ohne** diese Barriere | **17** |

Der Test findet einen Verlust nur, wenn drei Dinge zusammenkommen: das Umhängen
fällt in eine laufende Markierung, **derselbe Zyklus wird auch zu Ende geführt**
(sonst findet der nächste Vollauf alles wieder), und danach wird eine Müllwelle
alloziert, damit ein fälschlich freigegebener Block **wiederverwendet** wird —
der Inhalt eines freien Blocks bleibt sonst lesbar und der Verlust fällt nicht
auf. Alle drei Punkte stehen so im Test.

---

## 3. `GcMap`: Zustand im Schlüssel

Dieselben Slot-Puffer, Schrittweite 2. Offene Adressierung, lineares Sondieren,
Zweierpotenz-Kapazität, Lastgrenze 3/4 — dieselbe Bauart wie `lib/rt/map.fi`,
damit sich die beiden Abbildungen der Sprache nicht unterschiedlich verhalten
(die Streufunktion ist bitgleich übernommen).

Der Zustand je Platz steht **im Schlüssel**, nicht in einem Nebenfeld:

```text
Schlüssel 0 = nie belegt   (Suche darf abbrechen)
Schlüssel 1 = gelöscht     (Suche darf NICHT abbrechen)
```

Das ist sicher, weil `__gc_mark(1)` über `__gc_block_von` den Chunk zur Adresse
1 sucht — es gibt keinen, also passiert nichts. Ein Grabstein kann den Sammler
nicht in die Irre führen. Der Preis ist eine Zusage nach außen, und die steht
in der Datei:

> **0 und 1 sind als Schlüssel reserviert.**

Für Gc-Zeiger kostet das nichts (keine Adresse ist 0 oder 1), für rohe
Kennungen die beiden kleinsten Werte. Die Alternative wäre ein zweites Feld je
Platz — ein Drittel mehr Speicher und ein zweiter Speicherzugriff je
Sondierschritt, für nichts.

Der verfolgte Bereich einer `GcMap` ist **immer die volle Kapazität**, nicht die
Zahl der Einträge: die Plätze liegen gestreut, nicht dicht. Leere Plätze tragen
0 und kosten beim Verfolgen einen Vergleich.

---

## 4. Die Compiler-Haken

Vier Eingriffe, alle in **beiden** Übersetzern, alle klein.

**(a) Die Sammlungen kommen in eigenen Dateien** — `lib/gc/gcvec.fi`,
`lib/gc/gcmap.fi` — und werden an `gc.fi` *angehängt*; zusammen sind sie ein
Modul, sehen also dessen Namen ohne `import`. Neue Dateien statt Eingriffen
mitten in `gc.fi`, damit der Merge mit Runde 49 (Thread-Sicherheit desselben
Sammlers) nicht zerfällt. In `gc.fi` selbst stehen nur: die drei Konstanten,
`__gc_trace_slots`, ein `if` in `__gc_trace`, die zwei Regler und `gc_phase()`.

**(b) Angehängt wird nur bei Bedarf.** `quelle_braucht_sammlungen` sucht im
Tokenstrom einen Bezeichner mit Präfix `GcVec`/`GcMap`/`gcvec_`/`gcmap_` —
derselbe Mechanismus wie `quelle_braucht_gc` und `quelle_hat_allocerror`. Ein
Programm mit `gc class`, aber ohne Sammlungen, erzeugt danach **denselben Code
wie vorher**. Ein Modultest hält das fest.

**(c) `GcVec[E]` und `GcMap[K,V]` als Typnamen.** Beide sind Zeiger auf die
Laufzeitklassen; `GcVec[Gc[Node]]` ist genau `Gc[GcVec]`, nur so geschrieben,
wie es in der SPEC steht. Die Typargumente werden vollständig geparst (ein
Tippfehler soll auffallen) und dann verworfen.

**(d) `Gc[T]` und `GcWeak[T]` dürfen in einer generischen Vorlage stehen.** Der
Parser macht daraus die Namen `__gc#p:T` / `__gc#w:T`; ohne diesen Haken sucht
die Typauflösung später eine gc-Klasse namens `T` und meldet „unbekannte
gc-klasse 'T'". Damit war eine generische Funktion über einen Gc-Zeiger
unmöglich — und genau die ist die typsichere Oberfläche der Sammlungen:

```firn
gcvec_anhaengen[Node](eltern.kinder, kind)
let k: Gc[Node] = gcvec_lesen[Node](eltern.kinder, i)
```

### 4.1 Was diese Runde NICHT liefert, und zwar ausdrücklich

* **Kein je Elementtyp eigener Behälter.** Stufe 0 hat keine generischen
  `gc class`. `GcVec[Gc[Node]]` und `GcVec[Gc[Element]]` sind derselbe nominale
  Typ; der Elementtyp wird am **Zugriff** geprüft (`gcvec_anhaengen[Node]`),
  nicht am **Feld**. Wer in denselben Vektor einmal `[Node]` und einmal
  `[Listener]` schreibt, bekommt keinen Fehler. Das ist eine echte Lücke und
  keine Formalie.
* **Keine Methoden.** `SPEC.md` §3.5.2 schreibt `parent.children.push(child)`;
  hier steht `gcvec_anhaengen[Node](eltern.kinder, kind)`. Dieselbe Entscheidung
  wie bei den Finalisierern der Runde 47, aus demselben Grund.
* **Kein `virtual`.** Unverändert offen (Runde 46 hat Interfaces gebracht,
  `virtual` nicht).

---

## 5. Der DOM benutzt die Sammlungen

`lib/dom/dom.fi`:

| vorher | jetzt |
|---|---|
| `erstes_kind` / `letztes_kind` / `naechstes` | `kinder: GcVec[Gc[Node]]` |
| `listener: Gc[Listener]` (Kette) | `listener: GcVec[Gc[Listener]]` |
| `attr_name: [u32; 4]`, `attr_wert: [u32; 4]` | `attrs: GcMap[u32, Gc[Str]]` |

Die Zyklen 1, 2 und 3 laufen damit **über Sammlungen** — der Fall aus
`SPEC.md` §3.5.2. Die Sammlungen entstehen **erst bei Bedarf**: ein Blatt ohne
Attribute und ohne Listener kostet weiterhin genau ein Objekt, und das ist im
DOM der häufigste Knoten überhaupt.

Neu im Selbsttest und vorher unmöglich: **5000 Kinder an einem Knoten** (mit
Index, nicht als Kette), **192 Attribute an einem Element**, und `removeChild`
mit erhaltener Geschwisterreihenfolge.

`DOM_OBJEKTE_JE_SATZ` steigt von 7 auf **14**: Wurzelelement (1),
Attributtabelle + Puffer + `Str` (3), drei Kinder (3), Kinderliste + Puffer (2),
Listener + Listenerliste + Puffer (3), Sammlung (1), Wrapper (1).

**Die Gegenprobe wurde mitgezogen.** `lib/dom/soak_leck.fi` bildet den Satz
Objekt für Objekt nach — 128-Byte-Objekt mit generischen Verweisspalten,
Kinderliste und ihr Puffer als eigene Objekte, 14 je Satz, davon lecken 13
(frei wird die Sammlung, die als einzige keinen Rückverweis hat). Sonst wären
nicht mehr dieselben zwei Graphen verglichen, und der Bericht in
`docs/berichte/dom.md` hinge in der Luft.

### 5.1 Dauerlauf

`tools/dom_soak/run.sh`, im Rahmen von `test.sh`:

| | GC-Fassung | Zählverweis-Gegenprobe |
|---|---|---|
| RSS Median 2. Viertel → letztes Viertel | **1644,0 → 1644 KiB** | 357 572 → 853 258 KiB |
| lebende Objekte | 27 → 27 | 6 825 000 |
| Urteil | **kein Leck** | **LECK** (wie verlangt) |

Eigenständiger Lauf mit größerem Budget: GC **1640 → 1640 KiB**, Gegenprobe
364 884 → 853 256 KiB, **Faktor 520**.

`SOAK_LECK_ZYKLEN` musste von 2 000 000 auf 600 000 herunter: ein Satz leckt
jetzt 13 Objekte zu 128 Byte statt 6 zu 64, das sind rund 1,0 GiB statt
770 MiB. Ein Werkzeug, das die Maschine mitreißen kann, ist ein kaputtes
Werkzeug (Runde 44).

---

## 6. Messung: sind die Pausen schlechter geworden?

`tools/gc_mess/r53_pausen.sh` (neu). Drei Fälle, **dasselbe** Messprogramm
(`aufbau.fi`, 120 000 lebende Knoten, `AB_SCHWELLE = 0` also immer
inkrementell):

* **A BASIS** — Baum bei `cc1710f`, eigener Compiler, alter DOM
* **B KERN** — Compiler und GC-Laufzeit der Runde 53, aber der **alte** DOM.
  Die Sammlungen werden nie benutzt; gemessen wird der Aufpreis des
  `F_SLOTS`-Zweigs allein.
* **C SAMMLUNGEN** — Runde 53, wie sie ist.

Maßgeblich ist die **Rechenzeit des Fadens**. Auf dieser Maschine liefen
mehrere Runden gleichzeitig; die Wanduhr misst dann Verdrängung, nicht den
Sammler (Fehlbefund der Runde 40). `callgrind` scheidet aus — es verschiebt den
Stapel (`docs/RUNDE47.md` §4.1).

### 6.1 Lauf I — 7 Läufe je 5 s

| Kennzahl (min / Median / max) | A Basis | B Kern | C Sammlungen |
|---|---|---|---|
| **längste Unterbrechung, Rechenzeit** | 425 790 / **476 711** / 1 049 161 ns | 407 900 / **446 020** / 925 520 ns | 471 420 / **496 501** / 566 110 ns |
| Unterbrechungen über 1,02 ms (Rechenzeit) | **1** von 307 089 | **0** von 285 135 | **0** von 538 936 |
| RSS (Median) | 12 904 KiB | 12 644 KiB | 13 860 KiB |
| Sammelläufe / volle STW (Median) | 62 / 0 | 58 / 0 | 290 / 0 |

### 6.2 Lauf II und III — je 3 Läufe zu 3 s, mit deterministischen Zählern

| | A | B | C |
|---|---|---|---|
| längste Unterbrechung, Rechenzeit (Median, Lauf II) | 456 641 ns | 443 980 ns | 566 890 ns |
| längste Unterbrechung, Rechenzeit (Median, Lauf III) | 528 830 ns | 476 620 ns | 604 371 ns |
| **Objekte alloziert** (Lauf III) | 2 325 001 | 2 325 001 | **10 690 018** |
| **Summe aller Sammlerpausen** (Lauf III) | 2 804 931 396 ns | 2 835 007 979 ns | **1 969 280 572 ns** |
| Markier- / Fegescheiben (Lauf III) | 25 967 / 493 | 26 240 / 488 | 47 089 / 2 349 |

**A und B allozieren auf das Objekt genau gleich viel** (2 325 001) — der Fall
B ist also wirklich derselbe Arbeitsablauf, nur mit dem neuen Sammler.

### 6.3 Was daraus folgt

* **Der Umbau des Sammlers kostet nichts.** B liegt in allen drei Läufen
  *unter* A (446 / 444 / 477 µs gegen 477 / 457 / 529 µs). Der `F_SLOTS`-Zweig
  und die zwei zusätzlichen Zustandswörter sind unter der Messschwelle; dass B
  systematisch besser aussieht, ist Codelayout-Rauschen und wird hier nicht als
  Verbesserung verkauft.
* **Der Umbau des DOM kostet 10 bis 15 % an der längsten Unterbrechung**
  (C 497 / 567 / 604 µs). Das liegt in derselben Größenordnung wie die 0,45 ms
  aus Runde 44 und die 460 µs aus Runde 47 und weiterhin deutlich unter 1 ms.
  Über alle Läufe zusammen: **1 Unterbrechung über 1,02 ms bei 837 087**
  gemessenen (Basis: 1 bei 466 024).
* **Der Durchsatz ist nicht vergleichbar** und wird deshalb nicht verglichen:
  ein Satz hat jetzt 14 statt 7 Objekte. Bemerkenswert ist trotzdem, was die
  deterministischen Zähler zeigen: C alloziert in derselben Zeit **4,6-mal so
  viele Objekte** und verbringt dabei **weniger** Zeit im Sammler (1,97 s statt
  2,80 s von 3 s). Der Grund liegt auf der Hand, ist aber nicht Gegenstand
  dieser Runde und deshalb nur als Beobachtung notiert: ein Knoten trägt statt
  drei Listenzeigern (`erstes_kind`, `letztes_kind`, `naechstes`) nur noch
  einen Verweis auf seine `GcVec`, und die Kinder liegen dicht in einem Puffer
  statt verstreut in einer Kette. Die Markierarbeit je Sammellauf sinkt dadurch
  auf etwa ein Drittel. **Belegt ist die Zahl, nicht die Ursache.**

---

## 7. Was schiefging

**Der Packer packte nur den Kern.** `tools/gen_gctext.sh` schreibt die
Laufzeit als u64-Wörter für den Selbsthosting-Compiler. Beim Umbau auf die
Verkettung `gc.fi + gcvec.fi + gcmap.fi` blieb die Schleifengrenze auf der
Länge von `gc.fi` stehen — die Sammlungen landeten in der Längenangabe, aber
nicht in den Daten. `firnc1` meldete dazu **gar nichts**: Stufe 0 zählt Fehler,
sie druckt sie nicht. Gefunden durch Bisektion mit Kleinstprogrammen.

**Die Vorlagen der Laufzeit waren zu spät bekannt.** `bin/firnc1.fi` sammelt die
generischen Namen aller Dateien in einem ersten Durchgang ein (`gen_vorab`) —
aber die Sammler-Laufzeit steht nicht auf der Platte und wird erst ganz zuletzt
geparst. Der Parser sah `gcvec_anhaengen[Z](…)` in der Wurzeldatei deshalb als
Indizierung statt als Ausprägung. Behoben mit einem Vorabscan der Laufzeit vor
dem Parsen; der Preis ist ein zusätzliches Lexen, genau wie bei den Modulen.

**Der erste Provokationstest prüfte nichts.** Siehe §2.1 — er testete einen
Fall, den es nicht gibt, und blieb ohne Barriere grün. Erst die Gegenprobe hat
das gezeigt. Ein Test, dessen Gegenprobe nicht anschlägt, ist kein Test.

**Die erste Pausenmessung maß den falschen Pfad.** `aufbau.fi` hat
`AB_SCHWELLE = 8388608` voreingestellt, also atomare Vollzyklen unterhalb von
8 MiB Halde. Gemessen kamen 11,6 ms heraus — das sind die drei
Stop-the-World-Läufe der Aufbauphase aus Runde 44, nicht die inkrementellen
Scheiben. Die Zahl stimmte, sie beantwortete nur eine andere Frage.
`r53_pausen.sh` setzt die Schwelle deshalb auf 0.

---

## 8. Verworfen

* **Ein je Elementtyp eigener `gc class`** (`GcVec[Node]` bekommt eine eigene
  Typkennung). Gäbe echte nominale Typsicherheit, verlangt aber eine
  Klassenregistrierung zur Parsezeit — und die Reihenfolge der Typkennungen
  müsste zwischen beiden Übersetzern auf das Bit übereinstimmen, sonst fällt
  `fixpunkt.sh`. Zu viel Risiko für den Gewinn, in §4.1 als Lücke benannt.
* **Ein neuer Eintrag in der Typtabelle** statt des Blockkopf-Bits. Der Eintrag
  ist 64 Byte groß und beginnt bei `tabelle+8`, liegt also über zwei
  Cache-Zeilen; jede Verfolgung hätte einen zusätzlichen Speicherzugriff
  bezahlt. Das Zustandswort wird ohnehin gelesen.
* **Ein segmentierter Vektor** (Kette von Puffern fester Größe, kein
  Umkopieren). Umgeht die Barrierenfrage, statt sie zu lösen — und macht den
  indizierten Zugriff, den der DOM braucht, wieder teuer.
* **Den Puffer als externen Wurzelbereich anmelden** (`gc_wurzel_anmelden`).
  Wäre trivial und wäre falsch: eine `GcVec` in einem toten Zyklus hielte ihre
  Elemente dann für immer.
* **Den Puffer beim Wachsen von Hand wieder grau färben.** Nicht nötig (§2),
  und es hätte den Fehler verdeckt, den `tests/841` finden soll.
* **`callgrind` für die Pausenmessung.** Verschiebt den Stapel; seit Runde 47
  bekannt und in `docs/RUNDE47.md` §4.1 begründet.

---

## 9. Offen

* **Nominale Typsicherheit der Behälter** (§4.1). Die dringendste der drei
  Lücken.
* **Methodensyntax** `parent.children.push(child)` — braucht Methoden auf
  `gc class`, also eine eigene Runde.
* **`virtual`** — unverändert offen.
* **`GcMap` mit Schlüsseln 0 und 1.** Reserviert (§3). Wer sie braucht, muss
  ein zweites Zustandsfeld je Platz bezahlen.
* **Iteratoren.** Der Durchlauf einer `GcMap` läuft über alle Plätze und
  überspringt die leeren. Bei einer sehr dünn besetzten Abbildung ist das
  teuer.
* **`gcvec_entfernen` ist O(n).** Für `removeChild` an einem Knoten mit sehr
  vielen Kindern ist das die falsche Komplexität.
* **Fragmentierung bei sehr großen Puffern.** Ein Puffer über der größten
  Größenklasse bekommt einen eigenen Chunk; ein Vektor, der lange wächst,
  hinterlässt eine Spur immer größerer Einzelchunks. Nicht gemessen.
* **Der 24-Stunden-Lauf** aus der Abnahme steht weiter aus.

---

## 10. Dateien

| Datei | Inhalt |
|---|---|
| `lib/gc/gc.fi` | `F_SLOTS`, `SLOT_KOPF`, `SLOT_SCHEIBE`, `__gc_trace_slots`, ein `if` in `__gc_trace`, `gc_set_scheibe`, `gc_set_zeitbudget`, `gc_phase` |
| `lib/gc/gcvec.fi` | `GcSlots`, `GcVec`, Wachsen mit Barriere, typisierte Sicht |
| `lib/gc/gcmap.fi` | `GcMap`, offene Adressierung, Grabsteine, Durchlauf |
| `compiler/src/gc.rs` | `quelle_braucht_sammlungen`, `GcVec[…]`/`GcMap[…]` im Parser, `laufzeit_quelle` mit drittem Parameter |
| `compiler/src/mono.rs` | `Gc[T]`/`GcWeak[T]` in einer Vorlage |
| `lib/firnc1/parser.fi`, `mono.fi`, `gc.fi` | dasselbe in Firn |
| `tools/gen_gctext.sh`, `lib/firnc1/gctext.fi` | Laufzeit als Daten, jetzt mit zwei Längen |
| `bin/firnc1.fi` | Vorabscan der Laufzeit-Vorlagen |
| `tests/840`–`843` | Grundlagen, der inkrementelle Fall, Zusammenspiel |
| `lib/dom/dom.fi`, `soak_leck.fi` | der DOM auf Sammlungen, Gegenprobe mitgezogen |
| `tools/gc_mess/r53_pausen.sh` | die Pausenmessung dieser Runde |
