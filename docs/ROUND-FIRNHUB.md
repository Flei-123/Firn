# Runde FIRNHUB — woher ein Paket kommt, wem man es glaubt, und was Gesamtprogramm-Übersetzung wirklich kostet

**Zweig `firnhub`, abgezweigt von `r93-lock`.** Warum nicht von `main`, steht
in Abschnitt 0 — es ist kein Geschmacksurteil, sondern eine Messung.

**Der Stand vorher:** Runde 48 hat das Projektsystem gebaut (Manifest,
Suchreihenfolge, Sichtbarkeit auf Modulebene, Paketzyklen, `--package`),
Runde 93 die Sperrdatei `firn.lock` mit Prüfsummen über jede beteiligte
Quelldatei. Der eigene Bericht der Runde 93 hat festgehalten, was fehlt:
*„Kein Netzwerk, keine Registry. `needs` kennt nur lokale Pfade."* Genau das
war die Aufgabe dieser Runde.

**Das Ergebnis in einer Tabelle:**

| | |
|---|---|
| `needs` kann jetzt | lokalen Pfad · `git+<url>#<ref>` · `<url>#sha256=<64>` · `<version>` über einen Index |
| Neu dabei | der Beschaffer `firnpkg` (in Firn), ein inhaltsadressierter Zwischenspeicher, `firn.have`, `firn.lock` Format 2 |
| `tools/hub/run.sh` | **37 Prüfungen, 0 Fehler** (neu) |
| `tools/packages/run.sh` | **39 bestanden, 0 Fehler** (vorher wie nachher) |
| `cargo test` | **247 bestanden, 0 Fehler** (vorher wie nachher) |
| Zwei echte Bibliotheken als Beleg | `demos/hub/json` 1.0.0, `demos/hub/date` 1.1.0, jede mit eigenen Tests |

---

## 0. Von welchem Zweig — und der Fund, der dabei herausfiel

Die Vorgabe war: nachsehen, welcher Zweig den neuesten Stand von
`bin/package.fi` und `bin/lock.fi` hat, von dort abzweigen, die Wahl
begründen. Nachgesehen (`git log -1 --format=... <zweig> -- <datei>` über
alle 90 Zweige):

```
bin/package.fi   86b6ce67d  2026-08-19   auf JEDEM Zweig, der sie hat
bin/lock.fi      e411845a7  2026-08-24   auf JEDEM Zweig, der sie hat
```

Beide Dateien sind auf `main`, `speed`, `r93-lock` und allen anderen
**derselbe Commit**. Der „neueste Stand" ist damit kein Auswahlkriterium
mehr — jeder Zweig, der die Dateien hat, hat dieselben. `main` liegt 20
Commits vor `r93-lock` (ARM, Lizenzkopfzeilen, Runde SPEED).

Also wurde das entschieden, was man messen kann: **wo ist die Messlatte
grün?**

```
r93-lock:  bash tools/packages/run.sh   ->  39 bestanden, 0 Fehler
main:      bash tools/packages/run.sh   ->  22 bestanden, 17 Fehler
```

### Der Fund: `thread-bool` aus Runde SPEED übersetzt `firnc1` falsch

Auf `main` (und auf `speed`, von dem `main` es geerbt hat) ist der
selbstgehostete Compiler **kaputt**: `firnc1` kann keinen einzigen `import`
mehr auflösen und bricht mit Exit 2 und **ohne Meldung** ab.

```
$ export FIRNLIB=$PWD/lib
$ firnc bin/firnc1.fi -o fc1 && ./fc1 tests/110_module.fi -o /tmp/m
$ echo $?
2                       # und nichts auf stderr
```

Nicht vermutet, sondern eingegrenzt. `firnc` kann einzelne Optimierläufe
abschalten (`--no-pass=`), also wurde `firnc1` elfmal gebaut, jedes Mal mit
einem Lauf weniger:

| abgeschalteter Lauf | `firnc1 tests/110_module.fi` |
|---|---|
| — (unverändert, `dev-fast`) | **Exit 2** |
| `--no-opt` (alles aus) | Exit 0 |
| `fold`, `cse`, `licm`, `bce`, `copyprop`, `merge-blocks`, `simplify-term` | Exit 2 |
| `dce` | Exit 1 |
| `mem2reg` | Exit 0 |
| `strength` | Exit 0 |
| **`thread-bool`** | **Exit 0** |

