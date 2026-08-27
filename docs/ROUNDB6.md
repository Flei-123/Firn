# Runde B6 — Certus: das Layout wird erwachsen, und der Fingerabdruck wird stumpf

Runde B5 hat `https://` möglich gemacht und damit die letzte Zeile
aufgehoben, in der der Browser eine Adresszeile hatte, die log. Was danach
übrig blieb, war die **schwächste Zahl der ganzen Kette**: das Layout
bestand 59 von 186 Tests der offiziellen Web Platform Tests — 31,72 %,
während ein echtes Chromium auf demselben Korpus durch dieselbe Messung
138 von 186 schafft. Eine Seite, deren Rechtecke an jeder dritten Stelle
falsch stehen, nützt kein TLS.

Diese Runde ist deshalb zuerst Layout. Sie ist außerdem die Runde, in der
der Browser seinen **Namen** bekommen hat (27.08.2026: **Certus**, von
AE2s „Certus Quartz" und von lateinisch *certus* — sicher, verlässlich,
derselbe Wortstamm wie *Zertifikat*, worauf B5 eine ganze Runde verwendet
hat), und die Runde, in der aus Kapitel Z der Anforderungen zum ersten Mal
Code geworden ist.

---

## 1. Die Zahlen

Jede Zahl unten stammt aus einem Lauf, der wirklich gefahren wurde
(AMD EPYC 7571, Linux x86_64, 27.08.2026). Wo eine Zahl aus einer
**früheren** Runde übernommen ist, steht das dabei — sie ist dann nicht in
dieser Runde gemessen worden und behauptet auch nicht, es zu sein.

### 1.1 Das Layout gegen die offiziellen Web Platform Tests

Der Korpus ist unverändert der von Runde B2: jede Datei der
WPT-Verzeichnisse `css/css-flexbox`, `css/CSS2`, `css/css-box`,
`css/css-sizing`, `css/css-position` und `css/css-align`, die
`check-layout-th.js` einbindet, abzüglich der drei mechanisch
abgetrennten Gruppen (`script`, `grid`, `vertical`). Nichts daran wurde
für diese Runde angefasst — hätte ich den Korpus verändert, wäre der
Vergleich mit B2 wertlos.

| | Tests des Korpus B2 | Quote | Einzelprüfungen |
|---|---|---|---|
| vor dieser Runde (Runde B2/B5) | **59 / 186** | 31,72 % | 3535 / 4867 |
| **nach dieser Runde** | **97 / 186** | **52,15 %** | **3975 / 4867** |
| Chromium 141, dieselbe Messung | 138 / 186 | 74,19 % | — |

**Die dritte Zeile ist aus Runde B2 übernommen und in dieser Runde NICHT
neu gemessen worden.** `tools/layoutb2/chrome_check.py` startet ein echtes
Chromium und ist absichtlich nichts, was `test.sh` aufruft; die Zahl steht
hier als Maßstab, nicht als Ergebnis dieses Laufs.

Pro Verzeichnis, damit sichtbar bleibt, wo die 38 Tests herkommen und wo
der Motor weiter steht, wo er stand:

```
Verzeichnis                                    Tests    B2    B6   Chromium
css/css-flexbox                                   66    31    33     52
css/css-sizing/stretch                            20     7    12     20
css/css-box/margin-trim/computed-margin-values    19     5    14      0
css/css-flexbox/abspos                            18     5    13     18
css/css-sizing                                    16     5     9     16
css/css-box/margin-trim                            7     0     7      0
css/css-flexbox/alignment                          7     0     0      7
css/css-flexbox/intrinsic-size                     7     1     1      5
css/css-sizing/aspect-ratio                        7     0     3      4
css/CSS2/floats                                    4     2     2      4
css/css-align/blocks                               4     0     0      2
css/css-sizing/contain-intrinsic-size              3     0     0      3
css/CSS2/normal-flow                               2     2     2      2
css/css-align/baseline-rules                       2     0     0      2
css/CSS2/linebox                                   1     0     0      0
css/css-align/abspos                               1     0     0      1
css/css-flexbox/balance                            1     0     0      1
css/css-position                                   1     1     1      1
TOTAL                                            186    59    97    138
```

Zwei Zeilen darin verdienen einen Satz.

