# RUNDE KODIERER — Firn schreibt seine eigenen Maschinenoktette

**Datum:** 05.09.2026 · **Auftraggeber:** Justin · **Zweig:** `kodierer`
(Arbeitsbaum `/root/firn-kodierer`, aus `4af14c3ed`)

> **NACHTRAG — RUNDE KODIERER II, selber Tag.** Der einzige Grund, warum
> `as` nach der ersten Runde noch Vorgabe blieb, war das fehlende
> `.debug_line`. Der ist weg: TEIL 7 baut die DWARF-Zeilentabelle selbst,
> TEIL 8 nimmt sie ab (1 705 + 1 680 Einheiten, **oktettgleich**, dazu
> 34 Mio. ausgewertete Tabellenzeilen und eine `gdb`-Sitzung, die
> zeichengleich ist), TEIL 9 hält die vier SIMD-Befehle der Runde RASTERN
> fest, TEIL 10 misst neu. **Seit dieser Runde ist der eigene Kodierer die
> Vorgabe**; `--asm-extern` ist die Rückfallebene. `firnc` ruft kein
> fremdes Programm mehr, außer `ld`.

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

> **Überholt durch RUNDE KODIERER II.** Der Absatz bleibt stehen, weil er
> die Ausgangslage beschreibt; was er sagt, gilt seit TEIL 7 nicht mehr.

**Die Fehlersuchinformation.** `as` baut aus den `.loc`/`.file`-Direktiven
ein `.debug_line`-Programm; der interne Weg verwarf sie. Eine über den
internen Weg gebaute Datei hatte also kein DWARF — das entsprach einem Bau
ohne `-g`.

Das war der Hauptgrund, warum der alte Weg zunächst Vorgabe blieb.

Der DWARF-Zeilenzähler nachzubauen ist keine große Sache (die
Zeilenprogramm-Kodierung ist gut beschrieben), aber sie *bitgleich* zu `as`
nachzubauen ist eine eigene Runde: `as` wählt die Spezialopcodes optimal,
und jede andere Wahl ergibt andere Oktette bei gleicher Bedeutung. Genau
das ist RUNDE KODIERER II geworden — und sie ist bitgleich geworden.

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

Was noch fehlte, um `as` **ganz** zu streichen: das `.debug_line`. Das ist
seit RUNDE KODIERER II erledigt (TEIL 7); der Vorgabepfad ruft `as` nicht
mehr.

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

# TEIL 7 — RUNDE KODIERER II: DIE ZEILENTABELLE

## 7.1 Warum das der letzte Stein war

Nach TEIL 3 war der Kodierer über 484 Millionen Oktette bitgleich zu `as` —
und `as` lief trotzdem weiter, bei jedem Bau. Der Grund war ein einziger
Abschnitt: `.debug_line`. `codegen_x86.rs` schreibt `.file`- und
`.loc`-Direktiven in den Assemblertext; daraus baut `as` die
Zeilennummerntabelle, ohne die kein `gdb`-Haltepunkt und kein `addr2line`
funktioniert. Der interne Weg las die beiden Direktiven und warf sie weg.

Eine Zeilentabelle **ohne** Fehlersuchinformation wäre kein Fortschritt
gewesen, sondern ein Tausch: schneller übersetzen, dafür nicht mehr
fehlersuchen können. Deshalb blieb `as` Vorgabe. Diese Runde holt das nach.

## 7.2 Was eine Zeilentabelle wirklich ist

Sie ist keine Tabelle. Sie ist ein **Programm**, das eine Tabelle erzeugt.
Ein Zustandsautomat hält Adresse, Datei, Zeile, Spalte und `is_stmt`; die
Befehle schieben die Zustände weiter, und einer davon legt eine Zeile der
Matrix ab. Der Trick, der die Sache klein macht, ist der **Sonderopcode**:
ein einziges Oktett, das Adresse *und* Zeile weiterschiebt *und* die Zeile
ablegt.

```
   opcode = (Zeilenschritt − Zeilenbasis) + Zeilenspanne · Adressschritt
            + Opcodebasis
```

mit Zeilenbasis −5, Zeilenspanne 14, Opcodebasis 13. Damit deckt ein Oktett
Zeilenschritte von −5 bis +8 und Adressschritte von 0 bis 17 ab. Alles
andere braucht Vorspann: `DW_LNS_const_add_pc` (schiebt um genau 17),
`DW_LNS_advance_pc` (LEB128), `DW_LNS_advance_line` (vorzeichenbehaftete
LEB128).

## 7.3 Bitgleich, nicht nur gleichwertig — und warum das die Mühe wert war

Der Auftrag erlaubte ausdrücklich, die Tabelle *anders zu kodieren*, solange
die **ausgewertete** Tabelle gleich ist. Das wäre der bequeme Weg gewesen.
Ich habe den unbequemen genommen und **oktettgleich** gebaut, aus einem
Grund, der nichts mit Eitelkeit zu tun hat:

