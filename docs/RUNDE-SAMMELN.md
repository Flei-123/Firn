# Runde SAMMELN (Firn-Hälfte) — anlegeweg, billig, kodierer, dns-pic in EINEN Baum

Zweig `sammeln`, Arbeitsbaum `/root/firn-sammeln`. 05.09.2026.
Die Certus-Hälfte derselben Runde steht in `certus/docs/RUNDE-SAMMELN.md`;
dieser Bericht ist die Firn-Seite und für sich lesbar.

```
  3ce89a951  Runde ANLEGEWEG (heute erst eingecheckt, siehe Abschnitt 1)
  e68c09b06  SAMMELN, Merge 2: billig       -- drei Konflikte
  cd5ce8742  SAMMELN, Merge 3: kodierer     -- KEIN Konflikt
  9ffe9dcd5  SAMMELN: und der Uebersetzer trotzdem unbaubar
  3571fe1f9  SAMMELN, Merge 4: dns-pic      -- ein Konflikt
```

## 1. Der Zweig, der gar nicht im Verzeichnis lag

`anlegeweg` zeigte auf `4af14c3ed` — denselben Commit wie `speed`. Der
Zweig hatte **null eigene Commits**; die ganze Runde stand unversioniert im
Arbeitsbaum: 2749 geänderte Zeilen in `compiler/src/gc.rs`,
`compiler/src/gc_lower.rs`, `lib/gc/gc.fi`, `lib/firnc1/{gc,gctext,lower}.fi`,
zwei Testfällen und den neu gemessenen `tools/gc_soak/measurement-*.tsv`,
dazu unversioniert `docs/RUNDE-ANLEGEWEG.md` und `tools/anlegeweg/`.

Das ist der schnelle Anlegeweg (2,80 x), `__gc_mark` auf einer
Nichtzeiger-Adresse von 279 auf 5 ns und die längste Sammelpause von
27,5 auf 10,0 ms. Ein `rm -rf` und die Runde wäre weg gewesen.

Nachgemessen im **Endstand** (`tools/anlegeweg/mikro.sh`, Maschine unter
Fremdlast, deshalb ein paar Prozent über den Zahlen der Runde):

```
                              Runde ANLEGEWEG      Sammelbaum
    ALLOCATE 24 Oktette        54,9 ns              57,8 ns
    ALLOCATE 56 Oktette        74,5 ns              82,1 ns
    vier Objekte (eine Umgebung) 210,9 ns          220,7 ns
    ALLOCATE mit 8192 lebenden  84,7 ns             87,8 ns
```

## 2. Die Reihenfolge

Zwei Linien, die seit `52a13e750` auseinanderlaufen:

```
  52a13e750
    ├── 508f6c796 -> 4af14c3ed ── anlegeweg ── kodierer      2 + 2 Commits
    └── ...38 Commits... 2a20c514b (main) -> a58d17dd5 ── billig
                                              └── dns-pic
```

`anlegeweg` zuerst (er ist die Basis von `kodierer`), dann `billig` (der
große Sprung über die Linien), dann `kodierer` — **der sitzt am weitesten
unten und ist per Fahne abschaltbar, also kommt er spät**; wäre er zuerst
gekommen, hätte jeder folgende Konflikt zwei mögliche Ursachen gehabt.
`dns-pic` kam als vierter dazu, und zwar nicht aus dem Auftrag, sondern
gemessen (Abschnitt 5).

## 3. Merge 2 — `billig`: drei Konflikte

**(a) `.gitignore` — VEREINIGUNG.** HEAD bringt den Block der Runde FIRN-ENV
(`.env-work/`, `.env/`), billig die Blöcke B5, B6 und WINDOWS. Alle drei
Arbeitsverzeichnisse gibt es im Sammelbaum, also gelten alle Regeln.

**(b) `lib/firnc1/parser.fi`, `par_new` — VEREINIGUNG der Feldliste.**

```
billig    (Zeile 276)   arch_pending
FIRN-ENV  (295-298)     estart, eallow_p, eallow_n, enotes
```

