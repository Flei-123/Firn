# TEMPO 13 -- Lebensdauern mit Luecken

Stand vorher: TEMPO 11/12, MP3-Dekoder 8 s Ton = 128,5 Mio Befehle
(callgrind). TEMPO 12 hatte gezeigt, dass SROA und die Sicherung am Aufruf
beide an derselben Stelle scheitern: ein Intervall war `[erstes Beruehren,
letztes Beruehren]` am Stueck, `l3_huffman` hatte 86 "gleichzeitig lebende"
Werte bei 14 Registern.

## Was gebaut ist

1. **Stuecke statt Intervall** (`regalloc.rs`, "LEBENSDAUER MIT LUECKEN").
   Jeder Wert bekommt je Block ein Stueck aus der Datenfluss-Lebendigkeit:
   ab Blockanfang, wenn er hineinlebt, bis Blockende, wenn er hinauslebt,
   sonst erstes/letztes Beruehren im Block. Zwei Werte stoeren sich nur,
   wenn sich Stuecke ueberschneiden. Der Scan haelt je Register die
   Intervalle, die es traegt; frei ist es, wenn kein Stueck kollidiert.
   Verdraengt wird nach Dichte (Gewicht je Stuecklaenge). Beide Klassen
   (Ganzzahl und SSE). `l3_huffman`: maxlive 86 -> 40.
   Abschalten: `FIRN_NO_HOLES=1`.
2. **Kopien mit mehreren Schreibstellen verschmelzen.** Die Sperre
   `defs != 1` im Coalescing faellt; `interferes` prueft ohnehin jede
   Schreibstelle (Chaitin). Damit werden die phi-Variablen der Schleifen
   verschmolzen. Alt: `FIRN_COAL_SINGLE=1`.
3. **SROA** (`sroa.rs`, vom Zweig `tempo12-versuch`), jetzt knapp positiv.
   `FIRN_NO_SROA=1`.
4. **Sicherung am Aufruf fuer Gleitzahlen** (vom Zweig `tempo12-versuch`),
   angepasst an die Luecken: gesichert wird nur, wo der Aufruf in einem
   Stueck des Werts liegt (`a <= p < z`). Die erste Fassung pruefte `a < p`
   und vergass damit Aufrufe, die am Blockanfang stehen -- `l3_huffman`
   gab falsche Werte aus. Ganzzahl-Variante (`FIRN_CS_INT=1`) bleibt aus:
   sie verliert weiterhin (124,3 statt 123,7 Mio). `FIRN_NO_CALLSAVE=1`.

## Messung (MP3, 8 s Ton, Befehle)

| Stand | Mio |
|---|---|
| TEMPO 11 | 128,5 |
| + Luecken | 126,4 |
| + Coalescing mehrere Schreibstellen | 124,9 |
| + SROA | 124,7 |
| + Sicherung am Aufruf (Gleitzahl) | **123,7** |

-3,8 % insgesamt; `l3_huffman` 20,0 -> 16,8 Mio (C: 12,1).

Benchmark-Bank (`bench/firn`, Befehle, Ausgabe je Programm gleich):
branchy -9,0 %, bytecount -14,2 %, jsonscan -13,0 %, statemachine -11,5 %,
bitmap -2,7 %, Rest +-0. Keine Verschlechterung.

Wanduhr nicht aussagekraeftig gemessen: die Maschine lief mit Last 10
(fremde Testlaeufe), beide Staende schwankten 0,28 -- 0,39 s.

## Ein Test war falsch

`tests/822_gc_weak_zeroed.fi` erwartete, dass `lives` einen
`gc_collect()` ueberlebt, obwohl `lives` danach nie mehr gelesen wird. Tot
ist tot -- der alte Zuteiler liess den Zeiger nur zufaellig in einem
Register stehen. Der Test liest `lives` jetzt nach der Sammlung.

## Geprueft

Alle Tests in `release-fast`, `release-safe`, `dev-fast` gruen (1018 Laeufe),
neuer Test `tests/1703_ra_holes.fi`. Fixpunkt Stufe 2 == Stufe 3
(793 453 Zeilen), `self_compare` 342 von 342 gleiches Verhalten, 0 abweichend. MP3 bitgleich (8 s und 60 s gegen `ref60.pcm`).

## Was als Naechstes lohnt

`synth` (35,0 Mio, C 22,7) ist die groesste Luecke. Die innere Schleife
`k < 8` hat feste Laenge und verzweigt je Durchlauf auf `k == 0` und
`k & 1` -- vollstaendiges Ausrollen konstanter kurzer Schleifen wuerde die
Verzweigungen wegfalten (gcc macht das). Geschaetzt 5-6 Mio.