> **Bitgleichheit ist ein Prüfstand, Gleichwertigkeit ist eine Meinung.**

Wer nur die dekodierten Tabellen vergleicht, vergleicht durch die Brille des
Werkzeugs, das dekodiert. `readelf` normalisiert, fasst zusammen und
verschweigt, was es nicht versteht. Eine falsch gesetzte LEB128, die
zufällig dieselbe Zeile ergibt, fällt dabei nicht auf — bis eines Tages ein
anderer Leser (LLDB, ein Profiler, ein Absturzsammler) sie anders liest.
Oktettvergleich hat diese Lücke nicht.

Der Preis: `as` muss **genau** nachgebaut werden, bis in die
Reihenfolge der Entscheidungen. Diese Runde hat dafür `gas/dwarf2dbg.c`
(binutils 2.40) gelesen statt zu raten — dieselbe Regel wie in TEIL 2, wo
die Intel-Referenz die Grundlage war. Was daraus übernommen wurde und im
Quelltext auch so benannt ist:

* **`emit_inc_line_addr()`** — die Reihenfolge Sonderopcode →
  `const_add_pc` + Sonderopcode → `advance_pc` + Opcode. Warum erst
  `const_add_pc` versuchen? Weil es zwei Oktette braucht statt drei. Warum
  nicht immer? Weil es nur um genau 17 schiebt.
* **„Prettier, I think":** bei Zeilenschritt 0 und Adressschritt 0 schreibt
  `as` `DW_LNS_copy` statt des gleichwertigen Sonderopcodes 13. Ein reiner
  Geschmacksentscheid im fremden Quelltext — und ein Oktett Unterschied.
* **`process_entries()`** — erst `DW_LNS_set_file`, dann
  `DW_LNS_set_column`. Andersherum käme dieselbe Tabelle heraus.
* **`dwarf2_directive_loc()`** — *„If we see two .loc directives in a row,
  force the first one to be output now."* Deshalb liegen drei
  aufeinanderfolgende `.loc` alle auf **derselben** Adresse, nämlich der des
  nächsten Befehls. Das ist die Regel, die man beim Nachbauen mit Sicherheit
  falsch rät, und der Codeerzeuger von Firn erzeugt sie ständig
  (`.loc 1 7 17` / `.loc 1 7 20` / `.loc 1 7 13` vor einem einzigen `mov`).
* **`dwarf2_gen_line_info()`** — eine `.loc` mit **Zeile 0** erzeugt gar
  keine Zeile. `panic_rt.rs` schreibt genau das vor die geteilte
  Absturzbehandlung, damit `gdb` sie keiner Quellzeile zuordnet.
* **`get_basename()`/`get_directory_table_entry()`** — die Aufteilung eines
  Pfades in Verzeichnis und Namen, mit dem Sonderfall `/a.fi`: der letzte
  Schrägstrich ganz vorn heißt *kein* Verzeichnis, sonst wäre es die leere
  Zeichenkette. Und: Steckplatz 0 der Verzeichnistabelle bleibt frei, der
  erste echte Eintrag ist die 1.
* **`scale_addr_delta()`** — auf ARM64 ist die kleinste Befehlslänge 4, und
  Adressschritte zählen in **Befehlen**. Ein Sonderopcode deckt dort also
  68 Oktette ab statt 17.
* **`remap_debug_filename()`** — `--debug-prefix-map <cwd>=.`, das
  `main.rs` seit Runde 93 an `as` übergibt, damit zwei Arbeitskopien an
  verschiedenen Orten gleiche Ergebnisse liefern. Der interne Weg muss
  dieselbe Abbildung selbst machen, sonst stünde plötzlich wieder ein
  absoluter Pfad im Ergebnis.

## 7.4 Die kleine Übersetzungseinheit, die niemand bestellt hat

`.debug_line` allein nützt nichts. `gdb` und `addr2line` gehen **über**
`.debug_info`: sie suchen die Übersetzungseinheit und folgen deren
`DW_AT_stmt_list` zur Zeilentabelle. `as` weiß das und legt, wenn das
Programm keine eigene `.debug_info` mitbringt, still eine winzige an —
`.debug_info`, `.debug_abbrev`, `.debug_aranges`, `.debug_str`, zusammen
knapp 130 Oktette.

Also legen wir sie auch an, mit denselben Oktetten. **Eine** Abweichung ist
Absicht und steht auch so im Quelltext: `DW_AT_producer` sagt `firnc 0.1.0`
und nicht `GNU AS 2.40`. Der Übersetzer soll nicht behaupten, ein anderes
Programm zu sein. Die Zeichenkette steht am **Ende** von `.debug_str`, also
verschiebt sie nichts — `.debug_info`, `.debug_abbrev` und
`.debug_aranges` bleiben oktettgleich, und die Gegenprobe prüft `.debug_str`
bis genau vor diese eine Zeichenkette (`pruefe_debug_str` in
`vergleich.py`).

