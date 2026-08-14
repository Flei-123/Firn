# `bench/tokenizer` — Messlatte html5ever

Eigenes Cargo-Projekt, **niemals** eine Abhaengigkeit des Compilers
(`compiler/Cargo.toml` bleibt ohne externe Kisten). Es tokenisiert denselben
Eingabekorpus wie der Firn-Tokenizer mit `html5ever` und gibt Tokenzahl und
Zeit aus.

## Bauen und messen

```
cargo build --release --manifest-path bench/tokenizer/Cargo.toml
bash tools/tokenizer/durchsatz.sh .tokenizer-work/tokenize
```

`durchsatz.sh` erzeugt den Korpus (`tools/tokenizer/korpus.py`, rund 4 MB aus
den Eingaben der html5lib-Faelle), misst zuerst den Firn-Tokenizer und danach
— sobald dieses Binary gebaut ist — `html5ever` auf **derselben Datei**. Beide
Zeiten werden von aussen genommen (Prozessstart, Einlesen, Tokenisieren), also
gleich gemessen.

Ein Einzelaufruf geht auch von Hand:

```
bench/tokenizer/target/release/html5ever_bench .tokenizer-work/korpus.html
tokens=771264 zeichen=1810815 bytes=4275135 sekunden=0.396089
```

## Selbst gemessen (14.08.2026, geteilte Maschine unter Last)

Korpus: `.tokenizer-work/korpus.html`, 4,08 MB, 302.912 `&`, 109.696 `<`.
Je drei Laeufe, CPU-Zeit (`user`+`sys`, stabiler als die Wanduhr):

| | CPU-Zeit (bester Lauf) | Faktor |
|---|---:|---:|
| Firn (`lib/html/`, alle 6.810 Faelle bestanden bis auf 3) | 0,886 s | **2,6x** |
| html5ever 0.27, `--release` | 0,335 s | 1,0x |

Die Wanduhr-Werte aus `durchsatz.sh` schwanken auf dieser Maschine je nach
Fremdlast zwischen 1,8x und 5,0x — die CPU-Zeit oben ist der belastbarere
Wert. Das Ziel der Abnahme (≤ 2x) ist damit **verfehlt**; der echte Faktor
steht hier.

## Was verglichen wird — und was nicht

* Gleicher Korpus, gleiche Messart (Wanduhr um den ganzen Prozess).
* Beide Seiten fahren die volle Zustandsmaschine einschliesslich
  Zeichenreferenzen.
* Der Firn-Treiber schreibt zusaetzlich html5lib-JSON auf die Standardausgabe;
  `html5ever` zaehlt hier nur Token. Der Faktor faellt damit eher zu guenstig
  fuer Firn aus als zu schlecht — er wird trotzdem so ausgewiesen, wie er
  gemessen wurde.
* Die Maschine ist geteilt: unter Last schwanken **beide** Werte. Massgeblich
  ist das Verhaeltnis aus einem Lauf, nicht der absolute MB/s-Wert.
