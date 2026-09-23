# TEMPO 11 -- `lea` in 32 Bit, Konstante nach rechts, Schleifenrotation

Messgrundlage wie immer: MP3-Dekoder, 8 s Ton, Befehle nach callgrind,
`release-fast`. PCM nach jedem Schritt bitgleich zur Goldausgabe.

| Schritt | Befehle |
|---|---|
| nach TEMPO 10 | 135,4 Mio |
| `lea` mit 32-Bit-Ziel + Konstante nach rechts | 132,2 Mio |
| Schleifenrotation (nur Vergleich im Kopf) | 129,5 Mio |
| Schleifenrotation (bis zu drei Rechnungen vor dem Vergleich) | **128,5 Mio** |
| dasselbe mit `--cpu=avx` | **120,2 Mio** |
| C, `gcc -O2` | 74,8 Mio |

## 1. `lea` auch mit 32-Bit-Ergebnis (`regalloc.rs`, `lea_possible`)

`i + 1` auf einem `i32` war `mov rdx,r10` + `add edx,1`. `lea r32,[..]`
bildet die Adresse in 64 Bit und legt die unteren 32 ab; die unteren 32 Bit
einer Summe haengen nur von den unteren 32 Bit der Summanden ab. Also ist
es bitgleich. Die Verschiebung im `lea` ist vorzeichenbehaftet, ein
32-Bit-Immediate wird darum modulo 2^32 umgedeutet (`0xFFFFFFFF` -> `-1`).
Test: `tests/1701_lea_32bit.fi` (Ueberlauf, Unterlauf, grosse `u32`-
Konstante, `u8`/`i16`).

## 2. Konstante nach rechts (`peephole.rs`)

`4 * i` wurde `mov $4,r10; imul r8d,r10d`, weil der Erzeuger nur `imm(b)`
fragt. Fuer vertauschbare Ganzzahl-Operationen (`+ * & | ^`) steht die
Konstante jetzt rechts.

## 3. Schleifenrotation (`regalloc.rs`, `emit_block`, `Term::Br`)

Die Messung (Befehle je Adresse) zeigte 4,46 Mio unbedingte Rueckspruenge,
fast alle `jmp head`, wobei der Kopf nur `cmp` + `jcc` ist. Jetzt steht am
Ende eines Blocks, der zu so einem Kopf springt, der Kopf selbst:

```
vorher                         jetzt
  body: ...                      body: ...
        jmp  head                      cmp  r8,r10
  head: cmp  r8,r10                    jl   body
        jge  exit
```

Zwei statt drei Befehle je Durchlauf. Der Kopf darf bis zu drei schlichte
Rechnungen vor dem Vergleich haben (`Bin`, `Un`, `Cast`, `Copy`, `Const`,
`PtrAdd`, `Load`), z. B. `lea 8(r11),rdx; cmp r10,rdx` in `rt.mem_copy`.

Warum das nichts brechen kann: der Block springt mit `jmp` -- es gibt nur
diese eine Kante, und alle phi-Kopien stehen schon davor. Der
Maschinenzustand am Ende des Blocks IST der am Eingang des Kopfes, und die
Ausgabe einer Anweisung haengt nur an den festen Plaetzen ihrer Werte. Nicht
zugelassen ist, was eigene Sprungmarken erzeugt (gepruefte Rechnungen) oder
Speicher schreibt/aufruft. Die Uebergaberegister (`fp_handover`) gelten nur
innerhalb eines Blocks und werden mitkopiert.

Abschaltbar mit `FIRN_NO_ROTATE=1`. Test: `tests/1702_loop_rotation.fi`
(null Durchlaeufe, `continue` als zweiter Rueckweg, Kopf mit Rechnung,
verschachtelt, Gleitzahl, ohne Vorzeichen).

## Was noch dasteht (gleiche Zaehlung)

- Holen aus dem Rahmen: 9,8 Mio -- vor allem `l3_huffman` (63 lebende Werte).
- `movaps xmm,xmm` der Zweioperandenform: 8,7 Mio -- mit `--cpu=avx` weg.
- Blattfunktionen reservieren einen Rahmen, auch wenn sie keinen brauchen
  (`sub $0xa0,rsp` in einer Funktion ohne einen einzigen Rahmenzugriff).

## Geprueft

Schnelltest in `release-fast` (320), `release-safe` (316) und `dev-fast`
(316): alle gut. Fixpunkt: Stufe 2 und 3 zeichengleich (793 453 Zeilen),
`self_compare` 341 von 341 gleiches Verhalten, 0 abweichend.

Wanduhr (60 s Ton, beste von neun, Maschine nicht ruhig): Firn 0,14 s,
mit `--cpu=avx` 0,13 s, C `gcc -O2` 0,07 s, C ohne Auto-Vektorisierung 0,10 s.