Baut Firn **mit** `--no-opt`, schreibt der Übersetzer seit Runde 64 seine
eigene `.debug_info` mit Namen, Typen und Variablen. Dann legt `as` keine
an — und wir auch nicht. Auch dieser Zweig steckt in der Abnahme (die
Baustufe `no-opt` in `run.sh`).

## 7.5 Die `.loc`-Zusätze: lieber abbrechen als still lügen

`.loc` kann mehr als Datei, Zeile und Spalte: `is_stmt`, `basic_block`,
`prologue_end`, `epilogue_begin`, `isa`, `discriminator`, `view`. Der
Codeerzeuger von Firn schreibt keinen davon — aber Firn hat
**Inline-Assembler**, und wer in einem `asm`-Block `.loc 1 5 3 is_stmt 0`
hinschreibt, bekäme von einem nachlässigen Zerteiler eine Zeilentabelle, die
`as` anders gebaut hätte. Also: jeder nicht erkannte Zusatz ist ein
**Fehler mit Namen**, kein stilles Überlesen.

```
   error: interner Kodierer: Zeile 5: `.loc`-Zusatz `is_stmt` wird nicht
          nachgebildet (nur Datei, Zeile, Spalte)
```

Die Spalte selbst ist umgekehrt **freiwillig**: fehlt sie, behält `as` die
zuletzt genannte (`current` in `dwarf2_directive_loc` ist statisch). Auch
das ist nachgebildet und geprüft.

## 7.6 Was der interne Assembler dafür lernen musste

Bis dahin kannte er fünf Abschnitte. Jetzt sind es zehn: die fünf
`.debug_*` kommen dazu — zwei davon (`.debug_abbrev`, `.debug_info`) als
Oktette aus dem Text, drei erzeugt er selbst. Dazu ein neues Stück
`Piece::Loc`, null Oktette lang: **die Adresse, an der es steht, ist sein
ganzer Inhalt**. Es geht durch dieselbe Fixpunkt-Iteration wie alles andere,
und deshalb sind die Adressen der Zeilentabelle die *endgültigen* — nach der
Sprung-Relaxation, nicht davor. Das war die Stelle, an der ein
selbstgebauter Zeilenzähler sonst schiefgeht.

---

# TEIL 8 — DIE ABNAHME DER ZEILENTABELLE

## 8.1 Drei Prüfungen übereinander

1. **Oktettweise** gegen `as`: `.debug_line`, `.debug_info`,
   `.debug_abbrev`, `.debug_aranges` — Inhalt *und* Umsetzungen.
   `.debug_str` bis vor die Erzeugerangabe.
2. **Die ausgewertete Tabelle:** beide Objektdateien durch
   `readelf --debug-dump=decodedline`, Zeile für Zeile verglichen
   (Datei, Zeilennummer, Adresse, Sicht, `is_stmt`). Das ist die Prüfung,
   die der Auftrag verlangt hat. Sie ist **schwächer** als die erste und
   steht trotzdem daneben: sie prüft, ob ich den Automaten *verstanden*
   habe, nicht nur, ob ich `as` abgeschrieben habe.
3. **Gestreute Quellstellen** (`tools/kodierer/loc_streuer.py`). Der
   Codeerzeuger schreibt `.loc` nur in einem schmalen Muster: aufsteigende
   Zeilen aus einer Datei, kleine Spalten, nie ein Sprung über die Spanne
   des Sonderopcodes hinaus. Also nimmt der Prüfstand echten Assemblertext
   und streut zusätzliche `.loc` hinein — Rücksprünge, Sprünge über 300
   Zeilen, drei Dateien, Zeile 0, mehrere `.loc` auf derselben Adresse.
   Beide Wege bekommen **denselben** Text.

Punkt 3 ist auf ARM64 die **einzige** Deckung, und das ist ein Befund für
sich: `codegen_a64.rs` schreibt bis heute überhaupt keine `.loc`
(Runde 80 hat das ausdrücklich offengelassen). Auf der ARM64-Seite gibt es
also gar keine Fehlersuchinformation zu erzeugen — weder über `as` noch
über den eigenen Weg. Der Kodierer *kann* es, geprüft an gestreuten
Quellstellen mit 4-Oktett-Schrittweite; der Codeerzeuger liefert ihm nur
nichts. Das ist eine offene Baustelle von Runde 80, nicht von dieser.

## 8.2 Das Ergebnis

```
                                        x86-64          ARM64
   Übersetzungseinheiten                 1 705         1 680
   bitgleich gegen `as`                  1 705         1 680     (100 %)
   verglichene Oktette             424 702 304    502 284 944
   verglichene Umsetzungen           3 568 565      4 354 424
   davon Fehlersuch-Oktette        123 562 840     58 638 305
   verglichene Tabellenzeilen       23 225 373     11 104 058
   Abweichungen                              0               0
   übersprungen                              0              25
```

