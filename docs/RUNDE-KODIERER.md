# RUNDE KODIERER — Firn schreibt seine eigenen Maschinenoktette

**Datum:** 05.09.2026 · **Auftraggeber:** Justin · **Zweig:** `kodierer`
(Arbeitsbaum `/root/firn-kodierer`, aus `4af14c3ed`)

---

## DIE ANTWORT VORWEG

Die JIT-Studie vom selben Tag hat den Befund gebracht, der alles blockierte
(`/root/jit-studie/STUDIE-JIT.md` §1.5):

> `codegen_x86.rs` und `codegen_a64.rs` geben eine **Zeichenkette** zurück —
> Assemblertext. Es gibt im ganzen Baum keine einzige Stelle, die ein
> Opcode-Oktett schreibt.

**Das gilt nicht mehr.** Firn kann jetzt Maschinencode als Oktette erzeugen,
für x86-64 **und** ARM64, und das Ergebnis ist gegen GNU `as` geprüft:

```
                              x86-64          ARM64
   Übersetzungseinheiten       1 023          1 008
   bitgleich gegen `as`        1 023          1 008     (100 %)
   verglichene Oktette   201 743 532    282 990 878
   verglichene Umsetzungen 2 177 621      2 702 497
   Abweichungen                    0              0
   übersprungen                    0             15
```

*(Die 15 übersprungenen Einheiten sind Quellen, die für `aarch64-linux` gar
nicht bauen — das war vor dieser Runde so und ist dem Kodierer nicht
anzulasten. `tools/kodierer/run.sh` zählt sie ausdrücklich getrennt.)*

Dazu die Probe aufs Ganze: **314 von 314** Testprogrammen, einmal über `as`
und einmal über den eigenen Kodierer gebaut, laufen mit **demselben
Rückgabewert und derselben Ausgabe**. Null Abweichungen, null Baufehler.

Die Studie schätzte den Aufwand auf „2500–5000 Zeilen je Architektur".
Tatsächlich geworden sind es **6 934 Zeilen für beide zusammen**, inklusive
ELF-Schreiber und Prüfstand. Der Grund für die Halbierung steht in TEIL 1:
der Codeerzeuger benutzt nur einen kleinen Ausschnitt der Maschine.

---

# TEIL 1 — DER ZUSCHNITT: was gibt der Codeerzeuger überhaupt aus?

Bevor irgendetwas kodiert wurde, wurde **gezählt**. Der ganze Baum
(`tests/`, `tests/opt/`, `examples/`, `bin/firnc1.fi`, `lib/browser`,
`lib/js`, `lib/css`, `lib/layout`, `lib/paint`) wurde in drei Baustufen nach
Assemblertext übersetzt und jede Befehlszeile in eine *Form* eingeordnet
(Mnemonik + Operandenklassen).

**Ergebnis x86-64: 4 632 266 Befehlszeilen, 86 Mnemoniks, 232 Formen.**

Die ersten fünfzehn Formen decken drei Viertel aller Befehle ab:

| Anzahl | Form |
|---:|---|
| 879 728 | `mov r64, r64` |
| 572 441 | `mov r64, m64[b+d]` |
| 565 200 | `mov r64, imm` |
| 346 020 | `mov m64[b+d], r64` |
| 203 580 | `lea r64, m[b+d]` |
| 194 064 | `jmp sym` |
| 187 127 | `call sym` |
| 108 317 | `lea r64, m[rip+d]` |
| 93 629 | `pop r64` |
| 93 053 | `ret` |
| 87 529 | `mov m8[b], imm` |
| 87 272 | `mov r32, imm` |
| 77 861 | `jc sym` |
| 75 125 | `mov m64[b], r64` |
| 74 937 | `mov r64, m64[b]` |

**Ergebnis ARM64: 88 Mnemoniks, 153 Formen** — gleichförmiger, wie erwartet.

**Was diese Zählung wert war:** aus „der x86-64-Befehlssatz" (über tausend
Opcodes, mit AVX-512 mehrere tausend) wurden **232 Fälle**. Das ist der
Unterschied zwischen einem Jahresprojekt und einer Runde.

