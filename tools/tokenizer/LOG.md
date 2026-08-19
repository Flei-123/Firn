# Auftragsprotokoll des Tokenizer-Treibers

Vertrag zwischen `lib/html/tokenize_main.fi` (Firn) und
`tools/tokenizer/harness.py` (Werkbank). Wer eine Seite aendert, aendert die
andere mit — sonst nichts.

## Eingabe (stdin, binaer, little-endian)

Ein Strom aus Auftraegen, ohne Kopf, ohne Ende-Marke:

| Feld | Typ | Bedeutung |
|---|---|---|
| `startzustand` | `u32` | 0 = Data, 1 = PLAINTEXT, 2 = RCDATA, 3 = RAWTEXT, 4 = Script data, 5 = CDATA section |
| `flaggen` | `u32` | Bitfeld, siehe unten. Bit 0 = XML-Anpassung. Alle anderen Bits sind 0 |
| `len_lasttag` | `u32` | Laenge von `lasttag` in Bytes |
| `lasttag` | `u8[len_lasttag]` | `lastStartTag` des Testfalls, WTF-8 |
| `len_input` | `u32` | Laenge von `input` in Bytes |
| `input` | `u8[len_input]` | Eingabetext, WTF-8 (haelt ungepaarte Surrogate) |

### Flaggen

| Bit | Name | Wirkung |
|---|---|---|
| 0 | `XML_MODUS` | XML-Anpassung des Tokenstroms nach dem Abschnitt "Coercing an HTML DOM into an infoset" des HTML-Standards. Der Harness setzt sie genau fuer die Faelle unter dem Schluessel `xmlViolationTests` (Datei `xmlViolation.test`, 4 Faelle); `--ohne-xml-modus` schaltet sie ab. |

Umgesetzt sind im XML-Modus (`lib/html/tokens.fi`, `xml_cp`, `out_json_cpbuf_text`,
`out_json_comment`), soweit der Tokenstrom betroffen ist:

* Zeichen, die XML 1.0 nicht kennt (C0-Steuerzeichen ausser TAB/LF, ungepaarte
  Surrogate, Nichtzeichen `U+FFFE`/`U+FFFF` jeder Ebene) werden in Text,
  Attributwerten und Kommentaren durch `U+FFFD` ersetzt.
* `U+000C` (FORM FEED) wird zu `U+0020` (SPACE) statt zu `U+FFFD`.
* Im Kommentartext wird zwischen zwei aufeinanderfolgende `U+002D` ein
  `U+0020` eingefuegt; endet der Kommentar auf `U+002D`, folgt ein `U+0020`.

Element-, Attribut- und DOCTYPE-**Namen** bleiben unangetastet: deren Anpassung
betrifft den DOM-Aufbau, nicht den Tokenstrom.

Ohne die Flagge verhaelt sich der Treiber exakt wie zuvor (reiner HTML-Standard).

Der Treiber liest bis zum Ende der Eingabe und beantwortet **jeden** Auftrag
mit **genau einer** Zeile.

## Ausgabe (stdout, reines ASCII, eine Zeile je Auftrag)

* Normalfall: das html5lib-Token-Array, z. B.
  `[["StartTag","div",{"a":"b"}],["Character","x"],["EndTag","div"]]`
* `["NICHT-UNTERSTUETZT"]`: der Tokenizer hat einen Zustand erreicht, den er
  noch nicht umsetzt, oder ein Mittel stand nicht zur Verfuegung (z. B. konnte
  die Namenstabelle der Zeichenreferenzen nicht angelegt werden, siehe
  `lib/html/entities.fi`). Der Harness zaehlt so etwas als **Fehlschlag** —
  niemals als "uebersprungen".

### Zweites Feld: die Parse-Fehler

Hinter dem Token-Array folgt ein **Tabulatorzeichen** (`0x09`) und danach die
Liste der Parse-Fehler dieses Auftrags als JSON, in der Reihenfolge des
Auftretens:

```
[]\t[{"code":"eof-in-tag","line":1,"col":5}]        (Eingabe: <div)
```

* `code` ist der WHATWG-Codename aus §13.2 "Parse errors"
  (`lib/html/error_codes.fi` fuehrt alle Namen, die der Tokenizer melden
  kann).
* `line` zaehlt ab 1, `col` ist die Spalte **hinter** dem Zeichen, das den
  Fehler ausgeloest hat — dieselbe Zaehlung wie in den `errors`-Listen der
  html5lib-Suite. Zeilenumbrueche sind die des normalisierten Eingabestroms
  (`\r\n` und `\r` sind da bereits `\n`).
* Ohne Fehler steht dort `[]`. Das Feld fehlt nie.
* Der Harness vergleicht es nur mit `--mit-fehlern`; ohne den Schalter zaehlt
  allein der Tokenstrom. `run.sh` weist **beide** Quoten aus.

Alle Zeichen ausserhalb von `0x20..0x7E` werden als `\uXXXX` geschrieben
(Codepunkte > 0xFFFF als Ersatzpaar). Damit ist die Zeile ASCII und `json.loads`
liefert auch ungepaarte Surrogate zurueck.

EOF erzeugt **kein** Token (html5lib fuehrt EOF nicht in `output`).
Aufeinanderfolgende Zeichen werden zu **einem** `Character`-Token verschmolzen.