**`margin-trim`: 21 von 26, wo Chromium 0 von 26 hat.** Das ist kein
Kunststück, sondern der Grund, warum diese Gruppe so viel gebracht hat:
`margin-trim` ist eine Eigenschaft aus css-box-4, die in Chromium schlicht
nicht umgesetzt ist, und die Tests messen deshalb nur, ob man sie *hat*.
Sie sind trotzdem echte Tests der CSS-Arbeitsgruppe, und sie standen von
Anfang an im Korpus — herausnehmen hätte geheißen, sich die Quote
schönzurechnen.

**`css/css-flexbox/alignment`: weiterhin 0 von 7.** Diese Gruppe misst die
Ausrichtung an der **Grundlinie** über Tabellen, Mehrspaltensatz und
`line-clamp` hinweg. Nichts davon existiert hier, und ich habe es diese
Runde nicht angefangen, statt drei halbe Dinge zu haben.

Die drei getrennt gezählten Gruppen, die **nicht** Korpus B2 sind, laufen
mit und werden von demselben Lauf ausgegeben:

```
                         vorher                     nachher
vertical    0 / 171 Tests   3053 / 7984 Prüf.    0 / 171   3277 / 7984
grid        1 / 22  Tests     54 /  151 Prüf.    3 / 22      59 /  151
script      6 / 92  Tests    483 / 1874 Prüf.    6 / 92     555 / 1874
alles außer `script`   60 / 379 Tests           100 / 379 Tests
```

Bei `vertical` bleibt die Testzahl 0 und nur die Einzelprüfungen steigen:
die logischen Eigenschaften werden jetzt verstanden, die Schreibrichtung
aber weiter nicht — und genau deshalb bleibt keine dieser Dateien ganz.
Das ist der ehrliche Ausweis dafür, dass die Abbildung aus 2.1 eine
Abbildung für **einen** Fall ist.

### 1.2 Die drei Baustufen und der Umbruch

| | |
|---|---|
| `opt`, `--no-opt`, `dev-fast` auf demselben Korpus | **97 / 97 / 97** |
| Dokumente, deren Layout bei 800 → 400 → 800 identisch bleibt | **471 / 471** |

Die erste Zeile ist keine Formalie: eine Quote, die vom Optimierer
abhängt, ist eine Quote, die von der Maschine abhängt. Die zweite ist die
Zusage aus Runde B2 und sie ist unverändert.

### 1.3 Kapitel Z — die Fingerabdruck-Abwehr, gemessen

`tools/fpz/run.sh`, 16 Prüfungen, 500 Ursprünge, 500 Sitzungen:

| | |
|---|---|
| zwanzig Lesevorgänge, eine Sitzung, ein Ursprung | **Oktett für Oktett gleich** |
| 500 Ursprünge, eine Sitzung | **500 verschiedene** Leinwände |
| 500 Sitzungen, ein Ursprung | **500 verschiedene** Leinwände |
| **Gegenprobe: derselbe Weg OHNE Rauschen, 500 Ursprünge** | **1** (muss 1 sein) |
| größte Abweichung eines Farbkanals | **1** |
| Alphakanäle angefasst | **0** |
| Anteil der veränderten Bildpunkte | **3,11 %** (einer von 32) |
| 16 × 16-Leinwand, 500 Ursprünge | 500 verschiedene, **1 unberührt** |
| `navigator.userAgent` über 500 Ursprünge | **1** (eingefroren, nicht verrauscht) |
| `navigator.hardwareConcurrency` | 11 verschiedene Werte, 2 bis 12, je Ursprung stabil |
| `navigator.deviceMemory` | 4 verschiedene Werte (1, 2, 4, 8) |
| Uhr: 64 Lesungen desselben Zeitpunkts | 2 verschiedene, alle Vielfache von 100 µs |
| dieselben Zahlen in allen drei Baustufen | **ja** |

**Die vierte Zeile ist die, ohne die keine der anderen etwas wert wäre.**
„500 Ursprünge liefern 500 verschiedene Leinwände" gilt auch für ein
Programm, das reines Rauschen zurückgibt — und das wäre nicht besser,
sondern schlechter als gar nichts: ein Skript liest zweimal, sieht einen
Unterschied und weiß, dass es belogen wird. Erst die Gegenprobe, bei der
dasselbe Programm mit ausgebautem Rauschen über dieselben 500 Ursprünge
**genau eine** Antwort geben muss, trennt „die Abwehr wirkt" von „die
Messung wackelt".