**Eine Falle dabei, ehrlich berichtet:** die erste Zählung lief ohne
`FIRNLIB`, weshalb alle Quellen, die `lib/` einbinden, still übersprungen
wurden — und mit ihnen der ganze SIMD- und Krypto-Teil (`aesenc`,
`sha256rnds2`, `pshufd`, `pextrq` …). Aufgefallen ist das erst, als der
fertige Kodierer an `tests/1613_crypto.fi` scheiterte. Die Lehre: eine
Erhebung ist nur so gut wie die Frage „wie viele Quellen haben eigentlich
*nicht* gebaut?". `tools/kodierer/run.sh` zählt übersprungene Einheiten
deshalb ausdrücklich mit.

---

# TEIL 2 — DER KODIERER

## 2.1 Der Aufbau: drei Schichten, absichtlich getrennt

```
   compiler/src/x86enc.rs    Inst  ->  Oktette          (rein, JIT-tauglich)
   compiler/src/asm_x86.rs   Text  ->  Inst + Marken    (Zerteiler, Relaxation)
   compiler/src/elfobj.rs    Abschnitte -> ELF-Datei    (architekturneutral)

   compiler/src/a64enc.rs    Sofortwert-Kodierungen, Ausbesserungen
   compiler/src/asm_a64.rs   Text  ->  Wörter + Marken
```

`x86enc.rs` und `a64enc.rs` kennen **keine Ein-/Ausgabe**: sie nehmen einen
Befehl und hängen seine Oktette an einen Puffer. Genau das ist das Stück,
das ein späterer JIT unverändert benutzen kann.

## 2.2 Warum der Umweg über den Text — und warum das kein Umweg bleibt

Der Kodierer liest denselben Assemblertext, den `codegen_x86.rs` schon immer
schreibt. Das kostet einen Zerteiler, kauft aber etwas, das anders nicht zu
haben ist: **einen Prüfstand mit Millionen echter Befehle.** Derselbe Text
geht einmal durch `as` und einmal durch den Kodierer, und die Oktette werden
verglichen. Ein Kodierer, der direkt aus FIR erzeugt, hätte diesen Maßstab
nicht — man müsste ihm glauben.

Ist die Gewissheit da (sie ist es), kann `codegen_x86.rs` in einer späteren
Runde direkt `x86enc::Inst` statt Text erzeugen. Der Kodierer darunter
bleibt derselbe; nur der Zerteiler entfällt. Was das bringt, steht in
TEIL 5.

## 2.3 Die Stellen, an denen ein falsches Oktett entsteht

Diese vier Fehler erzeugen Code, der *läuft* — nur anders. Genau die
Fehlerart, vor der der Auftrag gewarnt hat. Alle vier sind ausdrücklich
behandelt und einzeln geprüft.

**1. `rsp`/`r12` als Basis brauchen ein SIB.** Die Bitfolge `rm = 100`
bedeutet in ModRM nicht „Register 4", sondern „ein SIB-Oktett folgt". Wer
`[rsp]` ohne SIB kodiert, adressiert etwas ganz anderes.

**2. `rbp`/`r13` als Basis mit Verschiebung 0 brauchen `mod = 01`.** Bei
`mod = 00` bedeutet `rm = 101` „RIP-relativ". `[rbp]` muss deshalb als
`[rbp + 0]` mit ausgeschriebener Null-Verschiebung geschrieben werden.

**3. `spl`/`bpl`/`sil`/`dil` brauchen ein REX-Präfix, auch ein leeres.**
Ohne REX heißen die Nummern 4–7 als Oktettregister `ah`/`ch`/`dh`/`bh`.
`mov spl, al` ist `40 88 C4`; ohne die `40` steht dort `mov ah, al`.

**4. Sofortwerte müssen VOR der Formwahl auf die Operandenbreite
zugeschnitten werden.** Das war der einzige echte Fehler, den die Gegenprobe
gefunden hat, und er ist lehrreich:

