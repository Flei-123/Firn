# Runde TON 2 -- warum Firn hier zehnmal langsamer ist als C

Stand 18.09.2026, Zweig `ton`. Die Frage der Runde war: **kann Firn das Tempo
von C erreichen?** Antwort vorweg: ja, aber nicht durch besseren Firn-Code --
die Bremse sitzt im Uebersetzer, und sie ist genau benannt.

## 1. Wo die Zeit hingeht (gemessen, nicht geschaetzt)

60 s Audio (MPEG-1, 44,1 kHz, Stereo, 192 kbit/s), `--opt-level=release-fast`.
Gemessen durch Weglassen einzelner Stufen:

| Stufe | Zeit | Anteil |
|---|---|---|
| alles | 2,34 s | 100 % |
| ohne Synthesefilterbank | 0,80 s | -- |
| **daraus: Synthese (DCT-II + Fensterung)** | **1,54 s** | **66 %** |
| ohne Synthese und ohne IMDCT | 0,70 s | -- |
| daraus: IMDCT | 0,10 s | 4 % |
| Rest (Rahmen, Skalenfaktoren, Huffman) | 0,70 s | 30 % |

Die Optimierstufen selbst:

| Stufe | Zeit |
|---|---|
| `dev` | 10,44 s |
| `dev-fast` | 3,32 s |
| `release-safe` | 2,97 s |
| `release-fast` | 2,46 s |

## 2. Was am Quelltext geholfen hat -- und was nicht

**Geholfen (11 %):** die heisse Schleife der Synthesefilterbank ausgerollt,
die vier Summen als einzelne Variablen statt als Feld, die Adresse einmal je
`(i,k)` statt viermal je Stimme: **2,34 s -> 2,07 s**, Ausgabe weiterhin
bitgleich.

**Nicht geholfen (schlechter!):** die Vorzeichen-Verzweigung in `adr4` durch
einen verzweigungsfreien Bitmuster-Trick (`i64` -> `u64` ueber den Speicher)
ersetzen: **2,34 s -> 3,21 s**. Der Umweg ueber Speicher kostet mehr als der
gut vorhersagbare Sprung. Notiert, damit es niemand nochmal probiert.

## 3. Die eigentliche Ursache: der Registerzuteiler kennt kein Fliesskomma

`compiler/src/regalloc.rs` (Linear Scan, 4946 Zeilen) sagt es selbst:

```rust
// FLOATING POINT: this allocator knows only the integer registers.
if f.val_types.iter().any(|t| t.is_float()) {
    return Some("f64 in the value set".into());
}
```

**Jede Funktion, in der auch nur ein `f32`/`f64` vorkommt, faellt auf den
Basis-Pfad zurueck** -- und der gibt jedem Zwischenwert einen eigenen
Stack-Platz. Der erzeugte Code sieht dann so aus (aus der innersten Schleife
des Messkerns):

```asm
movsxd rax, dword ptr [rbp-1168]
mov    qword ptr [rbp-240], rax
mov    rax, qword ptr [rbp-240]
mov    rcx, qword ptr [rbp-248]
imul   rax, rcx
mov    qword ptr [rbp-256], rax
```

Sechs Speicherzugriffe fuer eine Multiplikation. Kein Register haelt etwas
laenger als eine Anweisung.

### Die Gegenprobe, die es beweist

Derselbe Schleifenbau zweimal, einmal in `f32` und einmal in `i64`
(`lib/ton/bench_main.fi`, `lib/ton/bench_int_main.fi` gegen
`bench/kern.c`, `bench/kern_int.c`, beide `gcc -O2`), je 2 Mio Durchlaeufe,
identisches Ergebnis auf beiden Seiten:

| Rechenart | C | Firn | Faktor |
|---|---|---|---|
| `f32` (ohne Registerzuteilung) | 0,03 s | 0,50 s | **~17x** |
| `i64` (**mit** Registerzuteilung) | 0,03 s | 0,085 s | **~2,8x** |

Damit ist die Frage beantwortet: Firn ist **nicht** grundsaetzlich zehnmal
langsamer. Mit Registerzuteilung liegt es beim Faktor drei -- und der Rest
davon ist gcc's Vektorisierung, nicht die Sprache. Ohne Registerzuteilung
(also bei jedem Fliesskomma-Programm) ist es Faktor 17.

## 4. Was daraus folgt

**Die naechste sinnvolle Arbeit ist keine Ton-Runde, sondern eine
Uebersetzer-Runde: eine zweite Registerklasse (`xmm0`-`xmm15`) im Linear
Scan.** Umrisse:

1. Intervalle pro Registerklasse trennen (Ganzzahl / Fliesskomma), der
   bestehende Linear Scan laeuft zweimal ueber dieselbe Nummerierung.
2. Ausgabe fuer `f32`/`f64` in Registern: `movss`/`movsd`, `addss`, `mulss`,
   `comiss` statt Speicher-Speicher-Verkehr.
3. Spill-Plaetze mit 4 bzw. 8 Oktett, Aufrufkonvention beachten (alle `xmm`
   sind auf System V **caller-saved**, ueber einen `call` hinweg also nur
   nach Sicherung).
4. Der Basis-Pfad bleibt als Rueckfallebene erhalten; die Schutzklausel
   `supported()` gibt Fliesskomma frei, sobald der Pfad es wirklich kann.

Nutzen weit ueber diesen Dekoder hinaus: alles, was in Firn rechnet --
Grafik, Physik, Layout in Certus, das Spezies-Projekt -- laeuft heute ohne
Registerzuteilung.

## 5. Stand des Dekoders nach dieser Runde

* 2,07 s fuer 60 s Audio = **29x Echtzeit**, weiterhin auf allen Testfaellen
  **bitgleich** (`tools/ton_bauen.sh` -> PASS 4/4).
* Die ausgerollte Synthese steht in `lib/ton/mp3.fi`; die Messkerne liegen in
  `lib/ton/bench_main.fi` und `lib/ton/bench_int_main.fi`.