**Die 1 unberührte 16 × 16-Leinwand ist der unangenehme Befund und bleibt
stehen.** Bei einem Bildpunkt von 32 und 256 Bildpunkten trifft es
statistisch etwa alle 3000 Ursprünge einen, bei dem gar nichts verändert
wird — der bekommt dieselben Oktette wie ein anderer solcher Ursprung.
Die Rate zu erhöhen würde das verschieben und das Bild körniger machen;
was hier steht, ist die gemessene Wirkung der Rate, die drinsteht, und
nicht die, die ich gern hätte.

### 1.4 Was nicht schlechter geworden ist

Alle Zahlen dieser Zeile stammen aus Läufen **dieser** Runde, nicht aus
den Logbüchern der früheren:

| | vor dieser Runde | danach |
|---|---|---|
| html5lib Baumbau (Runde B1) | 1837 / 1936 | **1837 / 1936** (94,89 %) |
| DOM- und Stilfälle (Runde B1) | 17 / 17 | **17 / 17** |
| eigene Layoutfälle gegen Chromium (Runden 61/67) | 1087 / 1087 | **1087 / 1087**, Abweichung **0,00 %** |
| Zeichenreihenfolge (Runde B3) | 5171 / 5171 | **5171 / 5171** |
| WPT-Referenzbilder, Korpus B3 | 202 / 541 | **202 / 541** (37,34 %), 32 leere abgezogen |
| Glyphen gegen den zweiten Rasterer (B3) | 393 | **393** |
| WPT-DOM, Korpus B4 | 390 / 1714 | **390 / 1714** (22,75 %), 169 konnten nicht laufen, 16 / 313 ganz |
| TLS-Bausteine (Runde B5) | 647 / 647 | **647 / 647**, 49 Gegenproben |
| Zertifikatsfälle (B5) | 26 / 26 | **26 / 26**, 14 Ablehnungen |
| Handschläge gegen OpenSSL und das Netz (B5) | 18 / 18 | **18 / 18**, 7 Ablehnungen |
| `https://` an der Naht (B5) | 14 / 14 | **14 / 14** |
| JPEG gegen libjpeg (B5) | 16 / 16 | **16 / 16** |
| `<img>` (B5) | 15 / 15 | **15 / 15** |
| das Fenster unter `xwd` (B5) | 10 / 10 | **10 / 10** |

Die letzte Zeile hat diese Runde noch etwas geleistet, das sie nicht
vorhatte: der Fenstertitel heißt jetzt `Certus`, und `xwd -name Certus`
findet das Fenster. Hätte ich den Titel nur im Quelltext geändert und die
Prüfung nicht mitgezogen, wäre der Lauf durchgefallen — die Messung hängt
an genau der Zeichenkette, die der X-Server bekommt.

---

## 2. Was gebaut wurde

### 2.1 Die logischen Eigenschaften — der größte einzelne Hebel

`margin-block-start`, `inline-size`, `inset-inline-end` und ihre 22
Geschwister sind in einem waagerechten, links-nach-rechts geschriebenen
Dokument **dieselben** Eigenschaften wie `margin-top`, `width` und
`right`. Der Motor kannte keine einzige davon: sie fielen in
`property_id` durch und die Deklaration verschwand.

Das betraf **33 der 127 fehlschlagenden Tests** — und zwar quer durch alle
Gruppen, weil die WPT-Tests der letzten Jahre durchweg logisch geschrieben
sind. Die Abbildung steht in `logical_property_id` und in
`logical_pair` für die Kurzformen (`margin-block: 10px 20px`, `inset: 0`).

**Sie wird in `property_id` selbst gemacht, und das ist eine Entscheidung
mit Folgen:** eine logische Langform **ist** ab da die physische. Sie
nimmt an derselben Stelle an der Kaskade teil, und
`margin-inline-start: 5px; margin-left: 9px` kommt als 9 heraus, weil die
spätere Deklaration gewinnt. Für gleichen Ursprung und gleiche Spezifität
sagt der Standard genau das.

**Und es ist eine Lüge, sobald jemand `writing-mode: vertical-rl` oder
`direction: rtl` schreibt.** Dann bezeichnen diese Namen andere Seiten.
Ehrlich ist es hier nur, weil der Korpus, gegen den gemessen wird, beides
ausschließt: `tools/layoutb2/harness.py` legt 171 solche Tests in die
Gruppe `vertical` und weist sie getrennt aus. Die Einzelprüfungen dort
sind von 3053 auf 3277 gestiegen und die Testzahl ist bei 0 geblieben —
das ist genau das Bild, das man erwartet, wenn die Eigenschaften
ankommen und die Richtung fehlt.

### 2.2 Die ersetzten Elemente, die keine Bilder sind

