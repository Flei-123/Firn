# Runde ASSEMBLER — der Baustein, der Oktette schreibt

Zweig `assembler`, Arbeitsbaum `/root/firn-asm`. Nicht nach `main` gefuehrt.

## Worum es ging

Die JIT-Studie vom 05.09.2026 (`/root/jit-studie/STUDIE-JIT.md`) hat
festgestellt: Firn hat keinen Maschinencode-Erzeuger. `codegen_x86.rs` und
`codegen_a64.rs` geben `Result<String, String>` zurueck — Text fuer GNU `as`.
Danach startet `main.rs` die Prozesse `as` und `ld`. Was existiert, ist
Befehlsauswahl und Registerzuteilung; was fehlt, ist die Binaerkodierung.

Ohne die kann es keinen JIT geben, denn ein JIT hat keinen Assembler zur
Hand — er schreibt Oktette in eine ausfuehrbare Speicherseite.

**Der Befund der Studie hat sich bestaetigt.** Ich habe nichts gefunden, was
ihr widerspricht. Was ich zusaetzlich gefunden habe, steht unter „Was anders
war als erwartet".

## Etappe 1 — das Pflichtenheft MESSEN, nicht schaetzen

Die Aufgabe sagte: zaehl, welche Mnemonics mit welchen Operandenformen
wirklich vorkommen. Quelltext durchlesen waere geraten gewesen — `writeln!`
mit Platzhaltern verraet nicht, welche Operanden am Ende dastehen.

Also habe ich die Ausgabe gemessen: alle 309 Testprogramme mit
`--emit=asm` fuer beide Ziele uebersetzt (230 liefen durch), das ergab
1 046 825 Zeilen x86-Assembler und 1 322 364 Zeilen ARM64. Ein Zerleger
(`tools/asmstudie/formen.py`) normalisiert jede Zeile zu einer *Form*:
`mov rax, qword ptr [rbp-8]` wird zu `mov r64, m64(bd8)`.

Ergebnis — das ist das Pflichtenheft:

| Ziel | Mnemonics | Operandenformen |
|---|---|---|
| x86-64 | 83 | **215** |
| ARM64 | 75 | **129** |
| zusammen | | **344** |

Die Verteilung ist extrem ungleich. Die haeufigsten 30 x86-Formen decken rund
95 % aller erzeugten Befehle ab; `mov r64, r64` allein kommt 172 717-mal vor.
Das ist der Grund, warum diese Aufgabe ueberhaupt in endlicher Zeit machbar
ist: man braucht nicht den Befehlssatz, man braucht diese 344 Formen.

Die vollstaendige Liste mit Haeufigkeiten: `tools/asmstudie/formen.txt`.

## Etappe 2 — die Abnahme zuerst

Bevor ich eine Zeile Kodierung geschrieben habe, habe ich den Pruefstand
gebaut. Sonst haette ich am Ende eine Bibliothek gehabt, von der ich behaupte,
sie sei richtig.

Aus jeder gemessenen Form werden konkrete Testfaelle erzeugt
(`formen_liste.py`) — und zwar gezielt mit den Registern, an denen Kodierer
scheitern: `rsp` und `r12` (erzwingen ein SIB-Oktett), `rbp` und `r13`
(koennen kein disp0), `spl/bpl/sil/dil` (nur mit REX erreichbar), dazu die
hohen Register r8–r15. Das ergibt 16 524 x86-Faelle und 8 166 ARM64-Faelle.

Die werden durch das echte GNU `as` geschickt (`gnu_wahrheit.py`), und aus
der Objektdatei werden ueber die Symboltabelle die Oktette jedes einzelnen
Befehls herausgeschnitten. Das ist die Wahrheit, gegen die verglichen wird.

## Die Zahlen

**x86-64** (`asmdiff` + `sprungdiff`):

```
byteweise GLEICH : 16138
FALSCH           : 0
Quote            : 99.8 % von 16164 pruefbaren Faellen
Formen gruen     : 193 von 211   (Rest: Spruenge, eigener Pruefstand)

Spruenge         : 544 von 544 gleich, RIP-relativ 4 von 4
-> zusammen        211 von 211 Formen
```

**ARM64** (`a64diff` + `a64sprung`):

