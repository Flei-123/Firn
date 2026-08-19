# Runde 48 — Pakete, Projektmanifest, Sichtbarkeit auf Modulebene

**Stand vor dieser Runde:** es gab `import a.b`, `export { … }` je Datei und
die Umgebungsvariable `FIRNLIB`. Mehr nicht — kein Projektmanifest, keine
Abhängigkeiten, kein Bau-Werkzeug. `ABNAHME.md` Punkt 5 (`W1`,
„Paketverwaltung baut reproduzierbar") stand deshalb auf `[~]`.

**Was jetzt da ist:** ein Projektmanifest `firn.paket`, eine festgelegte und
deterministische Modul-Suchreihenfolge mit Fehlermeldungen für Zyklen,
fehlende Pakete und Namenskonflikte, Sichtbarkeit auf **Modulebene** als
echte Paketschnittstelle, und der Bau-Treiber `--paket`. Alles in **beiden**
Übersetzern — `firnc0` (Rust) und `firnc1` (Firn) — mit zeichengleichen
Meldungen.

---

## 1. Warum kein TOML — die Formatentscheidung

Die Wahl stand zwischen `firn.toml` und einem eigenen Format. Entschieden
wurde für ein eigenes, absichtlich winziges Zeilenformat namens
**`firn.paket`**. Die Gründe, der Reihe nach:

1. **Alles muss zweimal stehen.** Firn hostet sich selbst. Jede Zeile
   Manifestlogik existiert in `compiler/src/package.rs` (Rust) *und* in
   `lib/firnc1/package.fi` (Firn, **ohne libc**, nur Puffer und `syscall`).
   Ein TOML-Leser wäre in Firn mehrere tausend Zeilen: maskierte und
   mehrzeilige Zeichenketten, Reihungen, eingebettete Tabellen,
   Datumswerte, Zahlensyntax mit Unterstrichen, Hex/Oktal/Binär.
2. **Ein halbes TOML ist schlimmer als gar keins.** Eine Datei, die
   `firn.toml` heißt, weckt die Erwartung, dass jedes gültige TOML gelesen
   wird. Wird es nicht — dann lügt der Dateiname. Ein eigener Name mit
   eigener Endung weckt diese Erwartung erst gar nicht.
3. **Keine Fremdbibliotheken.** Das ist eine Grundentscheidung des Projekts
   (`SPEC.md`); ein `toml`-Crate in `firnc0` hätte in `firnc1` kein
   Gegenstück und die beiden Übersetzer sofort auseinanderlaufen lassen.
4. **Das Format soll langweilig sein.** Ein Manifest wird gelesen, bevor
   irgendetwas anderes passiert. Es darf keine überraschende Semantik haben.

Der Preis ist ehrlich zu nennen: **es gibt keine fertigen Werkzeuge** für
`firn.paket` (kein Editor-Highlighting, keine Bibliothek in anderen
Sprachen). Bei einem Format aus sechs Schlüsselwörtern, das mit `awk`
lesbar ist, ist das vertretbar.

## 2. Das Format

Eine Anweisung je Zeile: `schluessel wert [wert …]`. Trennzeichen sind
Leerzeichen und Tabulator, `#` leitet einen Kommentar bis zum Zeilenende
ein, Leerzeilen zählen nicht. **Keine Anführungszeichen, keine
Maskierungen** — ein Wert enthält deshalb weder Leerzeichen noch `#`.

```text
paket        demo            # Pflicht, genau einmal
version      0.1.0           # Pflicht, genau einmal, zahl.zahl.zahl
start        src/main.fi     # höchstens einmal; eine Bibliothek hat keinen
quelle       src             # 0..n; ohne Angabe gilt das Manifestverzeichnis
oeffentlich  geo punkt       # 0..n; ohne Angabe ist alles öffentlich
brauche      geo ../geo      # 0..n; Name + lokaler Pfad
```

Regeln, die wirklich geprüft werden:

| Angabe | Regel |
|---|---|
| `paket` | Bezeichner: Buchstabe oder `_` zuerst, dann Buchstaben, Ziffern, `_` |
| `version` | genau `zahl.zahl.zahl` |
| `start`, `quelle` | relativ, ohne `..`, nicht leer (ein Paket bleibt in seinem Verzeichnis) |
| `brauche` | Name wie `paket`; der Pfad **darf** hinausführen (`../geo`) |
| doppelte Angaben | Fehler — auch doppelte `quelle`, doppelte `oeffentlich`-Namen, doppelte Abhängigkeitsnamen |
| Abhängigkeit heißt wie das Paket selbst | Fehler |
| unbekannter Schlüssel | **Fehler**, kein stilles Überlesen — ein vertipptes `oeffentlih` würde sonst eine Schnittstelle öffnen, die niemand öffnen wollte |
| Name der Abhängigkeit ≠ `paket`-Zeile des Ziels | Fehler |

`start` ist **keine** Pflicht: ein Bibliothekspaket hat keinen
Einstiegspunkt. Erst `--paket` verlangt einen.

## 3. Suchreihenfolge

Für `import t1.t2…tn` in der Datei `F`, erster Treffer gewinnt:

```
1.  <verzeichnis von F>/t1/…/tn.fi          (wie bisher)
2.  <verzeichnis der Wurzeldatei>/t1/…/tn.fi (wie bisher)
3.  <paketwurzel>/<quelle>/t1/…/tn.fi        für jedes 'quelle' des Pakets,
                                             zu dem F gehört            NEU
4.  <abhängigkeit>/<quelle>/t2/…/tn.fi       wenn t1 der Name einer
                                             'brauche'-Abhängigkeit ist NEU
5.  $FIRNLIB/t1/…/tn.fi                      (wie bisher)
6.  <exe>/../lib/t1/…/tn.fi                  (wie bisher)
```

Bei `import geo` (nur ein Teil) und `geo` als Abhängigkeit wird
`<geo>/<quelle>/geo.fi` gesucht — das Modul mit dem Namen des Pakets ist
sein Hauptmodul.

**Zu welchem Paket eine Datei gehört**, entscheidet ihr Pfad: das Paket mit
der längsten passenden Wurzel. Deshalb darf ein Paket im Verzeichnis eines
anderen liegen. Dateien außerhalb aller Paketwurzeln (typisch: alles aus
`$FIRNLIB`) gehören zu keinem Paket; für sie entfallen Schritt 3 und 4 und
die Sichtbarkeitsprüfung.

**Das Manifest selbst** wird ohne `--paket` vom Verzeichnis der Quelldatei
aus nach **oben** gesucht, höchstens 64 Ebenen. Wird keines gefunden, ist
die „Paketwelt" leer, die Schritte 3 und 4 entfallen, und die Auflösung ist
Zeichen für Zeichen die von Runde 47. **Ohne Manifest ändert sich nichts** —
das ist der Grund, warum die 696 bestehenden Tests unverändert grün bleiben.

Pfade werden **rein lexikalisch** normalisiert (`a/./b/../c` → `a/c`);
symbolische Verweise werden nicht aufgelöst. Das muss so sein: `firnc1` hat
kein `realpath`, und ohne diese Regel wäre `--paket-info` rechnerabhängig.

## 4. Sichtbarkeit auf Modulebene

`oeffentlich a b c` in `firn.paket` ist die **Schnittstelle des Pakets**.
Führt ein Import in ein *anderes* Paket, gilt:

* Das Zielpaket muss eine eingetragene Abhängigkeit sein
  (`paket 'x' ist keine abhaengigkeit von paket 'y'`).
* Das Modul muss in dessen `oeffentlich`-Liste stehen
  (`modul 'x' ist in paket 'p' nicht oeffentlich`).

**Innerhalb** eines Pakets gibt es keine Schranke: `demos/packages/geo`
benutzt sein privates Modul `innen` und darf das.

**Fehlt `oeffentlich`, ist alles öffentlich.** Das ist bewusst dieselbe
Regel wie bei `export { … }` innerhalb einer Datei („fehlt sie, ist alles
sichtbar", `modules.rs`). Eine strengere Voreinstellung (ohne Liste ist
nichts öffentlich) war in der Abwägung: sie fängt vergessene Schnittstellen,
macht aber jedes unfertige Manifest zu einem unverständlichen Fehler und
wäre gegenüber der bestehenden `export`-Regel inkonsistent. Wer eine echte
Schnittstelle will, schreibt sie hin — `geo` tut es, `text` tut es nicht,
beide Fälle stehen im Beispielprojekt.

Die beiden Ebenen greifen ineinander: `oeffentlich` sagt, **welche Module**
ein Paket zeigt, `export { … }` sagt, **welche Namen** ein Modul zeigt.

## 5. Namenskonflikte

Das Modulsystem benennt Namen aus Nicht-Wurzelmodulen intern in
`modul__name` um; `modul` ist der Dateiname ohne Endung. Zwei
**verschiedene** Dateien mit demselben Namen fielen damit auf dieselbe
Umbenennung und hätten sich still überdeckt. Das ist jetzt ein Fehler:

```
error: namenskonflikt: modul 'hilfe' kommt aus zwei dateien
hinweis: '/…/anwendung/src/help.fi' und '/…/geo/src/help.fi'
```

Geprüft wird über die absoluten Pfade — zwei Schreibweisen derselben Datei
sind kein Konflikt. Die Prüfung läuft **nur mit Manifest**; ohne Manifest
bleibt es beim Verhalten von Runde 47 (sonst wäre die Änderung nicht
rückwärtskompatibel).

## 6. Der Bau-Treiber

```
firnc  --paket <verzeichnis> [-o ziel]     # Projekt übersetzen
firnc  --paket-info <verzeichnis>          # Manifest lesen und berichten
firnc1 --paket <verzeichnis> [-o ziel]     # dasselbe, in Firn
firnc1 --paket-info <verzeichnis>
```

`--paket` liest `<verzeichnis>/firn.paket`, lädt alle Abhängigkeiten,
prüft den Graphen auf Zyklen und übersetzt `start`. Ohne `-o` heißt das
Ergebnis wie das Paket:

```
$ firnc --paket demos/packages/app
$ ./demos/packages/app/anwendung
12 14 3
```

`--paket-info` gibt einen maschinenlesbaren Bericht aus, rein lexikalisch
aus dem übergebenen Verzeichnis gerechnet (kein `getcwd`, keine
symbolischen Verweise) — deshalb ist er auf beiden Übersetzern und auf
jedem Rechner derselbe:

```
$ firnc --paket-info demos/packages/app
paket anwendung
version 0.1.0
wurzel demos/packages/app
start demos/packages/app/src/main.fi
quelle demos/packages/app/src
brauche geo demos/packages/geo
brauche text demos/packages/text
```

**Inkrementell ist es nicht.** Der Treiber übersetzt immer alles. Das war
die bewusste Wahl aus dem Rundenziel („korrekt schlägt schnell"): ein
falscher Frische-Vergleich baut still den Stand von gestern, und genau
diese Falle hat dieses Projekt in den Runden 35, 45 und 46 schon dreimal
getroffen.

## 7. Das Beispielprojekt

`demos/packages/` — ein Programm und zwei Bibliotheken:

```
anwendung/   firn.paket   brauche geo, brauche text; quelle src
             src/main.fi  import geo · import geo.punkt · import text · import hilfe
             src/help.fi eigenes Modul aus 'quelle src'
geo/         firn.paket   oeffentlich geo punkt   (KEIN start: Bibliothek)
             src/geo.fi   öffentlich, benutzt intern 'innen'
             src/dot.fi öffentlich
             src/inner.fi PRIVAT — von außen nicht einbindbar
text/        firn.paket   ohne 'oeffentlich' → alles öffentlich
             src/text.fi
```

## 8. Was geprüft wird

`tools/packages/run.sh` (neu, in `test.sh` als Schritt 18): **21 Fälle**,
jeder durch **beide** Übersetzer, Fehlermeldungen Oktett für Oktett
verglichen. Positiv: Bau des Beispielprojekts (firnc0 und firnc1), Ausgabe
`12 14 3`, Benennung nach dem Manifest, `--paket-info`-Gleichheit, privates
Modul im eigenen Paket, Vorrang der Projektquelle, Manifestsuche nach oben,
zweites `quelle`-Verzeichnis, Regression ohne Manifest. Negativ: privates
Modul einer Abhängigkeit, Paket ohne `brauche`, Paketzyklus, Abhängigkeit
ohne Manifest, falscher Paketname, ungültige Version, unbekannter
Schlüssel, fehlende `paket`-Zeile, Namenskonflikt, Bibliothek ohne `start`,
Verzeichnis ohne Manifest, `--paket` zusammen mit einer Quelldatei.

Dazu **13 neue Rust-Modultests** in `compiler/src/package.rs` (11) und
`compiler/src/package_world.rs` (2): Format, Pflichtangaben, doppelte Einträge,
Stelligkeit, Pfadrechnen, Paketzugehörigkeit, `--paket-info`-Text und die
festen Fehlertexte.

## 9. Migrationshinweise

* **Bestehende Projekte müssen nichts tun.** Ohne `firn.paket` ist alles
  wie vorher; `FIRNLIB` gilt unverändert und wird weiterhin als Schritt 5
  durchsucht. `test.sh`, `tools/self_compare.sh` und
  `tools/fixpunkt.sh` setzen `FIRNLIB` selbst und laufen unverändert.
* **Ein Projekt umstellen:** `firn.paket` ins Wurzelverzeichnis legen
  (`paket`, `version`, `start`, `quelle`), Abhängigkeiten mit `brauche`
  eintragen, und in jeder Bibliothek `oeffentlich` schreiben. Danach baut
  `firnc --paket <verzeichnis>`.
* **Achtung beim Umstellen:** sobald ein Manifest existiert, greifen auch
  die Namenskonflikt- und Sichtbarkeitsprüfungen. Zwei gleichnamige Module
  in einer Übersetzung sind dann ein Fehler statt einer stillen
  Überdeckung — das ist der Zweck, kann aber beim ersten Lauf auffallen.
* **Kein Manifest im Wurzelverzeichnis dieses Repos.** Das ist Absicht: es
  würde die Auflösung aller Testprogramme im Repo verändern. Das Beispielprojekt
  liegt deshalb unter `demos/packages/`.

## 10. Offen (ehrlich)

* **Kein Netzwerk, keine Registry, keine Sperrdatei.** `brauche` kennt nur
  lokale Pfade. Reproduzierbarkeit über zwei Rechner (`ABNAHME.md` Punkt 5)
  ist damit **noch nicht** erfüllt; es fehlen Prüfsummen und eine
  `firn.sperre`.
* **Kein `firn build --locked`, keine Versionsauflösung.** `version` wird
  geprüft, aber nicht *verglichen* — zwei Pakete können nicht verschiedene
  Fassungen derselben Abhängigkeit verlangen.
* **Nicht inkrementell** (siehe 6). Es gibt weiterhin keine getrennten
  Objektdateien und keine Schnittstellendateien; übersetzt wird das
  Gesamtprogramm.
* **Symbolische Verweise** werden bei der Paketzugehörigkeit nicht
  aufgelöst. Ein Paket, das über einen Symlink erreicht wird, gilt als an
  der Symlink-Stelle liegend.
* **Fehler im Manifest zeigen keine Quelltextzeile** mit Markierung,
  sondern `datei:zeile: meldung`. Der Grund ist Gleichheit: `firnc1` hat
  die Diagnose-Maschinerie von `firnc0` nicht, und für die neuen Meldungen
  war Zeichengleichheit wichtiger als der Ausschnitt.
* **Die Sichtbarkeitsprüfung greift erst mit Manifest.** Wer ohne Manifest
  baut, hat keine Paketgrenzen — dann gibt es auch keine zu verletzen.

## 11. Abnahme (gemessen, 19.08.2026, Branch `r48-pakete`)

Gemessen wurde nach `rm -f .firnc1 .firnc2 .firnc3` — kein Binary aus einem
früheren Lauf war beteiligt.

| Prüfung | Ergebnis |
|---|---|
| `bash ./test.sh` | **PASS 697/697**, Exit 0 (Basis 696/696; +1 = Schritt 18) |
| ⤷ Schritt 18 `tools/packages/run.sh` | **21 bestanden, 0 fehlgeschlagen** |
| `bash tools/self_compare.sh` | **201 gleiches Verhalten · 0 abweichend · 0 fehlerhaft**, Exit 0 |
| `bash tools/fixpunkt.sh` | **Stufe 2 == Stufe 3, zeichengleich**, 2.070.856 Oktette, 364.765 Zeilen Assembler; Korpus: `.firnc2` verhält sich wie `firnc0`, Exit 0 |

Zum Vergleich der Ausgangsstand von Commit `a492d26`: `test.sh` 696/696,
`self_compare.sh` 201/0/0, `fixpunkt.sh` zeichengleich bei 2.065.816
Oktetten. Der Zuwachs von 5.040 Oktetten im selbst übersetzten Compiler ist
`lib/firnc1/package.fi` plus die Änderungen in `bin/firnc1.fi`.
