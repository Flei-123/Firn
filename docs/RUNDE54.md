# Runde 54 — HTML-Baumkonstruktion und DOM-Kern

Branch `r54-dom`, Basis `cc1710f`. Diese Runde baut die Stufe hinter dem
Tokenizer: aus dem Tokenstrom wird ein **DOM-Baum**, nach WHATWG §13.2.6, in
Firn geschrieben.

Ergebnis vorweg, alles selbst gemessen (Zahlen in §7):

| | Basis (`cc1710f`) | Runde 54 |
|---|---|---|
| `bash ./test.sh` | 751 / 751 | **761 / 761** |
| `bash tools/selbst_vergleich.sh` | 213 gleich / 0 abweichend / 0 fehlerhaft | **216 / 0 / 0** |
| eigene Baumfälle (WHATWG, von Hand) | — | **150 / 150** (in allen drei Baustufen) |
| davon von html5lib 1.1 bestätigt | — | **149 / 150** (1× folgt html5lib einer älteren Fassung) |
| echte Seiten aus `testdata/realweb/` | — | **8 / 8 baumgleich zu html5lib**, Zeile für Zeile |
| bekannte Lücken (eigene Datei, müssen fehlschlagen) | — | **0 / 10** |
| Dauerlauf: DOM-Bäume bauen und verwerfen | — | 20 000 Runden, 2 560 000 GC-Objekte, **RSS-Zuwachs 0 KiB** |
| Gegenprobe (jeder Baum wird festgehalten) | — | **+41 756 KiB** — die Messung kann ein Leck sehen |
| html5lib-Tokenizer-Suite | 6810 / 6810 | **6810 / 6810** |
| Tokenizer-Durchsatz durch den Eingriff | — | −0,6 % / +1,8 % (Rauschen, §5) |

---

## 1. Was gebaut wurde

Vier neue Module unter `lib/browser/` und ein Werkzeugordner `tools/html/`:

```
lib/browser/knoten.fi       DOM-Kern: Knotenarten, Baum, Attribute, Ketten
lib/browser/tag.fi          erzeugt — die feste Namenstabelle (124 Namen)
lib/browser/namen.fi        Atomtabelle (Element-/Attributnamen -> u32)
lib/browser/tokenstrom.fi   Leser des binären Tokenprotokolls
lib/browser/baum.fi         die Baumkonstruktion (22 Einfügemodi)
lib/browser/schreiben.fi    Ausgabe im html5lib-Format
lib/browser/treiber.fi      Tokenizer + Baumaufbau zu einem Weg verbunden
lib/browser/parse_main.fi   Treiberprogramm (stdin -> stdout)
lib/browser/soak_baum.fi    Dauerlauf mit Gegenprobe
```

Am Tokenizer (`lib/html/`) wurde **additiv** ergänzt, was der Baumaufbau
braucht — begründet in §5.

---

## 2. Die Datenstrukturen

### 2.1 Knoten (`lib/browser/knoten.fi`)

```firn
gc class Knoten {
    eltern: Gc[Knoten], erstes_kind: Gc[Knoten], letztes_kind: Gc[Knoten],
    vorheriges: Gc[Knoten], naechstes: Gc[Knoten],
    art: u32, kinder: u32,
}
gc class Element   extends Knoten { erstes_attr, letztes_attr, name, ns, nattr }
gc class Text      extends Knoten { daten: Gc[Kette] }
gc class Kommentar extends Knoten { daten: Gc[Kette] }
gc class Doctype   extends Knoten { name, pubid, sysid, hat_pubid, hat_sysid }
gc class Dokument  extends Knoten { modus }        // quirks / limited / no
gc class Fragment  extends Knoten { wirt }
gc class Attribut  { naechstes: Gc[Attribut], wert: Gc[Kette], name: u32, ns: u32 }
```

**Alle Baumverweise sind STARK, in beide Richtungen.** Ein Baum aus *n* Knoten
hat damit rund 2*n* Zyklen. Das ist Absicht und der Kern der Sache: genau diese
Struktur ist der Grund, warum Firn einen Tracing-Sammler hat (SPEC §3.5.1). Ein
Zählverweis gibt hier **nichts** frei. Der Nachweis steht in §4.

