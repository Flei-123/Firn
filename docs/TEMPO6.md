# Runde TEMPO 6 — zwei Befehle, die C gar nicht erst schreibt

Stand 21.09.2026, Zweig `xmm-ra`. Diese Runde beantwortet eine Frage mit
Messung statt Meinung: **warum braucht Firn fuer dieselbe Arbeit rund
dreimal so viele Befehle wie C?**

## Die Messung zuerst

Derselbe Tondekoder, dieselben 8 s Ton, Funktion fuer Funktion
(`callgrind`, Firn vor dieser Runde gegen `minimp3` mit `gcc -O2`):

| Funktion | Firn | C | Faktor |
|---|---|---|---|
| `synth` | 59,1 Mio | 22,7 Mio | 2,6x |
| `l3_huffman` | 30,1 Mio | 12,1 Mio | 2,5x |
| `l3_imdct36` | 27,1 Mio | 12,9 Mio | 2,1x |
| `dct_ii` | 18,7 Mio | 11,1 Mio | 1,7x |
| `l3_dct3_9` | 12,4 Mio | 8,0 Mio | 1,6x |

Es gibt also **keine einzelne grosse Ursache** — es ist ueberall derselbe
Faktor. Deshalb lohnt der Blick auf die kleinste, uebersichtlichste
Funktion: `l3_dct3_9`, gerade Rechnung ohne Schleife und ohne Adressspiele.
Firn 155 Befehle, C 109. Der Unterschied steht im Erzeugten woertlich da:

**Firn (vorher)**
```text
    mov   r8, rdi
    mov   rcx, r8              <- Kopie der Adresse
    movss xmm15, [rcx]
    lea   r9, [r8+8]
    mov   rcx, r9              <- noch eine
    movss xmm14, [rcx]
    ...
    mov   eax, 0x3f000000      <- die Konstante 0,5 zur Laufzeit bauen
    movd  xmm10, eax           <- und ein Register dafuer verbrauchen
    mulss xmm9, xmm10
```

**C**
```text
    movss xmm2, [rdi+0x18]           <- Abstand direkt im Befehl
    mulss xmm8, [rip + 0x4ecf]       <- Konstante aus .rodata als Operand
```

Zwei Sachen, die C gar nicht erst schreibt. Genau die sind jetzt weg.

## 1. Die Adresse steht schon in einem Register

`addr_mem` -- der Weg, ueber den jeder Fliesskomma- und Vektorzugriff seinen
Speicheroperanden bekommt -- hat die Adresse **immer** erst nach `rcx`
kopiert:

```rust
self.load_full(e, "rcx", v);
"[rcx]".to_string()
```

Liegt sie bereits in einem Register, ist sie der Operand. Sechs Zeilen.

**Gemessen: 209,6 -> 198,4 Mio Befehle (-5,4 %).**

## 2. Gleitzahl-Konstanten gehoeren in `.rodata` (`fpool.rs`)

SSE hat keine Form mit unmittelbarer Konstante. Bis hierher baute Firn jede
Gleitzahl-Konstante zur Laufzeit aus zwei Befehlen auf — und hielt sie
danach in einem `xmm`-Register fest, solange sie gebraucht wurde. In
`l3_dct3_9` sind das **sechs Konstanten, also sechs der zwoelf Register**,
die der Zuteiler ueberhaupt zu vergeben hat.

Jetzt gibt es einen Vorrat je Uebersetzungseinheit: ein Eintrag je Bitmuster
und Breite, `rip`-relativ adressiert, und die Konstante ist der
SPEICHEROPERAND der Rechnung (`mulss xmm8, dword ptr [rip + .Lfconst3]`).
Ein Befehl, kein Register.

`l3_dct3_9` faellt damit von 155 auf 131 Befehle.

### Der Fehler, den die Probe gefunden hat

Nach der Aufloesung der `phi`-Knoten ist FIR **nicht mehr SSA**: ein Wert
darf mehrmals geschrieben werden. Eine Schleifenvariable, die bei `1.0`
beginnt, hat den `const` im Vorkopf und eine Kopie auf der
Rueckwaertskante — beide auf denselben Wert. Wer so einen Wert in den Vorrat
legt, liest in der Schleife fuer immer die `1.0`.

Gemessen: `math.powi(2.0, 10)` gab **1.0 statt 1024.0**, und `math.exp(0.0)`
gab 0. Die Regel steht schon einmal im selben Modul (`immediate_consts`,
Runde 92) und gilt hier genauso: in den Vorrat kommt nur, was **genau eine
Schreibstelle** hat. Zweiter Fund derselben Probe: die Kopie eines
Gleitzahlwertes fragte nach `place()` statt nach `fpo()` — eine Konstante aus
dem Vorrat hat aber gar keinen Platz.

## Die Zahlen

| | Befehle (8 s Ton) |
|---|---|
| nach TEMPO 5 | 209,6 Mio |
| + Adresse direkt als Operand | 198,4 Mio |
| + Konstanten in `.rodata` | **197,3 Mio** |
| dasselbe mit `--cpu=avx` | **180,2 Mio** |
| `minimp3` in C, `gcc -O2` | 74,8 Mio |

PCM bitgleich, Selbsttest PASS 4/4, in beiden CPU-Stufen.

Im Dekoder bringt der Vorrat wenig (dort stehen die Konstanten laengst in
Registern und der Druck war nicht der Engpass) — in rechenlastigem Code wie
`std.math` ist er der Unterschied zwischen "sechs Register weg" und "kein
Register weg".

## Und was bleibt von den 2,6x?

Fuer `synth`, die heisseste Funktion, sagt die Zaehlung der erzeugten
Befehle, wo die Arbeit liegt: **204 `mov`, 31 `add`, 27 `lea`** gegen
26 `movaps` und die gepackten Rechnungen. Also Adressrechnung, nicht Rechnen.
C kommt dort mit weniger aus, weil `gcc` zwei Dinge tut, die Firn noch nicht
tut:

1. **Zeiger weiterschalten statt Adressen neu rechnen** (Induktionsvariablen
   mit Staerkereduktion auf der ADRESSE, nicht nur auf der Multiplikation —
   die allein hat Runde TEMPO 3 gemessen und wieder verworfen).
2. **Lebensdauern an Aufrufen zerschneiden**, damit ein Zeiger, der einen
   Aufruf ueberlebt, nicht bei jedem Zugriff neu aus dem Rahmen geholt wird.

Beides ist Arbeit am Zuteiler, nicht an der Rechnung — und der naechste
grosse Brocken.