*(Die 25 übersprungenen sind wieder Quellen, die für `aarch64-linux` gar
nicht bauen — dieselben wie in TEIL 4, jetzt über fünf Baustufen statt drei
gezählt.)*

Zusammen mit TEIL 3/4 sind damit **über 1,6 Milliarden Oktette** gegen `as`
geprüft, davon 182 Millionen Fehlersuchinformation, und 34 Millionen
ausgewertete Tabellenzeilen.

Fünf Baustufen je Quelle statt vorher drei: `dev-fast`, `release-fast`,
`release-safe`, `no-opt` (eigene `.debug_info`) und `streu` (gestreute
Quellstellen).

## 8.3 Die praktische Gegenprobe: `gdb` und `addr2line`

Ein Programm mit drei ineinander verschachtelten Funktionen, einmal über
`as` und einmal über den eigenen Kodierer gebaut, dann derselbe
`gdb`-Ablauf: Haltepunkt auf `g.fi:5`, `run`, `bt`, `info line`,
`continue`, `bt`.

**Die Ausgabe ist zeichengleich.** Haltepunktadresse, Quellzeilentext,
Rücksprungspur mit allen Rahmen, `Line 5 of "g.fi" starts at address … and
ends at …` — identisch, sowohl mit `--no-opt` als auch optimiert.
`addr2line` liefert für `tief`, `mitte` und `main` dieselben Datei- und
Zeilenangaben. Und `readelf --debug-dump=decodedline` ist auf beiden
Ergebnissen Zeile für Zeile dasselbe.

Das ist die Prüfung, die zählt: nicht „die Oktette sehen richtig aus",
sondern „der Fehlersucher tut dasselbe".

## 8.4 Die drei Proben, die die Umstellung tragen

Die Fahne umzudrehen ist der Schritt, bei dem ein Fehler nicht mehr nur
theoretisch wäre. Drei Messungen stehen dahinter, alle mit dem Übersetzer
dieser Runde:

```
   Testprogramme, gleiches Verhalten (Vorgabe gegen --asm-extern)   314 / 314
     -- gebaut und AUSGEFUEHRT, gleicher Rueckgabewert, gleiche Ausgabe
   Rueckfallebene oktettgleich zum unberuehrten Uebersetzer         313 / 313
     -- `--asm-extern` tut, was firnc vor beiden Runden tat
   Modultests des Uebersetzers (cargo test --release)               283 / 283
     -- darunter neun neue in `dwarf_line.rs`
```

Dazu die **Wiederholbarkeit** (Runde 93, `ACCEPTANCE.md` Punkt 5): dasselbe
Programm aus zwei verschiedenen Verzeichnissen gebaut ist bitgleich — über
den eigenen Weg genauso wie über `as`. Das war die Stelle, an der eine
selbstgeschriebene Zeilentabelle am ehesten etwas kaputtmacht, denn sie
schreibt Pfade; `remap_debug_filename` wird deshalb nachgebildet (§7.3).

## 8.5 Die volle Prüfsuite

`bash test.sh`, mit dem eigenen Kodierer als **Vorgabe** — also der Fall,
der zählt. Stand bei Abgabe: **Abschnitte 1 bis 15 grün, null Fehlschläge.**

```
    1  Übersetzer bauen                                       ok
    2  Modultests des Übersetzers                     283 / 283
    3  positive Tests, JEDE Baustufe      334 Programme x 4 Läufe
    4  negative Tests (Fehlermeldungen)                       ok
    5  Nachweis des Optimierers                         45 / 45
    6  Ergebnisort-Garantie (SPEC 13.1)                       ok
    7  Feldzugriff / Speicherort getrennt                     ok
    8  Symbolschema                                           ok
   8b  `lock xadd`, FIR oktettgleich, beide Übersetzer        ok
  8b2  Bau-Umgebungsvariablen, beide Übersetzer               ok
   8c  statische Bindung ohne indirekten Aufruf               ok
    9  HTML5-Zerteiler gegen html5lib              6810 / 6810
   9b  HTML-Baumbau + DOM                            150 / 150
   9c  CSS: Syntax, Selektoren, Kaskade    305/305 · 109/109 ·
                                            840/840 gegen cssselect2
   10  DOM-Dauerlauf ohne Leck                 flach 1764 KiB
   11  Lexer in Firn gegen Rust      752 gleich, 1 343 686 Marken
   28  Zahlenleser, vier Leser                 0 Abweichungen
   31  `str`-Dauerlauf, 200 000 Runden                        ok
   12  Zerteiler in Firn gegen Rust       435 gleich, 1 bekannt
   13  Auslegung/ABI in Firn gegen Rust       370 gleich, 0 ab
   14  Typprüfer in Firn gegen Rust     187 gleich, 37 201 Ausdrücke
   15  Absenkung in Firn gegen Rust       183 gleich, 1 bekannt
   16  Selbstübersetzung (self_compare.sh)                LÄUFT
```