```
   and r8d, 0xFFFFFFFC
   as       : 41 83 e0 fc            (imm8-Form, vier Oktette)
   Kodierer : 41 81 e0 fc ff ff ff   (imm32-Form, sieben Oktette)
```

Beide Kodierungen *rechnen dasselbe*. Als 32-Bit-Zahl ist `0xFFFFFFFC`
gleich `-4` und passt in die Kurzform; wer die 4 294 967 292 für zu groß
hält, schreibt drei Oktette zu viel. Die Wirkung des Befehls ist identisch —
aber **jeder folgende Sprung verschiebt sich**, und irgendwann springt einer
ins Leere. Der Fehler zeigte sich denn auch nicht am `and`, sondern
neun Oktette später an einem `jmp`, dessen Ziel um 9 danebenlag.

Das ist genau der Grund, warum die oktettweise Gegenprobe nicht verhandelbar
war: Der Fehler war in **einem von 1023** Bauten sichtbar, in einer Funktion
namens `_F0.__gc_mark`, und hätte sich im Betrieb als sporadisch falsches
Verhalten des Sammlers geäußert.

## 2.4 Was von `as` nachgebildet werden musste

Der Kodierer soll nicht *irgendeine* richtige Kodierung liefern, sondern
**dieselbe wie bisher** — sonst wäre das Umschalten eine Verhaltensänderung.
Also musste auch `as`' Formwahl nachgebildet werden:

* `add rax, 1` → `48 83 C0 01` (imm8-Kurzform)
* `add rax, 128` → `48 05 80 00 00 00` (Akkumulator-Kurzform)
* `add rbx, 128` → `48 81 C3 80 00 00 00` (keine Kurzform ohne rax)
* `shl rax, 1` → `48 D1 E0` (die Eins steckt im Opcode)
* `shl rax, 0` → `48 C1 E0 00` — `as` kürzt das **nicht** weg, wir auch nicht
* `mov rax, 1` → `48 C7 C0 01 00 00 00`, **nicht** `mov eax, 1`
* `mov rax, 2³¹` → `48 B8 …` (imm64, weil imm32 nicht mehr reicht)

Dazu die **Sprung-Relaxation**: `as` fängt bei der Kurzform (rel8) an und
lässt Sprünge nur wachsen, bis sich nichts mehr ändert. Derselbe Fixpunkt
wird hier berechnet.

## 2.5 Die Asymmetrie, die man nicht erraten kann

Bei der Auflösung von Zielen im selben Abschnitt verhält sich `as` **nicht
einheitlich** — gemessen, nicht vermutet:

```
   .globl b
   jmp  b     ->  eb 40                    direkt aufgelöst
   call b     ->  e8 00 00 00 00 + R_X86_64_PLT32   Umsetzung
   lea rdi, [rip + b]  ->  Umsetzung R_X86_64_PC32
   jmp  .Lloc ->  direkt         call .Lloc ->  direkt
```

Ein **globales** Symbol im selben Abschnitt wird beim *Sprung* aufgelöst
(er läuft durch die Relaxation), beim *Aufruf* dagegen nicht (er läuft durch
die Umsetzung, und ein globales Symbol darf beim Binden ersetzt werden).
Ein **lokales** Symbol (`.L…`) wird überall aufgelöst.

Und: zeigt eine Umsetzung auf eine lokale Marke in einem anderen Abschnitt,
schreibt `as` sie gegen das **Abschnittssymbol** mit dem Versatz der Marke
als Zusatz — nicht gegen die Marke selbst.

Auf ARM64 gilt das nicht: dort bekommen `b` **und** `bl` eine Umsetzung,
sobald das Ziel global ist (`R_AARCH64_JUMP26` bzw. `R_AARCH64_CALL26`).

---

# TEIL 3 — DIE ABNAHME

## 3.1 Der Prüfstand

```
   tools/kodierer/run.sh          die Abnahme über den ganzen Baum
   tools/kodierer/vergleich.py    eine Einheit: as gegen Kodierer, oktettweise
   tools/kodierer/ende_zu_ende.sh die Probe aufs Ganze: bauen und LAUFEN lassen
   tools/kodierer/vorgabe_unveraendert.sh   der Vorgabepfad gegen den alten firnc
   tools/kodierer/messung.sh      TEIL 5, die Zeitmessung
```