`thread-bool` ist der Lauf aus Runde SPEED („jump threading through bool
cells and bool phis, short circuit && / ||"). `mem2reg` und `strength` sind
Mitläufer: ohne sie entstehen die Bool-Zellen erst gar nicht, an denen
`thread-bool` sich vergreift.

Die Stelle, an der es sich zeigt, ist `imports_collect` in `bin/firnc1.fi` —
eine Kette aus `if !found { … found = true … }`. `strace` zeigt, dass der
Compiler die Datei **findet** und trotzdem weitersucht:

```
access("tests/modules/math.fi", F_OK)             = 0      <- gefunden
access("/root/firn-hub/lib/modules/math.fi", F_OK) = -1 ENOENT
access("/tmp/../lib/modules/math.fi", F_OK)        = -1 ENOENT
+++ exited with 2 +++
```

Das `if !found` nach dem Treffer greift nicht mehr. Ein minimales
Gegenbeispiel derselben Form (`var found: bool`, vier `if !found`-Stufen)
ließ sich in der zur Verfügung stehenden Zeit **nicht** herstellen — es
übersetzt in allen vier Baustufen richtig. Der Fehler braucht mehr Kontext,
als ein Zwanzigzeiler hat.

**Das ist nicht in dieser Runde repariert worden**, und das ist eine
Entscheidung: eine Sprungfaden-Optimierung zu reparieren ist eine Runde für
sich, und diese Runde hatte fünf Teile. Was hier steht, ist die
Reproduktion, die Eingrenzung auf **einen** Lauf und die Stelle im
Quelltext — damit kann die nächste Runde vorne anfangen statt von null.

Bis dahin ist `r93-lock` der Zweig, auf dem die Messlatte dieser Runde
überhaupt gemessen werden kann.

---

## Teil 0 — Was umbenannt wurde

Der Projekteigner hat englische Bezeichner in Code und Dateiformaten
verlangt. Beim Nachsehen stellte sich heraus: **die Runden 55 und 57 hatten
den größten Teil schon gemacht.** Die Feldnamen `paket`, `quelle`,
`oeffentlich`, `brauche` und der Schalter `--paket` existierten nicht mehr;
sie hießen längst `package`, `source`, `public`, `needs` und `--package`.
Der Auftrag beschrieb einen Stand, den nur noch `SPEC.md` Punkt 15 und
`docs/ROUND48.md` behaupteten — dort standen die deutschen Namen als
Altlast in der Prosa.

Übrig waren genau **drei** Dinge:

| vorher | nachher | wo |
|---|---|---|
| Datei `firn.package` | **`firn.pkg`** | `package.rs` `MANIFEST`, `package.fi` `mn`, `lock.fi` `mname`, alle Werkzeuge, Dokumente, die drei Demo-Manifeste (`git mv`) |
| Schlüssel `start` | **`main`** | Schlüssel, drei Meldungen, die Liste der erlaubten Schlüssel, die Ausgabe von `--package-info`, beide Compiler |
| `SUCHTIEFE` | **`SEARCH_DEPTH`** | `package.rs` — der letzte deutsche Bezeichner in der Datei |

Ein Manifest sieht heute so aus (`demos/hub/json/firn.pkg`, zum Abtippen):

```text
package  json
version  1.0.0
main     test/main.fi
source   src
source   test
public   json
```

**Der Schalter bleibt `--package`.** Der Auftrag erlaubte „`--pkg` (bzw.
`--package`)". `--package` ist bereits englisch, steht in `test.sh`, in
`ACCEPTANCE.md` und in jedem Dokument, und eine zweite Schreibweise für
dieselbe Sache wäre genau der Alias, den diese Runde nicht mitschleppen
sollte.

**Keine Aliase.** `firn.package` und `start` werden nicht mehr angenommen;
`start` fällt in die Meldung `unknown key 'start' (allowed: package,
version, main, source, public, needs)`. Vorher nachgesehen: `find / -name
'firn.paket'` und `find / -name 'firn.package'` finden außerhalb dieses
Repositoriums **nichts**, es war also nichts zu migrieren.

Bei den Firn-Seiten musste jedes feste `[u8; N]` neu ausgezählt werden — die
Sprache hat keine Zeichenketten mit Länge zur Übersetzungszeit an dieser
Stelle, und `tools/english/check_lengths.py` vergleicht jede Zahl mit dem
Text daneben. Nach der Umbenennung: **0 falsche Längen.**

Gemessen nach Teil 0: `tools/packages/run.sh` **39/0**, `cargo test`
**247/0**, `check_lengths.py` **0**. Getrennt committet
(`f0818c05c`).

---

## Teil 1 — Wie eine Abhängigkeit heute deklariert wird

`needs <name> <quelle> [<version>]`. Die Quelle ist **ein Wort** (das Format
hat keine Anführungszeichen, ein Wert enthält also nie ein Leerzeichen), und
ihre Art wird **rein lexikalisch** entschieden — ohne Dateisystem, ohne
Netz, damit beide Compiler denselben Text gleich einordnen.

```text
needs  json  ../json                                       lokaler Pfad
needs  json  ../json  0.2.0                                ... mit Versionswunsch
needs  json  git+https://github.com/justin/firn-json#v1.2.0   Git, fester Verweis
needs  json  https://example.org/json-1.2.0.tar#sha256=9c1e…  Archiv mit Prüfsumme
needs  json  1.2.0                                         Registry-Kurzform
```

Ein echtes Manifest zum Abtippen (`demos/hub/app/firn.pkg`, in der Fassung,
die `tools/hub/run.sh` baut):

```text
package  app
version  0.1.0
main     src/main.fi
source   src

needs    json  ../json  1.0.0
needs    date  git+file:///pfad/zum/firn-date#v1.1.0  1.1.0
```

Die Regeln, und warum sie so sind:

* **`git+` MUSS `#<verweis>` tragen.** Ein Commit oder ein Etikett. Die
  schwebende Form ohne `#` ist ein Fehler *im Manifest* — nicht erst dann,
  wenn der Bau scheitert. Ein Zweigname wird als Text angenommen und ist
  eine schlechte Idee; was refüsiert wird, ist das Fehlen jedes Verweises.
* **`http(s)://` und `file://` MÜSSEN `#sha256=<64 Hexziffern>` tragen.** Ein
  Archiv ohne Prüfsumme ist keine Abhängigkeit, sondern ein Wunsch.
* **Die Registry-Kurzform ist die, die WIE EINE VERSION AUSSIEHT**
  (`zahl.zahl.zahl`). Der Preis dafür ist genau ein pathologischer
  Verzeichnisname: ein lokales `./1.2.0` ist nicht mehr erreichbar. Bezahlt,
  damit `needs json 1.2.0` das heißen kann, was jeder hineinliest.

**Version auflösen: die Regel der Runde 93 bleibt unangetastet** — genaue
Version oder Etikett, ein Name = ein Paket, die höhere Version gewinnt, und
zwei unvereinbare Wünsche sind ein Fehler mit Meldung, kein stiller
Kompromiss. Es wurde **keine** Semver-Löserei mit Rückverfolgung erfunden.

### Die Kommentarregel musste weichen

Runde 48 schnitt eine Zeile an **jedem** `#`. Damit ist
`git+https://h/r#v1.2.0` unmöglich: der Verweis wäre eine Bemerkung. Seit
dieser Runde öffnet ein `#` einen Kommentar nur **da, wo ein Wort
beginnt** — am Zeilenanfang oder hinter einem Trennzeichen. Ein `#` innerhalb
eines Wortes gehört zum Wort.

```text
needs j git+https://h/r#v1.2.0   # das hier ist eine Bemerkung
```

Ein Satz länger als vorher, und **jedes bestehende Manifest liest sich
unverändert** (ein Kommentar hat praktisch immer ein Leerzeichen vor sich
oder steht am Zeilenanfang). Beide Compiler, geprüft in
`tools/hub/run.sh`.

---

## Teil 2 — Zwischenspeicher und Beschaffung

### Die Entscheidung: der Compiler spricht nicht mit dem Netz

Der Auftrag ließ die Wahl zwischen einem Unterbefehl `firnc fetch` und einem
getrennten Werkzeug. Gewählt wurde das getrennte Werkzeug, und die Begründung
ist die harte Projektregel selbst:

> Alles *im Compiler* muss **zweimal** existieren — als Rust (`firnc0`) und
> als Firn (`firnc1`) — und beide müssen dieselben Oktette und dieselben
> Sätze liefern.

Diese Regel ist ihren Preis wert für ein Manifestformat und für eine
Prüfsumme. Sie ist ihn **nicht** wert für `git clone`: der Netzteil wären
dann zwei Umsetzungen desselben Unterprozessaufrufs, doppelte Fläche,
doppelte Gelegenheit auseinanderzulaufen — und keine einzige Zeile davon
landet je in einem erzeugten Binärprogramm.

Also getrennt nach **Zuständigkeit**:

* Der **Compiler** liest Text und rechnet Prüfsummen. Er öffnet **nie** eine
  Verbindung. Eine entfernte Abhängigkeit erreicht ihn als lokales
  Verzeichnis.
* Der **Beschaffer** spricht nach außen. Er existiert **einmal**, in Firn,
  und wird von `firnc` übersetzt wie jedes andere Programm.

Der schöne Nebeneffekt: **ein Bau ohne Netz braucht keinen Schalter.** Es ist
nichts da, was ins Netz gehen könnte. `--offline` gehört dem Beschaffer und
heißt dort: nimm nur, was im Zwischenspeicher liegt, und sag es laut, wenn
etwas fehlt.

### Die Brücke: `firn.have`

Der Beschaffer schreibt neben das **Wurzelmanifest** eine Datei, die der
Compiler liest:

```text
have 1
need date git+file:///p/firn-date#v1.1.0 40b4a07b28fa6eda0a95ef7f80cb76f4d33390b0 766a039e2a38e0cb…
need json 1.2.0 git+https://host/firn-json#v1.2.0 9c1e4f…
```

Felder: Name · Quelle **genau so, wie sie im Manifest steht** · worauf sie
sich aufgelöst hat (ein Commit, oder die Prüfsumme der Archivoktette) · der
**Inhaltshash** des ausgepackten Baums. Der letzte ist die Adresse im
Zwischenspeicher und das Einzige, was der Compiler braucht; die ersten drei
sind da, damit ein Auseinanderlaufen von Manifest und geholtem Stand ein
Fehler ist und keine Überraschung.

*Warum eine eigene Datei und nicht `firn.lock`:* `firn.lock` wird **nach**
einem Bau geschrieben und sagt, was hineinging. `firn.have` wird **vor**
einem geschrieben und sagt, auf welche Oktette eine Quelle aufgelöst wurde.
Zwei Erzeuger, zwei Zeitpunkte, zwei Dateien — ein Beschaffer, der in
`firn.lock` schriebe, müsste die Prüfsummen eines Baus erfinden, der noch
nicht stattgefunden hat.

*Warum eine Datei für den ganzen Graphen und nicht eine je Paket:* dieselbe
Entscheidung, die `firn.lock` trifft. Ein geholtes Paket bleibt damit ein
reiner Inhaltsbaum und muss nicht neu veröffentlicht werden, nur weil sich
etwas unter ihm bewegt hat.

### Der Zwischenspeicher

```text
<cache>/pkg/<64 Hexziffern>/   das ausgepackte Paket, unveränderlich
<cache>/tmp/                   Arbeitsplatz einer laufenden Beschaffung
<cache>/index.txt              was der Beschaffer schon kennt
```

Ort: `$FIRN_CACHE`, sonst `$HOME/.firn/cache`, sonst **leer** — und dann ist
jede entfernte Abhängigkeit ein Fehler mit einem Satz, der das sagt. Kein
stilles Ausweichen ins Arbeitsverzeichnis: ein Zwischenspeicher, der woanders
liegt, als der Benutzer denkt, ist schlimmer als keiner.

Vorbild ist `pkg/opk.py` aus OrientOS: ein Paket wird zu **einem
unveränderlichen Verzeichnis, benannt nach der SHA-256 seines Inhalts**.
Übernommen wurde die Idee, **nicht** ihr eines Zugeständnis: opk kürzt den
Verzeichnisnamen auf zwanzig Hexziffern, weil ein Verzeichniseintrag in OFS
vierundzwanzig Oktette hat (`kernel/fs.fi`, `NAME_LEN = 24`). Diese Grenze
gibt es hier nicht, also ist der Name der **ganze** Hash — eine Kollision ist
nichts, wofür man Bequemlichkeit eintauschen muss.

### Der Inhaltshash

Derselbe Oktettstrom, den `lock.rs` schon für ein Paket rechnet: je Datei
`relativer Pfad \n Länge \n Inhalt \n`, nach dem Pfad sortiert. **Über jede
reguläre Datei des Baums**, nicht nur über die, die ein Bau zufällig liest —
die Adresse eines Pakets darf nicht davon abhängen, wer es ansieht. `.git`
fliegt raus: das ist Maschinerie eines Transports, kein Teil eines Pakets,
und reproduzierbar ist es auch nicht.

Gegengeprüft gegen `sha256sum` aus coreutils über denselben Strom, in Shell —
eine dritte Umsetzung des Formats, auf vier verschiedenen Bäumen
(`tools/hub/run.sh`).

Ein Nebenergebnis, das man kennen sollte: für ein Paket, dessen **jede**
Datei an einem Bau teilnimmt, kommen der Inhaltshash und die
`package`-Zeile von `firn.lock` **gleich** heraus. Derselbe Strom, zweimal
gemessen.

### `firnpkg`

```
firnpkg fetch [--offline] [--quiet] <dir>   auflösen, prüfen, firn.have schreiben
firnpkg verify <dir>                        alles, was firn.have nennt, neu hashen
firnpkg hash <dir>                          der Inhaltshash eines Baums
```

`git`, `curl` und `tar` werden als **Programme** gerufen — genau so, wie
`firnc` schon `as` und `ld` ruft. Keine fremde Bibliothek wird gebunden, kein
fremder Quelltext übersetzt. Was ein Transportprogramm zurückgibt, wird als
feindlich behandelt: ein Archiv wird gegen die Prüfsumme aus dem Manifest
gewogen, **bevor** `tar` es zu sehen bekommt, und jeder Baum wird gegen
seinen eigenen Inhaltshash gemessen, bevor er eine Adresse wird. Kein
Shell-Aufruf, sondern ein Argumentfeld — eine URL aus einem Manifest kann
also nichts als Befehl gelesen werden.

Der aufgelöste Commit wird **nicht** über eine Pipe erfragt, sondern aus
`.git/HEAD` gelesen: ein losgelöster HEAD hält den rohen Commit, und damit
braucht der Beschaffer weder Pipe noch Shell.

**Der eine Fehler, den das Verzeichnislesen gekostet hat:** `rt.ld16(p, i)`
indiziert in **Einheiten von zwei Oktetten**. `d_reclen` eines
`linux_dirent64` liegt bei Oktett 16, also ist der richtige Aufruf
`ld16(p, 8)` und nicht `ld16(p, 16)`. Mit dem falschen Index kam eine
Zufallszahl als Satzlänge heraus, die Schleife lief aus dem Block, und jede
Rekursion endete nach dem ersten Eintrag — `firnpkg hash demos/packages/geo`
sah genau eine Datei statt vier. Gefunden mit `firnpkg list`, das die
gefundenen Pfade ausgibt und gegen `find | sort` gehalten wurde.

---

## Teil 3 — Vertrauen

### `firn.lock` Format 2

```text
lock 2
root app
package app 0.1.0 . 86f1f779…
package date 1.1.0 cache:766a039e… 50efa016…
package json 1.0.0 ../json dd2dc3d4…
origin date git+file:///p/firn-date#v1.1.0 40b4a07b… 766a039e…
outside 0 e3b0c442…
total 4312ee4d…
```

Zwei Zusätze:

* Ein **geholtes** Paket hat keinen Pfad, der auf einer anderen Maschine
  etwas bedeuten würde. Sein Ort ist deshalb `cache:<inhaltshash>` — die
  Adresse im inhaltsadressierten Speicher, auf jeder Maschine dieselbe, die
  dieselben Oktette geholt hat.
* Eine **`origin`-Zeile** je geholtem Paket, nach dem `package`-Block, in
  derselben Reihenfolge: Quelle wie im Manifest · aufgelöster Verweis ·
  Inhaltshash.

Die Formatnummer geht von 1 auf 2. Eine alte Sperrdatei wird **refüsiert**,
nicht umgedeutet.

**Passt der Hash nicht mehr, bricht der Bau ab.** Das ist gemessen und nicht
behauptet: `tools/hub/run.sh` hängt eine Zeile an eine Datei **im
Zwischenspeicher** an, und der Fehler wird **zweimal unabhängig** gefangen —

```
$ firnpkg verify proj/app
error: package 'date' in the cache is not what it says
note: wanted 766a039e2a38e0cb2ba73127cdb97e60693b163852ed6ddccd813d7cfe4ec4cf
note: got    1e0c…

$ firnc --package proj/app --locked -o app
error: proj/app/firn.lock: the lock file does not match the sources
note: line 4 of the file:  'package date 1.1.0 cache:766a039e… 50efa016…'
note: line 4 of the build: 'package date 1.1.0 cache:766a039e… a91b77c3…'
```

— und beide Compiler sagen denselben Satz, Oktett für Oktett.

### Vertrauen beim ersten Kontakt, und was es wirklich kauft

`<cache>/index.txt` hält je Quelle `quelle aufgelöst inhalt`. Kommt dieselbe
Quelle je mit **anderen** Oktetten zurück, wird sie laut refüsiert:

```
error: the source changed under a fixed reference
note: source 'git+file:///p/firn-date#v1.1.0'
note: was    766a039e…
note: is now 4f31c8a1…
```

Das fängt einen **umgehängten Git-Tag** — ein echter Angriffsweg, und in
`tools/hub/run.sh` wird genau er nachgestellt: das Etikett `v1.1.0` wird
gelöscht, ein neuer Commit gemacht, das Etikett neu gesetzt, der
Zwischenspeicher geleert. Die erneute Beschaffung bricht ab.

Was es **nicht** kann: den ersten Kontakt von einer Lüge unterscheiden. Es
ist Trust on first use und wird hier so genannt.

### Ed25519 — bewusst nicht gebaut, und was es gekostet hätte

Der Auftrag sagte: „Ed25519 existiert im Projektumfeld bereits mehrfach
(Certus, OrientOS opk) — nachsehen und wiederverwenden statt neu schreiben."
**Nachgesehen. Es existiert nicht in Firn.**

* `lib/std/crypto/x25519.fi`, Punkt X3 des eigenen Kopfes: *„NO EDWARDS
  FORM, no signature. Ed25519 is a different curve arithmetic on the same
  field; this file is only the key exchange."*
* `lib/tls/x509.fi`: *„P-521, Ed25519 and DSA come back `UNSUPPORTED`."*
* `lib/std/crypto/ecdsa.fi`: *„P-256 AND P-384 ONLY. Not P-521, not
  Ed25519."*

Die einzige Umsetzung im Projektumfeld ist **Python**
(`orientos-base/pkg/opk.py`, dort sogar zweimal — eine eigene neben der des
Wirts). Aus Python lässt sich hier nichts wiederverwenden.

Was ein Ed25519-Prüfer in Firn gebraucht hätte, ehrlich aufgeschlüsselt:

| Baustein | Stand |
|---|---|
| SHA-512 | **da** (`lib/std/crypto/sha512.fi`) |
| Feldarithmetik mod 2²⁵⁵−19 | **da** (`lib/std/crypto/big.fi`, Montgomery, von x25519 benutzt) |
| Punktarithmetik auf der verdrehten Edwards-Kurve, erweiterte Koordinaten | fehlt, ~150 Zeilen |
| Punktdekompression (Wurzel über `pow((p+3)/8)`, Vorzeichenbit) | fehlt, ~50 Zeilen |
| Reduktion eines 512-Bit-Werts modulo der Gruppenordnung L | fehlt, ~80 Zeilen |
| Prüfvektoren aus RFC 8032 | fehlt |

Zusammen etwa **400 Zeilen neuer Kryptographie** — und wenn die Prüfung je in
den Compiler wandert, muss sie dort **zweimal** stehen. Das ist eine Runde für
sich, keine halbe Stunde am Ende einer anderen.

**Was stattdessen da ist, und das ist mehr als nichts:** der Inhaltshash
steht in `firn.have` **und** in `firn.lock`, beide sind eincheckbar, und ein
abweichender Zwischenspeicher bricht den Bau ab. Wer heute
`git+https://…#<commit40>` schreibt, hat einen Verweis, der selbst ein
Inhaltshash ist — von Git, nicht von uns, aber es ist einer. Jede
Beschaffung meldet die Quelle ausdrücklich als **unsigniert**:

```
fetched date  git+file:///p/firn-date#v1.1.0
note: unsigned source, trusted on first use (no signatures yet)
```

Das ist die ehrliche Auskunft, die der Auftrag verlangt hat: unsignierte
Quellen sind erlaubt und werden **sichtbar als solche gemeldet**.

---

## Teil 4 — Was Gesamtprogramm-Übersetzung kostet

Firn übersetzt **Gesamtprogramme**: ein importiertes Modul landet in
derselben Übersetzungseinheit, es gibt keine getrennten Objektdateien und
keine Schnittstellendateien (`SPEC.md` Punkt 15). Bei zehn Abhängigkeiten
wird also jedes Mal alles neu übersetzt. Die Frage war: **ab welcher Größe
ist das untragbar, und muss getrennte Übersetzung vor einem Ökosystem
kommen?**

Gemessen statt vermutet. `tools/hub/scale.py` erzeugt künstliche
Abhängigkeitsbäume — N Pakete mit je 2.000 Zeilen Firn plus ein Wurzelprojekt,
das jedes einbindet und je eine Funktion daraus ruft — und **zusätzlich** ein
einzelnes Paket mit **derselben Gesamtzeilenzahl**. Der zweite Wert ist der
wichtige: er trennt die Kosten der *Pakete* von den Kosten der *Zeilen*.

Maschine: AMD EPYC 7571, Linux x86_64, `firnc0` in `--release`,
Baustufe `dev-fast` (die Voreinstellung). Bester von drei Läufen, größter
RSS aus `/usr/bin/time`. **Vorbehalt:** die Maschine trug während der
Messung eine Lastspitze von ~20 (andere Runden liefen mit). „Bester von
drei" fängt das teilweise ab; die *Verhältnisse* unten sind robust, die
absoluten Sekunden auf einer ruhigen Maschine kleiner.

### Die Zahlen

| Pakete | Zeilen gesamt | N Pakete: Zeit | RSS | **1 Paket, gleiche Zeilen**: Zeit | RSS | s / 1000 Zeilen |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 2.674 | 0,12 s | 9 MiB | 0,12 s | 9 MiB | 0,044 |
| 5 | 13.346 | 0,64 s | 30 MiB | 0,70 s | 30 MiB | 0,048 |
| 10 | 26.686 | 1,77 s | 57 MiB | 1,57 s | 57 MiB | 0,066 |
| 25 | 66.706 | 4,31 s | 136 MiB | 4,69 s | 137 MiB | 0,065 |
| 50 | 133.406 | 15,70 s | 269 MiB | 19,45 s | 271 MiB | 0,118 |
| 100 | 266.806 | 115,12 s | 536 MiB | 120,78 s | 536 MiB | 0,431 |

**Wiederaufbau nach einer Änderung an EINER Zeile der Wurzel** — der Alltag,
denn man ändert sein eigenes Programm und nicht seine Abhängigkeiten:

| Pakete | Zeilen gesamt | Bauzeit |
|---:|---:|---:|
| 1 | 2.675 | 0,12 s |
| 5 | 13.347 | 0,61 s |
| 10 | 26.687 | 1,70 s |
| 25 | 66.707 | 4,69 s |
| 50 | 133.407 | 18,13 s |
| 100 | 266.807 | 106,11 s |

Es ist **jedes Mal die volle Bauzeit**. Genau diese Zahl wäre bei getrennter
Übersetzung nahezu null.

### Drei Befunde, und der dritte ist der eigentliche

**1. Das Paketsystem kostet nichts.** N Pakete und ein einzelnes Paket mit
derselben Zeilenzahl liegen über den ganzen Bereich innerhalb von ±13 % —
mal ist das eine, mal das andere schneller. Der Suchpfad, die
Sichtbarkeitsprüfung, die Zyklensuche und die Auflösung aus Runde 48/93
tauchen in der Messung **nicht auf**. Wer also sagt „Pakete sind teuer",
misst in Wahrheit Zeilen.

**2. Der Speicher wächst sauber linear.** 9 → 30 → 57 → 136 → 269 → 536 MiB,
das sind gleichbleibend **~2 MiB je 1.000 Zeilen**. 267.000 Zeilen kosten
eine halbe Milliarde Oktette — viel, aber es ist kein Auslagern und keine
Speicherwand. Die Zeitkurve unten ist also **nicht** durch Speicherdruck
erklärbar.

**3. Die Zeit wächst NICHT linear, und es liegt an genau einer Phase.**
`firnc --timings` sagt, woran:

| Phase | 25 Pakete (66.706 Z.) | 100 Pakete (266.806 Z.) | Faktor bei 4× Zeilen |
|---|---:|---:|---:|
| **codegen** | 1.923 ms (46 %) | **133.002 ms (93 %)** | **69×** |
| as + ld | 1.480 ms | 6.944 ms | 4,7× |
| optimizer | 446 ms | 1.384 ms | 3,1× |
| lex+parse | 138 ms | 971 ms | 7,0× |
| lower | 110 ms | 530 ms | 4,8× |
| sema | 72 ms | 368 ms | 5,1× |
| mono | 6 ms | 72 ms | 12× |
| **gesamt** | **4.187 ms** | **143.318 ms** | **34×** |

Vorderende, Optimierer und Assembler sind **linear** (Faktor 3–7 bei 4×
Zeilen). Der **Codegenerator** ist es nicht: Faktor **69 bei 4× Zeilen**,
das ist ein Exponent von log 69 / log 4 ≈ **2,9 — kubisch**. Zwischen 50 und
100 Paketen allein: 18,2 s → 133,0 s, also 7,3× bei 2× Zeilen (Exponent
2,87).

**Und es ist nicht der Inliner.** Das war die naheliegende Vermutung, weil
der Prüfstand Funktionsketten erzeugt, die ein gefundenes Fressen für ihn
sind. Gegenprobe mit `--no-pass=inline`:

| | codegen mit `inline` | codegen ohne `inline` |
|---|---:|---:|
| 50 Pakete | 18.204 ms | 11.658 ms |
| 100 Pakete | 133.002 ms | **107.628 ms** |
| Faktor 50→100 | 7,3× | **9,2×** |

Ohne Inliner ist es *absolut* billiger und *relativ* sogar schlechter. Der
kubische Term steckt im Codegenerator selbst, nicht in dem, was der
Optimierer ihm vorlegt.

### Die Antwort auf die Frage

**Ab welcher Größe wird es untragbar?** Aus der Tabelle, mit dem
Wiederaufbau als Maßstab:

| Größe | volle Bauzeit | Urteil |
|---|---|---|
| bis 25 Pakete / ~67.000 Zeilen | 4–5 s | bequem |
| 50 Pakete / ~133.000 Zeilen | 16–23 s | spürbar, noch arbeitsfähig |
| 100 Pakete / ~267.000 Zeilen | **~2 Minuten je geänderter Zeile** | untragbar |

Die Wand steht also bei ungefähr **130.000 bis 260.000 Zeilen im ganzen
Programm** — bei 2.000 Zeilen je Paket sind das **50 bis 100 Pakete**. Zum
Vergleich: ein durchschnittliches Projekt mit zehn direkten Abhängigkeiten
zieht in npm oder Cargo transitiv 50 bis 200 Pakete nach. Ein Ökosystem
dieser Art würde diese Wand **erreichen**.

**Muss getrennte Übersetzung vor einem Ökosystem kommen? Nein — aber etwas
anderes muss, und zwar vorher.**

Die Empfehlung, mit Begründung aus den Zahlen:

1. **Zuerst den Codegenerator geradebiegen.** Wäre `codegen` linear mit der
   Rate, die es bei 25 Paketen hat (1,92 s / 66.706 Zeilen = 0,029 s je
   1.000 Zeilen), dann kosteten 266.806 Zeilen **7,7 s statt 133 s**, und der
   ganze Bau **~17,7 s statt 143 s** — ein Faktor **8**. Damit rutscht die
   Wand von 130.000 auf über eine Million Zeilen, also weit jenseits dessen,
   was ein Firn-Ökosystem in den nächsten Jahren erreicht. Das ist eine
   Optimierung *innerhalb* einer bestehenden Phase, kein neues Konzept, und
   `--timings` zeigt schon, wo zu suchen ist.
2. **Getrennte Übersetzung ist das viel teurere Vorhaben** und steht mit dem
   Rest der Sprache im Streit: Firn monomorphisiert Generik (Runde 50) und
   inlinet über Modulgrenzen. Eine getrennte Übersetzungseinheit braucht
   dafür Schnittstellendateien, eine stabile ABI und eine Antwort auf die
   Frage, wo eine `Vec[MeinTyp]` übersetzt wird. `SPEC.md` Punkt 15 sagt zu
   Recht, dass davon nichts existiert. Das ist eine große Runde, und sie
   kauft bei den heutigen Größen **weniger** als Punkt 1.
3. **Getrennte Übersetzung wird trotzdem gebraucht** — nur nicht wegen der
   Gesamtgröße, sondern wegen des Wiederaufbaus. Sobald Punkt 1 erledigt ist,
   ist ein Bau von 267.000 Zeilen in ~18 s zwar tragbar, aber bei jeder
   geänderten Zeile 18 s zu warten bleibt teuer. Das ist der Punkt, an dem
   sie sich lohnt — **nach** dem Codegenerator, nicht davor.
4. **Der billige Zwischenschritt existiert hier noch nicht und wäre keiner.**
   Es liegt nahe, mit dem neuen inhaltsadressierten Speicher auch das
   *Übersetzte* je Paket zwischenzuspeichern. Das geht nicht: bei
   Gesamtprogramm-Übersetzung hängt der erzeugte Code eines Pakets vom
   ganzen Programm ab (Monomorphisierung, Inlining über Modulgrenzen). Der
   Zwischenspeicher dieser Runde adressiert **Quelltext**, und mehr kann er
   ehrlich nicht.

**Kurz:** Gesamtprogramm-Übersetzung reicht noch lange — aber nur, wenn der
kubische Term im Codegenerator verschwindet. Heute ist nicht das Konzept die
Grenze, sondern eine Phase.


---

## Teil 5 — Der Beleg

### Zwei echte Bibliotheken

**`demos/hub/json` 1.0.0** — ein JSON-Leser, der **nichts allokiert**: jede
Antwort ist ein Versatz in den Text, den der Aufrufer ohnehin hat. Deshalb
hat er keine einzige Abhängigkeit und lässt sich in eine leere Welt holen.
Er entscheidet die Fragen, die ein echter Leser entscheidet: keine führende
Null (`01` ist zwei Werte, aneinandergeklebt), kein rohes Steuerzeichen in
einer Zeichenkette, `\u` wird **refüsiert** statt halb dekodiert. 26 eigene
Prüfungen.

**`demos/hub/date` 1.1.0** — bürgerliche Daten nach Howard Hinnants
verschobenem Jahr: der Februar wird zum letzten Monat, der Schalttag rutscht
ans Jahresende, und die Länge eines Monats wird eine geschlossene Formel
statt einer Tabelle. Exakt und ohne Schleife. Die Umkehrprobe läuft über
**jeden Tag von 1600 bis 2400** — 292.194 Tage, hin und zurück. 29 eigene
Prüfungen.

Beide tragen ihre Tests als ihr **`main`**:

```text
package  json
version  1.0.0
main     test/main.fi     <- die Tests
source   src
source   test
public   json             <- 'test' bleibt draußen
```

`firnc --package demos/hub/json` baut und **läuft** sie. Damit wohnen die
Tests einer Bibliothek in der Bibliothek, und wer sie einbindet, sieht `json`
und sonst nichts. Das ist keine neue Sprachfunktion — nur `main` plus eine
zweite `source`-Zeile plus `public`, alles aus Runde 48.

### Das Beweisprojekt

`demos/hub/app` zieht `json` über einen lokalen Pfad und `date` über eine
Git-Quelle. Im Quelltext sieht man den Unterschied nicht — genau das ist der
Punkt. `tools/hub/run.sh` baut dafür ein **echtes Git-Repositorium mit einem
echten Etikett** und holt es über `file://` (kein GitHub-Zugang auf dieser
Maschine; der Transport ist dasselbe `git`).

**37 Prüfungen, 0 Fehler.** Was darin steckt:

* beide Bibliotheken bestehen ihre eigenen Tests, **in beiden Compilern**;
* der Inhaltshash von `firnpkg` == `sha256sum` aus coreutils, auf vier Bäumen;
* der Bau **refüsiert**, bevor etwas geholt wurde — mit derselben Meldung aus
  beiden Compilern;
* `--offline` mit leerem Zwischenspeicher refüsiert, statt hinauszugreifen;
* nach `firnpkg fetch`: beide Compiler bauen, das Programm druckt
  `firn 2026-08-30 Sunday`, und **beide schreiben zeichengleich dieselbe
  `firn.lock`**;
* zweimal holen und zweimal bauen ändert weder `firn.have` noch `firn.lock`,
  Oktett für Oktett;
* eine Datei **im Zwischenspeicher** verändert → zweimal gefangen;
* ein **umgehängtes Etikett** → refüsiert;
* die **Archivstrecke** vollständig: herunterladen, wiegen, auspacken, das
  umhüllende Verzeichnis abziehen, hashen, adressieren — und ein Archiv mit
  falscher Prüfsumme wird refüsiert, *bevor* `tar` es sieht;
* die **Registry-Kurzform** `needs date 1.1.0` über einen Index;
* jede neue Fehlermeldung zeichengleich aus beiden Compilern.

---

## Was bewusst weggelassen wurde

* **Signaturen.** Kein Ed25519, kein signierter Index. Begründet oben mit dem
  Preis, der dafür fällig wäre.
* **Ein Index über HTTPS.** `$FIRN_INDEX` zeigt auf eine **Datei**. Wer den
  Index aus dem Netz will, lädt ihn selbst und zeigt darauf. Ein `curl` mehr
  im Beschaffer wäre billig; ein Index, dem man ohne Signatur glaubt, wäre
  es nicht.
* **Kein Semver-Löser.** Genaue Version oder Etikett. Ein Konflikt ist ein
  Fehler mit einer Meldung.
* **Kein Aufräumen des Zwischenspeichers.** Es gibt kein `firnpkg gc`. Was
  einmal drin ist, bleibt.
* **Keine parallele Beschaffung.** Ein Paket nach dem anderen. Bei zehn
  Abhängigkeiten aus dem Netz ist das spürbar und wäre leicht zu ändern.
* **Kein `git` ohne Netz für den ersten Kontakt.** `--offline` ist ehrlich:
  was nicht im Zwischenspeicher liegt, gibt es nicht.
* **Der Beschaffer ist kein zweiter Manifestleser.** Er sieht sich die
  `needs`-Zeilen an und sonst nichts; Stelligkeit, Namen, Versionen,
  Sichtbarkeit und Zyklen prüft der Compiler danach — in beiden Umsetzungen.
  Ein dritter vollständiger Leser des Formats wäre eine dritte Stelle zum
  Auseinanderlaufen.

## Was für eine echte öffentliche Registry noch fehlt

1. **Signaturen, in beide Richtungen.** Ein signierter Index (wer darf einen
   Namen vergeben?) und signierte Pakete (wer hat diese Oktette gebilligt?).
   Ohne das ist der Index nur eine Datei, der man glaubt.
2. **Ein Dienst, der Namen vergibt** — und die Regeln dazu: wer bekommt
   `json`, was passiert bei Aufgabe, was bei Streit. Das ist keine
   technische Frage und die Erfahrung sagt, dass sie die teuerste ist.
3. **Unveränderlichkeit der Veröffentlichungen.** Eine Version, die einmal
   veröffentlicht ist, darf sich nie wieder ändern. Heute hängt das am
   Wohlverhalten dessen, der den Git-Tag hält; unser TOFU merkt es
   *hinterher*.
4. **Spiegel und ein Beweis, dass alle dasselbe sehen** (ein Transparenzlog).
   Ein Index, den jeder anders sieht, ist schlimmer als keiner.
5. **Ein Zurückziehen** („diese Version hat ein Loch"), das einen Bau warnt,
   ohne ihn zu brechen.
6. **Getrennte Übersetzung.** Siehe Teil 4 — das ist die Zahl, nicht die
   Meinung.

## Die zwei Funde, die nebenbei abfielen

**1. `thread-bool` (Runde SPEED) übersetzt `firnc1` falsch.** Abschnitt 0.
Auf `main` und `speed` ist die Selbstübersetzung kaputt und
`tools/packages/run.sh` steht bei 22/39. Eingegrenzt auf **einen** Lauf.

**2. Die `outside`-Zeile von `firn.lock` hängt davon ab, WELCHER Compiler
baut — für jedes Programm, das `str` benutzt.** Ein Programm mit `str` zieht
die Sammler-Laufzeit nach (Runde 70). `firnc0` bettet sie über
`include_str!("../../lib/gc/gc.fi")` ein und **zählt sie als Datei außerhalb
jedes Pakets** mit; `firnc1` bringt denselben Text als `gctext.fi` eingebaut
mit und zählt ihn **nicht** mit. Gemessen an einer früheren Fassung von
`demos/hub/app`:

```
firnc0:  outside 1 7a1ede73…      total b8b763b1…
firnc1:  outside 0 e3b0c442…      total a46de792…
```

Das ist ein Loch im Versprechen der Runde 93 („beide Compiler schreiben
dieselbe Sperrdatei"), und es ist **älter als diese Runde**: die Demos der
Runde 93 benutzen kein `str`, deshalb ist es nie aufgefallen. Repariert
wurde es hier nicht — dafür müssten die beiden eingebetteten Texte Oktett
für Oktett gleich sein und `lock.fi` müsste den seinen mit demselben
Schlüssel in den Strom hängen. Das Beweisprojekt geht dem Fund aus dem Weg
(kein `str` in `demos/hub/app`), damit es misst, was es messen soll — und
sagt das in seinem eigenen Kopf.

## Messwerte vorher und nachher

| Messlatte | vorher (`r93-lock`) | nachher (`firnhub`) |
|---|---|---|
| `cargo test --release` | 247 bestanden, 0 Fehler | 247 bestanden, 0 Fehler |
| `bash tools/packages/run.sh` | 39 bestanden, 0 Fehler | 39 bestanden, 0 Fehler |
| `bash tools/hub/run.sh` | — (neu) | **37 bestanden, 0 Fehler** |
| `bash tools/english/check.sh` | 0 / 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 / 0 |
| `bash test.sh` | PLATZHALTER_TESTSH_VORHER | PLATZHALTER_TESTSH_NACHHER |

`tools/english/check.sh` hat eine **benannte Ausnahme** bekommen:
`docs/ROUND-FIRNHUB.md` und `demos/hub/` dürfen deutsche Prosa tragen, weil
der Projekteigner genau das bestellt hat. Eine Ausnahme, die in der Datei
steht, ist ehrlicher als eine Messlatte, die still auf 16 Zeilen fällt.
