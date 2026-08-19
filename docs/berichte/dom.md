# DOM-Prototyp und Dauerlauf — Bericht (Abnahmepunkt 2)

Stand 14.08.2026. Alle Zahlen sind selbst gemessen; die Rohdaten liegen als TSV
im Baum und lassen sich mit `tools/dom_soak/run.sh` neu erzeugen.

## Worum es geht

`FIRN-ANFORDERUNGEN.md` §13, Punkt 2 verlangt einen **DOM-Prototyp mit Eltern-/
Kind-Rückverweisen und Listener-Zyklen, der im Dauerlauf nicht leckt**. Das ist
der Punkt, an dem sich das Speichermodell entscheidet: der DOM ist ein
**zyklischer** Graph, und ein Zählverweis gibt einen Zyklus prinzipiell nie
frei. Gecko musste deshalb nachträglich einen Zyklensammler einbauen; `SPEC.md`
§3.5 zieht daraus die Konsequenz und entscheidet sich für einen
**Opt-in-Tracing-GC**.

Ein Bericht, der nur behauptet „läuft ohne Leck", ist wertlos. Deshalb misst
dieser Aufbau **zwei** Programme mit identischem Objektgraphen:

| Fassung | Speichermodell | Erwartung |
|---|---|---|
| `lib/dom/soak_gc.fi` | `gc class` / `Gc[T]`, Mark-Sweep | **darf nicht wachsen** |
| `lib/dom/soak_leak.fi` | eigener Zählverweis über `mmap(2)` | **muss lecken** |

Bleibt die Gegenprobe grün, bricht `tools/dom_soak/run.sh` mit Fehler ab: eine
Messung, die ein Leck gar nicht anzeigen kann, ist keine Messung.

## Der Objektgraph (`lib/dom/dom.fi`, 6 Zyklenarten)

Jede Art kommt in einer echten Engine wirklich vor:

1. **Knoten ↔ Kind** — `Node.eltern` ist ein **starker** `Gc[Node]`, der Elter
   hält seine Kinder ebenfalls stark. Kein `GcWeak`, sonst wäre der Zyklus
   wegdefiniert statt aufgelöst.
2. **`Element extends Node`** — Vererbung mit Präfixlayout, Attribute als
   Atom-Kennung → Wert, geprüfte Abwärtsumwandlung `x.as?[Element]`.
3. **Knoten ↔ Listener** — der Knoten hält den Listener, der Listener hält
   seinen Knoten. Genau dieser Zyklus zwang Gecko zum Zyklensammler.
4. **Sammlung → Wurzel** — eine live `HTMLCollection`-artige Struktur hält
   ihren Wurzelknoten.
5. **Observer über `GcWeak[Node]`** — Gegenprobe: hält sein Ziel **nicht** am
   Leben, `stark(w)` liefert nach dem Sammellauf den Nullwert.
6. **Knoten ↔ JS-Wrapper** — der simulierte Wrapper hält den Knoten, der Knoten
   hält den Wrapper. Das ist die Grenze DOM ↔ JS-Engine, um die es in
   `FIRN-ANFORDERUNGEN.md` §1 geht.

Ein Satz aus `dom_zyklus_bauen()` besteht aus **7 Objekten**: Wurzelelement,
drei Kindknoten, Listener, Sammlung, Wrapper.

## Ergebnis des Dauerlaufs

### GC-Fassung — 100 Millionen Zyklensätze

`tools/dom_soak/longrun/gc-100mio-cycles.tsv` (1.001 Stichproben):

| Größe | Wert |
|---|---|
| Zyklensätze | **100.000.000** |
| angelegte Objekte | **700.000.000** |
| Laufzeit | 116.459 ms (≈ 1 min 56 s) |
| **RSS ab der ersten Stichprobe** | **1.364 KiB** |
| **RSS am Ende** | **1.364 KiB** |
| RSS-Höchstwert über den ganzen Lauf | **1.364 KiB** |
| lebende Objekte (Stichproben) | 8–12 |
| Sammelläufe | 47.300 |
| Haldengröße | 1.310.720 B, konstant |
| längste Pause | 3,54 ms |

Stichproben quer durch den Lauf:

| Zyklen | RSS | lebende Objekte | Sammelläufe |
|---|---|---|---|
| 100.000 | 1.364 KiB | 12 | 47 |
| 1.000.000 | 1.364 KiB | 11 | 473 |
| 10.000.000 | 1.364 KiB | 12 | 4.730 |
| 50.000.000 | 1.364 KiB | 11 | 23.650 |
| 100.000.000 | 1.364 KiB | 8 | 47.300 |

