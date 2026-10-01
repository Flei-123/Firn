# Round TEMPO 10 — three answers to the same question: who owns the register?

State 23.09.2026, branch `xmm-ra`. The starting point was the count after
TEMPO 9 — not per function, but **per pattern over the whole program**
(`/tmp` tool, template in TEMPO 9):

| Pattern | Instructions | Share |
|---|---|---|
| **`movaps xmm,xmm`** (SSE two-operand form) | **16.3 M** | **11.1 %** |
| **fetch from the frame** | **10.3 M** | **7.0 %** |
| `mov rA,rB` + `add $K` (instead of `lea`) | 2.1 M | 1.4 % |
| `mov $K,r` + `imul` (instead of `lea`/`shl`) | 0.8 M | 0.6 % |

The first two are the same question in two disguises: **who gets a
register, and how long does he keep it.** This round gives three answers.

---

## 1. The two-operand form is also a copy (16.3 → 8.7 M)

SSE has no three-operand form: `mulps d, s` means `d = d * s`. The
generator therefore first copies the first operand into the destination and then calculates:

```text
movaps %xmm13,%xmm7
mulps  %xmm12,%xmm7
```

If `%xmm13` dies at this multiplication, the copy is for nothing — `a`
and `d` may have the same register. That is **exactly the question that the
merging from TEMPO 8 already answers**, only for a different kind of
instruction. The candidate search now additionally takes

* `Op::Bin(+,-,*,/)` with a floating-point type,
* `Op::Un(Neg)` with a floating-point type,
* every `Op::Simd` that calculates in its first operand
  (`addps`, `subps`, `mulps`, the three comparisons, `pcmpgtd`, `pand`,
  `pandn`, `por`, `pxor`, `paddd`, `psubd`, `punpckldq/hi`).

**The condition had to be sharpened for this**, and that was the instructive
part. For a real copy it said "the source has exactly one reader".
For the two-operand form that is too strict: in the hot loop of the
synthesis filter bank the same vector is multiplied TWICE, and the
second time it dies — exactly there the destination may inherit its register. The
right question is not how often the value is read, but whether it **still lives
after this instruction**. With "exactly one reader": 146.3 → 141.6 M. With
"dies here": 146.3 → **138.8 M**.

The same relaxation has applied since to real copies too: the Chaitin question
("do they interfere?") is the complete condition, "exactly one reader" was
only caution. `FIRN_COAL_ENG=1` restores the cautious version
(measured 1.0 M worse).

In addition, **two frame slots** are now also merged, but
**only for integers**: for floating-point numbers `fp_handover` puts a value without a
register into `xmm2` instead of the frame, and the copy would then have believed that
source and destination were the same slot. TEMPO 8 already died of exactly this trap
once (`tests/1182_layout_float_probe.fi`).

---

## 2. Density instead of sum (137.7 → 136.7 M, and the door opener)

When no register is free, the linear scan displaces the active interval
with the **smallest weight** (uses times loop depth). That
favours long intervals: a value with fifty uses scattered over the whole function
beats one with three in the innermost loop
— although the first occupies its register the whole time and the second would need it
only briefly.

Now **weight per length** is compared. A one-line change,
`FIRN_RA_SUMME=1` restores the old answer.

---

## 3. Cutting lifetimes — and why the first attempt was wrong

The linear scan knows ONE interval and ONE slot per value. A pointer that is
set at the beginning and needed once more at the end occupies its
register over the whole function — or none, and then in the
hot loop in between it is fetched from the frame at EVERY use.

The textbook answer is *live range splitting*. In the allocator itself that would be
a rebuild of every output site (`loc(v)` would have to depend on the POSITION).
The same result is obtained without this rebuild by making the piece
a separate VALUE: a copy into the pre-header of the loop, and the
body reads the copy.

**First attempt: a pass in the optimiser.** It cut every value that is read several times in
a loop and not written there. Measured:
137.7 → **140.4 M, so two percent WORSE**. The reason is obvious in hindsight: where the new value gets
only a frame slot, you pay for the copy in the pre-header and gain nothing — the body
then simply reads the other slot. Raising the threshold did not help (min=8: still
138.0).

**Second attempt: cut after you know where it pinches.**
`emit_func_ra` now allocates once, asks `split::nach_zuteilung` which
values really **landed in the frame AND are read several times in a loop**,
cuts only these, and allocates once more. If in the second allocation not a single one of the new values
gets a register, the
result is discarded — then the cut costs only translation time and not a
single bit in the program.