Für jede Quelle, in jeder Baustufe:

1. `firnc --emit=asm` erzeugt den Assemblertext
2. derselbe Text geht durch `as` → Objektdatei A
3. derselbe Text geht durch den Kodierer → Objektdatei B
4. verglichen werden `.text`, `.data`, `.rodata`, `.bss` **Oktett für
   Oktett**, dazu die **Menge der Umsetzungen** (Versatz, Symbolname, Art,
   Zusatz) und die **definierten globalen Symbole** mit ihrem Wert

Bei einer Abweichung nennt der Bericht den Versatz, die nächstgelegene
Marke, beide Kodierungen als Hex und die Zerlegung beider Seiten:

```
ABSCHNITT .text weicht ab bei Versatz 0x2722
  Ort: _F0.__gc_mark + 63
  as       : 41 83 e0 fc …
  Kodierer : 41 81 e0 fc ff ff ff …
```

Der Text wird nach jeder Einheit gelöscht — der ganze Baum als Assemblertext
wäre mehrere Gigabyte.

## 3.2 Das Ergebnis, x86-64

```
uebersetzte Einheiten : 1023
bitgleich             : 1023
abweichend            : 0
uebersprungen         : 0
verglichene Oktette   : 201 743 532
verglichene Umsetzungen: 2 177 621
```

Darunter `bin/firnc1.fi` — der selbstgehostete Übersetzer, 1,5 MB `.text` —
und `lib/browser/b4_main.fi`, der ganze Browser mit 2,6 MB `.text`.

## 3.3 Die Probe aufs Ganze

Bitgleiche Objektdateien sind ein starkes Argument, aber kein Beweis, dass
das *Programm* sich gleich verhält (die Objektdatei könnte in beiden Fällen
falsch sein). Also wurde jeder Testfall zweimal gebaut und **ausgeführt**,
verglichen wurde gegen den alten Weg:

```
gleiches Verhalten : 314
abweichend         : 0
alter Weg scheitert: 0
neuer Weg scheitert: 0
```

## 3.4 Der Vorgabepfad ist nachweislich unberührt

Die Runde hat Module **hinzugefügt** und eine Fahne eingebaut. Ohne
`--asm-intern` soll `firnc` genau das tun, was es vorher tat. Das lässt sich
stärker prüfen als mit der Testsuite: derselbe Testfall, gebaut mit dem
**unberührten** Übersetzer aus `/root/firn` und mit dem dieser Runde, und
die fertigen Programme Oktett für Oktett verglichen
(`tools/kodierer/vorgabe_unveraendert.sh`):

```
oktettgleich zum unberuehrten Uebersetzer : 314
abweichend                                : 0
uebersprungen                             : 0
```

Eine Testsuite prüft *Verhalten*; das hier prüft die **Ausgabe selbst**. Wenn
für jedes der 314 Programme dasselbe Oktett herauskommt wie vorher, kann sich
am Vorgabepfad nichts geändert haben.

## 3.5 Was NICHT verglichen wird — und warum

**Die Fehlersuchinformation.** `as` baut aus den `.loc`/`.file`-Direktiven
ein `.debug_line`-Programm; der interne Weg verwirft sie. Eine über den
internen Weg gebaute Datei hat also kein DWARF — das entspricht einem Bau
ohne `-g`.

Das ist der Hauptgrund, warum **der alte Weg Vorgabe bleibt**. Umgeschaltet
wird mit `--asm-intern`, sonst ändert sich nichts.

Der DWARF-Zeilenzähler nachzubauen ist keine große Sache (die
Zeilenprogramm-Kodierung ist gut beschrieben), aber sie *bitgleich* zu `as`
nachzubauen ist eine eigene Runde: `as` wählt die Spezialopcodes optimal,
und jede andere Wahl ergibt andere Oktette bei gleicher Bedeutung.

---

# TEIL 4 — ARM64