Die beiden mit „1 bekannt" sind die Grundlinie, die schon vor beiden Runden
rot war und im Werkzeug namentlich steht — sie gehören nicht dieser Runde.

**Abschnitt 16 und die folgenden liefen bei Abgabe noch.** Sie sind auf
diesem Wirt sehr langsam: `self_compare.sh` lässt den *in Firn geschriebenen*
Übersetzer jede Testdatei übersetzen und ausführen, und daneben liefen drei
fremde Prüfsuiten der Runde SAMMELN. Das Protokoll läuft weiter unter
`/root/KOD2-test.log`; bis dahin ist kein einziger Fehlschlag aufgetreten.

Ich sage ausdrücklich nicht „die Suite ist grün". Was ich sagen kann:
**bis Abschnitt 15 einschließlich ist sie grün**, und für den Vorgabepfad
liegt mit §8.4 die stärkere Aussage vor (314/314 gleiches Verhalten,
313/313 Oktettgleichheit der Rückfallebene).

## 8.6 Was das für die Fahne heißt

`--asm-intern` ist **Vorgabe**. `--asm-extern` ruft `as` und bleibt als
Rückfallebene erhalten — und als Vergleichsmaß, denn ohne `as` gäbe es
keinen Prüfstand mehr. `tools/kodierer/vorgabe_unveraendert.sh` prüft
weiterhin, dass diese Rückfallebene oktettgleich zum unberührten Übersetzer
ist.

---

# TEIL 9 — DIE VIER SIMD-BEFEHLE AUS DER RUNDE RASTERN

Die Runde RASTERN hat gemessen und gemeldet: Firn hat 29 Vektorbefehle,
aber weder eine Ganzzahl-Multiplikation noch ein Packen/Auspacken 8↔16.
Damit ist Alphaüberblenden im Vektor unmöglich — und genau das kostet auf
`xoffi.ai` rund 180 ms in der Farbberechnung der Verläufe.

Warum diese vier zusammengehören: Überblenden rechnet je Farbanteil
`(vorn·α + hinten·(255−α))/255`. Acht Oktette gleichzeitig geht nur über
16 Bit — also **auspacken** (8→16), **multiplizieren** (16-Bit-Produkt,
unten und oben), **zurückpacken** (16→8, gesättigt).

| Firn | x86-64 | ARM64 |
|---|---|---|
| `__v128_unpacklo8(a,b)` | `punpcklbw` | `zip1 .16b` |
| `__v128_mullo16(a,b)` | `pmullw` | `mul .8h` |
| `__v128_mulhi16u(a,b)` | `pmulhuw` | `umull` + `umull2` + `uzp2 .8h` |
| `__v128_packus16(a,b)` | `packuswb` | `sqxtun` + `sqxtun2` |

Zwei davon haben auf ARM64 **kein** Gegenstück in einem Befehl:

* **`pmulhuw`** will die oberen 16 Bit von acht 16×16-Produkten. ARM rechnet
  die Produkte breit (`umull` für die unteren vier Halbwörter, `umull2` für
  die oberen) und greift sich die oberen Hälften mit `uzp2 .8h` heraus —
  bei kleinem Ende sind das genau die ungeraden Halbwörter.
* **`packuswb`** verengt 8+8 vorzeichenbehaftete Halbwörter auf 16
  vorzeichenlos gesättigte Oktette. `sqxtun` macht die untere Hälfte,
  `sqxtun2` schreibt die obere in **dasselbe** Zielregister — die
  Reihenfolge ist damit erzwungen.

**Wie sie geprüft sind.** Genau wie die 42 Befehle der Runde 82:

1. Der Kodierer beider Maschinen bekam sie dazu (x86: `pmullw` D5,
   `pmulhuw` E4, `packuswb` 67, dazu `pmulhw`, `packsswb`, `punpckhbw`,
   `punpcklwd`, `punpckhwd`, weil sie im selben Opcodeblock liegen und
   nichts kosten; ARM64: `mul` auf Vektoren, `umull`/`umull2`/`smull`/
   `smull2`, `sqxtun`/`sqxtn`/`uqxtn`/`xtn` samt ihren `2`-Formen).
   Jede Form ist oktettweise gegen `as` geprüft.
2. `tests/1614_simd_ops.fi` — Abschnitt I — rechnet jeden der vier gegen
   eine **skalare Fassung in Firn**, in derselben Datei. Dazu ein
   ausdrücklicher Sättigungsfall (negatives Halbwort → 0, zu großes → 255,
   die Vorgabewerte treffen das nicht sicher) und ein ganzes
   Alphaüberblenden über acht Oktette.