Runde B5 hat `<img>` eine intrinsische Größe gegeben, aus einem
dekodierten Bild oder aus den Maßattributen. Damit fehlte **jedes andere**
ersetzte Element von HTML, und die offizielle Testsammlung ist voll davon.
`lib/layout/build.fi` kennt jetzt:

* **`<canvas>`** — die Bitmap ist die natürliche Größe, die Attribute
  `width`/`height` mit den Vorgaben 300 und 150 (HTML 4.12.5);
* **`<video>`, `<object>`, `<embed>`, `<iframe>`** — keine natürliche
  Größe **und kein natürliches Seitenverhältnis**;
* **`<svg>`** — Größe aus `width`/`height`, Verhältnis aus `viewBox`.

**Der Unterschied zwischen „hat eine Größe" und „hat ein Verhältnis" ist
der ganze Punkt**, und er ist der Grund für das neue Feld `Box.ratio`.
`<video style="height: 100px">` ist **300** breit und nicht 200: es gibt
kein Verhältnis, durch das die Höhe wandern könnte, also fällt die Breite
auf die Vorgabegröße zurück. Ein Motor, der nur `iw`/`ih` speichert und
dividiert, kann diese beiden Fälle nicht auseinanderhalten —
`css/css-sizing/intrinsic-size-fallback-video` prüft vier davon.

`box.replaced_size` ist entsprechend von vier Fällen auf den vollen
Vorgabe-Algorithmus von css-images-3 5.1 gewachsen, einschließlich der
`contain`-Bedingung: ein 4:1-Bild ohne bekannte Größe wird 300 × 75 und
nicht 300 × 150.

Dazu ein Sonderfall, der als solcher gekennzeichnet ist: ein `<svg>` mit
`viewBox` und **ohne** `width`/`height` füllt die Inline-Achse
(SVG2 8.2: `auto` heißt dort `100%`). Das steht als Flag `F_FILL_INLINE`
auf der Box und nicht als Wert im Stil, weil es eine Regel von SVG ist und
keine von CSS — und es schlägt eine `width: max-content` des Autors, was
`css/css-sizing/svg-intrinsic-size-003` genau so misst.

### 2.3 `aspect-ratio`

Neu in der Kaskade, mit der Grammatik `auto || <ratio>`. `auto 2 / 1`
bevorzugt ein natürliches Verhältnis und fällt auf 2/1 zurück, ein bloßes
`2 / 1` gewinnt immer (css-sizing-4 4). Ein negativer oder Null-Wert wird
angenommen und verhält sich wie „keins" — das steht so im Standard und ist
sicherer, als später durch ihn zu teilen.

Angewendet wird es an zwei Stellen: in `box.replaced_size` für ersetzte
Elemente und in `layout_content_report` für alle anderen. Die zweite
Stelle achtet auf `box-sizing`: mit `border-box` beschreibt das Verhältnis
den **Rahmenkasten**, also müssen die Ringe hinterher wieder abgezogen
werden.

### 2.4 `margin-trim`

Auch neu in der Kaskade, als Bitmenge. Umgesetzt für zwei Sorten
Container, und die Regeln sind verschieden genug, dass sie getrennt
stehen:

**Im Flex-Container** ist ein Rand getrimmt, wenn er **an** der genannten
Kante liegt — und das sind zwei verschiedene Fragen je Achse: an der
Hauptachse das **erste (letzte) Element jeder Zeile**, an der Querachse
**jedes Element der ersten (letzten) Zeile**. Deshalb trimmt
`margin-trim: inline-start` an einer umbrechenden Zeile zwei Ränder und
`margin-trim: block-start` an derselben Box alle Elemente der ersten
Zeile.

**Im Blockcontainer** ist der getrimmte Rand am Ende kein Rand, sondern
ein **Lauf**: der untere Rand des letzten Kindes und mit ihm die ganze
Randmenge jedes selbstkollabierenden Kastens, der dahinter steht. Genau
diese Kästen sind die, deren Position von dieser Randmenge bestimmt wurde
— sie wandern also mit nach oben. Und wo das letzte Kind, das **nicht**
selbstkollabiert, eine offene Unterkante hat, geht der Lauf **in dieses
Kind hinein** (`trim_block_end` ruft sich selbst auf). Ein Rahmen an
diesem Kind schließt die Kante und beendet den Lauf — das ist der
Unterschied zwischen `…-offsets-nested-once` und
`…-last-child-with-border`, und beide Dateien messen ihn.