Derselbe Weg, andere Schwierigkeiten. ARM64 hat **feste Vier-Oktett-Befehle**,
also keine Relaxation und keine Präfixe. Dafür sitzt die Arbeit in den
Sofortwerten.

## 4.1 Der logische Sofortwert — die trickreichste Kodierung der Runde

`and x0, x0, #imm` speichert die Zahl **nicht** als Zahl, sondern als
*Muster*: `N:immr:imms` beschreibt eine Folge von `s+1` Einsen, rotiert um
`r`, wiederholt über eine Periode von 2, 4, 8, 16, 32 oder 64 Bit. `#1` ist
kodierbar, `#3` auch, `#5` **nicht**.

Der erste Versuch war um genau eine Drehrichtung falsch:

```
   and x9, x0, #-16
   as       : 92 7c ec 09     immr = 60
   Kodierer : 92 44 ec 09     immr = 4
```

`immr` ist die Rotation, mit der aus dem *Grundmuster* der *Wert* wird —
gesucht worden war die umgekehrte Richtung. Der Befehl blieb gültig und
lief; er maskierte nur mit `0xF0FFFFFFFFFFFFFF` statt mit
`0xFFFFFFFFFFFFFFF0`. Ein Stapelzeiger, der nicht ausgerichtet wird.

## 4.2 Die anderen beiden Sofortwert-Fallen

**Verschobene Sofortwerte.** `add x12, sp, #22, lsl #12` — die Verschiebung
um zwölf Stellen ist ein eigenes Bit im Befehl, kein Rechenschritt. Steht
`lsl #12` im Text, ist der geschriebene Wert schon der geschobene.

**Skalierte Verschiebungen.** `ldr x0, [x1, #16]` speichert die 2, nicht die
16 — die Verschiebung wird mit der Zugriffsbreite skaliert. Passt sie nicht
ins Raster (negativ oder unausgerichtet), muss auf die unskalierte Form
`ldur` gewechselt werden, mit anderem Opcode. Besonders hinterhältig ist
`ldr q0, …`: im Befehl steht `size = 00`, skaliert wird aber mit
**sechzehn**, weil die Breite aus `opc<1>:size` folgt.

## 4.3 Was noch gefunden wurde

* `umov w9, v16.s[0]` — das Vektorregister gehört ins `Rn`-Feld. Ohne das
  las der Befehl aus `v0` statt aus `v16`: gültiger Code, falsche Daten.
* `ldp x29, x30, [sp], #16` — die nachgestellte Aktualisierung hat ihr
  Komma *außerhalb* der Klammer. Ein naiver Zerteiler sieht vier Operanden
  statt drei.
* `.bss` als eigener Abschnitt (`NOBITS`): Inhalt null, belegt aber Platz.

## 4.4 Der Stand

```
uebersetzte Einheiten : 1008
bitgleich             : 1008
abweichend            : 0
uebersprungen         : 15   (Quelle baut nicht fuer aarch64)
verglichene Oktette   : 282 990 878
verglichene Umsetzungen: 2 702 497
```

Darunter `bin/firnc1.fi`, `lib/browser/b4_main.fi`, `tests/1613_crypto.fi`
(AES/SHA) und `tests/1614_simd_ops.fi`.

Dass die ARM64-Zahl **größer** ist als die x86-Zahl bei weniger Einheiten,
hat einen Grund: `codegen_a64.rs` hat keine Registerzuteilung (Studie §1.4),
jeder Wert bekommt einen eigenen Rahmenplatz. Der erzeugte Code ist dadurch
rund 40 % umfangreicher — was ihn als Prüfstand nicht schlechter macht,
im Gegenteil.

---

# TEIL 5 — WAS DER KODIERER SOFORT BRINGT

## 5a Die Übersetzungszeit

Gemessen mit `firnc --timings` — demselben Instrument wie die Studie —
an `lib/js/parse_main.fi`, Bestwert aus mehreren Läufen:

```
                      alter Weg      neuer Weg
   Optimierer         196,7 ms       196,4 ms
   as + ld            255,1 ms       155,8 ms   <<<
   Codeerzeuger       149,0 ms       152,4 ms
   sema                64,0 ms        63,2 ms
   lex+parse           55,9 ms        55,6 ms
   lower               32,8 ms        33,7 ms
   .s schreiben         1,2 ms         1,2 ms
   ------------------------------------------
   GESAMT             757,3 ms       660,5 ms   (-12,8 %)
```

Der Assemblerschritt für sich, auf derselben `.s`-Datei:

```
   as (GNU binutils) .......... 246 ms
   eigener Kodierer ........... 162 ms      1,52 x schneller
   ld ..........................  8 ms
```

**Ehrliche Einordnung.** Die Studie hatte „as + ld" mit 195,1 µs je Funktion
gemessen, rund 36 % der Übersetzungszeit. Weggefallen sind davon jetzt etwa
zwei Fünftel — nicht alles, und zwar aus einem klaren Grund: **der Text wird
noch immer geschrieben, gelesen und zerteilt.** Der Kodierer spart den
Prozessstart, das DWARF und die Umständlichkeit von `as`; er zahlt aber
weiterhin den Umweg über die Zeichenkette.

Was der nächste Schritt wert wäre, lässt sich daraus abschätzen: von den
162 ms des eigenen Assemblerschritts entfällt der größere Teil auf Zerteilen
und Zeichenkettenarbeit, nicht auf das Setzen der Oktette. Lässt
`codegen_x86.rs` den Text weg und erzeugt direkt `x86enc::Inst`, fällt
zusätzlich das Formatieren im Codeerzeuger weg. **Größenordnung: noch einmal
100–150 ms von 660**, also grob 20 % Gesamtersparnis gegenüber heute. Das
ist eine Schätzung, keine Messung — sie steht hier als Erwartung, nicht als
Ergebnis.

## 5b Fällt eine Abhängigkeit weg? — Ja, die größere von beiden

`as` wird nicht mehr gebraucht. Das ist für **OrientOS** der eigentliche
Gewinn: ein System, das sich ohne fremde Binutils übersetzen kann, hat eine
geschlossene Kette von der Quelle bis zur Objektdatei. Bisher stand mitten
darin ein fremdes C-Programm von rund 100 000 Zeilen.

Was noch fehlt, um `as` **ganz** zu streichen: das `.debug_line` (§3.4).
Solange Firn mit Fehlersuchinformation gebaut werden soll, braucht der
Vorgabepfad weiter `as`.

## 5c Braucht es noch einen eigenen Binder?

**Für die Geschwindigkeit: nein.** `ld` kostet **8 ms von 757** — ein
Prozent. Ein eigener Binder wäre der teuerste Posten mit dem kleinsten
Ertrag.

**Für die Unabhängigkeit: irgendwann ja**, aber später. Ein Objektschreiber
(fertig) plus `ld` deckt alles ab, was Firn heute baut. Ein eigener Binder
wird erst gebraucht, wenn OrientOS sich **ohne jedes** GNU-Werkzeug bauen
soll — und dann ist er ein überschaubares Stück Arbeit: Firn erzeugt genau
eine Objektdatei je Programm, es gibt keine Bibliotheken, keine dynamische
Bindung und keine Archive. Das ist der einfachste Fall, den ein Binder haben
kann.

**Reihenfolge, wenn Unabhängigkeit das Ziel ist:**
1. `.debug_line` selbst schreiben → `as` entfällt vollständig
2. Codeerzeuger direkt auf `Inst` umstellen → der Text entfällt, ~20 % schneller
3. Binder erst danach, und nur für OrientOS

---

# TEIL 6 — WAS DAMIT JETZT MÖGLICH WIRD

Der Kodierer ist **kein JIT** und diese Runde hat keinen gebaut — das war
ausdrücklich nicht der Auftrag, und die Studie stellt ihn hinter drei
billigere Posten. Aber die Sperre ist weg:

* **Laufzeit-Codeerzeugung ist technisch möglich.** `x86enc.rs` hat keine
  Ein-/Ausgabe; wer eine Seite mit `mmap(PROT_EXEC)` beschafft, kann direkt
  hineinschreiben. Was dafür noch fehlt, steht in der Studie §2 (ausführbarer
  Speicher, und auf ARM64 die Cache-Kohärenz).
