# Runde TEMPO 8 — Kopien verschmelzen

Stand 22.09.2026, Zweig `xmm-ra`. Fortsetzung von TEMPO 7, das mit einer
Diagnose endete: der Abstand zu C sitzt nicht mehr in der Rechnung, sondern
im **Registerdruck** — `synth` hat 44 gleichzeitig lebende Werte,
`l3_huffman` 63, bei vierzehn Registern.

Diese Runde nimmt sich den Zuteiler vor. Sie beginnt aber nicht mit einer
Idee, sondern mit einer Zaehlung.

## Die Messung, die die Richtung vorgab

`valgrind --tool=callgrind --dump-instr=yes` zaehlt die Befehle je ADRESSE.
Zusammen mit dem Disassemblat ergibt das die Frage "wofuer geht die Zeit in
dieser Funktion drauf" in Zahlen statt in Vermutungen. Fuer `synth` (37,3 von
174,2 Mio Befehlen, also 21 % des ganzen Programms):

| Sorte | Befehle |
|---|---|
| **Register-zu-Register-Kopien** | **9,73 Mio** |
| Ablegen in den Rahmen | 4,03 Mio |
| Holen aus dem Rahmen | 3,24 Mio |
| `mulps` | 2,67 Mio |
| `movdqu` | 2,42 Mio |

Der groesste Posten der heissesten Funktion war also nicht das Rechnen,
sondern das **Kopieren**. Der Blick in die heisse Schleife zeigte, wovon:

```text
667440  movdqu (%r10),%xmm13
667440  mov    %rbx,%rax            # <-- vier Laufzeiger, je vier Befehle
667440  add    $0x2,%rax
667440  mov    %rax,-0xa18(%rbp)
667440  mov    %r10,%rax
667440  add    $0x8,%rax
667440  mov    %rax,-0xa30(%rbp)
        ...
250290  movaps %xmm0,-0xd60(%rbp)   # <-- zwei Sammler, je zwei Befehle
250290  movaps -0xd60(%rbp),%xmm14
250290  mov    -0xa18(%rbp),%rbx    # <-- und auf der Rueckwaertskante
250290  mov    -0xaa8(%rbp),%r11       alles wieder zurueck
250290  mov    -0xac0(%rbp),%r9
250290  mov    -0xa30(%rbp),%r10
```

Vier Befehle fuer das, was C mit `add $0x2,%rbx` erledigt — mal vier Zeiger,
mal 667 440 Durchlaeufe.

## Woher das kommt

`phi.rs` loest jeden phi-Knoten in eine **Kopie am Ende des Vorgaengers** auf.
`fold_into_definitions` gibt diese Kopie dort zurueck, wo der eingehende Wert
IM SELBEN BLOCK gerechnet wird — und genau das ist in einer Schleife mit
Verzweigung nicht der Fall:

```text
bb_kopf:   i' = i + 2          <- hier wird fortgeschaltet
           if ... -> bb_a else bb_b
bb_a:      ...
bb_b:      ...
bb_ende:   i <- i'             <- hier verlaesst die Rueckwaertskante
           jmp bb_kopf
```

Die Kopie steht in `bb_ende`, gerechnet wurde in `bb_kopf`. Die vier
Bedingungen von `fold_into_definitions` greifen nicht, und `i'` bekommt einen
eigenen Platz — im Rahmen, weil alle Register vergeben sind.

Der Kommentarkopf von `phi.rs` sagt das seit Runde 92 voraus: *"When a later
round teaches the allocator to coalesce copies, splitting has to come with
it."* Das ist diese Runde.

## Was gebaut wurde

Fuer eine Kopie `p = copy t` werden **`t` und `p` zu EINEM Intervall
zusammengelegt, bevor der lineare Scan laeuft**. Beide bekommen damit
denselben Platz; `emit_bin` rechnet gleich dort (`load_full` schreibt nichts,
wenn der Wert schon steht) und die Kopie gibt keinen Befehl mehr aus — alle
drei Kopiewege in `emit_inst` pruefen das bereits.

Drei Bedingungen, jede einzeln notwendig:

1. **`t` wird genau einmal geschrieben und genau einmal gelesen**, und zwar
   von dieser Kopie. Ein zweiter Leser saehe sonst ein Register, das die
   Rueckwaertskante laengst weitergedreht hat.
2. **Gleicher Typ, kein Sonderplatz.** Ein unmittelbarer Wert traegt
   `Slot(0)` als Platzhalter, ein Vorrats-Wert (TEMPO 6) steht in `.rodata`,
   eine Zelle und ein `alloca` liegen woanders. Wer `t` einen dieser
   "Plaetze" gibt, schreibt ins Nichts.