Nach dem Merge stehen beide Feldersätze in `struct Parser`. Eine Seite allein
zu nehmen hätte Felder uninitialisiert gelassen.

**(c) `lib/firnc1/gctext.fi` — NEU ERZEUGT, nicht von Hand gemergt.** Die
Datei ist Erzeugnis von `tools/gen_gctext.sh` aus `lib/gc/gc.fi` und sagt das
im Kopf. Beide Seiten haben `gc.fi` geändert, `gc.fi` selbst lief konfliktfrei
zusammen — also ist die richtige Fassung die aus dem **gemergten** gc.fi:

```
              anlegeweg    billig     neu erzeugt
GCTEXT_N        139811     131041        139843
GCTEXT_ALL      162292     153549        162388
```

Keine der beiden Seiten wäre richtig gewesen.

## 4. Merge 3 — `kodierer`: kein Konflikt, und der Übersetzer trotzdem unbaubar

`kodierer` und `billig` berühren einander in **keiner Zeile**; git legte sie
ohne einen Konflikt zusammen. `cargo build` danach:

```
error[E0004]: non-exhaustive patterns: `Target::X86_64None`,
`Target::Aarch64None` and `Target::X86_64Windows` not covered
    --> compiler/src/main.rs:1198
```

KODIERER stand auf der SPEED-Linie, wo `enum Target` **zwei** Werte hat; ihr
`match t` in `assemble()` ist dort vollständig. Die Linie von BILLIG bringt
über main/arm-freestanding/windows **drei weitere** Ziele mit.

**Keiner der beiden Zweige ist falsch. Der Fehler entsteht erst durch das
Zusammenführen — und er wäre in keinem der beiden Bäume je aufgefallen.**

Entschieden: der Kodierer fragt nach dem **Befehlssatz** (`t.arch()`), nicht
nach dem Ziel.

```
x86_64-linux, x86_64-none    -> asm_x86    (ELF, dieselben Oktette)
aarch64-linux, aarch64-none  -> asm_a64
x86_64-windows               -> Fehlermeldung, Vorgabepfad ueber `as`
```

