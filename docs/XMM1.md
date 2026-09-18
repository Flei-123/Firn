# Runde XMM 1 -- Fliesskomma ohne den Umweg ueber `rax`

Stand 18.09.2026, Zweig `xmm`. Vorgeschichte: `docs/TON2.md` (im Zweig `ton`)
hat gemessen, dass Firn bei Fliesskomma rund zehnmal langsamer ist als C,
bei Ganzzahlen dagegen nur knapp dreimal. Der Grund lag im Uebersetzer, nicht
in der Sprache -- und diese Runde raeumt den ersten, billigsten Teil davon ab.

## Was falsch war

`codegen_x86.rs` hat jeden Fliesskommawert ueber `rax` gefaehrt:

```asm
mov  eax, dword ptr [rbp-40]     ; Slot -> Ganzzahlregister
movd xmm0, eax                   ; Ganzzahlregister -> SSE-Register
```

Und auf dem Rueckweg dasselbe in umgekehrter Richtung. Das sind ZWEI
Anweisungen je Operand und zwei je Ergebnis -- bei einer Multiplikation
also sechs statt drei. Der Grund war historisch (Runde 71 hat Fliesskomma
ueberhaupt erst eingefuehrt und den kuerzesten sicheren Weg genommen), nicht
technisch: `movss`/`movsd` lesen und schreiben den Speicher unmittelbar.

## Was jetzt steht

```asm
movss xmm0, dword ptr [rbp-40]   ; eine Anweisung
...
movss dword ptr [rbp-48], xmm0
```

Geaendert sind genau zwei Funktionen (`load_xmm`, `store_xmm`). Alles
andere -- Rechnung, Reihenfolge, Rundung -- bleibt unberuehrt.

Ein Punkt musste dabei entschieden werden: `movss` schreibt nur vier Oktett
in einen acht Oktett breiten Platz, die oberen vier behalten ihren alten
Inhalt. Das ist zulaessig, weil ein `f32`-Platz ausschliesslich als `dword`
gelesen wird (`load_xmm` mit `single`, `cvtss2sd`, die Argumentuebergabe);
wer acht Oktett kopiert, kopiert die oberen mit, ohne sie je zu deuten.

## Messung

Alles auf derselben Maschine, `--opt-level=release-fast`:

| Messfall | vorher | nachher | C zum Vergleich |
|---|---|---|---|
| Messkern `f32` (2 Mio Durchlaeufe) | 0,50 s | **0,35 s** (-30 %) | 0,03 s |
| MP3-Dekoder, 60 s Audio | 2,07 s | **1,93 s** (-7 %) | 0,34 s |

Der Dekoder gewinnt weniger, weil er nicht nur rechnet, sondern auch
Huffman-Bits liest und Tabellen adressiert -- das ist Ganzzahlarbeit und war
nie betroffen.

Richtigkeit: `tools/ton_bauen.sh` -> PASS 4/4, die Ausgabe des Dekoders ist
weiterhin **bitgleich** zur C-Vorlage. Die Testreihe des Repos (`test.sh`)
laeuft unveraendert durch.

## Was noch aussteht (Runde XMM 2)

Der grosse Rest liegt weiter da, wo `docs/TON2.md` ihn benannt hat:
**Fliesskommawerte bleiben zwischen zwei Anweisungen nicht im Register.**
Jede Rechnung laedt neu aus dem Rahmenplatz und schreibt das Ergebnis
zurueck. Zwei Wege fuehren da raus:

1. **Der `xmm`-Wertecache des Basispfads auch fuer Skalare.**
   `simd.rs` hat ihn bereits vollstaendig -- mit Ruecknahmeplan, Ausspuelen
   an Blockgrenzen und Verdraengung -- aber fest auf `v128` (16 Oktett,
   `movdqa`) verdrahtet. Er braucht eine Breite je Eintrag (4/8/16) und die
   passende Bewegungsanweisung. Kleiner Eingriff, grosser Teil des Ertrags,
   weil die meisten Zwischenwerte im selben Block gelesen werden.
2. **Eine zweite Registerklasse im Linear Scan** (`regalloc.rs`).
   Der saubere Weg, aber der groessere: die Zuteilung muss zwei Pools
   fuehren, und die Ausgabe des RA-Pfads kennt bisher keine einzige
   Fliesskomma-Anweisung (Funktionen mit `f32`/`f64` sind dort nie
   angekommen). Dazu kommt, dass auf System V ALLE `xmm`-Register
   caller-saved sind: ein Wert, dessen Lebensdauer einen Aufruf kreuzt,
   braucht Sicherung oder bleibt im Speicher.

Reihenfolge: erst 1, dann messen, dann 2.