`Element extends Knoten` nutzt das Präfixlayout (SPEC §4.4): eine
`Gc[Element]` ist kostenlos eine `Gc[Knoten]`, abwärts nur mit `.as?[Element]`.

### 2.2 Namen sind Atome

Element- und Attributnamen sind `u32`-Kennungen in eine Tabelle
(`lib/browser/namen.fi`, SPEC §8.3 `Z4`). Der Grund ist nicht Sparsamkeit,
sondern Machbarkeit: die WHATWG-Baumkonstruktion besteht zu großen Teilen aus
Fragen der Form *„ist der Name des aktuellen Knotens einer aus dieser Liste von
40 Namen"*. Mit Zeichenketten wäre jede davon ein Haufen `memcmp`.

Die **124 Namen, die der Standard beim Namen nennt**, haben eine feste Kennung
(`lib/browser/tag.fi`, erzeugt von `tools/html/gen_namen.py`) — `tag.M_DIV`
ist damit eine Übersetzungszeitkonstante. Alles darüber hinaus (eigene
Elementnamen, beliebige Attributnamen) bekommt beim ersten Auftreten eine
Kennung > 124. Die Ablage ist WTF-8, nicht UTF-8: ein Tagname darf ungepaarte
Surrogate enthalten (SPEC §8.2).

Die Tabelle ist vollständig `#[no_gc]` und darf deshalb auch aus heißen Pfaden
gerufen werden.

### 2.3 Text ist eine GC-Kette

```firn
gc class Strang { naechster: Gc[Strang], len: u32, zeichen: [u32; 28] }
gc class Kette  { erstes: Gc[Strang], letztes: Gc[Strang], laenge: u64 }
```

Textknoten, Kommentare, Attributwerte und die DOCTYPE-Kennungen liegen als
Kette aus GC-Stücken vor. **Damit lebt jedes Byte des Dokuments im GC-Heap.**
Das ist eine bewusste Entscheidung gegen die naheliegende Abkürzung (eine Arena
je Dokument, Text als Offset/Länge): hält später eine JS-Referenz einen
einzelnen Textknoten fest, bleibt genau dieser Text am Leben und sonst nichts —
und nichts zeigt ins Leere, wenn das Dokument abgeräumt wird. Eine Arena hätte
genau dort weh getan, wo dieses Projekt hin will.

Der Preis ist benannt: ein Zeichen kostet 4 Byte (Codepunkt), ein Stück fasst
28 davon. 28 × 4 + 8 (`naechster`) + 4 (`len`) = 124 → aufgerundet 128, **genau
eine Größenklasse des Sammlers** (`lib/gc/gc.fi`, `__gc_klassen_bytes`), also
kein Verschnitt. Anhängen ist O(1) (deshalb `letztes`), Indizieren O(n/28) —
wer eine Kette zeichenweise durchläuft, nimmt `kette_nach_cpbuf`.

### 2.4 Der Zustand des Baumaufbaus

```firn
struct Baum {
    namen, strom,                       // Umgebung
    dok: Gc[Dokument], kopf: Gc[Element], form: Gc[Element],
    offen: [Gc[Knoten]; 512], noffen,   // Stapel offener Elemente
    aktiv: [Gc[Knoten]; 128], naktiv,   // aktive Formatierungselemente
    modus, urmodus, frameset_ok, foster,
    tt: Gc[Kette], tt_nur_space,        // "in table text"
    umschaltung, um_pos, um_tag,        // Rückkopplung auf den Tokenizer
    tname, fehler, kaputt,
}
```

**Ein `Baum` MUSS im Rahmen des Aufrufers liegen** (als `var`, nicht auf der
Halde). Der Stapelscan des Sammlers ist konservativ (SPEC §3.5.3): auf dem
Stapel findet er die `Gc[…]`-Felder ohne jede Anmeldung, auf `mem.heap_alloc`
nicht — dort müsste `gc_wurzel_anmelden` (Runde 47) benutzt werden. Das steht
so im Kopf jeder betroffenen Datei, weil es die eine Regel ist, die man beim
Weiterbauen nicht übersehen darf.

Der Stapel ist ein **Feld fester Größe**, weil Firn noch kein `GcVec[T]` hat
(§6). 512 Ebenen bzw. 128 Formatierungseinträge; ein Überlauf ist ein
**sichtbarer Abbruch** (`kaputt != 0`, im Treiber `#KAPUTT 2`), keine stille
Kürzung. Zum Vergleich: die realen Seiten in `testdata/realweb/` kommen auf
höchstens 32 Ebenen.

