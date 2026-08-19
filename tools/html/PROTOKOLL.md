# Auftragsprotokoll des Baumaufbau-Treibers

Vertrag zwischen `lib/browser/parse_main.fi` (Firn) und
`tools/html/harness_baum.py` bzw. `tools/html/realweb.py` (Werkbank). Wer eine
Seite ändert, ändert die andere mit — sonst nichts.

## Eingabe (stdin, binär, little-endian)

Ein Strom aus Aufträgen, ohne Kopf, ohne Ende-Marke:

| Feld | Typ | Bedeutung |
|---|---|---|
| `len_input` | `u32` | Länge von `input` in Bytes |
| `input` | `u8[len_input]` | Das HTML-Dokument, WTF-8 (hält ungepaarte Surrogate) |

Zeilenenden werden vom Treiber normalisiert (`\r\n` und `\r` → `\n`), genau
wie beim Tokenizer-Treiber (`lib/html/tokenize_main.fi`).

## Ausgabe (stdout, UTF-8)

Je Auftrag der Baum im **`.dat`-Format der html5lib-`tree-construction`-Tests**,
danach eine Zeile `#ENDE`:

```
| <!DOCTYPE html>
| <html>
|   <head>
|   <body>
|     <p>
|       "x"
#ENDE
```

Regeln des Formats (`lib/browser/write.fi`):

* Jede Zeile beginnt mit `| `, danach **zwei Leerzeichen je Ebene**. Die Kinder
  des Dokuments stehen auf Ebene 0.
* Element: `<name>`. Außerhalb des HTML-Namensraums mit Präfix: `<svg circle>`,
  `<math mi>`.
* Attribute stehen **vor** den Kindern, eine Ebene tiefer als ihr Element, und
  sind **nach dem Namen sortiert** — der DOM behält die Reihenfolge des
  Quelltexts, nur die Ausgabe sortiert. Namensraum-Attribute mit Präfix:
  `xlink href="…"`.
* Text: `"daten"`, ohne Maskierung.
* Kommentar: `<!-- daten -->`.
* Doctype: `<!DOCTYPE name>` bzw. `<!DOCTYPE name "public" "system">`.

Kommt der Treiber nicht durch, steht **vor** dem Baum eine Zeile
`#KAPUTT <code>`; der Läufer zählt den Fall dann als Fehlschlag. Die Codes
stehen in `lib/browser/driver.fi` (`lauf_dokument_bauen`):

| Code | Bedeutung |
|---|---|
| 1 | der Tokenizer erreichte einen Zustand, den er nicht umsetzt |
| 2 | der Baumaufbau brach ab (Stapelüberlauf oder Speicher aus) |
| 3 | zu viele Tokenizer-Umschaltungen (kann nur ein Fehler im Code sein) |
| 9 | kein Dokument (Speicher aus) |

## Warum der Umweg über einen Puffer

Der Tokenizer ist vollständig `#[no_gc]` (SPEC §3.5.4) — er darf keine Funktion
aufrufen, die GC-Speicher anfordert. Der Baumaufbau tut genau das (jeder Knoten
ist ein GC-Objekt). Zwischen beiden steht deshalb ein **binäres Tokenprotokoll**
(`lib/html/tokens.fi`, `tb_*`; gelesen von `lib/browser/token_stream.fi`).

Jeder Satz trägt neben dem Token die **Quellposition unmittelbar hinter dem
Token**. Die braucht der Baumaufbau, um den Tokenizer bei `<title>`, `<style>`,
`<script>`, `<textarea>` und `<plaintext>` in einen anderen Startzustand zu
schicken (WHATWG „generic raw text element parsing algorithm"). Wie das
gemacht wird — und was es kostet — steht im Kopf von `lib/browser/driver.fi`.