```
wortweise GLEICH : 7556
FALSCH           : 0
Quote            : 99.4 % von 7605 pruefbaren Faellen
Formen gruen     : 110 von 124   (Rest: Spruenge)

Spruenge         : 266 von 266 gleich, alle vier Reichweitengrenzen korrekt
-> zusammen        126 von 126 Formen
```

**Zusammen: 337 von 337 pruefbaren Formen byteweise gleich mit GNU `as`,
null Abweichungen.**

Zur Ehrlichkeit gehoert, warum 337 und nicht 344. Sieben Formen haben keinen
gueltigen Testfall, und die Gruende sind unterschiedlich schwer:

- `mov r64, qword ptr fs:0` und `jnz 1f` — handgeschriebene Zeilen aus dem
  Inline-Assembler (Segmentpraefix, lokale Marke). Gehoeren nicht in eine
  Kodierbibliothek fuer erzeugten Code.
- `mrs Xn, sym` und `msr sym, Xn` — Systemregister werden mit Namen
  angesprochen; dafuer braeuchte es eine Namenstabelle. Die Kodierfunktionen
  stehen (`A64::mrs`/`msr`), ungeprueft ist nur die Namensaufloesung.
- `rep movsb`, `rep stosb` — sind kodiert (`X86::rep_movsb`) und in den
  Einzeltests belegt, aber mein Zerleger fuehrt `rep` als Teil des Mnemonics,
  weshalb der Fallerzeuger sie uebersprungen hat. Ein Fehler im Pruefstand,
  nicht in der Kodierung.
- `add Xn, Xn, #imm, shift` — nur mit `lsl #12` zulaessig, meine Testwerte
  trafen das nicht.

Das sind sieben von 344. Ich nenne sie hier einzeln, damit niemand aus
„337 von 337" liest, es sei alles geprueft.

Die „uebersprungenen" Faelle (360 bzw. 526) sind solche, die `as` selbst
zurueckweist — etwa `shr rax, 1000` oder ein Bitmuster, das sich nicht
darstellen laesst. Dass sie abgelehnt werden, ist das richtige Verhalten,
nicht ein Fehler.

## Etappe 3 — laeuft es auch wirklich?

Byteweise Gleichheit mit `as` bleibt ein Vergleich mit einem anderen
Werkzeug. Der haertere Beweis ist die Maschine selbst.

`jitprobe` schreibt kodierte Oktette in eine `mmap`-Seite, setzt sie mit
`mprotect` auf ausfuehrbar (W^X: nie gleichzeitig schreibbar und ausfuehrbar)
und ruft sie auf. **14 von 14 Programmen liefern das erwartete Ergebnis.**

Darunter bewusst die Faelle, die ein reiner Textvergleich nicht prueft:

- eine Schleife (`Summe 1..n = 55`) mit **Rueckwaertssprung** und einem
  Sprungziel, das erst nachtraeglich eingesetzt wird — das ist der zweite
  Durchlauf eines Assemblers, im Kleinen
- Speicherzugriff ueber `r13` (erzwungenes disp8) und `rsp` (erzwungenes SIB)
- `setne sil` — ein 8-Bit-Register, das ohne REX `dh` waere
- `idiv` mit `cqo`, `movabs` mit echtem 64-Bit-Wert

Das ist genau der Vorgang, den ein JIT ausfuehrt. Der fehlende Baustein
existiert und ist belegt.

## Was GNU `as` mir widersprochen hat

Vier Fehler, die ich ohne den Vergleich nicht bemerkt haette. Sie sind der
eigentliche Ertrag der Abnahme.

**1. Die Sammlerform.** `add rax, 1000` ist nicht `81 /0` mit ModRM, sondern
`05` mit dem Wert direkt dahinter — eine kuerzere Sonderkodierung, die es nur
fuer den Sammler (al/ax/eax/rax) gibt. `as` nimmt sie immer, wenn sie passt.
Bei 8 Bit gilt sie sogar dann, wenn der Wert in ein Oktett passt
(`add al, 127` = `04 7f`), bei groesseren Breiten nur, wenn er es *nicht* tut
— denn sonst waere `83 /0` kuerzer. 74 Faelle lagen deswegen falsch.

**2. Die Bitmuster-Konstanten von ARM64 — der schwierigste Punkt der
Aufgabe.** Logische Befehle haben kein Zahlenfeld. Der Wert wird als
`(N, immr, imms)` beschrieben: ein sich wiederholendes Muster aus Einsen,
rotiert. Nur was sich so beschreiben laesst, ist ueberhaupt zulaessig —
`4095` geht, `4097` nicht, und `0` und lauter Einsen sind es ebenfalls nicht.