3. Dasselbe Programm läuft auf **beiden** Maschinen — x86-64 nativ,
   ARM64 unter `qemu-aarch64` — und gibt dasselbe aus. Ein Gegenstück, das
   nur auf ARM64 falsch ist, fiele hier auf.

**So ruft man sie:**

```firn
let zero  = __v128_zero()
let vorn  = __v128_unpacklo8(pixel, zero)      // 8 Oktette -> 8 Halbwörter
let mix   = __v128_mullo16(vorn, alpha)        // untere 16 Bit
let hoch  = __v128_mulhi16u(vorn, alpha)       // obere 16 Bit
let zrk   = __v128_packus16(mix_lo, mix_hi)    // 16 Halbwörter -> 16 Oktette
```

Das Blending in Certus baut diese Runde **nicht** — das gehört zu RASTERN.
Hier steht nur: die Befehle sind da, auf beiden Maschinen, und sie rechnen
das Richtige.

*Nachbarschaft, falls RASTERN sie braucht:* `punpckhbw` (die obere Hälfte
auspacken) ist im Kodierer schon drin und bräuchte nur noch einen Namen in
`simd.rs` — vier Zeilen, ARM64-Gegenstück `zip2 .16b`. Sie wurde nicht
hinzugefügt, weil der Auftrag ausdrücklich vier Befehle nannte.


---

# TEIL 10 — DIE MESSUNG NACH DER UMSTELLUNG

## 10.1 Was das Übersetzen jetzt kostet

`lib/js/parse_main.fi`, `firnc --timings`, Bestwert aus neun Läufen. **Der
Wirt war dabei nicht ruhig** — drei fremde Prüfsuiten und eine zweite
Abnahme liefen daneben (Lastmittel um 12 auf 20 Kernen). Die absoluten
Zahlen sind darum nicht mit denen aus TEIL 5 vergleichbar; das **Verhältnis**
schon, denn beide Wege wurden unter denselben Bedingungen gemessen.

```
                      über `as`      eigener Weg
   Optimierer          162,6 ms        167,1 ms
   as + ld             238,9 ms        146,8 ms   <<<
   Codeerzeuger        123,4 ms        127,5 ms
   sema                 52,5 ms         55,7 ms
   lex+parse            45,1 ms         51,6 ms
   lower                27,1 ms         27,9 ms
   mono                  1,7 ms          1,7 ms
   .s schreiben          0,9 ms          1,1 ms
   ------------------------------------------
   GESAMT              657,4 ms        587,5 ms   (−10,6 %)
```

Über drei große Quellen (`parse_main.fi`, `firnc1.fi`, `b4_main.fi`,
`tools/kodierer/messung.sh`, Bestwert aus fünf Läufen):
**9 240 ms → 8 152 ms, −11,8 %.**

## 10.2 Was die Zeilentabelle kostet — und was sie bei `as` kostete

Dieselbe `.s`-Datei, einmal wie sie ist und einmal mit entfernten
`.file`/`.loc`-Zeilen (118 053 gegen 93 785 Zeilen), beide durch beide
Assembler:

```
                                    mit Tabelle   ohne Tabelle   Differenz
   eigener Kodierer                   173,2 ms      146,6 ms      26,7 ms
   as (GNU binutils 2.40)             229,7 ms      163,9 ms      65,8 ms
   -----------------------------------------------------------------------
   Faktor                               1,33 x        1,12 x
   ld                                    6,1 ms
```

Zwei Dinge stehen da:

1. **Die Zeilentabelle kostet uns 26,7 ms, `as` kostet sie 65,8 ms.** Wir
   erzeugen sie also **2,5-mal so schnell** — obwohl sie oktettgleich ist.
   Der Grund ist kein Kunstgriff, sondern eine fehlende Umständlichkeit:
   `as` legt für jede `.loc` ein Symbol und einen Fragment-Eintrag an und
   relaxiert die Zeilenschritte anschließend in einer eigenen Runde. Wir
   haben die endgültigen Adressen ohnehin schon, weil die Quellstellen als
   nulllange Stücke durch dieselbe Fixpunkt-Iteration laufen wie die
   Sprünge (§7.5).
2. **Der ehrliche Rückschritt gegenüber TEIL 5.** Dort stand „1,52 ×" für
   den Assemblerschritt — gemessen an einem Kodierer, der die
   Fehlersuchinformation gar nicht erzeugte. Jetzt sind es 1,33 ×, weil er
   sie erzeugt. Der Vergleich, der zählt, ist der mit gleichem Ergebnis,
   und der lautet 1,33 ×.

## 10.3 Und was ist mit dem selbstgehosteten Übersetzer?

Hier gehört eine Einschränkung hin, die man leicht übersieht und die für
OrientOS die wichtigere Hälfte ist:

> **`firnc1` ruft weiterhin `/usr/bin/as` und `/usr/bin/ld`.**