### 2.5 Ein echter Fehler, den `margin-trim` ans Licht gebracht hat

Die Bedingung für „die beiden Ränder dieses Kastens grenzen aneinander"
(CSS 2.1 8.3.1) stand im Motor als `all_through && used_h == 0 &&
v_extra == 0 && !bfc_root && !height_given`. Das letzte Glied ist falsch:
der Standard sagt „`height` ist 0 **oder** `auto`". Ein Kasten mit
`block-size: 0px` kollabierte deshalb **nicht** durch und stand in jedem
verschachtelten Fall 50 Bildpunkte tiefer als in jedem Browser.

Gefunden hat es die Trimm-Schleife, nicht das Auge: sie lief rückwärts
über die Kinder, hielt beim ersten nicht-durchkollabierenden an — und hielt
sofort an. Ersetzt ist es durch die vierte Bedingung des Standards, die
vorher gar nicht geprüft wurde: `min-height` muss null sein
(`min_height_zero`).

### 2.5a Und ein Fehler, den diese Runde selbst eingebaut hat

Das neue Feld `Box.ratio` (2.2) war an einer Stelle nicht gesetzt: in
`lib/browser/images.fi`, wo ein **dekodiertes Bild** an seine Box gehängt
wird. Vorher war das Verhältnis überall implizit `ih / iw`; jetzt ist es
ein eigenes Feld, und wer es nicht füllt, hat ein Bild ohne Verhältnis.

Was dabei herauskam: ein `<img width="80">` auf ein 40 × 30 großes Bild
wurde **80 × 30** statt 80 × 60. Also genau der Fehler, gegen den der
Kommentar in `box.replaced_size` seit Runde B5 anschreibt — „ein Browser,
der die Höhe bei `auto` auf null lässt, lässt jede bilderreiche Seite
umbrechen, wenn die Bilder ankommen".

**Gefunden hat es der Abnahmelauf von B5**, `tools/tlsb5/img_check.py`,
mit fünf von fünfzehn Fällen rot — Zeile für Zeile mit dem erwarteten und
dem bekommenen Maß. Nicht gefunden hätte es die Layout-Messung dieser
Runde: im WPT-Korpus gibt es kein einziges dekodiertes Bild, weil der
Prüfstand keine Bilder lädt. Das ist der Grund, warum die alten Läufe bei
jeder Runde vollständig mitlaufen und nicht nur die neue Zahl.

### 2.6 Die zweiwortigen Ausrichtungswerte

`align-self: last baseline` wurde als der Bezeichner `last` gelesen, traf
nichts und blieb still auf `auto` stehen. Acht Dateien in
`css/css-flexbox/abspos` messen genau diesen Wert. Jetzt gibt es
`value_align_pair` für `first baseline`, `last baseline` und die
Überlauf-Vorsätze `safe`/`unsafe`.

`last baseline` wird auf `flex-end` abgebildet und **nicht** auf `end`,
und das ist keine Kosmetik: `end` ist absolut und dreht sich mit
`flex-wrap: wrap-reverse` nicht mit, `flex-end` schon. Die Dateien 001 und
002 unterscheiden sich in genau diesem einen Wort und erwarten 5 und 1.

Dazu `left`/`right` (css-align-3 4.2): sie bedeuten nur etwas auf der
**Inline-Achse**; auf der Block-Achse verhalten sie sich wie `start`. Das
kann die Kaskade nicht entscheiden, weil sie die Achse nicht kennt — also
gibt es dafür jetzt eigene Werte `AL_LEFT`/`AL_RIGHT` und ein
`flow.align_phys`, das sie an der Stelle auflöst, an der die Achse
bekannt ist.

### 2.7 Kapitel Z — und warum es *in* den Bausteinen steht

`lib/browser/fpz.fi`. Der Anlass ist der AliExpress-Fall vom 24.08.2026:
ein WebAudio-Graph mit Sägezahn-Oszillator und `AnalyserNode` bei
Lautstärke **null**, aus dem ein Geräte-Fingerabdruck gelesen wurde.
Stummschalten half nicht — es gab kein Media-Element, an dem es hätte
greifen können. Die Messung war nie zum Hören gedacht.

Gebaut ist das, wofür die Bausteine da sind: die Rücklesewege der
Leinwand und die `navigator`-Felder. Das Verfahren ist Braves:

```
key = SHA-256("certus/fp/v1" | Sitzungsgeheimnis | Länge | Ursprung)
```