Freistehend ist eine ELF-Datei wie Linux auch (`target.rs`: *„freestanding,
an ELF object file"*), Windows nicht: `elfobj.rs` schreibt ELF, PE/COFF kann
der eigene Kodierer nicht. Das von `rustc` vorgeschlagene `todo!()` wäre ein
Absturz zur Laufzeit gewesen.

**Die inhaltliche Frage hinter dem textuell konfliktfreien Merge** lautet:
kodiert der eigene Kodierer auch das, was BILLIGs neuer ARM64-Zuteiler
erzeugt? Gemessen im Endstand, alle drei Baustufen, Objektdatei gegen `as`:

```
                        ARM64                 x86-64
uebersetzte Einheiten    1020                  1023
bitgleich                1020                  1023
abweichend                  0                     0
uebersprungen               3                     0
verglichene Oktette   231 594 172           207 536 809
verglichene Umsetzungen 2 734 828             2 203 311
```

Die Runde KODIERER hatte 1008/1008 (ARM64) und 1023/1023 (x86-64). Im
Sammelbaum sind es auf ARM64 mehr Einheiten, weil mehr Quellen da sind — und
alle bitgleich. **Zuteiler und Kodierer vertragen sich Oktett für Oktett.**

## 5. Merge 4 — `dns-pic`, nicht im Auftrag, sondern gemessen

Die Certus-Runde ABSCHLUSS behauptet, der ganze Browser sei für den
Befehlssatz des Telefons übersetzbar, und belegt es mit

```
firnc --target=aarch64-linux --pic -c lib/android/a_main.fi
```

Mit dem Firn, den Certus in `vendor/firn/COMMIT` festnagelt (`a58d17dd5`):

```
error: unknown option '--pic'
```

`--pic` (`0fb4d51ea`) und `mkdir -> mkdirat` für aarch64 (`ac6a1fc36`, im
Betreff ausdrücklich *„Certus, Runde ABSCHLUSS"*) liegen auf `dns-pic` und in
keinem der Zweige dieser Runde. Ohne sie ist ABSCHLUSS nicht nachvollziehbar.

Ein Konflikt: `compiler/src/main.rs`, der **Hilfetext**; beide Seiten hängen
eine Zeile an dieselbe Stelle. **Vereinigung.**

Nachher, aus Certus' eigenem festgenageltem Firn:
`a_main.fi -> 5 556 768 Oktett Objektdatei, rc=0`.

## 6. Die Testreihe

`bash test.sh`, gemessen auf dem Ausgangsstand und nach den Merges, auf
derselben Maschine, mit denselben nebenherlaufenden Läufen.

### Die Zahlen

```
                                    Punkte   rot
Grundlinie B0  (4af14c3ed, SPEED-Linie)   1586     3
Endstand   B4  (3571fe1f9, beide Linien)     -     9
zum Vergleich: Zweig billig allein (main) 1562    25
```

**Die Grundlinie hat DREI rote Punkte, nicht dreiundzwanzig.** Die 23 aus
dem Bericht MERGE-WIN gelten fuer die main-Linie (`2a20c514b`); der
Ausgangsstand dieser Runde liegt auf der SPEED-Linie und ist deutlich
gruener. Wer die 23 als Messlatte nimmt, misst gegen den falschen Baum.

```
rot in B0 (Grundlinie)              rot in B4 (Endstand)
  english/check.sh                    english/check.sh          geerbt B0+billig
  fmt/run.sh                          fmt/run.sh                geerbt B0+billig
  k3net/run.sh                        k3net/run.sh              geerbt B0+billig
                                      packages/run.sh           geerbt von billig
                                      freestanding/none.sh      geerbt von billig
                                      aarch64/syscall_table.sh  geerbt von billig
                                      checkidx/run.sh           geerbt von billig
                                      aarch64/run.sh            NEU
                                      aarch64/run.sh --no-opt   NEU
```

**Sieben der neun sind geerbt** -- jeder einzelne ist vor dem Merge auf
einer der beiden Linien nachweislich rot gewesen. **Zwei sind neu, und
beide sind derselbe Fehler** (Abschnitt 6b).

Und die Gegenrichtung, die genauso zaehlt: **der Merge macht 16 rote
Punkte gruen**, die der Zweig `billig` allein hatte -- strsoak,
sema_compare, fir_compare, self_compare, fixpoint, thread,
freestanding/run, core, round74, extfn, escape, firstrun, state,
optlevels, repro/two_machines, testrunner (dazu die zwei
Windows-Punkte). Die SPEED-Linie hat sie repariert; auf `billig` waren
sie nur deshalb noch rot, weil der Zweig nicht nachgezogen war. **Das ist
das Argument fuer zuegiges Zusammenfuehren in einer Zahl.**

Der Lauf B4 ist in Abschnitt 63 abgebrochen -- nicht am Code, sondern am
**vollen Datentraeger** (`error: cannot write '.b4-work/b4_main_opt.s':
No space left on device`). Abschnitt 63 wurde nach dem Aufraeumen einzeln
nachgefahren: `B4 OK: 390 / 1993 WPT-Untertests, 16 Dateien ganz, 14
eigene Faelle, 28 HTTP-Regeln, 103x weniger Stilarbeit, 0 Kaesten
falsch`. Der Fixpunkt haelt im Endstand: *stage 2 == stage 3,
character-identical (811140 lines of assembly)*, und `.firnc2 behaves
like firnc0`.

Der Zwischenlauf B3 (ohne dns-pic) wurde nach 50 Minuten **absichtlich
abgebrochen**, um Grundlinie und Endstand die Maschine zu lassen. Die
Zuordnung der neuen roten Punkte braucht ihn nicht -- sie ist mit dem
gezielten Einzelwerkzeug gemacht (Abschnitt 6b) und ist damit schaerfer,
als ein ganzer Suite-Lauf sie haette machen koennen.

## 6b. Der zweite Fund: gruener Merge, roter Sammler auf ARM64

`tools/aarch64/run.sh` faellt im Endstand ueber genau einen Fall:

```
DIFF tests/843_collections_interplay.fi :: exit code x86=0 aarch64=3
```

Der Fall laeuft auf x86-64 durch und geht auf ARM64 mit 3 heraus. Exit 3
ist in dem Test die Zeile

```fi
    gcvec_clear(v)
    scrub(3)
    gc_stack_clean()
    gc_collect()
    gc_collect()
    if lives(b1, 2) { return 3 }      // "das darf jetzt NICHT mehr leben"
```

also die Behauptung, dass ein Objekt nach dem Leeren der Sammlung und
zwei Sammellaeufen **nicht mehr erreichbar** ist. Auf ARM64 lebt es noch.

**Die Zuordnung, gemessen statt vermutet** (derselbe Fall, vier Baeume):

```
                     ANLEGEWEG   BILLIGs        tests/843
                     im Baum     a64-Zuteiler   auf aarch64
Grundlinie B0          nein        nein           rc = 0   gruen
B1  anlegeweg          JA          nein           rc = 0   gruen
Zweig billig allein    nein        JA             rc = 0   gruen
B2  anlegeweg+billig   JA          JA             rc = 3   ROT
```

Und es liegt **nicht an der Testdatei**: die ALTE Fassung des Tests
(ohne die zwei `gc_stack_clean()`-Zeilen, die ANLEGEWEG hinzugefuegt hat)
faellt mit dem zusammengefuehrten Uebersetzer genauso (rc=3), und die
NEUE Fassung ist mit dem Uebersetzer ohne `billig` gruen (rc=0).
`FIRN_A64_CELLS=1` aendert nichts.

**Das ist ein echter Laufzeitfehler, den keiner der beiden Zweige allein
zeigt.** Er gehoert in dieselbe Familie wie BILLIGs eigener Nachtrag
(*„DER ZUTEILER MACHT DEN SAMMLER BLIND -- die befoerderte Zelle haelt
4600 von 4680 Knoten fest"*): der neue Zuteiler laesst einen toten
Verweis an einer Stelle stehen, die der konservative Sammler abtastet.
BILLIGs Gegenmittel war, die Zellen auf ARM64 auszuschalten -- **das
reicht nicht**, sobald ANLEGEWEGs Anlegeweg dazukommt.

Als Vermutung ausgewiesen und NICHT gemessen: wahrscheinlich haelt ein
aufgerufener-gerettetes Register oder ein Rahmenplatz den Verweis, den
`scrub`/`gc_stack_clean` auf ARM64 nicht erreichen.

Nachstellen:

```bash
cd /root/firn-sammeln
FIRNLIB=$PWD/lib compiler/target/release/firnc --target=aarch64-linux \
    -o /tmp/t843.a64 tests/843_collections_interplay.fi
qemu-aarch64 /tmp/t843.a64 ; echo $?      # 3, auf x86 0
```


## 7. Was hier NICHT erledigt ist

* **`tools/kodierer/run.sh` steht nicht in `test.sh`.** Die 1020 bzw. 1023
  bitgleichen Einheiten muss man von Hand messen. Eine Garantie, die nicht in
  der Testreihe steht, verfällt still.
* **Sieben Zweige sind weiter draußen:** `firnhub` (10 Commits),
  `certus-windows` (3), `android` (2), `bootstrap` (1), `certus-mobil-pic`
  (1), `tempo` (1).
* **BILLIGs ARM64-Zahlen** (b3_main −24,66 % Befehle, Durchsatz 1,384 x) sind
  im Sammelbaum **nicht neu gemessen** worden. Die bitgleiche
  Kodierer-Gegenprobe zeigt, dass der Zuteiler unverändert arbeitet, aber sie
  ist keine Tempomessung.