Mein Fehler lag in der Rotation: ich hatte die Linksdrehung, mit der ich die
Einsen nach unten hole, direkt als `immr` eingesetzt. `immr` ist aber die
**Rechts**drehung. Fuer den Wert 2^60 schreibt `as` `immr=4`, ich schrieb
`immr=60`. Richtig ist `immr = (Periode − r) mod Periode`. Bei allen Werten
mit `r = 0` — also 1, 3, 255, 4095 — faellt das nicht auf; erst ein einzelnes
hohes Bit deckt es auf. Ein Testsatz ohne solche Werte haette den Fehler
durchgelassen.

**3. `fmov Wn, Sn` hatte `rmode=11` statt `00`.** Gleiche Befehlslaenge,
andere Bedeutung: aus dem blossen Umschaufeln der Bits waere ein
Rundungsbefehl geworden. 35 Faelle.

**4. Nachindex-Versatz ist nicht skaliert.** Bei `ldr x0, [x0, #8]` steht im
Feld die 1 (8 geteilt durch 8), bei `ldr x0, [x0], #8` steht die 8. Zwei
verschiedene Zahlenraeume im selben Befehl.

**5. Eine Luecke im Pruefstand, nicht in der Kodierung.** `stp x29, x30,
[sp, #-16]!` — die Anpassung VOR dem Zugriff, also der Rahmenaufbau jeder
Funktion — kommt 7 744-mal in der Ernte vor und wurde von meinem
Fallerzeuger stillschweigend uebersprungen. Aufgefallen ist es erst, als ich
beim Schreiben dieses Berichts nachgezaehlt habe, welche Formen wirklich
einen Testfall haben. Nachgetragen: 240 Faelle, alle auf Anhieb richtig. Die
Lehre ist unangenehm und wichtig — ein Pruefstand, der eine Form gar nicht
erzeugt, meldet keinen Fehler, sondern schweigt.

## Gemessene Reichweiten

Nicht aus dem Gedaechtnis, sondern mit `as` ausprobiert, wo es kippt:

| Befehl | Reichweite | gemessenes Verhalten |
|---|---|---|
| x86 `jmp`/`jcc` | rel8 bis ±127 | ab Abstand 128 wird der Befehl laenger (rel32) |
| x86 `call` | nur rel32 | hat gar keine kurze Form |
| ARM64 `b`/`bl` | ±128 MiB | |
| ARM64 `b.<cond>` | ±1 MiB | |
| ARM64 `cbz`/`cbnz` | ±1 MiB | |
| ARM64 `tbz`/`tbnz` | **±32 KiB** | die enge Stelle |

Der x86-Fall ist der unangenehme: ein Sprung, der knapp nicht in ein Oktett
passt, wird drei Oktette laenger — und schiebt damit alles Nachfolgende
weiter, wodurch weitere Spruenge zu lang werden koennen. Deshalb braucht ein
Assembler zwei Durchlaeufe. Bei ARM64 wird nie etwas laenger; dort passt ein
Sprung ab einer gewissen Weite gar nicht mehr, und der Uebersetzer muss einen
Umweg bauen. `tbz` mit ±32 KiB ist die Stelle, an der das zuerst passiert.

Die Kodierung lehnt zu weite Spruenge ab, statt still etwas Falsches zu
schreiben — das ist ausdruecklich geprueft.

## Wo `as` fuer etwas anderes als Befehle benutzt wird

Wie beauftragt, klar benannt — das ist **nicht** Teil dieser Runde:

| Direktive | Vorkommen (x86-Ernte) | wofuer |
|---|---|---|
| `.loc` | 174 366 | Zeilennummern fuer den Fehlersucher |
| `.ascii` | 28 426 | Zeichenketten |
| `.globl` | 7 974 | sichtbare Symbole |
| `.quad` | 1 037 | Datenworte, Sprungtabellen |
| `.section` | 502 | Abschnitte |
| `.align` | 69 | Ausrichtung |

Das ist der zweite Schritt: eine **Objektdatei schreiben** (ELF-Abschnitte,
Symboltabelle, Relokationen, DWARF). Wer den JIT will, braucht ihn nicht —
ein JIT hat keine Objektdatei. Wer `as` und `ld` ganz loswerden will, braucht
ihn sehr wohl.