Die Länge steht **vor** dem Ursprung und nicht dahinter, damit
`https://a.example` und `https://a.example.x` nicht durch Verschieben der
Grenze denselben Schlüssel bekommen können. Das Sitzungsgeheimnis kommt
aus `getrandom(2)`; wenn der Kern keins gibt, ist das ein Abbruch und kein
Rückfall auf einen Zähler — ein vorhersagbares Geheimnis sieht aus wie
eine Abwehr und ist keine.

Aus dem Schlüssel läuft ein zählerbasierter Strom (SplitMix64), so dass
Bildpunkt 4711 gefragt werden kann, ohne die 4710 davor zu rechnen, und
zwei Läufe über dasselbe Bild dieselben Wörter an dieselben Stellen legen.
Getroffen wird das **niederwertigste Bit** eines der drei Farbkanäle bei
etwa jedem 32. Bildpunkt. **Der Alphakanal nie** — ein gekipptes
Alphabit ist an einem ganz durchsichtigen Bildpunkt sichtbar und ist der
eine Fall, den ein Compositor anders behandeln darf.

Der Benutzeragent wird **eingefroren statt verrauscht**, und das ist eine
andere Entscheidung als alles darüber. Ein je Seite gewürfelter
Benutzeragent ist selbst ein Merkmal — keine zwei Anfragen einer Person
stimmen überein —, und das halbe Netz liefert einem Browser, den es nicht
kennt, eine kaputte Seite. Eine Zeichenkette für jedes Certus auf jeder
Maschine ist das kleinere Übel: sie sagt nichts über **diese** Maschine.

**Verankert ist die Anforderung dort, wo der Fehler gemacht würde, und
nicht in einem Dokument daneben:** im Kopf von `lib/paint/canvas.fi` (dem
Besitzer der Bildpunkte) und im Kopf von `lib/browser/domjs.fi` (dem
Besitzer der Felder, die eine Seite zu sehen bekommt). Wer den nächsten
Rückleseweg baut, liest die Regel, bevor er die Funktion findet. Kapitel Z
verlangt außerdem **keinen Schalter** (Z6) — die API hat keinen, und es
gibt auch keinen „strengen" und „normalen" Modus: jeder Modus, in dem ein
Nutzer sein kann, ist selbst ein Bit.

### 2.8 Der Name

Fenstertitel und Anzeigename sind **Certus**
(`lib/browser/window_main.fi`), der Befehl im System bleibt
`/bin/browser`, der Benutzeragent sagt `Certus/1.0`, und
`tools/tlsb5/ui_check.py` sucht das Fenster mit `xwd -name Certus`. Die
Zeile in `orientos/ROADMAP.md` 6.3 ist nachgezogen. App-Name „Certus",
Store-Titel „Certus Browser" und Paket-ID `com.orientos.certus` stehen in
der Roadmap unter 7.3 und sind **nicht** gebaut — es gibt keine APK.

---

## 3. Was NICHT bewiesen ist

Dieser Abschnitt ist der Preis für die Zahlen oben.

1. **52,15 % sind nicht „das halbe Web".** Der Korpus sind 186
   selbstbeschreibende Layouttests aus sechs CSS-Modulen. Er enthält kein
   Grid (22 Tests, getrennt gezählt, 3 bestanden), keine Schreibrichtung
   (171 Tests, 0 bestanden), keine Tabellen als eigenes Modul, kein
   Mehrspaltenlayout, kein `float`-Modul jenseits von vier Dateien. Ein
   Motor mit 52 % hier kann eine echte Seite immer noch falsch stellen.

2. **Die logischen Eigenschaften sind für EINEN Schreibmodus richtig.**
   In `writing-mode: vertical-rl` oder `direction: rtl` bezeichnet
   `margin-block-start` eine andere Seite, und die Abbildung aus 2.1 wäre
   dann schlicht falsch. Es gibt keine Prüfung, die das verhindert — nur
   die Tatsache, dass die betroffenen Tests in einer eigenen Gruppe
   liegen und dort weiterhin 0 von 171 stehen.

3. **`last baseline` ist als `flex-end` umgesetzt.** Für die statische
   Position eines absolut positionierten Kindes ist das dasselbe. Für ein
   Element **im Fluss** ist es das nicht: eine echte letzte Grundlinie
   richtet die letzte Zeile jedes Elements aus. Kein Test im Korpus
   unterscheidet die beiden — deshalb steht es hier und nicht in den
   Zahlen.

