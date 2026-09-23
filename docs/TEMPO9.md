# Runde TEMPO 9 — die Schleife, die nur Nullen schreibt

Stand 23.09.2026, Zweig `xmm-ra`. Eine kleine Runde mit einem grossen
Ergebnis, und sie steht hier vor allem wegen der Messung, die sie ausgeloest
hat.

## Die Messung: je Adresse, nicht je Funktion

Bis TEMPO 8 wurde je FUNKTION gezaehlt. Das reicht, solange der Aufwand
gleichmaessig verteilt ist — und verdeckt alles andere. Diese Runde zaehlt
mit `valgrind --tool=callgrind --dump-instr=yes` **je Adresse** und ordnet
die Adressen ueber die **Symboltabelle** (`nm`) zu.

Die Zuordnung ueber das Disassemblat, die zuerst dastand, liess ein Viertel
der Befehle unter `??` liegen — und ausgerechnet dort lag das Ergebnis. Der
Grund: die Positionen in der callgrind-Datei sind ueberwiegend relativ, ein
einziger nicht verstandener Eintrag verschiebt alle folgenden um ein paar
Oktette, und ein Vergleich auf Gleichheit trifft dann nichts mehr. Ueber die
Symboltabelle ist die Funktion trotzdem eindeutig; fuer die Frage "welche
SORTE Befehl" genuegt der naechste Befehl davor.

Damit sah der MP3-Dekoder (8 s Ton, 160,6 Mio Befehle) so aus:

| Funktion | Befehle | Anteil |
|---|---|---|
| `synth` | 39,0 Mio | 24 % |
| `l3_huffman` | 23,1 Mio | 14 % |
| `l3_imdct36` | 19,7 Mio | 12 % |
| **`mp3_decode_frame`** | **17,2 Mio** | **11 %** |

`mp3_decode_frame` entscheidet nichts und rechnet nichts — es liest den
Rahmenkopf und ruft die anderen. Elf Prozent konnten dort nicht stimmen.

## Was dort stand

```text
2848362   cmp   $0x1200,%rdx
2848362   jae   fertig
2848362   mov   -0x8b0(%rbp),%r11      ; den Zeiger JEDES MAL neu holen
2848362   movb  $0x0,(%r11,%rdx,1)
2847744   lea   0x1(%rdx),%rdx
          jmp   kopf
```

Das ist `rt.mem_set((&(*s).grbuf[0][0]) as u64, 0, 576 * 2 * 4)` — einmal je
Granulat, 4608 Oktette. `rt.mem_set` ist in Firn geschrieben:

```firn
fn mem_set(target: u64, value: u8, n: usize) {
    var i: usize = 0
    while i < n {
        st8(target, i, value)
        i = i + 1
    }
}
```

Richtig, ueberall verwendbar — und **fuenf Befehle je Oktett**. 14,2 von
160,6 Millionen Befehlen, nur um ein Feld auf null zu setzen.

## Was gebaut wurde

Ein neuer Pass `memset` (`compiler/src/memset.rs`). Er erkennt in FIR

```text
P:    br H
H:    %i = phi [P %null, B %i2]
      %c = cmp.lt.uXX %i, %n
      brcond %c, B, X
B:    %a = add %base, %i
      store.u8 %wert, %a
      %i2 = add %i, %eins
      br H
```

und macht daraus `secure_zero(%base, %n)` im Vorkopf plus `br X`.

**Warum `secure_zero` und kein neuer Befehl:** `Op::SecureZero` gibt es seit
`secure_zero(inout buf)` in allen drei Erzeugern — x86 `rep stosb`, aarch64
achtbyteweise. Es tut exakt das Verlangte, und "eine Frage, eine Antwort"
heisst hier: keinen zweiten Befehl fuer dieselbe Sache erfinden. Dass
`secure_zero` zusaetzlich verspricht, nie wegoptimiert zu werden, ist fuer
diesen Fall staerker als noetig und darum unschaedlich.

## Die Bedingungen — eine davon ist gefaehrlich

Alle stehen im Dateikopf von `memset.rs`. Die wichtigste:

> **Der Vergleich muss VORZEICHENLOS sein.**

Bei `i64` kann `n` negativ sein. Die Schleife laeuft dann null Mal.
`rep stosb` mit `rcx = -1` schreibt den halben Adressraum voll. Das ist kein
theoretischer Unterschied, sondern der zwischen "nichts tun" und "Rechner
weg" — und ein Modultest (`the_signed_comparison_stays_a_loop`) haelt ihn
fest.

Die uebrigen: Rumpf mit genau drei Anweisungen (jede weitere waere eine
Wirkung, die `rep stosb` nicht hat), Wert konstant 0 und ein Oktett breit,
Anfang 0, Schrittweite 1, `base` und `n` ausserhalb definiert, nichts aus der
Schleife wird draussen gelesen. Und: feste Laengen unter sechzehn Oktetten
bleiben Schleife, weil `rep stosb` eine Anlaufzeit von einigen Dutzend Takten
hat.

## Das Ergebnis

| | Befehle (8 s Ton) |
|---|---|
| nach TEMPO 8 | 160,6 Mio |
| **nach TEMPO 9** | **146,3 Mio** |

`mp3_decode_frame` faellt von 17,2 auf 3,0 Mio. Die PCM-Ausgabe ist
bitgleich. `tests/1700_memset_schleife.fi` prueft die Raender: genau `n`
Oktette null, davor und dahinter unberuehrt, Laenge 0 schreibt nichts, ein
Wert ungleich null bleibt eine Schleife, und eine erst zur Laufzeit bekannte
Laenge funktioniert genauso — der letzte Punkt ist der wichtigste, weil der
Uebersetzer eine feste Laenge auch ganz ausrechnen koennte.

## Was daneben liegen blieb

`rt.mem_copy` ist dieselbe Schleife mit einem Lesen dazu und steht mit 3,5
Mio Befehlen da. Dafuer gibt es `Op::CopyMem` (`rep movsb`) — aber dessen
Groesse ist eine KONSTANTE im Befehl, keine Laufzeitgroesse, und die Frage
nach ueberlappenden Bereichen ist eine andere als bei einem Nullsetzen. Das
waere eine eigene Runde mit eigener Messung.