---

## 3. Die Baumkonstruktion

### 3.1 Umgesetzte Einfügemodi — 22 von 23

| Modus | umgesetzt | Anmerkung |
|---|---|---|
| „initial" | ✔ | DOCTYPE, Quirks-Erkennung (eingeschränkt, §8) |
| „before html" | ✔ | |
| „before head" | ✔ | |
| „in head" | ✔ | inkl. Rohtext-/RCDATA-Weg |
| „in head noscript" | ✔ | Skriptflagge ist **aus** |
| „after head" | ✔ | inkl. `head` zurück auf den Stapel |
| „in body" | ✔ | vollständig, inkl. Adoption Agency |
| „text" | ✔ | |
| „in table" | ✔ | inkl. foster parenting |
| „in table text" | ✔ | |
| „in caption" | ✔ | |
| „in column group" | ✔ | |
| „in table body" | ✔ | |
| „in row" | ✔ | |
| „in cell" | ✔ | |
| „in select" | ✔ | |
| „in select in table" | ✔ | |
| „after body" | ✔ | |
| „in frameset" | ✔ | |
| „after frameset" | ✔ | |
| „after after body" | ✔ | |
| „after after frameset" | ✔ | |
| **„in template"** | **✘** | siehe §8 |

Dazu vollständig umgesetzt:

* **Stapel offener Elemente** mit allen vier Geltungsbereichen (*scope*,
  *list item scope*, *button scope*, *table scope*) und dem eigenen Weg für
  *select scope*.
* **Liste der aktiven Formatierungselemente** mit Marken, **Noahs Arche**
  (höchstens drei gleiche Einträge nach der letzten Marke) und
  *reconstruct the active formatting elements*.
* **Das Adoption-Agency-Verfahren**, beide Schleifen, mit Lesezeichen. Die
  äußere Schleife läuft wirklich bis zu achtmal — dass sie das tut, ist der
  Unterschied zwischen `<b><p>a</b>b` richtig und falsch (Fall 1 in
  `03_formatierung.dat`, beim ersten Anlauf falsch gehabt).
* **Implizites Schließen**, auch die gründliche Fassung.
* **Foster parenting** mit der vollständigen Fundstelle
  (*appropriate place for inserting a node*, inkl. Überschreibungsziel).
* **„reset the insertion mode appropriately"**.
* Der generische **Rohtext-/RCDATA-Weg mit Rückkopplung auf den Tokenizer**
  (§5).
* Kategorien **Special** und **Formatting** wörtlich nach Standard.

### 3.2 Ein Fehler, der zweimal derselbe war

Die Endtag-Gruppe *„address, article, …"* in „in body" ist **nicht** dieselbe
wie die gleichnamige Starttag-Gruppe: `p` hat eine eigene Regel (es wird
notfalls erzeugt), dafür gehören `button`, `listing` und `pre` dazu. Beim
ersten Anlauf war es dieselbe Funktion — Folge: `</p>` rief
`implizit_schliessen` (das `p` selbst poppt) und danach *„pop until p popped"*,
und das räumte, weil kein `p` mehr da war, **den ganzen Stapel leer**. Beides
ist repariert; `stapel_pop_bis_name` prüft seither vorher, ob der Name
überhaupt im Stapel steht, damit ein solcher Denkfehler nicht mehr als leerer
Baum, sondern an der Aufrufstelle auffällt.

---

## 4. Der GC-Nachweis

Ein DOM-Baum ist genau die Zyklenart, an der ein Zählverweis scheitert.
`tools/html/gc_baum.sh` fährt `lib/browser/soak_baum.fi`: in einer Schleife
wird aus einem echten HTML-Stück (446 Byte, mit DOCTYPE, Rohtext, Tabelle,
verschachtelter Formatierung, Attributen, Kommentar, Auswahlliste) ein
vollständiger Baum gebaut, geprüft und wieder losgelassen.

```
t_ms    runden  knoten  rss_kib  lebende  laeufe  heap_bytes  pause_max_ns
85      500     62      2136     551      5       1572864     212702
…
20000 Runden
# angelegt=2560000 lebende=624 leck=0
RSS erste Stichprobe: 2136 KiB, letzte: 2136 KiB, Zuwachs: 0 KiB
```