4. **`margin-trim` im Flex-Container wird NACH dem Zeilenumbruch
   angewendet.** Der Umbruch sieht also noch die ungetrimmten Ränder. In
   den zwölf Dateien, die es misst, ändert das nichts; ein Fall, in dem
   ein getrimmter Rand die Zeilenaufteilung verschieben würde, ist nicht
   abgedeckt. Trimmen davor bräuchte die Zeilen, und die Zeilen brauchen
   die Ränder.

5. **`margin-trim` gilt hier für Block- und Flex-Container.** Grid ist
   nicht dabei; die acht `grid-*`-Dateien der Gruppe liegen im Korpus
   `grid` und sind nicht mitgezählt.

6. **Das `<svg>` wird nicht geparst.** Sein `viewBox` wird als vier Zahlen
   gelesen, `preserveAspectRatio` wird ignoriert, und nichts im Inneren
   wird gezeichnet. Von den sieben SVG-Tests im Korpus bestehen drei.

7. **`<img>` mit einer `data:`-URL wird nicht dekodiert.** Sieben Dateien
   in `css/css-flexbox` (`image-as-flexitem-size-00X`,
   `flex-aspect-ratio-img-*`) hängen daran und bleiben rot. Ein `<img>`
   ohne Bild gilt nur mit **beiden** Maßattributen als ersetzt — dieselbe
   Regel, die sich Runde B5 gegeben hat.

8. **Kapitel Z ist zu einem Drittel gebaut.** Z3 und Z4 gelten für
   Leinwand, `navigator`, Bildschirm und Uhr; Z6 gilt (kein Schalter).
   **Z1 (WebAudio) gibt es nicht**, weil es kein WebAudio gibt — die
   Anforderung steht in `REQUIREMENTS.md` als Bedingung an dessen Bau.
   **Z2 (Stummschaltung am Ausgabepfad des Reiters) gibt es nicht**, es
   gibt keinen Ton. **Z5 (Filterlisten) gibt es nicht.** Und
   WebGL-Kennungen und die Schriftenliste, die Z3 ausdrücklich nennt,
   sind nicht dabei — es gibt weder WebGL noch eine Schriftenliste, die
   eine Seite abfragen könnte.

9. **Das Rauschen der Leinwand ist noch an keine echte JS-Funktion
   angeschlossen.** Es gibt kein `toDataURL` und kein `getImageData` in
   dieser Engine. Gemessen ist die Funktion, die diese beiden benutzen
   **müssen** — die Regel dazu steht im Kopf von `lib/paint/canvas.fi`,
   und sie ist eine Regel und keine Prüfung: nichts hindert einen
   künftigen Rückleseweg daran, sie zu missachten. Ein Prüfschritt, der
   das erzwingt, wäre die nächste ehrliche Ergänzung.

10. **Die eine unberührte 16 × 16-Leinwand von 500** (1.3) ist eine echte
    Lücke und keine Rundung: für diesen Ursprung wirkt die Abwehr auf
    dieser Leinwandgröße nicht.

11. **Der `navigator`-Rauschraum ist klein.** `hardwareConcurrency` hat
    11 mögliche Werte, `deviceMemory` 4. Das trennt zwei Seiten
    zuverlässig, aber es macht die Maschine nicht anonym — es macht die
    beiden Werte nur wertlos zum **Zusammenführen**.

12. **Die Chromium-Zeile in 1.1 ist aus Runde B2 übernommen.** In dieser
    Runde lief kein Chromium.

13. **`css/css-flexbox/alignment` steht weiter bei 0 von 7** und
    `css/css-align/blocks` bei 0 von 4. Grundlinienausrichtung über
    Tabellen, Mehrspaltensatz und `line-clamp` ist nicht angefangen.