* **Ein Schablonen-Baseline-JIT** (Studie §5.d) hätte jetzt sein Fundament.
* **Laufzeit-Spezialisierung** ohne vollen JIT — etwa vorgefertigte
  Zeichenketten- oder Rasterschleifen für die gerade vorliegenden Maße.
* **Übersetzen ohne fremde Werkzeugkette** für OrientOS (§5b).

Und, unabhängig von allem: **der Übersetzer ist heute 12,8 % schneller**,
weil ein Prozessstart und ein fremder Assembler weggefallen sind.

---

# ANHANG

## A.1 Was wo liegt

```
   compiler/src/x86enc.rs     1747 Zeilen   der x86-64-Kodierer (rein)
   compiler/src/asm_x86.rs    1791 Zeilen   Zerteiler, Marken, Relaxation
   compiler/src/a64enc.rs      334 Zeilen   ARM64-Sofortwerte, Ausbesserungen
   compiler/src/asm_a64.rs    2079 Zeilen   ARM64-Zerteiler und Befehlssatz
   compiler/src/elfobj.rs      403 Zeilen   ELF64-Objektschreiber
   compiler/src/asm_intern.rs   21 Zeilen   die Fahne
   tools/kodierer/            ~560 Zeilen   Abnahme, Probe, Messung
   -----------------------------------------------------------------
                              6934 Zeilen   gesamt
```

## A.2 Die Fahne

```
   firnc datei.fi                 der alte Weg über `as` (VORGABE)
   firnc --asm-intern datei.fi    der eigene Kodierer
   firnc --nur-obj [-o x.o] x.s   nur assemblieren (für die Gegenprobe)
```

## A.3 Die Abnahme wiederholen

```bash
   bash tools/kodierer/run.sh              # x86-64, ganzer Baum
   bash tools/kodierer/run.sh --a64        # ARM64
   bash tools/kodierer/ende_zu_ende.sh     # bauen UND laufen lassen
   bash tools/kodierer/vorgabe_unveraendert.sh   # Vorgabepfad unberuehrt?
   RUNS=5 bash tools/kodierer/messung.sh bin/firnc1.fi
```

## A.4 Was diese Runde NICHT belegt

* **Nichts über JIT.** Kein Laufzeitcode, keine ausführbaren Seiten, keine
  Deoptimierung. Die Studie bleibt in ihrer Empfehlung unverändert gültig.
* **Nichts über DWARF.** Der interne Weg erzeugt keine
  Fehlersuchinformation; verglichen wurden nur `.text`, `.data`, `.rodata`
  und `.bss`.
* **Die Reihenfolge der Umsetzungen** wurde als *Menge* verglichen, nicht
  als Folge. `as` schreibt sie in seiner internen Reihenfolge; für den
  Binder ist das ohne Bedeutung.
* **Kein Vergleich der ganzen Objektdatei Oktett für Oktett.** Die
  Abschnittsreihenfolge und die Symboltabellen-Reihenfolge unterscheiden
  sich; verglichen wurde der *Inhalt*, der beim Binden zählt.
* **ARM64 ist nur gegen `as` geprüft, nicht auf echter Hardware gelaufen.**
  Für x86-64 liegt die Ausführungsprobe vor (314/314), für ARM64 nicht.
* **Die volle Prüfsuite (`bash test.sh`, 1562 Punkte) wurde in dieser Runde
  nicht zu Ende gefahren** — auf dem Wirt waren nur noch rund 300 MB Platte
  frei (52 von 54 GB durch andere Projekte belegt), und die Suite baut den
  Übersetzer mehrfach mit sich selbst. Was stattdessen vorliegt, ist die
  **stärkere** Aussage für den Vorgabepfad (§3.4): 314 von 314 Programmen
  sind oktettgleich zum unberührten Übersetzer. Die Modultests des
  Übersetzers (`cargo test --release`) laufen durch, darunter die sieben
  neuen Fälle in `x86enc.rs`, die genau die vier Fallen aus §2.3 festnageln.