**And then exactly this happened: zero registers, every time.** Two causes,
found one after the other:

1. **The merging closed the cut at once again.** `%v2 = copy %v`
   is a perfect candidate for TEMPO 8. For that there is now
   `Func::no_coalesce` — a list of values that must not be merged,
   filled solely by the allocator for its own second version.
2. **Even after that: zero.** And that was no bug, but the answer of the
   allocator. With the SUM as the yardstick a short interval with
   three uses loses against a long one with fifty — always. Only with the
   density from point 2 does the cut win its registers.

The two changes are thus connected: **density without the cut brings
1.0 M, the cut without density brings nothing, both together 2.3 M.**

Threshold: three readers in the loop (`FIRN_SPLIT_MIN`), switchable off with
`FIRN_NO_SPLIT=1`.

---

## Die Zahlen

MP3-Dekoder, 8 s Ton, `release-fast`, `valgrind --tool=callgrind`:

| | Befehle |
|---|---|
| nach TEMPO 9 | 146,3 Mio |
| + Zweioperandenform | 138,8 Mio |
| + aggressives Verschmelzen, Plaetze | 137,7 Mio |
| **+ Dichte + Schnitt (TEMPO 10)** | **135,4 Mio** |
| dasselbe mit `--cpu=avx` | **127,1 Mio** |
| `minimp3` in C, `gcc -O2` | 74,8 Mio |

Und die Wanduhr, 60 s Ton, kleinste von elf Laeufen, Ausgabe nach
`/dev/null`:

| | Zeit |
|---|---|
| Firn nach TEMPO 8 | 0,15 s |
| **Firn jetzt** | **0,13 s** |
| Firn mit `--cpu=avx` | 0,12 s |
| C, `gcc -O2` | 0,06 s |
| dasselbe C ohne Auto-Vektorisierung | 0,09 s |
| dasselbe C mit `-O0` | 0,36 s |

Also **2,2x hinter `gcc -O2`** (mit AVX 2,0x) und **1,4x** hinter demselben
C ohne Auto-Vektorisierung. Zu Beginn der Tempo-Runden waren es 8,3x.

Die Bank (`bench/firn/`) zeigt keine Verschlechterung: zehn von elf
Programmen Befehl fuer Befehl gleich, `jsonscan` −1,8 %. Diese Programme
haben kaum Registerdruck — dort gibt es nichts zu verteilen.

Die Uebersetzung von `bin/firnc1.fi` dauert 4,9 statt 4,5 Sekunden (+7 %);
das ist die zweite Zuteilung fuer Funktionen mit Schleifen und Ueberlauf.

## Geprueft

Alle 318 Testprogramme in vier Baustufen, `self_compare` 339 von 339 mit
gleichem Verhalten (0 abweichend, 0 fehlerhaft), und der **Fixpunkt**: Firn
uebersetzt sich selbst, Stufe 2 und Stufe 3 zeichengleich (793 453 Zeilen
Assembler). Die PCM-Ausgabe des Dekoders ist nach jedem einzelnen Schritt
bitgleich, in beiden CPU-Stufen. Die drei roten Punkte des Laufs
(`tools/js/run.sh` an einer fehlenden `testdata/test262/subset.sha256`,
`tools/fmt/run.sh` an ungeformten Dateien in `lib/fui/`,
`tools/english/check.sh` an Bezeichnern in `lib/fui/`) sind aelter als diese
Runde; die Bezeichner IM UEBERSETZER sind seit dieser Runde alle englisch
(25 -> 18 gemeldete, keiner mehr unter `compiler/src/`).

## Was jetzt noch dasteht

| Muster | Befehle |
|---|---|
| aus dem Rahmen holen | 10,0 Mio |
| `movaps xmm,xmm` (die Quelle lebt wirklich weiter) | 8,7 Mio |
| `mov rA,rB` + `add $K` | 2,0 Mio |
| `mov $K,r` + `imul` | 0,8 Mio |

Die letzten beiden sind der naechste, einfache Schritt: `lea` darf auch mit
32-Bit-Ziel benutzt werden (`lea %edx,0x1(%r10)` rechnet die Adresse in 64
Bit und schneidet auf 32 — genau die Arithmetik modulo 2^32, die ein 32-Bit
`add` macht), und bei einer vertauschbaren Rechnung gehoert die Konstante
nach rechts, damit die vorhandene Faltung sie sieht.
