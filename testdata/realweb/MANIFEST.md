# testdata/realweb/ — acht echte HTML-Seiten (Messkorpus B)

Am **14.08.2026** mit `curl -sSL` geholt und **unveraendert** abgelegt (kein
Umformatieren, kein Kuerzen, keine Ersetzungen). Sie dienen ausschliesslich der
Durchsatzmessung (`tools/tokenizer/throughput.sh`, Korpus `realweb`) — sie sind
keine Testdaten fuer die Quote und beeinflussen die html5lib-Bilanz nicht.

Warum es diesen zweiten Korpus gibt: der Korpus aus den html5lib-Eingaben ist
absichtlich pathologisch (fast nur Grenzfaelle, extrem viele Zustandswechsel je
Byte, kaum lange Textlaeufe). Er misst den schlechtesten Fall. Erst zusammen
mit echten Seiten ergibt der Vergleich gegen `html5ever` ein ehrliches Bild;
beide Faktoren stehen in `README.md` und `ACCEPTANCE.md`.

| Datei | Bytes | Quelle (URL) |
|---|---:|---|
| `hackernews.html` | 34.320 | https://news.ycombinator.com/ |
| `rustdoc_vec.html` | 951.960 | https://doc.rust-lang.org/std/vec/struct.Vec.html |
| `w3c_html52.html` | 154.701 | https://www.w3.org/TR/html52/ |
| `whatwg_parsing.html` | 774.608 | https://html.spec.whatwg.org/multipage/parsing.html |
| `wikipedia_de_html.html` | 266.414 | https://de.wikipedia.org/wiki/HTML |
| `wikipedia_en_linux.html` | 978.810 | https://en.wikipedia.org/wiki/Linux |
| `wikipedia_en_rust.html` | 1.009.516 | https://en.wikipedia.org/wiki/Rust_(programming_language) |
| `wikipedia_en_www.html` | 761.483 | https://en.wikipedia.org/wiki/World_Wide_Web |

**Summe: 4.931.812 Bytes (4,70 MB)**, als Korpus mit `\n` verbunden:
4.931.819 Bytes (4,70 MB).

Die Seiten sind gemischt gebaut: Wikipedia-Artikel (viel Text, viele Links,
Tabellen), der WHATWG-Standard (sehr viele kurze Inline-Elemente), rustdoc
(sehr viele Attribute und `<code>`-Fragmente), W3C-Uebersicht (Listen), Hacker
News (alte Tabellenlayouts, teils nicht geschlossene Tags). Alle acht Seiten
enthalten benannte und numerische Zeichenreferenzen sowie `<script>`- und
`<style>`-Bloecke, also RAWTEXT/ScriptData-Zustaende.

Nachzaehlen:

    ls -l testdata/realweb/*.html
    python3 tools/tokenizer/korpus.py /tmp/k.html /tmp/k.auftrag --quelle realweb
