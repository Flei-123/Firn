# Runde TEMPO 7 — vier Ursachen, aus der Zaehlung geholt

Stand 21.09.2026, Zweig `xmm-ra`. Fortsetzung von TEMPO 6, wo die Frage
beantwortet wurde, WARUM Firn mehr Befehle braucht. Diese Runde raeumt die
naechsten vier Posten weg — jeden aus der Messung, keinen aus dem Gefuehl.

## Der Erzeuger: zwei neue Vektorbefehle

| Name | x86 | aarch64 |
|---|---|---|
| `__v128_store64` | `movlps` | `str d<n>` |
| `__v128_unpacklo32` / `hi32` (jetzt im Zuteilerweg) | `punpckldq` / `punpckhdq` | `zip1` / `zip2` |

`store64` schreibt nur die untere Haelfte. Zwei benachbarte `f32` abzulegen
ging vorher nur als volles Sechzehn-Oktett-Schreiben — und das zerstoert die
beiden Werte daneben. `punpck*` gab es schon als Intrinsic, aber es schickte
die ganze Funktion auf den Grundweg; jetzt gibt der Weg mit
Registerzuteilung es selbst aus. `tests/1617_simd_pack.fi` prueft beides
Spur fuer Spur.

## Die vier Posten im Dekoder

### 1. Acht Einzelzugriffe am Schleifenkopf -> zwei Ladungen (197,3 -> 194,6 Mio)

`synth` fuellt zu Beginn jedes Durchlaufs acht Werte in `zlin`, paarweise aus
`xl` und `xr`, und zwar aus BENACHBARTEN Plaetzen:

```text
xl: [A, A+1, .., ..]   xr: [A, A+1, .., ..]
punpckldq  ->  [xl[A], xr[A], xl[A+1], xr[A+1]]
```

Das ist genau die Reihenfolge der vier Zuweisungen. Statt acht Zugriffen mit
acht Indexrechnungen: ein Laden, ein Verzahnen, ein Schreiben. Die zweite
Haelfte landet an zwei getrennten Stellen — dafuer `store64` und einmal
`shuffle32(.., 0x0E)`.

### 2. Beiwerte auf dem Stapel (194,6 -> 188,5 Mio)

`scale_pcm4` begann mit

```firn
var konst: [f32; 12] = [0.5f, 32766.5f, ...]
var grenzen: [i32; 8] = [32767, ...]
```

— zwanzig Werte, jeder mit Laden und Schreiben, **bei 1,3 Millionen
Aufrufen**. Im Erzeugten waren das vierzig der rund hundert Befehle der
Funktion. Jetzt stehen sie als `static` einmal im Programm. Dasselbe in
`dct_ii_4`.

### 3. Nullsetzen fuer nichts (188,4 -> 184,0 Mio)

`l3_imdct36` legte seine beiden Hilfsfelder (`co`, `si`, je neun Werte)
INNERHALB der Bandschleife an — achtzehn Schreibbefehle je Band, obwohl beide
vor dem ersten Lesen vollstaendig beschrieben werden. Jetzt stehen sie vor
der Schleife.

### 4. Der heisse `mem_copy` (184,0 -> 180,9 Mio)

Der Zustand der Synthesefilterbank sind 3840 Oktette, zweimal je Granulat
kopiert. `rt.mem_copy` schafft acht Oktette je Durchlauf — richtig und
ueberall verwendbar, auch im Kernprofil ohne SSE. Fuer diesen einen Fall
steht jetzt ein `kopiere16` im Dekoder, das sechzehn nimmt. **`rt.mem_copy`
selbst bleibt unangetastet**: es muss auch dort laufen, wo es keine
SSE-Register gibt.

## Die Zahlen

| | Befehle (8 s Ton) |
|---|---|
| nach TEMPO 6 | 197,3 Mio |
| nach TEMPO 7 | **180,9 Mio** |
| dasselbe mit `--cpu=avx` | **163,6 Mio** |
| `minimp3` in C, `gcc -O2` | 74,8 Mio |

Abstand zu C: **2,4x** (mit AVX 2,2x). Zu Beginn der Tempo-Runden waren es
8,6x. Nach jedem einzelnen Schritt ist die PCM-Ausgabe bitgleich und der
Selbsttest gibt PASS 4/4.

## Wo die restlichen 2,4x sitzen

Funktion fuer Funktion, Firn gegen C:

| | Firn | C |
|---|---|---|
| `synth` | 52,6 Mio | 22,7 Mio |
| `l3_huffman` | 30,1 Mio | 12,1 Mio |
| `l3_imdct36` | 19,7 Mio | 12,9 Mio |
| `mp3_decode_frame` | 17,1 Mio | ~6 Mio |
| `dct_ii` | 17,6 Mio | 11,1 Mio |
| `l3_dct3_9` | 9,7 Mio | 8,0 Mio |

Die beiden letzten sind praktisch gleichauf — dort wird gerechnet, und das
kann Firn jetzt. Die beiden ersten haengen an derselben Sache, und die
Messung benennt sie eindeutig: **Registerdruck**. `FIRN_RA_STATS=1` sagt fuer
`synth` `maxlive=44`, fuer `l3_huffman` `maxlive=63` — bei **vierzehn**
Registern. Jeder Wert darueber liegt im Rahmen und wird vor jeder Benutzung
neu geholt; im Erzeugten stehen darum Ketten wie

```text
mov  -0xfd8(%rbp),%rcx     ; Zeiger neu holen
mov  (%rcx),%eax           ; Wert lesen
mov  %rax,-0xfe0(%rbp)     ; Zwischenwert ablegen
movslq -0xfe0(%rbp),%rax   ; und wieder holen
```

Was dagegen hilft, ist keine weitere Vektorarbeit, sondern Arbeit am
Zuteiler:

1. **Wiederherstellen statt Auslagern** (Rematerialisierung): ein Wert wie
   `15 - i` oder eine Vorzeichenerweiterung ist billiger neu gerechnet als
   aus dem Rahmen geholt.
2. **Lebensdauern an Aufrufen zerschneiden**, damit ein Zeiger, der einen
   Aufruf ueberlebt, nicht bei jedem Zugriff neu geladen wird.
3. **Kleinere Schleifenruempfe** — auch eine Aufgabe des Erzeugers
   (Aufteilen), nicht nur des Programmierers.