3. **Keine Stoerung (Chaitin):** an keiner Stelle, an der das eine
   geschrieben wird, lebt das andere noch. Die Kopie selbst ist ausgenommen —
   sie ist der Grund, aus dem verschmolzen wird.

Mehrere Quellen je `p` sind erlaubt, solange sie sich untereinander nicht
stoeren. Ein `if` im Schleifenrumpf schreibt die Schleifenvariable in jedem
Zweig einmal, und die Zweige schliessen einander aus — in `synth` sind das
drei Quellen je Sammler.

### Die drei Fehlversuche, die die Bedingungen erklaeren

Die Runde brauchte drei Anlaeufe, und jeder hat etwas gelehrt:

**Erster Anlauf: nach dem Scan, mit der Bedingung "Lebensdauer von `t` ganz
in der von `p`".** Klang richtig, war zu streng: die Bloecke eines `if` im
Schleifenrumpf stehen in der linearen Nummerierung oft HINTER dem Block mit
der Rueckwaertskante, und `t` lebt durch sie hindurch — `p` nicht, denn genau
das ist ja die Voraussetzung. Von den vier Laufzeigern scheiterten alle vier.
Ergebnis: 174,2 -> 173,9 Mio, also nichts.

**Zweiter Anlauf: die Bedingung direkt gestellt** (kein anderer Wert mit
demselben Register ueberschneidet sich mit `t`, und nichts zerstoert es) und
die grobe Zerstoerungsmaske durch die exakte aus `exact_crossings` ersetzt —
`rough` summiert ueber die lineare Nummerierung und sieht darum Aufrufe, die
auf dem Weg von `t` gar nicht liegen. 174,2 -> 162,1 Mio.

**Dritter Anlauf: vor den Scan.** Mit der `fp_taugt`-Schranke (siehe unten)
bekamen die Sammler ploetzlich selbst ein `xmm` — und waren damit dem
nachtraeglichen Verschmelzen entzogen, weil ein `t` mit eigenem Register
nicht mehr zu bewegen ist, ohne dem Nachbarn seines wegzunehmen. Die Kopie
blieb als `movaps` stehen. Erst als das Verschmelzen VOR dem Scan
stattfindet, traegt `p` von vornherein die Lebensdauer, das Gewicht und die
zerstoerten Register beider Werte. Gemessen: vier verschmolzene Werte
nachtraeglich, **fuenfundzwanzig** davor.

## Der Fehler, den die Runde selbst gemacht hat

`tests/1182_layout_float_probe.fi` gab 2 statt 0 zurueck — nur mit
Registerzuteilung, also in drei der vier Baustufen. Schuldig war eine
einzige Verschmelzung in `flow__layout_inline`, und die Ursache ist
lehrreich genug, um hier zu stehen:

Verschmolzen wurden `%311 = load.f64 ...` und der phi-Wert `%851`. `%851`
bekam **kein Register** (seine Lebensdauer kreuzt Aufrufe), also teilten sich
beide einen RAHMENPLATZ. Das ist fuer sich genommen richtig — aber die
Fliesskomma-Uebergabe `fp_handover` (Runde TEMPO 2) sieht einen Wert ohne
Register mit genau einem Leser und legt ihn in `xmm2`, statt ihn abzulegen.
Die Kopie sah daraufhin "Quelle und Ziel sind derselbe Platz" und gab nichts
aus — waehrend der Wert in `xmm2` stand und niemand ihn je in den Rahmen
schrieb. Der Leser holte sich aus dem Platz, was zufaellig darin lag.

Die Antwort ist nicht ein Sonderfall mehr, sondern eine engere Regel:
**verschmolzen wird nur, wenn das Ziel wirklich ein Register bekommen hat.**
Zwei Werte auf einem Rahmenplatz bringen ohnehin nichts — nachgemessen: null
Befehle Unterschied — und jeder weitere Weg, der einem Wert ohne Register
nachtraeglich doch einen Platz gibt (Zellalias, Zellfortschaltung,
Uebergaberegister), waere dieselbe Falle noch einmal.

## Ein Fehler, den die Runde aufgedeckt hat

`fp_taugt` sagt, welcher Wert ueberhaupt ein SSE-Register sehen darf. Dort
stand

```rust
Op::Bin(..) | Op::Copy { .. } => inst.ty.is_float(),
```