**Der Verbrauch ist über 700 Millionen Objekte hinweg auf das Byte konstant** —
kein einziger Messpunkt weicht ab. Die 8–12 „lebenden" Objekte sind die
Zufallstreffer des konservativen Stapelscans; sie schwanken, wachsen aber nicht.
Ein zweiter, identischer Lauf lieferte dasselbe Bild bei 131.401 ms und einer
längsten Pause von 6,58 ms — die Pausenlänge streut, der Verbrauch nicht.

### Zählverweis-Gegenprobe — dasselbe Programm, anderes Speichermodell

`tools/dom_soak/longrun/leak-2mio-cycles.tsv`:

| Zyklen | RSS | lebende Objekte |
|---|---|---|
| 100.000 | 37.536 KiB | 600.000 |
| 1.000.000 | 375.056 KiB | 6.000.000 |
| 2.000.000 | **750.080 KiB** | **12.000.000** |

Pro Satz bleiben **6 von 7 Objekten** liegen: alles, was in einem Zyklus hängt.
Freigegeben wird genau die **Sammlung** — sie hat als einzige keinen
Rückverweis. Damit ist belegt, dass der Zähler **korrekt** arbeitet und
ausschließlich an den Zyklen scheitert. Das ist der Kern der Sache: nicht die
Umsetzung ist schlecht, das Verfahren kann es nicht.

**Verhältnis am selben Messpunkt (2 Mio. Zyklen): 1.364 KiB gegen 750.080 KiB —
Faktor 550.** Nach dem Urteil von `run.sh` (Median des letzten Viertels):
Faktor **481**.

## Was schiefging (und warum es hier steht)

Der erste Langlauf der Gegenprobe war **ungedeckelt** und lief auf 100 Mio.
Zyklen zu. Bei 384 Byte Leck je Zyklus sind das ≈ 38 GB; die Maschine hat 12 GB.
Ich habe den Lauf bei 7,0 GB abgebrochen. `tools/dom_soak/run.sh` hat seitdem
zwei Bremsen:

* `SOAK_LECK_ZYKLEN` (Standard **2.000.000**, ≈ 770 MiB) begrenzt die
  Gegenprobe unabhängig von der Zyklenzahl der GC-Fassung;
* `ulimit -v` (Standard 3 GiB) als harte Grenze, falls die erste versagt.

Ein Werkzeug, das die Maschine mitreißen kann, ist ein kaputtes Werkzeug — auch
wenn die Messung selbst stimmte.

## Was der Test wirklich prüft (und was nicht)

**Geprüft:**

* dass der Sammler Zyklen über zwei und über sieben Objekte auflöst,
* dass `GcWeak` sein Ziel nicht am Leben hält,
* dass der Verbrauch über 100 Mio. Sätze flach bleibt (RSS, nicht Selbstauskunft),
* dass alle drei Baustufen (`release-fast`, `--no-opt`, `dev-fast`) dasselbe
  Ergebnis liefern — ein Speichermodell, das nur mit Optimierer hält, taugt nichts,
* dass die Gegenprobe wirklich anschlägt.

**Nicht geprüft, ehrlich benannt:**

* **Der 24-Stunden-Lauf aus der Abnahme steht aus.** 131 Sekunden mit 100 Mio.
  Sätzen sind ein starker Hinweis, aber nicht die geforderte Zusage.
* **Fragmentierung** über lange Zeit bei *wechselnden* Objektgrößen. Der
  Dauerlauf benutzt immer denselben Satz; das ist der freundliche Fall.
  `SPEC.md` §3.5 nennt genau das als Hauptrisiko des nicht kompaktierenden
  Sammlers.
* **Kein inkrementelles Sammeln** (`S5`), **keine Finalisierer** (`S4`), kein
  `GcVec`/`GcMap`, kein `virtual`. Die längste gemessene Pause ist **6,58 ms** —
  für einen Browser mit 16-ms-Bildabstand ist das bereits zu viel und der Grund,
  warum `S5` in der ROADMAP steht.
  *(Stand 14.08.2026. Inkrementelles Sammeln kam in Runde 44, Finalisierer in
  Runde 47, `GcVec`/`GcMap` in Runde 53 — siehe den Nachtrag am Ende dieses
  Berichts.)*
* **Ein Faden.** Der Zustandsblock ist fadenlokal gedacht, Stufe 0 hat nur einen
  Faden.

## Die unbequeme Stelle: konservativer Stapelscan

Der Sammler scannt Stapel und Register **konservativ** (`SPEC.md` §3.5.3). Das
hat eine Folge, die man beim Testen sehen kann und die deshalb hier steht:

> Eine alte Kopie eines starken Zeigers in einem **lebenden** Stapelrahmen hält
> das Objekt am Leben.