## Die Sprachfrage: Rust oder Firn?

Beauftragt war eine Begruendung. Hier ist der Befund, der sie bestimmt.

Firns Uebersetzer existiert **zweimal**: einmal in Rust (`compiler/src/`,
der arbeitende Uebersetzer) und einmal in Firn selbst
(`lib/firnc1/codegen.fi`, der selbsttragende). `codegen.fi` erzeugt ebenfalls
Text und kann nur x86. Wer die Kodierung in beide schreibt, pflegt sie
doppelt — und bei einer Sache, bei der ein einziges falsches Oktett zaehlt,
ist das die schlechteste aller Moeglichkeiten.

Ich habe sie in **Rust** gebaut, und zwar bewusst als Etappe, nicht als
Endzustand:

- Der heutige Uebersetzer ist Rust. Nur dort ist die Kodierung sofort
  benutzbar, und nur dort konnte ich sie gegen die 270 bestehenden Tests
  stellen (alle laufen weiter).
- Die Bibliothek hat **keine Abhaengigkeiten** und benutzt keine
  Rust-Eigenheiten, die sich nicht nach Firn uebertragen lassen: nur ganze
  Zahlen, Verschiebungen, Bitverknuepfungen und ein Feld von Oktetten. Sie
  ist absichtlich so geschrieben, dass die Portierung eine Uebersetzung ist
  und keine Neuentwicklung.

Der richtige Endzustand ist die Kodierung **in Firn**, aus beiden
Uebersetzern heraus aufrufbar — denn der JIT soll spaeter aus Firn heraus
laufen. Der Weg dorthin ist: erst `lib/firnc1` auf den Stand des
Rust-Uebersetzers bringen (es kann heute kein ARM64), dann die Kodierung
einmal nach Firn uebersetzen und die Rust-Fassung als Pruefmass behalten —
der Differenzpruefer laeuft dann gegen beide. Diese Reihenfolge zu drehen
hiesse, eine Bibliothek in eine Sprache zu schreiben, die ihr eigenes Ziel
noch nicht uebersetzen kann.

**Ehrlich gesagt:** das ist eine Entscheidung, die ich fuer diese Etappe
getroffen habe, nicht eine, die fuer immer gilt. Sie erlaubt beides, ohne die
Kodierung heute zweimal zu schreiben — aber sie verschiebt die Portierung,
sie erspart sie nicht.

## Was noch fehlt bis zu einem Baseline-JIT

Eine ehrliche Schaetzung, wie beauftragt. Vorhanden ist die Kodierung —
das ist die Grundlage, aber sie ist nicht der JIT.

**1. Bindung von Sprungzielen und Marken.** Heute nimmt die Kodierung einen
fertigen Abstand entgegen. Ein JIT braucht die Verwaltung darueber: Marken,
die noch nicht bekannt sind, Stellen die spaeter gefuellt werden, und fuer
x86 den Fixpunkt ueber die Sprunglaengen. Der Kern steht (`jitprobe` fuellt
ein Ziel nachtraeglich), die Verwaltung fehlt. — *ueberschaubar, gut pruefbar*

**2. Speicherverwaltung fuer ausfuehrbare Seiten.** `mmap`/`mprotect` sind
in `jitprobe` belegt. Was fehlt: mehrere Seiten, Freigabe, W^X sauber ueber
die Lebenszeit, und auf ARM64 zusaetzlich das Leeren des Befehlszwischen-
speichers — ohne das fuehrt der Rechner alten Code aus. — *ueberschaubar,
aber auf ARM64 eine echte Fehlerquelle*

**3. Aufrufregeln und Rahmen.** Argumente in Registern, Register die der
Aufgerufene retten muss, Ausrichtung des Stapels, der Uebergang zwischen
uebersetztem und gedolmetschtem Code. — *mittel*

**4. Der Weg von Firns Zwischendarstellung zur Kodierung.** Das ist die
eigentliche Arbeit. Die bestehenden Codeerzeuger schreiben Text; sie muessen
auf die Kodierbibliothek umgestellt werden — oder ein zweiter, schlanker
Erzeuger fuer den JIT entsteht daneben. Registerzuteilung existiert bereits,
was gut ist. — *der groesste Brocken*