`is_float()` ist fuer `FTy::V128` **falsch** — und `emit_inst` hat fuer die
Vektorkopie einen eigenen Zweig, der sehr wohl im SSE-Weg steht. Jeder Wert,
den eine Vektorkopie las, verlor damit sein Register. Das betraf seit TEMPO 4
genau die beiden Sammler der Synthesefilterbank: sie liefen ueber den Rahmen,
obwohl zwoelf `xmm` frei waren.

## Die Zahlen

MP3-Dekoder, 8 s Ton, `release-fast`, `valgrind --tool=callgrind`:

| | Befehle |
|---|---|
| nach TEMPO 7 | 174,2 Mio |
| **nach TEMPO 8** | **160,6 Mio** |
| dasselbe mit `--cpu=avx` | **144,7 Mio** |
| `minimp3` in C, `gcc -O2` | 74,8 Mio |

Und die Wanduhr, 60 s Ton, kleinste von elf Laeufen, Ausgabe nach
`/dev/null`:

| | Zeit |
|---|---|
| Firn nach TEMPO 7 | 0,18 s |
| **Firn jetzt** | **0,15 s** |
| Firn mit `--cpu=avx` | 0,17 s |
| C, `gcc -O2` | 0,07 s |
| dasselbe C ohne Auto-Vektorisierung | 0,10 s |
| dasselbe C mit `-O0` | 0,39 s |

Also **2,1x hinter `gcc -O2`** und **1,5x** hinter demselben C, wenn man gcc
die Auto-Vektorisierung wegnimmt. Die PCM-Ausgabe ist ueber 60 s Ton
oktettgleich mit der von `minimp3`.

Die heisse Schleife von `synth` sieht jetzt so aus:

```text
667440  movdqu (%r10),%xmm13
667440  pshufd $0x0,%xmm13,%xmm12
667440  pshufd $0x55,%xmm13,%xmm11
667440  lea    0x2(%rbx),%rbx        # ein Befehl je Laufzeiger
667440  lea    0x8(%r10),%r10
667440  movdqu (%r11),%xmm13
667440  movdqu (%r9),%xmm10
667440  lea    -0x100(%r11),%r11
667440  lea    0x100(%r9),%r9
        ...
584010  addps  %xmm10,%xmm15         # der Sammler bleibt im Register
```

Nach jedem einzelnen Schritt ist die PCM-Ausgabe **bitgleich**.

## Geprueft

Die volle Testreihe (`test.sh`) laeuft ohne neuen Fehler durch: alle
Testprogramme in vier Baustufen, 338 von 338 gleichem Verhalten im
Selbstvergleich, und der **Fixpunkt** -- Firn uebersetzt sich selbst, Stufe 2
und Stufe 3 sind zeichengleich (793 453 Zeilen Assembler). Die drei roten
Punkte des Laufs (`tools/js/run.sh` scheitert an einer fehlenden
`testdata/test262/subset.sha256`, `tools/fmt/run.sh` an ungeformten Dateien
in `lib/fui/`, `tools/english/check.sh` an `nur_hier` aus TEMPO 6) sind
aelter als diese Runde und haben mit ihr nichts zu tun.

## Was es kostet

Die Uebersetzung von `bin/firnc1.fi` dauert 3,92 statt 3,76 Sekunden, also
gut vier Prozent mehr. Die erste Fassung lag bei zwoelf Prozent, weil
`stoerung` je Kandidat die ganze Funktion ablief; jetzt stehen die
Schreibstellen einmal in `defsites` und geprueft wird nur dort — an jeder
anderen Stelle koennen zwei Werte gar nicht gleichzeitig lebendig werden.

`FIRN_NO_COALESCE=1` schaltet die Runde ab, `FIRN_COAL_DBG=<name>` sagt fuer
jede Kopie der betroffenen Funktionen, warum sie verschmolzen wurde oder
nicht.

## Was als naechstes kaeme

Die Zaehlung der abgelehnten Kandidaten im Dekoder:

| Grund | Anzahl |
|---|---|
| `t` hat mehr als einen Leser oder Schreiber | 217 |
| **verschmolzen** | **25** |
| `t` ist schon anderweitig vergeben | 18 |
| Sonderplatz | 11 |

Die 217 sind nicht mit Verschmelzen zu holen — dort braucht es das, was
TEMPO 7 als Punkt 2 aufgeschrieben hat: **Lebensdauern an Aufrufen und an
Bloecken zerschneiden**, damit ein Wert in einem Abschnitt ein Register haben
darf und im naechsten nicht. Das ist der naechste Brocken.