Der Kodierer steckt in `firnc0`, dem Übersetzer in Rust. `bin/firnc1.fi` —
der Übersetzer **in Firn**, der sich selbst übersetzt — schreibt nach wie vor
`target.s` und startet die beiden Programme über `fork`/`execve`
(`bin/firnc1.fi`, Zeile 1384). Für den Bau *mit* `firnc0` ist `as` also weg;
für die selbstgehostete Kette ist es das nicht.

Was dafür nötig wäre: `x86enc.rs`, `asm_x86.rs`, `dwarf_line.rs` und
`elfobj.rs` nach Firn übersetzen und in `lib/firnc1/` legen — rund 4 900
Zeilen, deren Verhalten durch diese beiden Runden bereits **oktettgenau
festgenagelt** ist. Das ist die günstigste Portierung, die es in diesem Baum
gibt: die Vorlage ist bewiesen, und der Prüfstand (`as` als Maß) gilt für die
Firn-Fassung unverändert weiter. Erst danach ist die Kette *von der Quelle
bis zur Objektdatei* wirklich geschlossen.

## 10.4 Braucht es jetzt einen eigenen Binder?

Nein, und die Zahl ist noch deutlicher als in TEIL 5c: **`ld` kostet 6,1 ms**
von 587. Ein eigener Binder wäre für die Geschwindigkeit sinnlos. Er wäre
nur dann interessant, wenn OrientOS auch die *letzte* fremde Binärabhängigkeit
loswerden soll — und dann ist es der einfachste Binderfall, den es gibt: eine
Objektdatei je Programm, keine Bibliotheken, keine dynamische Bindung.

## 10.5 Der nächste Schritt — und wie der Prüfstand ihn überlebt

Der Auftrag fragt nach dem, was in TEIL 5a als Schätzung stand: `codegen_x86.rs`
soll `x86enc::Inst` **direkt** erzeugen statt Assemblertext. Diese Runde hat
damit **nicht** angefangen, und das ist eine bewusste Entscheidung — nicht
aus Zeitmangel, sondern weil die Antwort auf die Prüfstandsfrage zuerst
stehen muss. Sie lautet:

> **Der Text bleibt — er wird nur nicht mehr gebraucht.**

Der Prüfstand dieser beiden Runden lebt davon, dass es einen Assemblertext
gibt, den man `as` vorlegen kann. Fällt der Text weg, fällt der Prüfstand
weg. Die Lösung ist eine Kette statt eines Vergleichs:

```
   (a)  Text  --as-->        Oktette A     (heute geprüft: A == B)
   (b)  Text  --Zerteiler--> Oktette B
   (c)  FIR   --codegen-->   Inst   --Kodierer--> Oktette C

   zu zeigen:  C == B     dann folgt  C == A
```

Konkret:

1. `codegen_x86.rs` erzeugt **beides** — den Text wie bisher (für
   `--emit=asm`, für `--asm-extern`, für Menschen) *und* den `Inst`-Strom.
   Das kostet Zeit, aber nur solange die Umstellung läuft.
2. Ein neuer Prüflauf (`tools/kodierer/direkt.sh`) baut jede Einheit beide
   Male und vergleicht C gegen B — Oktett für Oktett, mit demselben
   Werkzeug, das heute A gegen B vergleicht.
3. Ist das über den ganzen Baum grün, wird die Texterzeugung im
   Codeerzeuger **abschaltbar** (nicht gelöscht): Vorgabe ist der direkte
   Weg, `--emit=asm` und `--asm-extern` schalten sie wieder ein.

Damit bleibt bis zum Schluss nachprüfbar, was der Kodierer tut, und `as`
bleibt bis zum Schluss das Maß. Der geschätzte Gewinn steht in §5a: noch
einmal grob 100–150 ms von 587, also rund 20 %. Es bleibt eine Schätzung.

**Reihenfolge, wenn OrientOS ohne fremde Binutils gebaut werden soll:**

1. ~~`.debug_line` selbst schreiben~~ — erledigt (TEIL 7/8).
2. Den Kodierer nach Firn portieren, damit auch `firnc1` ohne `as` auskommt
   (§10.3). Das ist der Schritt mit dem größten Gewinn für OrientOS und dem
   kleinsten Risiko, weil die Vorlage oktettgenau geprüft ist.
3. Codeerzeuger auf `Inst` umstellen (§10.5) — reine Geschwindigkeit.
4. Eigener Binder zuletzt, und nur wenn die *letzte* fremde Binärdatei
   verschwinden soll: 6,1 ms von 587 sind kein Argument.


---

# ANHANG

## A.1 Was wo liegt

