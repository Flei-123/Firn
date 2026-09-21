# Runde TEMPO 3 — eine Faltung, zwei verworfene Ideen

Stand 21.09.2026, Zweig `xmm-ra`. Diese Runde ist kurz, und sie besteht zu
zwei Dritteln aus **verworfenen** Versuchen. Beides steht hier, weil das
Messergebnis der eigentliche Inhalt ist.

## Was geblieben ist: die Skalierung wandert in die Adressrechnung

Ein Basiszeiger, den MEHRERE Zugriffe benutzen, kann nicht in den Operanden
eines einzelnen Befehls wandern — dafuer gibt es `foldable_addresses`, und
die verlangt, dass die Adresse genau einmal gelesen wird. Die SKALIERUNG des
Index aber kann trotzdem mitgehen:

```text
vorher:   lea rdx, [0 + rdi*4]        nachher:   lea rdi, [rax + rdi*4]
          lea rdi, [rax + rdx]
```

Bedingungen: die Summe rechnet mit voller Breite, der skalierte Teil steht
UNMITTELBAR davor und wird nur dort gelesen (dann darf er ganz entfallen,
ohne dass sich an den Lebensdauern etwas aendert), Grundwert und Index liegen
in Registern. Faktor 2, 4 oder 8 — was die Adressierung selbst kann.

**Gemessen** (MP3-Dekoder, 8 s Ton, `release-fast`, callgrind):
**308,2 Mio -> 304,4 Mio Befehle**, −1,3 %. Ausgabe bitgleich.

## Verworfen 1: die Multiplikation mit der Laufvariablen (IVRED)

Der klassische Pass: `i * m` im Schleifenrumpf wird eine zweite
Laufvariable, die je Durchlauf um `m * Schritt` weitergeht — die
Multiplikation verschwindet. Gebaut, getestet (richtig in allen vier
Baustufen, `imul` wirklich weg), **gemessen**:

| Programm | mit IVRED | ohne | |
|---|---|---|---|
| MP3-Dekoder | 313,3 Mio | 304,4 Mio | **+3 %** |
| `bench/firn/matmul` | 543,6 Mio | 501,8 Mio | **+8 %** |
| `bench/firn/bubblesort` | 261,8 Mio | 234,7 Mio | **+12 %** |
| `bench/firn/bytecount` | 2031,1 Mio | 2031,1 Mio | 0 % |

**Schlechter, ueberall.** Der Grund ist die Maschine: x86 hat `imul r, r,
imm` als EINEN Befehl, und was an der Multiplikation haengt (Vorzeichenweite,
`lea`) bleibt ja stehen. Dafuer kostet die neue Laufvariable ein Register
ueber die ganze Schleife und eine Fortschaltung je Durchlauf — und Register
sind genau das, was in diesen Schleifen fehlt (gemessen: Druck 16 bis 20 bei
14 verfuegbaren).

Der Pass ist wieder entfernt. Lohnen wuerde sich die Idee erst, wenn die
GANZE Adresskette zum Zeiger wird, der weitergeschaltet wird (das macht C an
dieser Stelle) — nicht der eine `imul`.

## Verworfen 2: `synth_pair` einbetten

`synth` ruft `synth_pair` zwoelfmal; jeder Aufruf zerstoert alle
caller-saved Register, weshalb drei Basiszeiger dort im Rahmen stehen
bleiben. Naheliegend: die Obergrenze des Einbettens (`MAX_CALLEE_INSTS = 40`)
so weit heben, dass die Funktion (rund 120 Anweisungen) mitkommt.

Gemessen: **308,2 -> 307,3 Mio Befehle (−0,3 %)** bei wachsendem Programm.
Der Druck in `synth` bleibt ja derselbe — die Zeiger bekommen trotzdem kein
Register. Die Grenze bleibt, wo sie war.

## Nachtrag zu Runde TEMPO 2: der eine AVX-Fehlschlag war keiner

Der erste Testlauf mit `FIRN_CPU=avx` meldete in `tools/self_compare.sh`
`FAULTY: 1` (erste Abweichung `tests/1002_js_interp.fi`). Nachgestellt: zu
dem Zeitpunkt liefen ZWEI volle Testreihen gleichzeitig auf derselben
Maschine, und das Werkzeug arbeitet mit `timeout 20` je Programm. Der Lauf
allein, auf ruhiger Maschine:

```text
SAME BEHAVIOUR:     338      DIFFERING: 0      FAULTY: 0
```

— Zahl fuer Zahl dasselbe wie in der Grundstufe. Die Ursache war die
Maschine, nicht der Erzeuger.

## Stand gegen C

Unveraendert gegenueber TEMPO 2, die 1,3 % dieser Runde liegen innerhalb der
Messgenauigkeit der Uhr: MP3-Dekoder 0,25 s (`baseline`) bzw. 0,23 s
(`--cpu=avx`) gegen 0,07 s fuer `minimp3` mit `gcc -O2` — **3,3x**, und
**2,3x** gegen dasselbe C ohne Auto-Vektorisierung.