**5. Wann wird ueberhaupt uebersetzt?** Zaehler fuer heisse Stellen, Wechsel
vom Dolmetscher in den uebersetzten Code und zurueck, Umgang mit Annahmen,
die sich als falsch herausstellen. Fuer einen *Baseline*-JIT darf das
schlicht bleiben (uebersetzen bei Aufruf, keine Ruecknahme), aber es ist
Entwurfsarbeit. — *mittel bis gross*

Mein Eindruck nach dieser Runde: Punkt 1 und 2 sind naeher, als die Studie
vermuten laesst — die Kodierung war der Teil mit den vielen kleinen
Fallstricken, und der ist jetzt gemessen statt vermutet. Punkt 4 und 5 sind
der eigentliche Weg. Eine Zeitschaetzung in Wochen gebe ich nicht ab; ich
haette keine Grundlage dafuer, und eine erfundene Zahl waere schlechter als
keine.

Was ich mit Zahlen sagen kann: 344 Formen gemessen, 337 pruefbar,
337 byteweise gleich, 0 falsch, 14 von 14 Programmen wirklich ausgefuehrt.

## Was anders war als erwartet

- **Die Studie stimmt.** Kein Widerspruch gefunden.
- **Die Doppelung des Codeerzeugers** (Rust *und* `lib/firnc1/codegen.fi`)
  stand nicht in der Studie, bestimmt aber die Sprachfrage. `codegen.fi`
  kann heute kein ARM64.
- **344 Formen sind weniger, als „x86 kodieren" klingt.** Weil nur zaehlt,
  was Firn wirklich ausgibt. Die Aufgabe war dadurch endlich.
- **Der Pruefstand hat mehr gefunden als erwartet** — und zwar Fehler, die
  ich beim Lesen des eigenen Codes fuer richtig gehalten haette. Der
  Bitmuster-Fehler faellt nur bei Werten mit Rotation auf. Ohne den Vergleich
  waere er in den JIT gewandert und haette dort als sporadisch falsches
  Ergebnis gewirkt — die teuerste Sorte Fehler.

## Dateien

Kodierung:
- `compiler/src/encode_x86.rs` (931 Zeilen)
- `compiler/src/encode_a64.rs` (716 Zeilen)

Pruefstand:
- `compiler/src/bin/asmdiff.rs` — x86 Befehlsformen
- `compiler/src/bin/sprungdiff.rs` — x86 Sprungweiten und RIP
- `compiler/src/bin/a64diff.rs` — ARM64 Befehlsformen
- `compiler/src/bin/a64sprung.rs` — ARM64 Sprungreichweiten
- `compiler/src/bin/jitprobe.rs` — Ausfuehrungsprobe auf echter Maschine

Messung:
- `tools/asmstudie/formen.py` — Ernte zerlegen, Pflichtenheft erzeugen
- `tools/asmstudie/formen_liste.py` — Formen zu konkreten Faellen machen
- `tools/asmstudie/gnu_wahrheit.py` — Wahrheit von GNU `as` holen
- `tools/asmstudie/sprung_wahrheit.py`, `sprung_a64.py` — Sprungweiten messen
- `tools/asmstudie/formen.txt` — das Pflichtenheft mit Haeufigkeiten

## Nachvollziehen

```sh
cd /root/firn-asm/compiler && cargo build --release
cd /root/firn-asm

# Wahrheit neu von GNU as holen (braucht as und aarch64-linux-gnu-as)
python3 tools/asmstudie/gnu_wahrheit.py x86
python3 tools/asmstudie/gnu_wahrheit.py a64
python3 tools/asmstudie/sprung_wahrheit.py
python3 tools/asmstudie/sprung_a64.py

# Vergleichen
./compiler/target/release/asmdiff    tools/asmstudie/wahrheit_x86.json
./compiler/target/release/sprungdiff tools/asmstudie/wahrheit_sprung.json
./compiler/target/release/a64diff    tools/asmstudie/wahrheit_a64.json
./compiler/target/release/a64sprung  tools/asmstudie/wahrheit_a64spr.json

# Und wirklich ausfuehren
./compiler/target/release/jitprobe
```

Die Wahrheitsdateien liegen bewusst nicht im Zweig (`.gitignore`) — sie sind
erzeugte Messdaten und jederzeit reproduzierbar.