14. **Der Prüfstand von B4 ist nicht deterministisch, und das ist in
    dieser Runde aufgefallen.** Abschnitt 9 von `tools/liveb4/run.sh`
    schickt einen Ausschnitt von 60 Dateien durch alle drei Baustufen und
    verlangt dieselbe Quote. Der erste Lauf dieser Runde meldete für die
    `--no-opt`-Stufe `10 / 114` statt `10 / 163` — dieselbe Zahl
    bestandener Untertests, ein **kleinerer Nenner**. Ein Nenner ändert
    sich, wenn eine Datei ihren Prüfstand nicht zu Ende bringt; das
    Zeitlimit steht bei 25 Sekunden je Datei.

    Nachgemessen: derselbe `--no-opt`-Binärkörper auf demselben Ausschnitt
    dreimal hintereinander ergab **163, 163, 114**. Auf dem **unberührten**
    Baum `firn-b5-tls` ergab dieselbe Messung viermal **163**. Der saubere
    Gesamtlauf dieser Runde ist grün (`390 / 1714`, alle drei Stufen
    `10 / 163`, `B4 OK`), und die Zahl in 1.4 stammt aus ihm.

    Was ich sagen kann: die Schwankung ist echt und nicht auf die
    Auslastung allein zu schieben. Was ich **nicht** sagen kann: ob sie
    von dieser Runde kommt. Vier stabile Läufe drüben und ein Ausreißer
    von drei hier sind kein Beweis in beide Richtungen, und ich habe die
    Ursache nicht gefunden. Wer als Nächstes an B4 arbeitet, sollte
    zuerst diesen Ausschnitt zwanzigmal laufen lassen, bevor er eine Zahl
    daraus glaubt.

---

## 4. Wie geprüft wurde

Dieselbe Regel wie in B1 bis B5: **die Gegenstelle darf nicht von hier
sein.** Für das Layout ist sie die Web-Platform-Test-Sammlung — die
Erwartungen stehen in den Dateien, geschrieben von der CSS-Arbeitsgruppe
und den Ingenieuren der anderen Browser, und der Prüfstand rechnet nur die
Differenz aus. Für Kapitel Z ist sie Pythons `hashlib` und die
Auswertung in `tools/fpz/fp_check.py`, die die Abweichungen selbst
ausrechnet und dem geprüften Programm keine einzige Zahl glaubt.

Und jede Zusage hat eine Gegenprobe:

| Zusage | Gegenprobe |
|---|---|
| das Layout ist besser geworden | dieselben drei Baustufen müssen dieselbe Zahl liefern (97/97/97) |
| ein zweites Layout ist das erste | 800 → 400 → 800, 471 / 471 Dokumente identisch |
| das Rauschen trennt Ursprünge | ohne Rauschen **muss** über dieselben 500 Ursprünge **1** herauskommen |
| das Rauschen ist unsichtbar | größte Kanalabweichung 1, Alpha 0-mal angefasst |
| das Rauschen ist stabil | 20 Lesungen, Oktett für Oktett gleich |
| das Fenster heißt Certus | `xwd -name Certus` findet es vom X-Server aus |
| nichts ist kaputtgegangen | B1, 61/67, B3, B4 und B5 laufen vollständig durch |

---

## 5. Wo der Code liegt

| Datei | was |
|---|---|
| `lib/css/cascade.fi` | `aspect-ratio`, `margin-trim`, die logischen Eigenschaften, die zweiwortigen Ausrichtungswerte |
| `lib/layout/box.fi` | `Box.ratio`, `F_BR`, `F_FILL_INLINE`, der volle Vorgabe-Algorithmus in `replaced_size` |
| `lib/layout/build.fi` | `replaced_attach`: canvas, video, svg, object/embed/iframe; `stretch` in `depends_on_height` |
| `lib/layout/flow.fi` | `layout_replaced` neu, `align_phys`, `margin-trim` in Flex und Block, `trim_block_end`, `min_height_zero` |
| `lib/browser/fpz.fi` | Kapitel Z: Schlüssel, Strom, Leinwand, `navigator`, Uhr |
| `lib/browser/fpz_main.fi` | der Treiber, über den Kapitel Z von außen messbar ist |
| `tools/fpz/` | `run.sh`, `fp_check.py`, `minquota.txt` |
| `tools/layoutb2/minquota.txt` | die neue Untergrenze: 97 |
| `test.sh` | Abschnitt 64 |

---

## 6. Was als Nächstes dran wäre

1. **Grundlinienausrichtung** (`css-flexbox/alignment`, `css-align`):
   11 Tests, die zusammenhängen und alle dieselbe fehlende Sache
   brauchen.
2. **`data:`-URLs für `<img>`** — sieben Flexbox-Tests, und im echten Netz
   sind kleine eingebettete SVG viel häufiger als in dieser Liste.
3. **`contain-intrinsic-size`** — 3 Tests, kleine Eigenschaft.
4. **Ein Prüfschritt, der Punkt 9 aus Abschnitt 3 erzwingt:** kein
   Rückleseweg der Leinwand ohne `fpz`.
5. **Grid.** 22 Tests im eigenen Korpus, 3 davon grün, und ohne Grid ist
   kein Layout einer heutigen Seite richtig.