```
   compiler/src/x86enc.rs     1747 Zeilen   der x86-64-Kodierer (rein)
   compiler/src/asm_x86.rs    2004 Zeilen   Zerteiler, Marken, Relaxation
   compiler/src/a64enc.rs      334 Zeilen   ARM64-Sofortwerte, Ausbesserungen
   compiler/src/asm_a64.rs    2304 Zeilen   ARM64-Zerteiler und Befehlssatz
   compiler/src/dwarf_line.rs  737 Zeilen   die DWARF-Zeilentabelle (II)
   compiler/src/elfobj.rs      449 Zeilen   ELF64-Objektschreiber
   compiler/src/asm_intern.rs   32 Zeilen   die Fahne
   tools/kodierer/             911 Zeilen   Abnahme, Streuer, Probe, Messung
   -----------------------------------------------------------------
                              8518 Zeilen   gesamt (beide Runden)
```

`x86enc.rs`, `a64enc.rs` und `dwarf_line.rs` sind **rein**: keine
Ein-/Ausgabe, kein Zustand, keine Abhängigkeit vom Rest des Übersetzers.
Wer eines Tages einen JIT baut, kann sie unverändert benutzen — auch die
Zeilentabelle, die ein Laufzeitprofiler genauso braucht wie ein
Fehlersucher.

## A.2 Die Fahne

```
   firnc datei.fi                 der eigene Kodierer (VORGABE seit
                                  RUNDE KODIERER II)
   firnc --asm-extern datei.fi    der alte Weg über `as` (Rückfallebene)
   firnc --nur-obj [-o x.o] x.s   nur assemblieren (für die Gegenprobe)
```

## A.3 Die Abnahme wiederholen

```bash
   bash tools/kodierer/run.sh              # x86-64, ganzer Baum
   bash tools/kodierer/run.sh --a64        # ARM64
   bash tools/kodierer/ende_zu_ende.sh     # bauen UND laufen lassen
   bash tools/kodierer/vorgabe_unveraendert.sh   # Rueckfallebene unberuehrt?
   bash tools/kodierer/probe.sh tests/1613_crypto.fi   # kleine Stichprobe
   RUNS=5 bash tools/kodierer/messung.sh bin/firnc1.fi
```

## A.4 Was diese Runde NICHT belegt

* **Nichts über JIT.** Kein Laufzeitcode, keine ausführbaren Seiten, keine
  Deoptimierung. Die Studie bleibt in ihrer Empfehlung unverändert gültig.
* ~~**Nichts über DWARF.**~~ *Überholt: RUNDE KODIERER II vergleicht
  zusätzlich `.debug_line`, `.debug_info`, `.debug_abbrev` und
  `.debug_aranges` oktettweise sowie die ausgewertete Zeilentabelle
  Zeile für Zeile (TEIL 8).*
* **Die Reihenfolge der Umsetzungen** wurde als *Menge* verglichen, nicht
  als Folge. `as` schreibt sie in seiner internen Reihenfolge; für den
  Binder ist das ohne Bedeutung.
* **Kein Vergleich der ganzen Objektdatei Oktett für Oktett.** Die
  Abschnittsreihenfolge und die Symboltabellen-Reihenfolge unterscheiden
  sich; verglichen wurde der *Inhalt*, der beim Binden zählt.
* **ARM64 ist nur gegen `as` geprüft, nicht auf echter Hardware gelaufen.**
  Für x86-64 liegt die Ausführungsprobe vor (314/314), für ARM64 nur unter
  `qemu-aarch64` — und zwar für `tests/1614_simd_ops.fi`, also für die vier
  neuen Vektorbefehle aus TEIL 9. Eine echte ARM64-Maschine hat diese Runde
  nicht gesehen.
* **Auf ARM64 gibt es gar keine Zeilentabelle zu erzeugen.**
  `codegen_a64.rs` schreibt keine `.loc` (offen seit Runde 80). Der
  ARM64-Zweig von `dwarf_line.rs` ist deshalb nur an *gestreuten*
  Quellstellen geprüft (§8.1, Punkt 3) — dort allerdings über 58 Millionen
  Oktette und 11 Millionen Tabellenzeilen.
* **Der Codeerzeuger schreibt weiterhin Assemblertext.** Der Schritt
  „direkt `Inst`" ist geplant, aber nicht angefangen (§10.4).
* **Die volle Prüfsuite (`bash test.sh`, 1562 Punkte) wurde in dieser Runde
  nicht zu Ende gefahren** — auf dem Wirt waren nur noch rund 300 MB Platte
  frei (52 von 54 GB durch andere Projekte belegt), und die Suite baut den
  Übersetzer mehrfach mit sich selbst. Was stattdessen vorliegt, ist die
  **stärkere** Aussage für den Vorgabepfad (§3.4): 314 von 314 Programmen
  sind oktettgleich zum unberührten Übersetzer. Die Modultests des
  Übersetzers (`cargo test --release`) laufen durch, darunter die sieben
  neuen Fälle in `x86enc.rs`, die genau die vier Fallen aus §2.3 festnageln.