**20 000 Runden, 2 560 000 angelegte GC-Objekte, RSS konstant 2136 KiB
(Zuwachs 0 KiB), am Ende 624 lebende Objekte.** Der Heap steht bei 1,5 MB.

Damit die Messung nicht bloß hübsch aussieht, läuft **jedes Mal die
Gegenprobe** mit: derselbe Code, aber jeder Baum wird in einer GC-Kette
festgehalten.

```
112     500     62      7000     44718    3       6553600     395734
…
RSS erste Stichprobe: 7000 KiB, letzte: 48756 KiB, Zuwachs: 41756 KiB
```

**+41 756 KiB in 4000 Runden.** Bleibt die Gegenprobe flach, bricht `gc_baum.sh`
ab — eine Messung, die ein Leck nicht anzeigen kann, wäre schlimmer als keine.

Zwei Dinge waren dafür nötig und stehen im Code:

* `baum_loslassen` nullt am Rundenende alle `Gc[…]`-Felder des Zustands.
  Ohne das hält der Zustand den letzten Baum fest.
* `scrub_tief` legt einen tiefen, genullten Rahmen über die aufgegebenen
  Stapelrahmen. Der Stapelscan ist konservativ; ein Altzeiger in einem toten
  Rahmen hielte sonst einen ganzen Baum am Leben. Das ist der ehrliche Preis
  des konservativen Scans, hier bezahlt statt verschwiegen (dieselbe Bauart
  wie in `lib/dom/dom.fi` seit Runde 4).

Ein **echtes Leck** hat der Dauerlauf dabei gefunden, und zwar keines im
Sammler: `baum_init` rief `mem.cp_init` auf zwei Arbeitspuffer, und `cp_init`
nullt den Zeiger, **ohne freizugeben**. Jedes weitere Dokument verlor die
Puffer des vorigen — gemessen **8 KiB je Runde bei völlig flachem GC-Heap**.
Seit der Trennung in `baum_puffer_init` (einmal) und `baum_init` (je Dokument)
ist der Zuwachs 0. Der Fall steht hier, weil er zeigt, was der Sammler *nicht*
tut: manueller Speicher bleibt manuell.

Die Struktur derselben Aussage prüft `tests/901_dom_baum_gc.fi` ohne Werkzeuge:
ein Baum aus 4680 Elementen, Rückverweise geprüft, überlebt einen Sammellauf
solange er erreichbar ist, und ist danach fort.

---

## 5. Der Eingriff am Tokenizer — was, warum, und was er kostet

Der Tokenizer ist vollständig `#[no_gc]` (SPEC §3.5.4). Der Baumaufbau legt
GC-Objekte an. **Der Tokenizer kann den Baumaufbau also nicht aufrufen** — die
Zusage gilt transitiv, und der Compiler setzt sie durch. Zwischen beiden muss
etwas stehen, das keine GC-Zeiger kennt.

Dazu kommt die Rückkopplung: bei `<title>`, `<style>`, `<script>`,
`<textarea>` und `<plaintext>` schaltet der **Baumaufbau** den Tokenizer in
einen anderen Startzustand (WHATWG *generic raw text element parsing
algorithm*). Ohne das wäre `<title>a<b>c</title>` falsch.

Umgesetzt wurde beides mit zwei **additiven** Ergänzungen in `lib/html/`:

1. **Binäres Tokenprotokoll** (`tokens.fi`, `tb_*`): im Betrieb
   `sink_set_baum_modus(true)` schreibt der Sink statt html5lib-JSON kompakte
   Datensätze. Der JSON-Weg und der Messbetrieb sehen davon nur einen Zweig,
   der bei ihnen nie genommen wird.
2. **Quellposition je Token**: `tok_emit(s)` wurde an allen 52 Stellen in
   `tokenizer.fi` zu `tok_emit_bei(s, pos)`. Damit trägt jeder Datensatz die
   Position unmittelbar hinter dem Token — die braucht der Treiber zum
   Umschalten. **In der heißen Schleife steht dadurch kein einziges
   zusätzliches Wort**; der Wert wird nur beim Ausgeben eines Tokens in einem
   Register übergeben.