Nachgemessen am Beobachter-Test: steht `stark(b.ziel)` unmittelbar im Rumpf des
prüfenden Aufrufers, liegt der starke Zeiger in dessen Rahmen — das Ziel
überlebt und der Test misst etwas, das die Sprache nie zugesagt hat. Mit
`--no-pass=inline` bestand derselbe Test, mit Einbettung nicht. Deshalb:

* `dom_observer_lebt()` liefert ein `bool` statt eines `Gc[Node]` und ist
  **rekursiv**, damit der Einbetter sie nicht in den lebenden Rahmen zieht;
* `dom_zyklus_verwerfen()` überschreibt den toten Stapelbereich unterhalb des
  aktuellen Rahmens (dieselbe Bauart wie `__gc_scrub_tief` in der Laufzeit).

Das ist kein Trick, sondern der dokumentierte Preis der Bauart. Wer ihn nicht
zahlen will, braucht präzise Stapelkarten — und die brauchen ein anderes
Codegen-Modell (ROADMAP, nach der echten Registerzuteilung).

## Dateien

| Datei | Inhalt |
|---|---|
| `lib/dom/dom.fi` | DOM-Prototyp, 6 Zyklenarten, Selbsttest |
| `lib/dom/meas.fi` | Zeit, RSS aus `/proc/self/statm`, TSV-Ausgabe (ohne GC) |
| `lib/dom/soak_gc.fi` | Dauerlauf mit `Gc[T]` |
| `lib/dom/soak_leak.fi` | Gegenprobe mit Zählverweis, selbst enthalten |
| `tools/dom_soak/run.sh` | Bau in drei Stufen, beide Läufe, Auswertung, Urteil |
| `tools/dom_soak/messung-gc.tsv` | letzte Messreihe GC |
| `tools/dom_soak/messung-leck.tsv` | letzte Messreihe Gegenprobe |
| `tools/dom_soak/longrun/gc-100mio-cycles.tsv` | der 100-Mio-Lauf |
| `tests/560_dom_cycles.fi` | Strukturtest, läuft in allen drei Baustufen |

---

## Nachtrag Runde 53 (19.08.2026): der Prototyp benutzt jetzt Sammlungen

`docs/RUNDE53.md` hat `GcVec`/`GcMap` gebracht, und `lib/dom/dom.fi` ist darauf
umgestellt. Was sich am Objektgraphen geändert hat:

| vorher | jetzt |
|---|---|
| `erstes_kind` / `letztes_kind` / `naechstes` | `kinder: GcVec[Gc[Node]]` |
| `listener: Gc[Listener]` (Kette) | `listener: GcVec[Gc[Listener]]` |
| `attr_name: [u32; 4]`, `attr_wert: [u32; 4]` | `attrs: GcMap[u32, Gc[Str]]` |

Die Zyklenarten 1, 2 und 3 laufen damit **über Sammlungen**. Ein Satz besteht
jetzt aus **14** statt 7 Objekten: Wurzelelement (1), Attributtabelle + Puffer
+ `Str` (3), drei Kinder (3), Kinderliste + Puffer (2), Listener +
Listenerliste + Puffer (3), Sammlung (1), Wrapper (1).

**Die Gegenprobe wurde mitgezogen** — `lib/dom/soak_leak.fi` bildet denselben
Satz Objekt für Objekt nach (128-Byte-Objekt, generische Verweisspalten,
14 Objekte, davon lecken 13). Ohne das wären nicht mehr dieselben zwei Graphen
verglichen worden, und der ganze Bericht hinge in der Luft.

Messung mit `tools/dom_soak/run.sh` (im Rahmen von `test.sh`, 19.08.2026):

| | GC-Fassung | Zählverweis-Gegenprobe |
|---|---|---|
| RSS Median 2. Viertel → letztes Viertel | **1.644,0 → 1.644 KiB** | 357.572 → 853.258 KiB |
| lebende Objekte | 27 → 27 | 6.825.000 |
| Urteil | **kein Leck** | **LECK** (wie verlangt) |

Eigenständiger Lauf mit größerem Budget: GC **1.640 → 1.640 KiB**, Gegenprobe
364.884 → 853.256 KiB, **Faktor 520**.

`SOAK_LECK_ZYKLEN` steht seitdem auf **600.000** statt 2.000.000: ein Satz leckt
jetzt 13 Objekte zu 128 Byte statt 6 zu 64, das sind rund 1,0 GiB statt
770 MiB. Dieselbe Bremse, derselbe Grund wie oben.

**Pausen:** die längste Unterbrechung in Rechenzeit liegt bei 120.000 lebenden
Knoten im Median bei **497 µs** (Basis vor Runde 53: 477 µs), keine einzige von
538.936 gemessenen Unterbrechungen über 1,02 ms. Ohne die Stückelung der
Puffer-Verfolgung wären es **2,67 ms** — die Zahl steht in `docs/RUNDE53.md`
§1.1.
