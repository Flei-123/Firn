# Auftragsprotokoll des Tokenizer-Treibers

Vertrag zwischen `lib/html/tokenize_main.fi` (Firn) und
`tools/tokenizer/harness.py` (Werkbank). Wer eine Seite aendert, aendert die
andere mit — sonst nichts.

## Eingabe (stdin, binaer, little-endian)

Ein Strom aus Auftraegen, ohne Kopf, ohne Ende-Marke:

| Feld | Typ | Bedeutung |
|---|---|---|
| `startzustand` | `u32` | 0 = Data, 1 = PLAINTEXT, 2 = RCDATA, 3 = RAWTEXT, 4 = Script data, 5 = CDATA section |
| `len_lasttag` | `u32` | Laenge von `lasttag` in Bytes |
| `lasttag` | `u8[len_lasttag]` | `lastStartTag` des Testfalls, WTF-8 |
| `len_input` | `u32` | Laenge von `input` in Bytes |
| `input` | `u8[len_input]` | Eingabetext, WTF-8 (haelt ungepaarte Surrogate) |

Der Treiber liest bis zum Ende der Eingabe und beantwortet **jeden** Auftrag
mit **genau einer** Zeile.

## Ausgabe (stdout, reines ASCII, eine Zeile je Auftrag)

* Normalfall: das html5lib-Token-Array, z. B.
  `[["StartTag","div",{"a":"b"}],["Character","x"],["EndTag","div"]]`
* `["NICHT-UNTERSTUETZT"]`: der Tokenizer hat einen Zustand erreicht, den er
  noch nicht umsetzt. Der Harness zaehlt so etwas als **Fehlschlag** —
  niemals als "uebersprungen".

Alle Zeichen ausserhalb von `0x20..0x7E` werden als `\uXXXX` geschrieben
(Codepunkte > 0xFFFF als Ersatzpaar). Damit ist die Zeile ASCII und `json.loads`
liefert auch ungepaarte Surrogate zurueck.

EOF erzeugt **kein** Token (html5lib fuehrt EOF nicht in `output`).
Aufeinanderfolgende Zeichen werden zu **einem** `Character`-Token verschmolzen.