`lib/browser/treiber.fi` tokenisiert danach den **Rest** der Eingabe ab der
gemerkten Position erneut, mit dem neuen Startzustand. Der Preis ist ehrlich:
**O(k·n) statt O(n)**, wobei *k* die Zahl der Umschaltungen ist. Auf den acht
echten Seiten ist das tragbar; der saubere Weg wäre ein **fortsetzbarer
Tokenizer**, und der gehört in eine eigene Runde, weil er die 6810/6810
anfasst.

**Nachgemessen, dass nichts kaputtging und nichts langsamer wurde:**

| | vorher (`cc1710f`) | nachher |
|---|---|---|
| html5lib-Tokenizer-Suite | 6810 / 6810 | **6810 / 6810** |
| dieselbe mit Fehlercodes | 6809 / 6810 | **6809 / 6810** |
| Durchsatz Korpus „html5lib" | 11,43 MB/s | 11,37 MB/s (**−0,6 %**) |
| Durchsatz Korpus „realweb" | 28,32 MB/s | 28,82 MB/s (**+1,8 %**) |

Beide Durchsatzwerte sind der beste aus sieben Läufen, direkt gegeneinander
auf derselben Maschine gemessen (Binary „vorher" aus `git show HEAD:` gebaut).
Die Vorzeichen sind gegenläufig — das ist Rauschen, nicht Wirkung. Die
absoluten MB/s liegen niedriger als in `docs/RUNDE40.md`, weil auf dieser
Maschine gleichzeitig vier andere Runden liefen; für ein A/B ist das ohne
Belang, für einen Vergleich mit html5ever wäre es einer.

---

## 6. Gebrauchte Sprachfeatures — was Firn noch nicht kann

Alles hier ist beim Bauen dieser Runde wirklich aufgetreten. Nichts davon
wurde am Compiler geändert (fremdes Revier); die Umwege stehen im Code.

### 6.1 `Gc[modul.Typ]` lässt sich nicht schreiben

```firn
fn nimm(e: Gc[knoten.Element]) -> u32 { … }
//              ^ error: erwartet ']' nach dem typargument, gefunden '.'
```

Die Position eines Typarguments nimmt keinen qualifizierten Namen. Dass es
trotzdem geht, liegt an einer zweiten Eigenheit: **`gc class`-Namen liegen in
einem globalen Namensraum**, nicht im Modulnamensraum. `Gc[Element]` ist
deshalb aus jedem Modul erreichbar — auch aus einem, das `knoten` gar nicht
importiert. Beides zusammen ist tragbar, aber keins von beidem ist Absicht:
sobald zwei Bibliotheken eine `gc class Element` haben, kollidieren sie
still.

*Gebraucht:* qualifizierte Namen in Typargumenten, und `gc class` im
Modulnamensraum.

### 6.2 Keine wachsenden Sammlungen über `Gc[T]`

`SPEC §3.5.2` zeigt `GcVec[Gc[Node]]` und `GcMap[Atom, Str]` — die gibt es
nicht. `Vec[T]`/`Map[K,V]` (`lib/rt/`) liegen auf der manuellen Halde, und die
sieht der Sammler nicht; ein `Gc[T]` darin wäre unsichtbar und sein Ziel würde
eingesammelt. (Seit Runde 47 ließe sich das mit `gc_wurzel_anmelden` von Hand
reparieren, dann aber bei jedem Umkopieren neu.)

Folge in dieser Runde: der Stapel offener Elemente und die Liste der aktiven
Formatierungselemente sind **Felder fester Größe im Zustand** (512/128), weil
sie so auf dem Stapel liegen und konservativ gefunden werden. Das ist die
größte einzelne Lücke, die diese Runde gespürt hat.

*Gebraucht:* `GcVec[T]` — ein wachsendes Feld, dessen Inhalt der Sammler
präzise verfolgt.

### 6.3 Konstanten anderer Module in einem `const`

```firn
const A_DIV: u32 = namen_tab.M_DIV
//                 ^ error: unbekannter name 'namen_tab__M_DIV'
```

Ein `const` kann nicht aus einem anderen Modul initialisiert werden. Folge:
`baum.fi` schreibt an rund 600 Stellen `tag.M_DIV` statt eines kurzen Alias.
(Deshalb heißt das Modul `tag` und nicht `namen_tab`.)

*Gebraucht:* qualifizierte Namen in konstanten Ausdrücken.

### 6.4 Zeichenkettenliterale brauchen ihre Länge ausgeschrieben

```firn
var t: [u8; 446] = "<!DOCTYPE html>…"   // 446 muss stimmen, sonst Fehler
let a = "abc"                            // error: typ nicht ableitbar
var a: [u8; _] = "abc"                   // error: erwartet ganzzahlige laenge
```

Jedes Literal ist ein Array-Literal mit fester Länge, und die zählt der Mensch.
In dieser Runde wurden die Längen deshalb von `tools/html/gen_namen.py` und vom
Erzeuger für `tests/902_baum_konstruktion.fi` ausgerechnet — was für erzeugte
Dateien in Ordnung ist, für handgeschriebenen Code aber eine Fehlerquelle
bleibt (dreimal darauf hereingefallen).

*Gebraucht:* `[u8; _]` bzw. Längenableitung für Literale.

### 6.5 Kein Zeilenumbruch vor einem Operator

```firn
cp = ((c0 & 0x07) << 18)
    | ((b1 & 0x3F) << 12)      // error: erwartet einen ausdruck, gefunden '|'
```

Das Semikolon ist optional, also endet die Anweisung am Zeilenende. Ein
Ausdruck über mehrere Zeilen muss den Operator ans **Zeilenende** setzen. Das
ist eine Stilfrage, aber es kostet beim Portieren fremder Formeln Zeit.

### 6.6 Keine Tupel / kein Mehrfachrückgabewert

*„Appropriate place for inserting a node"* liefert im Standard zwei Werte
(Elternknoten und Referenzknoten). In Firn geht das nur über zwei
Ausgabezeiger:

```firn
fn passende_stelle(b: *mut Baum, ueberschreibung: Gc[Knoten],
                   eltern_aus: *mut Gc[Knoten], ref_aus: *mut Gc[Knoten])
```

### 6.7 Keine Zerstörer — und das ist kein theoretisches Problem

`mem.cp_init` nullt, `mem.cp_free` gibt frei; wer sie verwechselt, leckt. Genau
das ist passiert (§4) und hat 8 KiB je Dokument gekostet, unbemerkt bei völlig
flachem GC-Heap. Ein Zerstörer oder ein `defer` an der richtigen Stelle hätte
es verhindert; `defer` gibt es (Runde 9), nur nutzt es nichts bei einem
Zustand, der zwischen Dokumenten *lebt*.

### 6.8 `lib/firnc1` erkennt GC-Gebrauch ohne eigene `gc class` nicht

Gefunden, weil diese Runde den Fall zum ersten Mal erzeugt hat: eine Datei, die
GC-Typen BENUTZT, aber selbst keine `gc class` deklariert (sie kommen aus einem
Modul). `lib/firnc1/gc.fi::gc_quelle_scan` sucht am Tokenstrom nach `gc class`,
`error AllocError` und `fn __gc_finalisiere` — keins davon steht dann in der
Datei. Folge: `bin/astdump.fi` meldet für `gc_null[Kette]()` einen
**Syntaxfehler (rc=1)** statt „nicht Kernsprache" (rc=3), und
`tools/parser_vergleich.sh` zählt eine unerwartete Abweichung.

Nicht repariert — `lib/firnc1` ist in dieser Runde fremdes Revier. Im Test
umgangen: `tests/900_dom_kern.fi` hat eine Funktion mit einem `Gc[…]` in der
**Signatur**, und daran erkennt der Parser den Fall bereits. Für die
Compiler-Runden: `gc_quelle_scan` sollte auch `Gc[`/`GcWeak[`/`gc_null[` im
Tokenstrom sehen.

### 6.9 Kein fortsetzbarer Aufruf (Koroutine/Generator)

Die eigentliche Ursache für den Umweg in §5: `tokenize()` ist eine Schleife,
die einmal durchläuft. Ein `yield` je Token — oder ganz allgemein eine
Funktion, die anhält und weiterläuft — würde den Tokenizer zur Quelle machen,
aus der der Baumaufbau zieht, und die Rückkopplung wäre ein Feldzugriff statt
einer erneuten Tokenisierung.

---

## 7. Zahlen

### 7.1 Baumkonstruktion

`tools/html/faelle/*.dat` — **150 Fälle, alle bestanden**:

| Datei | bestanden | gesamt |
|---|---|---|
| `01_grundgeruest.dat` | 27 | 27 |
| `02_in_body.dat` | 31 | 31 |
| `03_formatierung.dat` | 20 | 20 |
| `04_tabellen.dat` | 25 | 25 |
| `05_rohtext.dat` | 24 | 24 |
| `06_auswahl_rahmen.dat` | 23 | 23 |
| **gesamt** | **150** | **150** |

Dieselbe Quote in allen drei Baustufen (`opt`, `--no-opt`, `dev-fast`).

### 7.2 Woher die Testdaten kommen — und wie ehrlich sie sind

**Die `tree-construction`-Daten von html5lib liegen diesem Projekt nicht vor.**
`testdata/html5lib-tokenizer/` enthält nur den Tokenizer-Teil; im Repository
gibt es keine `.dat`-Datei, und der festgeschriebene Upstream-Commit
`224991ec…` hat auch keinen Ordner `tree-construction` (nachgesehen über die
GitHub-API: `encoding/`, `serializer/`, `tokenizer/`, `lint_lib/` — mehr nicht,
auf keinem Branch).

Also wurden **150 eigene Fälle von Hand aus dem WHATWG-Standard geschrieben** —
Eingabe *und* erwarteter Baum, je Fall aus der Regel abgeleitet, die er prüfen
soll. Das Format ist trotzdem **genau das `.dat`-Format von html5lib**: liegen
die Originaldaten eines Tages vor, läuft `tools/html/harness_baum.py` ohne
Änderung dagegen.

Von Hand heißt auch: fehleranfällig. Deshalb gibt es `tools/html/orakel.py` —
es fährt **jede Erwartung gegen html5lib 1.1** (aus PyPI, in einer eigenen venv,
kein Teil des Projekts) und meldet jede Abweichung. Ergebnis:

* **149 von 150 Erwartungen stimmen mit html5lib überein.**
* Der eine Rest ist `<ruby><rb>a<rt>b</ruby>`: der aktuelle Standard nimmt `rb`
  in die *implied end tags* auf, html5lib 1.1 folgt der älteren Fassung. Hier
  ist der Standard maßgeblich; der Fall trägt `#orakel-abweichung`.
* Beim ersten Durchgang hat das Orakel **acht eigene Denkfehler** in den
  Erwartungen gefunden (u. a. `</p>` in „after head", Leerraum in „before
  head", `<body></body><frameset>`). Die Fälle stehen jetzt richtig da.

Das Orakel ist eine **Prüfung** der Erwartungen, keine Erzeugung: es steht in
keiner Testkette und wird nur von Hand gefahren (`tools/html/run.sh` braucht es
nicht und kommt ohne Netz aus).

### 7.3 Echte Seiten

`tools/html/realweb.py` fährt die acht unveränderten Seiten aus
`testdata/realweb/` (Hacker News, rustdoc, W3C, WHATWG, vier Wikipedia-Seiten;
0,03 bis 1,0 MB). Kein `#KAPUTT`, kein Abbruch, und alle drei Baustufen liefern
denselben Baum Byte für Byte.

Zusätzlich von Hand gegen html5lib geprüft: **alle acht Bäume sind Zeile für
Zeile identisch** mit dem, was html5lib 1.1 baut — bis zu 61 325 Ausgabezeilen
je Seite. Ehrlich dazugesagt: keine dieser Seiten enthält `<svg>`, `<math>`
oder `<template>`, die drei großen Lücken werden davon also nicht berührt.

### 7.4 Bekannte Lücken

`tools/html/luecken/bekannte_luecken.dat` hält fest, was **nicht** geht, mit
den **richtigen** erwarteten Bäumen. `harness_baum.py --luecken` fährt sie
getrennt aus: **0 von 10 bestanden** — wie erwartet. Eine Testsuite, die nur
enthält, was schon geht, sagt nichts über das, was fehlt.

### 7.5 Abnahme

Alles mit frisch gebauten Binaries (`rm -f .firnc1 .firnc2 .firnc3 .astdump …`
vor dem Lauf):

| Prüfung | Ergebnis |
|---|---|
| `bash ./test.sh` | **PASS 761/761** (Basis 751/751; +9 durch drei neue Programme × drei Baustufen, +1 durch Abschnitt 9b) |
| `bash tools/selbst_vergleich.sh` | **GLEICHES VERHALTEN 216, ABWEICHEND 0, FEHLERHAFT 0** (Basis 213/0/0) |
| `bash tools/html/run.sh` | 150/150 in drei Baustufen, 8/8 Seiten byte-gleich, Dauerlauf bestanden |
| `bash tools/tokenizer/run.sh` | 6810/6810 bzw. 6809/6810 mit Fehlercodes — unverändert |
| `tools/fixpunkt.sh` (in `test.sh`) | Stufe 2 == Stufe 3, zeichengleich |

`tools/html/orakel.py` getrennt gefahren: `faelle/` 150 geprüft, 0 unerwartete
Abweichungen (1 vermerkte); `luecken/` 10 geprüft, 0 unerwartete Abweichungen
(4 vermerkte — html5lib 1.1 legt Template-Inhalte nicht in einem eigenen
Inhaltsbaum ab und kann die Erwartung dort nicht bestätigen; sie ist von Hand
aus dem Standard geschrieben).

Der Compiler in Firn (`.firnc1`) übersetzt die drei neuen Tests
`900_dom_kern`, `901_dom_baum_gc` und `902_baum_konstruktion` selbst, und die
Ergebnisse verhalten sich wie die von `firnc0` — der Baumaufbau ist damit auch
für Stufe 1 Kernsprache.

---

## 8. Offen

**Nicht umgesetzt, mit Absicht und hier benannt statt versteckt:**

1. **„in template" und der `<template>`-Inhalt.** Der 23. Einfügemodus braucht
   einen zweiten Baum je Template plus einen eigenen Stapel von Einfügemodi.
   `<template>` wird derzeit wie ein gewöhnliches Element behandelt.
   (`bekannte_luecken.dat` Fälle 6–9.)
2. **Fremdinhalt (SVG/MathML) als Regelsatz.** Namensräume gibt es — `<svg>`
   und `<math>` erzeugen Elemente im richtigen Namensraum, und der Namensraum
   steht am Knoten und am Attribut. Die Sonderregeln für den *Inhalt* fehlen:
   Namenskorrektur (`clipPath`, `foreignObject`), Attributanpassung
   (`xlink:href`), Integrationspunkte, Ausbruchtags.
   (`bekannte_luecken.dat` Fälle 1–5.)
3. **Fragmentzerlegung** (`innerHTML`) mit Kontextelement.
   (`bekannte_luecken.dat` Fall 10.)
4. **Quirks-Erkennung** kennt nur die „force quirks"-Flagge, einen Namen
   != `html` und vier öffentliche Kennungen; die vollständige Liste des
   Standards (rund 55 Präfixe aus der Zeit vor HTML5) fehlt. Sichtbar wird das
   nur bei `<table>` in „in body" (dort hängt das Schließen von `<p>` am
   Modus).
5. **Parse-Fehler** werden gezählt, aber nicht gegen eine Erwartung geprüft,
   und die Zählung ist unvollständig (die Quittierung des
   `self-closing`-Zeichens fehlt). Der Tokenizer prüft seine Fehlercodes
   dagegen exakt (6809/6810).
6. **Skriptausführung** und alles daran (`document.write`, die *scripting
   flag*) — die Flagge ist aus.
7. **Der fortsetzbare Tokenizer** (§5). Solange er fehlt, ist der Zerleger
   O(k·n) statt O(n).

**Nicht angefasst, mit Absicht:** `compiler/src`, `lib/firnc1` und `lib/gc`
(fremdes Revier — parallel liefen vier Compiler-Runden). Auch `SELBSTHOSTING.md`
bleibt unverändert; der Absatz zu dieser Runde gehört in den Merge, nicht in
den Branch.

**Was als Nächstes sinnvoll ist:** `GcVec[T]` (§6.2) räumt gleichzeitig den
Stapel, die Formatierungsliste und den Weg zu `<template>` frei; danach der
fortsetzbare Tokenizer, weil er den einzigen strukturellen Kostenpunkt dieser
Runde beseitigt. Fremdinhalt ist viel Tabellenarbeit und wenig Struktur — das
kann warten, bis eine Seite es braucht.
